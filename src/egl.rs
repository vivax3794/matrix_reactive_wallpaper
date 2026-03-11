use std::ffi::CString;
use std::time::Instant;

use khronos_egl as egl;
use wayland_client::Proxy;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_display::WlDisplay;

const VERT_SRC: &str = r#"#version 300 es
precision highp float;
const vec2 verts[4] = vec2[4](
    vec2(-1.0, -1.0),
    vec2( 1.0, -1.0),
    vec2(-1.0,  1.0),
    vec2( 1.0,  1.0)
);
void main() {
    gl_Position = vec4(verts[gl_VertexID], 0.0, 1.0);
}
"#;

const FRAG_HEADER: &str = r#"#version 300 es
precision highp float;

uniform vec3  iResolution;
uniform float iTime;
uniform float u_core_phases[64];
uniform int   u_num_cores;
uniform float u_mem;
uniform float u_temp;

out vec4 out_color;
"#;

const FRAG_FOOTER: &str = r#"
void main() {
    mainImage(out_color, gl_FragCoord.xy);
}
"#;

fn load_shader_source() -> String {
    let user_code = include_str!("shader.frag");
    format!("{FRAG_HEADER}\n{user_code}\n{FRAG_FOOTER}")
}

pub struct Renderer {
    egl: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    context: egl::Context,
    surface: egl::Surface,
    _egl_window: wayland_egl::WlEglSurface,
    program: u32,
    loc_resolution: i32,
    loc_time: i32,
    loc_core_phases: i32,
    loc_num_cores: i32,
    loc_mem: i32,
    loc_temp: i32,
    start: Instant,
    last_frame: Instant,
    phases: Vec<f32>,
    width: i32,
    height: i32,
}

impl Renderer {
    pub fn new(wl_display: &WlDisplay, wl_surface_id: &ObjectId, width: i32, height: i32) -> Self {
        let egl = unsafe {
            egl::DynamicInstance::<egl::EGL1_4>::load_required().expect("Failed to load libEGL")
        };

        let wl_display_ptr = wl_display.id().as_ptr() as *mut std::ffi::c_void;
        let display = unsafe { egl.get_display(wl_display_ptr as egl::NativeDisplayType) }
            .expect("Failed to get EGL display");

        egl.initialize(display).expect("Failed to initialize EGL");

        let attributes = [
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT,
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::NONE,
        ];

        let config = egl
            .choose_first_config(display, &attributes)
            .expect("EGL config selection failed")
            .expect("No matching EGL config");

        let context_attributes = [
            egl::CONTEXT_MAJOR_VERSION,
            3,
            egl::CONTEXT_MINOR_VERSION,
            0,
            egl::NONE,
        ];

        let context = egl
            .create_context(display, config, None, &context_attributes)
            .expect("Failed to create EGL context");

        let egl_window = wayland_egl::WlEglSurface::new(wl_surface_id.clone(), width, height)
            .expect("Failed to create EGL window");

        let surface = unsafe {
            egl.create_window_surface(
                display,
                config,
                egl_window.ptr() as egl::NativeWindowType,
                None,
            )
        }
        .expect("Failed to create EGL surface");

        egl.make_current(display, Some(surface), Some(surface), Some(context))
            .expect("Failed to make EGL context current");

        gl::load_with(|s| {
            egl.get_proc_address(s)
                .map_or(std::ptr::null(), |f| f as *const _)
        });

        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::ONE, gl::ONE_MINUS_SRC_ALPHA);
        }

        let program = compile_program();

        let loc = |name: &str| unsafe {
            let cname = CString::new(name).unwrap();
            gl::GetUniformLocation(program, cname.as_ptr())
        };

        unsafe {
            gl::UseProgram(program);
        }

        let now = Instant::now();
        Self {
            egl,
            display,
            context,
            surface,
            _egl_window: egl_window,
            program,
            loc_resolution: loc("iResolution"),
            loc_time: loc("iTime"),
            loc_core_phases: loc("u_core_phases[0]"),
            loc_num_cores: loc("u_num_cores"),
            loc_mem: loc("u_mem"),
            loc_temp: loc("u_temp"),
            start: now,
            last_frame: now,
            phases: Vec::new(),
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
        self._egl_window.resize(width, height, 0, 0);
        unsafe {
            gl::Viewport(0, 0, width, height);
        }
    }

    pub fn render(&mut self, cores: &[f32], mem: f32, temp: f32) {
        self.egl
            .make_current(
                self.display,
                Some(self.surface),
                Some(self.surface),
                Some(self.context),
            )
            .expect("Failed to make EGL context current");

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        let num_cores = cores.len().min(64);
        self.phases.resize(num_cores, 0.0);
        for (phase, &core) in self.phases.iter_mut().zip(cores.iter()) {
            *phase += dt * (2.0 + 18.0 * core * core);
        }

        let mut buf = [0.0f32; 64];
        buf[..num_cores].copy_from_slice(&self.phases);

        let t = self.start.elapsed().as_secs_f32();

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(self.program);
            gl::Uniform3f(
                self.loc_resolution,
                self.width as f32,
                self.height as f32,
                1.0,
            );
            gl::Uniform1f(self.loc_time, t);
            gl::Uniform1fv(self.loc_core_phases, 64, buf.as_ptr());
            gl::Uniform1i(self.loc_num_cores, num_cores as i32);
            gl::Uniform1f(self.loc_mem, mem);
            gl::Uniform1f(self.loc_temp, temp);

            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        self.egl
            .swap_buffers(self.display, self.surface)
            .expect("swap_buffers failed");
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_surface(self.display, self.surface);
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
    }
}

fn compile_program() -> u32 {
    unsafe {
        let vs = compile_shader(gl::VERTEX_SHADER, VERT_SRC);
        let fs = compile_shader(gl::FRAGMENT_SHADER, &load_shader_source());

        let program = gl::CreateProgram();
        gl::AttachShader(program, vs);
        gl::AttachShader(program, fs);
        gl::LinkProgram(program);

        let mut ok = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len as usize];
            gl::GetProgramInfoLog(
                program,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut _,
            );
            panic!("Program link error: {}", String::from_utf8_lossy(&buf));
        }

        gl::DeleteShader(vs);
        gl::DeleteShader(fs);

        let mut vao = 0;
        gl::GenVertexArrays(1, &mut vao);
        gl::BindVertexArray(vao);

        program
    }
}

fn compile_shader(kind: u32, src: &str) -> u32 {
    unsafe {
        let shader = gl::CreateShader(kind);
        let csrc = CString::new(src).unwrap();
        gl::ShaderSource(shader, 1, &csrc.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);

        let mut ok = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len as usize];
            gl::GetShaderInfoLog(
                shader,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut _,
            );
            let label = if kind == gl::VERTEX_SHADER {
                "Vertex"
            } else {
                "Fragment"
            };
            panic!(
                "{label} shader compile error: {}",
                String::from_utf8_lossy(&buf)
            );
        }

        shader
    }
}

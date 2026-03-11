mod egl;
mod stats;

use std::time::Duration;

use calloop::EventLoop;
use calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::WaylandSurface,
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};

const TARGET_FPS: u64 = 30;

struct Surface {
    output: wl_output::WlOutput,
    _layer_surface: LayerSurface,
    wl_surface: wl_surface::WlSurface,
    renderer: Option<egl::Renderer>,
    configured: bool,
}

struct App {
    registry: RegistryState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    _output: OutputState,

    surfaces: Vec<Surface>,
    stats: stats::Stats,
    running: bool,
}

impl App {
    fn create_surface_for_output(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
    ) {
        let wl_surface = self.compositor.create_surface(qh);

        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            wl_surface.clone(),
            Layer::Background,
            Some("vitals-rain"),
            Some(output),
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);

        wl_surface.commit();

        self.surfaces.push(Surface {
            output: output.clone(),
            _layer_surface: layer_surface,
            wl_surface,
            renderer: None,
            configured: false,
        });
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, mut event_queue) = registry_queue_init(&conn).expect("Failed to init registry");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell not available");

    let mut app = App {
        registry: RegistryState::new(&globals),
        compositor,
        layer_shell,
        _output: OutputState::new(&globals, &qh),
        surfaces: Vec::new(),
        stats: stats::Stats::new(),
        running: true,
    };

    event_queue
        .roundtrip(&mut app)
        .expect("Initial roundtrip failed");

    let mut event_loop: EventLoop<App> = EventLoop::try_new().expect("Failed to create event loop");
    let loop_handle = event_loop.handle();

    smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .expect("Failed to insert wayland source");

    let frame_interval = Duration::from_millis(1000 / TARGET_FPS);
    let timer = Timer::from_duration(frame_interval);

    loop_handle
        .insert_source(timer, move |_, _, app| {
            app.stats.poll();

            let cores = &app.stats.cores;
            let mem = app.stats.mem;
            let temp = app.stats.temp;

            for surface in &mut app.surfaces {
                if surface.configured
                    && let Some(renderer) = &mut surface.renderer
                {
                    renderer.render(cores, mem, temp);
                }
            }

            TimeoutAction::ToDuration(frame_interval)
        })
        .expect("Failed to insert timer");

    while app.running {
        event_loop
            .dispatch(Duration::from_millis(16), &mut app)
            .expect("Event loop dispatch failed");
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self._output
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.create_surface_for_output(qh, &output);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.surfaces.retain(|s| s.output != output);
        if self.surfaces.is_empty() {
            self.running = false;
        }
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let layer_wl = layer.wl_surface();
        self.surfaces.retain(|s| s.wl_surface != *layer_wl);
        if self.surfaces.is_empty() {
            self.running = false;
        }
    }

    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (width, height) = (configure.new_size.0 as i32, configure.new_size.1 as i32);
        if width == 0 || height == 0 {
            return;
        }

        let layer_wl = layer.wl_surface();
        let Some(surface) = self.surfaces.iter_mut().find(|s| s.wl_surface == *layer_wl) else {
            return;
        };

        if let Some(renderer) = &mut surface.renderer {
            renderer.resize(width, height);
        } else {
            let display = conn.display();
            let surface_id = surface.wl_surface.id();
            surface.renderer = Some(egl::Renderer::new(&display, &surface_id, width, height));
        }

        surface.configured = true;
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_layer!(App);
delegate_registry!(App);

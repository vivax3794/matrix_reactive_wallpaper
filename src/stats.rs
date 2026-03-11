use std::fs;

const SMOOTHING: f32 = 0.05;

pub struct Stats {
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub mem: f32,
    pub temp: f32,
    prev_idle: u64,
    prev_total: u64,
    prev_core_idle: Vec<u64>,
    prev_core_total: Vec<u64>,
    temp_path: Option<String>,
}

impl Stats {
    pub fn new() -> Self {
        let temp_path = find_temp_sensor();
        let mut s = Self {
            cpu: 0.0,
            cores: Vec::new(),
            mem: 0.0,
            temp: 0.0,
            prev_idle: 0,
            prev_total: 0,
            prev_core_idle: Vec::new(),
            prev_core_total: Vec::new(),
            temp_path,
        };
        s.poll();
        s
    }

    pub fn poll(&mut self) {
        let (raw_cpu, raw_cores) = self.read_cpu();
        let raw_mem = Self::read_mem();
        let raw_temp = self.read_temp();

        self.cpu = lerp(self.cpu, raw_cpu, SMOOTHING);
        self.mem = lerp(self.mem, raw_mem, SMOOTHING);
        self.temp = lerp(self.temp, raw_temp, SMOOTHING);

        if self.cores.len() != raw_cores.len() {
            self.cores = raw_cores;
        } else {
            for (smoothed, raw) in self.cores.iter_mut().zip(&raw_cores) {
                *smoothed = lerp(*smoothed, *raw, SMOOTHING);
            }
        }
    }

    fn read_cpu(&mut self) -> (f32, Vec<f32>) {
        let Ok(content) = fs::read_to_string("/proc/stat") else {
            return (self.cpu, self.cores.clone());
        };

        let mut aggregate = self.cpu;
        let mut core_utils: Vec<f32> = Vec::new();

        for line in content.lines() {
            let Some(rest) = line.strip_prefix("cpu") else {
                continue;
            };

            let is_aggregate = rest.starts_with(' ');
            if !is_aggregate && !rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                continue;
            }

            let vals: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if vals.len() < 4 {
                continue;
            }

            let idle = vals[3] + vals.get(4).copied().unwrap_or(0);
            let total: u64 = vals.iter().sum();

            if is_aggregate {
                let d_idle = idle.wrapping_sub(self.prev_idle) as f64;
                let d_total = total.wrapping_sub(self.prev_total) as f64;
                self.prev_idle = idle;
                self.prev_total = total;
                aggregate = if d_total > 0.0 {
                    (1.0 - d_idle / d_total) as f32
                } else {
                    self.cpu
                };
            } else {
                let i = core_utils.len();
                if i >= 64 {
                    continue;
                }

                if i >= self.prev_core_idle.len() {
                    self.prev_core_idle.push(0);
                    self.prev_core_total.push(0);
                }

                let d_idle = idle.wrapping_sub(self.prev_core_idle[i]) as f64;
                let d_total = total.wrapping_sub(self.prev_core_total[i]) as f64;
                self.prev_core_idle[i] = idle;
                self.prev_core_total[i] = total;

                let util = if d_total > 0.0 {
                    (1.0 - d_idle / d_total) as f32
                } else {
                    0.0
                };
                core_utils.push(util);
            }
        }

        (aggregate, core_utils)
    }

    fn read_mem() -> f32 {
        let Ok(content) = fs::read_to_string("/proc/meminfo") else {
            return 0.0;
        };
        let mut total = 0u64;
        let mut available = 0u64;

        for line in content.lines() {
            if let Some(val) = line.strip_prefix("MemTotal:") {
                total = parse_kb(val);
            } else if let Some(val) = line.strip_prefix("MemAvailable:") {
                available = parse_kb(val);
            }
        }

        if total > 0 {
            1.0 - (available as f32 / total as f32)
        } else {
            0.0
        }
    }

    fn read_temp(&self) -> f32 {
        let Some(path) = &self.temp_path else {
            return self.temp;
        };
        let Ok(content) = fs::read_to_string(path) else {
            return self.temp;
        };
        let Ok(millideg) = content.trim().parse::<f64>() else {
            return self.temp;
        };
        let celsius = millideg / 1000.0;
        ((celsius - 30.0) / 70.0).clamp(0.0, 1.0) as f32
    }
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn find_temp_sensor() -> Option<String> {
    let base = "/sys/class/thermal";
    let Ok(entries) = fs::read_dir(base) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let type_path = path.join("type");
        if let Ok(t) = fs::read_to_string(&type_path) {
            let t = t.trim();
            if t.contains("x86_pkg") || t.contains("k10temp") || t.contains("coretemp") {
                let temp_path = path.join("temp");
                if temp_path.exists() {
                    return Some(temp_path.to_string_lossy().into_owned());
                }
            }
        }
    }

    for entry in fs::read_dir(base).ok()?.flatten() {
        let temp_path = entry.path().join("temp");
        if temp_path.exists() {
            return Some(temp_path.to_string_lossy().into_owned());
        }
    }

    None
}

fn lerp(current: f32, target: f32, alpha: f32) -> f32 {
    current + (target - current) * alpha
}

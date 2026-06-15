// Reads Minime host CPU metrics from /proc/self/stat.
// The result is cached for one second to avoid repeated disk reads.

use std::time::{Duration, Instant};

pub struct MinimeStats {
    last_use_ticks: u64,
    last_update: Instant,
    cached_value: f64,
}

impl MinimeStats {
    pub fn new() -> Self {
        Self {
            last_use_ticks: 0,
            last_update: Instant::now(),
            cached_value: 0.0,
        }
    }
}

impl MinimeStats {
    pub fn cpu_usage(&mut self) -> Option<f64> {
        let now = Instant::now();
        if now.duration_since(self.last_update) < Duration::from_secs(1) {
            return Some(self.cached_value);
        }

        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let r_paren = stat.rfind(')')?;
        let after_comm = &stat[r_paren + 1..];
        let mut parts = after_comm.split_whitespace();
        let utime_str = parts.nth(11)?;
        let ticks = utime_str.parse::<u64>().ok()?;
        let ticksps = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if ticksps <= 0 {
            return None;
        }

        let use_ticks = ticks * 100 / ticksps as u64;
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        let value = if self.last_use_ticks > 0 && elapsed > 0.0 {
            let diff = use_ticks.saturating_sub(self.last_use_ticks);
            diff as f64 / elapsed
        } else {
            0.0
        };

        self.last_use_ticks = use_ticks;
        self.last_update = now;
        self.cached_value = value;
        Some(value)
    }
}

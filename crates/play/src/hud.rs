// This module manages emulated game overlay metrics (HUD).
// It tracks real-time performance indicators such as FPS, emulated CPU speed, and host CPU usage.
// It handles rendering the statistics text directly onto the video buffer at appropriate positions.

use std::time::{Duration, Instant};
use crate::scale::{ScaleMode, calculate_scale_rect, ScaleRect};
use crate::video::frame::VideoFrameFormat;

pub struct HudState {
    enabled: bool,
    last_update: Instant,
    fps_ticks: u32,
    cpu_ticks: u32,
    fps_val: f64,
    cpu_val: f64,
    use_val: f64,
    #[allow(dead_code)]
    last_use_ticks: u64,
}

impl HudState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_update: Instant::now(),
            fps_ticks: 0,
            cpu_ticks: 0,
            fps_val: 0.0,
            cpu_val: 0.0,
            use_val: 0.0,
            last_use_ticks: 0,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn tick_fps(&mut self) {
        self.fps_ticks += 1;
    }

    pub fn tick_cpu(&mut self) {
        self.cpu_ticks += 1;
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        if elapsed >= Duration::from_secs(1) {
            let elapsed_secs = elapsed.as_secs_f64();
            self.fps_val = self.fps_ticks as f64 / elapsed_secs;
            self.cpu_val = self.cpu_ticks as f64 / elapsed_secs;
            self.update_cpu_usage(elapsed_secs);
            self.last_update = now;
            self.fps_ticks = 0;
            self.cpu_ticks = 0;
        }
    }

    #[cfg(target_os = "linux")]
    fn update_cpu_usage(&mut self, elapsed_secs: f64) {
        if let Some(ticks) = get_cpu_usage_ticks() {
            let ticksps = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            if ticksps > 0 {
                let use_ticks = ticks * 100 / ticksps as u64;
                if self.last_use_ticks > 0 {
                    let diff = use_ticks.saturating_sub(self.last_use_ticks);
                    self.use_val = diff as f64 / elapsed_secs;
                }
                self.last_use_ticks = use_ticks;
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn update_cpu_usage(&mut self, _elapsed_secs: f64) {}

    pub fn draw(
        &self,
        data: &mut [u8],
        width: u32,
        height: u32,
        pitch: usize,
        format: VideoFrameFormat,
        scale_mode: ScaleMode,
        aspect_ratio: f32,
    ) {
        let rect = calculate_scale_rect(scale_mode, width, height, aspect_ratio, 640, 480)
            .unwrap_or(ScaleRect { x: 0, y: 0, width: 640, height: 480 });
        let scale = (640 / width).min(480 / height).max(1);
        let tl = format!("{}x{} {}x", width, height, scale);
        let tr = format!("{},{} {}x{}", rect.x, rect.y, width * scale, height * scale);
        let bl = format!("{:.1}/{:.1} {}%", self.fps_val, self.cpu_val, self.use_val as i32);
        let br = format!("{}x{}", rect.width, rect.height);
        
        crate::video::hud::blit_text(&tl, 2, 2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&tr, -2, 2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&bl, 2, -2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&br, -2, -2, data, pitch, width, height, format);
    }
}

#[cfg(target_os = "linux")]
fn get_cpu_usage_ticks() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let r_paren = stat.rfind(')')?;
    let after_comm = &stat[r_paren + 1..];
    let mut parts = after_comm.split_whitespace();
    let utime_str = parts.nth(11)?;
    utime_str.parse::<u64>().ok()
}

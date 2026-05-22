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

    pub fn update(&mut self, host_cpu: f64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        if elapsed >= Duration::from_secs(1) {
            let elapsed_secs = elapsed.as_secs_f64();
            self.fps_val = self.fps_ticks as f64 / elapsed_secs;
            self.cpu_val = self.cpu_ticks as f64 / elapsed_secs;
            self.use_val = host_cpu;
            self.last_update = now;
            self.fps_ticks = 0;
            self.cpu_ticks = 0;
        }
    }

    fn format_hud_texts(&self, width: u32, height: u32, scale: u32, rect: &ScaleRect) -> (String, String, String, String) {
        let tl = format!("{}x{} {}x", width, height, scale);
        let tr = format!("{},{} {}x{}", rect.x, rect.y, width * scale, height * scale);
        let bl = format!("{:.1}/{:.1} {}%", self.fps_val, self.cpu_val, self.use_val as i32);
        let br = format!("{}x{}", rect.width, rect.height);
        (tl, tr, bl, br)
    }

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
        let (tl, tr, bl, br) = self.format_hud_texts(width, height, scale, &rect);
        
        crate::video::hud::blit_text(&tl, 2, 2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&tr, -2, 2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&bl, 2, -2, data, pitch, width, height, format);
        crate::video::hud::blit_text(&br, -2, -2, data, pitch, width, height, format);
    }
}

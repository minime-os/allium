// This module manages emulated game overlay metrics (HUD).
// It tracks real-time performance indicators such as FPS, emulated CPU speed, and host CPU usage.
// It handles rendering the statistics text directly onto the video buffer at appropriate positions.

use std::time::{Duration, Instant};
use crate::video::{ScaleMode, calculate_scale_rect, ScaleRect};
use crate::video::VideoFrameFormat;

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

    fn format_hud_texts(&self,
        width: u32,
        height: u32,
        scale: u32,
        rect: &ScaleRect,
    ) -> (String, String, String, String) {
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

        blit_text(&tl, 2, 2, data, pitch, width, height, format);
        blit_text(&tr, -2, 2, data, pitch, width, height, format);
        blit_text(&bl, 2, -2, data, pitch, width, height, format);
        blit_text(&br, -2, -2, data, pitch, width, height, format);
    }
}

// ---- Text rendering primitives for the HUD overlay ----

const FONT_MAP: [(char, &str); 18] = [
    ('0', " 111 1   11   11  111 1 111  11   11   1 111 "),
    ('1', "   1  111    1    1    1    1    1    1    1 "),
    ('2', " 111 1   1    1   1   1   1   1    1    11111"),
    ('3', " 111 1   1    1    1 111     1    11   1 111 "),
    ('4', "1   11   11   11   11   11   111111    1    1"),
    ('5', "111111    1    1111     1    1    11   1 111 "),
    ('6', " 111 1    1    1111 1   11   11   11   1 111 "),
    ('7', "11111    1    1   1   1   1   1   1   1   1  "),
    ('8', " 111 1   11   11   1 111 1   11   11   1 111 "),
    ('9', " 111 1   11   11   11   1 1111    1    1 111 "),
    ('.', "                                      11   11"),
    (',', "                                 11   11  1  "),
    ('(', "   1   1    1   1    1    1     1    1     1 "),
    (')', " 1     1    1     1    1    1   1    1   1   "),
    ('/', "    1    1   1    1   1    1   1    1   1    "),
    ('x', "          1   1 1 1   1   1 1 1   1          "),
    ('%', " 1   1 1  1 1 1 1 1   1   1 1 1 1 1  1 1   1 "),
    ('-', "                    111                      "),
];

fn get_char_bitmap(c: char) -> &'static str {
    FONT_MAP.iter()
        .find(|&&(ch, _)| ch == c)
        .map(|&(_, map)| map)
        .unwrap_or("                                             ")
}

fn write_pixel(
    x: i32,
    y: i32,
    color: u32,
    data: &mut [u8],
    pitch: usize,
    format: VideoFrameFormat,
) {
    let bpp = match format {
        VideoFrameFormat::Rgb565 => 2,
        VideoFrameFormat::Xrgb8888 => 4,
    };
    let offset = y as usize * pitch + x as usize * bpp;
    match format {
        VideoFrameFormat::Rgb565 if offset + 1 < data.len() => {
            data[offset..offset + 2].copy_from_slice(&(color as u16).to_le_bytes());
        }
        VideoFrameFormat::Xrgb8888 if offset + 3 < data.len() => {
            data[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
        }
        _ => {}
    }
}

fn draw_black_rect(
    ox: i32,
    oy: i32,
    w: i32,
    h: i32,
    data: &mut [u8],
    pitch: usize,
    format: VideoFrameFormat,
    width: u32,
    height: u32,
) {
    for y in oy.max(0)..(oy + h).min(height as i32) {
        for x in ox.max(0)..(ox + w).min(width as i32) {
            write_pixel(x, y, 0, data, pitch, format);
        }
    }
}

fn draw_pixel_on_grid(
    c: char,
    gx: i32,
    gy: i32,
    params: (i32, i32, u32, usize, VideoFrameFormat, u32, u32),
    data: &mut [u8],
) {
    let (ox, oy, white, pitch, format, width, height) = params;
    let dx = ox + gx;
    let dy = oy + gy;
    if dx >= 0 && dx < width as i32 && dy >= 0 && dy < height as i32 {
        let bytes = get_char_bitmap(c).as_bytes();
        let idx = (gy * 5 + gx) as usize;
        if idx < bytes.len() && bytes[idx] == b'1' {
            write_pixel(dx, dy, white, data, pitch, format);
        }
    }
}

fn draw_character(
    c: char,
    ox: i32,
    oy: i32,
    data: &mut [u8],
    pitch: usize,
    format: VideoFrameFormat,
    width: u32,
    height: u32,
) {
    let white = match format {
        VideoFrameFormat::Rgb565 => 0xffff,
        VideoFrameFormat::Xrgb8888 => 0xffffffff,
    };
    for y in 0..9 {
        for x in 0..5 {
            let params = (ox, oy, white, pitch, format, width, height);
            draw_pixel_on_grid(c, x, y, params, &mut *data);
        }
    }
}

fn blit_text(
    text: &str,
    mut ox: i32,
    mut oy: i32,
    data: &mut [u8],
    pitch: usize,
    width: u32,
    height: u32,
    format: VideoFrameFormat,
) {
    let w = (6 * text.len() as i32) - 1;
    if ox < 0 { ox += width as i32 - w; }
    if oy < 0 { oy += height as i32 - 9; }
    draw_black_rect(ox - 1, oy - 1, w + 2, 11, data, pitch, format, width, height);
    let mut curr_x = ox;
    for c in text.chars() {
        draw_character(c, curr_x, oy, data, pitch, format, width, height);
        curr_x += 6;
    }
}

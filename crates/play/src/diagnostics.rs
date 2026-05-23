// Diagnostics and observability for the Play emulator.
// Includes the runtime HUD overlay (FPS, CPU metrics) and developer frame dump utilities.

use std::time::{Duration, Instant};
use crate::video::{ScaleMode, calculate_scale_rect, ScaleRect, VideoFrameFormat};

// ---- HUD: runtime performance overlay ----

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

// ---- Text rendering primitives ----

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
    let bytes = get_char_bitmap(c).as_bytes();
    for gy in 0..9 {
        for gx in 0..5 {
            let dx = ox + gx;
            let dy = oy + gy;
            if dx < 0 || dx >= width as i32 || dy < 0 || dy >= height as i32 {
                continue;
            }
            let idx = (gy * 5 + gx) as usize;
            if idx < bytes.len() && bytes[idx] == b'1' {
                write_pixel(dx, dy, white, data, pitch, format);
            }
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

// ---- Debug: frame dump to PPM ----

use anyhow::{Result, anyhow};
use std::fs;
use std::path::Path;
use crate::video::{
    CapturedFrame, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL, rgb565_to_rgb, validate_frame,
};

pub fn dump_frame(path: &Path, frame: &CapturedFrame, format: Option<VideoFrameFormat>) -> Result<()> {
    let ppm_data = match format {
        Some(VideoFrameFormat::Rgb565) => encode_rgb565(frame)?,
        Some(VideoFrameFormat::Xrgb8888) => encode_xrgb8888(frame)?,
        None => return Err(anyhow!("Frame dump requires a supported pixel format")),
    };
    fs::write(path, ppm_data)?;
    Ok(())
}

pub fn encode_rgb565(frame: &CapturedFrame) -> Result<Vec<u8>> {
    encode_ppm(frame, RGB565_BYTES_PER_PIXEL, |bytes| rgb565_to_rgb(bytes))
}

pub fn encode_xrgb8888(frame: &CapturedFrame) -> Result<Vec<u8>> {
    encode_ppm(frame, XRGB8888_BYTES_PER_PIXEL, |bytes| {
        [bytes[2], bytes[1], bytes[0]]
    })
}

fn encode_ppm<F>(frame: &CapturedFrame, bytes_per_pixel: usize, extract_rgb: F) -> Result<Vec<u8>>
where
    F: Fn(&[u8]) -> [u8; 3],
{
    validate_frame(frame, bytes_per_pixel)?;

    let mut ppm_data = Vec::with_capacity(ppm_len(frame.width, frame.height));
    ppm_data.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * bytes_per_pixel;
            ppm_data.extend_from_slice(&extract_rgb(&frame.data[pixel_start..]));
        }
    }

    Ok(ppm_data)
}

fn ppm_len(width: u32, height: u32) -> usize {
    format!("P6\n{} {}\n255\n", width, height).len() + width as usize * height as usize * 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::CapturedFrame;

    #[test]
    fn encodes_rgb565_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);
        let ppm = encode_rgb565(&frame).unwrap();
        assert_eq!(ppm, b"P6\n2 1\n255\n\xff\x00\x00\x00\xff\x00");
    }

    #[test]
    fn respects_pitch_padding() {
        let frame = CapturedFrame::new(
            vec![0x00, 0xf8, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
            1,
            2,
            4,
        );
        let ppm = encode_rgb565(&frame).unwrap();
        assert_eq!(ppm, b"P6\n1 2\n255\n\xff\x00\x00\x00\x00\xff");
    }

    #[test]
    fn encodes_xrgb8888_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0x00, 0xff, 0x00], 1, 1, 4);
        let ppm = encode_xrgb8888(&frame).unwrap();
        assert_eq!(ppm, b"P6\n1 1\n255\n\xff\x00\x00");
    }

    #[test]
    fn rejects_short_rows() {
        let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);
        let err = encode_rgb565(&frame).unwrap_err();
        assert_eq!(err.to_string(), "Frame pitch 2 is smaller than row size 4");
    }
}

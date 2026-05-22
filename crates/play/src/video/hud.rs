//! HUD rendering module for drawing text onto the video frame buffer.

use crate::video::frame::VideoFrameFormat;

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
    // Retrieves the 5x9 glyph bitmap string for the given character.
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
    // Writes a pixel color to the buffer according to the format at (x, y).
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
    // Fills a bounding rectangle with black background color for text contrast.
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
    // Draws a single pixel of the character's grid, ensuring memory-safety.
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
    // Draws a single character using a 5x9 glyph layout.
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

pub fn blit_text(
    text: &str,
    mut ox: i32,
    mut oy: i32,
    data: &mut [u8],
    pitch: usize,
    width: u32,
    height: u32,
    format: VideoFrameFormat,
) {
    // Renders the text string onto the target framebuffer at coordinate (ox, oy).
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


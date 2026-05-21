use crate::video::frame::VideoFrameFormat;

// Return a 5x9 pixel bitmap font map for the requested character.
// The font map is returned as a 45-byte string (9 rows of 5 columns).
fn get_char_bitmap(c: char) -> &'static str {
    match c {
        '0' => concat!(
            " 111 ",
            "1   1",
            "1   1",
            "1  11",
            "1 1 1",
            "11  1",
            "1   1",
            "1   1",
            " 111 "
        ),
        '1' => concat!(
            "   1 ",
            " 111 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 "
        ),
        '2' => concat!(
            " 111 ",
            "1   1",
            "    1",
            "   1 ",
            "  1  ",
            " 1   ",
            "1    ",
            "1    ",
            "11111"
        ),
        '3' => concat!(
            " 111 ",
            "1   1",
            "    1",
            "    1",
            " 111 ",
            "    1",
            "    1",
            "1   1",
            " 111 "
        ),
        '4' => concat!(
            "1   1",
            "1   1",
            "1   1",
            "1   1",
            "1   1",
            "1   1",
            "11111",
            "    1",
            "    1"
        ),
        '5' => concat!(
            "11111",
            "1    ",
            "1    ",
            "1111 ",
            "    1",
            "    1",
            "    1",
            "1   1",
            " 111 "
        ),
        '6' => concat!(
            " 111 ",
            "1    ",
            "1    ",
            "1111 ",
            "1   1",
            "1   1",
            "1   1",
            "1   1",
            " 111 "
        ),
        '7' => concat!(
            "11111",
            "    1",
            "    1",
            "   1 ",
            "  1  ",
            "  1  ",
            "  1  ",
            "  1  ",
            "  1  "
        ),
        '8' => concat!(
            " 111 ",
            "1   1",
            "1   1",
            "1   1",
            " 111 ",
            "1   1",
            "1   1",
            "1   1",
            " 111 "
        ),
        '9' => concat!(
            " 111 ",
            "1   1",
            "1   1",
            "1   1",
            "1   1",
            " 1111",
            "    1",
            "    1",
            " 111 "
        ),
        '.' => concat!(
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            " 11  ",
            " 11  "
        ),
        ',' => concat!(
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "  1  ",
            "  1  ",
            " 1   "
        ),
        '(' => concat!(
            "   1 ",
            "  1  ",
            " 1   ",
            " 1   ",
            " 1   ",
            " 1   ",
            " 1   ",
            "  1  ",
            "   1 "
        ),
        ')' => concat!(
            " 1   ",
            "  1  ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "   1 ",
            "  1  ",
            " 1   "
        ),
        '/' => concat!(
            "   1 ",
            "   1 ",
            "   1 ",
            "  1  ",
            "  1  ",
            "  1  ",
            " 1   ",
            " 1   ",
            " 1   "
        ),
        'x' => concat!(
            "     ",
            "     ",
            "1   1",
            "1   1",
            " 1 1 ",
            "  1  ",
            " 1 1 ",
            "1   1",
            "1   1"
        ),
        '%' => concat!(
            " 1   ",
            "1 1  ",
            "1 1 1",
            " 1 1 ",
            "  1  ",
            " 1 1 ",
            "1 1 1",
            "  1 1",
            "   1 "
        ),
        '-' => concat!(
            "     ",
            "     ",
            "     ",
            "     ",
            " 111 ",
            "     ",
            "     ",
            "     ",
            "     "
        ),
        _ => concat!(
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     ",
            "     "
        ),
    }
}

// Safely write a single pixel color (RGB565 or XRGB8888) with raw buffer boundary protection.
fn write_pixel(
    x: i32,
    y: i32,
    color: u32,
    data: &mut [u8],
    pitch: usize,
    format: VideoFrameFormat,
) {
    match format {
        VideoFrameFormat::Rgb565 => {
            let offset = y as usize * pitch + x as usize * 2;
            if offset + 1 < data.len() {
                data[offset] = (color & 0xff) as u8;
                data[offset + 1] = ((color >> 8) & 0xff) as u8;
            }
        }
        VideoFrameFormat::Xrgb8888 => {
            let offset = y as usize * pitch + x as usize * 4;
            if offset + 3 < data.len() {
                data[offset] = (color & 0xff) as u8;
                data[offset + 1] = ((color >> 8) & 0xff) as u8;
                data[offset + 2] = ((color >> 16) & 0xff) as u8;
                data[offset + 3] = ((color >> 24) & 0xff) as u8;
            }
        }
    }
}

// Safely clear a rectangular boundary region to black.
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
    for y in oy..(oy + h) {
        if y < 0 || y >= height as i32 {
            continue;
        }
        for x in ox..(ox + w) {
            if x < 0 || x >= width as i32 {
                continue;
            }
            write_pixel(x, y, 0, data, pitch, format);
        }
    }
}

// Draw a single 5x9 character onto the video buffer.
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
    let bitmap = get_char_bitmap(c);
    let bytes = bitmap.as_bytes();
    let white_color = match format {
        VideoFrameFormat::Rgb565 => 0xffff,
        VideoFrameFormat::Xrgb8888 => 0xffffffff,
    };
    for y in 0..9 {
        let draw_y = oy + y;
        if draw_y < 0 || draw_y >= height as i32 {
            continue;
        }
        for x in 0..5 {
            let draw_x = ox + x;
            if draw_x < 0 || draw_x >= width as i32 {
                continue;
            }
            let idx = (y * 5 + x) as usize;
            if idx < bytes.len() && bytes[idx] == b'1' {
                write_pixel(draw_x, draw_y, white_color, data, pitch, format);
            }
        }
    }
}

// Main entrypoint to blit aligned debug overlays onto the active presentation frame buffer.
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
    let char_width = 5;
    let letter_spacing = 1;
    let len = text.len() as i32;
    let w = ((char_width + letter_spacing) * len) - 1;
    let h = 9;

    if ox < 0 {
        ox = width as i32 - w + ox;
    }
    if oy < 0 {
        oy = height as i32 - h + oy;
    }

    draw_black_rect(ox - 1, oy - 1, w + 2, h + 2, data, pitch, format, width, height);

    let mut current_x = ox;
    for c in text.chars() {
        draw_character(c, current_x, oy, data, pitch, format, width, height);
        current_x += char_width + letter_spacing;
    }
}

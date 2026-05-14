use anyhow::{Result, anyhow};

use super::frame::{
    BGRA8888_BYTES_PER_PIXEL, CapturedFrame, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL,
    rgb565_to_rgb, validate_frame,
};
use crate::scale::ScaleRect;

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
pub fn scale_rgb565_to_xrgb8888(
    frame: &CapturedFrame,
    output: &mut [u32],
    output_width: u32,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_u32_output(output, output_width, output_height, rect)?;
    output.fill(0);

    for_each_scaled_pixel(
        frame,
        RGB565_BYTES_PER_PIXEL,
        output_width,
        rect,
        |source_start, output_index| {
            let [r, g, b] = rgb565_to_rgb(&frame.data[source_start..source_start + 2]);
            output[output_index] = pack_xrgb8888(r, g, b);
        },
    );

    Ok(())
}

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
pub fn scale_xrgb8888_to_xrgb8888(
    frame: &CapturedFrame,
    output: &mut [u32],
    output_width: u32,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_scaled_u32_output(output, output_width, output_height, rect)?;
    output.fill(0);

    for_each_scaled_pixel(
        frame,
        XRGB8888_BYTES_PER_PIXEL,
        output_width,
        rect,
        |source_start, output_index| {
            let bytes = &frame.data[source_start..source_start + XRGB8888_BYTES_PER_PIXEL];
            output[output_index] = pack_xrgb8888(bytes[2], bytes[1], bytes[0]);
        },
    );

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn scale_rgb565_to_rgb565(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(
        output,
        output_pitch,
        output_height,
        rect,
        RGB565_BYTES_PER_PIXEL,
    )?;
    output.fill(0);

    for_each_scaled_pixel(
        frame,
        RGB565_BYTES_PER_PIXEL,
        output_pitch as u32 / RGB565_BYTES_PER_PIXEL as u32,
        rect,
        |source_start, output_index| {
            let output_start = output_index * RGB565_BYTES_PER_PIXEL;
            output[output_start..output_start + RGB565_BYTES_PER_PIXEL]
                .copy_from_slice(&frame.data[source_start..source_start + RGB565_BYTES_PER_PIXEL]);
        },
    );

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn scale_rgb565_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(
        output,
        output_pitch,
        output_height,
        rect,
        BGRA8888_BYTES_PER_PIXEL,
    )?;
    fill_bgra8888_black(output);

    for_each_scaled_pixel(
        frame,
        RGB565_BYTES_PER_PIXEL,
        output_pitch as u32 / BGRA8888_BYTES_PER_PIXEL as u32,
        rect,
        |source_start, output_index| {
            let output_start = output_index * BGRA8888_BYTES_PER_PIXEL;
            let [r, g, b] = rgb565_to_rgb(&frame.data[source_start..source_start + 2]);
            output[output_start] = b;
            output[output_start + 1] = g;
            output[output_start + 2] = r;
            output[output_start + 3] = 0xff;
        },
    );

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn scale_xrgb8888_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(
        output,
        output_pitch,
        output_height,
        rect,
        BGRA8888_BYTES_PER_PIXEL,
    )?;
    fill_bgra8888_black(output);

    for_each_scaled_pixel(
        frame,
        XRGB8888_BYTES_PER_PIXEL,
        output_pitch as u32 / BGRA8888_BYTES_PER_PIXEL as u32,
        rect,
        |source_start, output_index| {
            let output_start = output_index * BGRA8888_BYTES_PER_PIXEL;
            output[output_start..output_start + BGRA8888_BYTES_PER_PIXEL].copy_from_slice(
                &frame.data[source_start..source_start + XRGB8888_BYTES_PER_PIXEL],
            );
            output[output_start + 3] = 0xff;
        },
    );

    Ok(())
}

fn for_each_scaled_pixel<F>(
    frame: &CapturedFrame,
    bytes_per_pixel: usize,
    output_width: u32,
    rect: ScaleRect,
    mut write: F,
) where
    F: FnMut(usize, usize),
{
    for dst_y in 0..rect.height {
        let src_y = dst_y as u64 * frame.height as u64 / rect.height as u64;
        for dst_x in 0..rect.width {
            let src_x = dst_x as u64 * frame.width as u64 / rect.width as u64;
            let source_start = src_y as usize * frame.pitch + src_x as usize * bytes_per_pixel;
            let output_index =
                (rect.y + dst_y) as usize * output_width as usize + (rect.x + dst_x) as usize;
            write(source_start, output_index);
        }
    }
}

fn validate_scaled_u32_output(
    output: &[u32],
    output_width: u32,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_scaled_rect(output_width, output_height, rect)?;
    let expected_len = output_width as usize * output_height as usize;
    if output.len() < expected_len {
        return Err(anyhow!(
            "Output buffer has {} pixels, expected at least {}",
            output.len(),
            expected_len
        ));
    }

    Ok(())
}

fn validate_scaled_byte_output(
    output: &[u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
    bytes_per_pixel: usize,
) -> Result<()> {
    let output_width = (output_pitch / bytes_per_pixel) as u32;
    validate_scaled_rect(output_width, output_height, rect)?;
    let expected_len = output_pitch * output_height as usize;
    if output.len() < expected_len {
        return Err(anyhow!(
            "Destination buffer has {} bytes, expected at least {}",
            output.len(),
            expected_len
        ));
    }

    Ok(())
}

fn validate_scaled_rect(output_width: u32, output_height: u32, rect: ScaleRect) -> Result<()> {
    if rect.width == 0 || rect.height == 0 {
        return Err(anyhow!("Scale destination size must be non-zero"));
    }
    if rect.x + rect.width > output_width || rect.y + rect.height > output_height {
        return Err(anyhow!("Scale destination rect exceeds output bounds"));
    }

    Ok(())
}

fn fill_bgra8888_black(output: &mut [u8]) {
    for pixel in output.chunks_exact_mut(BGRA8888_BYTES_PER_PIXEL) {
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
        pixel[3] = 0xff;
    }
}

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
fn pack_xrgb8888(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_rgb565_to_softbuffer_pixels() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8], 1, 1, 2);
        let mut output = vec![0; 4];

        scale_rgb565_to_xrgb8888(
            &frame,
            &mut output,
            2,
            2,
            ScaleRect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        )
        .unwrap();

        assert_eq!(output, vec![0x00ff0000; 4]);
    }

    #[test]
    fn scales_rgb565_to_bgra8888_with_letterbox() {
        let frame = CapturedFrame::new(vec![0xe0, 0x07], 1, 1, 2);
        let mut output = vec![0xaa; 24];

        scale_rgb565_to_bgra8888(
            &frame,
            &mut output,
            12,
            2,
            ScaleRect {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            },
        )
        .unwrap();

        assert_eq!(
            output,
            vec![
                0x00, 0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00,
                0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff
            ]
        );
    }
}

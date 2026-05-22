use anyhow::{Result, anyhow};

use super::frame::{
    CapturedFrame, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL, validate_frame,
};
#[cfg(feature = "simulator")]
use super::frame::rgb565_to_rgb;
#[cfg(feature = "miyoo")]
use super::frame::BGRA8888_BYTES_PER_PIXEL;
use crate::scale::ScaleRect;

#[cfg(feature = "simulator")]
pub fn scale_rgb565_to_xrgb8888(
    frame: &CapturedFrame,
    output: &mut [u32],
    w: u32,
    h: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_u32_output(output, w, h, rect)?;
    output.fill(0);
    for_each_scaled_pixel(frame, RGB565_BYTES_PER_PIXEL, w, rect, |src, out| {
        let [r, g, b] = rgb565_to_rgb(&frame.data[src..src + 2]);
        output[out] = pack_xrgb8888(r, g, b);
    });
    Ok(())
}

#[cfg(feature = "simulator")]
pub fn scale_xrgb8888_to_xrgb8888(
    frame: &CapturedFrame,
    output: &mut [u32],
    w: u32,
    h: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_scaled_u32_output(output, w, h, rect)?;
    output.fill(0);
    for_each_scaled_pixel(frame, XRGB8888_BYTES_PER_PIXEL, w, rect, |src, out| {
        let bytes = &frame.data[src..src + XRGB8888_BYTES_PER_PIXEL];
        output[out] = pack_xrgb8888(bytes[2], bytes[1], bytes[0]);
    });
    Ok(())
}

/// Scaler row routine utilizing direct pointer writing to avoid slice boundary checks,
/// and performing 16.16 fixed-point step accumulation to completely avoid costly
/// hardware divisions on the Miyoo's ARM Cortex-A7 CPU.
#[cfg(feature = "miyoo")]
fn scale_rgb565_row(frame: &CapturedFrame, out: *mut u16, out_pitch_px: usize, out_h: u32, rect: ScaleRect, dst_y: u32, src_y: usize, step_x: u32) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    let out_row = unsafe { out.add((out_h - 1 - rect.y - dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let out_x = (rect.x + rect.width - 1 - dst_x) as usize;
        unsafe { *out_row.add(out_x) = pixel; }
    }
}

/// Scaling routine optimized for Miyoo hardware to bypass memory bandwidth limits
/// by directly scaling to the framebuffer, avoiding clearing or allocations.
#[cfg(feature = "miyoo")]
pub fn scale_rgb565_to_rgb565(frame: &CapturedFrame, output: &mut [u8], output_pitch: usize, output_height: u32, rect: ScaleRect) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(output, output_pitch, output_height, rect, RGB565_BYTES_PER_PIXEL)?;
    let step_x = ((frame.width as u32) << 16) / rect.width;
    let step_y = ((frame.height as u32) << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u16;
    let out_pitch_px = output_pitch / 2;
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_row(frame, out_ptr, out_pitch_px, output_height, rect, dst_y, src_y, step_x);
    }
    Ok(())
}

/// Specialized pixel conversion and scaling that maps RGB565 input to 32-bit BGRA
/// output, utilizing single-word 32-bit writes to bypass per-channel memory stores
/// and applying bitwise approximations instead of division to scale colors.
#[cfg(feature = "miyoo")]
fn scale_rgb565_to_bgra8888_row(frame: &CapturedFrame, out: *mut u32, out_pitch_px: usize, out_h: u32, rect: ScaleRect, dst_y: u32, src_y: usize, step_x: u32) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    let out_row = unsafe { out.add((out_h - 1 - rect.y - dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) } as u32;
        let r = (pixel >> 11) & 0x1f;
        let g = (pixel >> 5) & 0x3f;
        let b = pixel & 0x1f;
        let bgra = (0xff << 24) | (((r << 3) | (r >> 2)) << 16) | (((g << 2) | (g >> 4)) << 8) | ((b << 3) | (b >> 2));
        let out_x = (rect.x + rect.width - 1 - dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
    }
}

/// Scaling routine optimized for Miyoo hardware to bypass memory bandwidth limits
/// by directly scaling to the 32-bit framebuffer, avoiding clearing or allocations.
#[cfg(feature = "miyoo")]
pub fn scale_rgb565_to_bgra8888(frame: &CapturedFrame, output: &mut [u8], output_pitch: usize, output_height: u32, rect: ScaleRect) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(output, output_pitch, output_height, rect, BGRA8888_BYTES_PER_PIXEL)?;
    let step_x = ((frame.width as u32) << 16) / rect.width;
    let step_y = ((frame.height as u32) << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u32;
    let out_pitch_px = output_pitch / 4;
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_to_bgra8888_row(frame, out_ptr, out_pitch_px, output_height, rect, dst_y, src_y, step_x);
    }
    Ok(())
}

/// Specialized scaler row routine that scales XRGB8888 input to BGRA8888 by applying
/// a fast bitwise OR to inject full alpha, using 32-bit word copies to maximize throughput.
#[cfg(feature = "miyoo")]
fn scale_xrgb8888_to_bgra8888_row(frame: &CapturedFrame, out: *mut u32, out_pitch_px: usize, out_h: u32, rect: ScaleRect, dst_y: u32, src_y: usize, step_x: u32) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u32).add(src_y * (frame.pitch / 4)) };
    let out_row = unsafe { out.add((out_h - 1 - rect.y - dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let bgra = pixel | 0xff000000;
        let out_x = (rect.x + rect.width - 1 - dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
    }
}

/// Scaling routine optimized for Miyoo hardware to bypass memory bandwidth limits
/// by directly scaling to the 32-bit framebuffer, avoiding clearing or allocations.
#[cfg(feature = "miyoo")]
pub fn scale_xrgb8888_to_bgra8888(frame: &CapturedFrame, output: &mut [u8], output_pitch: usize, output_height: u32, rect: ScaleRect) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(output, output_pitch, output_height, rect, BGRA8888_BYTES_PER_PIXEL)?;
    let step_x = ((frame.width as u32) << 16) / rect.width;
    let step_y = ((frame.height as u32) << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u32;
    let out_pitch_px = output_pitch / 4;
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_xrgb8888_to_bgra8888_row(frame, out_ptr, out_pitch_px, output_height, rect, dst_y, src_y, step_x);
    }
    Ok(())
}
#[cfg(feature = "simulator")]
fn for_each_scaled_pixel<F>(frame: &CapturedFrame, bpp: usize, out_w: u32, rect: ScaleRect, mut write: F)
where F: FnMut(usize, usize) {
    for dst_y in 0..rect.height {
        let src_y = dst_y as u64 * frame.height as u64 / rect.height as u64;
        let y_pitch = src_y as usize * frame.pitch;
        let out_y_pitch = (rect.y + dst_y) as usize * out_w as usize;
        for dst_x in 0..rect.width {
            let src_x = dst_x as u64 * frame.width as u64 / rect.width as u64;
            let source_start = y_pitch + src_x as usize * bpp;
            let output_index = out_y_pitch + (rect.x + dst_x) as usize;
            write(source_start, output_index);
        }
    }
}

#[cfg(feature = "simulator")]
fn validate_scaled_u32_output(
    output: &[u32],
    output_width: u32,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    validate_scaled_rect(output_width, output_height, rect)?;
    let expected = output_width as usize * output_height as usize;
    if output.len() < expected {
        return Err(anyhow!("Output buffer has {} pixels, expected at least {}", output.len(), expected));
    }
    Ok(())
}

#[cfg(feature = "miyoo")]
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
        return Err(anyhow!("Destination buffer has {} bytes, expected at least {}", output.len(), expected_len));
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



#[cfg(feature = "simulator")]
fn pack_xrgb8888(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "simulator")]
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
    #[cfg(feature = "miyoo")]
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
                0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0xff, 0x00, 0xff, 0xaa, 0xaa, 0xaa, 0xaa,
                0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0xff, 0x00, 0xff, 0xaa, 0xaa, 0xaa, 0xaa,
            ]
        );
    }
}

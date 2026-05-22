// Miyoo-specific framebuffer presentation logic.
// This module writes raw pixels directly to the physical /dev/fb0 framebuffer.

use crate::platform::VideoBackend;
use crate::scale::{ScaleMode, ScaleRect, calculate_scale_rect};
use crate::frame::{
    CapturedFrame, VideoFrameFormat, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL,
    validate_frame,
};

pub(crate) const BGRA8888_BYTES_PER_PIXEL: usize = 4;
use crate::platform::VideoPresentResult;
use anyhow::{Result, anyhow};
use framebuffer::Framebuffer;
use log::info;

const FRAMEBUFFER_PATH: &str = "/dev/fb0";
const RGB565_BITS_PER_PIXEL: u32 = 16;
const BGRA8888_BITS_PER_PIXEL: u32 = 32;

pub struct MiyooVideo {
    fb: Framebuffer,
    pitch: usize,
    height: u32,
    format: MiyooFramebufferFormat,
    rect: ScaleRect,
}

#[derive(Clone, Copy)]
enum MiyooFramebufferFormat {
    Rgb565,
    Bgra8888,
}

impl MiyooVideo {
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        let mut fb = Framebuffer::new(FRAMEBUFFER_PATH)?;
        let format = get_miyoo_fb_format(fb.var_screen_info.bits_per_pixel)?;
        let pitch = fb.fix_screen_info.line_length as usize;
        let width = fb.var_screen_info.xres;
        let height = fb.var_screen_info.yres;
        let rect = calculate_scale_rect(scale, source_width, source_height, aspect_ratio, width, height)?;
        info!(
            "Miyoo framebuffer initialized at {}x{} pitch={} bpp={}",
            width, height, pitch, fb.var_screen_info.bits_per_pixel
        );
        fb.frame.fill(0);
        Ok(Self { fb, pitch, height, format, rect })
    }

    fn width(&self) -> u32 {
        self.fb.var_screen_info.xres
    }

    fn scale_to_fb(&mut self, frame: &CapturedFrame, fmt: VideoFrameFormat) -> Result<()> {
        match (self.format, fmt) {
            (MiyooFramebufferFormat::Rgb565, VideoFrameFormat::Rgb565) => {
                scale_rgb565_to_rgb565(frame, &mut self.fb.frame, self.pitch, self.height, self.rect)
            }
            (MiyooFramebufferFormat::Rgb565, VideoFrameFormat::Xrgb8888) => {
                Err(anyhow!("Miyoo 16-bit framebuffer does not support XRGB8888 frames"))
            }
            (MiyooFramebufferFormat::Bgra8888, VideoFrameFormat::Rgb565) => {
                scale_rgb565_to_bgra8888(frame, &mut self.fb.frame, self.pitch, self.height, self.rect)
            }
            (MiyooFramebufferFormat::Bgra8888, VideoFrameFormat::Xrgb8888) => {
                scale_xrgb8888_to_bgra8888(frame, &mut self.fb.frame, self.pitch, self.height, self.rect)
            }
        }
    }
}

impl VideoBackend for MiyooVideo {
    fn present(
        &mut self,
        frame: &CapturedFrame,
        pixel_format: VideoFrameFormat,
    ) -> Result<VideoPresentResult> {
        self.scale_to_fb(frame, pixel_format)?;
        Ok(VideoPresentResult::default())
    }

    fn set_scale(
        &mut self,
        mode: ScaleMode,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
    ) -> Result<()> {
        self.rect = calculate_scale_rect(
            mode,
            source_width,
            source_height,
            aspect_ratio,
            self.width(),
            self.height,
        )?;
        self.fb.frame.fill(0);
        Ok(())
    }
}

fn get_miyoo_fb_format(bits: u32) -> Result<MiyooFramebufferFormat> {
    match bits {
        RGB565_BITS_PER_PIXEL => Ok(MiyooFramebufferFormat::Rgb565),
        BGRA8888_BITS_PER_PIXEL => Ok(MiyooFramebufferFormat::Bgra8888),
        bpp => Err(anyhow!(
            "Play Miyoo video supports 16-bit RGB565 or 32-bit BGRA8888 framebuffer, got {} bits per pixel",
            bpp
        )),
    }
}

// =========================================================================
// Miyoo Pixel Scaling & Color Space Conversion
// =========================================================================

/// Scaler row routine utilizing direct pointer writing to avoid slice boundary checks,
/// and performing 16.16 fixed-point step accumulation to completely avoid costly
/// hardware divisions on the Miyoo's ARM Cortex-A7 CPU.
fn scale_rgb565_row(
    frame: &CapturedFrame,
    out: *mut u16,
    out_pitch_px: usize,
    out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
) {
    // Access raw input data directly to bypass bounds check for maximum CPU throughput.
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    // Miyoo framebuffer is vertically flipped relative to typical capture buffers, so
    // we reverse the y index here to present it upright.
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
pub fn scale_rgb565_to_rgb565(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    // Validate inputs upfront so that subsequent unsafe operations are guaranteed
    // to be memory-safe and won't cause segfaults.
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
fn scale_rgb565_to_bgra8888_row(
    frame: &CapturedFrame,
    out: *mut u32,
    out_pitch_px: usize,
    out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
) {
    // Access input pixels as 16-bit values and cast to 32-bit to compute scaling factor.
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    // Handle the upside-down layout of the Miyoo LCD framebuffer relative to the console frame.
    let out_row = unsafe { out.add((out_h - 1 - rect.y - dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) } as u32;
        let r = (pixel >> 11) & 0x1f;
        let g = (pixel >> 5) & 0x3f;
        let b = pixel & 0x1f;
        // Use fast shifts instead of division to extend 5/6 bits to 8 bits.
        let bgra = (0xff << 24) | (((r << 3) | (r >> 2)) << 16) | (((g << 2) | (g >> 4)) << 8) | ((b << 3) | (b >> 2));
        let out_x = (rect.x + rect.width - 1 - dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
    }
}

/// Scaling routine optimized for Miyoo hardware to bypass memory bandwidth limits
/// by directly scaling to the 32-bit framebuffer, avoiding clearing or allocations.
pub fn scale_rgb565_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    // Perform boundary checks once before entering the critical performance loop.
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
fn scale_xrgb8888_to_bgra8888_row(
    frame: &CapturedFrame,
    out: *mut u32,
    out_pitch_px: usize,
    out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
) {
    // Read source pixels as 32-bit values directly.
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u32).add(src_y * (frame.pitch / 4)) };
    // Reverse vertical layout for the device screen orientation.
    let out_row = unsafe { out.add((out_h - 1 - rect.y - dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        // Miyoo 32-bit mode is BGRA, XRGB has identical byte order on little-endian ARM.
        // We set the alpha channel to 0xff explicitly to prevent transparency.
        let bgra = pixel | 0xff000000;
        let out_x = (rect.x + rect.width - 1 - dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
    }
}

/// Scaling routine optimized for Miyoo hardware to bypass memory bandwidth limits
/// by directly scaling to the 32-bit framebuffer, avoiding clearing or allocations.
pub fn scale_xrgb8888_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
) -> Result<()> {
    // Check bounds beforehand to guarantee safe raw memory access during scale loop.
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

/// Validates that the output buffer is large enough for the scaled rect.
fn validate_scaled_byte_output(
    output: &[u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
    bytes_per_pixel: usize,
) -> Result<()> {
    // Derive the pixel width from byte pitch to validate rectangle sizing.
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

/// Verifies that the scale rectangle boundaries do not exceed output screen sizes.
fn validate_scaled_rect(output_width: u32, output_height: u32, rect: ScaleRect) -> Result<()> {
    // Prevent zero dimension sizes to avoid divisions by zero in layout formulas.
    if rect.width == 0 || rect.height == 0 {
        return Err(anyhow!("Scale destination size must be non-zero"));
    }
    // Prevent drawing out of bounds, protecting from out of bounds memory writes.
    if rect.x + rect.width > output_width || rect.y + rect.height > output_height {
        return Err(anyhow!("Scale destination rect exceeds output bounds"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
                0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0xff, 0x00, 0xff, 0xaa, 0xaa, 0xaa, 0xaa,
                0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0xff, 0x00, 0xff, 0xaa, 0xaa, 0xaa, 0xaa,
            ]
        );
    }
}

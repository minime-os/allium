// Minime-specific framebuffer presentation logic.
// This module writes raw pixels directly to the physical /dev/fb0 framebuffer.

use crate::settings::{ScreenEffect, ScreenSharpness};
use crate::video::{
    CapturedFrame, RGB565_BYTES_PER_PIXEL, VideoFrameFormat, XRGB8888_BYTES_PER_PIXEL,
    validate_frame,
};
use crate::video::{
    ScaleMode, ScaleRect, apply_rgb565_effect, calculate_scale_rect, rgb565_to_bgra8888,
    validate_scaled_rect,
};

pub(crate) const BGRA8888_BYTES_PER_PIXEL: usize = 4;
use anyhow::{Result, anyhow};
use common::platform::minime::Traits;
use framebuffer::Framebuffer;
use log::info;

const RGB565_BITS_PER_PIXEL: u32 = 16;
const BGRA8888_BITS_PER_PIXEL: u32 = 32;

pub struct MinimeVideo {
    fb: Framebuffer,
    logical_frame: Vec<u8>,
    logical_pitch: usize,
    width: u32,
    height: u32,
    rotation: u32,
    format: MinimeFramebufferFormat,
    rect: ScaleRect,
    effect: ScreenEffect,
    sharpness: ScreenSharpness,
    /// Integer scale factor (1–4) when ScaleMode::Native, None otherwise.
    scale_factor: Option<u32>,
}

#[derive(Clone, Copy)]
enum MinimeFramebufferFormat {
    Rgb565,
    Bgra8888,
}

impl MinimeVideo {
    pub fn new(
        traits: &Traits,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        let mut fb = Framebuffer::new(&traits.video_device)?;
        let format = get_fb_format(fb.var_screen_info.bits_per_pixel)?;
        let width = traits.screen_width;
        let height = traits.screen_height;
        let bytes_per_pixel = fb.var_screen_info.bits_per_pixel as usize / 8;
        let logical_pitch = width as usize * bytes_per_pixel;
        let rect = calculate_scale_rect(
            scale,
            source_width,
            source_height,
            aspect_ratio,
            width,
            height,
        )?;
        info!(
            "Minime framebuffer initialized at {}x{} pitch={} bpp={}",
            width, height, fb.fix_screen_info.line_length, fb.var_screen_info.bits_per_pixel
        );
        fb.frame.fill(0);
        Ok(Self {
            fb,
            logical_frame: vec![0; logical_pitch * height as usize],
            logical_pitch,
            width,
            height,
            rotation: traits.screen_rotation,
            format,
            rect,
            effect: ScreenEffect::None,
            sharpness: ScreenSharpness::Soft,
            scale_factor: None,
        })
    }

    fn fb_width(&self) -> u32 {
        self.width
    }

    fn scale_to_fb(&mut self, frame: &CapturedFrame, fmt: VideoFrameFormat) -> Result<()> {
        match (self.format, fmt) {
            (MinimeFramebufferFormat::Rgb565, VideoFrameFormat::Rgb565) => scale_rgb565_to_rgb565(
                frame,
                &mut self.logical_frame,
                self.logical_pitch,
                self.height,
                self.rect,
                self.effect,
                self.scale_factor,
            ),
            (MinimeFramebufferFormat::Rgb565, VideoFrameFormat::Xrgb8888) => Err(anyhow!(
                "Minime 16-bit framebuffer does not support XRGB8888 frames"
            )),
            (MinimeFramebufferFormat::Bgra8888, VideoFrameFormat::Rgb565) => {
                scale_rgb565_to_bgra8888(
                    frame,
                    &mut self.logical_frame,
                    self.logical_pitch,
                    self.height,
                    self.rect,
                    self.effect,
                    self.scale_factor,
                )
            }
            (MinimeFramebufferFormat::Bgra8888, VideoFrameFormat::Xrgb8888) => {
                scale_xrgb8888_to_bgra8888(
                    frame,
                    &mut self.logical_frame,
                    self.logical_pitch,
                    self.height,
                    self.rect,
                )
            }
        }?;
        copy_rotated(
            &self.logical_frame,
            self.logical_pitch,
            self.width,
            self.height,
            self.fb.var_screen_info.bits_per_pixel as usize / 8,
            &mut self.fb.frame,
            self.fb.fix_screen_info.line_length as usize,
            self.fb.var_screen_info.xres,
            self.fb.var_screen_info.yres,
            self.rotation,
        )
    }
}

impl MinimeVideo {
    pub fn present(
        &mut self,
        frame: &CapturedFrame,
        pixel_format: VideoFrameFormat,
    ) -> Result<bool> {
        self.scale_to_fb(frame, pixel_format)?;
        Ok(false)
    }

    pub fn set_scale(
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
            self.fb_width(),
            self.height,
        )?;
        self.scale_factor = if mode == ScaleMode::Native {
            let sx = self.fb_width() / source_width;
            let sy = self.height / source_height;
            Some(sx.min(sy).max(1))
        } else {
            None
        };
        self.logical_frame.fill(0);
        Ok(())
    }

    pub fn set_effect(&mut self, effect: ScreenEffect) {
        self.effect = effect;
    }

    pub fn set_sharpness(&mut self, sharpness: ScreenSharpness) {
        self.sharpness = sharpness;
    }
}

fn get_fb_format(bits: u32) -> Result<MinimeFramebufferFormat> {
    match bits {
        RGB565_BITS_PER_PIXEL => Ok(MinimeFramebufferFormat::Rgb565),
        BGRA8888_BITS_PER_PIXEL => Ok(MinimeFramebufferFormat::Bgra8888),
        bpp => Err(anyhow!(
            "Play Minime video supports 16-bit RGB565 or 32-bit BGRA8888 framebuffer, got {} bits per pixel",
            bpp
        )),
    }
}

// =========================================================================
// Minime Pixel Scaling & Color Space Conversion
// =========================================================================

/// Per-blit context bundling all parameters that stay constant
/// across row-scaling calls. Modeled after minarch's GFX_Renderer.
struct BlitContext<'a> {
    frame: &'a CapturedFrame,
    rect: ScaleRect,
    step_x: u32,
    effect: ScreenEffect,
    scale_factor: Option<u32>,
}

fn scale_rgb565_row(ctx: &BlitContext, out: *mut u16, pitch_px: usize, dst_y: u32, src_y: usize) {
    let src_ptr =
        unsafe { (ctx.frame.data.as_ptr() as *const u16).add(src_y * (ctx.frame.pitch / 2)) };
    let out_row = unsafe { out.add((ctx.rect.y + dst_y) as usize * pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..ctx.rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += ctx.step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let pixel = if let Some(scale) = ctx.scale_factor {
            apply_rgb565_effect(pixel, ctx.effect, scale, dst_x, dst_y)
        } else {
            pixel
        };
        let out_x = (ctx.rect.x + dst_x) as usize;
        unsafe {
            *out_row.add(out_x) = pixel;
        }
    }
}

pub fn scale_rgb565_to_rgb565(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
    effect: ScreenEffect,
    scale_factor: Option<u32>,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(
        output,
        output_pitch,
        output_height,
        rect,
        RGB565_BYTES_PER_PIXEL,
    )?;
    let step_x = (frame.width << 16) / rect.width;
    let step_y = (frame.height << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u16;
    let out_pitch_px = output_pitch / 2;
    let ctx = BlitContext {
        frame,
        rect,
        step_x,
        effect,
        scale_factor,
    };
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_row(&ctx, out_ptr, out_pitch_px, dst_y, src_y);
    }
    Ok(())
}

fn scale_rgb565_to_bgra8888_row(
    ctx: &BlitContext,
    out: *mut u32,
    pitch_px: usize,
    dst_y: u32,
    src_y: usize,
) {
    let src_ptr =
        unsafe { (ctx.frame.data.as_ptr() as *const u16).add(src_y * (ctx.frame.pitch / 2)) };
    let out_row = unsafe { out.add((ctx.rect.y + dst_y) as usize * pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..ctx.rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += ctx.step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let pixel = if let Some(scale) = ctx.scale_factor {
            apply_rgb565_effect(pixel, ctx.effect, scale, dst_x, dst_y)
        } else {
            pixel
        };
        let bgra = rgb565_to_bgra8888(pixel);
        let out_x = (ctx.rect.x + dst_x) as usize;
        unsafe {
            *out_row.add(out_x) = bgra;
        }
    }
}

pub fn scale_rgb565_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
    output_height: u32,
    rect: ScaleRect,
    effect: ScreenEffect,
    scale_factor: Option<u32>,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_scaled_byte_output(
        output,
        output_pitch,
        output_height,
        rect,
        BGRA8888_BYTES_PER_PIXEL,
    )?;
    let step_x = (frame.width << 16) / rect.width;
    let step_y = (frame.height << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u32;
    let out_pitch_px = output_pitch / 4;
    let ctx = BlitContext {
        frame,
        rect,
        step_x,
        effect,
        scale_factor,
    };
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_to_bgra8888_row(&ctx, out_ptr, out_pitch_px, dst_y, src_y);
    }
    Ok(())
}

fn scale_xrgb8888_to_bgra8888_row(
    ctx: &BlitContext,
    out: *mut u32,
    pitch_px: usize,
    dst_y: u32,
    src_y: usize,
) {
    let src_ptr =
        unsafe { (ctx.frame.data.as_ptr() as *const u32).add(src_y * (ctx.frame.pitch / 4)) };
    let out_row = unsafe { out.add((ctx.rect.y + dst_y) as usize * pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..ctx.rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += ctx.step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let bgra = pixel | 0xff000000;
        let out_x = (ctx.rect.x + dst_x) as usize;
        unsafe {
            *out_row.add(out_x) = bgra;
        }
    }
}

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
    let step_x = (frame.width << 16) / rect.width;
    let step_y = (frame.height << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u32;
    let out_pitch_px = output_pitch / 4;
    let ctx = BlitContext {
        frame,
        rect,
        step_x,
        effect: ScreenEffect::default(),
        scale_factor: None,
    };
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_xrgb8888_to_bgra8888_row(&ctx, out_ptr, out_pitch_px, dst_y, src_y);
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

#[allow(clippy::too_many_arguments)]
fn copy_rotated(
    source: &[u8],
    source_pitch: usize,
    source_width: u32,
    source_height: u32,
    bytes_per_pixel: usize,
    output: &mut [u8],
    output_pitch: usize,
    output_width: u32,
    output_height: u32,
    rotation: u32,
) -> Result<()> {
    if source.len() < source_pitch * source_height as usize
        || output.len() < output_pitch * output_height as usize
    {
        return Err(anyhow!("Framebuffer buffer is smaller than its dimensions"));
    }
    for y in 0..source_height {
        for x in 0..source_width {
            let (output_x, output_y) = rotate_point(x, y, output_width, output_height, rotation);
            if output_x >= output_width || output_y >= output_height {
                return Err(anyhow!("Rotated framebuffer coordinate is out of bounds"));
            }
            let source_index = y as usize * source_pitch + x as usize * bytes_per_pixel;
            let output_index =
                output_y as usize * output_pitch + output_x as usize * bytes_per_pixel;
            output[output_index..output_index + bytes_per_pixel]
                .copy_from_slice(&source[source_index..source_index + bytes_per_pixel]);
        }
    }
    Ok(())
}

fn rotate_point(x: u32, y: u32, width: u32, height: u32, rotation: u32) -> (u32, u32) {
    match rotation {
        90 => (width - y - 1, x),
        180 => (width - x - 1, height - y - 1),
        270 => (y, height - x - 1),
        _ => (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_logical_arc_coordinates_clockwise() {
        assert_eq!(rotate_point(0, 0, 480, 640, 90), (479, 0));
        assert_eq!(rotate_point(639, 479, 480, 640, 90), (0, 639));
    }
}

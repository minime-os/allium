// RG35xxSP-specific framebuffer presentation logic.
// This module writes raw pixels directly to the physical /dev/fb0 framebuffer.

use crate::video::{ScaleMode, ScaleRect, calculate_scale_rect, validate_scaled_rect, rgb565_to_bgra8888, apply_rgb565_effect};
use crate::settings::{ScreenEffect, ScreenSharpness};
use crate::video::{
    CapturedFrame, VideoFrameFormat, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL,
    validate_frame,
};

pub(crate) const BGRA8888_BYTES_PER_PIXEL: usize = 4;
use anyhow::{Result, anyhow};
use framebuffer::Framebuffer;
use log::info;

const FRAMEBUFFER_PATH: &str = "/dev/fb0";
const RGB565_BITS_PER_PIXEL: u32 = 16;
const BGRA8888_BITS_PER_PIXEL: u32 = 32;

pub struct Rg35xxspVideo {
    fb: Framebuffer,
    pitch: usize,
    height: u32,
    format: Rg35xxspFramebufferFormat,
    rect: ScaleRect,
    effect: ScreenEffect,
    sharpness: ScreenSharpness,
    /// Integer scale factor (1–4) when ScaleMode::Native, None otherwise.
    scale_factor: Option<u32>,
}

#[derive(Clone, Copy)]
enum Rg35xxspFramebufferFormat {
    Rgb565,
    Bgra8888,
}

impl Rg35xxspVideo {
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        let mut fb = Framebuffer::new(FRAMEBUFFER_PATH)?;
        let format = get_fb_format(fb.var_screen_info.bits_per_pixel)?;
        let pitch = fb.fix_screen_info.line_length as usize;
        let width = fb.var_screen_info.xres;
        let height = fb.var_screen_info.yres;
        let rect = calculate_scale_rect(scale, source_width, source_height, aspect_ratio, width, height)?;
        info!(
            "RG35xxSP framebuffer initialized at {}x{} pitch={} bpp={}",
            width, height, pitch, fb.var_screen_info.bits_per_pixel
        );
        fb.frame.fill(0);
        Ok(Self { fb, pitch, height, format, rect, effect: ScreenEffect::None, sharpness: ScreenSharpness::Soft, scale_factor: None })
    }

    fn width(&self) -> u32 {
        self.fb.var_screen_info.xres
    }

    fn scale_to_fb(&mut self,
        frame: &CapturedFrame,
        fmt: VideoFrameFormat,
    ) -> Result<()> {
        match (self.format, fmt) {
            (Rg35xxspFramebufferFormat::Rgb565, VideoFrameFormat::Rgb565) => {
                scale_rgb565_to_rgb565(
                    frame, &mut self.fb.frame, self.pitch, self.height, self.rect,
                    self.effect, self.scale_factor,
                )
            }
            (Rg35xxspFramebufferFormat::Rgb565, VideoFrameFormat::Xrgb8888) => {
                Err(anyhow!("RG35xxSP 16-bit framebuffer does not support XRGB8888 frames"))
            }
            (Rg35xxspFramebufferFormat::Bgra8888, VideoFrameFormat::Rgb565) => {
                scale_rgb565_to_bgra8888(
                    frame, &mut self.fb.frame, self.pitch, self.height, self.rect,
                    self.effect, self.scale_factor,
                )
            }
            (Rg35xxspFramebufferFormat::Bgra8888, VideoFrameFormat::Xrgb8888) => {
                scale_xrgb8888_to_bgra8888(frame, &mut self.fb.frame, self.pitch, self.height, self.rect)
            }
        }
    }
}

impl Rg35xxspVideo {
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
            self.width(),
            self.height,
        )?;
        self.scale_factor = if mode == ScaleMode::Native {
            let sx = self.width() / source_width;
            let sy = self.height / source_height;
            Some(sx.min(sy).max(1))
        } else {
            None
        };
        self.fb.frame.fill(0);
        Ok(())
    }

    pub fn set_effect(&mut self, effect: ScreenEffect) {
        self.effect = effect;
    }

    pub fn set_sharpness(&mut self, sharpness: ScreenSharpness) {
        self.sharpness = sharpness;
    }
}

fn get_fb_format(bits: u32) -> Result<Rg35xxspFramebufferFormat> {
    match bits {
        RGB565_BITS_PER_PIXEL => Ok(Rg35xxspFramebufferFormat::Rgb565),
        BGRA8888_BITS_PER_PIXEL => Ok(Rg35xxspFramebufferFormat::Bgra8888),
        bpp => Err(anyhow!(
            "Play RG35xxSP video supports 16-bit RGB565 or 32-bit BGRA8888 framebuffer, got {} bits per pixel",
            bpp
        )),
    }
}

// =========================================================================
// RG35xxSP Pixel Scaling & Color Space Conversion
// =========================================================================

fn scale_rgb565_row(
    frame: &CapturedFrame,
    out: *mut u16,
    out_pitch_px: usize,
    _out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
    effect: ScreenEffect,
    scale_factor: Option<u32>,
) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    let out_row = unsafe { out.add((rect.y + dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let pixel = if let Some(scale) = scale_factor {
            apply_rgb565_effect(pixel, effect, scale, dst_x, dst_y)
        } else {
            pixel
        };
        let out_x = (rect.x + dst_x) as usize;
        unsafe { *out_row.add(out_x) = pixel; }
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
    validate_scaled_byte_output(output, output_pitch, output_height, rect, RGB565_BYTES_PER_PIXEL)?;
    let step_x = ((frame.width as u32) << 16) / rect.width;
    let step_y = ((frame.height as u32) << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u16;
    let out_pitch_px = output_pitch / 2;
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_row(frame, out_ptr, out_pitch_px, output_height, rect, dst_y, src_y, step_x, effect, scale_factor);
    }
    Ok(())
}

fn scale_rgb565_to_bgra8888_row(
    frame: &CapturedFrame,
    out: *mut u32,
    out_pitch_px: usize,
    _out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
    effect: ScreenEffect,
    scale_factor: Option<u32>,
) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u16).add(src_y * (frame.pitch / 2)) };
    let out_row = unsafe { out.add((rect.y + dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let pixel = if let Some(scale) = scale_factor {
            apply_rgb565_effect(pixel, effect, scale, dst_x, dst_y)
        } else {
            pixel
        };
        let bgra = rgb565_to_bgra8888(pixel);
        let out_x = (rect.x + dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
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
    validate_scaled_byte_output(output, output_pitch, output_height, rect, BGRA8888_BYTES_PER_PIXEL)?;
    let step_x = ((frame.width as u32) << 16) / rect.width;
    let step_y = ((frame.height as u32) << 16) / rect.height;
    let out_ptr = output.as_mut_ptr() as *mut u32;
    let out_pitch_px = output_pitch / 4;
    let mut fp_y = 0;
    for dst_y in 0..rect.height {
        let src_y = (fp_y >> 16) as usize;
        fp_y += step_y;
        scale_rgb565_to_bgra8888_row(frame, out_ptr, out_pitch_px, output_height, rect, dst_y, src_y, step_x, effect, scale_factor);
    }
    Ok(())
}

fn scale_xrgb8888_to_bgra8888_row(
    frame: &CapturedFrame,
    out: *mut u32,
    out_pitch_px: usize,
    _out_h: u32,
    rect: ScaleRect,
    dst_y: u32,
    src_y: usize,
    step_x: u32,
) {
    let src_ptr = unsafe { (frame.data.as_ptr() as *const u32).add(src_y * (frame.pitch / 4)) };
    let out_row = unsafe { out.add((rect.y + dst_y) as usize * out_pitch_px) };
    let mut fp_x = 0;
    for dst_x in 0..rect.width {
        let src_x = (fp_x >> 16) as usize;
        fp_x += step_x;
        let pixel = unsafe { *src_ptr.add(src_x) };
        let bgra = pixel | 0xff000000;
        let out_x = (rect.x + dst_x) as usize;
        unsafe { *out_row.add(out_x) = bgra; }
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

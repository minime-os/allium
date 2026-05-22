// Miyoo-specific framebuffer presentation logic.
// This module writes raw pixels directly to the physical /dev/fb0 framebuffer.

use crate::platform::VideoBackend;
use crate::scale::{ScaleMode, ScaleRect, calculate_scale_rect};
use crate::video::convert::{
    scale_rgb565_to_bgra8888, scale_rgb565_to_rgb565, scale_xrgb8888_to_bgra8888,
};
use crate::video::frame::{CapturedFrame, VideoFrameFormat};
use crate::video::VideoPresentResult;
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

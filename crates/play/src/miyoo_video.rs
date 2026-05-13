use crate::frame::{
    CapturedFrame, scale_rgb565_to_bgra8888, scale_rgb565_to_rgb565, scale_xrgb8888_to_bgra8888,
};
use crate::scale::{ScaleMode, ScaleRect, calculate_scale_rect};
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

#[derive(Clone, Copy)]
pub enum MiyooPixelFormat {
    Rgb565,
    Xrgb8888,
}

impl MiyooVideo {
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        let fb = Framebuffer::new(FRAMEBUFFER_PATH)?;
        let format = match fb.var_screen_info.bits_per_pixel {
            RGB565_BITS_PER_PIXEL => MiyooFramebufferFormat::Rgb565,
            BGRA8888_BITS_PER_PIXEL => MiyooFramebufferFormat::Bgra8888,
            bits_per_pixel => {
                return Err(anyhow!(
                    "Play Miyoo video supports 16-bit RGB565 or 32-bit BGRA8888 framebuffer, got {} bits per pixel",
                    bits_per_pixel
                ));
            }
        };

        let pitch = fb.fix_screen_info.line_length as usize;
        let width = fb.var_screen_info.xres;
        let height = fb.var_screen_info.yres;
        let rect = calculate_scale_rect(
            scale,
            source_width,
            source_height,
            aspect_ratio,
            width,
            height,
        )?;
        info!(
            "Miyoo framebuffer initialized at {}x{} pitch={} bpp={}",
            width, height, pitch, fb.var_screen_info.bits_per_pixel
        );

        Ok(Self {
            fb,
            pitch,
            height,
            format,
            rect,
        })
    }

    pub fn present(&mut self, frame: &CapturedFrame, pixel_format: MiyooPixelFormat) -> Result<()> {
        match (self.format, pixel_format) {
            (MiyooFramebufferFormat::Rgb565, MiyooPixelFormat::Rgb565) => scale_rgb565_to_rgb565(
                frame,
                &mut self.fb.frame,
                self.pitch,
                self.height,
                self.rect,
            ),
            (MiyooFramebufferFormat::Rgb565, MiyooPixelFormat::Xrgb8888) => Err(anyhow!(
                "Miyoo 16-bit framebuffer does not support XRGB8888 frames"
            )),
            (MiyooFramebufferFormat::Bgra8888, MiyooPixelFormat::Rgb565) => {
                scale_rgb565_to_bgra8888(
                    frame,
                    &mut self.fb.frame,
                    self.pitch,
                    self.height,
                    self.rect,
                )
            }
            (MiyooFramebufferFormat::Bgra8888, MiyooPixelFormat::Xrgb8888) => {
                scale_xrgb8888_to_bgra8888(
                    frame,
                    &mut self.fb.frame,
                    self.pitch,
                    self.height,
                    self.rect,
                )
            }
        }
    }
}

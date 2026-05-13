use crate::frame::{CapturedFrame, copy_rgb565_to_rgb565, copy_xrgb8888_to_rgb565};
use anyhow::{Result, anyhow};
use framebuffer::Framebuffer;
use log::info;

const FRAMEBUFFER_PATH: &str = "/dev/fb0";
const RGB565_BITS_PER_PIXEL: u32 = 16;

pub struct MiyooVideo {
    fb: Framebuffer,
    output: Vec<u8>,
    pitch: usize,
}

#[derive(Clone, Copy)]
pub enum MiyooPixelFormat {
    Rgb565,
    Xrgb8888,
}

impl MiyooVideo {
    pub fn new() -> Result<Self> {
        let fb = Framebuffer::new(FRAMEBUFFER_PATH)?;
        if fb.var_screen_info.bits_per_pixel != RGB565_BITS_PER_PIXEL {
            return Err(anyhow!(
                "Play Miyoo video requires RGB565 framebuffer, got {} bits per pixel",
                fb.var_screen_info.bits_per_pixel
            ));
        }

        let pitch = fb.fix_screen_info.line_length as usize;
        let output = vec![0; fb.frame.len()];
        info!(
            "Miyoo framebuffer initialized at {}x{} pitch={}",
            fb.var_screen_info.xres, fb.var_screen_info.yres, pitch
        );

        Ok(Self { fb, output, pitch })
    }

    pub fn present(&mut self, frame: &CapturedFrame, format: MiyooPixelFormat) -> Result<()> {
        match format {
            MiyooPixelFormat::Rgb565 => copy_rgb565_to_rgb565(frame, &mut self.output, self.pitch)?,
            MiyooPixelFormat::Xrgb8888 => {
                copy_xrgb8888_to_rgb565(frame, &mut self.output, self.pitch)?
            }
        }

        self.fb.frame.copy_from_slice(&self.output);
        Ok(())
    }
}

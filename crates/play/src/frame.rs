use anyhow::{Result, anyhow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFrameFormat {
    Rgb565,
    Xrgb8888,
}

// Keep a copied frame because libretro owns callback memory.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
}

impl CapturedFrame {
    pub fn new(data: Vec<u8>, width: u32, height: u32, pitch: usize) -> Self {
        Self {
            data,
            width,
            height,
            pitch,
        }
    }
}

pub(crate) const RGB565_BYTES_PER_PIXEL: usize = 2;
pub(crate) const XRGB8888_BYTES_PER_PIXEL: usize = 4;

pub(crate) fn validate_frame(frame: &CapturedFrame, bytes_per_pixel: usize) -> Result<()> {
    let row_bytes = frame.width as usize * bytes_per_pixel;
    if frame.pitch < row_bytes {
        return Err(anyhow!(
            "Frame pitch {} is smaller than row size {}",
            frame.pitch,
            row_bytes
        ));
    }

    let expected_len = frame.pitch * frame.height as usize;
    if frame.data.len() < expected_len {
        return Err(anyhow!(
            "Frame buffer has {} bytes, expected at least {}",
            frame.data.len(),
            expected_len
        ));
    }

    Ok(())
}

pub(crate) fn rgb565_to_rgb(bytes: &[u8]) -> [u8; 3] {
    let pixel = u16::from_le_bytes([bytes[0], bytes[1]]);
    [
        scale_5_to_8((pixel >> 11) & 0x1f),
        scale_6_to_8((pixel >> 5) & 0x3f),
        scale_5_to_8(pixel & 0x1f),
    ]
}

fn scale_5_to_8(value: u16) -> u8 {
    (u32::from(value) * 255 / 31) as u8
}

fn scale_6_to_8(value: u16) -> u8 {
    (u32::from(value) * 255 / 63) as u8
}

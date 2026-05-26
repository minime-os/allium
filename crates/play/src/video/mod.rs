// Video output abstractions: frame formats, scaling, and pixel conversion tables.

pub mod effects;
pub mod pixel;
pub mod scale;

use anyhow::{Result, anyhow};
use std::time::Duration;

pub use effects::{apply_rgb565_effect, weight3_1_rgb565};
pub use pixel::rgb565_to_bgra8888;
pub use pixel::rgb565_to_rgb;
pub use scale::{ScaleMode, ScaleRect, calculate_scale_rect, validate_scaled_rect};

// ---- Frame timing ----

pub fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }
    Ok(Duration::from_secs_f64(1.0 / fps))
}

// ---- Pixel formats and frame buffer types ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFrameFormat {
    Rgb565,
    Xrgb8888,
}

/// Owned or borrowed frame buffer. The borrowed variant wraps a raw pointer that
/// is only valid for the current frame loop iteration (set by the libretro video
/// refresh callback and consumed immediately by the platform presentation code).
#[derive(Clone, Debug)]
pub enum FrameData {
    Owned(Vec<u8>),
    Borrowed { ptr: *const u8, len: usize },
}

impl FrameData {
    pub fn owned(v: Vec<u8>) -> Self {
        Self::Owned(v)
    }

    pub fn borrowed(ptr: *const u8, len: usize) -> Self {
        Self::Borrowed { ptr, len }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            FrameData::Owned(v) => v.as_slice(),
            // SAFETY: the pointer is only set during the libretro video_refresh callback
            // and is consumed immediately in the same frame loop iteration before the
            // next retro_run() call.
            FrameData::Borrowed { ptr, len } => unsafe { std::slice::from_raw_parts(*ptr, *len) },
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            FrameData::Owned(v) => v.as_mut_slice(),
            FrameData::Borrowed { .. } => {
                panic!("Cannot mutably borrow borrowed frame data; copy to Owned first")
            }
        }
    }
}

impl std::ops::Deref for FrameData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for FrameData {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl AsRef<[u8]> for FrameData {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for FrameData {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl From<Vec<u8>> for FrameData {
    fn from(v: Vec<u8>) -> Self {
        Self::Owned(v)
    }
}

// Keep a copied frame because libretro owns callback memory.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub data: FrameData,
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
}

impl CapturedFrame {
    pub fn new(data: impl Into<FrameData>, width: u32, height: u32, pitch: usize) -> Self {
        Self {
            data: data.into(),
            width,
            height,
            pitch,
        }
    }

    pub fn new_empty() -> Self {
        Self::new(FrameData::owned(Vec::new()), 0, 0, 0)
    }
}

pub(crate) const RGB565_BYTES_PER_PIXEL: usize = 2;
pub(crate) const XRGB8888_BYTES_PER_PIXEL: usize = 4;

/// Validate that the frame buffer covers the expected size for the given pixel format.
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

#[cfg(test)]
mod tests;

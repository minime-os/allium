use anyhow::{Result, anyhow};
use clap::ValueEnum;

// ---- Pixel formats and frame buffer types ----

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

// ---- Scaling and layout ----

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ScaleMode {
    Native,
    Aspect,
    Fullscreen,
}

impl ScaleMode {
    pub fn next(self) -> Self {
        match self {
            Self::Native => Self::Aspect,
            Self::Aspect => Self::Fullscreen,
            Self::Fullscreen => Self::Native,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn calculate_scale_rect(
    mode: ScaleMode,
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    output_width: u32,
    output_height: u32,
) -> Result<ScaleRect> {
    validate_scale_dimensions(source_width, source_height, output_width, output_height)?;
    match mode {
        ScaleMode::Native => Ok(scale_native(
            source_width,
            source_height,
            output_width,
            output_height,
        )),
        ScaleMode::Aspect => Ok(scale_aspect(
            source_width,
            source_height,
            aspect_ratio,
            output_width,
            output_height,
        )),
        ScaleMode::Fullscreen => Ok(ScaleRect {
            x: 0,
            y: 0,
            width: output_width,
            height: output_height,
        }),
    }
}

fn validate_scale_dimensions(
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
) -> Result<()> {
    if source_width == 0 || source_height == 0 {
        return Err(anyhow!("Scale source size must be non-zero"));
    }
    if output_width == 0 || output_height == 0 {
        return Err(anyhow!("Scale output size must be non-zero"));
    }
    Ok(())
}

fn scale_native(
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
) -> ScaleRect {
    let scale = (output_width / source_width).min(output_height / source_height).max(1);
    let width = (source_width * scale).min(output_width);
    let height = (source_height * scale).min(output_height);
    center_rect(width, height, output_width, output_height)
}

fn get_aspect_ratio(source_width: u32, source_height: u32, aspect_ratio: f32) -> f64 {
    if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio as f64
    } else {
        source_width as f64 / source_height as f64
    }
}

fn scale_aspect(
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    output_width: u32,
    output_height: u32,
) -> ScaleRect {
    let aspect = get_aspect_ratio(source_width, source_height, aspect_ratio);
    let output_ratio = output_width as f64 / output_height as f64;
    let (width, height) = if aspect > output_ratio {
        (output_width, ((output_width as f64 / aspect).round() as u32).max(1))
    } else {
        (((output_height as f64 * aspect).round() as u32).max(1), output_height)
    };
    center_rect(width, height, output_width, output_height)
}

fn center_rect(width: u32, height: u32, output_width: u32, output_height: u32) -> ScaleRect {
    ScaleRect {
        x: (output_width - width) / 2,
        y: (output_height - height) / 2,
        width,
        height,
    }
}

pub(crate) fn validate_scaled_rect(output_width: u32, output_height: u32, rect: ScaleRect) -> Result<()> {
    if rect.width == 0 || rect.height == 0 {
        return Err(anyhow!("Scale destination size must be non-zero"));
    }
    if rect.x + rect.width > output_width || rect.y + rect.height > output_height {
        return Err(anyhow!("Scale destination rect exceeds output bounds"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- frame tests --

    #[test]
    fn rgb565_to_rgb_maps_red_correctly() {
        let rgb = rgb565_to_rgb(&[0x00, 0xf8]);
        assert_eq!(rgb, [0xff, 0x00, 0x00]);
    }

    #[test]
    fn validate_frame_rejects_short_pitch() {
        let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);
        let err = validate_frame(&frame, 2).unwrap_err();
        assert!(err.to_string().contains("pitch"));
    }

    // -- scale tests --

    #[test]
    fn native_uses_largest_integer_scale_that_fits() {
        let rect = calculate_scale_rect(ScaleMode::Native, 160, 144, 0.0, 640, 480).unwrap();
        assert_eq!(
            rect,
            ScaleRect {
                x: 80,
                y: 24,
                width: 480,
                height: 432
            }
        );
    }

    #[test]
    fn native_centers_unscaled_frame_when_it_cannot_fit() {
        let rect = calculate_scale_rect(ScaleMode::Native, 800, 600, 0.0, 640, 480).unwrap();
        assert_eq!(
            rect,
            ScaleRect {
                x: 0,
                y: 0,
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn aspect_uses_core_aspect_ratio() {
        let rect = calculate_scale_rect(ScaleMode::Aspect, 256, 224, 4.0 / 3.0, 640, 480).unwrap();
        assert_eq!(
            rect,
            ScaleRect {
                x: 0,
                y: 0,
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn aspect_falls_back_to_source_ratio() {
        let rect = calculate_scale_rect(ScaleMode::Aspect, 160, 144, 0.0, 640, 480).unwrap();
        assert_eq!(
            rect,
            ScaleRect {
                x: 53,
                y: 0,
                width: 533,
                height: 480
            }
        );
    }

    #[test]
    fn fullscreen_fills_output() {
        let rect = calculate_scale_rect(ScaleMode::Fullscreen, 160, 144, 0.0, 640, 480).unwrap();
        assert_eq!(
            rect,
            ScaleRect {
                x: 0,
                y: 0,
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn rejects_zero_source_size() {
        let err = calculate_scale_rect(ScaleMode::Aspect, 0, 144, 0.0, 640, 480).unwrap_err();
        assert_eq!(err.to_string(), "Scale source size must be non-zero");
    }

    #[test]
    fn scale_modes_cycle_in_display_order() {
        assert_eq!(ScaleMode::Native.next(), ScaleMode::Aspect);
        assert_eq!(ScaleMode::Aspect.next(), ScaleMode::Fullscreen);
        assert_eq!(ScaleMode::Fullscreen.next(), ScaleMode::Native);
    }
}

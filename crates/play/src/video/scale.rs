// Fixed-point scaling calculations and scale mode abstractions.

use anyhow::{Result, anyhow};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ScaleMode {
    Native,
    #[default]
    Aspect,
    Cropped,
    Fullscreen,
}

impl std::str::FromStr for ScaleMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "native" => Ok(Self::Native),
            "aspect" => Ok(Self::Aspect),
            "cropped" => Ok(Self::Cropped),
            "fullscreen" => Ok(Self::Fullscreen),
            _ => Err(format!("Unknown scale mode: {}", s)),
        }
    }
}

impl ScaleMode {
    pub fn next(self) -> Self {
        match self {
            Self::Native => Self::Aspect,
            Self::Aspect => Self::Cropped,
            Self::Cropped => Self::Fullscreen,
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
        ScaleMode::Cropped => Ok(scale_cropped(
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
    let scale = (output_width / source_width)
        .min(output_height / source_height)
        .max(1);
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
        (
            output_width,
            ((output_width as f64 / aspect).round() as u32).max(1),
        )
    } else {
        (
            ((output_height as f64 * aspect).round() as u32).max(1),
            output_height,
        )
    };
    center_rect(width, height, output_width, output_height)
}

fn scale_cropped(
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    output_width: u32,
    output_height: u32,
) -> ScaleRect {
    let aspect = get_aspect_ratio(source_width, source_height, aspect_ratio);
    let output_ratio = output_width as f64 / output_height as f64;
    let (width, height) = if aspect > output_ratio {
        // Source is wider than output: scale to fill height, crop sides.
        (output_height as f64 * aspect, output_height as f64)
    } else {
        // Source is narrower than output: scale to fill width, crop top/bottom.
        (output_width as f64, output_width as f64 / aspect)
    };
    let width = width.min(output_width as f64);
    let height = height.min(output_height as f64);
    ScaleRect {
        x: (output_width as f64 - width).max(0.0) as u32 / 2,
        y: (output_height as f64 - height).max(0.0) as u32 / 2,
        width: width as u32,
        height: height as u32,
    }
}

fn center_rect(width: u32, height: u32, output_width: u32, output_height: u32) -> ScaleRect {
    ScaleRect {
        x: (output_width - width) / 2,
        y: (output_height - height) / 2,
        width,
        height,
    }
}

pub fn validate_scaled_rect(output_width: u32, output_height: u32, rect: ScaleRect) -> Result<()> {
    if rect.width == 0 || rect.height == 0 {
        return Err(anyhow!("Scale destination size must be non-zero"));
    }
    if rect.x + rect.width > output_width || rect.y + rect.height > output_height {
        return Err(anyhow!("Scale destination rect exceeds output bounds"));
    }
    Ok(())
}

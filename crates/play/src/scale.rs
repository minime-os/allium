use anyhow::{Result, anyhow};
use clap::ValueEnum;

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

#[cfg(test)]
mod tests {
    use super::*;

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

use anyhow::{Result, anyhow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    Native,
    Aspect,
    Fullscreen,
}

impl ScaleMode {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "native" => Ok(Self::Native),
            "aspect" => Ok(Self::Aspect),
            "fullscreen" => Ok(Self::Fullscreen),
            _ => Err(anyhow!("--scale must be native, aspect, or fullscreen")),
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
    if source_width == 0 || source_height == 0 {
        return Err(anyhow!("Scale source size must be non-zero"));
    }
    if output_width == 0 || output_height == 0 {
        return Err(anyhow!("Scale output size must be non-zero"));
    }

    match mode {
        ScaleMode::Native => {
            let scale = (output_width / source_width).min(output_height / source_height);
            let scale = scale.max(1);
            let width = (source_width * scale).min(output_width);
            let height = (source_height * scale).min(output_height);
            Ok(center_rect(width, height, output_width, output_height))
        }
        ScaleMode::Aspect => {
            let aspect_ratio = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
                aspect_ratio as f64
            } else {
                source_width as f64 / source_height as f64
            };

            let output_ratio = output_width as f64 / output_height as f64;
            let (width, height) = if aspect_ratio > output_ratio {
                let width = output_width;
                let height = (output_width as f64 / aspect_ratio).round() as u32;
                (width, height.max(1))
            } else {
                let height = output_height;
                let width = (output_height as f64 * aspect_ratio).round() as u32;
                (width.max(1), height)
            };
            Ok(center_rect(width, height, output_width, output_height))
        }
        ScaleMode::Fullscreen => Ok(ScaleRect {
            x: 0,
            y: 0,
            width: output_width,
            height: output_height,
        }),
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
}

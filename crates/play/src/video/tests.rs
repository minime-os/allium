// Unit tests for the video module.

use super::*;

// ---- pixel tests ----

#[test]
fn rgb565_to_rgb_maps_red_correctly() {
    let rgb = rgb565_to_rgb(&[0x00, 0xf8]);
    assert_eq!(rgb, [0xff, 0x00, 0x00]);
}

// ---- frame tests ----

#[test]
fn validate_frame_rejects_short_pitch() {
    let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);
    let err = validate_frame(&frame, 2).unwrap_err();
    assert!(err.to_string().contains("pitch"));
}

// ---- scale tests ----

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

// ---- frame timing tests ----

#[test]
fn frame_interval_uses_core_fps() {
    let interval = frame_interval(60.0).unwrap();
    assert_eq!(interval, Duration::from_nanos(16_666_667));
}

#[test]
fn frame_interval_rejects_zero_fps() {
    let err = frame_interval(0.0).unwrap_err();
    assert_eq!(err.to_string(), "Core reported invalid FPS: 0");
}

#[test]
fn frame_interval_rejects_nan_fps() {
    let err = frame_interval(f64::NAN).unwrap_err();
    assert_eq!(err.to_string(), "Core reported invalid FPS: NaN");
}

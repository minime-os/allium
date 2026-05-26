//! Procedural screen effects for integer/native scaling.
//!
//! Ported from MinUI `scaler.c` RGB565 blending formulas.
//! Effects are only applied when scaling at an integer factor (1x–4x).

use crate::settings::ScreenEffect;

/// Blend two RGB565 pixels with weight 3:1 (a gets 75%, b gets 25%).
/// Per-component arithmetic to avoid colour bleeding.
pub fn weight3_1_rgb565(a: u16, b: u16) -> u16 {
    let r = ((3 * ((a >> 11) & 0x1F) + ((b >> 11) & 0x1F)) >> 2) << 11;
    let g = ((3 * ((a >> 5) & 0x3F) + ((b >> 5) & 0x3F)) >> 2) << 5;
    let b_out = (3 * (a & 0x1F) + (b & 0x1F)) >> 2;
    r | g | b_out
}

/// Blend two RGB565 pixels with weight 2:3 (a gets 40%, b gets 60%).
pub fn weight2_3_rgb565(a: u16, b: u16) -> u16 {
    let r = ((2 * ((a >> 11) & 0x1F) + 3 * ((b >> 11) & 0x1F)) / 5) << 11;
    let g = ((2 * ((a >> 5) & 0x3F) + 3 * ((b >> 5) & 0x3F)) / 5) << 5;
    let b_out = (2 * (a & 0x1F) + 3 * (b & 0x1F)) / 5;
    r | g | b_out
}

/// Blend two RGB565 pixels with weight 3:2 (a gets 60%, b gets 40%).
pub fn weight3_2_rgb565(a: u16, b: u16) -> u16 {
    let r = ((3 * ((a >> 11) & 0x1F) + 2 * ((b >> 11) & 0x1F)) / 5) << 11;
    let g = ((3 * ((a >> 5) & 0x3F) + 2 * ((b >> 5) & 0x3F)) / 5) << 5;
    let b_out = (3 * (a & 0x1F) + 2 * (b & 0x1F)) / 5;
    r | g | b_out
}

/// Apply a procedural effect to an RGB565 pixel at a given output position.
/// `scale` is the integer magnification factor (1–4). `black` is 0x0000.
pub fn apply_rgb565_effect(
    pixel: u16,
    effect: ScreenEffect,
    scale: u32,
    dst_x: u32,
    dst_y: u32,
) -> u16 {
    if scale < 1 {
        return pixel;
    }
    let black: u16 = 0x0000;
    match effect {
        ScreenEffect::None => pixel,
        ScreenEffect::Line => {
            // Darken every alternate output row by 25%.
            if dst_y % 2 == 1 {
                weight3_1_rgb565(pixel, black)
            } else {
                pixel
            }
        }
        ScreenEffect::Grid => {
            // Checkerboard-like grid: darken pixels where (dst_x + dst_y) is odd.
            // At 2× this creates a diagonal checkerboard; at 3× a woven pattern.
            if (dst_x + dst_y) % 2 == 1 {
                weight3_1_rgb565(pixel, black)
            } else {
                pixel
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight3_1_halves_red() {
        // Pure red (0xF800) blended 3:1 with black (0x0000) = darker red
        let result = weight3_1_rgb565(0xF800, 0x0000);
        // R should be 0x1F * 3/4 = 0x17 (roughly), others 0
        assert!(result > 0);
        assert!(result < 0xF800);
    }

    #[test]
    fn none_effect_is_identity() {
        assert_eq!(
            apply_rgb565_effect(0xFFFF, ScreenEffect::None, 2, 0, 0),
            0xFFFF
        );
    }

    #[test]
    fn line_darkens_odd_rows() {
        let pixel = 0xFFFF;
        assert_eq!(
            apply_rgb565_effect(pixel, ScreenEffect::Line, 2, 0, 0),
            pixel
        );
        let dimmed = apply_rgb565_effect(pixel, ScreenEffect::Line, 2, 0, 1);
        assert!(dimmed < pixel);
    }

    #[test]
    fn grid_darkens_checkerboard() {
        let pixel = 0xFFFF;
        // (0+0) even → bright
        assert_eq!(
            apply_rgb565_effect(pixel, ScreenEffect::Grid, 2, 0, 0),
            pixel
        );
        // (1+0) odd → dimmed
        let dimmed = apply_rgb565_effect(pixel, ScreenEffect::Grid, 2, 1, 0);
        assert!(dimmed < pixel);
    }
}

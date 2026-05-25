// Pre-computed color-space conversion tables.
// Lookup tables eliminate per-pixel bit-shifting on every frame.

/// 65536-entry LUT that maps every possible RGB565 pixel value to BGRA8888.
/// The conversion uses fast shifts (not slow division) to extend 5/6 bits to 8 bits.
pub const RGB565_TO_BGRA8888: [u32; 65536] = {
    let mut lut = [0u32; 65536];
    let mut i = 0u32;
    while i < 65536 {
        let r = (i >> 11) & 0x1f;
        let g = (i >> 5) & 0x3f;
        let b = i & 0x1f;
        // Replicate high bits into low bits (same as r*255/31 but done with shifts).
        let r8 = (r << 3) | (r >> 2);
        let g8 = (g << 2) | (g >> 4);
        let b8 = (b << 3) | (b >> 2);
        // The existing Rust scaler writes (A<<24)|(R<<16)|(G<<8)|B.
        // On little-endian this stores [B, G, R, A] — i.e. BGRA8888 — matching the
        // framebuffer reported by fbset: rgba 8/16,8/8,8/0,8/24.
        lut[i as usize] = (0xff << 24) | (r8 << 16) | (g8 << 8) | b8;
        i += 1;
    }
    lut
};

/// Inline RGB565 → u32 (BGRA8888) conversion via the pre-computed LUT.
pub fn rgb565_to_bgra8888(pixel: u16) -> u32 {
    RGB565_TO_BGRA8888[pixel as usize]
}

/// Convert a packed RGB565 byte pair to an R, G, B triple (used by HUD font rendering).
pub fn rgb565_to_rgb(bytes: &[u8]) -> [u8; 3] {
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

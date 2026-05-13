use anyhow::{Result, anyhow};

// Keep a copied frame for debug output because libretro owns callback memory.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
}

impl CapturedFrame {
    // The caller may build this from raw callback data; validate at the point of use.
    pub fn new(data: Vec<u8>, width: u32, height: u32, pitch: usize) -> Self {
        Self {
            data,
            width,
            height,
            pitch,
        }
    }
}

const RGB565_BYTES_PER_PIXEL: usize = 2;

// PPM is used here because it needs no encoder: header plus raw RGB bytes.
pub fn encode_rgb565_ppm(frame: &CapturedFrame) -> Result<Vec<u8>> {
    validate_frame(frame)?;

    let mut ppm_data = Vec::with_capacity(ppm_len(frame.width, frame.height));
    ppm_data.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * RGB565_BYTES_PER_PIXEL;
            let [r, g, b] = rgb565_to_rgb(&frame.data[pixel_start..pixel_start + 2]);
            ppm_data.extend_from_slice(&[r, g, b]);
        }
    }

    Ok(ppm_data)
}

// Validate pitch and length first so conversion never indexes past the copied frame.
fn validate_frame(frame: &CapturedFrame) -> Result<()> {
    let row_bytes = frame.width as usize * RGB565_BYTES_PER_PIXEL;
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

// Reserving the exact size avoids reallocating while writing the dump.
fn ppm_len(width: u32, height: u32) -> usize {
    format!("P6\n{} {}\n255\n", width, height).len() + width as usize * height as usize * 3
}

// PPM stores 8-bit RGB channels, while the core gives us packed RGB565 pixels.
fn rgb565_to_rgb(bytes: &[u8]) -> [u8; 3] {
    let pixel = u16::from_le_bytes([bytes[0], bytes[1]]);
    [
        scale_5_to_8((pixel >> 11) & 0x1f),
        scale_6_to_8((pixel >> 5) & 0x3f),
        scale_5_to_8(pixel & 0x1f),
    ]
}

// Scaling keeps white white and black black when moving from 5/6 bits to 8 bits.
fn scale_5_to_8(value: u16) -> u8 {
    (u32::from(value) * 255 / 31) as u8
}

fn scale_6_to_8(value: u16) -> u8 {
    (u32::from(value) * 255 / 63) as u8
}

// These tests protect the easy mistakes: wrong color bits, ignored pitch, unsafe lengths.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_rgb565_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);

        let ppm = encode_rgb565_ppm(&frame).unwrap();

        assert_eq!(ppm, b"P6\n2 1\n255\n\xff\x00\x00\x00\xff\x00");
    }

    #[test]
    fn respects_pitch_padding() {
        let frame = CapturedFrame::new(
            vec![0x00, 0xf8, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
            1,
            2,
            4,
        );

        let ppm = encode_rgb565_ppm(&frame).unwrap();

        assert_eq!(ppm, b"P6\n1 2\n255\n\xff\x00\x00\x00\x00\xff");
    }

    #[test]
    fn rejects_short_rows() {
        let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);

        let err = encode_rgb565_ppm(&frame).unwrap_err();

        assert_eq!(err.to_string(), "Frame pitch 2 is smaller than row size 4");
    }
}

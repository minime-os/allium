use anyhow::Result;

use super::frame::{
    CapturedFrame, RGB565_BYTES_PER_PIXEL, XRGB8888_BYTES_PER_PIXEL, rgb565_to_rgb, validate_frame,
};

pub fn encode_rgb565(frame: &CapturedFrame) -> Result<Vec<u8>> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;

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

pub fn encode_xrgb8888(frame: &CapturedFrame) -> Result<Vec<u8>> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;

    let mut ppm_data = Vec::with_capacity(ppm_len(frame.width, frame.height));
    ppm_data.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * XRGB8888_BYTES_PER_PIXEL;
            let bytes = &frame.data[pixel_start..pixel_start + XRGB8888_BYTES_PER_PIXEL];
            ppm_data.extend_from_slice(&[bytes[2], bytes[1], bytes[0]]);
        }
    }

    Ok(ppm_data)
}

fn ppm_len(width: u32, height: u32) -> usize {
    format!("P6\n{} {}\n255\n", width, height).len() + width as usize * height as usize * 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_rgb565_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);

        let ppm = encode_rgb565(&frame).unwrap();

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

        let ppm = encode_rgb565(&frame).unwrap();

        assert_eq!(ppm, b"P6\n1 2\n255\n\xff\x00\x00\x00\x00\xff");
    }

    #[test]
    fn encodes_xrgb8888_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0x00, 0xff, 0x00], 1, 1, 4);

        let ppm = encode_xrgb8888(&frame).unwrap();

        assert_eq!(ppm, b"P6\n1 1\n255\n\xff\x00\x00");
    }

    #[test]
    fn rejects_short_rows() {
        let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);

        let err = encode_rgb565(&frame).unwrap_err();

        assert_eq!(err.to_string(), "Frame pitch 2 is smaller than row size 4");
    }
}

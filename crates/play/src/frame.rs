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

const XRGB8888_BYTES_PER_PIXEL: usize = 4;
const BGRA8888_BYTES_PER_PIXEL: usize = 4;

pub fn encode_xrgb8888_ppm(frame: &CapturedFrame) -> Result<Vec<u8>> {
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

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
pub fn convert_rgb565_to_xrgb8888(frame: &CapturedFrame, output: &mut [u32]) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_output(frame, output)?;

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        let output_row_start = y * frame.width as usize;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * RGB565_BYTES_PER_PIXEL;
            let [r, g, b] = rgb565_to_rgb(&frame.data[pixel_start..pixel_start + 2]);
            output[output_row_start + x] = pack_xrgb8888(r, g, b);
        }
    }

    Ok(())
}

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
pub fn convert_xrgb8888_to_xrgb8888(frame: &CapturedFrame, output: &mut [u32]) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_output(frame, output)?;

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        let output_row_start = y * frame.width as usize;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * XRGB8888_BYTES_PER_PIXEL;
            let bytes = &frame.data[pixel_start..pixel_start + XRGB8888_BYTES_PER_PIXEL];
            output[output_row_start + x] = pack_xrgb8888(bytes[2], bytes[1], bytes[0]);
        }
    }

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn copy_rgb565_to_rgb565(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_byte_output(frame, output, output_pitch, RGB565_BYTES_PER_PIXEL)?;

    let row_bytes = frame.width as usize * RGB565_BYTES_PER_PIXEL;
    for y in 0..frame.height as usize {
        let source_start = y * frame.pitch;
        let output_start = y * output_pitch;
        output[output_start..output_start + row_bytes]
            .copy_from_slice(&frame.data[source_start..source_start + row_bytes]);
    }

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn copy_rgb565_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
) -> Result<()> {
    validate_frame(frame, RGB565_BYTES_PER_PIXEL)?;
    validate_byte_output(frame, output, output_pitch, BGRA8888_BYTES_PER_PIXEL)?;

    for y in 0..frame.height as usize {
        let row_start = y * frame.pitch;
        let output_row_start = y * output_pitch;
        for x in 0..frame.width as usize {
            let pixel_start = row_start + x * RGB565_BYTES_PER_PIXEL;
            let output_start = output_row_start + x * BGRA8888_BYTES_PER_PIXEL;
            let [r, g, b] = rgb565_to_rgb(&frame.data[pixel_start..pixel_start + 2]);
            output[output_start] = b;
            output[output_start + 1] = g;
            output[output_start + 2] = r;
            output[output_start + 3] = 0xff;
        }
    }

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
pub fn copy_xrgb8888_to_bgra8888(
    frame: &CapturedFrame,
    output: &mut [u8],
    output_pitch: usize,
) -> Result<()> {
    validate_frame(frame, XRGB8888_BYTES_PER_PIXEL)?;
    validate_byte_output(frame, output, output_pitch, BGRA8888_BYTES_PER_PIXEL)?;

    let row_bytes = frame.width as usize * BGRA8888_BYTES_PER_PIXEL;
    for y in 0..frame.height as usize {
        let source_start = y * frame.pitch;
        let output_start = y * output_pitch;
        output[output_start..output_start + row_bytes]
            .copy_from_slice(&frame.data[source_start..source_start + row_bytes]);
        for alpha in (output_start + 3..output_start + row_bytes).step_by(BGRA8888_BYTES_PER_PIXEL)
        {
            output[alpha] = 0xff;
        }
    }

    Ok(())
}

// Validate pitch and length first so conversion never indexes past the copied frame.
fn validate_frame(frame: &CapturedFrame, bytes_per_pixel: usize) -> Result<()> {
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

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
fn validate_output(frame: &CapturedFrame, output: &[u32]) -> Result<()> {
    let expected_len = frame.width as usize * frame.height as usize;
    if output.len() < expected_len {
        return Err(anyhow!(
            "Output buffer has {} pixels, expected at least {}",
            output.len(),
            expected_len
        ));
    }

    Ok(())
}

#[cfg_attr(not(feature = "miyoo"), allow(dead_code))]
fn validate_byte_output(
    frame: &CapturedFrame,
    output: &[u8],
    output_pitch: usize,
    bytes_per_pixel: usize,
) -> Result<()> {
    let row_bytes = frame.width as usize * bytes_per_pixel;
    if output_pitch < row_bytes {
        return Err(anyhow!(
            "Destination pitch {} is smaller than row size {}",
            output_pitch,
            row_bytes
        ));
    }

    let expected_len = output_pitch * frame.height as usize;
    if output.len() < expected_len {
        return Err(anyhow!(
            "Destination buffer has {} bytes, expected at least {}",
            output.len(),
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

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
fn pack_xrgb8888(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
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
    fn encodes_xrgb8888_ppm() {
        let frame = CapturedFrame::new(vec![0x00, 0x00, 0xff, 0x00], 1, 1, 4);

        let ppm = encode_xrgb8888_ppm(&frame).unwrap();

        assert_eq!(ppm, b"P6\n1 1\n255\n\xff\x00\x00");
    }

    #[test]
    fn rejects_short_rows() {
        let frame = CapturedFrame::new(vec![0; 2], 2, 1, 2);

        let err = encode_rgb565_ppm(&frame).unwrap_err();

        assert_eq!(err.to_string(), "Frame pitch 2 is smaller than row size 4");
    }

    #[test]
    fn converts_rgb565_to_softbuffer_pixels() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);
        let mut output = vec![0; 2];

        convert_rgb565_to_xrgb8888(&frame, &mut output).unwrap();

        assert_eq!(output, vec![0x00ff0000, 0x0000ff00]);
    }

    #[test]
    fn converts_xrgb8888_to_softbuffer_pixels() {
        let frame = CapturedFrame::new(
            vec![0x00, 0x00, 0xff, 0x00, 0x00, 0xff, 0x00, 0x00],
            2,
            1,
            8,
        );
        let mut output = vec![0; 2];

        convert_xrgb8888_to_xrgb8888(&frame, &mut output).unwrap();

        assert_eq!(output, vec![0x00ff0000, 0x0000ff00]);
    }

    #[test]
    fn conversion_respects_source_pitch() {
        let frame = CapturedFrame::new(
            vec![0x00, 0xf8, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
            1,
            2,
            4,
        );
        let mut output = vec![0; 2];

        convert_rgb565_to_xrgb8888(&frame, &mut output).unwrap();

        assert_eq!(output, vec![0x00ff0000, 0x000000ff]);
    }

    #[test]
    fn conversion_rejects_short_output_buffer() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);
        let mut output = vec![0; 1];

        let err = convert_rgb565_to_xrgb8888(&frame, &mut output).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Output buffer has 1 pixels, expected at least 2"
        );
    }

    #[test]
    fn copies_rgb565_to_framebuffer_with_destination_pitch() {
        let frame = CapturedFrame::new(
            vec![0x00, 0xf8, 0x00, 0x00, 0xe0, 0x07, 0x00, 0x00],
            1,
            2,
            4,
        );
        let mut output = vec![0; 8];

        copy_rgb565_to_rgb565(&frame, &mut output, 4).unwrap();

        assert_eq!(output, vec![0x00, 0xf8, 0x00, 0x00, 0xe0, 0x07, 0x00, 0x00]);
    }

    #[test]
    fn framebuffer_copy_rejects_short_destination_pitch() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);
        let mut output = vec![0; 2];

        let err = copy_rgb565_to_rgb565(&frame, &mut output, 2).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Destination pitch 2 is smaller than row size 4"
        );
    }

    #[test]
    fn converts_rgb565_to_bgra8888_framebuffer() {
        let frame = CapturedFrame::new(vec![0x00, 0xf8, 0xe0, 0x07], 2, 1, 4);
        let mut output = vec![0; 8];

        copy_rgb565_to_bgra8888(&frame, &mut output, 8).unwrap();

        assert_eq!(output, vec![0x00, 0x00, 0xff, 0xff, 0x00, 0xff, 0x00, 0xff]);
    }

    #[test]
    fn copies_xrgb8888_to_bgra8888_framebuffer_with_pitch() {
        let frame = CapturedFrame::new(
            vec![0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0x00, 0x00],
            1,
            2,
            4,
        );
        let mut output = vec![0; 16];

        copy_xrgb8888_to_bgra8888(&frame, &mut output, 8).unwrap();

        assert_eq!(
            output,
            vec![
                0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0xff, 0x00, 0x00,
                0x00, 0x00
            ]
        );
    }
}

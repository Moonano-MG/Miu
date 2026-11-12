//! BMP decoder for uncompressed 24- and 32-bit bitmaps.
//!
//! Only `BITMAPINFOHEADER`-family files with `BI_RGB` compression are accepted,
//! which covers every bitmap produced by the usual art tools. Negative heights
//! (top-down rows) and the standard bottom-up layout are both handled.

use miu_core::Rgba8;

use super::{DecodedImage, ImageError};

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ImageError> {
    let slice = bytes.get(offset..offset + 2).ok_or(ImageError::Truncated)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ImageError> {
    let slice = bytes.get(offset..offset + 4).ok_or(ImageError::Truncated)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Decodes a BMP file.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.len() < 54 || &bytes[..2] != b"BM" {
        return Err(ImageError::Unsupported("not a bmp"));
    }
    let pixel_offset = read_u32(bytes, 10)? as usize;
    let dib_size = read_u32(bytes, 14)?;
    if dib_size < 40 {
        return Err(ImageError::Unsupported("bmp dib header variant"));
    }
    let width = read_u32(bytes, 18)? as i32;
    let raw_height = read_u32(bytes, 22)? as i32;
    let planes = read_u16(bytes, 26)?;
    let bits_per_pixel = read_u16(bytes, 28)?;
    let compression = read_u32(bytes, 30)?;
    if planes != 1 {
        return Err(ImageError::Corrupt("bmp plane count"));
    }
    if compression != 0 {
        return Err(ImageError::Unsupported("compressed bmp"));
    }
    if bits_per_pixel != 24 && bits_per_pixel != 32 {
        return Err(ImageError::Unsupported("bmp bit depth"));
    }
    if width <= 0 || raw_height == 0 {
        return Err(ImageError::Corrupt("bmp has no area"));
    }
    let top_down = raw_height < 0;
    let height = raw_height.unsigned_abs();
    let bytes_per_pixel = (bits_per_pixel / 8) as usize;
    let row_stride = ((width as usize * bits_per_pixel as usize + 31) / 32) * 4;
    let required = pixel_offset
        .checked_add(row_stride * height as usize)
        .ok_or(ImageError::TooLarge)?;
    if bytes.len() < required {
        return Err(ImageError::Truncated);
    }

    let mut pixels = vec![Rgba8::TRANSPARENT; width as usize * height as usize];
    for row in 0..height as usize {
        let source_row = if top_down { row } else { height as usize - 1 - row };
        let row_start = pixel_offset + source_row * row_stride;
        for column in 0..width as usize {
            let offset = row_start + column * bytes_per_pixel;
            let blue = bytes[offset];
            let green = bytes[offset + 1];
            let red = bytes[offset + 2];
            let alpha = if bytes_per_pixel == 4 { bytes[offset + 3] } else { 255 };
            let alpha = if alpha == 0 && bytes_per_pixel == 4 { 255 } else { alpha };
            pixels[row * width as usize + column] = Rgba8::new(red, green, blue, alpha);
        }
    }
    DecodedImage::new(width as u32, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_one_bmp() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&70u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&2i32.to_le_bytes());
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&24u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // Two pixels (blue, green, red) plus two bytes of row padding.
        bytes.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        bytes
    }

    #[test]
    fn decodes_24_bit_rows_in_bgr_order() {
        let image = decode(&two_by_one_bmp()).expect("valid bitmap");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels[0], Rgba8::new(255, 0, 0, 255));
        assert_eq!(image.pixels[1], Rgba8::new(0, 255, 0, 255));
    }

    #[test]
    fn rejects_unsupported_variants() {
        assert!(decode(b"BM").is_err());
    }
}

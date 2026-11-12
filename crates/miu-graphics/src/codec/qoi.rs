//! QOI decoder ("Quite Ok Image" format).
//!
//! Implements the reference specification: 14-byte header, index/difference/luma/run
//! opcodes and RGB/RGBA literal opcodes. QOI is useful for engine tests and tooling
//! because it decodes in a single pass with no entropy coding.

use miu_core::Rgba8;

use super::{DecodedImage, ImageError};

const HEADER_SIZE: usize = 14;
const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

const HASH_TABLE: usize = 64;

fn hash(pixel: Rgba8) -> usize {
    let value = pixel.r as usize * 3
        + pixel.g as usize * 5
        + pixel.b as usize * 7
        + pixel.a as usize * 11;
    value % HASH_TABLE
}

/// Decodes a QOI file.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.len() < HEADER_SIZE + END_MARKER.len() {
        return Err(ImageError::Truncated);
    }
    if &bytes[..4] != b"qoif" {
        return Err(ImageError::Unsupported("not a qoi file"));
    }
    let width = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let height = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let channels = bytes[12];
    if width == 0 || height == 0 {
        return Err(ImageError::Corrupt("qoi has no area"));
    }
    if channels != 3 && channels != 4 {
        return Err(ImageError::Corrupt("qoi channel count"));
    }
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageError::TooLarge)?;
    let mut pixels = Vec::with_capacity(pixel_count);
    let mut index = [[0u8; 4]; HASH_TABLE];
    let mut pixel = Rgba8::new(0, 0, 0, 255);
    let mut position = HEADER_SIZE;
    let mut run = 0usize;

    while pixels.len() < pixel_count {
        if run > 0 {
            run -= 1;
        } else {
            let byte = *bytes.get(position).ok_or(ImageError::Truncated)?;
            position += 1;
            match byte {
                0xFE => {
                    let slice = bytes.get(position..position + 3).ok_or(ImageError::Truncated)?;
                    pixel = Rgba8::new(slice[0], slice[1], slice[2], pixel.a);
                    position += 3;
                }
                0xFF => {
                    let slice = bytes.get(position..position + 4).ok_or(ImageError::Truncated)?;
                    pixel = Rgba8::new(slice[0], slice[1], slice[2], slice[3]);
                    position += 4;
                }
                _ => match byte >> 6 {
                    0 => {
                        let entry = index[(byte & 0x3F) as usize];
                        pixel = Rgba8::new(entry[0], entry[1], entry[2], entry[3]);
                    }
                    1 => {
                        let red = ((byte >> 4) & 0x03) as i16 - 2;
                        let green = ((byte >> 2) & 0x03) as i16 - 2;
                        let blue = (byte & 0x03) as i16 - 2;
                        pixel = Rgba8::new(
                            pixel.r.wrapping_add(red as u8),
                            pixel.g.wrapping_add(green as u8),
                            pixel.b.wrapping_add(blue as u8),
                            pixel.a,
                        );
                    }
                    2 => {
                        let second = *bytes.get(position).ok_or(ImageError::Truncated)?;
                        position += 1;
                        let delta_green = (byte & 0x3F) as i16 - 32;
                        let green = (pixel.g as i16 + delta_green) as u8;
                        let red = ((second >> 4) & 0x0F) as i16 - 8;
                        let blue = (second & 0x0F) as i16 - 8;
                        pixel = Rgba8::new(
                            (green as i16).wrapping_add(red) as u8,
                            green,
                            (green as i16).wrapping_add(blue) as u8,
                            pixel.a,
                        );
                    }
                    _ => {
                        run = (byte & 0x3F) as usize;
                    }
                },
            }
            let slot = hash(pixel);
            index[slot] = [pixel.r, pixel.g, pixel.b, pixel.a];
        }
        pixels.push(pixel);
    }
    DecodedImage::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"qoif".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.push(4);
        bytes.push(0);
        bytes
    }

    #[test]
    fn decodes_rgba_literal_opcodes() {
        let mut bytes = header(2, 1);
        bytes.push(0xFF);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.push(0xFE);
        bytes.extend_from_slice(&[5, 6, 7]);
        bytes.extend_from_slice(&END_MARKER);
        let image = decode(&bytes).expect("valid qoi");
        assert_eq!(image.pixels[0], Rgba8::new(1, 2, 3, 4));
        assert_eq!(image.pixels[1], Rgba8::new(5, 6, 7, 4));
    }

    #[test]
    fn decodes_run_opcodes() {
        let mut bytes = header(3, 1);
        bytes.push(0xFF);
        bytes.extend_from_slice(&[9, 9, 9, 255]);
        bytes.push(0xC1); // repeat twice
        bytes.extend_from_slice(&END_MARKER);
        let image = decode(&bytes).expect("valid qoi");
        assert_eq!(image.pixels.len(), 3);
        assert_eq!(image.pixels[2], Rgba8::new(9, 9, 9, 255));
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(decode(b"qoif"), Err(ImageError::Truncated));
    }
}

//! TGA decoder for uncompressed and run-length encoded true-colour images.
//!
//! TGA has no signature, so [`looks_like_tga`] performs the plausibility check used
//! by [`super::decode_image`]. Both bottom-up (the historical default) and top-down
//! (`descriptor` bit 5) origins are supported, as are 8-bit greyscale and 24/32-bit
//! colour images.

use miu_core::Rgba8;

use super::{DecodedImage, ImageError};

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ImageError> {
    let slice = bytes.get(offset..offset + 2).ok_or(ImageError::Truncated)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

/// Whether the header fields form a TGA image this decoder can handle.
pub fn looks_like_tga(bytes: &[u8]) -> bool {
    if bytes.len() < 18 {
        return false;
    }
    let color_map_type = bytes[1];
    let image_type = bytes[2];
    let bits_per_pixel = bytes[16];
    let supported_type = matches!(image_type, 2 | 3 | 10 | 11);
    let supported_depth = matches!(bits_per_pixel, 8 | 24 | 32);
    let width = u16::from_le_bytes([bytes[12], bytes[13]]);
    let height = u16::from_le_bytes([bytes[14], bytes[15]]);
    color_map_type <= 1 && supported_type && supported_depth && width > 0 && height > 0
}

/// Decodes a TGA file.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if !looks_like_tga(bytes) {
        return Err(ImageError::Unsupported("not a supported tga"));
    }
    let id_length = bytes[0] as usize;
    let color_map_type = bytes[1];
    let image_type = bytes[2];
    let width = read_u16(bytes, 12)? as usize;
    let height = read_u16(bytes, 14)? as usize;
    let bits_per_pixel = bytes[16] as usize;
    let descriptor = bytes[17];
    if color_map_type != 0 {
        return Err(ImageError::Unsupported("colour-mapped tga"));
    }
    let color_map_length = read_u16(bytes, 5)? as usize;
    let color_map_entry = bytes[7] as usize;
    let mut position = 18 + id_length + color_map_length * ((color_map_entry + 7) / 8);
    if position > bytes.len() {
        return Err(ImageError::Truncated);
    }

    let bytes_per_pixel = bits_per_pixel / 8;
    let top_down = descriptor & 0x20 != 0;
    let right_to_left = descriptor & 0x10 != 0;
    let pixel_count = width * height;
    let mut pixels = vec![Rgba8::TRANSPARENT; pixel_count];
    let mut index = 0usize;
    let read_pixel = |data: &[u8], offset: usize| -> Result<Rgba8, ImageError> {
        let slice = data.get(offset..offset + bytes_per_pixel).ok_or(ImageError::Truncated)?;
        Ok(match bytes_per_pixel {
            1 => Rgba8::new(slice[0], slice[0], slice[0], 255),
            3 => Rgba8::new(slice[2], slice[1], slice[0], 255),
            _ => Rgba8::new(slice[2], slice[1], slice[0], slice[3]),
        })
    };

    match image_type {
        2 | 3 => {
            while index < pixel_count {
                pixels[index] = read_pixel(bytes, position)?;
                position += bytes_per_pixel;
                index += 1;
            }
        }
        10 | 11 => {
            while index < pixel_count {
                let packet = *bytes.get(position).ok_or(ImageError::Truncated)?;
                position += 1;
                let count = (packet & 0x7F) as usize + 1;
                if packet & 0x80 != 0 {
                    let pixel = read_pixel(bytes, position)?;
                    position += bytes_per_pixel;
                    for _ in 0..count {
                        if index >= pixel_count {
                            return Err(ImageError::Corrupt("tga rle overrun"));
                        }
                        pixels[index] = pixel;
                        index += 1;
                    }
                } else {
                    for _ in 0..count {
                        if index >= pixel_count {
                            return Err(ImageError::Corrupt("tga raw overrun"));
                        }
                        pixels[index] = read_pixel(bytes, position)?;
                        position += bytes_per_pixel;
                        index += 1;
                    }
                }
            }
        }
        _ => return Err(ImageError::Unsupported("tga image type")),
    }

    // Reorder rows and columns to the engine's top-left origin convention.
    let mut ordered = vec![Rgba8::TRANSPARENT; pixel_count];
    for row in 0..height {
        for column in 0..width {
            let source_row = if top_down { row } else { height - 1 - row };
            let source_column = if right_to_left { width - 1 - column } else { column };
            ordered[row * width + column] = pixels[source_row * width + source_column];
        }
    }

    DecodedImage::new(width as u32, height as u32, ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tga_two_pixels(rle: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[2] = if rle { 10 } else { 2 };
        bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
        bytes[16] = 24;
        bytes[17] = 0x20; // top-down
        if rle {
            bytes.push(0x81); // one run of two pixels
            bytes.extend_from_slice(&[0, 0, 255]);
        } else {
            bytes.extend_from_slice(&[0, 0, 255, 0, 255, 0]);
        }
        bytes
    }

    #[test]
    fn decodes_uncompressed_truecolour() {
        let image = decode(&tga_two_pixels(false)).expect("valid tga");
        assert_eq!(image.pixels[0], Rgba8::new(255, 0, 0, 255));
        assert_eq!(image.pixels[1], Rgba8::new(0, 255, 0, 255));
    }

    #[test]
    fn decodes_rle_truecolour() {
        let image = decode(&tga_two_pixels(true)).expect("valid rle tga");
        assert_eq!(image.pixels[0], Rgba8::new(255, 0, 0, 255));
        assert_eq!(image.pixels[1], Rgba8::new(255, 0, 0, 255));
    }

    #[test]
    fn signature_heuristic_rejects_junk() {
        assert!(!looks_like_tga(&[0u8; 18]));
        assert!(!looks_like_tga(b"BM"));
    }
}

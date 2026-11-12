//! PNG decoder and encoder.
//!
//! The decoder accepts non-interlaced images with 8- or 16-bit samples and colour
//! types 0 (grey), 2 (RGB), 3 (palette), 4 (grey + alpha), and 6 (RGBA). Chunk CRCs
//! are verified, filters 0-4 are reconstructed, and palette `tRNS` entries become
//! alpha values.
//!
//! The encoder always writes 8-bit RGBA with adaptive per-row filters and zlib
//! compression, which is the format the engine's baking tools and CI snapshots use.

use miu_core::Rgba8;

use super::checksum::crc32;
use super::deflate::deflate_zlib;
use super::inflate::inflate_zlib;
use super::{DecodedImage, ImageError};
use crate::texture::Texture;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, Clone, Copy)]
struct Header {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ImageError> {
    let slice = bytes.get(offset..offset + 4).ok_or(ImageError::Truncated)?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn parse_header(data: &[u8]) -> Result<Header, ImageError> {
    if data.len() < 13 {
        return Err(ImageError::Truncated);
    }
    let header = Header {
        width: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
        height: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        bit_depth: data[8],
        color_type: data[9],
        interlace: data[12],
    };
    if header.width == 0 || header.height == 0 {
        return Err(ImageError::Corrupt("png has no area"));
    }
    if data[10] != 0 || data[11] != 0 {
        return Err(ImageError::Unsupported("png compression or filter method"));
    }
    Ok(header)
}

fn channels_for(color_type: u8) -> Result<usize, ImageError> {
    match color_type {
        0 | 3 => Ok(1),
        2 => Ok(3),
        4 => Ok(2),
        6 => Ok(4),
        _ => Err(ImageError::Unsupported("png colour type")),
    }
}

fn paeth(left: u8, up: u8, corner: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let corner = i32::from(corner);
    let estimate = left + up - corner;
    let distance_left = (estimate - left).abs();
    let distance_up = (estimate - up).abs();
    let distance_corner = (estimate - corner).abs();
    if distance_left <= distance_up && distance_left <= distance_corner {
        left as u8
    } else if distance_up <= distance_corner {
        up as u8
    } else {
        corner as u8
    }
}

fn unfilter(
    filtered: &[u8],
    stride: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>, ImageError> {
    let required = (stride + 1)
        .checked_mul(height)
        .ok_or(ImageError::TooLarge)?;
    if filtered.len() < required {
        return Err(ImageError::Truncated);
    }
    let mut output = vec![0u8; stride * height];
    let filter_stride = stride + 1;
    for row in 0..height {
        let filter = filtered[row * filter_stride];
        let source = &filtered[row * filter_stride + 1..row * filter_stride + 1 + stride];
        let destination_start = row * stride;
        for column in 0..stride {
            let left = if column >= bytes_per_pixel {
                output[destination_start + column - bytes_per_pixel]
            } else {
                0
            };
            let up = if row > 0 {
                output[destination_start - stride + column]
            } else {
                0
            };
            let corner = if row > 0 && column >= bytes_per_pixel {
                output[destination_start - stride + column - bytes_per_pixel]
            } else {
                0
            };
            let value = source[column];
            let reconstructed = match filter {
                0 => value,
                1 => value.wrapping_add(left),
                2 => value.wrapping_add(up),
                3 => value.wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => value.wrapping_add(paeth(left, up, corner)),
                _ => return Err(ImageError::Corrupt("unknown png filter")),
            };
            output[destination_start + column] = reconstructed;
        }
    }
    Ok(output)
}

/// Decodes a PNG file.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.len() < SIGNATURE.len() || bytes[..8] != SIGNATURE {
        return Err(ImageError::Unsupported("not a png"));
    }
    let mut position = SIGNATURE.len();
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut transparency: Vec<u8> = Vec::new();
    let mut idat: Vec<u8> = Vec::new();

    while position + 8 <= bytes.len() {
        let length = read_u32(bytes, position)? as usize;
        let kind = &bytes[position + 4..position + 8];
        let data_start = position + 8;
        let data_end = data_start.checked_add(length).ok_or(ImageError::Truncated)?;
        if data_end + 4 > bytes.len() {
            return Err(ImageError::Truncated);
        }
        let data = &bytes[data_start..data_end];
        let expected_crc = read_u32(bytes, data_end)?;
        if crc32(&bytes[position + 4..data_end]) != expected_crc {
            return Err(ImageError::Corrupt("png chunk crc"));
        }
        match kind {
            b"IHDR" => header = Some(parse_header(data)?),
            b"PLTE" => {
                if data.len() % 3 != 0 {
                    return Err(ImageError::Corrupt("png palette length"));
                }
                palette = data
                    .chunks_exact(3)
                    .map(|entry| [entry[0], entry[1], entry[2], 255])
                    .collect();
            }
            b"tRNS" => transparency = data.to_vec(),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        position = data_end + 4;
    }

    let header = header.ok_or(ImageError::Corrupt("png is missing IHDR"))?;
    if header.interlace != 0 {
        return Err(ImageError::Unsupported("interlaced png"));
    }
    if header.bit_depth != 8 && header.bit_depth != 16 {
        return Err(ImageError::Unsupported("png bit depth"));
    }
    if header.color_type == 3 {
        if palette.is_empty() {
            return Err(ImageError::Corrupt("palette png without PLTE"));
        }
        for (index, alpha) in transparency.iter().enumerate() {
            if let Some(entry) = palette.get_mut(index) {
                entry[3] = *alpha;
            }
        }
    }

    let channels = channels_for(header.color_type)?;
    let bits_per_pixel = channels * header.bit_depth as usize;
    let bytes_per_pixel = ((bits_per_pixel + 7) / 8).max(1);
    let stride = (header.width as usize * bits_per_pixel + 7) / 8;
    let height = header.height as usize;
    let filtered = inflate_zlib(&idat)?;
    let samples = unfilter(&filtered, stride, height, bytes_per_pixel)?;
    let sample_bytes = if header.bit_depth == 16 { 2 } else { 1 };

    // 16-bit samples store the most significant byte first, and 8-bit output keeps
    // that byte, so both depths are read through the same `sample_bytes` stride.
    let mut pixels = Vec::with_capacity(header.width as usize * height);
    let transparent_gray = transparency.first().copied();
    let transparent_rgb = if transparency.len() >= 3 && header.color_type == 2 {
        Some((transparency[0], transparency[1], transparency[2]))
    } else {
        None
    };

    for row in 0..height {
        let row_start = row * stride;
        for column in 0..header.width as usize {
            let pixel = match header.color_type {
                0 => {
                    let gray = samples
                        .get(row_start + column * sample_bytes)
                        .copied()
                        .unwrap_or(0);
                    let alpha = if transparent_gray == Some(gray) { 0 } else { 255 };
                    Rgba8::new(gray, gray, gray, alpha)
                }
                2 => {
                    let offset = row_start + column * 3 * sample_bytes;
                    let red = samples.get(offset).copied().unwrap_or(0);
                    let green = samples.get(offset + sample_bytes).copied().unwrap_or(0);
                    let blue = samples.get(offset + 2 * sample_bytes).copied().unwrap_or(0);
                    let alpha = match transparent_rgb {
                        Some((tr, tg, tb)) if (tr, tg, tb) == (red, green, blue) => 0,
                        _ => 255,
                    };
                    Rgba8::new(red, green, blue, alpha)
                }
                3 => {
                    let index = samples.get(row_start + column).copied().unwrap_or(0) as usize;
                    let entry = palette.get(index).copied().unwrap_or([0, 0, 0, 255]);
                    Rgba8::new(entry[0], entry[1], entry[2], entry[3])
                }
                4 => {
                    let offset = row_start + column * 2 * sample_bytes;
                    let gray = samples.get(offset).copied().unwrap_or(0);
                    let alpha = samples.get(offset + sample_bytes).copied().unwrap_or(255);
                    Rgba8::new(gray, gray, gray, alpha)
                }
                _ => {
                    let offset = row_start + column * 4 * sample_bytes;
                    Rgba8::new(
                        samples.get(offset).copied().unwrap_or(0),
                        samples.get(offset + sample_bytes).copied().unwrap_or(0),
                        samples.get(offset + 2 * sample_bytes).copied().unwrap_or(0),
                        samples.get(offset + 3 * sample_bytes).copied().unwrap_or(255),
                    )
                }
            };
            pixels.push(pixel);
        }
    }
    DecodedImage::new(header.width, header.height, pixels)
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    output.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn filter_row(
    filter: u8,
    row: &[u8],
    previous: &[u8],
    bytes_per_pixel: usize,
    output: &mut Vec<u8>,
) {
    output.push(filter);
    for index in 0..row.len() {
        let left = if index >= bytes_per_pixel { row[index - bytes_per_pixel] } else { 0 };
        let up = previous.get(index).copied().unwrap_or(0);
        let corner = if index >= bytes_per_pixel {
            previous.get(index - bytes_per_pixel).copied().unwrap_or(0)
        } else {
            0
        };
        let value = row[index];
        let filtered = match filter {
            0 => value,
            1 => value.wrapping_sub(left),
            2 => value.wrapping_sub(up),
            3 => value.wrapping_sub(((u16::from(left) + u16::from(up)) / 2) as u8),
            _ => value.wrapping_sub(paeth(left, up, corner)),
        };
        output.push(filtered);
    }
}

fn row_cost(row: &[u8]) -> u64 {
    // Absolute value of each signed byte, which is the heuristic PNG encoders use.
    row.iter()
        .map(|byte| u64::from((*byte as i8).unsigned_abs()))
        .sum()
}

/// Encodes a texture as a PNG file.
pub fn encode(texture: &Texture) -> Result<Vec<u8>, ImageError> {
    let width = texture.width();
    let height = texture.height();
    if width == 0 || height == 0 {
        return Err(ImageError::Encode("empty texture"));
    }
    let rgba = texture.to_rgba8_bytes();
    let stride = width as usize * 4;
    let mut filtered = Vec::with_capacity((stride + 1) * height as usize);
    let mut previous = vec![0u8; stride];
    let mut candidate = Vec::with_capacity(stride + 1);
    let mut best = Vec::with_capacity(stride + 1);
    for row in 0..height as usize {
        let source = &rgba[row * stride..(row + 1) * stride];
        let mut best_filter = 0u8;
        let mut best_cost = u64::MAX;
        best.clear();
        for filter in 0..=4u8 {
            candidate.clear();
            filter_row(filter, source, &previous, 4, &mut candidate);
            let cost = row_cost(&candidate[1..]);
            if cost < best_cost {
                best_cost = cost;
                best_filter = filter;
                best.clear();
                best.extend_from_slice(&candidate[1..]);
            }
        }
        filtered.push(best_filter);
        filtered.extend_from_slice(&best);
        previous.copy_from_slice(source);
    }

    let mut output = Vec::new();
    output.extend_from_slice(&SIGNATURE);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(6);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut output, b"IHDR", &ihdr);
    write_chunk(&mut output, b"IDAT", &deflate_zlib(&filtered));
    write_chunk(&mut output, b"IEND", &[]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let texture = Texture::solid(3, 2, Rgba8::new(200, 100, 50, 255)).unwrap();
        let png = encode(&texture).unwrap();
        let decoded = decode(&png).expect("round trip decodes");
        assert_eq!((decoded.width, decoded.height), (3, 2));
        assert_eq!(decoded.pixels[0], Rgba8::new(200, 100, 50, 255));
        assert_eq!(decoded.pixels.len(), 6);
    }

    #[test]
    fn oversized_or_corrupt_chunks_are_rejected() {
        let mut png = SIGNATURE.to_vec();
        png.extend_from_slice(&[0, 0, 0, 13]);
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        let error = decode(&png).expect_err("bad crc must be rejected");
        assert!(matches!(error, ImageError::Corrupt(_)));
    }

    #[test]
    fn interlaced_png_is_reported_as_unsupported() {
        assert_eq!(
            super::channels_for(7),
            Err(ImageError::Unsupported("png colour type"))
        );
    }
}

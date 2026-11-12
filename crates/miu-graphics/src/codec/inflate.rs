//! DEFLATE (RFC 1951) and zlib (RFC 1950) decoder.
//!
//! The implementation follows the classic `puff` structure: canonical Huffman
//! tables are decoded bit-by-bit from the LSB-first bit stream, so no large lookup
//! tables are allocated. It is used by the PNG decoder and by any embedded asset
//! that stores a zlib stream.

use crate::codec::ImageError;

/// Hard ceiling on the decompressed size, guarding against decompression bombs.
const MAX_OUTPUT_BYTES: usize = 512 * 1024 * 1024;

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0, bit: 0 }
    }

    fn read_bit(&mut self) -> Result<u32, ImageError> {
        if self.position >= self.data.len() {
            return Err(ImageError::Truncated);
        }
        let value = (self.data[self.position] >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.position += 1;
        }
        Ok(u32::from(value))
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, ImageError> {
        let mut value = 0u32;
        for index in 0..count {
            value |= self.read_bit()? << index;
        }
        Ok(value)
    }

    fn align_to_byte(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.position += 1;
        }
    }

    fn read_aligned_byte(&mut self) -> Result<u8, ImageError> {
        if self.position >= self.data.len() {
            return Err(ImageError::Truncated);
        }
        let value = self.data[self.position];
        self.position += 1;
        Ok(value)
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, ImageError> {
        let mut counts = [0u16; 16];
        for length in lengths {
            counts[*length as usize] = counts[*length as usize].saturating_add(1);
        }
        if counts[0] as usize > lengths.len() {
            return Err(ImageError::Corrupt("invalid Huffman lengths"));
        }
        let mut offsets = [0u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + counts[length];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, length) in lengths.iter().enumerate() {
            if *length != 0 {
                let index = offsets[*length as usize] as usize;
                if index >= symbols.len() {
                    return Err(ImageError::Corrupt("Huffman symbol overflow"));
                }
                symbols[index] = symbol as u16;
                offsets[*length as usize] = offsets[*length as usize].saturating_add(1);
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ImageError> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for length in 1..=15usize {
            code |= reader.read_bit()? as i32;
            let count = i32::from(self.counts[length]);
            if code - first < count {
                let position = (index + (code - first)) as usize;
                return self
                    .symbols
                    .get(position)
                    .copied()
                    .ok_or(ImageError::Corrupt("Huffman symbol out of range"));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(ImageError::Corrupt("invalid Huffman code"))
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];

const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

fn fixed_literal_table() -> Result<Huffman, ImageError> {
    let mut lengths = [0u8; 288];
    for (symbol, length) in lengths.iter_mut().enumerate() {
        *length = match symbol {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Huffman::new(&lengths)
}

fn dynamic_tables(reader: &mut BitReader<'_>) -> Result<(Huffman, Huffman), ImageError> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    if hlit > 288 || hdist > 30 {
        return Err(ImageError::Corrupt("dynamic table overflow"));
    }
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let mut code_lengths = [0u8; 19];
    for index in 0..hclen {
        code_lengths[ORDER[index]] = reader.read_bits(3)? as u8;
    }
    let code_table = Huffman::new(&code_lengths)?;

    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut index = 0usize;
    while index < total {
        let symbol = code_table.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[index] = symbol as u8;
                index += 1;
            }
            16 => {
                if index == 0 {
                    return Err(ImageError::Corrupt("repeat with no previous length"));
                }
                let previous = lengths[index - 1];
                let repeat = reader.read_bits(2)? as usize + 3;
                for _ in 0..repeat {
                    if index >= total {
                        return Err(ImageError::Corrupt("repeat overruns table"));
                    }
                    lengths[index] = previous;
                    index += 1;
                }
            }
            17 | 18 => {
                let repeat = if symbol == 17 {
                    reader.read_bits(3)? as usize + 3
                } else {
                    reader.read_bits(7)? as usize + 11
                };
                for _ in 0..repeat {
                    if index >= total {
                        return Err(ImageError::Corrupt("zero repeat overruns table"));
                    }
                    lengths[index] = 0;
                    index += 1;
                }
            }
            _ => return Err(ImageError::Corrupt("invalid code-length symbol")),
        }
    }
    let literal = Huffman::new(&lengths[..hlit])?;
    let distance = Huffman::new(&lengths[hlit..])?;
    Ok((literal, distance))
}

fn inflate_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    literal: &Huffman,
    distance: &Huffman,
) -> Result<(), ImageError> {
    loop {
        let symbol = literal.decode(reader)?;
        match symbol {
            0..=255 => {
                if output.len() >= MAX_OUTPUT_BYTES {
                    return Err(ImageError::TooLarge);
                }
                output.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = symbol as usize - 257;
                let length = u32::from(LENGTH_BASE[index]) + reader.read_bits(u32::from(LENGTH_EXTRA[index]))?;
                let distance_symbol = distance.decode(reader)? as usize;
                if distance_symbol >= DISTANCE_BASE.len() {
                    return Err(ImageError::Corrupt("invalid distance symbol"));
                }
                let offset = u32::from(DISTANCE_BASE[distance_symbol])
                    + reader.read_bits(u32::from(DISTANCE_EXTRA[distance_symbol]))?;
                if offset == 0 || offset as usize > output.len() {
                    return Err(ImageError::Corrupt("distance exceeds output"));
                }
                let start = output.len() - offset as usize;
                for index in 0..length as usize {
                    let byte = output[start + index];
                    if output.len() >= MAX_OUTPUT_BYTES {
                        return Err(ImageError::TooLarge);
                    }
                    output.push(byte);
                }
            }
            _ => return Err(ImageError::Corrupt("invalid literal symbol")),
        }
    }
}

/// Decompresses a zlib stream (2-byte header, DEFLATE data, Adler-32 trailer).
pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, ImageError> {
    if data.len() < 6 {
        return Err(ImageError::Truncated);
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err(ImageError::Unsupported("zlib compression method"));
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err(ImageError::Corrupt("zlib header checksum"));
    }
    if flg & 0x20 != 0 {
        return Err(ImageError::Unsupported("zlib preset dictionary"));
    }
    let mut reader = BitReader::new(&data[2..]);
    let mut output = Vec::new();
    loop {
        let final_block = reader.read_bits(1)? == 1;
        let block_type = reader.read_bits(2)?;
        match block_type {
            0 => {
                reader.align_to_byte();
                let length = u16::from(reader.read_aligned_byte()?)
                    | (u16::from(reader.read_aligned_byte()?) << 8);
                let inverse = u16::from(reader.read_aligned_byte()?)
                    | (u16::from(reader.read_aligned_byte()?) << 8);
                if length != !inverse {
                    return Err(ImageError::Corrupt("stored block length mismatch"));
                }
                for _ in 0..length {
                    let byte = reader.read_aligned_byte()?;
                    if output.len() >= MAX_OUTPUT_BYTES {
                        return Err(ImageError::TooLarge);
                    }
                    output.push(byte);
                }
            }
            1 => {
                let literal = fixed_literal_table()?;
                let lengths = [5u8; 30];
                let distance = Huffman::new(&lengths)?;
                inflate_block(&mut reader, &mut output, &literal, &distance)?;
            }
            2 => {
                let (literal, distance) = dynamic_tables(&mut reader)?;
                inflate_block(&mut reader, &mut output, &literal, &distance)?;
            }
            _ => return Err(ImageError::Corrupt("reserved DEFLATE block type")),
        }
        if final_block {
            break;
        }
    }
    // The Adler-32 trailer starts on the next byte boundary after the final block.
    let trailer_start = if reader.bit == 0 { reader.position } else { reader.position + 1 };
    if trailer_start + 4 <= data.len() {
        let expected = u32::from(data[trailer_start])
            | (u32::from(data[trailer_start + 1]) << 8)
            | (u32::from(data[trailer_start + 2]) << 16)
            | (u32::from(data[trailer_start + 3]) << 24);
        if expected != super::checksum::adler32(&output) {
            return Err(ImageError::Corrupt("zlib Adler-32 mismatch"));
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::deflate::deflate_zlib;

    #[test]
    fn round_trips_repetitive_data() {
        let text = b"miu renders 2d scenes quickly. ".repeat(64);
        let compressed = deflate_zlib(&text);
        let restored = inflate_zlib(&compressed).expect("valid zlib stream");
        assert_eq!(restored, text);
    }

    #[test]
    fn round_trips_incompressible_data() {
        let mut data = Vec::new();
        let mut state = 0x1234_5678u32;
        for _ in 0..5000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            data.push((state >> 24) as u8);
        }
        let compressed = deflate_zlib(&data);
        let restored = inflate_zlib(&compressed).expect("valid zlib stream");
        assert_eq!(restored, data);
    }

    #[test]
    fn rejects_bad_headers() {
        assert_eq!(inflate_zlib(&[]), Err(ImageError::Truncated));
        assert!(matches!(
            inflate_zlib(&[0x78, 0x00, 0, 0, 0, 0]),
            Err(ImageError::Corrupt(_))
        ));
    }
}

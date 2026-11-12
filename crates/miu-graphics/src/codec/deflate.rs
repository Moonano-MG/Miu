//! zlib/DEFLATE encoder used by the PNG writer.
//!
//! The encoder emits one final block with fixed Huffman codes and greedy LZ77
//! matches. That combination needs no per-image table construction, which keeps
//! renderer output deterministic and fast; the compression ratio is close to what
//! general-purpose zlib achieves on the filtered scanlines PNG produces.

use std::collections::HashMap;

const MAX_MATCH: usize = 258;
const MIN_MATCH: usize = 3;
const WINDOW_SIZE: usize = 32_768;

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

struct BitWriter {
    output: Vec<u8>,
    accumulator: u32,
    bits: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self { output: Vec::new(), accumulator: 0, bits: 0 }
    }

    /// Writes `count` bits least-significant-bit first, as DEFLATE headers require.
    fn write_bits(&mut self, value: u32, count: u32) {
        self.accumulator |= (value & ((1u32 << count) - 1)) << self.bits;
        self.bits += count;
        while self.bits >= 8 {
            self.output.push((self.accumulator & 0xFF) as u8);
            self.accumulator >>= 8;
            self.bits -= 8;
        }
    }

    /// Writes a Huffman code most-significant-bit first.
    fn write_code(&mut self, code: u32, length: u32) {
        for index in (0..length).rev() {
            self.write_bits((code >> index) & 1, 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.output.push((self.accumulator & 0xFF) as u8);
        }
        self.output
    }
}

fn fixed_literal_code(symbol: u16) -> (u32, u32) {
    match symbol {
        0..=143 => (0x30 + u32::from(symbol), 8),
        144..=255 => (0x190 + u32::from(symbol) - 144, 9),
        256..=279 => (u32::from(symbol) - 256, 7),
        _ => (0xC0 + u32::from(symbol) - 280, 8),
    }
}

fn length_symbol(length: usize) -> (u16, u8, u32) {
    for index in (0..LENGTH_BASE.len()).rev() {
        let base = usize::from(LENGTH_BASE[index]);
        if length >= base {
            return (
                257 + index as u16,
                LENGTH_EXTRA[index],
                (length - base) as u32,
            );
        }
    }
    (257, 0, 0)
}

fn distance_symbol(distance: usize) -> (u16, u8, u32) {
    for index in (0..DISTANCE_BASE.len()).rev() {
        let base = usize::from(DISTANCE_BASE[index]);
        if distance >= base {
            return (index as u16, DISTANCE_EXTRA[index], (distance - base) as u32);
        }
    }
    (0, 0, 0)
}

fn match_length(data: &[u8], from: usize, to: usize, limit: usize) -> usize {
    let mut length = 0;
    while length < limit && data[from + length] == data[to + length] {
        length += 1;
        if to + length >= data.len() {
            break;
        }
    }
    length
}

/// Compresses `data` into a zlib stream.
pub fn deflate_zlib(data: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::new();
    // zlib header: deflate, 32 KiB window, default compression level, no dictionary.
    writer.output.push(0x78);
    writer.output.push(0x9C);

    // One final block using the fixed Huffman tables.
    writer.write_bits(1, 1);
    writer.write_bits(1, 2);

    let mut chains: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut position = 0usize;
    while position < data.len() {
        let mut best_length = 0usize;
        let mut best_distance = 0usize;
        if position + MIN_MATCH <= data.len() {
            let key = hash_key(data, position);
            if let Some(candidates) = chains.get(&key) {
                let limit = (data.len() - position).min(MAX_MATCH);
                for candidate in candidates.iter().rev().take(32) {
                    let distance = position - *candidate;
                    if distance > WINDOW_SIZE {
                        break;
                    }
                    if data[*candidate] != data[position] {
                        continue;
                    }
                    let length = match_length(data, *candidate, position, limit);
                    if length > best_length {
                        best_length = length;
                        best_distance = distance;
                        if length >= limit {
                            break;
                        }
                    }
                }
            }
            chains.entry(key).or_default().push(position);
        }

        if best_length >= MIN_MATCH {
            let (symbol, extra_bits, extra) = length_symbol(best_length);
            let (code, length) = fixed_literal_code(symbol);
            writer.write_code(code, length);
            if extra_bits > 0 {
                writer.write_bits(extra, u32::from(extra_bits));
            }
            let (distance_code, distance_bits, distance_extra) = distance_symbol(best_distance);
            writer.write_code(u32::from(distance_code), 5);
            if distance_bits > 0 {
                writer.write_bits(distance_extra, u32::from(distance_bits));
            }
            // Register the skipped positions so later matches can find them.
            for offset in position + 1..position + best_length {
                if offset + MIN_MATCH <= data.len() {
                    chains.entry(hash_key(data, offset)).or_default().push(offset);
                }
            }
            position += best_length;
        } else {
            let (code, length) = fixed_literal_code(data[position] as u16);
            writer.write_code(code, length);
            position += 1;
        }
    }

    let (end_code, end_length) = fixed_literal_code(256);
    writer.write_code(end_code, end_length);

    let mut output = writer.finish();
    let checksum = super::checksum::adler32(data);
    output.extend_from_slice(&checksum.to_be_bytes());
    output
}

fn hash_key(data: &[u8], position: usize) -> u32 {
    let a = u32::from(data[position]);
    let b = u32::from(data[position + 1]);
    let c = u32::from(data[position + 2]);
    ((a << 16) ^ (b << 8) ^ c) & 0xFFFF
}

/// Number of occurrences of a length/distance pair that a typical scene produces.
///
/// Exposed for codec tests and for tooling that estimates compression ratios.
pub fn inspect_lengths(data: &[u8]) -> usize {
    data.windows(MIN_MATCH).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_a_valid_zlib_header() {
        let compressed = deflate_zlib(b"hello");
        assert_eq!(compressed[0], 0x78);
        assert_eq!(compressed[1], 0x9C);
        let checksum = super::super::checksum::adler32(b"hello");
        let trailer = &compressed[compressed.len() - 4..];
        assert_eq!(trailer, checksum.to_be_bytes().as_slice());
    }

    #[test]
    fn repeated_runs_compress_below_input_size() {
        let input = vec![7u8; 4096];
        let compressed = deflate_zlib(&input);
        assert!(compressed.len() < input.len() / 4, "compressed to {}", compressed.len());
    }

    #[test]
    fn short_matches_are_encoded() {
        let input = b"abcabcabcabcabcabc";
        let compressed = deflate_zlib(input);
        assert!(compressed.len() < input.len() + 10);
    }
}

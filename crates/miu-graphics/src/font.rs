//! TrueType font parsing.
//!
//! [`Font`] reads the tables a renderer needs from a `.ttf` file: `head` (units per
//! em and loca format), `hhea`/`hmtx` (vertical metrics and advances), `maxp`
//! (glyph count), `cmap` (character to glyph mapping), `loca`/`glyf` (outlines), and
//! the optional legacy `kern` table.
//!
//! Outlines are returned as [`Path`]s in font units with the Y axis already flipped
//! into Miu's screen convention, so a glyph positioned at a baseline extends upwards
//! to negative Y. Composite glyphs are resolved recursively up to
//! [`MAX_COMPOSITE_DEPTH`] levels.
//!
//! Deliberate limits: OpenType/CFF (`CFF `) outlines, variable-font axes, and `GPOS`
//! kerning are not interpreted. Applications that need them should pre-process the
//! font with a tool that flattens it to TrueType outlines.

use std::collections::HashMap;

use miu_core::{Affine2, Rect, Vec2};

use crate::path::{Path, PathBuilder};

/// Maximum composite-glyph recursion depth.
pub const MAX_COMPOSITE_DEPTH: u32 = 8;

/// Upper bound on mapped code points, protecting against hostile tables.
const MAX_CMAP_ENTRIES: usize = 300_000;

/// Failure while parsing a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    /// The file ended before a required field.
    Truncated,
    /// A required table is absent.
    MissingTable(&'static str),
    /// A table has invalid contents.
    BadTable(&'static str),
    /// The font uses a feature this parser does not implement.
    Unsupported(&'static str),
    /// A glyph index was outside the font.
    InvalidGlyph(u16),
    /// Composite glyphs nested deeper than [`MAX_COMPOSITE_DEPTH`].
    CompositeDepth,
}

impl std::error::Error for FontError {}

impl core::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("font data ended unexpectedly"),
            Self::MissingTable(tag) => write!(formatter, "font is missing the {tag} table"),
            Self::BadTable(tag) => write!(formatter, "font table {tag} is malformed"),
            Self::Unsupported(what) => write!(formatter, "unsupported font feature: {what}"),
            Self::InvalidGlyph(glyph) => write!(formatter, "glyph {glyph} is out of range"),
            Self::CompositeDepth => formatter.write_str("composite glyph nesting is too deep"),
        }
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let slice = data.get(offset..offset + 2).ok_or(FontError::Truncated)?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    Ok(read_u16(data, offset)? as i16)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let slice = data.get(offset..offset + 4).ok_or(FontError::Truncated)?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// A parsed TrueType font.
#[derive(Debug, Clone)]
pub struct Font {
    data: Vec<u8>,
    units_per_em: f32,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    num_glyphs: u16,
    number_of_h_metrics: u16,
    hmtx_offset: u32,
    glyf_offset: u32,
    loca: Vec<u32>,
    cmap: HashMap<u32, u16>,
    kerning: HashMap<(u16, u16), i16>,
}

impl Font {
    /// Parses a font from owned file bytes.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, FontError> {
        if data.len() < 12 {
            return Err(FontError::Truncated);
        }
        let num_tables = read_u16(&data, 4)? as usize;
        let mut tables: HashMap<[u8; 4], (u32, u32)> = HashMap::new();
        for index in 0..num_tables {
            let record = 12 + index * 16;
            let slice = data
                .get(record..record + 4)
                .ok_or(FontError::Truncated)?;
            let tag = [slice[0], slice[1], slice[2], slice[3]];
            let offset = read_u32(&data, record + 8)?;
            let length = read_u32(&data, record + 12)?;
            if (offset as usize) > data.len() {
                return Err(FontError::BadTable("table offset"));
            }
            tables.insert(tag, (offset, length));
        }

        let head = *tables.get(b"head").ok_or(FontError::MissingTable("head"))?;
        let hhea = *tables.get(b"hhea").ok_or(FontError::MissingTable("hhea"))?;
        let maxp = *tables.get(b"maxp").ok_or(FontError::MissingTable("maxp"))?;
        let hmtx = *tables.get(b"hmtx").ok_or(FontError::MissingTable("hmtx"))?;
        let loca_table = *tables.get(b"loca").ok_or(FontError::MissingTable("loca"))?;
        let glyf = *tables.get(b"glyf").ok_or(FontError::MissingTable("glyf"))?;
        let cmap_table = *tables.get(b"cmap").ok_or(FontError::MissingTable("cmap"))?;
        if tables.contains_key(b"CFF ") {
            return Err(FontError::Unsupported("CFF outlines"));
        }

        let head_start = head.0 as usize;
        let units_per_em = f32::from(read_u16(&data, head_start + 18)?);
        let index_to_loc_format = read_i16(&data, head_start + 50)?;
        if units_per_em <= 0.0 || units_per_em > 16_384.0 {
            return Err(FontError::BadTable("head"));
        }

        let hhea_start = hhea.0 as usize;
        let ascent = f32::from(read_i16(&data, hhea_start + 4)?);
        let descent = f32::from(read_i16(&data, hhea_start + 6)?);
        let line_gap = f32::from(read_i16(&data, hhea_start + 8)?);
        let number_of_h_metrics = read_u16(&data, hhea_start + 34)?;

        let num_glyphs = read_u16(&data, maxp.0 as usize + 4)?;
        if num_glyphs == 0 {
            return Err(FontError::BadTable("maxp"));
        }

        let loca_start = loca_table.0 as usize;
        let entry_count = num_glyphs as usize + 1;
        let mut loca = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let value = if index_to_loc_format == 0 {
                u32::from(read_u16(&data, loca_start + index * 2)?) * 2
            } else {
                read_u32(&data, loca_start + index * 4)?
            };
            if value as usize > data.len() {
                return Err(FontError::BadTable("loca"));
            }
            loca.push(value);
        }

        let cmap = parse_cmap(&data, cmap_table.0 as usize, cmap_table.1 as usize)?;
        let kerning = match tables.get(b"kern") {
            Some(table) => parse_kern(&data, table.0 as usize, table.1 as usize),
            None => HashMap::new(),
        };

        Ok(Self {
            data,
            units_per_em,
            ascent,
            descent: descent.min(0.0),
            line_gap,
            num_glyphs,
            number_of_h_metrics,
            hmtx_offset: hmtx.0,
            glyf_offset: glyf.0,
            loca,
            cmap,
            kerning,
        })
    }

    /// Design units per em, as declared by `head`.
    pub fn units_per_em(&self) -> f32 {
        self.units_per_em
    }

    /// Ascender in font units (positive, above the baseline).
    pub fn ascent(&self) -> f32 {
        self.ascent
    }

    /// Descender in font units (negative, below the baseline).
    pub fn descent(&self) -> f32 {
        self.descent
    }

    /// Recommended extra spacing between lines, in font units.
    pub fn line_gap(&self) -> f32 {
        self.line_gap
    }

    /// Number of glyphs in the font.
    pub fn glyph_count(&self) -> u16 {
        self.num_glyphs
    }

    /// Number of code points with an explicit mapping.
    pub fn mapped_code_points(&self) -> usize {
        self.cmap.len()
    }

    /// Glyph for a Unicode scalar, or `None` when the font does not cover it.
    pub fn glyph_index(&self, character: char) -> Option<u16> {
        self.cmap.get(&(character as u32)).copied()
    }

    /// Glyph 0, which fonts use for missing characters.
    pub const fn notdef(&self) -> u16 {
        0
    }

    /// Horizontal advance in font units.
    pub fn advance(&self, glyph: u16) -> f32 {
        if glyph >= self.num_glyphs || self.number_of_h_metrics == 0 {
            return 0.0;
        }
        let index = glyph.min(self.number_of_h_metrics.saturating_sub(1)) as usize;
        let offset = self.hmtx_offset as usize + index * 4;
        f32::from(read_u16(&self.data, offset).unwrap_or(0))
    }

    /// Kerning adjustment in font units between two glyphs.
    pub fn kerning(&self, left: u16, right: u16) -> f32 {
        f32::from(self.kerning.get(&(left, right)).copied().unwrap_or(0))
    }

    /// Whether the font has legacy `kern` data.
    pub fn has_kerning(&self) -> bool {
        !self.kerning.is_empty()
    }

    /// Glyph outline in font units, Y-flipped into screen orientation.
    pub fn outline(&self, glyph: u16) -> Result<Path, FontError> {
        let mut builder = PathBuilder::new();
        self.outline_into(glyph, &mut builder, Affine2::IDENTITY, 0)?;
        let flip = Affine2::from_scale(Vec2::new(1.0, -1.0));
        Ok(builder.build().transformed(&flip))
    }

    /// Bounding box of a glyph in font units, Y-flipped, or `None` for an empty glyph.
    pub fn glyph_bounds(&self, glyph: u16) -> Option<Rect> {
        if glyph >= self.num_glyphs {
            return None;
        }
        let start = self.loca.get(glyph as usize).copied()? as usize;
        let end = self.loca.get(glyph as usize + 1).copied()? as usize;
        if end <= start {
            return None;
        }
        let base = self.glyf_offset as usize + start;
        let x_min = f32::from(read_i16(&self.data, base + 2).ok()?);
        let y_min = f32::from(read_i16(&self.data, base + 4).ok()?);
        let x_max = f32::from(read_i16(&self.data, base + 6).ok()?);
        let y_max = f32::from(read_i16(&self.data, base + 8).ok()?);
        Some(Rect::from_min_max(
            Vec2::new(x_min, -y_max),
            Vec2::new(x_max, -y_min),
        ))
    }

    fn outline_into(
        &self,
        glyph: u16,
        builder: &mut PathBuilder,
        transform: Affine2,
        depth: u32,
    ) -> Result<(), FontError> {
        if glyph >= self.num_glyphs {
            return Err(FontError::InvalidGlyph(glyph));
        }
        let start = self.loca[glyph as usize] as usize;
        let end = self.loca[glyph as usize + 1] as usize;
        if end <= start {
            return Ok(());
        }
        let base = self.glyf_offset as usize + start;
        let endpoints = base + end;
        let contour_count = read_i16(&self.data, base)?;
        if contour_count >= 0 {
            self.simple_outline(base, contour_count as usize, builder, transform)?;
        } else {
            if depth >= MAX_COMPOSITE_DEPTH {
                return Err(FontError::CompositeDepth);
            }
            self.composite_outline(base, endpoints, builder, transform, depth)?;
        }
        Ok(())
    }

    fn simple_outline(
        &self,
        base: usize,
        contour_count: usize,
        builder: &mut PathBuilder,
        transform: Affine2,
    ) -> Result<(), FontError> {
        if contour_count == 0 {
            return Ok(());
        }
        if contour_count > 1024 {
            return Err(FontError::BadTable("glyf"));
        }
        let mut position = base + 10;
        let mut endpoints = Vec::with_capacity(contour_count);
        for _ in 0..contour_count {
            let value = read_u16(&self.data, position)?;
            position += 2;
            endpoints.push(value as usize);
        }
        let point_count = endpoints
            .last()
            .map(|value| value + 1)
            .unwrap_or(0);
        if point_count == 0 || point_count > 65535 {
            return Err(FontError::BadTable("glyf"));
        }
        let instruction_length = read_u16(&self.data, position)? as usize;
        position += 2 + instruction_length;

        // Flags, with the repeat bit expanding runs of identical flags.
        let mut flags = Vec::with_capacity(point_count);
        while flags.len() < point_count {
            let flag = *self.data.get(position).ok_or(FontError::Truncated)?;
            position += 1;
            flags.push(flag);
            if flag & 0x08 != 0 {
                let repeat = *self.data.get(position).ok_or(FontError::Truncated)? as usize;
                position += 1;
                for _ in 0..repeat {
                    if flags.len() >= point_count {
                        break;
                    }
                    flags.push(flag);
                }
            }
        }

        let mut x = 0i32;
        let mut y = 0i32;
        let mut points: Vec<(Vec2, bool)> = Vec::with_capacity(point_count);
        let mut x_values = Vec::with_capacity(point_count);
        for flag in &flags {
            if flag & 0x02 != 0 {
                let delta = *self.data.get(position).ok_or(FontError::Truncated)? as i32;
                position += 1;
                x += if flag & 0x10 != 0 { delta } else { -delta };
            } else if flag & 0x10 == 0 {
                x += i32::from(read_i16(&self.data, position)?);
                position += 2;
            }
            x_values.push(x);
        }
        for (index, flag) in flags.iter().enumerate() {
            if flag & 0x04 != 0 {
                let delta = *self.data.get(position).ok_or(FontError::Truncated)? as i32;
                position += 1;
                y += if flag & 0x20 != 0 { delta } else { -delta };
            } else if flag & 0x20 == 0 {
                y += i32::from(read_i16(&self.data, position)?);
                position += 2;
            }
            points.push((
                Vec2::new(x_values[index] as f32, y as f32),
                flag & 0x01 != 0,
            ));
        }

        let mut start = 0usize;
        for endpoint in endpoints {
            if endpoint >= points.len() {
                return Err(FontError::BadTable("glyf"));
            }
            emit_contour(&points[start..=endpoint], builder, transform);
            start = endpoint + 1;
        }
        Ok(())
    }

    fn composite_outline(
        &self,
        base: usize,
        limit: usize,
        builder: &mut PathBuilder,
        transform: Affine2,
        depth: u32,
    ) -> Result<(), FontError> {
        let mut position = base + 10;
        loop {
            if position + 4 > limit {
                return Err(FontError::BadTable("composite glyph"));
            }
            let flags = read_u16(&self.data, position)?;
            let glyph = read_u16(&self.data, position + 2)?;
            position += 4;
            let args_are_words = flags & 0x0001 != 0;
            let args_are_xy = flags & 0x0002 != 0;
            let (dx, dy) = if args_are_words {
                let first = read_i16(&self.data, position)?;
                let second = read_i16(&self.data, position + 2)?;
                position += 4;
                (f32::from(first), f32::from(second))
            } else {
                let first = *self.data.get(position).ok_or(FontError::Truncated)? as i8;
                let second = *self.data.get(position + 1).ok_or(FontError::Truncated)? as i8;
                position += 2;
                (f32::from(first), f32::from(second))
            };
            if !args_are_xy {
                return Err(FontError::Unsupported("point-matching composite glyphs"));
            }
            let mut component = Affine2::from_translation(Vec2::new(dx, dy));
            if flags & 0x0008 != 0 {
                let scale = f2dot14(self.data.get(position..position + 2).ok_or(FontError::Truncated)?);
                position += 2;
                component = component.then(Affine2::from_scale(Vec2::splat(scale)));
            } else if flags & 0x0040 != 0 {
                let scale_x = f2dot14(self.data.get(position..position + 2).ok_or(FontError::Truncated)?);
                let scale_y =
                    f2dot14(self.data.get(position + 2..position + 4).ok_or(FontError::Truncated)?);
                position += 4;
                component = component.then(Affine2::from_scale(Vec2::new(scale_x, scale_y)));
            } else if flags & 0x0080 != 0 {
                let a = f2dot14(self.data.get(position..position + 2).ok_or(FontError::Truncated)?);
                let b = f2dot14(self.data.get(position + 2..position + 4).ok_or(FontError::Truncated)?);
                let c = f2dot14(self.data.get(position + 4..position + 6).ok_or(FontError::Truncated)?);
                let d = f2dot14(self.data.get(position + 6..position + 8).ok_or(FontError::Truncated)?);
                position += 8;
                component = component.then(Affine2::new(a, b, c, d, 0.0, 0.0));
            }
            let combined = component.then(transform);
            self.outline_into(glyph, builder, combined, depth + 1)?;
            if flags & 0x0020 == 0 {
                break;
            }
        }
        Ok(())
    }
}

/// Converts one TrueType contour into path segments.
///
/// TrueType stores on-curve and off-curve points; two consecutive off-curve points
/// imply an on-curve midpoint between them. If a contour starts with an off-curve
/// point, the path starts at the midpoint between the last and first points.
fn emit_contour(points: &[(Vec2, bool)], builder: &mut PathBuilder, transform: Affine2) {
    let count = points.len();
    if count == 0 {
        return;
    }
    let point = |index: usize| transform.transform_point(points[index % count].0);
    let on_curve = |index: usize| points[index % count].1;

    let start_on_curve = if on_curve(0) {
        Some(0usize)
    } else if on_curve(count - 1) {
        Some(count - 1)
    } else {
        None
    };
    let start_point = match start_on_curve {
        Some(index) => point(index),
        None => (point(0) + point(count - 1)) * 0.5,
    };
    builder.move_to(start_point);

    let mut index = match start_on_curve {
        Some(index) => (index + 1) % count,
        None => 0,
    };
    let mut remaining = match start_on_curve {
        Some(_) => count - 1,
        None => count,
    };
    let mut guard = 0usize;
    while remaining > 0 && guard <= count * 4 {
        guard += 1;
        if on_curve(index) {
            builder.line_to(point(index));
            index = (index + 1) % count;
            remaining = remaining.saturating_sub(1);
        } else {
            let next = (index + 1) % count;
            if on_curve(next) {
                builder.quad_to(point(index), point(next));
                index = (next + 1) % count;
                remaining = remaining.saturating_sub(2);
            } else {
                builder.quad_to(point(index), (point(index) + point(next)) * 0.5);
                index = next;
                remaining = remaining.saturating_sub(1);
            }
        }
    }
    builder.close();
}

fn f2dot14(bytes: &[u8]) -> f32 {
    if bytes.len() < 2 {
        return 0.0;
    }
    f32::from(i16::from_be_bytes([bytes[0], bytes[1]])) / 16384.0
}

fn parse_cmap(data: &[u8], start: usize, length: usize) -> Result<HashMap<u32, u16>, FontError> {
    let version = read_u16(data, start)?;
    if version != 0 {
        return Err(FontError::BadTable("cmap"));
    }
    let table_count = read_u16(data, start + 2)? as usize;
    if table_count == 0 || table_count > 64 {
        return Err(FontError::BadTable("cmap"));
    }
    // Prefer full-repertoire Unicode tables, then BMP Unicode, then any Unicode one.
    let mut best: Option<(u8, u32)> = None;
    for index in 0..table_count {
        let record = start + 4 + index * 8;
        let platform = read_u16(data, record)?;
        let encoding = read_u16(data, record + 2)?;
        let offset = read_u32(data, record + 4)? as usize;
        let subtable = start + offset;
        if offset >= length {
            continue;
        }
        let preference = match (platform, encoding) {
            (3, 10) | (0, 6) | (0, 4) => 3u8,
            (3, 1) | (0, 3) | (0, 2) | (0, 1) | (0, 0) => 2,
            _ => 1,
        };
        if best.map(|(current, _)| preference > current).unwrap_or(true) {
            best = Some((preference, subtable as u32));
        }
    }
    let Some((_, subtable)) = best else {
        return Err(FontError::BadTable("cmap"));
    };
    let subtable = subtable as usize;
    let format = read_u16(data, subtable)?;
    let mut map: HashMap<u32, u16> = HashMap::new();
    match format {
        0 => {
            for code in 0..256usize {
                let glyph = *data.get(subtable + 6 + code).ok_or(FontError::Truncated)?;
                if glyph != 0 {
                    map.insert(code as u32, u16::from(glyph));
                }
            }
        }
        4 => {
            let segment_count = read_u16(data, subtable + 6)? as usize / 2;
            if segment_count == 0 || segment_count > 4096 {
                return Err(FontError::BadTable("cmap format 4"));
            }
            let end_codes = subtable + 14;
            let start_codes = end_codes + segment_count * 2 + 2;
            let deltas = start_codes + segment_count * 2;
            let range_offsets = deltas + segment_count * 2;
            for segment in 0..segment_count {
                let end = read_u16(data, end_codes + segment * 2)?;
                let start = read_u16(data, start_codes + segment * 2)?;
                let delta = read_i16(data, deltas + segment * 2)?;
                let range_offset = read_u16(data, range_offsets + segment * 2)?;
                if start > end {
                    continue;
                }
                for code in start..=end {
                    if code == 0xFFFF {
                        continue;
                    }
                    let glyph = if range_offset == 0 {
                        (code as i32 + i32::from(delta)) as u16
                    } else {
                        let address = range_offsets
                            + segment * 2
                            + range_offset as usize
                            + (code - start) as usize * 2;
                        match read_u16(data, address) {
                            Ok(0) => 0,
                            Ok(value) => (i32::from(value) + i32::from(delta)) as u16,
                            Err(_) => 0,
                        }
                    };
                    if glyph != 0 {
                        map.insert(u32::from(code), glyph);
                    }
                    if map.len() > MAX_CMAP_ENTRIES {
                        return Err(FontError::BadTable("cmap too large"));
                    }
                }
            }
        }
        6 => {
            let first = read_u16(data, subtable + 6)?;
            let count = read_u16(data, subtable + 8)? as usize;
            for index in 0..count {
                let glyph = read_u16(data, subtable + 10 + index * 2)?;
                if glyph != 0 {
                    map.insert(u32::from(first) + index as u32, glyph);
                }
            }
        }
        12 => {
            let group_count = read_u32(data, subtable + 12)? as usize;
            if group_count > 65_535 {
                return Err(FontError::BadTable("cmap format 12"));
            }
            for group in 0..group_count {
                let record = subtable + 16 + group * 12;
                let start = read_u32(data, record)?;
                let end = read_u32(data, record + 4)?;
                let glyph_start = read_u32(data, record + 8)?;
                if start > end || end - start > 65_535 {
                    continue;
                }
                for offset in 0..=(end - start) {
                    let glyph = glyph_start + offset;
                    if glyph != 0 && glyph <= u32::from(u16::MAX) {
                        map.insert(start + offset, glyph as u16);
                    }
                }
                if map.len() > MAX_CMAP_ENTRIES {
                    return Err(FontError::BadTable("cmap too large"));
                }
            }
        }
        _ => return Err(FontError::Unsupported("cmap subtable format")),
    }
    if map.is_empty() {
        return Err(FontError::BadTable("cmap has no mappings"));
    }
    Ok(map)
}

fn parse_kern(data: &[u8], start: usize, length: usize) -> HashMap<(u16, u16), i16> {
    let mut pairs = HashMap::new();
    let version = read_u16(data, start).unwrap_or(0);
    if version != 0 {
        return pairs;
    }
    let table_count = read_u16(data, start + 2).unwrap_or(0) as usize;
    let mut offset = start + 4;
    for _ in 0..table_count {
        if offset + 6 > start + length {
            break;
        }
        let subtable_length = read_u16(data, offset + 2).unwrap_or(0) as usize;
        let coverage = read_u16(data, offset + 4).unwrap_or(0);
        let format = coverage >> 8;
        let horizontal = coverage & 0x1 != 0;
        let cross_stream = coverage & 0x4 != 0;
        if format == 0 && horizontal && !cross_stream && subtable_length >= 14 {
            let pair_count = read_u16(data, offset + 6).unwrap_or(0) as usize;
            let pair_start = offset + 14;
            for index in 0..pair_count {
                let record = pair_start + index * 6;
                if record + 6 > start + length {
                    break;
                }
                let left = read_u16(data, record).unwrap_or(0);
                let right = read_u16(data, record + 2).unwrap_or(0);
                let value = read_i16(data, record + 4).unwrap_or(0);
                pairs.insert((left, right), value);
            }
        }
        if subtable_length < 6 {
            break;
        }
        offset += subtable_length;
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal TrueType font with two glyphs: an empty glyph 0 and a square
    /// glyph 1 mapped from `A`.
    fn tiny_font() -> Vec<u8> {
        fn push_u16(bytes: &mut Vec<u8>, value: u16) {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        fn push_i16(bytes: &mut Vec<u8>, value: i16) {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        fn push_u32(bytes: &mut Vec<u8>, value: u32) {
            bytes.extend_from_slice(&value.to_be_bytes());
        }

        let mut head = Vec::new();
        push_u32(&mut head, 0x0001_0000);
        push_u32(&mut head, 0);
        push_u32(&mut head, 0);
        push_u32(&mut head, 0x5F0F_3CF5);
        push_u16(&mut head, 0);
        push_u16(&mut head, 1000); // unitsPerEm
        head.extend_from_slice(&[0u8; 16]);
        push_i16(&mut head, 0);
        push_i16(&mut head, 0);
        push_i16(&mut head, 500);
        push_i16(&mut head, 700);
        push_u16(&mut head, 0);
        push_u16(&mut head, 8);
        push_i16(&mut head, 2);
        push_i16(&mut head, 0); // indexToLocFormat = short
        push_i16(&mut head, 0);

        let mut hhea = Vec::new();
        push_u32(&mut hhea, 0x0001_0000);
        push_i16(&mut hhea, 800);
        push_i16(&mut hhea, -200);
        push_i16(&mut hhea, 100);
        push_u16(&mut hhea, 500);
        hhea.extend_from_slice(&[0u8; 22]);
        push_i16(&mut hhea, 0);
        push_u16(&mut hhea, 2); // numberOfHMetrics
        assert_eq!(hhea.len(), 36);

        let mut maxp = Vec::new();
        push_u32(&mut maxp, 0x0001_0000);
        push_u16(&mut maxp, 2); // numGlyphs

        let mut hmtx = Vec::new();
        push_u16(&mut hmtx, 250);
        push_i16(&mut hmtx, 0);
        push_u16(&mut hmtx, 500);
        push_i16(&mut hmtx, 0);

        // cmap format 4 with two segments: 'A' and the 0xFFFF terminator.
        let mut cmap_subtable = Vec::new();
        push_u16(&mut cmap_subtable, 4);
        push_u16(&mut cmap_subtable, 32);
        push_u16(&mut cmap_subtable, 0);
        push_u16(&mut cmap_subtable, 4); // segCountX2
        push_u16(&mut cmap_subtable, 4); // searchRange
        push_u16(&mut cmap_subtable, 1); // entrySelector
        push_u16(&mut cmap_subtable, 0); // rangeShift
        push_u16(&mut cmap_subtable, 0x41); // endCode[0]
        push_u16(&mut cmap_subtable, 0xFFFF); // endCode[1]
        push_u16(&mut cmap_subtable, 0); // reservedPad
        push_u16(&mut cmap_subtable, 0x41); // startCode[0]
        push_u16(&mut cmap_subtable, 0xFFFF);
        push_i16(&mut cmap_subtable, 1 - 0x41); // idDelta[0]
        push_i16(&mut cmap_subtable, 1);
        push_u16(&mut cmap_subtable, 0); // idRangeOffset[0]
        push_u16(&mut cmap_subtable, 0);
        assert_eq!(cmap_subtable.len(), 32);

        let mut cmap = Vec::new();
        push_u16(&mut cmap, 0);
        push_u16(&mut cmap, 1);
        push_u16(&mut cmap, 3);
        push_u16(&mut cmap, 1);
        push_u32(&mut cmap, 12);
        cmap.extend_from_slice(&cmap_subtable);

        // Square glyph: 4 on-curve points, 1000 units tall.
        let mut glyf = Vec::new();
        push_i16(&mut glyf, 1); // numberOfContours
        push_i16(&mut glyf, 0); // xMin
        push_i16(&mut glyf, 0); // yMin
        push_i16(&mut glyf, 500); // xMax
        push_i16(&mut glyf, 700); // yMax
        push_u16(&mut glyf, 3); // endPtsOfContours
        push_u16(&mut glyf, 0); // instructionLength
        glyf.extend_from_slice(&[0x01, 0x01, 0x01, 0x01]); // all on-curve
        for delta in [0i16, 500, 0, -500] {
            push_i16(&mut glyf, delta);
        }
        for delta in [0i16, 0, 700, 0] {
            push_i16(&mut glyf, delta);
        }

        let mut loca = Vec::new();
        push_u16(&mut loca, 0);
        push_u16(&mut loca, 0);
        push_u16(&mut loca, (glyf.len() / 2) as u16);

        let tables: [(&[u8; 4], &Vec<u8>); 7] = [
            (b"cmap", &cmap),
            (b"glyf", &glyf),
            (b"head", &head),
            (b"hhea", &hhea),
            (b"hmtx", &hmtx),
            (b"loca", &loca),
            (b"maxp", &maxp),
        ];
        let mut directory = Vec::new();
        push_u32(&mut directory, 0x0001_0000);
        push_u16(&mut directory, tables.len() as u16);
        push_u16(&mut directory, 64);
        push_u16(&mut directory, 3);
        push_u16(&mut directory, 56);
        let mut body = Vec::new();
        let mut records = Vec::new();
        let mut offset = 12 + tables.len() * 16;
        for (tag, table) in tables.iter() {
            records.extend_from_slice(*tag);
            push_u32(&mut records, 0);
            push_u32(&mut records, offset as u32);
            push_u32(&mut records, table.len() as u32);
            body.extend_from_slice(table);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            offset = 12 + tables.len() * 16 + body.len();
        }
        let mut font = Vec::new();
        font.extend_from_slice(&directory);
        font.extend_from_slice(&records);
        font.extend_from_slice(&body);
        font
    }

    #[test]
    fn parses_metrics_and_mappings() {
        let font = Font::from_bytes(tiny_font()).expect("font parses");
        assert_eq!(font.units_per_em(), 1000.0);
        assert_eq!(font.ascent(), 800.0);
        assert_eq!(font.descent(), -200.0);
        assert_eq!(font.glyph_count(), 2);
        assert_eq!(font.glyph_index('A'), Some(1));
        assert_eq!(font.glyph_index('B'), None);
        assert_eq!(font.advance(1), 500.0);
        assert_eq!(font.advance(0), 250.0);
    }

    #[test]
    fn outlines_flip_the_y_axis() {
        let font = Font::from_bytes(tiny_font()).expect("font parses");
        let outline = font.outline(1).expect("glyph has an outline");
        let bounds = outline.bounds();
        assert!(bounds.min().approx_eq(Vec2::new(0.0, -700.0), 0.5));
        assert!(bounds.max().approx_eq(Vec2::new(500.0, 0.0), 0.5));
    }

    #[test]
    fn empty_glyph_has_no_outline() {
        let font = Font::from_bytes(tiny_font()).expect("font parses");
        assert!(font.outline(0).expect("glyph zero is empty").is_empty());
        assert!(font.glyph_bounds(0).is_none());
        assert!(font.glyph_bounds(1).is_some());
    }

    #[test]
    fn broken_fonts_are_rejected() {
        assert_eq!(Font::from_bytes(Vec::new()), Err(FontError::Truncated));
        let mut font = tiny_font();
        font.truncate(20);
        assert!(Font::from_bytes(font).is_err());
    }

    #[test]
    fn missing_kern_table_is_not_an_error() {
        let font = Font::from_bytes(tiny_font()).expect("font parses");
        assert!(!font.has_kerning());
        assert_eq!(font.kerning(0, 1), 0.0);
    }
}

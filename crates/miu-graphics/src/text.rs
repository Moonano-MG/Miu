//! Text layout and glyph caching.
//!
//! [`layout_text`] converts a string and a [`TextStyle`] into positioned glyphs using
//! a [`Font`]. Layout is left-to-right, handles explicit newlines, optional word or
//! character wrapping, letter spacing, `kern` pair adjustments, and horizontal
//! alignment within a known width.
//!
//! [`GlyphCache`] rasterises glyph outlines on demand and keeps the coverage masks
//! for reuse. The software renderer uses it so redrawing the same dialogue text every
//! frame costs one blit per glyph instead of one curve rasterisation.

use std::collections::HashMap;

use miu_core::{Affine2, Rect, Vec2};

use crate::font::Font;
use crate::raster::{rasterize, CoverageMask, RasterOptions};

/// Horizontal alignment of laid-out lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TextAlign {
    /// Lines start at the layout origin.
    #[default]
    Left = 0,
    /// Lines are centred inside the layout width.
    Center = 1,
    /// Lines end at the layout width.
    Right = 2,
}

/// Wrapping strategy for text that exceeds the available width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TextWrap {
    /// Never wrap; lines run past the available width.
    None = 0,
    /// Break at spaces, falling back to character breaks for long words.
    #[default]
    Word = 1,
    /// Break at any character.
    Character = 2,
}

/// Text appearance and layout parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Em size in logical pixels.
    pub font_size: f32,
    /// Multiplier applied to the font's natural line height.
    pub line_height: f32,
    /// Extra spacing between glyphs, in pixels.
    pub letter_spacing: f32,
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Wrapping strategy.
    pub wrap: TextWrap,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            line_height: 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            wrap: TextWrap::Word,
        }
    }
}

impl TextStyle {
    /// Creates a style with an em size in pixels.
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size: if font_size.is_finite() && font_size > 0.0 { font_size } else { 16.0 },
            ..Self::default()
        }
    }

    /// Sets the line-height multiplier.
    #[must_use]
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = if line_height.is_finite() && line_height > 0.0 { line_height } else { 1.2 };
        self
    }

    /// Sets the letter spacing in pixels.
    #[must_use]
    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = if letter_spacing.is_finite() { letter_spacing } else { 0.0 };
        self
    }

    /// Sets the alignment.
    #[must_use]
    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Sets the wrapping strategy.
    #[must_use]
    pub fn with_wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Sanitised em size in pixels.
    pub fn em_size(&self) -> f32 {
        if self.font_size.is_finite() && self.font_size > 0.0 {
            self.font_size
        } else {
            16.0
        }
    }

    /// Natural line height in pixels for a font.
    pub fn natural_line_height(&self, font: &Font) -> f32 {
        let units = font.ascent() - font.descent() + font.line_gap().max(0.0);
        let height = units * (self.em_size() / font.units_per_em());
        if height > 0.0 { height } else { self.em_size() }
    }

    /// Distance between consecutive baselines in pixels.
    pub fn line_advance(&self, font: &Font) -> f32 {
        self.natural_line_height(font) * self.line_height.max(0.01)
    }
}

/// One positioned glyph in layout space (origin at the text block's top-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    /// Glyph index inside the font.
    pub glyph: u16,
    /// Pen X in pixels, relative to the text block origin.
    pub x: f32,
    /// Baseline Y in pixels, relative to the text block origin.
    pub y: f32,
    /// Advance in pixels, including letter spacing and kerning.
    pub advance: f32,
}

/// One laid-out line.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// Index of the first glyph in [`TextLayout::glyphs`].
    pub first: usize,
    /// Number of glyphs on the line.
    pub count: usize,
    /// Advance width of the line in pixels.
    pub width: f32,
    /// Baseline Y in pixels.
    pub baseline: f32,
}

/// Result of [`layout_text`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLayout {
    /// Positioned glyphs in reading order.
    pub glyphs: Vec<PlacedGlyph>,
    /// Line records referencing ranges of `glyphs`.
    pub lines: Vec<TextLine>,
    /// Width of the widest line in pixels.
    pub width: f32,
    /// Total height in pixels, including the last line's descent.
    pub height: f32,
}

impl TextLayout {
    /// Number of positioned glyphs.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Whether the layout contains no glyphs.
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// Size of the laid-out block in pixels.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }

    /// Local-space rectangle covering the block.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
}

#[derive(Debug, Clone)]
struct LineBuild {
    glyphs: Vec<(u16, f32)>,
    width: f32,
    last_space: Option<usize>,
    width_at_space: f32,
}

impl LineBuild {
    fn new() -> Self {
        Self { glyphs: Vec::new(), width: 0.0, last_space: None, width_at_space: 0.0 }
    }

    fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    fn push(&mut self, ch: char, glyph: u16, advance: f32) {
        if ch == ' ' || ch == '\t' {
            self.last_space = Some(self.glyphs.len());
            self.width_at_space = self.width;
        }
        self.glyphs.push((glyph, advance));
        self.width += advance;
    }

    fn carry_after_space(&mut self) -> LineBuild {
        let mut next = LineBuild::new();
        if let Some(space) = self.last_space {
            let carried: Vec<(u16, f32)> = self.glyphs.drain(space + 1..).collect();
            for (glyph, advance) in carried {
                next.width += advance;
                next.glyphs.push((glyph, advance));
            }
            self.glyphs.truncate(space);
            self.width = self.width_at_space;
            if self.last_space.map(|index| index >= self.glyphs.len()).unwrap_or(false) {
                self.last_space = None;
            }
        }
        next
    }

    fn finish(&self) -> Vec<(u16, f32)> {
        self.glyphs.clone()
    }
}

/// Lays out `text` with `font`, optionally wrapping at `max_width` pixels.
///
/// `max_width` also defines the alignment box: centred and right-aligned text is
/// positioned inside that width. Passing `None` aligns inside the widest line.
pub fn layout_text(
    font: &Font,
    text: &str,
    style: &TextStyle,
    max_width: Option<f32>,
) -> TextLayout {
    let scale = style.em_size() / font.units_per_em().max(1.0);
    let line_advance = style.line_advance(font);
    let limit = max_width.filter(|value| value.is_finite() && *value > 0.0);

    let mut line_glyph_runs: Vec<Vec<(u16, f32)>> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = LineBuild::new();
        let mut previous_glyph: Option<u16> = None;
        for ch in paragraph.chars() {
            let glyph = font.glyph_index(ch).unwrap_or_else(|| font.notdef());
            let mut advance = font.advance(glyph) * scale + style.letter_spacing;
            if let Some(previous) = previous_glyph {
                advance += font.kerning(previous, glyph) * scale;
            }
            previous_glyph = Some(glyph);

            if let Some(maximum) = limit {
                if !line.is_empty() && line.width + advance > maximum {
                    match style.wrap {
                        TextWrap::None => {}
                        TextWrap::Character => {
                            line_glyph_runs.push(line.finish());
                            line = LineBuild::new();
                        }
                        TextWrap::Word => {
                            if line.last_space.is_some() {
                                let mut next = line.carry_after_space();
                                line_glyph_runs.push(line.finish());
                                if ch == ' ' {
                                    line = next;
                                    continue;
                                }
                                line = next;
                            } else {
                                line_glyph_runs.push(line.finish());
                                line = LineBuild::new();
                            }
                        }
                    }
                }
            }
            line.push(ch, glyph, advance);
        }
        line_glyph_runs.push(line.finish());
    }

    let line_widths: Vec<f32> = line_glyph_runs
        .iter()
        .map(|glyphs| glyphs.iter().map(|(_, advance)| *advance).sum())
        .collect();
    let widest = line_widths
        .iter()
        .copied()
        .fold(0.0f32, f32::max);
    let alignment_width = limit.unwrap_or(widest);

    let ascent = font.ascent() * scale;
    let mut layout = TextLayout {
        glyphs: Vec::new(),
        lines: Vec::new(),
        width: widest,
        height: line_advance * line_glyph_runs.len() as f32,
    };

    for (line_index, glyphs) in line_glyph_runs.iter().enumerate() {
        let width = line_widths[line_index];
        let offset = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (alignment_width - width) * 0.5,
            TextAlign::Right => alignment_width - width,
        };
        let baseline = ascent + line_advance * line_index as f32;
        let first = layout.glyphs.len();
        let mut pen = offset;
        for (glyph, advance) in glyphs {
            layout.glyphs.push(PlacedGlyph { glyph: *glyph, x: pen, y: baseline, advance: *advance });
            pen += *advance;
        }
        // The last glyph's trailing letter spacing is not part of the visual line.
        let visual_width = (width - style.letter_spacing.max(0.0)).max(0.0);
        layout.lines.push(TextLine {
            first,
            count: glyphs.len(),
            width: visual_width,
            baseline,
        });
    }
    layout.width = layout
        .lines
        .iter()
        .map(|line| line.width)
        .fold(0.0f32, f32::max);
    layout
}

/// Measures `text` and returns its size in pixels.
pub fn measure_text(
    font: &Font,
    text: &str,
    style: &TextStyle,
    max_width: Option<f32>,
) -> Vec2 {
    layout_text(font, text, style, max_width).size()
}

/// Identifier-keyed font lookup shared by scenes, the ABI, and renderers.
#[derive(Debug, Default, Clone)]
pub struct FontStore {
    fonts: HashMap<u64, Font>,
}

impl FontStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a font.
    pub fn insert(&mut self, id: u64, font: Font) -> Option<Font> {
        self.fonts.insert(id, font)
    }

    /// Looks up a font.
    pub fn get(&self, id: u64) -> Option<&Font> {
        self.fonts.get(&id)
    }

    /// Removes a font.
    pub fn remove(&mut self, id: u64) -> Option<Font> {
        self.fonts.remove(&id)
    }

    /// Whether `id` is present.
    pub fn contains(&self, id: u64) -> bool {
        self.fonts.contains_key(&id)
    }

    /// Number of stored fonts.
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Releases every font.
    pub fn clear(&mut self) {
        self.fonts.clear();
    }

    /// Iterates over `(id, font)` pairs without a defined order.
    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Font)> {
        self.fonts.iter()
    }
}

/// Rasterised glyph coverage, positioned relative to the glyph origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRaster {
    /// Coverage mask; `left`/`top` are offsets from the pen position in pixels.
    pub mask: CoverageMask,
    /// Horizontal offset from the pen position.
    pub bearing_x: i32,
    /// Vertical offset from the baseline.
    pub bearing_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    glyph: u16,
    size_quarters: u16,
}

/// Rasterised glyph cache with a byte budget.
///
/// Sizes are keyed in quarter-pixel steps, so an animating font size still reuses
/// masks between frames. When the budget is exceeded the cache is cleared wholesale;
/// that keeps eviction predictable and avoids fragmenting the budget with
/// rarely-used sizes.
#[derive(Debug)]
pub struct GlyphCache {
    entries: HashMap<GlyphKey, GlyphRaster>,
    bytes: u64,
    budget_bytes: u64,
}

impl GlyphCache {
    /// Creates a cache with a byte budget.
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            budget_bytes: budget_bytes.max(64 * 1024),
        }
    }

    /// Bytes currently held by cached masks.
    pub fn bytes_used(&self) -> u64 {
        self.bytes
    }

    /// Configured budget in bytes.
    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Number of cached glyphs.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replaces the budget, clearing the cache if it is now too large.
    pub fn set_budget(&mut self, budget_bytes: u64) {
        self.budget_bytes = budget_bytes.max(64 * 1024);
        if self.bytes > self.budget_bytes {
            self.clear();
        }
    }

    /// Drops every cached glyph.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// Returns the rasterised glyph, rasterising it on first use.
    ///
    /// Returns `None` for glyphs without outlines (for example the space glyph) and
    /// for invalid glyph indices.
    pub fn rasterize(
        &mut self,
        font_id: u64,
        font: &Font,
        glyph: u16,
        size_px: f32,
    ) -> Option<&GlyphRaster> {
        if !size_px.is_finite() || size_px <= 0.0 {
            return None;
        }
        let size_quarters = (size_px * 4.0).round().clamp(1.0, 65535.0) as u16;
        let key = GlyphKey { font: font_id, glyph, size_quarters };
        if !self.entries.contains_key(&key) {
            if self.bytes > self.budget_bytes {
                self.clear();
            }
            let outline = font.outline(glyph).ok()?;
            if outline.is_empty() {
                return None;
            }
            let scale = (size_quarters as f32 / 4.0) / font.units_per_em().max(1.0);
            let transform = Affine2::from_scale(Vec2::splat(scale));
            let mask = rasterize(&outline, &transform, &RasterOptions::default())?;
            let raster = GlyphRaster {
                bearing_x: mask.left(),
                bearing_y: mask.top(),
                mask,
            };
            self.bytes += raster.mask.allocated_bytes() as u64;
            self.entries.insert(key, raster);
        }
        self.entries.get(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_is_sixteen_pixels() {
        let style = TextStyle::default();
        assert_eq!(style.em_size(), 16.0);
        assert_eq!(style.align, TextAlign::Left);
        assert_eq!(style.wrap, TextWrap::Word);
    }

    #[test]
    fn font_store_tracks_entries() {
        let mut store = FontStore::new();
        assert!(store.is_empty());
        store.clear();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn glyph_cache_starts_empty() {
        let mut cache = GlyphCache::new(1024);
        assert_eq!(cache.bytes_used(), 0);
        assert!(cache.is_empty());
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}

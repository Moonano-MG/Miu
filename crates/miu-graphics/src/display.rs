//! Retained display lists: the renderer-neutral command stream for one frame.
//!
//! Scenes build a [`DisplayList`] once per frame; the software renderer executes it
//! immediately, and a GPU bridge can translate the same operations into its own
//! command buffer. Operations carry their own blend mode and transform, so
//! compositing order is exactly submission order.

use miu_core::{Affine2, BlendMode, Color, Rect, Vec2};

use crate::batch::Sprite;
use crate::paint::{Paint, StrokeStyle};
use crate::path::{FillRule, Path};
use crate::text::PlacedGlyph;

/// Filled vector shape.
#[derive(Debug, Clone, PartialEq)]
pub struct FillOp {
    /// Outline in local space.
    pub path: Path,
    /// Local-to-world transform applied before the camera.
    pub transform: Affine2,
    /// Fill description.
    pub paint: Paint,
    /// Fill rule for self-intersections.
    pub rule: FillRule,
    /// How the result is composited.
    pub blend: BlendMode,
    /// World-space axis-aligned culling bounds, when the producer knows them.
    pub cull_bounds: Option<Rect>,
}

/// Stroked vector shape.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeOp {
    /// Outline in local space.
    pub path: Path,
    /// Local-to-world transform applied before the camera.
    pub transform: Affine2,
    /// Stroke width, caps, joins, and dash pattern.
    pub style: StrokeStyle,
    /// Stroke colour or gradient.
    pub paint: Paint,
    /// How the result is composited.
    pub blend: BlendMode,
    /// World-space axis-aligned culling bounds, when the producer knows them.
    pub cull_bounds: Option<Rect>,
}

/// Pre-shaped text ready for rasterisation.
///
/// Glyph positions come from [`crate::text::layout_text`] and are expressed in
/// pixels relative to the text block's top-left corner. The renderer multiplies
/// `transform` and the camera into those positions.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// Font asset identifier.
    pub font: u64,
    /// Em size in pixels used during layout.
    pub size: f32,
    /// Text colour, premultiplied.
    pub color: Color,
    /// Blending mode.
    pub blend: BlendMode,
    /// Local-to-world transform applied before the camera.
    pub transform: Affine2,
    /// Positioned glyphs.
    pub glyphs: Vec<PlacedGlyph>,
    /// World-space axis-aligned culling bounds, when the producer knows them.
    pub cull_bounds: Option<Rect>,
}

impl TextRun {
    /// Glyph advance width in pixels, including trailing spacing.
    pub fn advance_width(&self) -> f32 {
        if self.glyphs.is_empty() {
            return 0.0;
        }
        let mut maximum = 0.0f32;
        for glyph in &self.glyphs {
            maximum = maximum.max(glyph.x + glyph.advance);
        }
        maximum
    }
}

/// One drawable operation.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    /// Textured quad.
    Sprite(Sprite),
    /// Filled path.
    Fill(FillOp),
    /// Stroked path.
    Stroke(StrokeOp),
    /// Shaped text.
    Text(TextRun),
}

impl DrawOp {
    /// World-space culling bounds, when available.
    pub fn cull_bounds(&self) -> Option<Rect> {
        match self {
            Self::Sprite(sprite) => {
                let size = if sprite.size.x > 0.0 && sprite.size.y > 0.0 {
                    sprite.size
                } else {
                    Vec2::ZERO
                };
                let origin = Vec2::new(-size.x * sprite.pivot.x, -size.y * sprite.pivot.y);
                let matrix = sprite.transform;
                let corners = [
                    matrix.transform_point(origin),
                    matrix.transform_point(origin + Vec2::new(size.x, 0.0)),
                    matrix.transform_point(origin + size),
                    matrix.transform_point(origin + Vec2::new(0.0, size.y)),
                ];
                let mut minimum = corners[0];
                let mut maximum = corners[0];
                for corner in corners.iter().skip(1) {
                    minimum = minimum.min(*corner);
                    maximum = maximum.max(*corner);
                }
                Some(Rect::from_min_max(minimum, maximum))
            }
            Self::Fill(op) => op.cull_bounds,
            Self::Stroke(op) => op.cull_bounds,
            Self::Text(run) => run.cull_bounds,
        }
    }

    /// Approximate paint layer, used by tools that group operations.
    pub fn layer(&self) -> i32 {
        match self {
            Self::Sprite(sprite) => sprite.layer,
            _ => 0,
        }
    }
}

/// Ordered operations for one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayList {
    ops: Vec<DrawOp>,
}

impl DisplayList {
    /// Creates an empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes every operation while keeping allocated capacity.
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    /// Reserves capacity for `additional` operations.
    pub fn reserve(&mut self, additional: usize) {
        self.ops.reserve(additional);
    }

    /// Appends an operation.
    pub fn push(&mut self, op: DrawOp) {
        self.ops.push(op);
    }

    /// Appends every operation of an iterator.
    pub fn extend<I: IntoIterator<Item = DrawOp>>(&mut self, ops: I) {
        self.ops.extend(ops);
    }

    /// Number of operations.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Operations in submission order.
    pub fn ops(&self) -> &[DrawOp] {
        &self.ops
    }

    /// Mutable view of the operations, used by post-processing tools.
    pub fn ops_mut(&mut self) -> &mut Vec<DrawOp> {
        &mut self.ops
    }

    /// Number of sprite operations.
    pub fn sprite_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Sprite(_)))
            .count()
    }

    /// Number of text operations.
    pub fn text_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Text(_)))
            .count()
    }

    /// Number of positioned glyphs across all text operations.
    pub fn glyph_count(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                DrawOp::Text(run) => run.glyphs.len(),
                _ => 0,
            })
            .sum()
    }

    /// Drops operations whose culling bounds miss `visible`.
    ///
    /// Operations without bounds are always kept, because their producer did not
    /// provide enough information to cull safely.
    pub fn cull(&mut self, visible: Rect) {
        self.ops.retain(|op| match op.cull_bounds() {
            Some(bounds) => bounds.intersects(visible),
            None => true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Paint;
    use miu_core::Color;

    fn fill(rect: Rect, bounds: Option<Rect>) -> DrawOp {
        DrawOp::Fill(FillOp {
            path: Path::rect(rect),
            transform: Affine2::IDENTITY,
            paint: Paint::solid(Color::WHITE),
            rule: FillRule::NonZero,
            blend: BlendMode::SourceOver,
            cull_bounds: bounds,
        })
    }

    #[test]
    fn lists_track_counts() {
        let mut list = DisplayList::new();
        assert!(list.is_empty());
        list.push(fill(Rect::new(0.0, 0.0, 10.0, 10.0), None));
        list.push(DrawOp::Sprite(Sprite::new(1)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.sprite_count(), 1);
        assert_eq!(list.text_count(), 0);
        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn culling_drops_only_out_of_view_operations_with_bounds() {
        let mut list = DisplayList::new();
        list.push(fill(Rect::new(0.0, 0.0, 10.0, 10.0), Some(Rect::new(0.0, 0.0, 10.0, 10.0))));
        list.push(fill(Rect::new(500.0, 500.0, 10.0, 10.0), Some(Rect::new(500.0, 500.0, 10.0, 10.0))));
        list.push(fill(Rect::new(0.0, 0.0, 10.0, 10.0), None));
        list.cull(Rect::new(0.0, 0.0, 64.0, 64.0));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn sprite_bounds_follow_transforms() {
        let mut sprite = Sprite::new(1).with_size(Vec2::new(10.0, 20.0));
        sprite.transform = Affine2::from_translation(Vec2::new(5.0, 5.0));
        let bounds = DrawOp::Sprite(sprite).cull_bounds().expect("sprite has bounds");
        assert!(bounds.min().approx_eq(Vec2::new(5.0, 5.0), 0.001));
        assert!(bounds.max().approx_eq(Vec2::new(15.0, 25.0), 0.001));
    }
}

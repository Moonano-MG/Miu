//! Sprite instances and the sorted batch contract used by GPU bridges.
//!
//! A [`Sprite`] is one textured quad: geometry, atlas source rectangle, tint, and
//! painter layer. [`BatchBuilder`] orders submitted sprites by layer while keeping
//! submission order stable, then merges neighbours that share a texture into a
//! [`DrawBatch`]. GPU bridges upload textures by id and render each batch with one
//! pipeline bind; the CPU reference renderer consumes exactly the same structures.

use core::cmp::Ordering;

use miu_core::{Affine2, BlendMode, Camera2D, CameraError, Color, Transform2D, Vec2};

/// Pixel source rectangle inside a texture atlas.
///
/// Only the all-zero sentinel `{ x: 0, y: 0, width: 0, height: 0 }` means "the
/// whole texture". Atlas users should always pass explicit dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct SourceRect {
    /// Left source texel.
    pub x: u32,
    /// Top source texel.
    pub y: u32,
    /// Source width in texels, or zero only in the full-texture sentinel.
    pub width: u32,
    /// Source height in texels, or zero only in the full-texture sentinel.
    pub height: u32,
}

impl SourceRect {
    /// Full-texture source region, resolved against the bound texture when drawn.
    pub const FULL: Self = Self { x: 0, y: 0, width: 0, height: 0 };

    /// Creates a source rectangle.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Whether this is the full-texture sentinel.
    pub const fn is_full_sentinel(self) -> bool {
        self.x == 0 && self.y == 0 && self.width == 0 && self.height == 0
    }
}

/// Normalised sampling rectangle inside a [`SourceRect`].
///
/// Reversing an axis (for example `left = 1, right = 0`) mirrors sampling without
/// changing geometry, which is how flipped sprites are expressed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct UvRect {
    /// Left coordinate in `[0, 1]`.
    pub left: f32,
    /// Top coordinate in `[0, 1]`.
    pub top: f32,
    /// Right coordinate in `[0, 1]`.
    pub right: f32,
    /// Bottom coordinate in `[0, 1]`.
    pub bottom: f32,
}

impl UvRect {
    /// The full `[0, 1]` rectangle.
    pub const FULL: Self = Self { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 };

    /// Creates a normalised sampling rectangle.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    /// Horizontally mirrored rectangle.
    pub const fn flipped_x(self) -> Self {
        Self { left: self.right, top: self.top, right: self.left, bottom: self.bottom }
    }

    /// Vertically mirrored rectangle.
    pub const fn flipped_y(self) -> Self {
        Self { left: self.left, top: self.bottom, right: self.right, bottom: self.top }
    }
}

impl Default for UvRect {
    fn default() -> Self {
        Self::FULL
    }
}

/// A textured quad ready to be appended to a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    /// Host-owned texture identifier.
    pub texture: u64,
    /// Transform from sprite-local space into world space.
    ///
    /// A matrix rather than a decomposed transform, because parent nodes can combine
    /// non-uniform scale with rotation, which no TRS triple can express.
    pub transform: Affine2,
    /// Pixel source rectangle in the texture atlas.
    pub source: SourceRect,
    /// Additional normalised sampling rectangle inside [`Self::source`].
    pub uv: UvRect,
    /// Destination size in world units before [`Transform2D::scale`] is applied.
    ///
    /// A zero vector resolves to the source rectangle's texel size.
    pub size: Vec2,
    /// Normalised origin: `(0, 0)` is top-left, `(0.5, 1)` is bottom-centre.
    pub pivot: Vec2,
    /// Premultiplied tint.
    pub color: Color,
    /// How the sprite is composited with the framebuffer.
    pub blend: BlendMode,
    /// Painter layer; lower layers render first.
    pub layer: i32,
}

impl Sprite {
    /// Creates a full-texture sprite anchored at its top-left corner.
    pub fn new(texture: u64) -> Self {
        Self {
            texture,
            transform: Affine2::IDENTITY,
            source: SourceRect::FULL,
            uv: UvRect::FULL,
            size: Vec2::ZERO,
            pivot: Vec2::ZERO,
            color: Color::WHITE,
            blend: BlendMode::SourceOver,
            layer: 0,
        }
    }

    /// Creates a sprite positioned at `position` with the given size.
    pub fn at(texture: u64, position: Vec2, size: Vec2) -> Self {
        let mut sprite = Self::new(texture);
        sprite.transform = Affine2::from_translation(position);
        sprite.size = size;
        sprite
    }

    /// Sets the atlas source rectangle.
    #[must_use]
    pub const fn with_source(mut self, source: SourceRect) -> Self {
        self.source = source;
        self
    }

    /// Sets the destination size.
    #[must_use]
    pub const fn with_size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    /// Sets the pivot.
    #[must_use]
    pub const fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// Sets the premultiplied tint.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the blend mode.
    #[must_use]
    pub const fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Sets the painter layer.
    #[must_use]
    pub const fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }

    /// Sets the full local-to-world matrix.
    #[must_use]
    pub fn with_transform(mut self, transform: Affine2) -> Self {
        self.transform = transform;
        self
    }

    /// Sets the matrix from a translation/rotation/scale triple.
    #[must_use]
    pub fn with_trs(mut self, transform: Transform2D) -> Self {
        self.transform = transform.to_affine();
        self
    }

    /// Whether the sprite can produce visible pixels.
    pub fn is_visible(&self) -> bool {
        self.color.a > 0.0
            && self.transform.is_finite()
            && self.size.x >= 0.0
            && self.size.y >= 0.0
    }
}

/// Draw-sorting policy used by [`BatchBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatchMode {
    /// Preserve submission order within a layer; safe for transparent content.
    #[default]
    Stable,
    /// Group texture changes within a layer; only valid for opaque content.
    Opaque,
}

#[derive(Debug, Clone, Copy)]
struct QueuedSprite {
    sprite: Sprite,
    sequence: u64,
}

/// A contiguous texture/layer range inside [`FrameBatches::sprites`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct DrawBatch {
    /// Texture shared by every sprite in this range.
    pub texture: u64,
    /// Painter layer shared by every sprite in this range.
    pub layer: i32,
    /// Blend mode shared by every sprite in this range.
    pub blend: BlendMode,
    /// Index of the first sprite.
    pub first: usize,
    /// Number of sprites in the range.
    pub count: usize,
}

/// Sorted per-frame sprites and their contiguous draw batches.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameBatches {
    /// Sorted sprite instances.
    pub sprites: Vec<Sprite>,
    /// Contiguous state-compatible ranges of `sprites`.
    pub batches: Vec<DrawBatch>,
}

impl FrameBatches {
    /// Builds batches from an already-ordered sprite list.
    pub fn from_sprites(sprites: Vec<Sprite>) -> Self {
        let mut batches: Vec<DrawBatch> = Vec::new();
        for (index, sprite) in sprites.iter().enumerate() {
            match batches.last_mut() {
                Some(batch)
                    if batch.texture == sprite.texture
                        && batch.layer == sprite.layer
                        && batch.blend == sprite.blend =>
                {
                    batch.count += 1;
                }
                _ => batches.push(DrawBatch {
                    texture: sprite.texture,
                    layer: sprite.layer,
                    blend: sprite.blend,
                    first: index,
                    count: 1,
                }),
            }
        }
        Self { sprites, batches }
    }

    /// Number of sprite instances.
    pub fn sprite_count(&self) -> usize {
        self.sprites.len()
    }

    /// Number of draw batches.
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// Whether the frame contains no sprites.
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

/// CPU-side command collector for one frame.
#[derive(Debug, Default)]
pub struct BatchBuilder {
    mode: BatchMode,
    queued: Vec<QueuedSprite>,
    next_sequence: u64,
}

impl BatchBuilder {
    /// Starts an empty frame collector.
    pub fn new(mode: BatchMode) -> Self {
        Self { mode, queued: Vec::new(), next_sequence: 0 }
    }

    /// Sorting policy in force.
    pub fn mode(&self) -> BatchMode {
        self.mode
    }

    /// Number of queued sprites.
    pub fn len(&self) -> usize {
        self.queued.len()
    }

    /// Whether nothing has been queued for this frame.
    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    /// Reserves capacity without submitting a draw.
    pub fn reserve(&mut self, additional: usize) {
        self.queued.reserve(additional);
    }

    /// Appends a sprite. Relative order is preserved in [`BatchMode::Stable`].
    pub fn push(&mut self, sprite: Sprite) {
        self.queued.push(QueuedSprite { sprite, sequence: self.next_sequence });
        self.next_sequence = self.next_sequence.saturating_add(1);
    }

    /// Appends every sprite of an iterator.
    pub fn extend<I: IntoIterator<Item = Sprite>>(&mut self, sprites: I) {
        for sprite in sprites {
            self.push(sprite);
        }
    }

    /// Clears submitted sprites while retaining allocated capacity.
    pub fn begin_frame(&mut self) {
        self.queued.clear();
        self.next_sequence = 0;
    }

    /// Sorts queued sprites and groups adjacent matching texture/layer pairs.
    pub fn finish(mut self) -> FrameBatches {
        self.queued.sort_by(|left, right| {
            let layer = left.sprite.layer.cmp(&right.sprite.layer);
            if layer != Ordering::Equal {
                return layer;
            }
            match self.mode {
                BatchMode::Stable => left.sequence.cmp(&right.sequence),
                BatchMode::Opaque => left
                    .sprite
                    .texture
                    .cmp(&right.sprite.texture)
                    .then_with(|| left.sequence.cmp(&right.sequence)),
            }
        });
        let sprites: Vec<Sprite> = self.queued.iter().map(|item| item.sprite).collect();
        FrameBatches::from_sprites(sprites)
    }
}

/// Applies a camera to a sprite and returns the four world-space corners.
///
/// Corner order is top-left, top-right, bottom-left, bottom-right in screen space.
pub fn sprite_screen_corners(camera: &Camera2D, sprite: &Sprite, texture_size: Vec2) -> Result<[Vec2; 4], CameraError> {
    let view = camera.view_affine()?;
    let size = if sprite.size.x > 0.0 && sprite.size.y > 0.0 {
        sprite.size
    } else {
        texture_size
    };
    let origin = Vec2::new(-size.x * sprite.pivot.x, -size.y * sprite.pivot.y);
    let local = [
        origin,
        Vec2::new(origin.x + size.x, origin.y),
        Vec2::new(origin.x, origin.y + size.y),
        Vec2::new(origin.x + size.x, origin.y + size.y),
    ];
    let transform = sprite.transform;
    let mut corners = [Vec2::ZERO; 4];
    for (index, point) in local.iter().enumerate() {
        corners[index] = view.transform_point(transform.transform_point(*point));
    }
    Ok(corners)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(texture: u64, layer: i32) -> Sprite {
        Sprite::new(texture).with_layer(layer)
    }

    #[test]
    fn stable_batches_never_reorder_transparent_sprites() {
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 0));
        builder.push(sprite(2, 0));
        let frame = builder.finish();
        assert_eq!(
            frame.sprites.iter().map(|item| item.texture).collect::<Vec<_>>(),
            vec![2, 1, 2]
        );
        assert_eq!(frame.batches.len(), 3);
    }

    #[test]
    fn opaque_batches_group_textures_per_layer() {
        let mut builder = BatchBuilder::new(BatchMode::Opaque);
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 0));
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 1));
        let frame = builder.finish();
        assert_eq!(
            frame.sprites.iter().map(|item| item.texture).collect::<Vec<_>>(),
            vec![1, 2, 2, 1]
        );
        assert_eq!(frame.batches.len(), 3);
        assert_eq!(frame.batches[1].count, 2);
        assert_eq!(frame.sprite_count(), 4);
    }

    #[test]
    fn layers_render_in_ascending_order() {
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(sprite(1, 5));
        builder.push(sprite(2, -1));
        let frame = builder.finish();
        assert_eq!(frame.sprites[0].texture, 2);
        assert_eq!(frame.sprites[1].texture, 1);
    }

    #[test]
    fn begin_frame_keeps_capacity() {
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(sprite(1, 0));
        builder.begin_frame();
        assert!(builder.is_empty());
        builder.push(sprite(2, 0));
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn uv_flips_mirror_sampling_only() {
        let uv = UvRect::FULL.flipped_x();
        assert_eq!(uv, UvRect::new(1.0, 0.0, 0.0, 1.0));
        assert_eq!(UvRect::FULL.flipped_y(), UvRect::new(0.0, 1.0, 1.0, 0.0));
    }

    #[test]
    fn screen_corners_follow_camera_zoom() {
        let camera = Camera2D::new(Vec2::new(100.0, 100.0)).unwrap();
        let sprite = Sprite::new(1).with_size(Vec2::new(10.0, 10.0));
        let corners = sprite_screen_corners(&camera, &sprite, Vec2::new(10.0, 10.0)).unwrap();
        assert!(corners[0].approx_eq(Vec2::new(45.0, 45.0), 0.001));
        assert!(corners[3].approx_eq(Vec2::new(55.0, 55.0), 0.001));
    }
}

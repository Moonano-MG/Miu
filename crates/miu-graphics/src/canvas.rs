//! CPU render target.
//!
//! A [`Canvas`] owns a premultiplied RGBA buffer and implements the drawing
//! operations the software renderer needs: clearing, sprite compositing with
//! atlas-safe sampling, path filling through the coverage rasteriser, stroking, and
//! PNG export. GPU bridges mirror these operations with their own pipelines.
//!
//! Every draw call accepts an optional device-space clip rectangle and a
//! [`BlendMode`], so a host can render UI masks or additive light without changing
//! the framebuffer configuration.

use miu_core::{Affine2, BlendMode, Camera2D, CameraError, Color, Rect, Rgba8, Vec2};

use crate::batch::{FrameBatches, SourceRect, Sprite};
use crate::codec::{self, ImageError};
use crate::paint::{Paint, StrokeStyle};
use crate::path::{FillRule, Path};
use crate::raster::{rasterize, CoverageMask, RasterOptions};
use crate::texture::{Texture, TextureError, TextureStore};

/// Failure while drawing into a canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasError {
    /// The camera could not produce a transform.
    Camera(CameraError),
    /// A sprite transform, tint, size, or pivot was invalid.
    InvalidSpriteTransform,
    /// A sprite source rectangle exceeded its texture.
    InvalidSourceRect,
    /// A clear colour was not premultiplied.
    InvalidPremultipliedColor,
    /// A frame referenced a texture id absent from the store.
    MissingTexture(u64),
    /// Canvas storage could not be allocated.
    Texture(TextureError),
}

impl std::error::Error for CanvasError {}

impl core::fmt::Display for CanvasError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Camera(error) => write!(formatter, "camera error: {}", error.id()),
            Self::InvalidSpriteTransform => formatter.write_str("invalid sprite transform"),
            Self::InvalidSourceRect => formatter.write_str("sprite source rectangle is invalid"),
            Self::InvalidPremultipliedColor => formatter.write_str("colour is not premultiplied"),
            Self::MissingTexture(id) => write!(formatter, "texture {id} is not loaded"),
            Self::Texture(error) => write!(formatter, "canvas storage error: {error}"),
        }
    }
}

impl From<TextureError> for CanvasError {
    fn from(error: TextureError) -> Self {
        Self::Texture(error)
    }
}

/// A mutable CPU render target with premultiplied RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
    bilinear: bool,
    gradient_dithering: bool,
}

impl Canvas {
    /// Allocates a transparent canvas.
    pub fn new(width: u32, height: u32) -> Result<Self, TextureError> {
        let texture = Texture::solid(width, height, Rgba8::TRANSPARENT)?;
        Ok(Self {
            width: texture.width(),
            height: texture.height(),
            pixels: texture.pixels().to_vec(),
            bilinear: true,
            gradient_dithering: false,
        })
    }

    /// Wraps an existing texture as a canvas.
    pub fn from_texture(texture: &Texture) -> Self {
        Self {
            width: texture.width(),
            height: texture.height(),
            pixels: texture.pixels().to_vec(),
            bilinear: true,
            gradient_dithering: false,
        }
    }

    /// Canvas width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Canvas size as a vector.
    pub fn size_f32(&self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }

    /// Full canvas rectangle in device space.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    /// Row-major premultiplied pixels.
    pub fn pixels(&self) -> &[Rgba8] {
        &self.pixels
    }

    /// Mutable pixel storage.
    pub fn pixels_mut(&mut self) -> &mut [Rgba8] {
        &mut self.pixels
    }

    /// Returns a pixel, or `None` outside the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    /// Enables or disables bilinear sprite sampling.
    ///
    /// The renderer sets this from [`miu_core::QualitySettings::bilinear_sampling`].
    pub fn set_bilinear(&mut self, bilinear: bool) {
        self.bilinear = bilinear;
    }

    /// Enables or disables gradient dithering.
    pub fn set_gradient_dithering(&mut self, dithering: bool) {
        self.gradient_dithering = dithering;
    }

    /// Whether bilinear sprite sampling is enabled.
    pub const fn bilinear(&self) -> bool {
        self.bilinear
    }

    /// Fills every pixel with a premultiplied colour.
    pub fn clear(&mut self, color: Rgba8) -> Result<(), CanvasError> {
        if !color.is_premultiplied() {
            return Err(CanvasError::InvalidPremultipliedColor);
        }
        self.pixels.fill(color);
        Ok(())
    }

    /// Composites one texture at an integer device position with source-over.
    pub fn blit_texture(&mut self, texture: &Texture, x: i32, y: i32) {
        for source_y in 0..texture.height() {
            for source_x in 0..texture.width() {
                let destination_x = x + source_x as i32;
                let destination_y = y + source_y as i32;
                if destination_x < 0 || destination_y < 0 {
                    continue;
                }
                let destination_x = destination_x as u32;
                let destination_y = destination_y as u32;
                if destination_x >= self.width || destination_y >= self.height {
                    continue;
                }
                let foreground = texture.pixel(source_x, source_y).unwrap_or(Rgba8::TRANSPARENT);
                if foreground.a == 0 {
                    continue;
                }
                self.blend_at(destination_x, destination_y, foreground, BlendMode::SourceOver);
            }
        }
    }

    /// Draws one sprite through a camera using the canvas sampling settings.
    pub fn draw_sprite(
        &mut self,
        camera: &Camera2D,
        sprite: &Sprite,
        texture: &Texture,
    ) -> Result<(), CanvasError> {
        self.draw_sprite_clipped(camera, sprite, texture, None)
    }

    /// Draws one sprite, optionally clipped to a device-space rectangle.
    pub fn draw_sprite_clipped(
        &mut self,
        camera: &Camera2D,
        sprite: &Sprite,
        texture: &Texture,
        clip: Option<Rect>,
    ) -> Result<(), CanvasError> {
        let resolved = ResolvedSprite::new(sprite, texture)?;
        let view = camera.view_affine().map_err(CanvasError::Camera)?;
        let matrix = sprite.transform.then(view);
        if !matrix.is_finite() {
            return Err(CanvasError::InvalidSpriteTransform);
        }
        let inverse = matrix.inverse().ok_or(CanvasError::InvalidSpriteTransform)?;
        let corners = resolved.corners();
        let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for corner in corners {
            let screen = matrix.transform_point(corner);
            minimum = minimum.min(screen);
            maximum = maximum.max(screen);
        }
        let bounds = Rect::from_min_max(minimum, maximum);
        let bounds = match clip {
            Some(clip) if clip.is_valid() => bounds.intersection(clip).unwrap_or(Rect::ZERO),
            _ => bounds,
        };
        if bounds.is_empty() {
            return Ok(());
        }
        let (min_x, min_y, max_x, max_y) = bounds.to_pixel_bounds(self.width, self.height);
        if min_x >= max_x || min_y >= max_y {
            return Ok(());
        }
        let step_x = inverse.transform_vector(Vec2::X);
        let step_y = inverse.transform_vector(Vec2::Y);
        let mut row_local = inverse.transform_point(Vec2::new(
            min_x as f32 + 0.5,
            min_y as f32 + 0.5,
        ));
        let bilinear = self.bilinear;
        for y in min_y..max_y {
            let mut local = row_local;
            for x in min_x..max_x {
                let u = (local.x - resolved.left) / resolved.size.x;
                let v = (local.y - resolved.top) / resolved.size.y;
                if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                    let texel_x = resolved.source.x as f32
                        + resolved.source.width as f32
                            * (sprite.uv.left + (sprite.uv.right - sprite.uv.left) * u);
                    let texel_y = resolved.source.y as f32
                        + resolved.source.height as f32
                            * (sprite.uv.top + (sprite.uv.bottom - sprite.uv.top) * v);
                    let foreground = if bilinear {
                        texture.sample_bilinear_in_source(texel_x, texel_y, resolved.source)
                    } else {
                        texture.sample_nearest_in_source(texel_x, texel_y, resolved.source)
                    }
                    .modulate(sprite.color);
                    if foreground.a > 0 {
                        self.blend_at(x, y, foreground, sprite.blend);
                    }
                }
                local += step_x;
            }
            row_local += step_y;
        }
        Ok(())
    }

    /// Draws every sprite of a prepared frame in its sorted order.
    ///
    /// The whole frame is validated before any pixel changes, so a missing texture
    /// never leaves a half-rendered canvas behind.
    pub fn draw_batches(
        &mut self,
        camera: &Camera2D,
        frame: &FrameBatches,
        textures: &TextureStore,
    ) -> Result<(), CanvasError> {
        camera.view_affine().map_err(CanvasError::Camera)?;
        for sprite in &frame.sprites {
            let texture = textures
                .get(sprite.texture)
                .ok_or(CanvasError::MissingTexture(sprite.texture))?;
            ResolvedSprite::new(sprite, texture)?;
        }
        for sprite in &frame.sprites {
            if let Some(texture) = textures.get(sprite.texture) {
                self.draw_sprite(camera, sprite, texture)?;
            }
        }
        Ok(())
    }

    /// Fills a path through `transform` with a paint.
    ///
    /// `transform` maps path-local coordinates to device coordinates. Gradient
    /// paints are sampled in path-local space, so they follow the geometry.
    pub fn fill_path(
        &mut self,
        path: &Path,
        transform: &Affine2,
        paint: &Paint,
        rule: FillRule,
        blend: BlendMode,
        clip: Option<Rect>,
    ) {
        let options = RasterOptions { fill_rule: rule, clip };
        let Some(mask) = rasterize(path, transform, &options) else {
            return;
        };
        let Some(inverse) = transform.inverse() else {
            // A degenerate transform has no interior; fill a flat colour instead of
            // sampling a gradient that cannot be mapped back.
            self.composite_mask_color(&mask, color_of(paint), blend, clip);
            return;
        };
        match paint {
            Paint::Solid(color) => self.composite_mask_color(&mask, *color, blend, clip),
            _ => self.composite_mask_paint(&mask, paint, &inverse, blend, clip),
        }
    }

    /// Strokes a path through `transform`, converting it to a fill first.
    pub fn stroke_path(
        &mut self,
        path: &Path,
        transform: &Affine2,
        style: &StrokeStyle,
        paint: &Paint,
        blend: BlendMode,
        clip: Option<Rect>,
    ) {
        if !style.is_visible() {
            return;
        }
        let outline = path.stroke_to_fill(style);
        if outline.is_empty() {
            return;
        }
        self.fill_path(&outline, transform, paint, FillRule::NonZero, blend, clip);
    }

    /// Composites a coverage mask with one colour.
    pub fn composite_mask_color(
        &mut self,
        mask: &CoverageMask,
        color: Color,
        blend: BlendMode,
        clip: Option<Rect>,
    ) {
        let (min_x, min_y, max_x, max_y) = self.mask_bounds(mask, clip);
        for y in min_y..max_y {
            for x in min_x..max_x {
                let coverage = mask.coverage((x as i32 - mask.left()) as u32, (y as i32 - mask.top()) as u32);
                if coverage == 0 {
                    continue;
                }
                let alpha = coverage as f32 / 255.0;
                let foreground = color.multiply_alpha(alpha).to_rgba8();
                if foreground.a > 0 {
                    self.blend_at(x, y, foreground, blend);
                }
            }
        }
    }

    /// Composites a coverage mask with a paint sampled through `inverse`.
    ///
    /// `inverse` maps device coordinates back into paint space, which is the same
    /// space the geometry was defined in.
    pub fn composite_mask_paint(
        &mut self,
        mask: &CoverageMask,
        paint: &Paint,
        inverse: &Affine2,
        blend: BlendMode,
        clip: Option<Rect>,
    ) {
        let dither_enabled = self.gradient_dithering && !paint.is_solid();
        let (min_x, min_y, max_x, max_y) = self.mask_bounds(mask, clip);
        for y in min_y..max_y {
            for x in min_x..max_x {
                let coverage = mask.coverage((x as i32 - mask.left()) as u32, (y as i32 - mask.top()) as u32);
                if coverage == 0 {
                    continue;
                }
                let device_point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let local_point = inverse.transform_point(device_point);
                let dither = if dither_enabled {
                    bayer_offset(x, y)
                } else {
                    0.0
                };
                let sample = paint.sample(local_point, dither);
                let alpha = coverage as f32 / 255.0;
                let foreground = sample.multiply_alpha(alpha).to_rgba8();
                if foreground.a > 0 {
                    self.blend_at(x, y, foreground, blend);
                }
            }
        }
    }

    /// Composites a coverage mask at an explicit device position.
    ///
    /// The text renderer uses this to blit cached glyph masks: the mask stores its
    /// own bearing offsets, and the caller supplies the pen position.
    pub fn blend_mask_at(
        &mut self,
        mask: &CoverageMask,
        x: i32,
        y: i32,
        color: Color,
        blend: BlendMode,
        clip: Option<Rect>,
    ) {
        for mask_y in 0..mask.height() {
            for mask_x in 0..mask.width() {
                let coverage = mask.coverage(mask_x, mask_y);
                if coverage == 0 {
                    continue;
                }
                let device_x = x + mask_x as i32;
                let device_y = y + mask_y as i32;
                if device_x < 0 || device_y < 0 {
                    continue;
                }
                let device_x = device_x as u32;
                let device_y = device_y as u32;
                if device_x >= self.width || device_y >= self.height {
                    continue;
                }
                if let Some(clip) = clip {
                    if !clip.contains(Vec2::new(device_x as f32 + 0.5, device_y as f32 + 0.5)) {
                        continue;
                    }
                }
                let foreground = color.multiply_alpha(coverage as f32 / 255.0).to_rgba8();
                if foreground.a > 0 {
                    self.blend_at(device_x, device_y, foreground, blend);
                }
            }
        }
    }

    /// Converts the canvas into an owned texture.
    pub fn to_texture(&self) -> Texture {
        Texture::new(self.width, self.height, self.pixels.clone())
            .expect("canvas pixels are always premultiplied and correctly sized")
    }

    /// Encodes the canvas as a PNG image.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        codec::encode_png(&self.to_texture())
    }

    /// Encodes the canvas as straight-alpha RGBA8 bytes.
    pub fn to_rgba8_bytes(&self) -> Vec<u8> {
        self.to_texture().to_rgba8_bytes()
    }

    fn mask_bounds(&self, mask: &CoverageMask, clip: Option<Rect>) -> (u32, u32, u32, u32) {
        let bounds = match clip {
            Some(clip) if clip.is_valid() => mask.bounds().intersection(clip).unwrap_or(Rect::ZERO),
            _ => mask.bounds(),
        };
        bounds.to_pixel_bounds(self.width, self.height)
    }

    fn blend_at(&mut self, x: u32, y: u32, foreground: Rgba8, blend: BlendMode) {
        let index = y as usize * self.width as usize + x as usize;
        if index >= self.pixels.len() {
            return;
        }
        self.pixels[index] = foreground.blend(self.pixels[index], blend);
    }
}

fn color_of(paint: &Paint) -> Color {
    paint.solid_color().unwrap_or(Color::TRANSPARENT)
}

/// Ordered 4x4 Bayer offset in `[-0.5, 0.5)` gradient steps, scaled for 8 bits.
fn bayer_offset(x: u32, y: u32) -> f32 {
    const BAYER: [[i8; 4]; 4] = [
        [0, 8, 2, 10],
        [12, 4, 14, 6],
        [3, 11, 1, 9],
        [15, 7, 13, 5],
    ];
    let value = BAYER[(y % 4) as usize][(x % 4) as usize] as f32;
    (value / 16.0 - 0.5) / 255.0
}

#[derive(Debug, Clone, Copy)]
struct ResolvedSprite {
    source: SourceRect,
    size: Vec2,
    left: f32,
    top: f32,
}

impl ResolvedSprite {
    fn new(sprite: &Sprite, texture: &Texture) -> Result<Self, CanvasError> {
        if !sprite.transform.is_finite()
            || !sprite.pivot.is_finite()
            || !sprite.uv.left.is_finite()
            || !sprite.uv.top.is_finite()
            || !sprite.uv.right.is_finite()
            || !sprite.uv.bottom.is_finite()
            || !sprite.size.is_finite()
            || !sprite.color.is_finite()
        {
            return Err(CanvasError::InvalidSpriteTransform);
        }
        let source = if sprite.source.is_full_sentinel() {
            SourceRect {
                x: 0,
                y: 0,
                width: texture.width(),
                height: texture.height(),
            }
        } else {
            sprite.source
        };
        let right = source
            .x
            .checked_add(source.width)
            .ok_or(CanvasError::InvalidSourceRect)?;
        let bottom = source
            .y
            .checked_add(source.height)
            .ok_or(CanvasError::InvalidSourceRect)?;
        if source.width == 0 || source.height == 0 || right > texture.width() || bottom > texture.height() {
            return Err(CanvasError::InvalidSourceRect);
        }
        let size = if sprite.size.x == 0.0 && sprite.size.y == 0.0 {
            Vec2::new(source.width as f32, source.height as f32)
        } else {
            sprite.size
        };
        if size.x <= 0.0 || size.y <= 0.0 || !size.is_finite() {
            return Err(CanvasError::InvalidSpriteTransform);
        }
        Ok(Self {
            source,
            size,
            left: -size.x * sprite.pivot.x,
            top: -size.y * sprite.pivot.y,
        })
    }

    fn corners(self) -> [Vec2; 4] {
        let right = self.left + self.size.x;
        let bottom = self.top + self.size.y;
        [
            Vec2::new(self.left, self.top),
            Vec2::new(right, self.top),
            Vec2::new(self.left, bottom),
            Vec2::new(right, bottom),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::GradientStop;
    use crate::texture::Texture;

    fn canvas() -> Canvas {
        Canvas::new(16, 16).expect("canvas allocates")
    }

    #[test]
    fn clear_rejects_unpremultiplied_colours() {
        let mut canvas = canvas();
        assert_eq!(
            canvas.clear(Rgba8::new(255, 0, 0, 128)),
            Err(CanvasError::InvalidPremultipliedColor)
        );
        canvas.clear(Rgba8::new(255, 0, 0, 255)).unwrap();
        assert_eq!(canvas.pixel(0, 0), Some(Rgba8::new(255, 0, 0, 255)));
    }

    #[test]
    fn sprite_draw_respects_pivot_and_scale() {
        let mut canvas = canvas();
        let texture = Texture::solid(4, 4, Rgba8::WHITE).unwrap();
        let camera = Camera2D::new(Vec2::new(16.0, 16.0)).unwrap();
        let sprite = Sprite::new(1)
            .with_size(Vec2::new(8.0, 8.0))
            .with_pivot(Vec2::splat(0.5));
        canvas.draw_sprite(&camera, &sprite, &texture).unwrap();
        assert_eq!(canvas.pixel(8, 8), Some(Rgba8::WHITE));
        assert_eq!(canvas.pixel(1, 1), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn fill_path_covers_inside_and_skips_outside() {
        let mut canvas = canvas();
        let path = Path::rect(Rect::new(2.0, 2.0, 6.0, 6.0));
        canvas.fill_path(
            &path,
            &Affine2::IDENTITY,
            &Paint::solid(Color::WHITE),
            FillRule::NonZero,
            BlendMode::SourceOver,
            None,
        );
        assert_eq!(canvas.pixel(4, 4), Some(Rgba8::WHITE));
        assert_eq!(canvas.pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn gradient_paint_varies_across_the_shape() {
        let mut canvas = canvas();
        let path = Path::rect(Rect::new(0.0, 0.0, 16.0, 16.0));
        let paint = Paint::linear(
            Vec2::ZERO,
            Vec2::new(16.0, 0.0),
            vec![
                GradientStop::new(0.0, Color::BLACK),
                GradientStop::new(1.0, Color::WHITE),
            ],
            crate::paint::Spread::Pad,
        )
        .unwrap();
        canvas.fill_path(
            &path,
            &Affine2::IDENTITY,
            &paint,
            FillRule::NonZero,
            BlendMode::SourceOver,
            None,
        );
        let left = canvas.pixel(1, 8).unwrap().r;
        let right = canvas.pixel(14, 8).unwrap().r;
        assert!(left < right, "gradient ran backwards: {left} vs {right}");
    }

    #[test]
    fn clipped_draws_leave_the_clip_area_untouched() {
        let mut canvas = canvas();
        let path = Path::rect(Rect::new(0.0, 0.0, 16.0, 16.0));
        canvas.fill_path(
            &path,
            &Affine2::IDENTITY,
            &Paint::solid(Color::WHITE),
            FillRule::NonZero,
            BlendMode::SourceOver,
            Some(Rect::new(0.0, 0.0, 8.0, 8.0)),
        );
        assert_eq!(canvas.pixel(4, 4), Some(Rgba8::WHITE));
        assert_eq!(canvas.pixel(12, 12), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn stroked_line_draws_pixels_between_endpoints() {
        let mut canvas = canvas();
        let line = Path::polyline(&[Vec2::new(2.0, 8.0), Vec2::new(14.0, 8.0)], false);
        canvas.stroke_path(
            &line,
            &Affine2::IDENTITY,
            &StrokeStyle::new(3.0),
            &Paint::solid(Color::WHITE),
            BlendMode::SourceOver,
            None,
        );
        assert!(canvas.pixel(8, 8).unwrap().a > 200);
        assert_eq!(canvas.pixel(8, 2), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn png_export_produces_a_signature() {
        let mut canvas = canvas();
        canvas.clear(Rgba8::WHITE).unwrap();
        let png = canvas.encode_png().expect("png encodes");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn missing_textures_fail_before_pixels_change() {
        let mut canvas = canvas();
        canvas.clear(Rgba8::WHITE).unwrap();
        let camera = Camera2D::new(Vec2::new(16.0, 16.0)).unwrap();
        let frame = FrameBatches::from_sprites(vec![Sprite::new(42)]);
        let textures = TextureStore::new();
        assert_eq!(
            canvas.draw_batches(&camera, &frame, &textures),
            Err(CanvasError::MissingTexture(42))
        );
        assert_eq!(canvas.pixel(3, 3), Some(Rgba8::WHITE));
    }
}

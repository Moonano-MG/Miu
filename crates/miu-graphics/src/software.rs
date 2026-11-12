//! CPU reference renderer.
//!
//! [`SoftwareRenderer`] owns a framebuffer, the asset stores, a clip rectangle, and a
//! glyph cache. It executes a [`DisplayList`] through a [`Camera2D`] and records
//! per-frame statistics. The renderer is deterministic and needs no graphics driver,
//! which makes it the reference for GPU bridges, CI snapshots, and headless tools.
//!
//! Text is drawn from cached coverage masks when the transform is axis-aligned and
//! positive; rotated or mirrored text falls back to filling glyph outlines exactly, so
//! quality never depends on the cache fast path.

use miu_core::{
    Affine2, BlendMode, Camera2D, CameraError, Color, QualityPreset, QualitySettings, Rect, Rgba8,
    Vec2,
};

use crate::assets::Assets;
use crate::canvas::{Canvas, CanvasError};
use crate::codec::ImageError;
use crate::display::{DisplayList, DrawOp, TextRun};
use crate::paint::Paint;
use crate::path::FillRule;
use crate::text::GlyphCache;
use crate::texture::{Texture, TextureError};

/// Failure while rendering a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The framebuffer could not be allocated or resized.
    Texture(TextureError),
    /// A drawing operation was rejected by the canvas.
    Canvas(CanvasError),
    /// The camera could not produce a transform.
    Camera(CameraError),
    /// A sprite referenced a texture that is not loaded.
    MissingTexture(u64),
    /// A text operation referenced a font that is not loaded.
    MissingFont(u64),
}

impl std::error::Error for RenderError {}

impl core::fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Texture(error) => write!(formatter, "framebuffer error: {error}"),
            Self::Canvas(error) => write!(formatter, "draw error: {error}"),
            Self::Camera(error) => write!(formatter, "camera error: {}", error.id()),
            Self::MissingTexture(id) => write!(formatter, "texture {id} is not loaded"),
            Self::MissingFont(id) => write!(formatter, "font {id} is not loaded"),
        }
    }
}

impl From<TextureError> for RenderError {
    fn from(error: TextureError) -> Self {
        Self::Texture(error)
    }
}

impl From<CanvasError> for RenderError {
    fn from(error: CanvasError) -> Self {
        Self::Canvas(error)
    }
}

impl From<CameraError> for RenderError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

/// Counters for the most recent frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Sprite operations executed.
    pub sprites: u32,
    /// Filled paths executed.
    pub fills: u32,
    /// Stroked paths executed.
    pub strokes: u32,
    /// Glyphs rasterised or blitted.
    pub glyphs: u32,
    /// Operations skipped because of culling or clipping.
    pub skipped: u32,
    /// Glyph outline fills that bypassed the cache (rotated or mirrored text).
    pub uncached_glyphs: u32,
    /// Estimated covered pixel area across all operations.
    pub covered_pixels: u64,
}

impl RenderStats {
    /// Total operations that produced pixels.
    pub fn operations(&self) -> u32 {
        self.sprites + self.fills + self.strokes + self.glyphs
    }
}

/// CPU renderer that draws display lists into a framebuffer.
#[derive(Debug)]
pub struct SoftwareRenderer {
    assets: Assets,
    canvas: Canvas,
    clip: Option<Rect>,
    stats: RenderStats,
    settings: QualitySettings,
    glyphs: GlyphCache,
}

impl SoftwareRenderer {
    /// Creates a transparent framebuffer with default (`Medium`) quality.
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        Self::with_settings(width, height, QualitySettings::default())
    }

    /// Creates a transparent framebuffer with explicit quality settings.
    pub fn with_settings(
        width: u32,
        height: u32,
        settings: QualitySettings,
    ) -> Result<Self, RenderError> {
        let mut canvas = Canvas::new(width, height)?;
        canvas.set_bilinear(settings.bilinear_sampling);
        canvas.set_gradient_dithering(settings.gradient_dithering);
        Ok(Self {
            assets: Assets::new(),
            canvas,
            clip: None,
            stats: RenderStats::default(),
            glyphs: GlyphCache::new(settings.glyph_cache_bytes),
            settings,
        })
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.canvas.width()
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.canvas.height()
    }

    /// Current quality settings.
    pub fn settings(&self) -> QualitySettings {
        self.settings
    }

    /// Applies a quality preset, updating sampling, dithering, and cache budget.
    pub fn set_preset(&mut self, preset: QualityPreset) {
        self.settings = QualitySettings::for_preset(preset);
        self.canvas.set_bilinear(self.settings.bilinear_sampling);
        self.canvas.set_gradient_dithering(self.settings.gradient_dithering);
        self.glyphs.set_budget(self.settings.glyph_cache_bytes);
    }

    /// Asset stores used by this renderer.
    pub fn assets(&self) -> &Assets {
        &self.assets
    }

    /// Mutable asset stores; upload textures and fonts here.
    pub fn assets_mut(&mut self) -> &mut Assets {
        &mut self.assets
    }

    /// The framebuffer.
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// Mutable framebuffer, used by diagnostic overlays.
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Glyph cache used by text operations.
    pub fn glyph_cache(&self) -> &GlyphCache {
        &self.glyphs
    }

    /// Current clip rectangle in device space.
    pub fn clip(&self) -> Option<Rect> {
        self.clip
    }

    /// Restricts every subsequent draw to `clip`, or removes the clip with `None`.
    pub fn set_clip(&mut self, clip: Option<Rect>) {
        self.clip = clip.filter(|rect| rect.is_valid() && !rect.is_empty());
    }

    /// Statistics for the most recent frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Resets per-frame statistics.
    pub fn reset_stats(&mut self) {
        self.stats = RenderStats::default();
    }

    /// Resizes the framebuffer, clearing its contents.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::Texture(TextureError::EmptyDimensions));
        }
        self.canvas = Canvas::new(width, height)?;
        self.canvas.set_bilinear(self.settings.bilinear_sampling);
        self.canvas.set_gradient_dithering(self.settings.gradient_dithering);
        Ok(())
    }

    /// Clears the framebuffer and resets statistics.
    ///
    /// Passing `None` leaves the previous contents in place, which is what transient
    /// overlays need.
    pub fn begin_frame(&mut self, clear: Option<Rgba8>) -> Result<(), RenderError> {
        self.reset_stats();
        if let Some(color) = clear {
            self.canvas.clear(color)?;
        }
        Ok(())
    }

    /// Draws a display list through a camera.
    ///
    /// Every operation is validated before it is executed, so a missing texture or
    /// font returns an error instead of leaving a partially composited frame.
    pub fn draw(&mut self, camera: &Camera2D, list: &DisplayList) -> Result<(), RenderError> {
        let view = camera.view_affine()?;
        for op in list.ops() {
            match op {
                DrawOp::Sprite(sprite) => {
                    let Some(texture) = self.assets.texture(sprite.texture) else {
                        return Err(RenderError::MissingTexture(sprite.texture));
                    };
                    if let Some(bounds) = op.cull_bounds() {
                        if !self.world_bounds_visible(&view, bounds) {
                            self.stats.skipped += 1;
                            continue;
                        }
                    }
                    let clip = self.clip;
                    self.canvas.draw_sprite_clipped(camera, sprite, texture, clip)?;
                    self.stats.sprites += 1;
                }
                DrawOp::Fill(fill) => {
                    let device = fill.transform.then(view);
                    self.canvas.fill_path(
                        &fill.path,
                        &device,
                        &fill.paint,
                        fill.rule,
                        fill.blend,
                        self.clip,
                    );
                    self.stats.fills += 1;
                }
                DrawOp::Stroke(stroke) => {
                    let device = stroke.transform.then(view);
                    self.canvas.stroke_path(
                        &stroke.path,
                        &device,
                        &stroke.style,
                        &stroke.paint,
                        stroke.blend,
                        self.clip,
                    );
                    self.stats.strokes += 1;
                }
                DrawOp::Text(run) => {
                    self.draw_text(&view, run)?;
                }
            }
        }
        Ok(())
    }

    /// Draws a prepared sprite batch through a camera.
    pub fn draw_batches(
        &mut self,
        camera: &Camera2D,
        frame: &crate::batch::FrameBatches,
    ) -> Result<(), RenderError> {
        camera.validate()?;
        for sprite in &frame.sprites {
            let Some(texture) = self.assets.texture(sprite.texture) else {
                return Err(RenderError::MissingTexture(sprite.texture));
            };
            let clip = self.clip;
            self.canvas.draw_sprite_clipped(camera, sprite, texture, clip)?;
            self.stats.sprites += 1;
        }
        Ok(())
    }

    /// Encodes the framebuffer as PNG.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        self.canvas.encode_png()
    }

    /// Consumes the renderer and returns its framebuffer texture.
    pub fn into_texture(self) -> Texture {
        self.canvas.to_texture()
    }

    fn world_bounds_visible(&self, view: &Affine2, bounds: Rect) -> bool {
        if !bounds.is_valid() {
            return true;
        }
        let corners = [
            view.transform_point(bounds.min()),
            view.transform_point(Vec2::new(bounds.max().x, bounds.min().y)),
            view.transform_point(bounds.max()),
            view.transform_point(Vec2::new(bounds.min().x, bounds.max().y)),
        ];
        let mut minimum = corners[0];
        let mut maximum = corners[0];
        for corner in corners.iter().skip(1) {
            minimum = minimum.min(*corner);
            maximum = maximum.max(*corner);
        }
        let screen = Rect::from_min_max(minimum, maximum);
        match self.clip {
            Some(clip) => screen.intersects(clip),
            None => screen.intersects(self.canvas.bounds()),
        }
    }

    fn draw_text(&mut self, view: &Affine2, run: &TextRun) -> Result<(), RenderError> {
        let Some(font) = self.assets.font(run.font) else {
            return Err(RenderError::MissingFont(run.font));
        };
        if run.glyphs.is_empty() {
            return Ok(());
        }
        let matrix = run.transform.then(*view);
        if !matrix.is_finite() {
            return Ok(());
        }
        if let Some(bounds) = run.cull_bounds {
            if !self.world_bounds_visible(view, bounds) {
                self.stats.skipped += 1;
                return Ok(());
            }
        }
        let scale = matrix.approximate_scale();
        let axis_aligned = matrix.m12.abs() < 1.0e-4
            && matrix.m21.abs() < 1.0e-4
            && matrix.m11 > 0.0
            && matrix.m22 > 0.0;
        let effective_size = run.size * scale;
        let clip = self.clip;

        if axis_aligned {
            for glyph in &run.glyphs {
                let origin = matrix.transform_point(Vec2::new(glyph.x, glyph.y));
                let rasterized = self.glyphs.rasterize(run.font, font, glyph.glyph, effective_size);
                let Some(raster) = rasterized else {
                    continue;
                };
                let x = origin.x.round() as i32 + raster.bearing_x;
                let y = origin.y.round() as i32 + raster.bearing_y;
                let mask = raster.mask.clone();
                self.canvas.blend_mask_at(&mask, x, y, run.color, run.blend, clip);
                self.stats.glyphs += 1;
                self.stats.covered_pixels += mask.covered_area() as u64;
            }
        } else {
            let paint = Paint::solid(run.color);
            for glyph in &run.glyphs {
                let Ok(outline) = font.outline(glyph.glyph) else {
                    continue;
                };
                if outline.is_empty() {
                    continue;
                }
                let glyph_transform = Affine2::from_scale(Vec2::splat(run.size / font.units_per_em()))
                    .then(matrix);
                self.canvas.fill_path(
                    &outline,
                    &glyph_transform,
                    &paint,
                    FillRule::NonZero,
                    run.blend,
                    clip,
                );
                self.stats.glyphs += 1;
            }
            self.stats.uncached_glyphs = self.stats.glyphs;
        }
        Ok(())
    }
}

/// Renders a display list once into a new framebuffer.
///
/// Convenience for tools and tests that only need the resulting pixels.
pub fn render_once(
    width: u32,
    height: u32,
    camera: &Camera2D,
    list: &DisplayList,
    assets: &Assets,
    clear: Option<Rgba8>,
    preset: QualityPreset,
) -> Result<Canvas, RenderError> {
    let settings = QualitySettings::for_preset(preset);
    let mut renderer = SoftwareRenderer::with_settings(width, height, settings)?;
    *renderer.assets_mut() = assets.clone();
    renderer.begin_frame(clear)?;
    renderer.draw(camera, list)?;
    Ok(renderer.canvas().clone())
}

/// Colour used by hosts that want an opaque black background.
pub const OPAQUE_BLACK: Rgba8 = Rgba8::BLACK;

/// Colour used by hosts that want a transparent background.
pub const TRANSPARENT: Rgba8 = Rgba8::TRANSPARENT;

/// Default clear colour for scene previews.
pub const DEFAULT_CLEAR: Color = Color::BLACK;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::FillOp;
    use crate::path::Path;

    fn fill_op(rect: Rect, color: Color) -> DrawOp {
        DrawOp::Fill(FillOp {
            path: Path::rect(rect),
            transform: Affine2::IDENTITY,
            paint: Paint::solid(color),
            rule: FillRule::NonZero,
            blend: BlendMode::SourceOver,
            cull_bounds: None,
        })
    }

    #[test]
    fn renders_fills_into_the_framebuffer() {
        let camera = Camera2D::new(Vec2::new(32.0, 32.0)).unwrap();
        let mut renderer = SoftwareRenderer::new(32, 32).unwrap();
        renderer.begin_frame(Some(Rgba8::TRANSPARENT)).unwrap();
        let mut list = DisplayList::new();
        list.push(fill_op(Rect::new(4.0, 4.0, 8.0, 8.0), Color::WHITE));
        renderer.draw(&camera, &list).unwrap();
        assert_eq!(renderer.stats().fills, 1);
        assert_eq!(renderer.canvas().pixel(8, 8), Some(Rgba8::WHITE));
        assert_eq!(renderer.canvas().pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn missing_textures_fail_before_drawing() {
        let camera = Camera2D::new(Vec2::new(8.0, 8.0)).unwrap();
        let mut renderer = SoftwareRenderer::new(8, 8).unwrap();
        let mut list = DisplayList::new();
        list.push(DrawOp::Sprite(crate::batch::Sprite::new(77)));
        assert_eq!(
            renderer.draw(&camera, &list),
            Err(RenderError::MissingTexture(77))
        );
    }

    #[test]
    fn clip_restricts_subsequent_operations() {
        let camera = Camera2D::new(Vec2::new(16.0, 16.0)).unwrap();
        let mut renderer = SoftwareRenderer::new(16, 16).unwrap();
        renderer.set_clip(Some(Rect::new(0.0, 0.0, 8.0, 8.0)));
        let mut list = DisplayList::new();
        list.push(fill_op(Rect::new(0.0, 0.0, 16.0, 16.0), Color::WHITE));
        renderer.draw(&camera, &list).unwrap();
        assert_eq!(renderer.canvas().pixel(4, 4), Some(Rgba8::WHITE));
        assert_eq!(renderer.canvas().pixel(12, 12), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn presets_reconfigure_sampling_budget() {
        let mut renderer = SoftwareRenderer::new(4, 4).unwrap();
        renderer.set_preset(QualityPreset::Low);
        assert_eq!(renderer.settings().preset, QualityPreset::Low);
        assert_eq!(renderer.glyph_cache().budget_bytes(), renderer.settings().glyph_cache_bytes);
    }

    #[test]
    fn culling_skips_offscreen_operations() {
        let camera = Camera2D::new(Vec2::new(16.0, 16.0)).unwrap();
        let mut renderer = SoftwareRenderer::new(16, 16).unwrap();
        let mut list = DisplayList::new();
        let sprite = crate::batch::Sprite::new(1)
            .with_size(Vec2::new(4.0, 4.0))
            .with_transform(Affine2::from_translation(Vec2::new(400.0, 400.0)));
        renderer
            .assets_mut()
            .insert_texture(1, Texture::solid(4, 4, Rgba8::WHITE).unwrap());
        list.push(DrawOp::Sprite(sprite));
        renderer.draw(&camera, &list).unwrap();
        assert_eq!(renderer.stats().skipped, 1);
        assert_eq!(renderer.stats().sprites, 0);
    }
}

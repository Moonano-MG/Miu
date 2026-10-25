//! Deterministic CPU rasterizer for Miu's 2D command stream.
//!
//! The software renderer is useful for tests, headless tools, asset previews, and
//! as a reference implementation for GPU bridges. It consumes the same
//! [`crate::FrameBatches`] that a native backend receives, so ordering and camera
//! behaviour can be verified without a graphics driver.

use std::collections::HashMap;

use crate::{Camera2D, CameraError, Color, FrameBatches, SourceRect, Sprite, Vec2};

/// Premultiplied RGBA colour stored with eight bits per channel.
///
/// Texture pixels supplied to Miu must be premultiplied: every RGB component is
/// less than or equal to alpha. This makes source-over blending stable at sprite
/// edges and matches the colour contract used by [`crate::Color`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Rgba8 {
    /// Red component.
    pub r: u8,
    /// Green component.
    pub g: u8,
    /// Blue component.
    pub b: u8,
    /// Alpha component.
    pub a: u8,
}

impl Rgba8 {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Fully opaque white.
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Whether RGB channels obey the premultiplied-alpha invariant.
    pub const fn is_premultiplied(self) -> bool {
        self.r <= self.a && self.g <= self.a && self.b <= self.a
    }

    /// Converts one straight-alpha RGBA8 input pixel to Miu's premultiplied form.
    /// RGB is multiplied by alpha with nearest-integer rounding.
    pub const fn from_straight_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: ((r as u16 * a as u16 + 127) / 255) as u8,
            g: ((g as u16 * a as u16 + 127) / 255) as u8,
            b: ((b as u16 * a as u16 + 127) / 255) as u8,
            a,
        }
    }

    fn modulate(self, tint: Color) -> Self {
        fn channel(channel: u8, multiplier: f32) -> u8 {
            let multiplier = if multiplier.is_finite() {
                multiplier.clamp(0.0, 1.0)
            } else {
                0.0
            };
            ((f32::from(channel) * multiplier).round()).clamp(0.0, 255.0) as u8
        }

        Self {
            r: channel(self.r, tint.r),
            g: channel(self.g, tint.g),
            b: channel(self.b, tint.b),
            a: channel(self.a, tint.a),
        }
    }

    fn source_over(self, destination: Self) -> Self {
        let inverse_alpha = u16::from(255_u8.saturating_sub(self.a));
        let combine = |source: u8, destination: u8| -> u8 {
            let value = u16::from(source) + (u16::from(destination) * inverse_alpha + 127) / 255;
            value.min(255) as u8
        };
        Self {
            r: combine(self.r, destination.r),
            g: combine(self.g, destination.g),
            b: combine(self.b, destination.b),
            a: combine(self.a, destination.a),
        }
    }
}

/// A CPU-readable premultiplied RGBA texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
}

/// Texture creation failed because its dimensions and pixel data disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureError {
    /// Width and height must both be nonzero.
    EmptyDimensions,
    /// `width * height` overflowed the addressable memory size.
    DimensionsOverflow,
    /// Pixel count did not equal `width * height`.
    PixelCountMismatch,
    /// A source pixel was not premultiplied (`rgb` exceeded `alpha`).
    InvalidPremultipliedPixel,
    /// The requested pixel storage could not be allocated.
    OutOfMemory,
}

impl Texture {
    /// Creates a texture from row-major premultiplied RGBA pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<Rgba8>) -> Result<Self, TextureError> {
        let expected = pixel_len(width, height)?;
        if pixels.len() != expected {
            return Err(TextureError::PixelCountMismatch);
        }
        if pixels.iter().any(|pixel| !pixel.is_premultiplied()) {
            return Err(TextureError::InvalidPremultipliedPixel);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Converts row-major straight-alpha RGBA8 pixels to Miu's premultiplied storage.
    ///
    /// This explicit conversion is the only straight-alpha input path; [`Self::new`]
    /// rejects non-premultiplied pixels so blending is never ambiguous.
    pub fn from_straight_alpha(
        width: u32,
        height: u32,
        pixels: Vec<Rgba8>,
    ) -> Result<Self, TextureError> {
        let expected = pixel_len(width, height)?;
        if pixels.len() != expected {
            return Err(TextureError::PixelCountMismatch);
        }
        let mut pixels = pixels;
        for pixel in &mut pixels {
            *pixel = Self::straight_pixel(*pixel);
        }
        Self::new(width, height, pixels)
    }

    const fn straight_pixel(pixel: Rgba8) -> Rgba8 {
        Rgba8::from_straight_alpha(pixel.r, pixel.g, pixel.b, pixel.a)
    }

    /// Creates a solid-colour texture of `width` by `height` pixels.
    pub fn solid(width: u32, height: u32, color: Rgba8) -> Result<Self, TextureError> {
        let expected = pixel_len(width, height)?;
        if !color.is_premultiplied() {
            return Err(TextureError::InvalidPremultipliedPixel);
        }
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(expected).map_err(|_| TextureError::OutOfMemory)?;
        pixels.resize(expected, color);
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Texture width in source pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Texture height in source pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns a source pixel, or `None` outside the texture extent.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.pixels.get(index).copied()
    }

    fn sample_nearest_in_source(&self, x: f32, y: f32, source: SourceRect) -> Rgba8 {
        // Atlas-safe nearest sampling clamps to the source rectangle, never to the
        // whole texture. UV values beyond the source are intentionally clamped.
        let max_x = source.x + source.width - 1;
        let max_y = source.y + source.height - 1;
        let x = x.floor().clamp(source.x as f32, max_x as f32) as u32;
        let y = y.floor().clamp(source.y as f32, max_y as f32) as u32;
        self.pixel(x, y).unwrap_or(Rgba8::TRANSPARENT)
    }
}

fn pixel_len(width: u32, height: u32) -> Result<usize, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::EmptyDimensions);
    }
    (width as usize)
        .checked_mul(height as usize)
        .ok_or(TextureError::DimensionsOverflow)
}

/// Texture lookup owned by a headless or reference-renderer host.
#[derive(Debug, Default)]
pub struct TextureStore {
    textures: HashMap<u64, Texture>,
}

impl TextureStore {
    /// Creates an empty texture store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces the texture named by `id` and returns the old texture.
    pub fn insert(&mut self, id: u64, texture: Texture) -> Option<Texture> {
        self.textures.insert(id, texture)
    }

    /// Retrieves a texture by the opaque sprite texture identifier.
    pub fn get(&self, id: u64) -> Option<&Texture> {
        self.textures.get(&id)
    }

    /// Removes a texture and returns it if it was present.
    pub fn remove(&mut self, id: u64) -> Option<Texture> {
        self.textures.remove(&id)
    }

    /// Number of textures registered in this store.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Whether no textures are registered in this store.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }
}

/// A mutable CPU render target with premultiplied RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
}

impl Canvas {
    /// Allocates a transparent CPU canvas.
    pub fn new(width: u32, height: u32) -> Result<Self, TextureError> {
        let length = pixel_len(width, height)?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(length).map_err(|_| TextureError::OutOfMemory)?;
        pixels.resize(length, Rgba8::TRANSPARENT);
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Canvas width in logical pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height in logical pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Fills every pixel with a premultiplied colour.
    ///
    /// A rejected colour leaves the canvas unchanged. Use
    /// [`Rgba8::from_straight_alpha`] before clearing with straight-alpha input.
    pub fn clear(&mut self, color: Rgba8) -> Result<(), SoftwareRenderError> {
        if !color.is_premultiplied() {
            return Err(SoftwareRenderError::InvalidPremultipliedColor);
        }
        self.pixels.fill(color);
        Ok(())
    }

    /// Returns a canvas pixel, or `None` when the coordinates are outside it.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels.get(y as usize * self.width as usize + x as usize).copied()
    }

    /// Provides an immutable row-major view for image export or testing.
    pub fn pixels(&self) -> &[Rgba8] {
        &self.pixels
    }

    /// Draws one sprite using centre-of-pixel nearest sampling and premultiplied
    /// source-over blending.
    ///
    /// The `Sprite::source` rectangle selects atlas texels. `Sprite::size` controls
    /// geometry independently, and `Sprite::pivot` is the point placed at
    /// `transform.translation`. A zero `size` preserves legacy behaviour by using
    /// the source rectangle dimensions. Camera and sprite matrices are built once
    /// per draw; the pixel loop only performs affine increments and blending.
    pub fn draw_sprite(
        &mut self,
        camera: &Camera2D,
        sprite: &Sprite,
        texture: &Texture,
    ) -> Result<(), SoftwareRenderError> {
        camera.validate().map_err(SoftwareRenderError::Camera)?;
        let resolved = ResolvedSprite::new(sprite, texture)?;
        self.draw_resolved_sprite(camera, sprite, texture, resolved);
        Ok(())
    }

    fn draw_resolved_sprite(
        &mut self,
        camera: &Camera2D,
        sprite: &Sprite,
        texture: &Texture,
        resolved: ResolvedSprite,
    ) {
        let camera_matrix = CameraMatrix::new(camera);
        let inverse = SpriteInverse::new(&camera_matrix, sprite);
        let corners = [
            Vec2::new(resolved.left, resolved.top),
            Vec2::new(resolved.right, resolved.top),
            Vec2::new(resolved.left, resolved.bottom),
            Vec2::new(resolved.right, resolved.bottom),
        ];
        let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for corner in corners {
            let screen = camera_matrix.world_to_screen(transform_local(sprite, corner));
            minimum.x = minimum.x.min(screen.x);
            minimum.y = minimum.y.min(screen.y);
            maximum.x = maximum.x.max(screen.x);
            maximum.y = maximum.y.max(screen.y);
        }

        // Pixel coverage is defined by testing the centre `(x + .5, y + .5)`.
        let min_x = minimum.x.floor().clamp(0.0, self.width as f32) as u32;
        let min_y = minimum.y.floor().clamp(0.0, self.height as f32) as u32;
        let max_x = maximum.x.ceil().clamp(0.0, self.width as f32) as u32;
        let max_y = maximum.y.ceil().clamp(0.0, self.height as f32) as u32;
        let first = inverse.local_at(Vec2::new(min_x as f32 + 0.5, min_y as f32 + 0.5));
        let step_x = inverse.local_delta(Vec2::new(1.0, 0.0));
        let step_y = inverse.local_delta(Vec2::new(0.0, 1.0));
        let mut row_local = first;
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
                    let foreground = texture
                        .sample_nearest_in_source(texel_x, texel_y, resolved.source)
                        .modulate(sprite.color);
                    self.blend_pixel(x, y, foreground);
                }
                local = local + step_x;
            }
            row_local = row_local + step_y;
        }
    }

    /// Draws the prepared command stream in its sorted sprite order.
    pub fn draw_frame(
        &mut self,
        camera: &Camera2D,
        frame: &FrameBatches,
        textures: &TextureStore,
    ) -> Result<(), SoftwareRenderError> {
        camera.validate().map_err(SoftwareRenderError::Camera)?;
        // Validate all commands before changing a pixel so callers never receive a
        // silently half-rendered frame when an asset or transform is invalid.
        for sprite in &frame.sprites {
            let texture = textures
                .get(sprite.texture)
                .ok_or(SoftwareRenderError::MissingTexture(sprite.texture))?;
            ResolvedSprite::new(sprite, texture)?;
        }
        for sprite in &frame.sprites {
            let texture = textures
                .get(sprite.texture)
                .expect("texture store was preflighted above");
            let resolved = ResolvedSprite::new(sprite, texture)
                .expect("sprite was preflighted above");
            self.draw_resolved_sprite(camera, sprite, texture, resolved);
        }
        Ok(())
    }

    fn blend_pixel(&mut self, x: u32, y: u32, foreground: Rgba8) {
        let index = y as usize * self.width as usize + x as usize;
        self.pixels[index] = foreground.source_over(self.pixels[index]);
    }
}

/// Failure while encoding the CPU reference renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftwareRenderError {
    /// The camera could not create a valid transform.
    Camera(CameraError),
    /// A sprite transform, colour, UV, size, or pivot was invalid.
    InvalidSpriteTransform,
    /// A pixel source rectangle exceeded the bound texture.
    InvalidSourceRect,
    /// A clear colour was not in premultiplied-alpha form.
    InvalidPremultipliedColor,
    /// The frame referenced a texture identifier absent from the texture store.
    MissingTexture(u64),
}

#[derive(Clone, Copy)]
struct ResolvedSprite {
    source: SourceRect,
    size: Vec2,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl ResolvedSprite {
    fn new(sprite: &Sprite, texture: &Texture) -> Result<Self, SoftwareRenderError> {
        validate_sprite(sprite)?;
        let source = if sprite.source.width == 0 && sprite.source.height == 0 {
            if sprite.source.x != 0 || sprite.source.y != 0 {
                return Err(SoftwareRenderError::InvalidSourceRect);
            }
            SourceRect { x: 0, y: 0, width: texture.width(), height: texture.height() }
        } else {
            sprite.source
        };
        let source_right = source.x.checked_add(source.width).ok_or(SoftwareRenderError::InvalidSourceRect)?;
        let source_bottom = source.y.checked_add(source.height).ok_or(SoftwareRenderError::InvalidSourceRect)?;
        if source.width == 0
            || source.height == 0
            || source_right > texture.width()
            || source_bottom > texture.height()
        {
            return Err(SoftwareRenderError::InvalidSourceRect);
        }
        let size = if sprite.size == Vec2::ZERO {
            Vec2::new(source.width as f32, source.height as f32)
        } else {
            sprite.size
        };
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return Err(SoftwareRenderError::InvalidSpriteTransform);
        }
        let left = -sprite.pivot.x * size.x;
        let top = -sprite.pivot.y * size.y;
        Ok(Self { source, size, left, top, right: left + size.x, bottom: top + size.y })
    }
}

#[derive(Clone, Copy)]
struct CameraMatrix {
    position: Vec2,
    centre: Vec2,
    zoom: f32,
    cos: f32,
    sin: f32,
}

impl CameraMatrix {
    fn new(camera: &Camera2D) -> Self {
        let (sin, cos) = (-camera.rotation_radians).sin_cos();
        Self {
            position: camera.position,
            centre: camera.viewport * 0.5,
            zoom: camera.zoom,
            cos,
            sin,
        }
    }

    fn world_to_screen(self, world: Vec2) -> Vec2 {
        let local = world - self.position;
        Vec2::new(
            (local.x * self.cos - local.y * self.sin) * self.zoom + self.centre.x,
            (local.x * self.sin + local.y * self.cos) * self.zoom + self.centre.y,
        )
    }
}

#[derive(Clone, Copy)]
struct SpriteInverse {
    origin_at_camera_centre: Vec2,
    camera_centre: Vec2,
    step_x: Vec2,
    step_y: Vec2,
}

impl SpriteInverse {
    fn new(camera: &CameraMatrix, sprite: &Sprite) -> Self {
        let (sprite_sin, sprite_cos) = sprite.transform.rotation_radians.sin_cos();
        let screen_to_world = |delta: Vec2| -> Vec2 {
            let scaled = delta * (1.0 / camera.zoom);
            // Transpose the precomputed R(-rotation) matrix to obtain R(+rotation).
            Vec2::new(
                scaled.x * camera.cos + scaled.y * camera.sin,
                -scaled.x * camera.sin + scaled.y * camera.cos,
            )
        };
        let to_local = |world_delta: Vec2| -> Vec2 {
            let unrotated = Vec2::new(
                world_delta.x * sprite_cos + world_delta.y * sprite_sin,
                -world_delta.x * sprite_sin + world_delta.y * sprite_cos,
            );
            Vec2::new(
                unrotated.x / sprite.transform.scale.x,
                unrotated.y / sprite.transform.scale.y,
            )
        };
        let world_at_centre = camera.position - sprite.transform.translation;
        Self {
            origin_at_camera_centre: to_local(world_at_centre),
            camera_centre: camera.centre,
            step_x: to_local(screen_to_world(Vec2::new(1.0, 0.0))),
            step_y: to_local(screen_to_world(Vec2::new(0.0, 1.0))),
        }
    }

    fn local_at(self, screen: Vec2) -> Vec2 {
        let offset = screen - self.camera_centre;
        self.origin_at_camera_centre + self.step_x * offset.x + self.step_y * offset.y
    }

    fn local_delta(self, screen_delta: Vec2) -> Vec2 {
        self.step_x * screen_delta.x + self.step_y * screen_delta.y
    }
}

fn validate_sprite(sprite: &Sprite) -> Result<(), SoftwareRenderError> {
    let transform = sprite.transform;
    let color = sprite.color;
    let uv = sprite.uv;
    let pivot = sprite.pivot;
    if !transform.translation.x.is_finite()
        || !transform.translation.y.is_finite()
        || !transform.rotation_radians.is_finite()
        || !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
        || transform.scale.x == 0.0
        || transform.scale.y == 0.0
        || !color.r.is_finite()
        || !color.g.is_finite()
        || !color.b.is_finite()
        || !color.a.is_finite()
        || color.r < 0.0
        || color.g < 0.0
        || color.b < 0.0
        || color.a < 0.0
        || color.r > color.a
        || color.g > color.a
        || color.b > color.a
        || !uv.left.is_finite()
        || !uv.top.is_finite()
        || !uv.right.is_finite()
        || !uv.bottom.is_finite()
        || !pivot.x.is_finite()
        || !pivot.y.is_finite()
        || !(0.0..=1.0).contains(&pivot.x)
        || !(0.0..=1.0).contains(&pivot.y)
    {
        return Err(SoftwareRenderError::InvalidSpriteTransform);
    }
    Ok(())
}

fn transform_local(sprite: &Sprite, local: Vec2) -> Vec2 {
    let scaled = Vec2::new(
        local.x * sprite.transform.scale.x,
        local.y * sprite.transform.scale.y,
    );
    rotate(scaled, sprite.transform.rotation_radians) + sprite.transform.translation
}

fn rotate(value: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(value.x * cos - value.y * sin, value.x * sin + value.y * cos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BatchBuilder, BatchMode, Transform2D, UvRect};

    fn camera() -> Camera2D {
        Camera2D::new(Vec2::new(4.0, 4.0)).expect("valid camera")
    }

    fn sprite(texture: u64) -> Sprite {
        Sprite {
            texture,
            transform: Transform2D::default(),
            source: SourceRect::FULL,
            uv: UvRect::FULL,
            size: Vec2::ZERO,
            pivot: Vec2::new(0.5, 0.5),
            color: Color::WHITE,
            layer: 0,
        }
    }

    #[test]
    fn draws_a_centered_sprite_to_the_canvas() {
        let texture = Texture::solid(
            2,
            2,
            Rgba8 {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        )
        .unwrap();
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.draw_sprite(&camera(), &sprite(5), &texture).unwrap();
        assert_eq!(canvas.pixel(1, 1), Some(texture.pixel(0, 0).unwrap()));
        assert_eq!(canvas.pixel(2, 2), Some(texture.pixel(1, 1).unwrap()));
        assert_eq!(canvas.pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn premultiplied_source_over_blending_is_correct() {
        let texture = Texture::solid(
            1,
            1,
            Rgba8 {
                r: 128,
                g: 0,
                b: 0,
                a: 128,
            },
        )
        .unwrap();
        let mut canvas = Canvas::new(1, 1).unwrap();
        canvas.clear(Rgba8 {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }).unwrap();
        let mut test_camera = Camera2D::new(Vec2::new(1.0, 1.0)).unwrap();
        test_camera.position = Vec2::new(0.5, 0.5);
        let mut test_sprite = sprite(1);
        test_sprite.transform.translation = Vec2::new(0.5, 0.5);
        canvas.draw_sprite(&test_camera, &test_sprite, &texture).unwrap();
        assert_eq!(
            canvas.pixel(0, 0),
            Some(Rgba8 {
                r: 128,
                g: 0,
                b: 127,
                a: 255,
            })
        );
    }

    #[test]
    fn frame_rendering_obeys_sorted_layer_order() {
        let mut textures = TextureStore::new();
        textures.insert(
            1,
            Texture::solid(
                1,
                1,
                Rgba8 {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            )
            .unwrap(),
        );
        textures.insert(
            2,
            Texture::solid(
                1,
                1,
                Rgba8 {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255,
                },
            )
            .unwrap(),
        );
        let mut behind = sprite(1);
        behind.layer = -1;
        let mut front = sprite(2);
        front.layer = 1;
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(front);
        builder.push(behind);
        let frame = builder.finish();
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.draw_frame(&camera(), &frame, &textures).unwrap();
        assert_eq!(canvas.pixel(1, 1).unwrap().g, 255);
    }

    #[test]
    fn atlas_source_and_destination_size_are_independent() {
        let texture = Texture::new(
            4,
            1,
            vec![
                Rgba8 { r: 255, g: 0, b: 0, a: 255 },
                Rgba8 { r: 0, g: 255, b: 0, a: 255 },
                Rgba8 { r: 0, g: 0, b: 255, a: 255 },
                Rgba8 { r: 255, g: 255, b: 255, a: 255 },
            ],
        )
        .unwrap();
        let mut atlas_sprite = sprite(7);
        atlas_sprite.source = SourceRect { x: 1, y: 0, width: 2, height: 1 };
        atlas_sprite.size = Vec2::new(2.0, 1.0);
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.draw_sprite(&camera(), &atlas_sprite, &texture).unwrap();
        assert_eq!(canvas.pixel(1, 1).unwrap().g, 255);
        assert_eq!(canvas.pixel(2, 1).unwrap().b, 255);
        assert_eq!(canvas.pixel(0, 1), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn reversed_uv_mirrors_sampling_without_changing_geometry() {
        let texture = Texture::new(
            2,
            1,
            vec![
                Rgba8 { r: 255, g: 0, b: 0, a: 255 },
                Rgba8 { r: 0, g: 255, b: 0, a: 255 },
            ],
        )
        .unwrap();
        let mut mirrored = sprite(8);
        mirrored.size = Vec2::new(2.0, 1.0);
        mirrored.uv = crate::UvRect { left: 1.0, top: 0.0, right: 0.0, bottom: 1.0 };
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.draw_sprite(&camera(), &mirrored, &texture).unwrap();
        assert_eq!(canvas.pixel(1, 1).unwrap().g, 255);
        assert_eq!(canvas.pixel(2, 1).unwrap().r, 255);
    }

    #[test]
    fn rotated_camera_rasterization_matches_public_camera_inverse_golden_pixels() {
        let texture = Texture::new(
            3,
            1,
            vec![
                Rgba8 { r: 255, g: 0, b: 0, a: 255 },
                Rgba8 { r: 0, g: 255, b: 0, a: 255 },
                Rgba8 { r: 0, g: 0, b: 255, a: 255 },
            ],
        )
        .unwrap();
        // Right-angle and non-right-angle cameras are all checked against the
        // public inverse. A non-symmetric atlas makes transpose/sign regressions
        // observable rather than hiding them behind a solid texture.
        for rotation in [
            core::f32::consts::FRAC_PI_2,
            core::f32::consts::PI,
            0.37_f32,
        ] {
            let mut rotated_camera = Camera2D::new(Vec2::new(32.0, 32.0)).unwrap();
            rotated_camera.zoom = 8.0;
            rotated_camera.rotation_radians = rotation;
            let mut subject = sprite(1);
            subject.size = Vec2::new(3.0, 1.0);
            let mut canvas = Canvas::new(32, 32).unwrap();
            canvas.draw_sprite(&rotated_camera, &subject, &texture).unwrap();

            for (world, expected_texel, expected) in [
                (Vec2::new(-1.0, 0.0), 0, Rgba8 { r: 255, g: 0, b: 0, a: 255 }),
                (Vec2::new(0.0, 0.0), 1, Rgba8 { r: 0, g: 255, b: 0, a: 255 }),
                (Vec2::new(1.0, 0.0), 2, Rgba8 { r: 0, g: 0, b: 255, a: 255 }),
            ] {
                let screen = rotated_camera.world_to_screen(world).unwrap();
                let x = screen.x.floor() as u32;
                let y = screen.y.floor() as u32;
                let raster_centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let recovered = rotated_camera.screen_to_world(raster_centre).unwrap();
                // The raster pixel centre must map back into this source texel.
                let source_texel = (recovered.x + 1.5).floor() as i32;
                assert_eq!(source_texel, expected_texel, "public inverse at rotation {rotation}");
                assert_eq!(canvas.pixel(x, y), Some(expected), "raster at rotation {rotation}");
            }
        }
    }

    #[test]
    fn atlas_sampling_never_reads_an_adjacent_source_region() {
        let texture = Texture::new(
            3,
            1,
            vec![
                Rgba8 { r: 255, g: 0, b: 0, a: 255 },
                Rgba8 { r: 0, g: 255, b: 0, a: 255 },
                Rgba8 { r: 0, g: 0, b: 255, a: 255 },
            ],
        )
        .unwrap();
        let mut subject = sprite(1);
        subject.source = SourceRect { x: 0, y: 0, width: 2, height: 1 };
        subject.size = Vec2::new(2.0, 1.0);
        subject.uv = crate::UvRect { left: 0.0, top: 0.0, right: 2.0, bottom: 1.0 };
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.draw_sprite(&camera(), &subject, &texture).unwrap();
        assert_ne!(canvas.pixel(2, 1).unwrap().b, 255);
    }

    #[test]
    fn invalid_frame_does_not_partially_modify_canvas() {
        let mut textures = TextureStore::new();
        textures.insert(1, Texture::solid(1, 1, Rgba8::WHITE).unwrap());
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(sprite(1));
        builder.push(sprite(404));
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.clear(Rgba8 { r: 0, g: 0, b: 255, a: 255 }).unwrap();
        let before = canvas.pixels().to_vec();
        assert_eq!(
            canvas.draw_frame(&camera(), &builder.finish(), &textures),
            Err(SoftwareRenderError::MissingTexture(404))
        );
        assert_eq!(canvas.pixels(), before.as_slice());
    }

    #[test]
    fn full_source_sentinel_requires_zero_origin() {
        let texture = Texture::solid(1, 1, Rgba8::WHITE).unwrap();
        let mut invalid = sprite(1);
        invalid.source = SourceRect { x: 1, y: 0, width: 0, height: 0 };
        let error = Canvas::new(4, 4).unwrap().draw_sprite(&camera(), &invalid, &texture);
        assert_eq!(error, Err(SoftwareRenderError::InvalidSourceRect));
    }

    #[test]
    fn straight_alpha_conversion_and_clear_contract_are_explicit() {
        let converted = Rgba8::from_straight_alpha(255, 127, 0, 128);
        assert_eq!(converted, Rgba8 { r: 128, g: 64, b: 0, a: 128 });
        let texture = Texture::from_straight_alpha(1, 1, vec![Rgba8 { r: 255, g: 0, b: 0, a: 128 }]).unwrap();
        assert_eq!(texture.pixel(0, 0), Some(Rgba8 { r: 128, g: 0, b: 0, a: 128 }));
        let mut canvas = Canvas::new(1, 1).unwrap();
        assert_eq!(
            canvas.clear(Rgba8 { r: 255, g: 0, b: 0, a: 0 }),
            Err(SoftwareRenderError::InvalidPremultipliedColor)
        );
        assert_eq!(canvas.pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn rejects_non_premultiplied_texture_pixels() {
        assert_eq!(
            Texture::solid(1, 1, Rgba8 { r: 255, g: 0, b: 0, a: 0 }),
            Err(TextureError::InvalidPremultipliedPixel)
        );
    }

    #[test]
    fn reports_missing_textures_instead_of_skipping_draws() {
        let frame = {
            let mut builder = BatchBuilder::new(BatchMode::Stable);
            builder.push(sprite(99));
            builder.finish()
        };
        let mut canvas = Canvas::new(4, 4).unwrap();
        let error = canvas
            .draw_frame(&camera(), &frame, &TextureStore::new())
            .expect_err("missing assets are diagnostics");
        assert_eq!(error, SoftwareRenderError::MissingTexture(99));
    }
}

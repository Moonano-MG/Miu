//! Deterministic CPU rasterizer for Miu's 2D command stream.
//!
//! The software renderer is useful for tests, headless tools, asset previews, and
//! as a reference implementation for GPU bridges. It consumes the same
//! [`crate::FrameBatches`] that a native backend receives, so ordering and camera
//! behaviour can be verified without a graphics driver.

use std::collections::HashMap;

use crate::{Camera2D, CameraError, Color, FrameBatches, Sprite, Vec2};

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
}

impl Texture {
    /// Creates a texture from row-major premultiplied RGBA pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<Rgba8>) -> Result<Self, TextureError> {
        let expected = pixel_len(width, height)?;
        if pixels.len() != expected {
            return Err(TextureError::PixelCountMismatch);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Creates a solid-colour texture of `width` by `height` pixels.
    pub fn solid(width: u32, height: u32, color: Rgba8) -> Result<Self, TextureError> {
        let expected = pixel_len(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![color; expected],
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

    fn sample_nearest(&self, u: f32, v: f32) -> Rgba8 {
        // Clamp rather than wrapping so atlas-edge sampling cannot read another tile.
        let u = u.clamp(0.0, 1.0 - f32::EPSILON);
        let v = v.clamp(0.0, 1.0 - f32::EPSILON);
        let x = (u * self.width as f32).floor() as u32;
        let y = (v * self.height as f32).floor() as u32;
        // Coordinates are clamped to valid ranges immediately above.
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
        Ok(Self {
            width,
            height,
            pixels: vec![Rgba8::TRANSPARENT; length],
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

    /// Fills every pixel with `color`.
    pub fn clear(&mut self, color: Rgba8) {
        self.pixels.fill(color);
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

    /// Draws one sprite using nearest-neighbour sampling and source-over blending.
    ///
    /// Sprite translation is its centre in world coordinates. With a scale of one,
    /// one texture pixel occupies one world unit. Negative scale mirrors the sprite.
    pub fn draw_sprite(
        &mut self,
        camera: &Camera2D,
        sprite: &Sprite,
        texture: &Texture,
    ) -> Result<(), SoftwareRenderError> {
        camera.validate().map_err(SoftwareRenderError::Camera)?;
        validate_sprite(sprite)?;

        let half = Vec2::new(texture.width as f32 * 0.5, texture.height as f32 * 0.5);
        let corners = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(-half.x, half.y),
            Vec2::new(half.x, half.y),
        ];
        let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for corner in corners {
            let screen = camera
                .world_to_screen(transform_local(sprite, corner))
                .map_err(SoftwareRenderError::Camera)?;
            minimum.x = minimum.x.min(screen.x);
            minimum.y = minimum.y.min(screen.y);
            maximum.x = maximum.x.max(screen.x);
            maximum.y = maximum.y.max(screen.y);
        }

        let min_x = minimum.x.floor().max(0.0) as u32;
        let min_y = minimum.y.floor().max(0.0) as u32;
        let max_x = maximum.x.ceil().min(self.width as f32).max(0.0) as u32;
        let max_y = maximum.y.ceil().min(self.height as f32).max(0.0) as u32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let screen = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let world = camera
                    .screen_to_world(screen)
                    .map_err(SoftwareRenderError::Camera)?;
                let local = inverse_transform_local(sprite, world);
                let source = local + half;
                if source.x < 0.0
                    || source.y < 0.0
                    || source.x >= texture.width as f32
                    || source.y >= texture.height as f32
                {
                    continue;
                }
                let source_u = source.x / texture.width as f32;
                let source_v = source.y / texture.height as f32;
                let u = sprite.uv.left + (sprite.uv.right - sprite.uv.left) * source_u;
                let v = sprite.uv.top + (sprite.uv.bottom - sprite.uv.top) * source_v;
                let foreground = texture.sample_nearest(u, v).modulate(sprite.color);
                self.blend_pixel(x, y, foreground);
            }
        }
        Ok(())
    }

    /// Draws the prepared command stream in its sorted sprite order.
    pub fn draw_frame(
        &mut self,
        camera: &Camera2D,
        frame: &FrameBatches,
        textures: &TextureStore,
    ) -> Result<(), SoftwareRenderError> {
        for sprite in &frame.sprites {
            let texture = textures
                .get(sprite.texture)
                .ok_or(SoftwareRenderError::MissingTexture(sprite.texture))?;
            self.draw_sprite(camera, sprite, texture)?;
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
    /// A sprite's scale, rotation, or translation was non-finite or singular.
    InvalidSpriteTransform,
    /// The frame referenced a texture identifier absent from the texture store.
    MissingTexture(u64),
}

fn validate_sprite(sprite: &Sprite) -> Result<(), SoftwareRenderError> {
    let transform = sprite.transform;
    if !transform.translation.x.is_finite()
        || !transform.translation.y.is_finite()
        || !transform.rotation_radians.is_finite()
        || !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
        || transform.scale.x == 0.0
        || transform.scale.y == 0.0
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

fn inverse_transform_local(sprite: &Sprite, world: Vec2) -> Vec2 {
    let local = rotate(
        world - sprite.transform.translation,
        -sprite.transform.rotation_radians,
    );
    Vec2::new(
        local.x / sprite.transform.scale.x,
        local.y / sprite.transform.scale.y,
    )
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
            uv: UvRect::FULL,
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
        });
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

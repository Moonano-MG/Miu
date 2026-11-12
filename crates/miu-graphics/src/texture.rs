//! Premultiplied RGBA textures and their lookup store.
//!
//! Textures are CPU-owned pixel buffers. A GPU bridge uploads them once and keeps
//! the same numeric id, so sprite instances and scene assets stay backend-neutral.
//! All storage uses premultiplied alpha; decoders convert straight-alpha image data
//! through [`Texture::from_straight_alpha`].

use std::collections::HashMap;

use miu_core::Rgba8;

use crate::batch::SourceRect;

/// A CPU-readable premultiplied RGBA texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
}

/// Texture creation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureError {
    /// Width and height must both be non-zero.
    EmptyDimensions,
    /// `width * height` overflowed the addressable size.
    DimensionsOverflow,
    /// The pixel count did not equal `width * height`.
    PixelCountMismatch,
    /// A source pixel had `rgb > alpha` and was therefore not premultiplied.
    InvalidPremultipliedPixel,
    /// The requested storage could not be allocated.
    OutOfMemory,
    /// A requested sub-rectangle exceeded the texture.
    OutOfBounds,
}

impl std::error::Error for TextureError {}

impl core::fmt::Display for TextureError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            Self::EmptyDimensions => "texture dimensions must be non-zero",
            Self::DimensionsOverflow => "texture dimensions overflow the address space",
            Self::PixelCountMismatch => "pixel count does not match texture dimensions",
            Self::InvalidPremultipliedPixel => "pixel data is not premultiplied",
            Self::OutOfMemory => "texture allocation failed",
            Self::OutOfBounds => "texture region is out of bounds",
        };
        formatter.write_str(text)
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::EmptyDimensions);
    }
    (width as usize)
        .checked_mul(height as usize)
        .ok_or(TextureError::DimensionsOverflow)
}

impl Texture {
    /// Creates a texture from row-major premultiplied pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<Rgba8>) -> Result<Self, TextureError> {
        let expected = pixel_count(width, height)?;
        if pixels.len() != expected {
            return Err(TextureError::PixelCountMismatch);
        }
        if pixels.iter().any(|pixel| !pixel.is_premultiplied()) {
            return Err(TextureError::InvalidPremultipliedPixel);
        }
        Ok(Self { width, height, pixels })
    }

    /// Converts row-major straight-alpha pixels, as produced by image decoders.
    pub fn from_straight_alpha(
        width: u32,
        height: u32,
        pixels: &[Rgba8],
    ) -> Result<Self, TextureError> {
        let expected = pixel_count(width, height)?;
        if pixels.len() != expected {
            return Err(TextureError::PixelCountMismatch);
        }
        let mut converted = Vec::new();
        converted
            .try_reserve_exact(expected)
            .map_err(|_| TextureError::OutOfMemory)?;
        for pixel in pixels {
            converted.push(Rgba8::from_straight_alpha(pixel.r, pixel.g, pixel.b, pixel.a));
        }
        Self::new(width, height, converted)
    }

    /// Creates a solid texture.
    pub fn solid(width: u32, height: u32, color: Rgba8) -> Result<Self, TextureError> {
        let expected = pixel_count(width, height)?;
        if !color.is_premultiplied() {
            return Err(TextureError::InvalidPremultipliedPixel);
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(expected)
            .map_err(|_| TextureError::OutOfMemory)?;
        pixels.resize(expected, color);
        Ok(Self { width, height, pixels })
    }

    /// Decodes straight-alpha RGBA8 bytes into a texture.
    pub fn from_rgba8_bytes(width: u32, height: u32, bytes: &[u8]) -> Result<Self, TextureError> {
        let expected = pixel_count(width, height)?;
        if bytes.len() != expected * 4 {
            return Err(TextureError::PixelCountMismatch);
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(expected)
            .map_err(|_| TextureError::OutOfMemory)?;
        for chunk in bytes.chunks_exact(4) {
            pixels.push(Rgba8::from_straight_alpha(chunk[0], chunk[1], chunk[2], chunk[3]));
        }
        Self::new(width, height, pixels)
    }

    /// Texture width in texels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Texture height in texels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Size in texels as a floating-point vector.
    pub fn size_f32(&self) -> miu_core::Vec2 {
        miu_core::Vec2::new(self.width as f32, self.height as f32)
    }

    /// Row-major premultiplied pixels.
    pub fn pixels(&self) -> &[Rgba8] {
        &self.pixels
    }

    /// Mutable row-major pixels, used by baking tools.
    pub fn pixels_mut(&mut self) -> &mut [Rgba8] {
        &mut self.pixels
    }

    /// Returns a texel, or `None` outside the texture.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    /// Writes a texel, returning `false` when the coordinate is outside the texture.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.pixels[index] = color.clamped();
        true
    }

    /// Encodes to straight-alpha RGBA8 bytes, reversing premultiplication.
    pub fn to_rgba8_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for pixel in &self.pixels {
            if pixel.a == 0 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let alpha = pixel.a as u32;
            let straight = |channel: u8| -> u8 {
                let value = (channel as u32 * 255 + alpha / 2) / alpha;
                value.min(255) as u8
            };
            bytes.push(straight(pixel.r));
            bytes.push(straight(pixel.g));
            bytes.push(straight(pixel.b));
            bytes.push(pixel.a);
        }
        bytes
    }

    /// Nearest-neighbour sample clamped to the whole texture.
    pub fn sample_nearest(&self, x: f32, y: f32) -> Rgba8 {
        let source = SourceRect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        };
        self.sample_nearest_in_source(x, y, source)
    }

    /// Nearest-neighbour sample clamped to an atlas source rectangle.
    ///
    /// Clamping to the source rectangle prevents bleeding into neighbouring atlas
    /// entries when geometry samples slightly outside its own tile.
    pub fn sample_nearest_in_source(&self, x: f32, y: f32, source: SourceRect) -> Rgba8 {
        if source.width == 0 || source.height == 0 {
            return Rgba8::TRANSPARENT;
        }
        let max_x = source.x.saturating_add(source.width).saturating_sub(1);
        let max_y = source.y.saturating_add(source.height).saturating_sub(1);
        let texel_x = x.floor().clamp(source.x as f32, max_x as f32) as u32;
        let texel_y = y.floor().clamp(source.y as f32, max_y as f32) as u32;
        self.pixel(texel_x, texel_y).unwrap_or(Rgba8::TRANSPARENT)
    }

    /// Bilinear sample clamped to an atlas source rectangle.
    ///
    /// Interpolation happens in premultiplied space, which keeps transparent
    /// neighbours from darkening the result.
    pub fn sample_bilinear_in_source(&self, x: f32, y: f32, source: SourceRect) -> Rgba8 {
        if source.width == 0 || source.height == 0 {
            return Rgba8::TRANSPARENT;
        }
        let max_x = source.x.saturating_add(source.width).saturating_sub(1);
        let max_y = source.y.saturating_add(source.height).saturating_sub(1);
        let clamp_x = |value: f32| value.clamp(source.x as f32, max_x as f32);
        let clamp_y = |value: f32| value.clamp(source.y as f32, max_y as f32);
        let u = clamp_x(x - 0.5);
        let v = clamp_y(y - 0.5);
        let left = u.floor();
        let top = v.floor();
        let right = clamp_x(left + 1.0);
        let bottom = clamp_y(top + 1.0);
        let weight_x = (u - left).clamp(0.0, 1.0);
        let weight_y = (v - top).clamp(0.0, 1.0);
        let left_u = left as u32;
        let top_v = top as u32;
        let right_u = right as u32;
        let bottom_v = bottom as u32;
        let texel = |tx: u32, ty: u32| self.pixel(tx, ty).unwrap_or(Rgba8::TRANSPARENT);
        let top_left = texel(left_u, top_v);
        let top_right = texel(right_u, top_v);
        let bottom_left = texel(left_u, bottom_v);
        let bottom_right = texel(right_u, bottom_v);
        let mix = |a: u8, b: u8, t: f32| -> f32 { a as f32 + (b as f32 - a as f32) * t };
        let top_row = [
            mix(top_left.r, top_right.r, weight_x),
            mix(top_left.g, top_right.g, weight_x),
            mix(top_left.b, top_right.b, weight_x),
            mix(top_left.a, top_right.a, weight_x),
        ];
        let bottom_row = [
            mix(bottom_left.r, bottom_right.r, weight_x),
            mix(bottom_left.g, bottom_right.g, weight_x),
            mix(bottom_left.b, bottom_right.b, weight_x),
            mix(bottom_left.a, bottom_right.a, weight_x),
        ];
        let mut channels = [0u8; 4];
        for index in 0..4 {
            let value = top_row[index] + (bottom_row[index] - top_row[index]) * weight_y;
            channels[index] = value.round().clamp(0.0, 255.0) as u8;
        }
        Rgba8::new(channels[0], channels[1], channels[2], channels[3]).clamped()
    }

    /// Extracts a sub-rectangle, or `None` when it is outside the texture.
    pub fn sub_image(&self, region: SourceRect) -> Option<Texture> {
        let right = region.x.checked_add(region.width)?;
        let bottom = region.y.checked_add(region.height)?;
        if region.width == 0 || region.height == 0 || right > self.width || bottom > self.height {
            return None;
        }
        let mut pixels = Vec::with_capacity((region.width * region.height) as usize);
        for y in region.y..bottom {
            for x in region.x..right {
                pixels.push(self.pixel(x, y).unwrap_or(Rgba8::TRANSPARENT));
            }
        }
        Texture::new(region.width, region.height, pixels).ok()
    }

    /// Copies another texture onto this one at `(x, y)` using source-over.
    pub fn blit(&mut self, source: &Texture, x: i32, y: i32) -> bool {
        if source.width == 0 || source.height == 0 {
            return false;
        }
        let mut written = false;
        for source_y in 0..source.height {
            for source_x in 0..source.width {
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
                let foreground = source.pixel(source_x, source_y).unwrap_or(Rgba8::TRANSPARENT);
                if foreground.a == 0 {
                    continue;
                }
                let index = destination_y as usize * self.width as usize + destination_x as usize;
                self.pixels[index] = foreground.source_over(self.pixels[index]);
                written = true;
            }
        }
        written
    }
}

/// Identifier-keyed texture lookup shared by scenes, the ABI, and renderers.
#[derive(Debug, Default, Clone)]
pub struct TextureStore {
    textures: HashMap<u64, Texture>,
}

impl TextureStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces the texture named by `id`, returning the old value.
    pub fn insert(&mut self, id: u64, texture: Texture) -> Option<Texture> {
        self.textures.insert(id, texture)
    }

    /// Looks up a texture.
    pub fn get(&self, id: u64) -> Option<&Texture> {
        self.textures.get(&id)
    }

    /// Looks up a texture mutably, used by baking tools.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Texture> {
        self.textures.get_mut(&id)
    }

    /// Removes a texture.
    pub fn remove(&mut self, id: u64) -> Option<Texture> {
        self.textures.remove(&id)
    }

    /// Whether `id` is present.
    pub fn contains(&self, id: u64) -> bool {
        self.textures.contains_key(&id)
    }

    /// Number of stored textures.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Releases every texture.
    pub fn clear(&mut self) {
        self.textures.clear();
    }

    /// Iterates over `(id, texture)` pairs without a defined order.
    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Texture)> {
        self.textures.iter()
    }

    /// Approximate resident bytes, used by cache diagnostics.
    pub fn resident_bytes(&self) -> u64 {
        let mut total = 0u64;
        for texture in self.textures.values() {
            total += (texture.width() as u64) * (texture.height() as u64) * 4;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miu_core::Vec2;

    fn checkerboard() -> Texture {
        let pixels = vec![
            Rgba8::WHITE,
            Rgba8::TRANSPARENT,
            Rgba8::TRANSPARENT,
            Rgba8::WHITE,
        ];
        Texture::new(2, 2, pixels).expect("valid texture")
    }

    #[test]
    fn rejects_unpremultiplied_pixels() {
        let pixels = vec![Rgba8::new(255, 0, 0, 128)];
        assert_eq!(
            Texture::new(1, 1, pixels),
            Err(TextureError::InvalidPremultipliedPixel)
        );
        assert_eq!(Texture::new(0, 4, Vec::new()), Err(TextureError::EmptyDimensions));
    }

    #[test]
    fn nearest_sampling_clamps_to_atlas_tile() {
        let texture = checkerboard();
        let tile = SourceRect { x: 0, y: 0, width: 1, height: 1 };
        assert_eq!(texture.sample_nearest_in_source(5.0, 5.0, tile), Rgba8::WHITE);
        assert_eq!(texture.sample_nearest_in_source(-5.0, -5.0, tile), Rgba8::WHITE);
    }

    #[test]
    fn bilinear_sampling_averages_neighbours() {
        let texture = checkerboard();
        let full = SourceRect { x: 0, y: 0, width: 2, height: 2 };
        let sample = texture.sample_bilinear_in_source(1.0, 1.0, full);
        assert!(sample.a > 60 && sample.a < 200, "alpha was {}", sample.a);
    }

    #[test]
    fn sub_images_and_blits_preserve_pixels() {
        let texture = checkerboard();
        let sub = texture
            .sub_image(SourceRect { x: 1, y: 1, width: 1, height: 1 })
            .expect("inside texture");
        assert_eq!(sub.pixel(0, 0), Some(Rgba8::WHITE));

        let mut target = Texture::solid(4, 1, Rgba8::TRANSPARENT).unwrap();
        assert!(target.blit(&texture, -1, 0));
        assert_eq!(target.pixel(0, 0), Some(Rgba8::TRANSPARENT));
        assert_eq!(target.pixel(1, 0), Some(Rgba8::WHITE));
    }

    #[test]
    fn straight_alpha_conversion_round_trips() {
        let texture = Texture::from_rgba8_bytes(1, 1, &[255, 128, 0, 128]).unwrap();
        assert_eq!(texture.pixel(0, 0), Some(Rgba8::new(128, 64, 0, 128)));
        assert_eq!(texture.to_rgba8_bytes(), vec![255, 127, 0, 128]);
    }

    #[test]
    fn store_tracks_resident_bytes() {
        let mut store = TextureStore::new();
        store.insert(7, Texture::solid(4, 4, Rgba8::WHITE).unwrap());
        assert!(store.contains(7));
        assert_eq!(store.resident_bytes(), 4 * 4 * 4);
        assert_eq!(store.get(7).map(Texture::size_f32), Some(Vec2::new(4.0, 4.0)));
    }
}

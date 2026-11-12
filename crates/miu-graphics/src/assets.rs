//! Texture and font stores shared by scenes and renderers.
//!
//! [`Assets`] is the ownership point for decoded resources. Scenes reference
//! resources by `u64` id so a display list never borrows asset data, and GPU bridges
//! can cache uploaded textures under the same ids.

use crate::codec::{decode_image, ImageError};
use crate::font::{Font, FontError};
use crate::texture::{Texture, TextureStore};

/// Texture and font stores owned by a renderer or a baking tool.
#[derive(Debug, Default, Clone)]
pub struct Assets {
    textures: TextureStore,
    fonts: crate::text::FontStore,
}

impl Assets {
    /// Creates an empty asset set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Texture store.
    pub fn textures(&self) -> &TextureStore {
        &self.textures
    }

    /// Mutable texture store.
    pub fn textures_mut(&mut self) -> &mut TextureStore {
        &mut self.textures
    }

    /// Font store.
    pub fn fonts(&self) -> &crate::text::FontStore {
        &self.fonts
    }

    /// Mutable font store.
    pub fn fonts_mut(&mut self) -> &mut crate::text::FontStore {
        &mut self.fonts
    }

    /// Inserts or replaces a texture, returning the previous one.
    pub fn insert_texture(&mut self, id: u64, texture: Texture) -> Option<Texture> {
        self.textures.insert(id, texture)
    }

    /// Inserts or replaces a font, returning the previous one.
    pub fn insert_font(&mut self, id: u64, font: Font) -> Option<Font> {
        self.fonts.insert(id, font)
    }

    /// Looks up a texture.
    pub fn texture(&self, id: u64) -> Option<&Texture> {
        self.textures.get(id)
    }

    /// Looks up a font.
    pub fn font(&self, id: u64) -> Option<&Font> {
        self.fonts.get(id)
    }

    /// Removes a texture.
    pub fn remove_texture(&mut self, id: u64) -> Option<Texture> {
        self.textures.remove(id)
    }

    /// Removes a font.
    pub fn remove_font(&mut self, id: u64) -> Option<Font> {
        self.fonts.remove(id)
    }

    /// Decodes an encoded image and stores it as a texture.
    pub fn load_texture(&mut self, id: u64, bytes: &[u8]) -> Result<(), ImageError> {
        let image = decode_image(bytes)?;
        let texture = image.to_texture()?;
        self.textures.insert(id, texture);
        Ok(())
    }

    /// Parses a TrueType font and stores it.
    pub fn load_font(&mut self, id: u64, bytes: Vec<u8>) -> Result<(), FontError> {
        let font = Font::from_bytes(bytes)?;
        self.fonts.insert(id, font);
        Ok(())
    }

    /// Number of stored textures.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Number of stored fonts.
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Approximate bytes held by textures.
    pub fn texture_bytes(&self) -> u64 {
        self.textures.resident_bytes()
    }

    /// Releases every texture and font.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.fonts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miu_core::Rgba8;

    #[test]
    fn inserting_assets_replaces_previous_entries() {
        let mut assets = Assets::new();
        assets.insert_texture(1, Texture::solid(2, 2, Rgba8::WHITE).unwrap());
        let replaced = assets.insert_texture(1, Texture::solid(4, 4, Rgba8::WHITE).unwrap());
        assert!(replaced.is_some());
        assert_eq!(assets.texture_count(), 1);
        assert_eq!(assets.texture(1).map(Texture::width), Some(4));
        assert!(assets.remove_texture(1).is_some());
        assert!(assets.texture(1).is_none());
    }

    #[test]
    fn invalid_image_data_is_reported() {
        let mut assets = Assets::new();
        assert!(assets.load_texture(9, b"not an image").is_err());
        assert_eq!(assets.texture_count(), 0);
    }
}

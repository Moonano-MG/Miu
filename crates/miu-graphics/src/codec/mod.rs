//! Image codecs used for textures and for renderer output.
//!
//! Supported formats:
//!
//! | Format | Decode | Encode | Notes |
//! |---|---|---|---|
//! | PNG | 8/16-bit, colour types 0/2/3/4/6, non-interlaced | 8-bit RGBA | zlib via [`crate::codec::inflate`] |
//! | BMP | 24/32-bit uncompressed, top-down and bottom-up | — | `BITMAPINFOHEADER` |
//! | TGA | 8/24/32-bit, uncompressed and RLE | — | origin flags honoured |
//! | QOI | full specification | — | lossless, fast |
//!
//! Decoders return straight-alpha RGBA texels; [`DecodedImage::to_texture`] converts
//! them to Miu's premultiplied storage. Interlaced PNG and 1/2/4-bit PNG are
//! rejected with [`ImageError::Unsupported`] rather than guessed at.

pub mod bmp;
pub mod checksum;
pub mod deflate;
pub mod inflate;
pub mod png;
pub mod qoi;
pub mod tga;

use miu_core::Rgba8;

use crate::texture::{Texture, TextureError};

/// Failure while decoding or encoding an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    /// The container or a required variant is not implemented.
    Unsupported(&'static str),
    /// The input ended before the image was complete.
    Truncated,
    /// The input has valid framing but invalid contents.
    Corrupt(&'static str),
    /// The decoded size exceeds the addressable limit.
    TooLarge,
    /// Encoding could not represent the image.
    Encode(&'static str),
}

impl std::error::Error for ImageError {}

impl core::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported(what) => write!(formatter, "unsupported image feature: {what}"),
            Self::Truncated => formatter.write_str("image data ended unexpectedly"),
            Self::Corrupt(what) => write!(formatter, "corrupt image: {what}"),
            Self::TooLarge => formatter.write_str("image dimensions are too large"),
            Self::Encode(what) => write!(formatter, "image encoding failed: {what}"),
        }
    }
}

impl From<TextureError> for ImageError {
    fn from(error: TextureError) -> Self {
        match error {
            TextureError::OutOfMemory | TextureError::DimensionsOverflow => Self::TooLarge,
            _ => Self::Corrupt("pixel storage rejected"),
        }
    }
}

/// Decoded image in straight-alpha RGBA8 form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major straight-alpha RGBA pixels.
    pub pixels: Vec<Rgba8>,
}

impl DecodedImage {
    /// Creates a decoded image, validating the pixel count.
    pub fn new(width: u32, height: u32, pixels: Vec<Rgba8>) -> Result<Self, ImageError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ImageError::TooLarge)?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(ImageError::Corrupt("pixel count does not match dimensions"));
        }
        Ok(Self { width, height, pixels })
    }

    /// Creates a transparent image of the given size.
    pub fn transparent(width: u32, height: u32) -> Result<Self, ImageError> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ImageError::TooLarge)?;
        if width == 0 || height == 0 {
            return Err(ImageError::Corrupt("image has no area"));
        }
        Ok(Self {
            width,
            height,
            pixels: vec![Rgba8::TRANSPARENT; count],
        })
    }

    /// Converts to a premultiplied texture.
    pub fn to_texture(&self) -> Result<Texture, ImageError> {
        Texture::from_straight_alpha(self.width, self.height, &self.pixels).map_err(ImageError::from)
    }

    /// Copies one pixel, returning `false` outside the image.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.pixels[y as usize * self.width as usize + x as usize] = color;
        true
    }
}

/// Decodes an image, detecting the format from the byte signature.
///
/// PNG, BMP, and QOI are self-identifying. TGA has no signature, so it is only
/// attempted when no other format matches and the header looks plausible.
pub fn decode_image(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return png::decode(bytes);
    }
    if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        return bmp::decode(bytes);
    }
    if bytes.len() >= 4 && &bytes[..4] == b"qoif" {
        return qoi::decode(bytes);
    }
    if tga::looks_like_tga(bytes) {
        return tga::decode(bytes);
    }
    Err(ImageError::Unsupported("unknown image signature"))
}

/// Encodes a texture as a PNG file (8-bit RGBA, non-interlaced).
pub fn encode_png(texture: &Texture) -> Result<Vec<u8>, ImageError> {
    png::encode(texture)
}

/// Encodes straight-alpha RGBA8 pixels as a PNG file.
pub fn encode_png_rgba8(
    width: u32,
    height: u32,
    pixels: &[Rgba8],
) -> Result<Vec<u8>, ImageError> {
    let image = DecodedImage::new(width, height, pixels.to_vec())?;
    let texture = image.to_texture()?;
    png::encode(&texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_signatures_are_rejected() {
        assert_eq!(
            decode_image(&[0, 1, 2, 3]),
            Err(ImageError::Unsupported("unknown image signature"))
        );
    }

    #[test]
    fn png_round_trip_preserves_premultiplied_pixels() {
        let pixels = vec![
            Rgba8::new(255, 0, 0, 255),
            Rgba8::new(0, 128, 0, 128),
            Rgba8::new(0, 0, 0, 0),
            Rgba8::new(64, 64, 64, 255),
        ];
        let texture = Texture::new(2, 2, pixels.clone()).unwrap();
        let encoded = encode_png(&texture).unwrap();
        let decoded = decode_image(&encoded).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        let restored = decoded.to_texture().unwrap();
        for (index, expected) in pixels.iter().enumerate() {
            let actual = restored.pixels()[index];
            assert!(actual.a.abs_diff(expected.a) <= 1, "alpha at {index}");
            assert!(actual.r.abs_diff(expected.r) <= 1, "red at {index}");
        }
    }

    #[test]
    fn decoded_images_validate_their_dimensions() {
        assert!(DecodedImage::new(2, 2, vec![Rgba8::WHITE]).is_err());
        assert!(DecodedImage::transparent(0, 4).is_err());
    }
}

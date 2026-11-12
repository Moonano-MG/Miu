//! Colour values and blend modes.
//!
//! Miu stores colour in **premultiplied alpha** form everywhere: textures, tints,
//! gradients, and the software framebuffer. Premultiplied storage keeps source-over
//! blending linear at sprite edges and makes additive effects trivial.
//!
//! [`Color`] is a floating-point working value; [`Rgba8`] is the storage texel.
//! [`Rgba8::from_straight_alpha`] is the only supported conversion from ordinary
//! (non-premultiplied) image data such as PNG pixels.

use crate::math::Vec2;

/// How a source colour is combined with the destination framebuffer.
///
/// Every mode expects premultiplied inputs. GPU bridges implement the same set;
/// a bridge that cannot express a mode must report it as unsupported instead of
/// silently falling back to [`BlendMode::SourceOver`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlendMode {
    /// Standard `src + dst * (1 - src.a)` compositing.
    #[default]
    SourceOver = 0,
    /// Additive light: `src + dst`, saturated at white.
    Add = 1,
    /// Darkening: component-wise product of the un-premultiplied colours.
    Multiply = 2,
    /// Lightening: inverse of [`BlendMode::Multiply`].
    Screen = 3,
    /// Replaces the destination inside the source's coverage.
    Copy = 4,
}

/// Premultiplied RGBA texel with eight bits per channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Rgba8 {
    /// Red, premultiplied.
    pub r: u8,
    /// Green, premultiplied.
    pub g: u8,
    /// Blue, premultiplied.
    pub b: u8,
    /// Alpha, unchanged by premultiplication.
    pub a: u8,
}

impl Rgba8 {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    /// Opaque white.
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    /// Opaque black.
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };

    /// Creates a texel from premultiplied components.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Converts straight-alpha components (as stored by PNG and JPEG) to Miu's
    /// premultiplied form using nearest-integer rounding.
    pub const fn from_straight_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: ((r as u16 * a as u16 + 127) / 255) as u8,
            g: ((g as u16 * a as u16 + 127) / 255) as u8,
            b: ((b as u16 * a as u16 + 127) / 255) as u8,
            a,
        }
    }

    /// Whether `rgb <= a` for every channel, which premultiplied storage requires.
    pub const fn is_premultiplied(self) -> bool {
        self.r <= self.a && self.g <= self.a && self.b <= self.a
    }

    /// Whether the texel is completely transparent.
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// Clamps `rgb` into the premultiplied range for the current alpha.
    pub const fn clamped(self) -> Self {
        Self::new(self.r.min(self.a), self.g.min(self.a), self.b.min(self.a), self.a)
    }

    /// Converts to a floating-point working colour.
    pub fn to_color(self) -> crate::color::Color {
        let scale = 1.0 / 255.0;
        crate::color::Color::new(
            self.r as f32 * scale,
            self.g as f32 * scale,
            self.b as f32 * scale,
            self.a as f32 * scale,
        )
    }

    /// Multiplies all channels by an opacity in `[0, 1]`, saturating NaN to zero.
    pub fn with_opacity(self, opacity: f32) -> Self {
        let factor = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 0.0 };
        let convert = |channel: u8| -> u8 {
            let scaled = (channel as f32 * factor).round();
            scaled.clamp(0.0, 255.0) as u8
        };
        Self::new(convert(self.r), convert(self.g), convert(self.b), convert(self.a))
    }

    /// Multiplies the texel by a premultiplied tint.
    pub fn modulate(self, tint: crate::color::Color) -> Self {
        fn channel(value: u8, multiplier: f32) -> u8 {
            let multiplier = if multiplier.is_finite() { multiplier.clamp(0.0, 1.0) } else { 0.0 };
            let scaled = (value as f32 * multiplier).round();
            scaled.clamp(0.0, 255.0) as u8
        }
        Self::new(
            channel(self.r, tint.r),
            channel(self.g, tint.g),
            channel(self.b, tint.b),
            channel(self.a, tint.a),
        )
    }

    /// Composites `self` over `destination` with the given mode.
    pub fn blend(self, destination: Self, mode: BlendMode) -> Self {
        match mode {
            BlendMode::SourceOver => self.source_over(destination),
            BlendMode::Add => self.additive(destination),
            BlendMode::Multiply => self.multiply(destination),
            BlendMode::Screen => self.screen(destination),
            BlendMode::Copy => self,
        }
    }

    /// Standard source-over compositing for premultiplied values.
    pub fn source_over(self, destination: Self) -> Self {
        let inverse_alpha = 255u16 - self.a as u16;
        let combine = |source: u8, destination: u8| -> u8 {
            let value = source as u16 + (destination as u16 * inverse_alpha + 127) / 255;
            value.min(255) as u8
        };
        Self::new(
            combine(self.r, destination.r),
            combine(self.g, destination.g),
            combine(self.b, destination.b),
            combine(self.a, destination.a),
        )
    }

    /// Additive compositing, saturating each channel.
    pub fn additive(self, destination: Self) -> Self {
        let combine = |source: u8, destination: u8| -> u8 {
            (source as u16 + destination as u16).min(255) as u8
        };
        Self::new(
            combine(self.r, destination.r),
            combine(self.g, destination.g),
            combine(self.b, destination.b),
            combine(self.a, destination.a),
        )
    }

    /// Component-wise multiply of the un-premultiplied colours.
    ///
    /// The result keeps the Porter-Duff "plus" composition terms so that a
    /// partially transparent source still reveals the destination where it has
    /// no coverage.
    pub fn multiply(self, destination: Self) -> Self {
        let source_alpha = self.a as u32;
        let destination_alpha = destination.a as u32;
        let inverse_source = 255 - source_alpha;
        let inverse_destination = 255 - destination_alpha;
        let combine = |source: u8, destination: u8| -> u8 {
            let product = (source as u32 * destination as u32 + 127) / 255;
            let value = product
                + (source as u32 * inverse_destination + 127) / 255
                + (destination as u32 * inverse_source + 127) / 255;
            value.min(255) as u8
        };
        Self::new(
            combine(self.r, destination.r),
            combine(self.g, destination.g),
            combine(self.b, destination.b),
            source_alpha.saturating_add(destination_alpha * inverse_source / 255) as u8,
        )
    }

    /// Component-wise screen blend: the photographic inverse of [`Self::multiply`].
    pub fn screen(self, destination: Self) -> Self {
        let multiplied = self.multiply(destination);
        let combine = |source: u8, destination: u8, product: u8| -> u8 {
            let value = source as u16 + destination as u16 - product as u16;
            value.min(255) as u8
        };
        Self::new(
            combine(self.r, destination.r, multiplied.r),
            combine(self.g, destination.g, multiplied.g),
            combine(self.b, destination.b, multiplied.b),
            multiplied.a,
        )
    }

    /// Linear interpolation in premultiplied space, used for fades.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let mix = |a: u8, b: u8| -> u8 {
            let value = a as f32 + (b as f32 - a as f32) * t;
            value.round().clamp(0.0, 255.0) as u8
        };
        Self::new(mix(self.r, other.r), mix(self.g, other.g), mix(self.b, other.b), mix(self.a, other.a))
    }
}

/// Premultiplied RGBA colour with `f32` components in `[0, 1]`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Color {
    /// Red, premultiplied by alpha.
    pub r: f32,
    /// Green, premultiplied by alpha.
    pub g: f32,
    /// Blue, premultiplied by alpha.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Color {
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    /// Opaque white.
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    /// Opaque black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Creates a premultiplied colour without validation.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a premultiplied colour from straight-alpha components.
    pub fn straight(r: f32, g: f32, b: f32, a: f32) -> Self {
        let a = if a.is_finite() { a.clamp(0.0, 1.0) } else { 0.0 };
        Self::new(
            clamp_unit(r) * a,
            clamp_unit(g) * a,
            clamp_unit(b) * a,
            a,
        )
    }

    /// Creates an opaque colour from 8-bit sRGB components.
    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::straight(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }

    /// Creates a premultiplied colour from straight-alpha 8-bit components.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba8::from_straight_alpha(r, g, b, a).to_color()
    }

    /// Converts to the 8-bit premultiplied storage texel.
    pub fn to_rgba8(self) -> Rgba8 {
        let convert = |value: f32| -> u8 {
            let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
            (value * 255.0).round() as u8
        };
        Rgba8::new(convert(self.r), convert(self.g), convert(self.b), convert(self.a))
    }

    /// Returns the colour with a different alpha, scaling RGB to stay premultiplied.
    pub fn with_alpha(self, alpha: f32) -> Self {
        let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
        if self.a <= 0.0 {
            return Self::new(0.0, 0.0, 0.0, alpha);
        }
        let scale = alpha / self.a;
        Self::new(self.r * scale, self.g * scale, self.b * scale, alpha)
    }

    /// Scales alpha (and RGB) by `factor`, used for fade transitions.
    pub fn multiply_alpha(self, factor: f32) -> Self {
        let factor = if factor.is_finite() { factor.clamp(0.0, 1.0) } else { 0.0 };
        Self::new(self.r * factor, self.g * factor, self.b * factor, self.a * factor)
    }

    /// Removes premultiplication, returning straight-alpha components.
    pub fn to_straight(self) -> [f32; 4] {
        if self.a <= 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }
        [self.r / self.a, self.g / self.a, self.b / self.a, self.a]
    }

    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        self.r.is_finite() && self.g.is_finite() && self.b.is_finite() && self.a.is_finite()
    }

    /// Clamps components into `[0, 1]` and keeps the premultiplied invariant.
    pub fn clamped(self) -> Self {
        let a = clamp_unit(self.a);
        Self::new(clamp_unit(self.r).min(a), clamp_unit(self.g).min(a), clamp_unit(self.b).min(a), a)
    }

    /// Linear interpolation from `self` to `other`, including alpha.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }

    /// Relative luminance of the straight-alpha colour, for contrast diagnostics.
    pub fn luminance(self) -> f32 {
        let straight = self.to_straight();
        0.2126 * straight[0] + 0.7152 * straight[1] + 0.0722 * straight[2]
    }

    /// Creates a colour from hue/saturation/value plus alpha.
    ///
    /// `hue` is measured in degrees; the other parameters are in `[0, 1]`.
    pub fn from_hsv(hue: f32, saturation: f32, value: f32, alpha: f32) -> Self {
        let hue = hue.rem_euclid(360.0) / 60.0;
        let saturation = clamp_unit(saturation);
        let value = clamp_unit(value);
        let chroma = value * saturation;
        let secondary = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
        let (r, g, b) = match hue as u32 {
            0 => (chroma, secondary, 0.0),
            1 => (secondary, chroma, 0.0),
            2 => (0.0, chroma, secondary),
            3 => (0.0, secondary, chroma),
            4 => (secondary, 0.0, chroma),
            _ => (chroma, 0.0, secondary),
        };
        let match_value = value - chroma;
        Self::straight(r + match_value, g + match_value, b + match_value, alpha)
    }
}

/// Clamps a float into `[0, 1]`, mapping NaN to zero.
pub fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 }
}

/// Converts a linear-light component in `[0, 1]` to an 8-bit sRGB value.
pub fn linear_to_srgb8(value: f32) -> u8 {
    let value = clamp_unit(value);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Converts an 8-bit sRGB component to linear light in `[0, 1]`.
pub fn srgb8_to_linear(value: u8) -> f32 {
    let encoded = value as f32 / 255.0;
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Gradient interpolation in a specific colour space.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ColorSpace {
    /// Interpolate 8-bit sRGB components directly; matches design tools.
    #[default]
    Srgb = 0,
    /// Interpolate in linear light, producing brighter mid-tones.
    Linear = 1,
}

/// A convenience alias for the classic "clear" colour used by renderers.
pub const CLEAR: Rgba8 = Rgba8::TRANSPARENT;

/// Returns the midpoint of two points; kept here so callers do not re-implement it.
pub fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    (a + b) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha_inputs_become_premultiplied() {
        let texel = Rgba8::from_straight_alpha(255, 128, 0, 128);
        assert_eq!(texel, Rgba8::new(128, 64, 0, 128));
        assert!(texel.is_premultiplied());
    }

    #[test]
    fn source_over_respects_existing_coverage() {
        let background = Rgba8::new(0, 0, 255, 255);
        let foreground = Rgba8::new(255, 0, 0, 128);
        let blended = foreground.source_over(background);
        assert_eq!(blended.a, 255);
        assert!(blended.r > 100 && blended.b > 100);
    }

    #[test]
    fn copy_mode_replaces_destination() {
        let blended = Rgba8::new(10, 20, 30, 40).blend(Rgba8::WHITE, BlendMode::Copy);
        assert_eq!(blended, Rgba8::new(10, 20, 30, 40));
    }

    #[test]
    fn add_saturates_at_white() {
        let combined = Rgba8::new(200, 200, 200, 255).blend(Rgba8::new(100, 100, 100, 255), BlendMode::Add);
        assert_eq!(combined, Rgba8::WHITE);
    }

    #[test]
    fn multiply_with_opaque_destination_darkens() {
        let source = Rgba8::new(128, 128, 128, 255);
        let destination = Rgba8::new(255, 255, 255, 255);
        let combined = source.blend(destination, BlendMode::Multiply);
        assert_eq!(combined.r, 128);
    }

    #[test]
    fn screen_is_the_inverse_of_multiply() {
        let source = Rgba8::new(128, 128, 128, 255);
        let destination = Rgba8::new(0, 0, 0, 255);
        let combined = source.blend(destination, BlendMode::Screen);
        assert_eq!(combined.r, 128);
    }

    #[test]
    fn float_colour_round_trips_through_texel() {
        let color = Color::from_rgba8(200, 100, 50, 128);
        let texel = color.to_rgba8();
        assert_eq!(texel, Rgba8::new(100, 50, 25, 128));
        assert!(texel.is_premultiplied());
    }

    #[test]
    fn alpha_scaling_keeps_premultiplied_invariant() {
        let color = Color::WHITE.multiply_alpha(0.5);
        assert_eq!(color.to_rgba8(), Rgba8::new(128, 128, 128, 128));
    }

    #[test]
    fn hsv_produces_expected_primaries() {
        assert_eq!(Color::from_hsv(0.0, 1.0, 1.0, 1.0).to_straight()[0], 1.0);
        assert_eq!(Color::from_hsv(120.0, 1.0, 1.0, 1.0).to_straight()[1], 1.0);
        assert_eq!(Color::from_hsv(240.0, 1.0, 1.0, 1.0).to_straight()[2], 1.0);
    }

    #[test]
    fn srgb_round_trip_is_close() {
        for value in [0u8, 32, 128, 200, 255] {
            let linear = srgb8_to_linear(value);
            let returned = linear_to_srgb8(linear);
            assert!(returned.abs_diff(value) <= 1, "{value} -> {returned}");
        }
    }
}

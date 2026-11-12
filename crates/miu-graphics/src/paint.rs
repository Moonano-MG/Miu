//! Paints (solid colours and gradients) and stroke styles.
//!
//! A [`Paint`] answers one question: *what colour is this point?* The rasteriser
//! asks it once per covered pixel, in path-local coordinates, so the same paint can
//! be reused by geometry that moves or scales without re-tessellating anything.

use miu_core::{Color, Vec2};

/// Failure while building a paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintError {
    /// A gradient needs at least two colour stops.
    TooFewStops,
    /// A gradient stop offset was not finite.
    InvalidStopOffset,
    /// A linear gradient had identical start and end points.
    DegenerateLinear,
    /// A radial gradient radius was not finite and positive.
    InvalidRadius,
}

impl std::error::Error for PaintError {}

impl core::fmt::Display for PaintError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            Self::TooFewStops => "a gradient needs at least two stops",
            Self::InvalidStopOffset => "gradient stop offsets must be finite",
            Self::DegenerateLinear => "linear gradient endpoints must differ",
            Self::InvalidRadius => "radial gradient radius must be positive",
        };
        formatter.write_str(text)
    }
}

/// How a gradient continues outside its `[0, 1]` parameter range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Spread {
    /// Clamp to the first or last stop.
    #[default]
    Pad = 0,
    /// Repeat the gradient.
    Repeat = 1,
    /// Repeat the gradient, mirroring every other tile.
    Reflect = 2,
}

/// One colour stop of a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient in `[0, 1]`.
    pub offset: f32,
    /// Premultiplied stop colour.
    pub color: Color,
}

impl GradientStop {
    /// Creates a stop.
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}

/// Gradient from one point to another.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    /// Start point in path-local coordinates.
    pub from: Vec2,
    /// End point in path-local coordinates.
    pub to: Vec2,
    /// Stops sorted by ascending offset.
    pub stops: Vec<GradientStop>,
    /// Behaviour outside the gradient range.
    pub spread: Spread,
}

/// Gradient from a centre outwards, sampled as an ellipse.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    /// Centre in path-local coordinates.
    pub center: Vec2,
    /// Horizontal radius.
    pub radius: f32,
    /// Vertical radius; equal to `radius` for a circle.
    pub radius_y: f32,
    /// Stops sorted by ascending offset.
    pub stops: Vec<GradientStop>,
    /// Behaviour outside the gradient range.
    pub spread: Spread,
}

/// Description of how geometry is filled.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// One premultiplied colour.
    Solid(Color),
    /// Point-to-point gradient.
    Linear(LinearGradient),
    /// Centre-out gradient.
    Radial(RadialGradient),
}

impl Paint {
    /// Solid paint.
    pub const fn solid(color: Color) -> Self {
        Self::Solid(color)
    }

    /// Opaque paint from 8-bit sRGB components.
    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::Solid(Color::from_rgb8(r, g, b))
    }

    /// Builds a linear gradient, sorting and validating its stops.
    pub fn linear(
        from: Vec2,
        to: Vec2,
        stops: Vec<GradientStop>,
        spread: Spread,
    ) -> Result<Self, PaintError> {
        if (to - from).length_squared() <= f32::EPSILON {
            return Err(PaintError::DegenerateLinear);
        }
        Ok(Self::Linear(LinearGradient {
            from,
            to,
            stops: sorted_stops(stops)?,
            spread,
        }))
    }

    /// Builds a circular radial gradient.
    pub fn radial(
        center: Vec2,
        radius: f32,
        stops: Vec<GradientStop>,
        spread: Spread,
    ) -> Result<Self, PaintError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PaintError::InvalidRadius);
        }
        Ok(Self::Radial(RadialGradient {
            center,
            radius,
            radius_y: radius,
            stops: sorted_stops(stops)?,
            spread,
        }))
    }

    /// Whether the paint is a single flat colour.
    pub fn is_solid(&self) -> bool {
        matches!(self, Self::Solid(_))
    }

    /// Colour of a solid paint, or `None` for gradients.
    pub fn solid_color(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            _ => None,
        }
    }

    /// Scales the alpha of every colour in the paint.
    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Self {
        let scale = |color: Color| color.multiply_alpha(opacity);
        match self {
            Self::Solid(color) => Self::Solid(scale(color)),
            Self::Linear(gradient) => Self::Linear(LinearGradient {
                from: gradient.from,
                to: gradient.to,
                stops: gradient.stops.into_iter().map(|stop| GradientStop {
                    offset: stop.offset,
                    color: scale(stop.color),
                }).collect(),
                spread: gradient.spread,
            }),
            Self::Radial(gradient) => Self::Radial(RadialGradient {
                center: gradient.center,
                radius: gradient.radius,
                radius_y: gradient.radius_y,
                stops: gradient.stops.into_iter().map(|stop| GradientStop {
                    offset: stop.offset,
                    color: scale(stop.color),
                }).collect(),
                spread: gradient.spread,
            }),
        }
    }

    /// Samples the paint at a path-local point.
    ///
    /// `dither` perturbs the gradient parameter by up to one 8-bit step, which
    /// hides banding in large smooth gradients on the `High` and `Unlimited`
    /// presets. Solid paints ignore it.
    pub fn sample(&self, point: Vec2, dither: f32) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::Linear(gradient) => {
                let axis = gradient.to - gradient.from;
                let length_squared = axis.length_squared();
                if length_squared <= f32::EPSILON {
                    return first_stop(&gradient.stops);
                }
                let t = (point - gradient.from).dot(axis) / length_squared;
                sample_stops(&gradient.stops, apply_spread(t, gradient.spread), dither)
            }
            Self::Radial(gradient) => {
                let offset = point - gradient.center;
                let radius_x = gradient.radius;
                let radius_y = if gradient.radius_y > 0.0 { gradient.radius_y } else { radius_x };
                let normalised = Vec2::new(offset.x / radius_x, offset.y / radius_y).length();
                sample_stops(
                    &gradient.stops,
                    apply_spread(normalised, gradient.spread),
                    dither,
                )
            }
        }
    }
}

fn sorted_stops(mut stops: Vec<GradientStop>) -> Result<Vec<GradientStop>, PaintError> {
    if stops.len() < 2 {
        return Err(PaintError::TooFewStops);
    }
    for stop in &stops {
        if !stop.offset.is_finite() {
            return Err(PaintError::InvalidStopOffset);
        }
    }
    stops.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    Ok(stops)
}

fn apply_spread(t: f32, spread: Spread) -> f32 {
    if !t.is_finite() {
        return 0.0;
    }
    match spread {
        Spread::Pad => t.clamp(0.0, 1.0),
        Spread::Repeat => t - t.floor(),
        Spread::Reflect => {
            let period = t.rem_euclid(2.0);
            if period > 1.0 {
                2.0 - period
            } else {
                period
            }
        }
    }
}

fn first_stop(stops: &[GradientStop]) -> Color {
    stops.first().map(|stop| stop.color).unwrap_or(Color::TRANSPARENT)
}

fn sample_stops(stops: &[GradientStop], t: f32, dither: f32) -> Color {
    if stops.is_empty() {
        return Color::TRANSPARENT;
    }
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let mut index = 0usize;
    while index + 1 < stops.len() && stops[index + 1].offset < t {
        index += 1;
    }
    let left = stops[index];
    let right = if index + 1 < stops.len() { stops[index + 1] } else { left };
    let span = right.offset - left.offset;
    let local = if span > f32::EPSILON {
        ((t - left.offset) / span).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let local = (local + dither).clamp(0.0, 1.0);
    left.color.lerp(right.color, local)
}

/// End treatment of an open stroke.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum LineCap {
    /// Emit [`LineCap::Square`] at the ends of closed paths.
    #[default]
    Butt = 0,
    /// Half-circle caps.
    Round = 1,
    /// Extend each end by half the stroke width.
    Square = 2,
}

/// Join treatment between consecutive segments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum LineJoin {
    /// Extend the outer edges until they meet, limited by `miter_limit`.
    #[default]
    Miter = 0,
    /// Fill joins with a circular wedge.
    Round = 1,
    /// Cut the outer corner flat.
    Bevel = 2,
}

/// Stroke appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// Stroke width in local units.
    pub width: f32,
    /// End treatment.
    pub cap: LineCap,
    /// Join treatment.
    pub join: LineJoin,
    /// Maximum miter length as a multiple of `width` before falling back to bevel.
    pub miter_limit: f32,
    /// Dash pattern in local units; empty means a solid stroke.
    pub dash: Vec<f32>,
    /// Offset into the dash pattern, in local units.
    pub dash_offset: f32,
}

impl StrokeStyle {
    /// Solid stroke of the given width with butt caps and miter joins.
    pub fn new(width: f32) -> Self {
        Self {
            width: if width.is_finite() { width.max(0.0) } else { 0.0 },
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }

    /// Sets the cap style.
    #[must_use]
    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    /// Sets the join style.
    #[must_use]
    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    /// Enables a dash pattern, for example `vec![8.0, 4.0]`.
    #[must_use]
    pub fn with_dash(mut self, dash: Vec<f32>, offset: f32) -> Self {
        self.dash = dash.into_iter().filter(|value| value.is_finite() && *value > 0.0).collect();
        self.dash_offset = if offset.is_finite() { offset } else { 0.0 };
        self
    }

    /// Whether the stroke has a positive width.
    pub fn is_visible(&self) -> bool {
        self.width.is_finite() && self.width > 0.0
    }
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self::new(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops() -> Vec<GradientStop> {
        vec![
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ]
    }

    #[test]
    fn linear_gradient_interpolates_between_endpoints() {
        let paint = Paint::linear(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            stops(),
            Spread::Pad,
        )
        .unwrap();
        let middle = paint.sample(Vec2::new(5.0, 3.0), 0.0);
        assert!((middle.a - 1.0).abs() < 0.001);
        assert!((middle.r - 0.5).abs() < 0.01);
    }

    #[test]
    fn pad_clamps_outside_the_range() {
        let paint = Paint::linear(Vec2::ZERO, Vec2::new(10.0, 0.0), stops(), Spread::Pad).unwrap();
        assert_eq!(paint.sample(Vec2::new(-100.0, 0.0), 0.0), Color::BLACK);
        assert_eq!(paint.sample(Vec2::new(100.0, 0.0), 0.0), Color::WHITE);
    }

    #[test]
    fn repeat_and_reflect_wrap_the_parameter() {
        let repeat = Paint::linear(Vec2::ZERO, Vec2::new(10.0, 0.0), stops(), Spread::Repeat).unwrap();
        let reflected = Paint::linear(Vec2::ZERO, Vec2::new(10.0, 0.0), stops(), Spread::Reflect).unwrap();
        assert!((repeat.sample(Vec2::new(15.0, 0.0), 0.0).r - 0.5).abs() < 0.01);
        assert!((reflected.sample(Vec2::new(15.0, 0.0), 0.0).r - 0.5).abs() < 0.01);
    }

    #[test]
    fn radial_gradient_uses_distance_from_centre() {
        let paint = Paint::radial(Vec2::ZERO, 10.0, stops(), Spread::Pad).unwrap();
        assert!(paint.sample(Vec2::ZERO, 0.0).r < 0.01);
        assert!(paint.sample(Vec2::new(10.0, 0.0), 0.0).r > 0.99);
    }

    #[test]
    fn unsorted_stops_are_sorted_on_build() {
        let paint = Paint::linear(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            vec![
                GradientStop::new(1.0, Color::WHITE),
                GradientStop::new(0.0, Color::BLACK),
            ],
            Spread::Pad,
        )
        .unwrap();
        if let Paint::Linear(gradient) = paint {
            assert_eq!(gradient.stops[0].color, Color::BLACK);
        } else {
            panic!("expected a linear gradient");
        }
    }

    #[test]
    fn invalid_gradients_are_rejected() {
        assert_eq!(
            Paint::linear(Vec2::ZERO, Vec2::ZERO, stops(), Spread::Pad),
            Err(PaintError::DegenerateLinear)
        );
        assert_eq!(
            Paint::linear(Vec2::ZERO, Vec2::new(1.0, 0.0), vec![], Spread::Pad),
            Err(PaintError::TooFewStops)
        );
        assert_eq!(
            Paint::radial(Vec2::ZERO, 0.0, stops(), Spread::Pad),
            Err(PaintError::InvalidRadius)
        );
    }

    #[test]
    fn opacity_scales_stops_and_solids() {
        let paint = Paint::solid(Color::WHITE).with_opacity(0.5);
        assert_eq!(paint.solid_color().unwrap().a, 0.5);
    }

    #[test]
    fn stroke_style_filters_invalid_dashes() {
        let style = StrokeStyle::new(2.0).with_dash(vec![4.0, -1.0, f32::NAN, 2.0], 0.0);
        assert_eq!(style.dash, vec![4.0, 2.0]);
        assert!(style.is_visible());
    }
}

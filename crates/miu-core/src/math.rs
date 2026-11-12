//! Vector, rectangle, and affine-transform primitives.
//!
//! All geometry in Miu is expressed in one of three spaces:
//!
//! * **local** — relative to an object's pivot,
//! * **world** — the scene coordinate system shared by nodes and cameras,
//! * **screen** — logical pixels with a top-left origin, produced by a camera.
//!
//! [`Affine2`] converts between them. The stored layout models the matrix
//! `[a c e; b d f]`, so `x' = a*x + c*y + e` and `y' = b*x + d*y + f`.

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Two-dimensional vector with `f32` components.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0);
    /// Both components equal to one.
    pub const ONE: Self = Self::new(1.0, 1.0);
    /// Unit vector along +X.
    pub const X: Self = Self::new(1.0, 0.0);
    /// Unit vector along +Y.
    pub const Y: Self = Self::new(0.0, 1.0);

    /// Creates a vector from components.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Creates a vector whose components share one value.
    pub const fn splat(value: f32) -> Self {
        Self::new(value, value)
    }

    /// Creates a unit vector pointing at `radians` (counter-clockwise from +X).
    pub fn from_angle(radians: f32) -> Self {
        Self::new(radians.cos(), radians.sin())
    }

    /// Squared Euclidean length; cheaper than [`Self::length`] for comparisons.
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Euclidean length.
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Distance to `other`.
    pub fn distance(self, other: Self) -> f32 {
        (other - self).length()
    }

    /// Dot product.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Z component of the 3D cross product; sign encodes turn direction.
    pub fn cross(self, other: Self) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// Returns a unit vector, or [`Vec2::ZERO`] when the length is not positive.
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > f32::MIN_POSITIVE {
            self * (1.0 / length)
        } else {
            Self::ZERO
        }
    }

    /// Rotated 90 degrees counter-clockwise.
    pub fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }

    /// Component-wise minimum.
    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// Component-wise maximum.
    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }

    /// Absolute value of each component.
    pub fn abs(self) -> Self {
        Self::new(self.x.abs(), self.y.abs())
    }

    /// Rounds each component towards negative infinity.
    pub fn floor(self) -> Self {
        Self::new(self.x.floor(), self.y.floor())
    }

    /// Rounds each component towards positive infinity.
    pub fn ceil(self) -> Self {
        Self::new(self.x.ceil(), self.y.ceil())
    }

    /// Rounds each component to the nearest integer.
    pub fn round(self) -> Self {
        Self::new(self.x.round(), self.y.round())
    }

    /// Linear interpolation from `self` (t = 0) to `other` (t = 1).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    /// Rotates the vector counter-clockwise by `radians`.
    pub fn rotated(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }

    /// Angle of the vector in radians, measured from +X.
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    /// Whether both components are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Component-wise approximate equality.
    pub fn approx_eq(self, other: Self, epsilon: f32) -> bool {
        (self.x - other.x).abs() <= epsilon && (self.y - other.y).abs() <= epsilon
    }

    /// Converts to a `[f32; 2]` array.
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

/// Axis-aligned rectangle stored as an origin and a size.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width; negative values are invalid input for most APIs.
    pub width: f32,
    /// Height; negative values are invalid input for most APIs.
    pub height: f32,
}

impl Rect {
    /// An empty rectangle at the origin.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// Creates a rectangle from an origin and a size.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Creates a rectangle from two corners in any order.
    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    /// Creates a rectangle centred on `center`.
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self::new(center.x - size.x * 0.5, center.y - size.y * 0.5, size.x, size.y)
    }

    /// Top-left corner.
    pub const fn min(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// Bottom-right corner.
    pub const fn max(self) -> Vec2 {
        Vec2::new(self.x + self.width, self.y + self.height)
    }

    /// Width and height as a vector.
    pub const fn size(self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }

    /// Centre point.
    pub const fn center(self) -> Vec2 {
        Vec2::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    /// Whether the rectangle has no area.
    pub fn is_empty(self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    /// Whether the rectangle has finite, non-negative extents.
    pub fn is_valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }

    /// Whether `point` lies inside the half-open rectangle.
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.width
            && point.y < self.y + self.height
    }

    /// Whether two rectangles share any area.
    pub fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    /// Overlapping area, or `None` when the rectangles are disjoint.
    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }
        let min = self.min().max(other.min());
        let max = self.max().min(other.max());
        Some(Self::from_min_max(min, max))
    }

    /// Smallest rectangle covering both inputs.
    pub fn union(self, other: Self) -> Self {
        let min = self.min().min(other.min());
        let max = self.max().max(other.max());
        Self::from_min_max(min, max)
    }

    /// Grows each edge by `amount` (negative values shrink).
    pub fn expand(self, amount: f32) -> Self {
        Self::new(
            self.x - amount,
            self.y - amount,
            self.width + amount * 2.0,
            self.height + amount * 2.0,
        )
    }

    /// Shrinks the rectangle by inset edges, clamping to an empty rectangle.
    pub fn inset(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        let width = (self.width - left - right).max(0.0);
        let height = (self.height - top - bottom).max(0.0);
        Self::new(self.x + left, self.y + top, width, height)
    }

    /// Moves the rectangle by `offset`.
    pub fn translated(self, offset: Vec2) -> Self {
        Self::new(self.x + offset.x, self.y + offset.y, self.width, self.height)
    }

    /// Rounds the edges outwards to whole pixels and clamps them to a canvas.
    pub fn to_pixel_bounds(self, canvas_width: u32, canvas_height: u32) -> (u32, u32, u32, u32) {
        let min_x = self.x.max(0.0).floor();
        let min_y = self.y.max(0.0).floor();
        let max_x = (self.x + self.width).min(canvas_width as f32).ceil();
        let max_y = (self.y + self.height).min(canvas_height as f32).ceil();
        if !(max_x > min_x && max_y > min_y) {
            return (0, 0, 0, 0);
        }
        (min_x as u32, min_y as u32, max_x as u32, max_y as u32)
    }
}

/// Position, rotation, and scale for a 2D object.
///
/// This is the friendly editing form used by scene nodes and the C ABI.
/// [`Transform2D::to_affine`] converts it to a matrix for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Transform2D {
    /// Translation in the parent space.
    pub translation: Vec2,
    /// Clockwise rotation in radians (screen Y grows downwards).
    pub rotation_radians: f32,
    /// Per-axis scale; negative values mirror.
    pub scale: Vec2,
}

impl Transform2D {
    /// Identity transform.
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation_radians: 0.0,
        scale: Vec2::ONE,
    };

    /// Creates a transform.
    pub const fn new(translation: Vec2, rotation_radians: f32, scale: Vec2) -> Self {
        Self { translation, rotation_radians, scale }
    }

    /// Creates a translation-only transform.
    pub const fn from_translation(translation: Vec2) -> Self {
        Self { translation, rotation_radians: 0.0, scale: Vec2::ONE }
    }

    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        self.translation.is_finite()
            && self.rotation_radians.is_finite()
            && self.scale.is_finite()
    }

    /// Converts to the equivalent affine matrix.
    pub fn to_affine(self) -> Affine2 {
        Affine2::from_transform(self)
    }

    /// Transforms a point in local space.
    pub fn transform_point(self, point: Vec2) -> Vec2 {
        self.to_affine().transform_point(point)
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// 2D affine matrix `[a c e; b d f]`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Affine2 {
    /// Row 1, column 1 (`a`): X scale along X.
    pub m11: f32,
    /// Row 2, column 1 (`b`): X shear into Y.
    pub m12: f32,
    /// Row 1, column 2 (`c`): Y shear into X.
    pub m21: f32,
    /// Row 2, column 2 (`d`): Y scale along Y.
    pub m22: f32,
    /// Translation along X (`e`).
    pub tx: f32,
    /// Translation along Y (`f`).
    pub ty: f32,
}

impl Affine2 {
    /// Identity matrix.
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Creates a matrix from raw components.
    pub const fn new(m11: f32, m12: f32, m21: f32, m22: f32, tx: f32, ty: f32) -> Self {
        Self { m11, m12, m21, m22, tx, ty }
    }

    /// Matrix that translates points by `offset`.
    pub const fn from_translation(offset: Vec2) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, offset.x, offset.y)
    }

    /// Matrix that scales points around the origin.
    pub const fn from_scale(scale: Vec2) -> Self {
        Self::new(scale.x, 0.0, 0.0, scale.y, 0.0, 0.0)
    }

    /// Matrix that rotates points counter-clockwise around the origin.
    pub fn from_rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// Converts an editing transform into a matrix.
    pub fn from_transform(transform: Transform2D) -> Self {
        let sin = transform.rotation_radians.sin();
        let cos = transform.rotation_radians.cos();
        let sx = transform.scale.x;
        let sy = transform.scale.y;
        Self::new(
            cos * sx,
            sin * sx,
            -sin * sy,
            cos * sy,
            transform.translation.x,
            transform.translation.y,
        )
    }

    /// Matrix that maps `source` onto `destination` (axis-aligned rectangles).
    pub fn from_rect_to_rect(source: Rect, destination: Rect) -> Self {
        let sx = if source.width != 0.0 { destination.width / source.width } else { 1.0 };
        let sy = if source.height != 0.0 { destination.height / source.height } else { 1.0 };
        Self::new(
            sx,
            0.0,
            0.0,
            sy,
            destination.x - source.x * sx,
            destination.y - source.y * sy,
        )
    }

    /// Returns the matrix that applies `self` first and `next` afterwards.
    pub fn then(self, next: Self) -> Self {
        let a = self.m11;
        let b = self.m12;
        let c = self.m21;
        let d = self.m22;
        let e = self.tx;
        let f = self.ty;
        let na = next.m11;
        let nb = next.m12;
        let nc = next.m21;
        let nd = next.m22;
        let ne = next.tx;
        let nf = next.ty;
        Self::new(
            na * a + nc * b,
            nb * a + nd * b,
            na * c + nc * d,
            nb * c + nd * d,
            na * e + nc * f + ne,
            nb * e + nd * f + nf,
        )
    }

    /// Transforms a point (including translation).
    pub fn transform_point(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.m11 * point.x + self.m21 * point.y + self.tx,
            self.m12 * point.x + self.m22 * point.y + self.ty,
        )
    }

    /// Transforms a direction (ignoring translation).
    pub fn transform_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(
            self.m11 * vector.x + self.m21 * vector.y,
            self.m12 * vector.x + self.m22 * vector.y,
        )
    }

    /// Determinant of the linear part.
    pub fn determinant(self) -> f32 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    /// Inverts the matrix, or returns `None` when it is singular.
    pub fn inverse(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::MIN_POSITIVE {
            return None;
        }
        let inverse_determinant = 1.0 / determinant;
        Some(Self::new(
            self.m22 * inverse_determinant,
            -self.m12 * inverse_determinant,
            -self.m21 * inverse_determinant,
            self.m11 * inverse_determinant,
            (self.m21 * self.ty - self.m22 * self.tx) * inverse_determinant,
            (self.m12 * self.tx - self.m11 * self.ty) * inverse_determinant,
        ))
    }

    /// Average of the absolute axis scale factors; used to pick raster resolution.
    pub fn approximate_scale(self) -> f32 {
        let x = Vec2::new(self.m11, self.m12).length();
        let y = Vec2::new(self.m21, self.m22).length();
        let average = (x + y) * 0.5;
        if average.is_finite() && average > 0.0 {
            average
        } else {
            1.0
        }
    }

    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        self.m11.is_finite()
            && self.m12.is_finite()
            && self.m21.is_finite()
            && self.m22.is_finite()
            && self.tx.is_finite()
            && self.ty.is_finite()
    }
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Clamps `value` to `[min, max]` and maps NaN to `min`.
pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value.is_nan() {
        min
    } else {
        value.max(min).min(max)
    }
}

/// Converts degrees to radians.
pub fn to_radians(degrees: f32) -> f32 {
    degrees * (core::f32::consts::PI / 180.0)
}

/// Converts radians to degrees.
pub fn to_degrees(radians: f32) -> f32 {
    radians * (180.0 / core::f32::consts::PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_composition_applies_left_matrix_first() {
        let translate = Affine2::from_translation(Vec2::new(10.0, 0.0));
        let scale = Affine2::from_scale(Vec2::new(2.0, 2.0));
        let composed = translate.then(scale);
        assert_eq!(composed.transform_point(Vec2::new(1.0, 0.0)), Vec2::new(22.0, 0.0));
    }

    #[test]
    fn affine_inverse_round_trips_points() {
        let matrix = Affine2::from_rotation(0.4)
            .then(Affine2::from_scale(Vec2::new(1.5, 0.75)))
            .then(Affine2::from_translation(Vec2::new(30.0, -12.0)));
        let inverse = matrix.inverse().expect("matrix is invertible");
        let point = Vec2::new(7.25, -3.5);
        let returned = inverse.transform_point(matrix.transform_point(point));
        assert!(returned.approx_eq(point, 0.0001));
    }

    #[test]
    fn singular_matrix_has_no_inverse() {
        let matrix = Affine2::new(0.0, 0.0, 0.0, 0.0, 5.0, 5.0);
        assert!(matrix.inverse().is_none());
    }

    #[test]
    fn transform_matches_manual_rotation_and_scale() {
        let transform = Transform2D::new(Vec2::new(3.0, 4.0), core::f32::consts::FRAC_PI_2, Vec2::new(2.0, 2.0));
        let point = transform.transform_point(Vec2::new(1.0, 0.0));
        assert!(point.approx_eq(Vec2::new(3.0, 6.0), 0.0001));
    }

    #[test]
    fn rect_intersection_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        let overlap = a.intersection(b).expect("rectangles overlap");
        assert_eq!(overlap, Rect::new(5.0, 5.0, 5.0, 5.0));
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert!(Rect::new(100.0, 100.0, 1.0, 1.0).intersection(a).is_none());
    }

    #[test]
    fn pixel_bounds_clamp_to_canvas() {
        let bounds = Rect::new(-4.5, 2.5, 20.0, 100.0).to_pixel_bounds(16, 16);
        assert_eq!(bounds, (0, 2, 16, 16));
    }
}

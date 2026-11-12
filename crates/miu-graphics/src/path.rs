//! Vector outlines: construction, flattening, hit testing, and stroking.
//!
//! A [`Path`] stores drawing verbs plus their control points. Geometry is kept in
//! local space and transformed at rasterisation time, so one path can be drawn at
//! any position, scale, or rotation without rebuilding it.
//!
//! Curves are quadratic or cubic Béziers; [`Path::flatten_contours`] converts them
//! to polylines within a caller-supplied tolerance. [`Path::stroke_to_fill`] turns a
//! stroked outline into a fillable path that the rasteriser fills with the non-zero
//! rule, which is how dashed lines, caps, and joins are implemented.

use miu_core::{Affine2, Rect, Vec2};

use crate::paint::{LineCap, LineJoin, StrokeStyle};

/// Filling rule for self-intersecting outlines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FillRule {
    /// A point is inside when its winding number is non-zero.
    #[default]
    NonZero = 0,
    /// A point is inside when it is crossed an odd number of times.
    EvenOdd = 1,
}

/// One drawing verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Start a new contour at one point.
    MoveTo,
    /// Add a straight segment.
    LineTo,
    /// Add a quadratic Bézier segment using one control point.
    QuadTo,
    /// Add a cubic Bézier segment using two control points.
    CubicTo,
    /// Close the current contour back to its first point.
    Close,
}

impl Verb {
    /// Number of points the verb consumes.
    pub const fn point_count(self) -> usize {
        match self {
            Self::MoveTo | Self::LineTo => 1,
            Self::QuadTo => 2,
            Self::CubicTo => 3,
            Self::Close => 0,
        }
    }
}

/// A flattened contour produced by [`Path::flatten_contours`].
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    /// Polyline points in path-local coordinates.
    pub points: Vec<Vec2>,
    /// Whether the contour was explicitly closed.
    pub closed: bool,
}

/// Ordered drawing verbs and their points.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    verbs: Vec<Verb>,
    points: Vec<Vec2>,
}

impl Path {
    /// Creates an empty path.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a [`PathBuilder`] for incremental construction.
    pub fn builder() -> PathBuilder {
        PathBuilder::new()
    }

    /// Whether the path has no verbs.
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Number of verbs.
    pub fn verb_count(&self) -> usize {
        self.verbs.len()
    }

    /// Verbs in drawing order.
    pub fn verbs(&self) -> &[Verb] {
        &self.verbs
    }

    /// Control points in verb order.
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Appends a rectangle as a closed contour.
    pub fn rect(rect: Rect) -> Self {
        let mut builder = PathBuilder::new();
        builder.rect(rect);
        builder.build()
    }

    /// Appends a rounded rectangle as a closed contour.
    pub fn round_rect(rect: Rect, radius: f32) -> Self {
        let mut builder = PathBuilder::new();
        builder.round_rect(rect, radius);
        builder.build()
    }

    /// Appends an ellipse as a closed contour.
    pub fn ellipse(center: Vec2, radii: Vec2) -> Self {
        let mut builder = PathBuilder::new();
        builder.ellipse(center, radii);
        builder.build()
    }

    /// Appends a circle as a closed contour.
    pub fn circle(center: Vec2, radius: f32) -> Self {
        Self::ellipse(center, Vec2::splat(radius))
    }

    /// Builds an open polyline through `points`.
    pub fn polyline(points: &[Vec2], closed: bool) -> Self {
        let mut builder = PathBuilder::new();
        if let Some(first) = points.first() {
            builder.move_to(*first);
            for point in points.iter().skip(1) {
                builder.line_to(*point);
            }
            if closed {
                builder.close();
            }
        }
        builder.build()
    }

    /// Returns a transformed copy, applying `matrix` to every control point.
    ///
    /// Affine transforms map Bézier control points to Bézier control points, so the
    /// result is exact rather than an approximation.
    pub fn transformed(&self, matrix: &Affine2) -> Self {
        let mut path = self.clone();
        for point in &mut path.points {
            *point = matrix.transform_point(*point);
        }
        path
    }

    /// Axis-aligned bounds of the flattened outline.
    pub fn bounds(&self) -> Rect {
        let contours = self.flatten_contours(0.1);
        let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut found = false;
        for contour in &contours {
            for point in &contour.points {
                minimum = minimum.min(*point);
                maximum = maximum.max(*point);
                found = true;
            }
        }
        if !found {
            return Rect::ZERO;
        }
        Rect::from_min_max(minimum, maximum)
    }

    /// Flattens every contour to line segments.
    ///
    /// `tolerance` is the maximum distance in local units between the true curve
    /// and the emitted polyline. Smaller values produce more points.
    pub fn flatten_contours(&self, tolerance: f32) -> Vec<Contour> {
        let tolerance = if tolerance.is_finite() && tolerance > 0.0 {
            tolerance
        } else {
            0.25
        };
        let mut contours: Vec<Contour> = Vec::new();
        let mut current: Vec<Vec2> = Vec::new();
        let mut closed = false;
        let mut cursor = Vec2::ZERO;
        let mut start = Vec2::ZERO;
        let mut index = 0usize;

        for verb in &self.verbs {
            match verb {
                Verb::MoveTo => {
                    if current.len() > 1 || (current.len() == 1 && closed) {
                        contours.push(Contour { points: current, closed });
                    }
                    current = Vec::new();
                    closed = false;
                    cursor = self.points.get(index).copied().unwrap_or(Vec2::ZERO);
                    start = cursor;
                    current.push(cursor);
                    index += 1;
                }
                Verb::LineTo => {
                    let point = self.points.get(index).copied().unwrap_or(cursor);
                    index += 1;
                    current.push(point);
                    cursor = point;
                }
                Verb::QuadTo => {
                    let control = self.points.get(index).copied().unwrap_or(cursor);
                    let point = self.points.get(index + 1).copied().unwrap_or(control);
                    index += 2;
                    flatten_quad(cursor, control, point, tolerance, 0, &mut current);
                    cursor = point;
                }
                Verb::CubicTo => {
                    let control_a = self.points.get(index).copied().unwrap_or(cursor);
                    let control_b = self.points.get(index + 1).copied().unwrap_or(control_a);
                    let point = self.points.get(index + 2).copied().unwrap_or(control_b);
                    index += 3;
                    flatten_cubic(cursor, control_a, control_b, point, tolerance, 0, &mut current);
                    cursor = point;
                }
                Verb::Close => {
                    if current.len() > 1 {
                        closed = true;
                        current.push(start);
                    }
                    cursor = start;
                }
            }
        }
        if current.len() > 1 || (current.len() == 1 && closed) {
            contours.push(Contour { points: current, closed });
        }
        contours
    }

    /// Total length of the flattened outline, in local units.
    pub fn length(&self, tolerance: f32) -> f32 {
        let mut total = 0.0;
        for contour in self.flatten_contours(tolerance) {
            for pair in contour.points.windows(2) {
                total += pair[0].distance(pair[1]);
            }
        }
        total
    }

    /// Whether a local-space point is inside the filled shape.
    pub fn contains(&self, point: Vec2, rule: FillRule) -> bool {
        let mut winding = 0i32;
        for contour in self.flatten_contours(0.2) {
            if contour.points.len() < 2 {
                continue;
            }
            let mut previous = contour.points[contour.points.len() - 1];
            for current in &contour.points {
                if previous.y <= point.y {
                    if current.y > point.y && cross(previous, *current, point) > 0.0 {
                        winding += 1;
                    }
                } else if current.y <= point.y && cross(previous, *current, point) < 0.0 {
                    winding -= 1;
                }
                previous = *current;
            }
        }
        match rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    }

    /// Converts a stroked outline into a fillable path.
    ///
    /// The result must be filled with [`FillRule::NonZero`]: every emitted piece has
    /// the same orientation, so overlapping pieces simply add up.
    pub fn stroke_to_fill(&self, style: &StrokeStyle) -> Path {
        let mut output = Path::new();
        if !style.is_visible() {
            return output;
        }
        let half_width = style.width * 0.5;
        let tolerance = (half_width * 0.05).max(0.05);
        for contour in self.flatten_contours(tolerance) {
            let mut points = contour.points;
            if points.len() < 2 {
                continue;
            }
            if contour.closed {
                if points.first().copied() != points.last().copied() {
                    points.push(points[0]);
                }
            } else {
                dedupe(&mut points);
            }
            let pieces = if style.dash.is_empty() {
                vec![(points.clone(), contour.closed)]
            } else {
                dash_polyline(&points, style)
            };
            for (piece, piece_closed) in pieces {
                stroke_polyline(
                    &mut output,
                    &piece,
                    half_width,
                    style,
                    piece_closed,
                );
            }
        }
        output
    }
}

/// Incremental [`Path`] construction.
#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    path: Path,
}

impl PathBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a contour at `point`.
    pub fn move_to(&mut self, point: Vec2) -> &mut Self {
        if !point.is_finite() {
            return self;
        }
        self.path.verbs.push(Verb::MoveTo);
        self.path.points.push(point);
        self
    }

    /// Adds a straight segment to `point`.
    pub fn line_to(&mut self, point: Vec2) -> &mut Self {
        if !point.is_finite() {
            return self;
        }
        self.path.verbs.push(Verb::LineTo);
        self.path.points.push(point);
        self
    }

    /// Adds a quadratic Bézier segment.
    pub fn quad_to(&mut self, control: Vec2, point: Vec2) -> &mut Self {
        if !control.is_finite() || !point.is_finite() {
            return self;
        }
        self.path.verbs.push(Verb::QuadTo);
        self.path.points.push(control);
        self.path.points.push(point);
        self
    }

    /// Adds a cubic Bézier segment.
    pub fn cubic_to(&mut self, control_a: Vec2, control_b: Vec2, point: Vec2) -> &mut Self {
        if !control_a.is_finite() || !control_b.is_finite() || !point.is_finite() {
            return self;
        }
        self.path.verbs.push(Verb::CubicTo);
        self.path.points.push(control_a);
        self.path.points.push(control_b);
        self.path.points.push(point);
        self
    }

    /// Closes the current contour.
    pub fn close(&mut self) -> &mut Self {
        self.path.verbs.push(Verb::Close);
        self
    }

    /// Adds a closed rectangle.
    pub fn rect(&mut self, rect: Rect) -> &mut Self {
        if !rect.is_valid() || rect.is_empty() {
            return self;
        }
        self.move_to(rect.min());
        self.line_to(Vec2::new(rect.x + rect.width, rect.y));
        self.line_to(rect.max());
        self.line_to(Vec2::new(rect.x, rect.y + rect.height));
        self.close()
    }

    /// Adds a closed rounded rectangle. Radius is clamped to half the shorter edge.
    pub fn round_rect(&mut self, rect: Rect, radius: f32) -> &mut Self {
        if !rect.is_valid() || rect.is_empty() {
            return self;
        }
        let limit = rect.width.min(rect.height) * 0.5;
        let radius = if radius.is_finite() { radius.clamp(0.0, limit) } else { 0.0 };
        if radius <= 0.0 {
            return self.rect(rect);
        }
        let left = rect.x;
        let top = rect.y;
        let right = rect.x + rect.width;
        let bottom = rect.y + rect.height;
        self.move_to(Vec2::new(left + radius, top));
        self.line_to(Vec2::new(right - radius, top));
        self.arc_corner(Vec2::new(right - radius, top + radius), radius, -core::f32::consts::FRAC_PI_2, core::f32::consts::FRAC_PI_2);
        self.line_to(Vec2::new(right, bottom - radius));
        self.arc_corner(Vec2::new(right - radius, bottom - radius), radius, 0.0, core::f32::consts::FRAC_PI_2);
        self.line_to(Vec2::new(left + radius, bottom));
        self.arc_corner(Vec2::new(left + radius, bottom - radius), radius, core::f32::consts::FRAC_PI_2, core::f32::consts::FRAC_PI_2);
        self.line_to(Vec2::new(left, top + radius));
        self.arc_corner(Vec2::new(left + radius, top + radius), radius, core::f32::consts::PI, core::f32::consts::FRAC_PI_2);
        self.close()
    }

    /// Adds a closed ellipse approximated by four cubic segments.
    pub fn ellipse(&mut self, center: Vec2, radii: Vec2) -> &mut Self {
        if !center.is_finite() || !radii.is_finite() {
            return self;
        }
        let rx = radii.x.abs();
        let ry = radii.y.abs();
        if rx <= 0.0 || ry <= 0.0 {
            return self;
        }
        // 0.5522847498 approximates a quarter circle with one cubic segment.
        let kappa = 0.552_284_8;
        let cx = center.x;
        let cy = center.y;
        self.move_to(Vec2::new(cx + rx, cy));
        self.cubic_to(
            Vec2::new(cx + rx, cy + ry * kappa),
            Vec2::new(cx + rx * kappa, cy + ry),
            Vec2::new(cx, cy + ry),
        );
        self.cubic_to(
            Vec2::new(cx - rx * kappa, cy + ry),
            Vec2::new(cx - rx, cy + ry * kappa),
            Vec2::new(cx - rx, cy),
        );
        self.cubic_to(
            Vec2::new(cx - rx, cy - ry * kappa),
            Vec2::new(cx - rx * kappa, cy - ry),
            Vec2::new(cx, cy - ry),
        );
        self.cubic_to(
            Vec2::new(cx + rx * kappa, cy - ry),
            Vec2::new(cx + rx, cy - ry * kappa),
            Vec2::new(cx + rx, cy),
        );
        self.close()
    }

    /// Adds a closed circle.
    pub fn circle(&mut self, center: Vec2, radius: f32) -> &mut Self {
        self.ellipse(center, Vec2::splat(radius))
    }

    /// Adds an elliptical arc as cubic segments, continuing from the current point.
    pub fn arc(
        &mut self,
        center: Vec2,
        radii: Vec2,
        start_angle: f32,
        sweep_angle: f32,
    ) -> &mut Self {
        if !center.is_finite() || !radii.is_finite() {
            return self;
        }
        let sweep = if sweep_angle.is_finite() { sweep_angle } else { 0.0 };
        if sweep.abs() < 1.0e-6 || radii.x.abs() <= 0.0 || radii.y.abs() <= 0.0 {
            return self;
        }
        let segments = ((sweep.abs() / core::f32::consts::FRAC_PI_2).ceil() as usize).max(1);
        let step = sweep / segments as f32;
        let mut angle = start_angle;
        if self.path.verbs.is_empty() {
            let start = Vec2::new(
                center.x + radii.x * angle.cos(),
                center.y + radii.y * angle.sin(),
            );
            self.move_to(start);
        }
        for _ in 0..segments {
            let next = angle + step;
            let kappa = (step.abs() * 0.25).tan() * (4.0 / 3.0);
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let cos_b = next.cos();
            let sin_b = next.sin();
            let start = Vec2::new(center.x + radii.x * cos_a, center.y + radii.y * sin_a);
            let end = Vec2::new(center.x + radii.x * cos_b, center.y + radii.y * sin_b);
            let control_a = Vec2::new(
                start.x - radii.x * sin_a * kappa * step.signum(),
                start.y + radii.y * cos_a * kappa * step.signum(),
            );
            let control_b = Vec2::new(
                end.x + radii.x * sin_b * kappa * step.signum(),
                end.y - radii.y * cos_b * kappa * step.signum(),
            );
            self.cubic_to(control_a, control_b, end);
            angle = next;
        }
        self
    }

    fn arc_corner(&mut self, center: Vec2, radius: f32, start_angle: f32, sweep: f32) -> &mut Self {
        self.arc(center, Vec2::splat(radius), start_angle, sweep)
    }

    /// Finishes construction.
    pub fn build(&self) -> Path {
        self.path.clone()
    }

    /// Finishes construction and consumes the builder.
    pub fn into_path(self) -> Path {
        self.path
    }
}

fn cross(origin: Vec2, a: Vec2, b: Vec2) -> f32 {
    (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x)
}

fn flatten_quad(
    start: Vec2,
    control: Vec2,
    end: Vec2,
    tolerance: f32,
    depth: u32,
    output: &mut Vec<Vec2>,
) {
    if depth >= 16 || quad_flatness(start, control, end) <= tolerance {
        output.push(end);
        return;
    }
    let mid_a = (start + control) * 0.5;
    let mid_b = (control + end) * 0.5;
    let middle = (mid_a + mid_b) * 0.5;
    flatten_quad(start, mid_a, middle, tolerance, depth + 1, output);
    flatten_quad(middle, mid_b, end, tolerance, depth + 1, output);
}

fn flatten_cubic(
    start: Vec2,
    control_a: Vec2,
    control_b: Vec2,
    end: Vec2,
    tolerance: f32,
    depth: u32,
    output: &mut Vec<Vec2>,
) {
    if depth >= 16 || cubic_flatness(start, control_a, control_b, end) <= tolerance {
        output.push(end);
        return;
    }
    let ab = (start + control_a) * 0.5;
    let bc = (control_a + control_b) * 0.5;
    let cd = (control_b + end) * 0.5;
    let abc = (ab + bc) * 0.5;
    let bcd = (bc + cd) * 0.5;
    let middle = (abc + bcd) * 0.5;
    flatten_cubic(start, ab, abc, middle, tolerance, depth + 1, output);
    flatten_cubic(middle, bcd, cd, end, tolerance, depth + 1, output);
}

fn quad_flatness(start: Vec2, control: Vec2, end: Vec2) -> f32 {
    let chord = end - start;
    let length = chord.length();
    if length <= f32::EPSILON {
        return (control - start).length();
    }
    let normal = Vec2::new(-chord.y, chord.x) / length;
    ((control - start).dot(normal)).abs()
}

fn cubic_flatness(start: Vec2, control_a: Vec2, control_b: Vec2, end: Vec2) -> f32 {
    let chord = end - start;
    let length = chord.length();
    if length <= f32::EPSILON {
        return (control_a - start).length().max((control_b - start).length());
    }
    let normal = Vec2::new(-chord.y, chord.x) / length;
    ((control_a - start).dot(normal))
        .abs()
        .max(((control_b - start).dot(normal)).abs())
}

fn dedupe(points: &mut Vec<Vec2>) {
    let mut index = 1;
    while index < points.len() {
        if points[index].distance(points[index - 1]) <= 1.0e-4 {
            points.remove(index);
        } else {
            index += 1;
        }
    }
}

/// Splits a polyline into dash pieces and returns each with its closure flag.
fn dash_polyline(points: &[Vec2], style: &StrokeStyle) -> Vec<(Vec<Vec2>, bool)> {
    let mut pattern: Vec<f32> = style.dash.clone();
    if pattern.is_empty() {
        return vec![(points.to_vec(), false)];
    }
    if pattern.len() % 2 == 1 {
        let repeated = pattern.clone();
        pattern.extend(repeated);
    }
    let pattern_length: f32 = pattern.iter().sum();
    if pattern_length <= 0.0 {
        return vec![(points.to_vec(), false)];
    }
    let mut offset = style.dash_offset.rem_euclid(pattern_length);
    let mut pattern_index = 0usize;
    let mut remaining = pattern[0] - offset;
    while remaining <= 0.0 {
        pattern_index = (pattern_index + 1) % pattern.len();
        remaining += pattern[pattern_index];
    }
    let mut on = pattern_index % 2 == 0;
    let mut pieces: Vec<(Vec<Vec2>, bool)> = Vec::new();
    let mut current: Vec<Vec2> = Vec::new();
    if on {
        current.push(points[0]);
    }
    for pair in points.windows(2) {
        let mut from = pair[0];
        let to = pair[1];
        let mut segment_length = from.distance(to);
        if segment_length <= 0.0 {
            continue;
        }
        let mut direction = (to - from) / segment_length;
        while segment_length > 0.0 {
            if remaining >= segment_length {
                remaining -= segment_length;
                if on {
                    current.push(to);
                }
                segment_length = 0.0;
            } else {
                let split = from + direction * remaining;
                if on {
                    current.push(split);
                    if current.len() > 1 {
                        pieces.push((std::mem::take(&mut current), false));
                    } else {
                        current.clear();
                    }
                }
                segment_length -= remaining;
                from = split;
                direction = (to - from).normalized();
                pattern_index = (pattern_index + 1) % pattern.len();
                remaining = pattern[pattern_index];
                on = pattern_index % 2 == 0;
                if on {
                    current.push(from);
                }
            }
        }
    }
    if current.len() > 1 {
        pieces.push((current, false));
    }
    pieces
}

fn stroke_polyline(
    output: &mut Path,
    points: &[Vec2],
    half_width: f32,
    style: &StrokeStyle,
    closed: bool,
) {
    if points.len() < 2 {
        return;
    }
    let mut unique: Vec<Vec2> = points.to_vec();
    if closed && unique.first().copied() == unique.last().copied() && unique.len() > 2 {
        unique.pop();
    }
    let count = unique.len();
    let mut segments: Vec<(Vec2, Vec2)> = Vec::new();
    for index in 0..count {
        let next = if index + 1 < count { index + 1 } else if closed { 0 } else { break };
        let from = unique[index];
        let to = unique[next];
        if from.distance(to) > 1.0e-5 {
            segments.push((from, to));
        }
    }
    if segments.is_empty() {
        return;
    }

    for (from, to) in &segments {
        let direction = (*to - *from).normalized();
        let normal = direction.perpendicular() * half_width;
        add_quad(
            output,
            *from + normal,
            *to + normal,
            *to - normal,
            *from - normal,
        );
    }

    let join_count = if closed { segments.len() } else { segments.len().saturating_sub(1) };
    for index in 0..join_count {
        let (from, to) = segments[index];
        let (next_from, next_to) = segments[(index + 1) % segments.len()];
        let incoming = (to - from).normalized();
        let outgoing = (next_to - next_from).normalized();
        if incoming.dot(outgoing) > 0.999_9 {
            continue;
        }
        let vertex = to;
        let normal_in = incoming.perpendicular() * half_width;
        let normal_out = outgoing.perpendicular() * half_width;
        match style.join {
            LineJoin::Round => add_circle(output, vertex, half_width),
            LineJoin::Bevel => {
                add_triangle(output, vertex + normal_in, vertex, vertex + normal_out);
                add_triangle(output, vertex - normal_in, vertex, vertex - normal_out);
            }
            LineJoin::Miter => {
                let mut miter_done = false;
                for side in [1.0f32, -1.0] {
                    let a = normal_in * side;
                    let b = normal_out * side;
                    let sum = a + b;
                    let length = sum.length();
                    if length <= 1.0e-5 {
                        continue;
                    }
                    let unit = sum / length;
                    let denominator = unit.dot(a / half_width);
                    if denominator <= 1.0 / style.miter_limit.max(1.0) {
                        add_triangle(output, vertex + a, vertex, vertex + b);
                        continue;
                    }
                    let miter = vertex + unit * (half_width / denominator);
                    add_triangle(output, vertex + a, miter, vertex + b);
                    miter_done = true;
                }
                if !miter_done {
                    add_triangle(output, vertex + normal_in, vertex, vertex + normal_out);
                    add_triangle(output, vertex - normal_in, vertex, vertex - normal_out);
                }
            }
        }
    }

    if !closed {
        let start_direction = (segments[0].1 - segments[0].0).normalized();
        let end_direction = (segments[segments.len() - 1].1 - segments[segments.len() - 1].0)
            .normalized();
        apply_cap(output, segments[0].0, start_direction * -1.0, half_width, style.cap);
        apply_cap(
            output,
            segments[segments.len() - 1].1,
            end_direction,
            half_width,
            style.cap,
        );
    }
}

fn apply_cap(output: &mut Path, point: Vec2, direction: Vec2, half_width: f32, cap: LineCap) {
    match cap {
        LineCap::Butt => {}
        LineCap::Round => add_circle(output, point, half_width),
        LineCap::Square => {
            let normal = direction.perpendicular() * half_width;
            let extension = direction * half_width;
            add_quad(
                output,
                point + normal,
                point + normal + extension,
                point - normal + extension,
                point - normal,
            );
        }
    }
}

fn add_quad(output: &mut Path, a: Vec2, b: Vec2, c: Vec2, d: Vec2) {
    output.verbs.push(Verb::MoveTo);
    output.points.push(a);
    output.verbs.push(Verb::LineTo);
    output.points.push(b);
    output.verbs.push(Verb::LineTo);
    output.points.push(c);
    output.verbs.push(Verb::LineTo);
    output.points.push(d);
    output.verbs.push(Verb::Close);
}

fn add_triangle(output: &mut Path, a: Vec2, b: Vec2, c: Vec2) {
    output.verbs.push(Verb::MoveTo);
    output.points.push(a);
    output.verbs.push(Verb::LineTo);
    output.points.push(b);
    output.verbs.push(Verb::LineTo);
    output.points.push(c);
    output.verbs.push(Verb::Close);
}

fn add_circle(output: &mut Path, center: Vec2, radius: f32) {
    let segments = if radius < 2.0 { 8 } else { 16 };
    let step = core::f32::consts::TAU / segments as f32;
    output.verbs.push(Verb::MoveTo);
    output.points.push(center + Vec2::new(radius, 0.0));
    for index in 1..segments {
        let angle = step * index as f32;
        output.verbs.push(Verb::LineTo);
        output.points.push(center + Vec2::new(radius * angle.cos(), radius * angle.sin()));
    }
    output.verbs.push(Verb::Close);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_bounds_match_input() {
        let path = Path::rect(Rect::new(2.0, 3.0, 10.0, 20.0));
        let bounds = path.bounds();
        assert!(bounds.min().approx_eq(Vec2::new(2.0, 3.0), 0.001));
        assert!(bounds.max().approx_eq(Vec2::new(12.0, 23.0), 0.001));
    }

    #[test]
    fn circle_contains_centre_and_not_corners() {
        let path = Path::circle(Vec2::ZERO, 5.0);
        assert!(path.contains(Vec2::ZERO, FillRule::NonZero));
        assert!(!path.contains(Vec2::new(4.5, 4.5), FillRule::NonZero));
        assert!(!path.contains(Vec2::new(10.0, 0.0), FillRule::EvenOdd));
    }

    #[test]
    fn flattening_respects_tolerance_for_curves() {
        let mut builder = PathBuilder::new();
        builder.move_to(Vec2::ZERO);
        builder.cubic_to(Vec2::new(0.0, 100.0), Vec2::new(100.0, 100.0), Vec2::new(100.0, 0.0));
        let path = builder.build();
        let coarse = path.flatten_contours(5.0);
        let fine = path.flatten_contours(0.1);
        assert!(fine[0].points.len() > coarse[0].points.len());
    }

    #[test]
    fn transform_maps_control_points_exactly() {
        let path = Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        let matrix = Affine2::from_scale(Vec2::new(2.0, 3.0))
            .then(Affine2::from_translation(Vec2::new(5.0, 5.0)));
        let moved = path.transformed(&matrix);
        let bounds = moved.bounds();
        assert!(bounds.min().approx_eq(Vec2::new(5.0, 5.0), 0.001));
        assert!(bounds.max().approx_eq(Vec2::new(25.0, 35.0), 0.001));
    }

    #[test]
    fn stroking_a_line_produces_a_filled_band() {
        let line = Path::polyline(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)], false);
        let stroke = line.stroke_to_fill(&StrokeStyle::new(2.0));
        assert!(!stroke.is_empty());
        assert!(stroke.contains(Vec2::new(5.0, 0.0), FillRule::NonZero));
        assert!(!stroke.contains(Vec2::new(5.0, 4.0), FillRule::NonZero));
    }

    #[test]
    fn round_caps_extend_beyond_the_endpoint() {
        let line = Path::polyline(&[Vec2::ZERO, Vec2::new(10.0, 0.0)], false);
        let square = line.stroke_to_fill(&StrokeStyle::new(4.0).with_cap(LineCap::Square));
        assert!(square.contains(Vec2::new(11.0, 0.0), FillRule::NonZero));
        let butt = line.stroke_to_fill(&StrokeStyle::new(4.0));
        assert!(!butt.contains(Vec2::new(11.0, 0.0), FillRule::NonZero));
    }

    #[test]
    fn dashed_strokes_leave_gaps() {
        let line = Path::polyline(&[Vec2::ZERO, Vec2::new(20.0, 0.0)], false);
        let dashed = line.stroke_to_fill(&StrokeStyle::new(2.0).with_dash(vec![4.0, 4.0], 0.0));
        assert!(dashed.contains(Vec2::new(2.0, 0.0), FillRule::NonZero));
        assert!(!dashed.contains(Vec2::new(6.0, 0.0), FillRule::NonZero));
    }

    #[test]
    fn round_rect_clamps_radius() {
        let path = Path::round_rect(Rect::new(0.0, 0.0, 10.0, 10.0), 100.0);
        let bounds = path.bounds();
        assert!(bounds.min().approx_eq(Vec2::ZERO, 0.1));
        assert!(bounds.max().approx_eq(Vec2::new(10.0, 10.0), 0.1));
        assert!(!path.contains(Vec2::new(0.2, 0.2), FillRule::NonZero));
        assert!(path.contains(Vec2::new(5.0, 5.0), FillRule::NonZero));
    }

    #[test]
    fn closed_rectangle_contour_is_reported_closed() {
        let path = Path::rect(Rect::new(0.0, 0.0, 4.0, 4.0));
        let contours = path.flatten_contours(0.1);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].closed);
        assert_eq!(contours[0].points.len(), 5);
    }
}

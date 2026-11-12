//! Anti-aliasing scanline rasteriser.
//!
//! [`rasterize`] converts a path and an affine transform into a [`CoverageMask`]:
//! one 8-bit coverage value per device pixel. Coverage is measured by sampling each
//! pixel row with several horizontal sub-scanlines and accumulating the horizontal
//! overlap of every filled span. This is an analytic horizontal estimate combined
//! with vertical supersampling, which is deterministic, allocation-light, and good
//! enough for text and UI edges.
//!
//! The mask is intentionally paint-agnostic. The caller composites it with a solid
//! colour, a gradient, or a glyph tint, so one rasterisation can be reused across
//! many colours.

use miu_core::{Affine2, Rect, Vec2};

use crate::path::{FillRule, Path};

/// Number of horizontal sample lines per pixel row.
///
/// Four lines give 17 distinct coverage levels per row, which matches the precision
/// of 8-bit coverage storage without visible stepping on curves.
const SUB_SCANLINES: u32 = 4;

/// Guard against paths that would allocate absurd amounts of memory.
const MAX_MASK_PIXELS: usize = 4096 * 4096;

/// Per-pixel coverage for one rasterised shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMask {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl CoverageMask {
    /// Creates a fully transparent mask at the given device position.
    pub fn new(left: i32, top: i32, width: u32, height: u32) -> Self {
        let length = width as usize * height as usize;
        Self {
            left,
            top,
            width,
            height,
            data: vec![0; length],
        }
    }

    /// Device-space X of the mask's left edge.
    pub const fn left(&self) -> i32 {
        self.left
    }

    /// Device-space Y of the mask's top edge.
    pub const fn top(&self) -> i32 {
        self.top
    }

    /// Mask width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Mask height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Raw row-major coverage bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable row-major coverage bytes, used by baking tools.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Coverage at a mask-relative coordinate.
    pub fn coverage(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data
            .get(y as usize * self.width as usize + x as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Device-space rectangle covered by this mask.
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.left as f32,
            self.top as f32,
            self.width as f32,
            self.height as f32,
        )
    }

    /// Whether the mask has no area.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Whether any pixel has non-zero coverage.
    pub fn has_coverage(&self) -> bool {
        self.data.iter().any(|value| *value > 0)
    }

    /// Total covered pixel area, in pixels.
    pub fn covered_area(&self) -> f64 {
        let mut total = 0.0;
        for value in &self.data {
            total += *value as f64 / 255.0;
        }
        total
    }

    /// Number of non-zero coverage bytes; a cheap allocation estimate.
    pub fn allocated_bytes(&self) -> usize {
        self.data.len()
    }
}

/// Rasterisation settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterOptions {
    /// How self-intersections are filled.
    pub fill_rule: FillRule,
    /// Optional device-space clip, applied before any pixel is written.
    pub clip: Option<Rect>,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            fill_rule: FillRule::NonZero,
            clip: None,
        }
    }
}

impl RasterOptions {
    /// Options using a fill rule and no clipping.
    pub const fn new(fill_rule: FillRule) -> Self {
        Self { fill_rule, clip: None }
    }

    /// Options clipped to a device-space rectangle.
    pub const fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Edge {
    fn top(self) -> f32 {
        self.y0.min(self.y1)
    }

    fn bottom(self) -> f32 {
        self.y0.max(self.y1)
    }

    fn direction(self) -> i32 {
        if self.y1 > self.y0 {
            1
        } else {
            -1
        }
    }

    fn x_at(self, y: f32) -> f32 {
        let span = self.y1 - self.y0;
        if span.abs() <= f32::MIN_POSITIVE {
            return self.x0;
        }
        let t = (y - self.y0) / span;
        self.x0 + (self.x1 - self.x0) * t
    }
}

/// Rasterises `path` through `transform`, returning coverage or `None` when the
/// shape is empty, clipped away, or too large to represent.
pub fn rasterize(path: &Path, transform: &Affine2, options: &RasterOptions) -> Option<CoverageMask> {
    if path.is_empty() || !transform.is_finite() {
        return None;
    }
    // Convert the local tolerance into device space so curves stay smooth when the
    // transform magnifies them.
    let scale = transform.approximate_scale();
    let tolerance = (0.15 / scale.max(0.01)).clamp(0.0005, 2.0);
    let contours = path.flatten_contours(tolerance);

    let mut edges: Vec<Edge> = Vec::new();
    for contour in &contours {
        let points = &contour.points;
        if points.len() < 2 {
            continue;
        }
        // Closed contours emit an extra edge from the last point back to the first;
        // open contours keep their endpoints open so strokes are not double-filled.
        let (start, skip) = if contour.closed {
            (points.len() - 1, 0)
        } else {
            (0, 1)
        };
        let mut previous = transform.transform_point(points[start]);
        for point in points.iter().skip(skip) {
            let current = transform.transform_point(*point);
            if previous.y != current.y && previous.is_finite() && current.is_finite() {
                edges.push(Edge { x0: previous.x, y0: previous.y, x1: current.x, y1: current.y });
            }
            previous = current;
        }
    }
    if edges.is_empty() {
        return None;
    }

    let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for edge in &edges {
        minimum = minimum.min(Vec2::new(edge.x0.min(edge.x1), edge.top()));
        maximum = maximum.max(Vec2::new(edge.x0.max(edge.x1), edge.bottom()));
    }
    let mut bounds = Rect::from_min_max(minimum, maximum);
    if let Some(clip) = options.clip {
        if !clip.is_valid() {
            return None;
        }
        bounds = bounds.intersection(clip)?;
    }
    if bounds.is_empty() {
        return None;
    }

    let left = bounds.x.floor() as i32;
    let top = bounds.y.floor() as i32;
    let right = bounds.x.max(bounds.max().x).ceil() as i32;
    let bottom = bounds.y.max(bounds.max().y).ceil() as i32;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width <= 0 || height <= 0 {
        return None;
    }
    let width = width as u32;
    let height = height as u32;
    let pixel_total = width as usize * height as usize;
    if pixel_total == 0 || pixel_total > MAX_MASK_PIXELS {
        return None;
    }

    let mut mask = CoverageMask::new(left, top, width, height);
    let mut row = vec![0f32; width as usize];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    let inverse_sub = 1.0 / SUB_SCANLINES as f32;
    let left_f32 = left as f32;
    let right_f32 = right as f32;

    for row_index in 0..height {
        for value in row.iter_mut() {
            *value = 0.0;
        }
        let base_y = top as f32 + row_index as f32;
        let mut any_crossing_in_row = false;
        for sub in 0..SUB_SCANLINES {
            let y = base_y + (sub as f32 + 0.5) * inverse_sub;
            crossings.clear();
            for edge in &edges {
                if y >= edge.top() && y < edge.bottom() {
                    crossings.push((edge.x_at(y), edge.direction()));
                }
            }
            if crossings.len() < 2 {
                continue;
            }
            any_crossing_in_row = true;
            crossings.sort_by(|left_pair, right_pair| left_pair.0.total_cmp(&right_pair.0));
            let mut winding = 0i32;
            let mut parity = false;
            let mut span_start = 0.0f32;
            for (x, direction) in &crossings {
                match options.fill_rule {
                    FillRule::NonZero => {
                        if winding == 0 {
                            span_start = *x;
                        }
                        winding += *direction;
                        if winding == 0 {
                            accumulate_span(&mut row, span_start, *x, left_f32, right_f32, inverse_sub);
                        }
                    }
                    FillRule::EvenOdd => {
                        parity = !parity;
                        if parity {
                            span_start = *x;
                        } else {
                            accumulate_span(&mut row, span_start, *x, left_f32, right_f32, inverse_sub);
                        }
                    }
                }
            }
        }
        if !any_crossing_in_row {
            continue;
        }
        let row_offset = row_index as usize * width as usize;
        for index in 0..width as usize {
            let value = (row[index] * 255.0 + 0.5) as u32;
            mask.data[row_offset + index] = value.min(255) as u8;
        }
    }

    if !mask.has_coverage() {
        return None;
    }
    Some(mask)
}

fn accumulate_span(
    row: &mut [f32],
    start: f32,
    end: f32,
    left: f32,
    right: f32,
    weight: f32,
) {
    if !(end > start) {
        return;
    }
    let start = start.max(left);
    let end = end.min(right);
    if !(end > start) {
        return;
    }
    let first = start.floor() as i32;
    let last = end.ceil() as i32;
    for index in first..last {
        let pixel_left = index as f32;
        let pixel_right = pixel_left + 1.0;
        let overlap = end.min(pixel_right) - start.max(pixel_left);
        if overlap > 0.0 {
            let target = index - left as i32;
            if target >= 0 && (target as usize) < row.len() {
                row[target as usize] += overlap * weight;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Path;

    fn solid_mask(rect: Rect) -> CoverageMask {
        let path = Path::rect(rect);
        rasterize(&path, &Affine2::IDENTITY, &RasterOptions::default()).expect("rectangle rasterises")
    }

    #[test]
    fn aligned_rectangle_is_fully_covered() {
        let mask = solid_mask(Rect::new(2.0, 3.0, 4.0, 6.0));
        assert_eq!((mask.left(), mask.top(), mask.width(), mask.height()), (2, 3, 4, 6));
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                assert_eq!(mask.coverage(x, y), 255, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn half_covered_edge_is_antialiased() {
        let mask = solid_mask(Rect::new(0.5, 0.0, 4.0, 4.0));
        let coverage = mask.coverage(0, 0);
        assert!((120..=140).contains(&coverage), "edge coverage was {coverage}");
    }

    #[test]
    fn clip_removes_pixels_and_bounds() {
        let path = Path::rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        let options = RasterOptions::default().with_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        let mask = rasterize(&path, &Affine2::IDENTITY, &options).expect("clip keeps a region");
        assert_eq!(mask.width(), 10);
        assert_eq!(mask.height(), 10);
    }

    #[test]
    fn empty_and_degenerate_paths_return_none() {
        assert!(
            rasterize(&Path::new(), &Affine2::IDENTITY, &RasterOptions::default()).is_none()
        );
        let line = Path::polyline(&[Vec2::ZERO, Vec2::new(10.0, 0.0)], false);
        assert!(rasterize(&line, &Affine2::IDENTITY, &RasterOptions::default()).is_none());
    }

    #[test]
    fn even_odd_rule_leaves_the_inner_contour_hole() {
        let mut builder = crate::path::PathBuilder::new();
        builder.rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        builder.rect(Rect::new(3.0, 3.0, 4.0, 4.0));
        let path = builder.build();
        let even_odd = rasterize(
            &path,
            &Affine2::IDENTITY,
            &RasterOptions::new(FillRule::EvenOdd),
        )
        .expect("outer ring is visible");
        assert_eq!(even_odd.coverage(5, 5), 0);
        assert_eq!(even_odd.coverage(1, 1), 255);

        let non_zero = rasterize(&path, &Affine2::IDENTITY, &RasterOptions::default())
            .expect("non-zero fills both contours");
        assert_eq!(non_zero.coverage(5, 5), 255);
    }

    #[test]
    fn scale_affects_mask_size_not_geometry() {
        let path = Path::rect(Rect::new(0.0, 0.0, 4.0, 4.0));
        let mask = rasterize(
            &path,
            &Affine2::from_scale(Vec2::splat(3.0)),
            &RasterOptions::default(),
        )
        .expect("scaled rectangle rasterises");
        assert_eq!(mask.width(), 12);
        assert_eq!(mask.height(), 12);
        assert_eq!(mask.coverage(6, 6), 255);
    }
}

//! Independent 2D camera and its interaction helpers.
//!
//! A [`Camera2D`] defines the mapping from world coordinates to logical screen
//! coordinates (top-left origin, Y down). Every scene owns at least one camera;
//! splitting the screen with several cameras (for example a world view plus a UI
//! view) is supported by rendering the same frame twice with different cameras.

use crate::math::{Affine2, Rect, Vec2, clamp_f32, to_radians};

/// Invalid camera input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraError {
    /// The camera position was not finite.
    InvalidPosition,
    /// A queried coordinate was not finite.
    InvalidCoordinate,
    /// Viewport dimensions were not finite and positive.
    InvalidViewport,
    /// Zoom was not finite and greater than zero.
    InvalidZoom,
    /// Rotation was not finite.
    InvalidRotation,
}

impl CameraError {
    /// Stable identifier for logs, diagnostics, and the C ABI status mapping.
    pub const fn id(self) -> &'static str {
        match self {
            Self::InvalidPosition => "camera.position",
            Self::InvalidCoordinate => "camera.coordinate",
            Self::InvalidViewport => "camera.viewport",
            Self::InvalidZoom => "camera.zoom",
            Self::InvalidRotation => "camera.rotation",
        }
    }
}

/// View transform for one 2D layer.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Camera2D {
    /// World-space point shown at the centre of the viewport.
    pub position: Vec2,
    /// Viewport size in logical pixels.
    pub viewport: Vec2,
    /// Uniform magnification; must be finite and greater than zero.
    pub zoom: f32,
    /// Clockwise screen rotation in radians.
    pub rotation_radians: f32,
}

impl Camera2D {
    /// Creates an unzoomed camera centred on the world origin.
    pub fn new(viewport: Vec2) -> Result<Self, CameraError> {
        let camera = Self {
            position: Vec2::ZERO,
            viewport,
            zoom: 1.0,
            rotation_radians: 0.0,
        };
        camera.validate()?;
        Ok(camera)
    }

    /// Creates a camera with explicit state, validating it.
    pub fn with_state(
        position: Vec2,
        viewport: Vec2,
        zoom: f32,
        rotation_radians: f32,
    ) -> Result<Self, CameraError> {
        let camera = Self { position, viewport, zoom, rotation_radians };
        camera.validate()?;
        Ok(camera)
    }

    /// Verifies that every field can produce a finite transform.
    pub fn validate(&self) -> Result<(), CameraError> {
        if !self.position.is_finite() {
            return Err(CameraError::InvalidPosition);
        }
        if !self.viewport.is_finite() || self.viewport.x <= 0.0 || self.viewport.y <= 0.0 {
            return Err(CameraError::InvalidViewport);
        }
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            return Err(CameraError::InvalidZoom);
        }
        if !self.rotation_radians.is_finite() {
            return Err(CameraError::InvalidRotation);
        }
        Ok(())
    }

    /// Matrix mapping world coordinates to logical screen coordinates.
    pub fn view_affine(&self) -> Result<Affine2, CameraError> {
        self.validate()?;
        Ok(Affine2::from_translation(-self.position)
            .then(Affine2::from_scale(Vec2::splat(self.zoom)))
            .then(Affine2::from_rotation(-self.rotation_radians))
            .then(Affine2::from_translation(self.viewport * 0.5)))
    }

    /// Matrix mapping logical screen coordinates back to world coordinates.
    pub fn screen_affine(&self) -> Result<Affine2, CameraError> {
        self.view_affine()?
            .inverse()
            .ok_or(CameraError::InvalidZoom)
    }

    /// Maps a world point to screen space.
    pub fn world_to_screen(&self, world: Vec2) -> Result<Vec2, CameraError> {
        if !world.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let screen = self.view_affine()?.transform_point(world);
        if !screen.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        Ok(screen)
    }

    /// Maps a screen point back to world space.
    pub fn screen_to_world(&self, screen: Vec2) -> Result<Vec2, CameraError> {
        if !screen.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let world = self.screen_affine()?.transform_point(screen);
        if !world.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        Ok(world)
    }

    /// World-space rectangle currently visible, useful for culling.
    pub fn visible_world_rect(&self) -> Result<Rect, CameraError> {
        let matrix = self.view_affine()?;
        let inverse = matrix.inverse().ok_or(CameraError::InvalidZoom)?;
        let corners = [
            inverse.transform_point(Vec2::ZERO),
            inverse.transform_point(Vec2::new(self.viewport.x, 0.0)),
            inverse.transform_point(self.viewport),
            inverse.transform_point(Vec2::new(0.0, self.viewport.y)),
        ];
        let mut minimum = corners[0];
        let mut maximum = corners[0];
        for corner in corners.iter().skip(1) {
            minimum = minimum.min(*corner);
            maximum = maximum.max(*corner);
        }
        Ok(Rect::from_min_max(minimum, maximum))
    }

    /// Screen-space rectangle covered by the viewport.
    pub fn viewport_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.viewport.x, self.viewport.y)
    }

    /// Resizes the viewport while preserving the world point at its centre.
    pub fn resize(&mut self, viewport: Vec2) -> Result<(), CameraError> {
        if !viewport.is_finite() || viewport.x <= 0.0 || viewport.y <= 0.0 {
            return Err(CameraError::InvalidViewport);
        }
        self.viewport = viewport;
        Ok(())
    }
}

/// Bounds and interaction rules for a [`Camera2D`].
///
/// The controller implements the input handling a 2D editor or game needs: drag
/// to pan, wheel (or pinch) to zoom around the cursor, rotate, and focus on content.
#[derive(Debug, Clone)]
pub struct CameraController {
    camera: Camera2D,
    min_zoom: f32,
    max_zoom: f32,
    pan_bounds: Option<Rect>,
}

impl CameraController {
    /// Creates a controller with zoom limits `0.05 .. 16`.
    pub fn new(camera: Camera2D) -> Self {
        Self {
            camera,
            min_zoom: 0.05,
            max_zoom: 16.0,
            pan_bounds: None,
        }
    }

    /// Sets the allowed zoom range. Values are normalised so `min <= max`.
    #[must_use]
    pub fn with_zoom_limits(mut self, min_zoom: f32, max_zoom: f32) -> Self {
        let (low, high) = if min_zoom <= max_zoom {
            (min_zoom, max_zoom)
        } else {
            (max_zoom, min_zoom)
        };
        self.min_zoom = clamp_f32(low, 1.0e-4, 1.0e6);
        self.max_zoom = clamp_f32(high, 1.0e-4, 1.0e6);
        self.camera.zoom = clamp_f32(self.camera.zoom, self.min_zoom, self.max_zoom);
        self
    }

    /// Restricts the camera centre to a world-space rectangle.
    #[must_use]
    pub fn with_pan_bounds(mut self, bounds: Rect) -> Self {
        if bounds.is_valid() && !bounds.is_empty() {
            self.pan_bounds = Some(bounds);
        }
        self
    }

    /// Current camera state.
    pub fn camera(&self) -> &Camera2D {
        &self.camera
    }

    /// Mutable camera state. Call [`CameraController::clamp`] afterwards if the
    /// caller changes zoom directly.
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// Replaces the camera, preserving the configured limits.
    pub fn set_camera(&mut self, camera: Camera2D) {
        self.camera = camera;
        self.clamp();
    }

    /// Minimum zoom the controller allows.
    pub fn min_zoom(&self) -> f32 {
        self.min_zoom
    }

    /// Maximum zoom the controller allows.
    pub fn max_zoom(&self) -> f32 {
        self.max_zoom
    }

    /// Pans by a screen-space delta, matching a drag gesture.
    pub fn pan_screen_delta(&mut self, delta: Vec2) {
        if !delta.is_finite() {
            return;
        }
        let view = match self.camera.view_affine() {
            Ok(view) => view,
            Err(_) => return,
        };
        // A screen delta becomes a world delta through the inverse linear part; the
        // camera rotation is compensated so dragging always follows the pointer.
        let inverse = match view.inverse() {
            Some(inverse) => inverse,
            None => return,
        };
        let world_delta = inverse.transform_vector(delta);
        self.camera.position -= world_delta;
        self.clamp();
    }

    /// Pans by a world-space delta.
    pub fn pan_world_delta(&mut self, delta: Vec2) {
        if !delta.is_finite() {
            return;
        }
        self.camera.position -= delta;
        self.clamp();
    }

    /// Multiplies zoom while keeping `anchor` (screen space) pointing at the same
    /// world position. This is the behaviour expected from a mouse wheel.
    pub fn zoom_at(&mut self, anchor: Vec2, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 || !anchor.is_finite() {
            return;
        }
        let before = match self.camera.screen_to_world(anchor) {
            Ok(world) => world,
            Err(_) => return,
        };
        self.camera.zoom = clamp_f32(self.camera.zoom * factor, self.min_zoom, self.max_zoom);
        let after = match self.camera.screen_to_world(anchor) {
            Ok(world) => world,
            Err(_) => return,
        };
        self.camera.position += before - after;
        self.clamp();
    }

    /// Zooms around the viewport centre, as keyboard shortcuts do.
    pub fn zoom_centre(&mut self, factor: f32) {
        self.zoom_at(self.camera.viewport * 0.5, factor);
    }

    /// Sets an absolute zoom around the viewport centre.
    pub fn set_zoom(&mut self, zoom: f32) {
        if !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        let anchor = self.camera.viewport * 0.5;
        let target = clamp_f32(zoom, self.min_zoom, self.max_zoom);
        let factor = target / self.camera.zoom;
        self.zoom_at(anchor, factor);
    }

    /// Rotates the camera by a delta in radians.
    pub fn rotate_by(&mut self, radians: f32) {
        if !radians.is_finite() {
            return;
        }
        self.camera.rotation_radians = (self.camera.rotation_radians + radians)
            .rem_euclid(core::f32::consts::TAU);
    }

    /// Rotates the camera by a delta in degrees.
    pub fn rotate_by_degrees(&mut self, degrees: f32) {
        self.rotate_by(to_radians(degrees));
    }

    /// Moves the camera so that `world` is centred.
    pub fn focus_on(&mut self, world: Vec2) {
        if !world.is_finite() {
            return;
        }
        self.camera.position = world;
        self.clamp();
    }

    /// Frames `area` inside the viewport with an optional margin in logical pixels.
    pub fn fit_rect(&mut self, area: Rect, margin_pixels: f32) {
        if !area.is_valid() || area.is_empty() {
            return;
        }
        let margin = if margin_pixels.is_finite() { margin_pixels.max(0.0) } else { 0.0 };
        let available = Vec2::new(
            (self.camera.viewport.x - margin * 2.0).max(1.0),
            (self.camera.viewport.y - margin * 2.0).max(1.0),
        );
        let zoom = (available.x / area.width).min(available.y / area.height);
        self.camera.zoom = clamp_f32(zoom, self.min_zoom, self.max_zoom);
        self.camera.position = area.center();
        self.camera.rotation_radians = 0.0;
        self.clamp();
    }

    /// Resets zoom, rotation, and position to the origin-centred default.
    pub fn reset(&mut self) {
        self.camera.position = Vec2::ZERO;
        self.camera.zoom = 1.0;
        self.camera.rotation_radians = 0.0;
    }

    /// Resizes the viewport, keeping the world point at the old centre visible.
    pub fn resize(&mut self, viewport: Vec2) {
        if self.camera.resize(viewport).is_ok() {
            self.clamp();
        }
    }

    /// Applies zoom and pan limits. Called after every mutation.
    pub fn clamp(&mut self) {
        self.camera.zoom = clamp_f32(self.camera.zoom, self.min_zoom, self.max_zoom);
        if let Some(bounds) = self.pan_bounds {
            self.camera.position.x = clamp_f32(
                self.camera.position.x,
                bounds.min().x,
                bounds.max().x,
            );
            self.camera.position.y = clamp_f32(
                self.camera.position.y,
                bounds.min().y,
                bounds.max().y,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera2D {
        Camera2D::new(Vec2::new(800.0, 600.0)).expect("default viewport is valid")
    }

    #[test]
    fn centre_maps_to_viewport_centre() {
        let mut camera = camera();
        camera.position = Vec2::new(12.0, -4.0);
        let centre = camera.world_to_screen(camera.position).unwrap();
        assert!(centre.approx_eq(Vec2::new(400.0, 300.0), 0.0001));
    }

    #[test]
    fn screen_world_round_trip() {
        let mut camera = camera();
        camera.position = Vec2::new(100.0, -20.0);
        camera.zoom = 1.5;
        camera.rotation_radians = 0.37;
        let point = Vec2::new(218.0, 51.5);
        let returned = camera.screen_to_world(camera.world_to_screen(point).unwrap()).unwrap();
        assert!(returned.approx_eq(point, 0.001));
    }

    #[test]
    fn visible_rect_covers_viewport() {
        let camera = camera();
        let visible = camera.visible_world_rect().unwrap();
        assert!((visible.width - 800.0).abs() < 0.01);
        assert!((visible.height - 600.0).abs() < 0.01);
    }

    #[test]
    fn zoom_at_keeps_anchor_stable() {
        let mut controller = CameraController::new(camera());
        let anchor = Vec2::new(640.0, 120.0);
        let before = controller.camera().screen_to_world(anchor).unwrap();
        controller.zoom_at(anchor, 2.0);
        let after = controller.camera().screen_to_world(anchor).unwrap();
        assert!(before.approx_eq(after, 0.001));
        assert!((controller.camera().zoom - 2.0).abs() < 0.0001);
    }

    #[test]
    fn zoom_limits_are_enforced() {
        let mut controller = CameraController::new(camera()).with_zoom_limits(0.5, 4.0);
        controller.set_zoom(100.0);
        assert!((controller.camera().zoom - 4.0).abs() < 0.0001);
        controller.set_zoom(0.0001);
        assert!((controller.camera().zoom - 0.5).abs() < 0.0001);
    }

    #[test]
    fn dragging_follows_the_pointer() {
        let mut controller = CameraController::new(camera());
        let grabbed = Vec2::new(100.0, 100.0);
        let world = controller.camera().screen_to_world(grabbed).unwrap();
        controller.pan_screen_delta(Vec2::new(50.0, -25.0));
        let moved = controller.camera().world_to_screen(world).unwrap();
        assert!(moved.approx_eq(Vec2::new(150.0, 75.0), 0.001));
    }

    #[test]
    fn fit_rect_frames_content() {
        let mut controller = CameraController::new(camera());
        controller.fit_rect(Rect::new(-100.0, -50.0, 400.0, 200.0), 0.0);
        assert!((controller.camera().zoom - 2.0).abs() < 0.0001);
        assert!(controller.camera().position.approx_eq(Vec2::new(100.0, 50.0), 0.0001));
    }

    #[test]
    fn invalid_cameras_are_rejected() {
        assert_eq!(Camera2D::new(Vec2::ZERO), Err(CameraError::InvalidViewport));
        assert!(Camera2D::with_state(Vec2::ZERO, Vec2::ONE, 0.0, 0.0).is_err());
    }
}

//! Platform-neutral policy and frame-preparation primitives for the Miu 2D renderer.
//!
//! This crate has no window-system or GPU dependency. A platform bridge reports the
//! available APIs, then [`select_renderer`] creates an auditable render plan before
//! any graphics resources are allocated.

#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]

use core::cmp::Ordering;

/// Operating-system or browser family used to choose a backend catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Platform {
    /// Microsoft Windows.
    Windows = 1,
    /// Android.
    Android = 2,
    /// macOS.
    MacOS = 3,
    /// iOS or iPadOS.
    IOS = 4,
    /// Linux and Linux-based desktop systems.
    Linux = 5,
    /// A browser/web host.
    Web = 6,
}

/// Quality/performance policy selected by an application.
///
/// A tier is a project minimum, not a promise that every visual effect exists on
/// every backend. Hosts should expose the selected [`RenderPlan`] to diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RenderTier {
    /// Prioritises reach and low overhead on older hardware.
    Compatibility = 1,
    /// Balances visual quality and current mainstream hardware performance.
    Balanced = 2,
    /// Enables the strongest backend profile available to the application.
    Premium = 3,
}

/// A concrete API profile that can execute Miu commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Backend {
    /// Direct2D 1.2.
    Direct2D12 = 1,
    /// Direct3D 11.3.
    Direct3D113 = 2,
    /// Direct3D 12.
    Direct3D12 = 3,
    /// OpenGL ES 3.0.
    OpenGLES30 = 4,
    /// OpenGL ES 2.0 emergency fallback.
    OpenGLES20 = 5,
    /// Metal 2.
    Metal2 = 6,
    /// Metal 3.
    Metal3 = 7,
    /// Metal 1 emergency fallback.
    Metal1 = 8,
    /// Vulkan 1.1.
    Vulkan11 = 9,
    /// Vulkan 1.2.
    Vulkan12 = 10,
    /// Vulkan 1.3.
    Vulkan13 = 11,
    /// Vulkan 1.4.
    Vulkan14 = 12,
    /// Desktop OpenGL 3.3 emergency fallback.
    OpenGL33 = 13,
    /// Desktop OpenGL 3.0 emergency fallback.
    OpenGL30 = 14,
    /// WebGL 2.
    WebGL2 = 15,
    /// WebGL 1 emergency fallback.
    WebGL1 = 16,
    /// WebGPU.
    WebGPU = 17,
}

impl Backend {
    /// Stable, lower-case identifier for configuration, logs, and telemetry.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Direct2D12 => "direct2d-1.2",
            Self::Direct3D113 => "direct3d-11.3",
            Self::Direct3D12 => "direct3d-12",
            Self::OpenGLES30 => "opengles-3.0",
            Self::OpenGLES20 => "opengles-2.0",
            Self::Metal2 => "metal-2",
            Self::Metal3 => "metal-3",
            Self::Metal1 => "metal-1",
            Self::Vulkan11 => "vulkan-1.1",
            Self::Vulkan12 => "vulkan-1.2",
            Self::Vulkan13 => "vulkan-1.3",
            Self::Vulkan14 => "vulkan-1.4",
            Self::OpenGL33 => "opengl-3.3",
            Self::OpenGL30 => "opengl-3.0",
            Self::WebGL2 => "webgl-2",
            Self::WebGL1 => "webgl-1",
            Self::WebGPU => "webgpu",
        }
    }

    /// Whether this implementation can satisfy a request for `required`.
    ///
    /// The comparison intentionally models only same-family API guarantees; it does
    /// not assume a Direct3D 12 driver can create every Direct3D 11 feature level.
    pub const fn satisfies(self, required: Self) -> bool {
        if self as u8 == required as u8 {
            return true;
        }
        matches!(
            (self, required),
            (Self::Vulkan14, Self::Vulkan13 | Self::Vulkan12 | Self::Vulkan11)
                | (Self::Vulkan13, Self::Vulkan12 | Self::Vulkan11)
                | (Self::Vulkan12, Self::Vulkan11)
                | (Self::Metal3, Self::Metal2)
                | (Self::OpenGLES30, Self::OpenGLES20)
                | (Self::OpenGL33, Self::OpenGL30)
        )
    }
}

/// Explicit capabilities gathered by a native platform bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapabilities {
    /// Platform for which the candidates are evaluated.
    pub platform: Platform,
    /// APIs that successfully passed the host's capability probe.
    pub backends: Vec<Backend>,
}

impl DeviceCapabilities {
    /// Creates an empty report for `platform`.
    pub const fn for_platform(platform: Platform) -> Self {
        Self {
            platform,
            backends: Vec::new(),
        }
    }

    /// Adds an available backend, ignoring duplicate reports.
    #[must_use]
    pub fn with_backend(mut self, backend: Backend) -> Self {
        if !self.backends.contains(&backend) {
            self.backends.push(backend);
        }
        self
    }

    /// Reports whether an available implementation satisfies `required`.
    pub fn supports(&self, required: Backend) -> bool {
        self.backends
            .iter()
            .copied()
            .any(|available| available.satisfies(required))
    }
}

/// A project's renderer policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererRequest {
    /// Lowest tier the project declares as supported.
    pub minimum_tier: RenderTier,
    /// Tier selected first when several compliant profiles are available.
    pub preferred_tier: RenderTier,
}

impl RendererRequest {
    /// Creates a request that prefers its declared minimum tier.
    pub const fn new(minimum_tier: RenderTier) -> Self {
        Self {
            minimum_tier,
            preferred_tier: minimum_tier,
        }
    }

    /// Changes the target tier while preserving the project minimum.
    ///
    /// Returns [`RequestError::PreferredBelowMinimum`] when the request would be
    /// internally inconsistent.
    pub const fn with_preferred_tier(
        mut self,
        preferred_tier: RenderTier,
    ) -> Result<Self, RequestError> {
        if preferred_tier < self.minimum_tier {
            return Err(RequestError::PreferredBelowMinimum);
        }
        self.preferred_tier = preferred_tier;
        Ok(self)
    }
}

/// Invalid renderer-policy input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    /// A preferred tier was lower than the application's declared minimum.
    PreferredBelowMinimum,
}

/// A planned graphics API selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPlan {
    /// API selected for renderer creation.
    pub backend: Backend,
    /// Policy tier represented by `backend`.
    pub tier: RenderTier,
    /// The selected plan is an emergency fallback rather than a normal profile.
    pub emergency_fallback: bool,
    /// The selected plan does not meet the application's minimum tier.
    pub below_minimum: bool,
}

impl RenderPlan {
    /// Indicates that the host should show a force-run warning before continuing.
    pub const fn requires_force_run_warning(self) -> bool {
        self.emergency_fallback || self.below_minimum
    }
}

/// Why no compliant renderer was selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionError {
    /// The requested project minimum.
    pub minimum_tier: RenderTier,
    /// Every candidate considered in policy order, useful for user-facing logs.
    pub attempted: Vec<Backend>,
}

/// Selects a normal, compliant profile for `device`.
///
/// This never chooses a backend below [`RendererRequest::minimum_tier`] and never
/// selects an emergency profile. Use [`select_best_effort`] only after the host has
/// obtained explicit confirmation to run below requirements.
pub fn select_renderer(
    device: &DeviceCapabilities,
    request: &RendererRequest,
) -> Result<RenderPlan, SelectionError> {
    let candidates = normal_candidates(device.platform, request);
    select_from(device, candidates, request.minimum_tier)
}

/// Selects a strict renderer for a browser host that can expose native APIs.
///
/// Some embedded web runtimes can provide a native device (for example, a Windows
/// host). In that case Miu tries the native host first and only then applies the
/// browser's WebGPU/WebGL policy. Pass `None` for ordinary browsers.
pub fn select_web_renderer(
    native_host: Option<&DeviceCapabilities>,
    web_device: &DeviceCapabilities,
    request: &RendererRequest,
) -> Result<RenderPlan, SelectionError> {
    if let Some(host) = native_host {
        if host.platform != Platform::Web {
            if let Ok(plan) = select_renderer(host, request) {
                return Ok(plan);
            }
        }
    }
    select_renderer(web_device, request)
}

/// Selects a renderer and, if necessary, permits a below-minimum or emergency plan.
///
/// A caller must present a warning whenever
/// [`RenderPlan::requires_force_run_warning`] is true. The function is deliberately
/// separate from [`select_renderer`] so that a force run cannot happen by accident.
pub fn select_best_effort(
    device: &DeviceCapabilities,
    request: &RendererRequest,
) -> Result<RenderPlan, SelectionError> {
    if let Ok(plan) = select_renderer(device, request) {
        return Ok(plan);
    }

    let mut candidates = normal_candidates(device.platform, request);
    // Prefer the closest lower tier when force-running. Profiles are listed low
    // to high, so traverse them in reverse order here.
    for profile in profiles_for(device.platform).iter().rev() {
        if profile.tier < request.minimum_tier
            && !candidates.iter().any(|candidate| candidate.backend == profile.backend)
        {
            candidates.push(*profile);
        }
    }
    for profile in emergency_profiles_for(device.platform) {
        if !candidates.iter().any(|candidate| candidate.backend == profile.backend) {
            candidates.push(*profile);
        }
    }
    select_from(device, candidates, request.minimum_tier)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Profile {
    backend: Backend,
    tier: RenderTier,
    emergency: bool,
}

const WINDOWS: [Profile; 3] = [
    Profile { backend: Backend::Direct2D12, tier: RenderTier::Compatibility, emergency: false },
    Profile { backend: Backend::Direct3D113, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Direct3D12, tier: RenderTier::Premium, emergency: false },
];
const ANDROID: [Profile; 4] = [
    Profile { backend: Backend::OpenGLES30, tier: RenderTier::Compatibility, emergency: false },
    // Android balanced tier prefers 1.3 but explicitly supports 1.2 devices.
    Profile { backend: Backend::Vulkan13, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Vulkan12, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Vulkan14, tier: RenderTier::Premium, emergency: false },
];
const APPLE: [Profile; 3] = [
    Profile { backend: Backend::Metal2, tier: RenderTier::Compatibility, emergency: false },
    Profile { backend: Backend::Metal3, tier: RenderTier::Balanced, emergency: false },
    // Metal 3 is both the balanced and premium API profile; feature policy differs.
    Profile { backend: Backend::Metal3, tier: RenderTier::Premium, emergency: false },
];
const LINUX: [Profile; 3] = [
    Profile { backend: Backend::Vulkan11, tier: RenderTier::Compatibility, emergency: false },
    Profile { backend: Backend::Vulkan12, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Vulkan13, tier: RenderTier::Premium, emergency: false },
];
const WEB: [Profile; 3] = [
    Profile { backend: Backend::WebGL2, tier: RenderTier::Compatibility, emergency: false },
    Profile { backend: Backend::WebGL2, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::WebGPU, tier: RenderTier::Premium, emergency: false },
];
const WINDOWS_EMERGENCY: [Profile; 1] = [Profile { backend: Backend::OpenGL33, tier: RenderTier::Compatibility, emergency: true }];
const ANDROID_EMERGENCY: [Profile; 1] = [Profile { backend: Backend::OpenGLES20, tier: RenderTier::Compatibility, emergency: true }];
const APPLE_EMERGENCY: [Profile; 1] = [Profile { backend: Backend::Metal1, tier: RenderTier::Compatibility, emergency: true }];
const LINUX_EMERGENCY: [Profile; 1] = [Profile { backend: Backend::OpenGL30, tier: RenderTier::Compatibility, emergency: true }];
const WEB_EMERGENCY: [Profile; 1] = [Profile { backend: Backend::WebGL1, tier: RenderTier::Compatibility, emergency: true }];

fn profiles_for(platform: Platform) -> &'static [Profile] {
    match platform {
        Platform::Windows => &WINDOWS,
        Platform::Android => &ANDROID,
        Platform::MacOS | Platform::IOS => &APPLE,
        Platform::Linux => &LINUX,
        Platform::Web => &WEB,
    }
}

fn emergency_profiles_for(platform: Platform) -> &'static [Profile] {
    match platform {
        Platform::Windows => &WINDOWS_EMERGENCY,
        Platform::Android => &ANDROID_EMERGENCY,
        Platform::MacOS | Platform::IOS => &APPLE_EMERGENCY,
        Platform::Linux => &LINUX_EMERGENCY,
        Platform::Web => &WEB_EMERGENCY,
    }
}

fn normal_candidates(platform: Platform, request: &RendererRequest) -> Vec<Profile> {
    let mut candidates = Vec::new();
    let profiles = profiles_for(platform);
    for profile in profiles {
        if profile.tier == request.preferred_tier && profile.tier >= request.minimum_tier {
            candidates.push(*profile);
        }
    }
    for profile in profiles {
        if profile.tier >= request.minimum_tier
            && !candidates.iter().any(|candidate| candidate.backend == profile.backend)
        {
            candidates.push(*profile);
        }
    }
    candidates
}

fn select_from(
    device: &DeviceCapabilities,
    candidates: Vec<Profile>,
    minimum_tier: RenderTier,
) -> Result<RenderPlan, SelectionError> {
    let attempted = candidates.iter().map(|profile| profile.backend).collect();
    for profile in candidates {
        if device.supports(profile.backend) {
            return Ok(RenderPlan {
                backend: profile.backend,
                tier: profile.tier,
                emergency_fallback: profile.emergency,
                below_minimum: profile.tier < minimum_tier,
            });
        }
    }
    Err(SelectionError { minimum_tier, attempted })
}

/// A two-dimensional vector.
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
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a vector.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn rotate(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }
}

impl core::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl core::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl core::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// View transform for an independent 2D camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera2D {
    /// World-space position at the centre of the viewport.
    pub position: Vec2,
    /// Viewport size in logical pixels.
    pub viewport: Vec2,
    /// Uniform zoom; it must be positive and finite.
    pub zoom: f32,
    /// Clockwise screen rotation in radians.
    pub rotation_radians: f32,
}

/// Invalid camera inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraError {
    /// Viewport dimensions must be finite and positive.
    InvalidViewport,
    /// Zoom must be finite and greater than zero.
    InvalidZoom,
    /// Rotation must be finite.
    InvalidRotation,
}

impl Camera2D {
    /// Creates a camera centred on the world origin.
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

    /// Verifies that this camera can safely form transforms.
    pub fn validate(&self) -> Result<(), CameraError> {
        if !self.viewport.x.is_finite()
            || !self.viewport.y.is_finite()
            || self.viewport.x <= 0.0
            || self.viewport.y <= 0.0
        {
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

    /// Maps a world point to logical screen coordinates with a top-left origin.
    pub fn world_to_screen(&self, world: Vec2) -> Result<Vec2, CameraError> {
        self.validate()?;
        let local = (world - self.position).rotate(-self.rotation_radians) * self.zoom;
        Ok(local + self.viewport * 0.5)
    }

    /// Maps a logical screen point with a top-left origin back to world coordinates.
    pub fn screen_to_world(&self, screen: Vec2) -> Result<Vec2, CameraError> {
        self.validate()?;
        let local = (screen - self.viewport * 0.5) * (1.0 / self.zoom);
        Ok(local.rotate(self.rotation_radians) + self.position)
    }
}

/// Position, rotation, and scale for a 2D item.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Transform2D {
    /// World-space translation.
    pub translation: Vec2,
    /// Clockwise rotation in radians.
    pub rotation_radians: f32,
    /// Per-axis scale.
    pub scale: Vec2,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            translation: Vec2::ZERO,
            rotation_radians: 0.0,
            scale: Vec2::new(1.0, 1.0),
        }
    }
}

/// Source rectangle in normalised texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct UvRect {
    /// Left coordinate.
    pub left: f32,
    /// Top coordinate.
    pub top: f32,
    /// Right coordinate.
    pub right: f32,
    /// Bottom coordinate.
    pub bottom: f32,
}

impl UvRect {
    /// The full texture rectangle.
    pub const FULL: Self = Self { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 };
}

/// Premultiplied linear RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Color {
    /// Red component.
    pub r: f32,
    /// Green component.
    pub g: f32,
    /// Blue component.
    pub b: f32,
    /// Alpha component.
    pub a: f32,
}

impl Color {
    /// Opaque white.
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
}

/// A sprite ready to be appended to a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    /// Opaque host-owned texture identifier.
    pub texture: u64,
    /// Transform from sprite-local to world space.
    pub transform: Transform2D,
    /// Normalised texture region.
    pub uv: UvRect,
    /// Premultiplied tint.
    pub color: Color,
    /// Painter's layer; lower layers render first.
    pub layer: i32,
}

/// Draw-sorting policy used by [`BatchBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    /// Preserve submission order within a layer. Safe for transparent sprites.
    Stable,
    /// Reorder texture state within a layer to reduce changes. Only safe for opaque
    /// content or content whose overlap order is intentionally unspecified.
    Opaque,
}

#[derive(Debug, Clone, Copy)]
struct QueuedSprite {
    sprite: Sprite,
    sequence: u64,
}

/// A contiguous texture/layer range in a [`FrameBatches`] command list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawBatch {
    /// Source texture shared by every sprite in this range.
    pub texture: u64,
    /// Painter's layer shared by every sprite in this range.
    pub layer: i32,
    /// Index into [`FrameBatches::sprites`].
    pub first: usize,
    /// Number of sprites to draw.
    pub count: usize,
}

/// Sorted per-frame sprites and their contiguous draw batches.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameBatches {
    /// Sorted sprite instances to encode.
    pub sprites: Vec<Sprite>,
    /// Contiguous state-compatible ranges in `sprites`.
    pub batches: Vec<DrawBatch>,
}

/// CPU-side command collector for one 2D frame.
#[derive(Debug, Default)]
pub struct BatchBuilder {
    mode: BatchMode,
    queued: Vec<QueuedSprite>,
    next_sequence: u64,
}

impl Default for BatchMode {
    fn default() -> Self {
        Self::Stable
    }
}

impl BatchBuilder {
    /// Starts an empty frame command collector.
    pub fn new(mode: BatchMode) -> Self {
        Self { mode, queued: Vec::new(), next_sequence: 0 }
    }

    /// Reserves command capacity without submitting a draw.
    pub fn reserve(&mut self, additional: usize) {
        self.queued.reserve(additional);
    }

    /// Appends a sprite. Its relative order is always retained in `Stable` mode.
    pub fn push(&mut self, sprite: Sprite) {
        self.queued.push(QueuedSprite { sprite, sequence: self.next_sequence });
        self.next_sequence = self.next_sequence.wrapping_add(1);
    }

    /// Sorts commands and groups adjacent matching texture/layer pairs.
    pub fn finish(mut self) -> FrameBatches {
        self.queued.sort_unstable_by(|left, right| {
            let layer = left.sprite.layer.cmp(&right.sprite.layer);
            if layer != Ordering::Equal {
                return layer;
            }
            match self.mode {
                BatchMode::Stable => left.sequence.cmp(&right.sequence),
                BatchMode::Opaque => left
                    .sprite
                    .texture
                    .cmp(&right.sprite.texture)
                    .then_with(|| left.sequence.cmp(&right.sequence)),
            }
        });

        let sprites: Vec<Sprite> = self.queued.iter().map(|item| item.sprite).collect();
        let mut batches = Vec::new();
        for (index, sprite) in sprites.iter().enumerate() {
            match batches.last_mut() {
                Some(batch) if batch.texture == sprite.texture && batch.layer == sprite.layer => {
                    batch.count += 1;
                }
                _ => batches.push(DrawBatch {
                    texture: sprite.texture,
                    layer: sprite.layer,
                    first: index,
                    count: 1,
                }),
            }
        }
        FrameBatches { sprites, batches }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_tier_is_preferred_but_a_higher_supported_tier_is_accepted() {
        let device = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct3D12);
        let request = RendererRequest::new(RenderTier::Balanced);
        let plan = select_renderer(&device, &request).expect("D3D12 is a compliant option");
        assert_eq!(plan.backend, Backend::Direct3D12);
        assert_eq!(plan.tier, RenderTier::Premium);
        assert!(!plan.requires_force_run_warning());
    }

    #[test]
    fn strict_selection_does_not_cross_the_minimum() {
        let device = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct2D12);
        let request = RendererRequest::new(RenderTier::Balanced);
        let error = select_renderer(&device, &request).expect_err("D2D is below balanced");
        assert_eq!(error.attempted, vec![Backend::Direct3D113, Backend::Direct3D12]);

        let plan = select_best_effort(&device, &request).expect("explicit force run allows D2D");
        assert_eq!(plan.backend, Backend::Direct2D12);
        assert!(plan.below_minimum);
    }

    #[test]
    fn emergency_fallback_is_marked() {
        let device = DeviceCapabilities::for_platform(Platform::Linux)
            .with_backend(Backend::OpenGL30);
        let request = RendererRequest::new(RenderTier::Compatibility);
        let plan = select_best_effort(&device, &request).expect("emergency OpenGL works");
        assert!(plan.emergency_fallback);
        assert!(plan.requires_force_run_warning());
    }

    #[test]
    fn best_effort_uses_the_closest_lower_web_tier() {
        let device = DeviceCapabilities::for_platform(Platform::Web)
            .with_backend(Backend::WebGL2);
        let request = RendererRequest::new(RenderTier::Premium);
        let plan = select_best_effort(&device, &request).expect("WebGL 2 can force-run");
        assert_eq!(plan.backend, Backend::WebGL2);
        assert_eq!(plan.tier, RenderTier::Balanced);
        assert!(plan.below_minimum);
    }

    #[test]
    fn newer_vulkan_satisfies_lower_profile() {
        let device = DeviceCapabilities::for_platform(Platform::Linux)
            .with_backend(Backend::Vulkan14);
        let request = RendererRequest::new(RenderTier::Balanced);
        let plan = select_renderer(&device, &request).expect("Vulkan 1.4 supports 1.3");
        assert_eq!(plan.backend, Backend::Vulkan13);
    }

    #[test]
    fn embedded_web_prefers_a_usable_native_host() {
        let host = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct3D113);
        let web = DeviceCapabilities::for_platform(Platform::Web)
            .with_backend(Backend::WebGPU);
        let plan = select_web_renderer(
            Some(&host),
            &web,
            &RendererRequest::new(RenderTier::Balanced),
        )
        .expect("host renderer is selected before browser fallback");
        assert_eq!(plan.backend, Backend::Direct3D113);
    }

    #[test]
    fn camera_transform_round_trips() {
        let mut camera = Camera2D::new(Vec2::new(1920.0, 1080.0)).unwrap();
        camera.position = Vec2::new(100.0, -20.0);
        camera.zoom = 1.5;
        camera.rotation_radians = 0.37;
        let point = Vec2::new(218.0, 51.5);
        let returned = camera.screen_to_world(camera.world_to_screen(point).unwrap()).unwrap();
        assert!((returned.x - point.x).abs() < 0.001);
        assert!((returned.y - point.y).abs() < 0.001);
    }

    fn sprite(texture: u64, layer: i32) -> Sprite {
        Sprite {
            texture,
            transform: Transform2D::default(),
            uv: UvRect::FULL,
            color: Color::WHITE,
            layer,
        }
    }

    #[test]
    fn stable_batches_never_reorder_transparent_sprites() {
        let mut builder = BatchBuilder::new(BatchMode::Stable);
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 0));
        builder.push(sprite(2, 0));
        let frame = builder.finish();
        assert_eq!(frame.sprites.iter().map(|item| item.texture).collect::<Vec<_>>(), vec![2, 1, 2]);
        assert_eq!(frame.batches.len(), 3);
    }

    #[test]
    fn opaque_batches_group_textures_per_layer() {
        let mut builder = BatchBuilder::new(BatchMode::Opaque);
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 0));
        builder.push(sprite(2, 0));
        builder.push(sprite(1, 1));
        let frame = builder.finish();
        assert_eq!(frame.sprites.iter().map(|item| item.texture).collect::<Vec<_>>(), vec![1, 2, 2, 1]);
        assert_eq!(frame.batches.len(), 3);
        assert_eq!(frame.batches[1].count, 2);
    }
}

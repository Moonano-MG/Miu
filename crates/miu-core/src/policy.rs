//! Backend catalogues and render-plan selection.
//!
//! Miu separates *policy* from *device creation*:
//!
//! 1. A platform bridge probes which graphics APIs actually work and reports them
//!    in [`DeviceCapabilities`].
//! 2. The application declares its minimum tier, preferred tier, and quality
//!    preset in a [`RendererRequest`].
//! 3. [`select_renderer`] returns a [`RenderPlan`] naming the API the bridge must
//!    create, or a [`SelectionError`] listing every candidate it tried.
//!
//! Emergency APIs (OpenGL 3.3, OpenGL ES 2, Metal 1, OpenGL 3.0, WebGL 1) are only
//! reachable through [`select_best_effort`] and are always flagged on the plan so a
//! host can warn the user before continuing.

use crate::quality::QualityPreset;

/// Operating-system or browser family used to choose a backend catalogue.
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
    /// Linux and other X11/Wayland desktops.
    Linux = 5,
    /// A browser or WebAssembly host.
    Web = 6,
}

impl Platform {
    /// Converts a C ABI discriminant into a platform.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Windows),
            2 => Some(Self::Android),
            3 => Some(Self::MacOS),
            4 => Some(Self::IOS),
            5 => Some(Self::Linux),
            6 => Some(Self::Web),
            _ => None,
        }
    }

    /// Stable identifier for configuration files and telemetry.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Android => "android",
            Self::MacOS => "macos",
            Self::IOS => "ios",
            Self::Linux => "linux",
            Self::Web => "web",
        }
    }
}

/// Quality/performance policy declared by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RenderTier {
    /// Prioritises reach and minimal overhead on older hardware.
    Compatibility = 1,
    /// Balances image quality against current mainstream hardware performance.
    Balanced = 2,
    /// Enables the strongest backend profile the platform offers.
    Premium = 3,
}

impl RenderTier {
    /// Converts a C ABI discriminant into a tier.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Compatibility),
            2 => Some(Self::Balanced),
            3 => Some(Self::Premium),
            _ => None,
        }
    }

    /// Stable identifier used by configuration and logs.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Compatibility => "compatibility",
            Self::Balanced => "balanced",
            Self::Premium => "premium",
        }
    }

    /// Highest quality preset this tier can honour.
    pub const fn max_preset(self) -> QualityPreset {
        match self {
            Self::Compatibility => QualityPreset::Medium,
            Self::Balanced => QualityPreset::High,
            Self::Premium => QualityPreset::Unlimited,
        }
    }
}

/// A concrete graphics API profile that can execute Miu command streams.
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
    /// Converts a C ABI discriminant into a backend.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Direct2D12),
            2 => Some(Self::Direct3D113),
            3 => Some(Self::Direct3D12),
            4 => Some(Self::OpenGLES30),
            5 => Some(Self::OpenGLES20),
            6 => Some(Self::Metal2),
            7 => Some(Self::Metal3),
            8 => Some(Self::Metal1),
            9 => Some(Self::Vulkan11),
            10 => Some(Self::Vulkan12),
            11 => Some(Self::Vulkan13),
            12 => Some(Self::Vulkan14),
            13 => Some(Self::OpenGL33),
            14 => Some(Self::OpenGL30),
            15 => Some(Self::WebGL2),
            16 => Some(Self::WebGL1),
            17 => Some(Self::WebGPU),
            _ => None,
        }
    }

    /// Every backend in catalogue order, emergency APIs last.
    pub const ALL: [Self; 17] = [
        Self::Direct2D12,
        Self::Direct3D113,
        Self::Direct3D12,
        Self::OpenGLES30,
        Self::OpenGLES20,
        Self::Metal2,
        Self::Metal3,
        Self::Metal1,
        Self::Vulkan11,
        Self::Vulkan12,
        Self::Vulkan13,
        Self::Vulkan14,
        Self::OpenGL33,
        Self::OpenGL30,
        Self::WebGL2,
        Self::WebGL1,
        Self::WebGPU,
    ];

    /// Converts a stable identifier produced by [`Backend::id`] into a backend.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|backend| backend.id() == id)
    }

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

    /// Human-readable API name for diagnostics and settings screens.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Direct2D12 => "Direct2D 1.2",
            Self::Direct3D113 => "Direct3D 11.3",
            Self::Direct3D12 => "Direct3D 12",
            Self::OpenGLES30 => "OpenGL ES 3.0",
            Self::OpenGLES20 => "OpenGL ES 2.0",
            Self::Metal2 => "Metal 2",
            Self::Metal3 => "Metal 3",
            Self::Metal1 => "Metal 1",
            Self::Vulkan11 => "Vulkan 1.1",
            Self::Vulkan12 => "Vulkan 1.2",
            Self::Vulkan13 => "Vulkan 1.3",
            Self::Vulkan14 => "Vulkan 1.4",
            Self::OpenGL33 => "OpenGL 3.3",
            Self::OpenGL30 => "OpenGL 3.0",
            Self::WebGL2 => "WebGL 2",
            Self::WebGL1 => "WebGL 1",
            Self::WebGPU => "WebGPU",
        }
    }

    /// Whether this implementation can satisfy a request for `required`.
    ///
    /// Only same-family API guarantees are modelled: newer Vulkan/Metal/OpenGL
    /// versions satisfy older requests, while one family never substitutes for
    /// another. Cross-family substitution is the selector's job, not this method's.
    pub const fn satisfies(self, required: Self) -> bool {
        if self as u8 == required as u8 {
            return true;
        }
        matches!(
            (self, required),
            (Self::Vulkan14, Self::Vulkan13 | Self::Vulkan12 | Self::Vulkan11)
                | (Self::Vulkan13, Self::Vulkan12 | Self::Vulkan11)
                | (Self::Vulkan12, Self::Vulkan11)
                | (Self::Metal3, Self::Metal2 | Self::Metal1)
                | (Self::Metal2, Self::Metal1)
                | (Self::OpenGLES30, Self::OpenGLES20)
                | (Self::OpenGL33, Self::OpenGL30)
                | (Self::WebGPU, Self::WebGL2 | Self::WebGL1)
                | (Self::WebGL2, Self::WebGL1)
        )
    }

    /// Version ordering inside one API family, used to pick the best available API.
    pub const fn version_rank(self) -> u8 {
        match self {
            Self::Vulkan11 => 11,
            Self::Vulkan12 => 12,
            Self::Vulkan13 => 13,
            Self::Vulkan14 => 14,
            Self::Metal1 => 1,
            Self::Metal2 => 2,
            Self::Metal3 => 3,
            Self::OpenGLES20 => 20,
            Self::OpenGLES30 => 30,
            Self::OpenGL30 => 30,
            Self::OpenGL33 => 33,
            Self::WebGL1 => 1,
            Self::WebGL2 => 2,
            Self::Direct2D12 => 12,
            Self::Direct3D113 => 113,
            Self::Direct3D12 => 120,
            Self::WebGPU => 10,
        }
    }
}

/// Explicit capabilities gathered by a native platform bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapabilities {
    /// Platform whose catalogue was probed.
    pub platform: Platform,
    /// APIs that passed the host's probe, in the order they were reported.
    pub backends: Vec<Backend>,
}

/// An empty report defaults to the host platform family detected at compile time.
impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self::for_platform(crate::policy::host_platform())
    }
}

/// Platform family of the current compilation target.
///
/// A browser/WebAssembly build reports [`Platform::Web`], which keeps the browser
/// catalogue in charge even when the runtime is hosted by a desktop OS.
pub const fn host_platform() -> Platform {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        Platform::Web
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(target_os = "android")]
        {
            Platform::Android
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(target_os = "ios")]
        {
            Platform::IOS
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        )))]
        {
            Platform::Linux
        }
    }
}

impl DeviceCapabilities {
    /// Creates an empty report for `platform`.
    pub const fn for_platform(platform: Platform) -> Self {
        Self {
            platform,
            backends: Vec::new(),
        }
    }

    /// Adds an available backend, ignoring duplicates.
    #[must_use]
    pub fn with_backend(mut self, backend: Backend) -> Self {
        self.add_backend(backend);
        self
    }

    /// Adds several available backends, ignoring duplicates.
    #[must_use]
    pub fn with_backends(mut self, backends: &[Backend]) -> Self {
        for backend in backends {
            self.add_backend(*backend);
        }
        self
    }

    /// Adds a backend in place, ignoring duplicates.
    pub fn add_backend(&mut self, backend: Backend) -> bool {
        if self.backends.contains(&backend) {
            return false;
        }
        self.backends.push(backend);
        true
    }

    /// Whether the report names `backend`.
    pub fn contains(&self, backend: Backend) -> bool {
        self.backends.contains(&backend)
    }

    /// Returns the highest-version available implementation satisfying `profile`.
    pub fn best_backend_for(&self, profile: Backend) -> Option<Backend> {
        self.backends
            .iter()
            .copied()
            .filter(|available| available.satisfies(profile))
            .max_by_key(|available| available.version_rank())
    }

    /// Reports whether any available implementation satisfies `required`.
    pub fn supports(&self, required: Backend) -> bool {
        self.best_backend_for(required).is_some()
    }

    /// Comma-separated backend identifiers, convenient for one-line logs.
    pub fn summary(&self) -> String {
        if self.backends.is_empty() {
            return "none".to_string();
        }
        let mut text = String::new();
        for (index, backend) in self.backends.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(backend.id());
        }
        text
    }
}

/// An application's renderer policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererRequest {
    /// Lowest tier the application declares as supported.
    pub minimum_tier: RenderTier,
    /// Compliant tier tried first; stronger tiers are considered only if it fails.
    pub preferred_tier: RenderTier,
    /// Requested quality preset. Plans clamp it to the selected tier's maximum.
    pub preset: QualityPreset,
}

impl RendererRequest {
    /// Creates a request preferring its declared minimum tier at the Medium preset.
    pub const fn new(minimum_tier: RenderTier) -> Self {
        Self {
            minimum_tier,
            preferred_tier: minimum_tier,
            preset: QualityPreset::Medium,
        }
    }

    /// Changes the target tier while preserving the project minimum.
    pub fn with_preferred_tier(mut self, preferred_tier: RenderTier) -> Result<Self, RequestError> {
        self.preferred_tier = preferred_tier;
        self.validate()?;
        Ok(self)
    }

    /// Changes the requested quality preset.
    #[must_use]
    pub const fn with_preset(mut self, preset: QualityPreset) -> Self {
        self.preset = preset;
        self
    }

    /// Validates a request built from public fields.
    pub fn validate(&self) -> Result<(), RequestError> {
        if self.preferred_tier < self.minimum_tier {
            return Err(RequestError::PreferredBelowMinimum);
        }
        Ok(())
    }
}

/// Invalid renderer-policy input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    /// The preferred tier was lower than the application's declared minimum.
    PreferredBelowMinimum,
}

impl RequestError {
    /// Stable identifier for logs and error strings.
    pub const fn id(self) -> &'static str {
        match self {
            Self::PreferredBelowMinimum => "request.preferred-below-minimum",
        }
    }

    /// Human-readable explanation.
    pub const fn message(self) -> &'static str {
        match self {
            Self::PreferredBelowMinimum => "preferred tier is below the declared minimum tier",
        }
    }
}

/// A planned graphics API selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPlan {
    /// Actual API the bridge must create.
    pub backend: Backend,
    /// Minimum API profile used to choose this plan.
    pub profile_backend: Backend,
    /// Policy tier represented by `profile_backend`.
    pub tier: RenderTier,
    /// Quality preset after clamping to the selected tier.
    pub preset: QualityPreset,
    /// The plan uses an emergency fallback rather than a normal profile.
    pub emergency_fallback: bool,
    /// The plan does not meet the application's declared minimum tier.
    pub below_minimum: bool,
}

impl RenderPlan {
    /// Whether the host must show a force-run warning before continuing.
    pub const fn requires_force_run_warning(self) -> bool {
        self.emergency_fallback || self.below_minimum
    }

    /// Settings implied by the plan's preset.
    pub fn quality(self) -> crate::quality::QualitySettings {
        crate::quality::QualitySettings::for_preset(self.preset)
    }
}

/// Returns a user-facing warning for a plan that needs explicit confirmation.
pub fn plan_warning(plan: &RenderPlan) -> Option<String> {
    if plan.emergency_fallback {
        Some(format!(
            "Miu is using the emergency {} fallback. Rendering may be unstable on this device; save your work before continuing.",
            plan.backend.display_name()
        ))
    } else if plan.below_minimum {
        Some(format!(
            "This device only offers {}, below the project's declared minimum ({}). Forcing the renderer may crash or render incorrectly.",
            plan.backend.display_name(),
            plan.tier.id()
        ))
    } else {
        None
    }
}

/// Why no compliant renderer was selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionError {
    /// The requested project minimum.
    pub minimum_tier: RenderTier,
    /// Every candidate considered, in policy order.
    pub attempted: Vec<Backend>,
    /// Set when selection failed because the request fields were inconsistent.
    pub invalid_request: Option<RequestError>,
}

impl SelectionError {
    /// One-line explanation suitable for logs or a settings dialog.
    pub fn describe(&self) -> String {
        if let Some(error) = self.invalid_request {
            return error.message().to_string();
        }
        let mut candidates = String::new();
        for (index, backend) in self.attempted.iter().enumerate() {
            if index > 0 {
                candidates.push_str(", ");
            }
            candidates.push_str(backend.display_name());
        }
        if candidates.is_empty() {
            format!(
                "no graphics API satisfies the {} tier",
                self.minimum_tier.id()
            )
        } else {
            format!(
                "no graphics API satisfies the {} tier (tried: {})",
                self.minimum_tier.id(),
                candidates
            )
        }
    }
}

/// Selects a normal, compliant profile for `device`.
///
/// This never chooses a backend below [`RendererRequest::minimum_tier`] and never
/// selects an emergency API. Use [`select_best_effort`] only after the host has
/// obtained explicit confirmation to run below requirements.
pub fn select_renderer(
    device: &DeviceCapabilities,
    request: &RendererRequest,
) -> Result<RenderPlan, SelectionError> {
    if let Err(error) = request.validate() {
        return Err(SelectionError {
            minimum_tier: request.minimum_tier,
            attempted: Vec::new(),
            invalid_request: Some(error),
        });
    }
    let candidates = normal_candidates(device.platform, request);
    select_from(device, candidates, request.minimum_tier, request.preset)
}

/// Selects a strict renderer for a browser host that can expose native APIs.
///
/// Some embedded web runtimes provide a native device (for example a Windows host).
/// Miu then tries that device first and only falls back to the browser's
/// WebGPU/WebGL policy. Pass `None` for ordinary browsers.
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

/// Selects a renderer, permitting below-minimum and emergency plans.
///
/// A caller must present [`plan_warning`] to the user whenever
/// [`RenderPlan::requires_force_run_warning`] is true. The function is deliberately
/// separate from [`select_renderer`] so a force run cannot happen by accident.
pub fn select_best_effort(
    device: &DeviceCapabilities,
    request: &RendererRequest,
) -> Result<RenderPlan, SelectionError> {
    if let Err(error) = request.validate() {
        return Err(SelectionError {
            minimum_tier: request.minimum_tier,
            attempted: Vec::new(),
            invalid_request: Some(error),
        });
    }
    if let Ok(plan) = select_renderer(device, request) {
        return Ok(plan);
    }

    let mut candidates = normal_candidates(device.platform, request);
    // Prefer the closest lower tier when force-running: profiles are listed from
    // weakest to strongest, so they are traversed in reverse here.
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
    select_from(device, candidates, request.minimum_tier, request.preset)
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
    // The balanced tier accepts Vulkan 1.2 and prefers 1.3 when the device has it.
    Profile { backend: Backend::Vulkan12, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Vulkan13, tier: RenderTier::Balanced, emergency: false },
    Profile { backend: Backend::Vulkan14, tier: RenderTier::Premium, emergency: false },
];
const APPLE: [Profile; 3] = [
    Profile { backend: Backend::Metal2, tier: RenderTier::Compatibility, emergency: false },
    Profile { backend: Backend::Metal3, tier: RenderTier::Balanced, emergency: false },
    // Metal 3 is both the balanced and premium API; the tiers differ by features.
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
const WINDOWS_EMERGENCY: [Profile; 1] = [Profile {
    backend: Backend::OpenGL33,
    tier: RenderTier::Compatibility,
    emergency: true,
}];
const ANDROID_EMERGENCY: [Profile; 1] = [Profile {
    backend: Backend::OpenGLES20,
    tier: RenderTier::Compatibility,
    emergency: true,
}];
const APPLE_EMERGENCY: [Profile; 1] = [Profile {
    backend: Backend::Metal1,
    tier: RenderTier::Compatibility,
    emergency: true,
}];
const LINUX_EMERGENCY: [Profile; 1] = [Profile {
    backend: Backend::OpenGL30,
    tier: RenderTier::Compatibility,
    emergency: true,
}];
const WEB_EMERGENCY: [Profile; 1] = [Profile {
    backend: Backend::WebGL1,
    tier: RenderTier::Compatibility,
    emergency: true,
}];

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
    // Honour the application's selected tier first. Within a tier the strongest
    // API profile wins; only when it cannot run do we escalate to a stronger tier.
    for profile in profiles.iter().rev() {
        if profile.tier == request.preferred_tier && profile.tier >= request.minimum_tier {
            candidates.push(*profile);
        }
    }
    for profile in profiles.iter().rev() {
        if profile.tier > request.preferred_tier
            && profile.tier >= request.minimum_tier
            && !candidates.iter().any(|candidate: &Profile| candidate.backend == profile.backend)
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
    preset: QualityPreset,
) -> Result<RenderPlan, SelectionError> {
    let attempted = candidates.iter().map(|profile| profile.backend).collect();
    for profile in candidates {
        if let Some(actual_backend) = device.best_backend_for(profile.backend) {
            return Ok(RenderPlan {
                backend: actual_backend,
                profile_backend: profile.backend,
                tier: profile.tier,
                preset: preset.clamp_to_tier(profile.tier),
                emergency_fallback: profile.emergency,
                below_minimum: profile.tier < minimum_tier,
            });
        }
    }
    Err(SelectionError {
        minimum_tier,
        attempted,
        invalid_request: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_tier_is_selected_before_a_higher_tier() {
        let device = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct3D113)
            .with_backend(Backend::Direct3D12);
        let request = RendererRequest::new(RenderTier::Balanced);
        let plan = select_renderer(&device, &request).expect("D3D 11.3 is the preferred target");
        assert_eq!(plan.backend, Backend::Direct3D113);
        assert_eq!(plan.profile_backend, Backend::Direct3D113);
        assert_eq!(plan.tier, RenderTier::Balanced);
        assert!(!plan.requires_force_run_warning());
    }

    #[test]
    fn higher_tier_is_used_when_preferred_tier_is_unavailable() {
        let device = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct3D12);
        let plan = select_renderer(&device, &RendererRequest::new(RenderTier::Balanced))
            .expect("premium backend is a valid upward fallback");
        assert_eq!(plan.backend, Backend::Direct3D12);
        assert_eq!(plan.tier, RenderTier::Premium);
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
        assert!(plan_warning(&plan).is_some());
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
    fn actual_backend_preserves_newer_vulkan_version() {
        let device = DeviceCapabilities::for_platform(Platform::Linux)
            .with_backend(Backend::Vulkan14);
        let request = RendererRequest::new(RenderTier::Balanced);
        let plan = select_renderer(&device, &request).expect("Vulkan 1.4 satisfies balanced");
        assert_eq!(plan.profile_backend, Backend::Vulkan13);
        assert_eq!(plan.backend, Backend::Vulkan14);
    }

    #[test]
    fn plan_presets_are_clamped_to_the_selected_tier() {
        let device = DeviceCapabilities::for_platform(Platform::Linux)
            .with_backend(Backend::Vulkan11);
        let request = RendererRequest::new(RenderTier::Compatibility)
            .with_preset(QualityPreset::Unlimited);
        let plan = select_renderer(&device, &request).expect("Vulkan 1.1 is compatible");
        assert_eq!(plan.preset, QualityPreset::Medium);
        assert_eq!(plan.quality().preset, QualityPreset::Medium);
    }

    #[test]
    fn public_request_fields_are_validated() {
        let request = RendererRequest {
            minimum_tier: RenderTier::Premium,
            preferred_tier: RenderTier::Compatibility,
            preset: QualityPreset::Medium,
        };
        let error = select_renderer(&DeviceCapabilities::for_platform(Platform::Linux), &request)
            .expect_err("inconsistent public fields must not select");
        assert_eq!(error.invalid_request, Some(RequestError::PreferredBelowMinimum));
        assert!(error.describe().contains("preferred tier"));

        let error = select_best_effort(&DeviceCapabilities::for_platform(Platform::Linux), &request)
            .expect_err("best effort must never bypass request validation");
        assert_eq!(error.invalid_request, Some(RequestError::PreferredBelowMinimum));
    }

    #[test]
    fn embedded_web_prefers_a_usable_native_host() {
        let host = DeviceCapabilities::for_platform(Platform::Windows)
            .with_backend(Backend::Direct3D113);
        let web = DeviceCapabilities::for_platform(Platform::Web)
            .with_backend(Backend::WebGPU);
        let plan = select_web_renderer(Some(&host), &web, &RendererRequest::new(RenderTier::Balanced))
            .expect("host renderer is selected before the browser fallback");
        assert_eq!(plan.backend, Backend::Direct3D113);
    }

    #[test]
    fn capability_summary_is_stable() {
        let device = DeviceCapabilities::for_platform(Platform::Android)
            .with_backend(Backend::OpenGLES30)
            .with_backends(&[Backend::Vulkan13, Backend::Vulkan12, Backend::Vulkan13]);
        assert_eq!(device.summary(), "opengles-3.0,vulkan-1.3,vulkan-1.2");
        assert_eq!(device.best_backend_for(Backend::Vulkan12), Some(Backend::Vulkan13));
    }
}

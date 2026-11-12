//! Foundation types shared by every Miu subsystem.
//!
//! This crate contains no windowing, GPU, or file-system dependency. It defines the
//! vocabulary the rest of the engine speaks:
//!
//! * [`math`] — vectors, rectangles, and affine transforms.
//! * [`color`] — premultiplied colour values, 8-bit texels, and blend functions.
//! * [`camera`] — an independent [`Camera2D`] plus pan/zoom interaction helpers.
//! * [`policy`] — backend catalogues, capability reports, and render-plan selection.
//! * [`quality`] — the Low/Medium/High/Unlimited presets and their runtime governor.
//!
//! A host integrates in three steps: probe the device into [`DeviceCapabilities`],
//! ask [`select_renderer`] for a [`RenderPlan`], then hand that plan to a platform
//! bridge which creates the matching graphics API objects.

#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod camera;
pub mod color;
pub mod error;
pub mod math;
pub mod policy;
pub mod quality;

pub use camera::{Camera2D, CameraController, CameraError};
pub use color::{BlendMode, Color, Rgba8};
pub use error::{ErrorCode, error_message};
pub use math::{Affine2, Rect, Transform2D, Vec2};
pub use policy::{
    Backend, DeviceCapabilities, Platform, RenderPlan, RenderTier, RendererRequest, RequestError,
    SelectionError, host_platform, plan_warning, select_best_effort, select_renderer,
    select_web_renderer,
};
pub use quality::{PresetBudget, QualityGovernor, QualityPreset, QualitySettings};

/// Human-readable engine name used in diagnostics and window titles.
pub const ENGINE_NAME: &str = "Miu";

/// Semantic version of the Rust crates in this workspace.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version of the C ABI exported by `miu-ffi`.
///
/// Increment this whenever a `#[repr(C)]` structure changes layout or an exported
/// function changes meaning. Bindings verify the version at load time.
pub const ABI_VERSION: u32 = 3;

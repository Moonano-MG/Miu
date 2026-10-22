//! C ABI entry points for Miu.
//!
//! The ABI is defined in `include/miu/miu.h`. This crate owns no graphics device;
//! it validates a capability report and keeps the immutable selected render plan.

#![deny(unsafe_op_in_unsafe_fn)]

use core::ptr;
use miu_core::{
    select_best_effort, select_renderer, Backend, DeviceCapabilities, Platform, RenderPlan,
    RendererRequest, RenderTier,
};

const ABI_VERSION: u32 = 1;
const STATUS_OK: u32 = 0;
const STATUS_INVALID_ARGUMENT: u32 = 1;
const STATUS_UNSUPPORTED: u32 = 2;

#[repr(C)]
struct MiuDeviceCapabilities {
    abi_version: u32,
    platform: u8,
    reserved: [u8; 3],
    backends: *const u8,
    backend_count: usize,
}

#[repr(C)]
struct MiuRendererRequest {
    abi_version: u32,
    minimum_tier: u8,
    preferred_tier: u8,
    allow_best_effort: u8,
    reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuRenderPlan {
    backend: u8,
    tier: u8,
    emergency_fallback: u8,
    below_minimum: u8,
}

#[repr(C)]
struct MiuRenderer {
    plan: MiuRenderPlan,
}

const fn platform_from_ffi(value: u8) -> Option<Platform> {
    match value {
        1 => Some(Platform::Windows),
        2 => Some(Platform::Android),
        3 => Some(Platform::MacOS),
        4 => Some(Platform::IOS),
        5 => Some(Platform::Linux),
        6 => Some(Platform::Web),
        _ => None,
    }
}

const fn backend_from_ffi(value: u8) -> Option<Backend> {
    match value {
        1 => Some(Backend::Direct2D12),
        2 => Some(Backend::Direct3D113),
        3 => Some(Backend::Direct3D12),
        4 => Some(Backend::OpenGLES30),
        5 => Some(Backend::OpenGLES20),
        6 => Some(Backend::Metal2),
        7 => Some(Backend::Metal3),
        8 => Some(Backend::Metal1),
        9 => Some(Backend::Vulkan11),
        10 => Some(Backend::Vulkan12),
        11 => Some(Backend::Vulkan13),
        12 => Some(Backend::Vulkan14),
        13 => Some(Backend::OpenGL33),
        14 => Some(Backend::OpenGL30),
        15 => Some(Backend::WebGL2),
        16 => Some(Backend::WebGL1),
        17 => Some(Backend::WebGPU),
        _ => None,
    }
}

const fn tier_from_ffi(value: u8) -> Option<RenderTier> {
    match value {
        1 => Some(RenderTier::Compatibility),
        2 => Some(RenderTier::Balanced),
        3 => Some(RenderTier::Premium),
        _ => None,
    }
}

const fn plan_to_ffi(plan: RenderPlan) -> MiuRenderPlan {
    MiuRenderPlan {
        backend: plan.backend as u8,
        tier: plan.tier as u8,
        emergency_fallback: plan.emergency_fallback as u8,
        below_minimum: plan.below_minimum as u8,
    }
}

/// Returns the ABI version implemented by this library.
#[no_mangle]
pub extern "C" fn miu_abi_version() -> u32 {
    ABI_VERSION
}

/// Creates a policy-renderer handle from a platform capability report.
///
/// # Safety
///
/// `capabilities`, `request`, and `out_renderer` must point to valid readable or
/// writable objects for this call. If `backend_count` is nonzero, `backends` must
/// point to that many readable bytes. On success the caller owns `*out_renderer`.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_create(
    capabilities: *const MiuDeviceCapabilities,
    request: *const MiuRendererRequest,
    out_renderer: *mut *mut MiuRenderer,
) -> u32 {
    if capabilities.is_null() || request.is_null() || out_renderer.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }

    // SAFETY: null was checked above; the caller contract requires valid objects.
    let capabilities = unsafe { &*capabilities };
    // SAFETY: null was checked above; the caller contract requires a valid object.
    let request = unsafe { &*request };
    if capabilities.abi_version != ABI_VERSION || request.abi_version != ABI_VERSION {
        return STATUS_INVALID_ARGUMENT;
    }

    let Some(platform) = platform_from_ffi(capabilities.platform) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(minimum_tier) = tier_from_ffi(request.minimum_tier) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(preferred_tier) = tier_from_ffi(request.preferred_tier) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Ok(renderer_request) = RendererRequest::new(minimum_tier).with_preferred_tier(preferred_tier) else {
        return STATUS_INVALID_ARGUMENT;
    };

    if capabilities.backend_count != 0 && capabilities.backends.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    let raw_backends = if capabilities.backend_count == 0 {
        &[]
    } else {
        // SAFETY: the caller contract guarantees this pointer/count range is valid.
        unsafe { core::slice::from_raw_parts(capabilities.backends, capabilities.backend_count) }
    };

    let mut device = DeviceCapabilities::for_platform(platform);
    for raw_backend in raw_backends {
        let Some(backend) = backend_from_ffi(*raw_backend) else {
            return STATUS_INVALID_ARGUMENT;
        };
        device = device.with_backend(backend);
    }

    let selection = if request.allow_best_effort == 0 {
        select_renderer(&device, &renderer_request)
    } else {
        select_best_effort(&device, &renderer_request)
    };
    let Ok(plan) = selection else {
        return STATUS_UNSUPPORTED;
    };

    let renderer = Box::new(MiuRenderer { plan: plan_to_ffi(plan) });
    // SAFETY: out_renderer was null-checked and the caller provides writable storage.
    unsafe { ptr::write(out_renderer, Box::into_raw(renderer)) };
    STATUS_OK
}

/// Copies a policy-renderer handle's immutable selected plan.
///
/// # Safety
///
/// `renderer` must be a non-null handle from [`miu_renderer_create`] that has not
/// been released. `out_plan` must point to writable storage for one plan.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_get_plan(
    renderer: *const MiuRenderer,
    out_plan: *mut MiuRenderPlan,
) -> u32 {
    if renderer.is_null() || out_plan.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: required by this function's caller contract after null validation.
    let plan = unsafe { (*renderer).plan };
    // SAFETY: required by this function's caller contract after null validation.
    unsafe { ptr::write(out_plan, plan) };
    STATUS_OK
}

/// Releases a policy-renderer handle created by [`miu_renderer_create`].
///
/// # Safety
///
/// If non-null, `renderer` must be an unreleased handle returned by this library.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_release(renderer: *mut MiuRenderer) {
    if !renderer.is_null() {
        // SAFETY: ownership transfers back exactly once according to the C ABI.
        unsafe { drop(Box::from_raw(renderer)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(backends: &[u8]) -> MiuDeviceCapabilities {
        MiuDeviceCapabilities {
            abi_version: ABI_VERSION,
            platform: 5,
            reserved: [0; 3],
            backends: backends.as_ptr(),
            backend_count: backends.len(),
        }
    }

    fn request(tier: u8, best_effort: u8) -> MiuRendererRequest {
        MiuRendererRequest {
            abi_version: ABI_VERSION,
            minimum_tier: tier,
            preferred_tier: tier,
            allow_best_effort: best_effort,
            reserved: 0,
        }
    }

    #[test]
    fn ffi_reports_strict_policy_failure() {
        let capabilities = caps(&[14]);
        let request = request(1, 0);
        let mut renderer = ptr::null_mut();
        // SAFETY: local structures satisfy the FFI contract.
        let status = unsafe { miu_renderer_create(&capabilities, &request, &mut renderer) };
        assert_eq!(status, STATUS_UNSUPPORTED);
        assert!(renderer.is_null());
    }

    #[test]
    fn ffi_returns_an_emergency_plan_when_explicitly_allowed() {
        let capabilities = caps(&[14]);
        let request = request(1, 1);
        let mut renderer = ptr::null_mut();
        // SAFETY: local structures satisfy the FFI contract.
        assert_eq!(unsafe { miu_renderer_create(&capabilities, &request, &mut renderer) }, STATUS_OK);
        let mut plan = MiuRenderPlan { backend: 0, tier: 0, emergency_fallback: 0, below_minimum: 0 };
        // SAFETY: renderer is a valid live local handle and plan has storage.
        assert_eq!(unsafe { miu_renderer_get_plan(renderer, &mut plan) }, STATUS_OK);
        assert_eq!(plan.backend, 14);
        assert_eq!(plan.emergency_fallback, 1);
        // SAFETY: renderer is released exactly once.
        unsafe { miu_renderer_release(renderer) };
    }
}

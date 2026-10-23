//! C ABI entry points for Miu.
//!
//! The ABI is defined in `include/miu/miu.h`. This crate owns no graphics device;
//! it validates a capability report and keeps the immutable selected render plan.

#![deny(unsafe_op_in_unsafe_fn)]

use core::ptr;
use miu_core::{
    select_best_effort, select_renderer, Backend, Camera2D, Canvas, Color, DeviceCapabilities,
    Platform, RenderPlan, RendererRequest, RenderTier, Rgba8, Sprite, Texture, Transform2D,
    UvRect, Vec2,
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

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuRgba8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuVec2 {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuTransform2D {
    translation: MiuVec2,
    rotation_radians: f32,
    scale: MiuVec2,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuUvRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuCamera2D {
    position: MiuVec2,
    viewport: MiuVec2,
    zoom: f32,
    rotation_radians: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MiuCpuSprite {
    transform: MiuTransform2D,
    uv: MiuUvRect,
    color: MiuColor,
    layer: i32,
}

#[repr(C)]
struct MiuCpuTexture {
    texture: Texture,
}

#[repr(C)]
struct MiuCpuCanvas {
    canvas: Canvas,
}

const fn rgba8_from_ffi(value: MiuRgba8) -> Rgba8 {
    Rgba8 {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
}

const fn vec2_from_ffi(value: MiuVec2) -> Vec2 {
    Vec2::new(value.x, value.y)
}

const fn color_from_ffi(value: MiuColor) -> Color {
    Color {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
}

const fn camera_from_ffi(value: MiuCamera2D) -> Camera2D {
    Camera2D {
        position: vec2_from_ffi(value.position),
        viewport: vec2_from_ffi(value.viewport),
        zoom: value.zoom,
        rotation_radians: value.rotation_radians,
    }
}

const fn sprite_from_ffi(value: MiuCpuSprite) -> Sprite {
    Sprite {
        texture: 0,
        transform: Transform2D {
            translation: vec2_from_ffi(value.transform.translation),
            rotation_radians: value.transform.rotation_radians,
            scale: vec2_from_ffi(value.transform.scale),
        },
        uv: UvRect {
            left: value.uv.left,
            top: value.uv.top,
            right: value.uv.right,
            bottom: value.uv.bottom,
        },
        color: color_from_ffi(value.color),
        layer: value.layer,
    }
}

fn rgba_byte_length(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
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

/// Creates a CPU texture from a caller-owned RGBA8 byte slice.
///
/// # Safety
///
/// `rgba_bytes` must point to `rgba_byte_count` readable bytes whenever that count
/// is nonzero. `out_texture` must point to writable storage for one opaque handle.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_texture_create_rgba8(
    width: u32,
    height: u32,
    rgba_bytes: *const u8,
    rgba_byte_count: usize,
    out_texture: *mut *mut MiuCpuTexture,
) -> u32 {
    if width == 0 || height == 0 {
        return STATUS_INVALID_ARGUMENT;
    }
    let Some(expected_length) = rgba_byte_length(width, height) else {
        return STATUS_INVALID_ARGUMENT;
    };
    if out_texture.is_null()
        || rgba_byte_count != expected_length
        || (expected_length != 0 && rgba_bytes.is_null())
    {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer/count validity is required by the function contract.
    let bytes = unsafe { core::slice::from_raw_parts(rgba_bytes, expected_length) };
    let pixels = bytes
        .chunks_exact(4)
        .map(|pixel| Rgba8 {
            r: pixel[0],
            g: pixel[1],
            b: pixel[2],
            a: pixel[3],
        })
        .collect();
    let Ok(texture) = Texture::new(width, height, pixels) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: out_texture was null-checked and is writable by the caller contract.
    unsafe { ptr::write(out_texture, Box::into_raw(Box::new(MiuCpuTexture { texture }))) };
    STATUS_OK
}

/// Releases a CPU texture created by [`miu_cpu_texture_create_rgba8`].
///
/// # Safety
///
/// If non-null, `texture` must be an unreleased handle from this library.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_texture_release(texture: *mut MiuCpuTexture) {
    if !texture.is_null() {
        // SAFETY: ownership transfers back exactly once according to the C ABI.
        unsafe { drop(Box::from_raw(texture)) };
    }
}

/// Allocates a transparent CPU canvas.
///
/// # Safety
///
/// `out_canvas` must point to writable storage for one opaque handle.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_create(
    width: u32,
    height: u32,
    out_canvas: *mut *mut MiuCpuCanvas,
) -> u32 {
    if out_canvas.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    let Ok(canvas) = Canvas::new(width, height) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: out_canvas was null-checked and is writable by the caller contract.
    unsafe { ptr::write(out_canvas, Box::into_raw(Box::new(MiuCpuCanvas { canvas }))) };
    STATUS_OK
}

/// Releases a CPU canvas created by [`miu_cpu_canvas_create`].
///
/// # Safety
///
/// If non-null, `canvas` must be an unreleased handle from this library.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_release(canvas: *mut MiuCpuCanvas) {
    if !canvas.is_null() {
        // SAFETY: ownership transfers back exactly once according to the C ABI.
        unsafe { drop(Box::from_raw(canvas)) };
    }
}

/// Clears a CPU canvas to a premultiplied colour.
///
/// # Safety
///
/// `canvas` must be a live handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_clear(
    canvas: *mut MiuCpuCanvas,
    color: MiuRgba8,
) -> u32 {
    if canvas.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: required by this function's caller contract after null validation.
    unsafe { (*canvas).canvas.clear(rgba8_from_ffi(color)) };
    STATUS_OK
}

/// Draws one transformed sprite to a CPU canvas.
///
/// # Safety
///
/// Every pointer must be non-null and refer to a live handle or readable structure
/// for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_draw_sprite(
    canvas: *mut MiuCpuCanvas,
    camera: *const MiuCamera2D,
    sprite: *const MiuCpuSprite,
    texture: *const MiuCpuTexture,
) -> u32 {
    if canvas.is_null() || camera.is_null() || sprite.is_null() || texture.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: every pointer was null-checked; readable/writable validity is contractual.
    let canvas = unsafe { &mut *canvas };
    // SAFETY: every pointer was null-checked; readable validity is contractual.
    let camera = unsafe { camera_from_ffi(*camera) };
    // SAFETY: every pointer was null-checked; readable validity is contractual.
    let sprite = unsafe { sprite_from_ffi(*sprite) };
    // SAFETY: every pointer was null-checked; readable validity is contractual.
    let texture = unsafe { &*texture };
    match canvas.canvas.draw_sprite(&camera, &sprite, &texture.texture) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_INVALID_ARGUMENT,
    }
}

/// Copies a CPU canvas as row-major premultiplied RGBA8 bytes.
///
/// # Safety
///
/// `canvas` must be live. `out_rgba_bytes` must point to at least
/// `out_rgba_byte_count` writable bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_copy_rgba8(
    canvas: *const MiuCpuCanvas,
    out_rgba_bytes: *mut u8,
    out_rgba_byte_count: usize,
) -> u32 {
    if canvas.is_null() || out_rgba_bytes.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: canvas was null-checked and valid readability is contractual.
    let canvas = unsafe { &*canvas };
    let Some(expected_length) = rgba_byte_length(canvas.canvas.width(), canvas.canvas.height()) else {
        return STATUS_INVALID_ARGUMENT;
    };
    if out_rgba_byte_count < expected_length {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: required writable capacity is checked above and pointer validity is contractual.
    let output = unsafe { core::slice::from_raw_parts_mut(out_rgba_bytes, expected_length) };
    for (destination, source) in output.chunks_exact_mut(4).zip(canvas.canvas.pixels()) {
        destination.copy_from_slice(&[source.r, source.g, source.b, source.a]);
    }
    STATUS_OK
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
    fn ffi_cpu_canvas_draws_and_exports_premultiplied_pixels() {
        let texture_bytes = [255, 0, 0, 255];
        let mut texture = ptr::null_mut();
        // SAFETY: local byte input and output pointer satisfy the FFI contract.
        assert_eq!(
            unsafe {
                miu_cpu_texture_create_rgba8(
                    1,
                    1,
                    texture_bytes.as_ptr(),
                    texture_bytes.len(),
                    &mut texture,
                )
            },
            STATUS_OK
        );
        let mut canvas = ptr::null_mut();
        // SAFETY: local output storage satisfies the FFI contract.
        assert_eq!(unsafe { miu_cpu_canvas_create(1, 1, &mut canvas) }, STATUS_OK);
        let camera = MiuCamera2D {
            position: MiuVec2 { x: 0.5, y: 0.5 },
            viewport: MiuVec2 { x: 1.0, y: 1.0 },
            zoom: 1.0,
            rotation_radians: 0.0,
        };
        let sprite = MiuCpuSprite {
            transform: MiuTransform2D {
                translation: MiuVec2 { x: 0.5, y: 0.5 },
                rotation_radians: 0.0,
                scale: MiuVec2 { x: 1.0, y: 1.0 },
            },
            uv: MiuUvRect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 },
            color: MiuColor { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            layer: 0,
        };
        // SAFETY: all structures and handles remain live for the draw call.
        assert_eq!(
            unsafe { miu_cpu_canvas_draw_sprite(canvas, &camera, &sprite, texture) },
            STATUS_OK
        );
        let mut output = [0_u8; 4];
        // SAFETY: the output slice has exactly one RGBA pixel of writable storage.
        assert_eq!(
            unsafe { miu_cpu_canvas_copy_rgba8(canvas, output.as_mut_ptr(), output.len()) },
            STATUS_OK
        );
        assert_eq!(output, texture_bytes);
        // SAFETY: each local handle is released exactly once.
        unsafe {
            miu_cpu_canvas_release(canvas);
            miu_cpu_texture_release(texture);
        }
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

//! C ABI entry points for Miu.
//!
//! The ABI is defined in `include/miu/miu.h`. This crate owns no graphics device;
//! it validates a capability report and keeps the immutable selected render plan.

#![deny(unsafe_op_in_unsafe_fn)]
// The public ABI is documented in include/miu/miu.h rather than duplicated here.
#![allow(missing_docs)]

use core::ptr;
use std::alloc::{alloc, dealloc, Layout};
use miu_core::{
    select_best_effort, select_renderer, Backend, Camera2D, Canvas, Color, DeviceCapabilities,
    Platform, RenderPlan, RendererRequest, RenderTier, Rgba8, SourceRect, Sprite, Texture,
    Transform2D, UvRect, Vec2,
};

const ABI_VERSION: u32 = 2;
const ABI_MIN_SUPPORTED_VERSION: u32 = 1;
// Backend IDs are a closed u8 enum from 1 through 17. Bound hostile FFI reports
// before constructing a slice or iterating untrusted caller memory.
const MAX_BACKEND_REPORT: usize = 17;
const STATUS_OK: u32 = 0;
const STATUS_INVALID_ARGUMENT: u32 = 1;
const STATUS_UNSUPPORTED: u32 = 2;
const STATUS_OUT_OF_MEMORY: u32 = 3;

#[repr(C)]
pub struct MiuDeviceCapabilities {
    abi_version: u32,
    platform: u8,
    reserved: [u8; 3],
    backends: *const u8,
    backend_count: usize,
}

#[repr(C)]
pub struct MiuRendererRequest {
    abi_version: u32,
    minimum_tier: u8,
    preferred_tier: u8,
    allow_best_effort: u8,
    reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MiuRenderPlanV2 {
    backend: u8,
    profile_backend: u8,
    tier: u8,
    emergency_fallback: u8,
    below_minimum: u8,
}

/// ABI v1 layout retained for the unsuffixed plan getter.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MiuRenderPlan {
    backend: u8,
    tier: u8,
    emergency_fallback: u8,
    below_minimum: u8,
}

#[repr(C)]
pub struct MiuRenderer {
    plan: MiuRenderPlanV2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuRgba8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuVec2 {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuTransform2D {
    translation: MiuVec2,
    rotation_radians: f32,
    scale: MiuVec2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuUvRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuSourceRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuCamera2D {
    position: MiuVec2,
    viewport: MiuVec2,
    zoom: f32,
    rotation_radians: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuCpuSpriteV2 {
    transform: MiuTransform2D,
    source: MiuSourceRect,
    uv: MiuUvRect,
    size: MiuVec2,
    pivot: MiuVec2,
    color: MiuColor,
    layer: i32,
}

/// ABI v1 sprite layout retained by `miu_cpu_canvas_draw_sprite`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MiuCpuSprite {
    transform: MiuTransform2D,
    uv: MiuUvRect,
    color: MiuColor,
    layer: i32,
}

#[repr(C)]
pub struct MiuCpuTexture {
    texture: Texture,
}

#[repr(C)]
pub struct MiuCpuCanvas {
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

const fn sprite_from_ffi(value: MiuCpuSpriteV2) -> Sprite {
    Sprite {
        texture: 0,
        transform: Transform2D {
            translation: vec2_from_ffi(value.transform.translation),
            rotation_radians: value.transform.rotation_radians,
            scale: vec2_from_ffi(value.transform.scale),
        },
        source: SourceRect {
            x: value.source.x,
            y: value.source.y,
            width: value.source.width,
            height: value.source.height,
        },
        uv: UvRect {
            left: value.uv.left,
            top: value.uv.top,
            right: value.uv.right,
            bottom: value.uv.bottom,
        },
        size: vec2_from_ffi(value.size),
        pivot: vec2_from_ffi(value.pivot),
        color: color_from_ffi(value.color),
        layer: value.layer,
    }
}

unsafe fn try_allocate_handle<T>(value: T) -> Option<*mut T> {
    let layout = Layout::new::<T>();
    // SAFETY: allocation uses the exact layout for `T`.
    let pointer = unsafe { alloc(layout) as *mut T };
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the allocation is suitably aligned and has space for one `T`.
        unsafe { ptr::write(pointer, value) };
        Some(pointer)
    }
}

unsafe fn release_handle<T>(pointer: *mut T) {
    let layout = Layout::new::<T>();
    // SAFETY: callers pass an allocation returned by `try_allocate_handle` exactly once.
    unsafe { ptr::drop_in_place(pointer) };
    // SAFETY: the pointer and layout match the allocating call above.
    unsafe { dealloc(pointer.cast::<u8>(), layout) };
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

const fn plan_to_ffi(plan: RenderPlan) -> MiuRenderPlanV2 {
    MiuRenderPlanV2 {
        backend: plan.backend as u8,
        profile_backend: plan.profile_backend as u8,
        tier: plan.tier as u8,
        emergency_fallback: plan.emergency_fallback as u8,
        below_minimum: plan.below_minimum as u8,
    }
}

const fn plan_to_ffi_v1(plan: MiuRenderPlanV2) -> MiuRenderPlan {
    MiuRenderPlan {
        backend: plan.backend,
        tier: plan.tier,
        emergency_fallback: plan.emergency_fallback,
        below_minimum: plan.below_minimum,
    }
}

/// Returns the ABI version implemented by this library.
#[no_mangle]
pub extern "C" fn miu_abi_version() -> u32 {
    ABI_VERSION
}

/// Returns the oldest revision implemented by legacy entry points.
#[no_mangle]
pub extern "C" fn miu_abi_min_supported_version() -> u32 {
    ABI_MIN_SUPPORTED_VERSION
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
    if !out_renderer.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_renderer, ptr::null_mut()) };
    }
    if capabilities.is_null() || request.is_null() || out_renderer.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }

    // SAFETY: null was checked above; the caller contract requires valid objects.
    let capabilities = unsafe { &*capabilities };
    // SAFETY: null was checked above; the caller contract requires a valid object.
    let request = unsafe { &*request };
    // v1 and v2 use identical inbound policy descriptors. Keep accepting v1
    // callers while v2 callers use the versioned result getter below.
    if capabilities.abi_version != request.abi_version
        || !(ABI_MIN_SUPPORTED_VERSION..=ABI_VERSION).contains(&capabilities.abi_version)
        || capabilities.reserved != [0; 3]
        || request.reserved != 0
    {
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
    if request.allow_best_effort > 1 {
        return STATUS_INVALID_ARGUMENT;
    }

    if capabilities.backend_count != 0 && capabilities.backends.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    if capabilities.backend_count > MAX_BACKEND_REPORT
        || capabilities.backend_count > isize::MAX as usize
    {
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

    // SAFETY: allocation returns either an owned handle or a null pointer.
    let Some(renderer) = (unsafe { try_allocate_handle(MiuRenderer { plan: plan_to_ffi(plan) }) }) else {
        return STATUS_OUT_OF_MEMORY;
    };
    // SAFETY: out_renderer was null-checked and the caller provides writable storage.
    unsafe { ptr::write(out_renderer, renderer) };
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
    if !out_plan.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_plan, MiuRenderPlan::default()) };
    }
    if renderer.is_null() || out_plan.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: required by this function's caller contract after null validation.
    let plan = plan_to_ffi_v1(unsafe { (*renderer).plan });
    // SAFETY: required by this function's caller contract after null validation.
    unsafe { ptr::write(out_plan, plan) };
    STATUS_OK
}

/// Copies a complete ABI v2 plan without changing the ABI v1 symbol or layout.
///
/// # Safety
/// `renderer` must be live and `out_plan` must be writable for one v2 plan.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_get_plan_v2(
    renderer: *const MiuRenderer,
    out_plan: *mut MiuRenderPlanV2,
) -> u32 {
    if !out_plan.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_plan, MiuRenderPlanV2::default()) };
    }
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
        unsafe { release_handle(renderer) };
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
    if !out_texture.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_texture, ptr::null_mut()) };
    }
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
    let mut pixels = Vec::new();
    if pixels.try_reserve_exact(expected_length / 4).is_err() {
        return STATUS_OUT_OF_MEMORY;
    }
    for pixel in bytes.chunks_exact(4) {
        pixels.push(Rgba8 { r: pixel[0], g: pixel[1], b: pixel[2], a: pixel[3] });
    }
    let texture = match Texture::new(width, height, pixels) {
        Ok(texture) => texture,
        Err(miu_core::TextureError::OutOfMemory) => return STATUS_OUT_OF_MEMORY,
        Err(_) => return STATUS_INVALID_ARGUMENT,
    };
    // SAFETY: allocation returns either an owned handle or a null pointer.
    let Some(texture) = (unsafe { try_allocate_handle(MiuCpuTexture { texture }) }) else {
        return STATUS_OUT_OF_MEMORY;
    };
    // SAFETY: out_texture was null-checked and is writable by the caller contract.
    unsafe { ptr::write(out_texture, texture) };
    STATUS_OK
}

/// Creates a CPU texture from explicitly straight-alpha RGBA8 bytes.
///
/// # Safety
/// The pointer/count and output-pointer requirements match
/// [`miu_cpu_texture_create_rgba8`].
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_texture_create_straight_rgba8(
    width: u32,
    height: u32,
    rgba_bytes: *const u8,
    rgba_byte_count: usize,
    out_texture: *mut *mut MiuCpuTexture,
) -> u32 {
    if !out_texture.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_texture, ptr::null_mut()) };
    }
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
    let mut pixels = Vec::new();
    if pixels.try_reserve_exact(expected_length / 4).is_err() {
        return STATUS_OUT_OF_MEMORY;
    }
    for pixel in bytes.chunks_exact(4) {
        pixels.push(Rgba8 { r: pixel[0], g: pixel[1], b: pixel[2], a: pixel[3] });
    }
    let texture = match Texture::from_straight_alpha(width, height, pixels) {
        Ok(texture) => texture,
        Err(miu_core::TextureError::OutOfMemory) => return STATUS_OUT_OF_MEMORY,
        Err(_) => return STATUS_INVALID_ARGUMENT,
    };
    // SAFETY: allocation returns either an owned handle or a null pointer.
    let Some(texture) = (unsafe { try_allocate_handle(MiuCpuTexture { texture }) }) else {
        return STATUS_OUT_OF_MEMORY;
    };
    // SAFETY: out_texture was null-checked and is writable by the caller contract.
    unsafe { ptr::write(out_texture, texture) };
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
        unsafe { release_handle(texture) };
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
    if !out_canvas.is_null() {
        // SAFETY: a non-null output pointer is writable by the C ABI contract.
        unsafe { ptr::write(out_canvas, ptr::null_mut()) };
    }
    if out_canvas.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    let canvas = match Canvas::new(width, height) {
        Ok(canvas) => canvas,
        Err(miu_core::TextureError::OutOfMemory) => return STATUS_OUT_OF_MEMORY,
        Err(_) => return STATUS_INVALID_ARGUMENT,
    };
    // SAFETY: allocation returns either an owned handle or a null pointer.
    let Some(canvas) = (unsafe { try_allocate_handle(MiuCpuCanvas { canvas }) }) else {
        return STATUS_OUT_OF_MEMORY;
    };
    // SAFETY: out_canvas was null-checked and is writable by the caller contract.
    unsafe { ptr::write(out_canvas, canvas) };
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
        unsafe { release_handle(canvas) };
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
    match unsafe { (*canvas).canvas.clear(rgba8_from_ffi(color)) } {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_INVALID_ARGUMENT,
    }
}

/// Draws one transformed sprite to a CPU canvas.
///
/// # Safety
///
/// Every pointer must be non-null and refer to a live handle or readable structure
/// for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_draw_sprite_v2(
    canvas: *mut MiuCpuCanvas,
    camera: *const MiuCamera2D,
    sprite: *const MiuCpuSpriteV2,
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

/// Draws an ABI v1 sprite without reinterpreting its shorter descriptor as v2.
///
/// # Safety
/// Every pointer must be non-null and valid for the call.
#[no_mangle]
pub unsafe extern "C" fn miu_cpu_canvas_draw_sprite(
    canvas: *mut MiuCpuCanvas,
    camera: *const MiuCamera2D,
    sprite: *const MiuCpuSprite,
    texture: *const MiuCpuTexture,
) -> u32 {
    if sprite.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: `sprite` was checked and is readable by the function contract.
    let legacy = unsafe { *sprite };
    let v2 = MiuCpuSpriteV2 {
        transform: legacy.transform,
        source: MiuSourceRect { x: 0, y: 0, width: 0, height: 0 },
        uv: legacy.uv,
        size: MiuVec2 { x: 0.0, y: 0.0 },
        pivot: MiuVec2 { x: 0.5, y: 0.5 },
        color: legacy.color,
        layer: legacy.layer,
    };
    // SAFETY: the other pointer requirements are forwarded unchanged; `v2` is live.
    unsafe { miu_cpu_canvas_draw_sprite_v2(canvas, camera, &v2, texture) }
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
    fn ffi_best_effort_cannot_bypass_an_invalid_request() {
        let capabilities = caps(&[11]);
        let mut invalid_request = request(3, 1);
        invalid_request.preferred_tier = 1;
        let mut renderer = ptr::null_mut();
        // SAFETY: local inputs are valid ABI objects; only policy fields conflict.
        assert_eq!(
            unsafe { miu_renderer_create(&capabilities, &invalid_request, &mut renderer) },
            STATUS_INVALID_ARGUMENT
        );
        assert!(renderer.is_null());
    }

    #[test]
    fn ffi_creation_failures_clear_output_handles() {
        let invalid_capabilities = MiuDeviceCapabilities {
            abi_version: ABI_VERSION,
            platform: 0,
            reserved: [0; 3],
            backends: ptr::null(),
            backend_count: 0,
        };
        let request = request(1, 0);
        let mut renderer = 1_usize as *mut MiuRenderer;
        // SAFETY: local structures satisfy the FFI contract.
        assert_eq!(
            unsafe { miu_renderer_create(&invalid_capabilities, &request, &mut renderer) },
            STATUS_INVALID_ARGUMENT
        );
        assert!(renderer.is_null());

        let mut texture = 1_usize as *mut MiuCpuTexture;
        // SAFETY: invalid dimensions are intentionally rejected and output is writable.
        assert_eq!(
            unsafe { miu_cpu_texture_create_rgba8(0, 1, ptr::null(), 0, &mut texture) },
            STATUS_INVALID_ARGUMENT
        );
        assert!(texture.is_null());

        let mut canvas = 1_usize as *mut MiuCpuCanvas;
        // SAFETY: invalid dimensions are intentionally rejected and output is writable.
        assert_eq!(unsafe { miu_cpu_canvas_create(0, 1, &mut canvas) }, STATUS_INVALID_ARGUMENT);
        assert!(canvas.is_null());
    }

    #[test]
    fn ffi_rejects_noncanonical_or_unbounded_capability_reports() {
        let oversized = [1_u8; MAX_BACKEND_REPORT + 1];
        let capabilities = MiuDeviceCapabilities {
            abi_version: ABI_VERSION, platform: 5, reserved: [1, 0, 0],
            backends: oversized.as_ptr(), backend_count: oversized.len(),
        };
        let request = request(1, 0);
        let mut renderer = ptr::null_mut();
        // SAFETY: all local FFI inputs are live; validation rejects before reading the list.
        assert_eq!(unsafe { miu_renderer_create(&capabilities, &request, &mut renderer) }, STATUS_INVALID_ARGUMENT);
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
        let sprite = MiuCpuSpriteV2 {
            transform: MiuTransform2D {
                translation: MiuVec2 { x: 0.5, y: 0.5 },
                rotation_radians: 0.0,
                scale: MiuVec2 { x: 1.0, y: 1.0 },
            },
            source: MiuSourceRect { x: 0, y: 0, width: 0, height: 0 },
            uv: MiuUvRect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 },
            size: MiuVec2 { x: 1.0, y: 1.0 },
            pivot: MiuVec2 { x: 0.5, y: 0.5 },
            color: MiuColor { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            layer: 0,
        };
        // SAFETY: all structures and handles remain live for the draw call.
        assert_eq!(
            unsafe { miu_cpu_canvas_draw_sprite_v2(canvas, &camera, &sprite, texture) },
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
    fn ffi_preserves_v1_layouts_and_converts_straight_alpha_explicitly() {
        assert_eq!(miu_abi_version(), ABI_VERSION);
        assert_eq!(miu_abi_min_supported_version(), ABI_MIN_SUPPORTED_VERSION);
        let straight = [255, 0, 0, 128];
        let mut texture = ptr::null_mut();
        // SAFETY: local inputs satisfy the ABI contract.
        assert_eq!(unsafe {
            miu_cpu_texture_create_straight_rgba8(1, 1, straight.as_ptr(), straight.len(), &mut texture)
        }, STATUS_OK);
        let mut canvas = ptr::null_mut();
        // SAFETY: local output storage is writable.
        assert_eq!(unsafe { miu_cpu_canvas_create(1, 1, &mut canvas) }, STATUS_OK);
        let camera = MiuCamera2D {
            position: MiuVec2 { x: 0.5, y: 0.5 }, viewport: MiuVec2 { x: 1.0, y: 1.0 },
            zoom: 1.0, rotation_radians: 0.0,
        };
        let v1_sprite = MiuCpuSprite {
            transform: MiuTransform2D { translation: MiuVec2 { x: 0.5, y: 0.5 }, rotation_radians: 0.0, scale: MiuVec2 { x: 1.0, y: 1.0 } },
            uv: MiuUvRect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 },
            color: MiuColor { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, layer: 0,
        };
        // SAFETY: all local structures and handles live for the call.
        assert_eq!(unsafe { miu_cpu_canvas_draw_sprite(canvas, &camera, &v1_sprite, texture) }, STATUS_OK);
        let mut output = [0_u8; 4];
        // SAFETY: output has exactly the required storage.
        assert_eq!(unsafe { miu_cpu_canvas_copy_rgba8(canvas, output.as_mut_ptr(), output.len()) }, STATUS_OK);
        assert_eq!(output, [128, 0, 0, 128]);
        // SAFETY: rejects a non-premultiplied clear without dereferencing invalid data.
        assert_eq!(unsafe { miu_cpu_canvas_clear(canvas, MiuRgba8 { r: 255, g: 0, b: 0, a: 0 }) }, STATUS_INVALID_ARGUMENT);
        // SAFETY: each handle is released exactly once.
        unsafe { miu_cpu_canvas_release(canvas); miu_cpu_texture_release(texture); }
    }

    #[test]
    fn v1_policy_create_and_plan_getter_remain_binary_layout_compatible() {
        let mut capabilities = caps(&[10]);
        capabilities.abi_version = 1;
        let mut legacy_request = request(2, 0);
        legacy_request.abi_version = 1;
        let mut renderer = ptr::null_mut();
        // SAFETY: v1 field layouts are preserved and local structures are valid.
        assert_eq!(unsafe { miu_renderer_create(&capabilities, &legacy_request, &mut renderer) }, STATUS_OK);
        let mut plan = MiuRenderPlan::default();
        // SAFETY: the v1 output layout exactly matches the unsuffixed getter.
        assert_eq!(unsafe { miu_renderer_get_plan(renderer, &mut plan) }, STATUS_OK);
        assert_eq!(plan.backend, 10);
        assert_eq!(plan.tier, 2);
        // SAFETY: renderer is released exactly once.
        unsafe { miu_renderer_release(renderer) };
    }

    #[test]
    fn ffi_returns_an_emergency_plan_when_explicitly_allowed() {
        let capabilities = caps(&[14]);
        let request = request(1, 1);
        let mut renderer = ptr::null_mut();
        // SAFETY: local structures satisfy the FFI contract.
        assert_eq!(unsafe { miu_renderer_create(&capabilities, &request, &mut renderer) }, STATUS_OK);
        let mut plan = MiuRenderPlanV2 { backend: 0, profile_backend: 0, tier: 0, emergency_fallback: 0, below_minimum: 0 };
        // SAFETY: renderer is a valid live local handle and plan has storage.
        assert_eq!(unsafe { miu_renderer_get_plan_v2(renderer, &mut plan) }, STATUS_OK);
        assert_eq!(plan.backend, 14);
        assert_eq!(plan.emergency_fallback, 1);
        // SAFETY: renderer is released exactly once.
        unsafe { miu_renderer_release(renderer) };
    }
}

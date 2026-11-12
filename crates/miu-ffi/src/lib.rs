//! Versioned C ABI for Miu (ABI version 3).
//!
//! The ABI is the only supported binary boundary. It exposes the CPU renderer, the
//! scene graph, fonts, textures, and document loading to C++, C#, and web hosts.
//! Rust callers should link the `miu-scene` crate directly instead.
//!
//! # Conventions
//!
//! * Every function returns [`MiuStatus`] unless it returns a value directly.
//!   `MIU_STATUS_OK` is zero, so `if (status)` in C means "failed".
//! * Out-parameters are only written when the call succeeds.
//! * Opaque handles (`miu_texture`, `miu_font`, `miu_renderer`, `miu_scene`,
//!   `miu_document`) are owned by the caller and must be released with their
//!   matching `*_release` function. Releasing `NULL` is a no-op.
//! * Byte buffers are copied on the way in, so the caller may free its data
//!   immediately. Text is UTF-8 without a terminating NUL.
//! * Pixels are premultiplied RGBA8 unless a function name says `straight`.
//! * No function is thread-safe for the same handle. Different handles may be used
//!   from different threads.
//! * Panics abort the process: the release profile is built with `panic = "abort"`,
//!   so the ABI never returns a partially initialised handle.

#![allow(unsafe_code)]
#![deny(missing_docs)]

use core::ffi::{c_char, c_void};
use std::collections::HashMap;
use std::ptr;
use std::slice;

use miu_core::{
    Affine2, BlendMode, Camera2D, Color, ErrorCode, Platform, QualityPreset, QualitySettings, Rect,
    RenderPlan, RenderTier, RendererRequest, Rgba8, Transform2D, Vec2, plan_warning,
    select_best_effort, select_renderer,
};
use miu_graphics::{
    Assets, DisplayList, DrawOp, FillOp, Font, Paint, Path, RenderStats, SoftwareRenderer, Sprite,
    TextAlign, TextRun, TextStyle, TextWrap, Texture, layout_text,
};
use miu_scene::{
    Document, Node, NodeId, ROOT, ResolvedAsset, Scene, SceneError, ShapeVisual, SpriteVisual,
    TextVisual,
};

/// ABI version implemented by this library.
pub const MIU_ABI_VERSION: u32 = 3;

/// Oldest ABI version this library still accepts.
pub const MIU_ABI_MIN_VERSION: u32 = 3;

/// `miu_status_t` value for success.
const STATUS_OK: u32 = ErrorCode::Ok as u32;

fn status_of(code: ErrorCode) -> u32 {
    code as u32
}

// ---------------------------------------------------------------------------
// Plain data structures
// ---------------------------------------------------------------------------

/// Two-dimensional vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuVec2 {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

/// Premultiplied RGBA colour with components in `[0, 1]`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuColor {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

/// Premultiplied RGBA texel.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MiuRgba8 {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
}

/// Axis-aligned rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// Translation, rotation, and scale.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MiuTransform2d {
    /// Translation in the parent space.
    pub translation: MiuVec2,
    /// Clockwise rotation in radians.
    pub rotation_radians: f32,
    /// Per-axis scale.
    pub scale: MiuVec2,
}

impl Default for MiuTransform2d {
    fn default() -> Self {
        Self {
            translation: MiuVec2::default(),
            rotation_radians: 0.0,
            scale: MiuVec2 { x: 1.0, y: 1.0 },
        }
    }
}

/// Independent 2D camera.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MiuCamera2d {
    /// World point shown at the viewport centre.
    pub position: MiuVec2,
    /// Viewport size in logical pixels.
    pub viewport: MiuVec2,
    /// Uniform zoom.
    pub zoom: f32,
    /// Clockwise rotation in radians.
    pub rotation_radians: f32,
}

impl Default for MiuCamera2d {
    fn default() -> Self {
        Self {
            position: MiuVec2::default(),
            viewport: MiuVec2 { x: 1.0, y: 1.0 },
            zoom: 1.0,
            rotation_radians: 0.0,
        }
    }
}

/// Pixel source rectangle inside a texture atlas.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuSourceRect {
    /// Left texel.
    pub x: u32,
    /// Top texel.
    pub y: u32,
    /// Width in texels; zero selects the whole texture.
    pub width: u32,
    /// Height in texels; zero selects the whole texture.
    pub height: u32,
}

/// Sprite description for direct renderer calls.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MiuSprite {
    /// Texture id previously uploaded to the renderer.
    pub texture: u64,
    /// Local-to-world transform.
    pub transform: MiuTransform2d,
    /// Atlas source rectangle.
    pub source: MiuSourceRect,
    /// Normalised sampling rectangle as `[left, top, right, bottom]`.
    pub uv: [f32; 4],
    /// Destination size; zero uses the source size.
    pub size: MiuVec2,
    /// Normalised origin inside the destination rectangle.
    pub pivot: MiuVec2,
    /// Premultiplied tint.
    pub color: MiuColor,
    /// Painter layer.
    pub layer: i32,
    /// Blend mode, as `miu_blend_t`.
    pub blend: u8,
}

impl Default for MiuSprite {
    fn default() -> Self {
        Self {
            texture: 0,
            transform: MiuTransform2d::default(),
            source: MiuSourceRect::default(),
            uv: [0.0, 0.0, 1.0, 1.0],
            size: MiuVec2::default(),
            pivot: MiuVec2::default(),
            color: MiuColor { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            layer: 0,
            blend: BlendMode::SourceOver as u8,
        }
    }
}

/// Text layout parameters.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MiuTextStyle {
    /// Em size in pixels.
    pub font_size: f32,
    /// Line-height multiplier.
    pub line_height: f32,
    /// Extra glyph spacing in pixels.
    pub letter_spacing: f32,
    /// `miu_text_align_t`.
    pub align: u8,
    /// `miu_text_wrap_t`.
    pub wrap: u8,
    /// Reserved for future fields; must be zero.
    pub reserved: [u8; 2],
}

impl Default for MiuTextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            line_height: 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left as u8,
            wrap: TextWrap::Word as u8,
            reserved: [0; 2],
        }
    }
}

/// Renderer selection request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MiuRenderRequest {
    /// Lowest tier the project supports.
    pub minimum_tier: u8,
    /// Tier tried first.
    pub preferred_tier: u8,
    /// Requested quality preset.
    pub preset: u8,
    /// Non-zero selects the best-effort path, which may plan an emergency API.
    pub best_effort: u8,
}

/// Selected render plan.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuRenderPlan {
    /// Actual backend to create.
    pub backend: u8,
    /// Profile that implied the tier.
    pub profile_backend: u8,
    /// Selected tier.
    pub tier: u8,
    /// Quality preset after tier clamping.
    pub preset: u8,
    /// Bit 0: emergency fallback; bit 1: below the declared minimum.
    pub flags: u32,
}

/// Per-frame renderer counters.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MiuRenderStats {
    /// Sprite operations.
    pub sprites: u32,
    /// Filled paths.
    pub fills: u32,
    /// Stroked paths.
    pub strokes: u32,
    /// Glyphs.
    pub glyphs: u32,
    /// Culled or clipped operations.
    pub skipped: u32,
    /// Glyph outline fills that bypassed the cache.
    pub uncached_glyphs: u32,
    /// Estimated covered pixels.
    pub covered_pixels: u64,
}

/// Failure while loading a document through the ABI.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiuDocumentError {
    /// Invalid JSON or document structure.
    InvalidDocument = 1,
    /// An asset referenced by the document was never supplied.
    MissingAsset = 2,
    /// An asset could not be decoded.
    Decode = 3,
    /// The scene rejected a node or animation.
    Scene = 4,
}

/// Asset category reported by `miu_document_asset_kind`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiuAssetKind {
    /// Encoded image.
    Texture = 0,
    /// TrueType font.
    Font = 1,
}

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------

/// Opaque CPU texture.
pub struct MiuTexture {
    inner: Texture,
}

/// Opaque parsed font.
pub struct MiuFont {
    inner: Font,
}

/// Opaque software renderer with its own asset stores.
pub struct MiuRenderer {
    inner: SoftwareRenderer,
}

/// Opaque scene with assets and animations.
pub struct MiuScene {
    scene: Scene,
    assets: Assets,
    animations: Vec<miu_scene::Animation>,
}

/// Opaque parsed document plus the assets supplied for it.
pub struct MiuDocument {
    document: Document,
    textures: HashMap<String, Texture>,
    fonts: HashMap<String, Font>,
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

fn vec2_from(value: MiuVec2) -> Vec2 {
    Vec2::new(value.x, value.y)
}

fn vec2_to(value: Vec2) -> MiuVec2 {
    MiuVec2 { x: value.x, y: value.y }
}

fn color_from(value: MiuColor) -> Color {
    Color::new(value.r, value.g, value.b, value.a)
}

fn color_to(value: Color) -> MiuColor {
    MiuColor { r: value.r, g: value.g, b: value.b, a: value.a }
}

fn rgba8_from(value: MiuRgba8) -> Rgba8 {
    Rgba8::new(value.r, value.g, value.b, value.a)
}

fn rect_from(value: MiuRect) -> Rect {
    Rect::new(value.x, value.y, value.width, value.height)
}

fn transform_from(value: MiuTransform2d) -> Transform2D {
    Transform2D::new(vec2_from(value.translation), value.rotation_radians, vec2_from(value.scale))
}

fn camera_from(value: MiuCamera2d) -> Camera2D {
    Camera2D::with_state(
        vec2_from(value.position),
        vec2_from(value.viewport),
        value.zoom,
        value.rotation_radians,
    )
    .unwrap_or(Camera2D {
        position: Vec2::ZERO,
        viewport: Vec2::new(1.0, 1.0),
        zoom: 1.0,
        rotation_radians: 0.0,
    })
}

fn blend_from(value: u8) -> BlendMode {
    match value {
        1 => BlendMode::Add,
        2 => BlendMode::Multiply,
        3 => BlendMode::Screen,
        4 => BlendMode::Copy,
        _ => BlendMode::SourceOver,
    }
}

fn style_from(value: &MiuTextStyle) -> TextStyle {
    let align = match value.align {
        1 => TextAlign::Center,
        2 => TextAlign::Right,
        _ => TextAlign::Left,
    };
    let wrap = match value.wrap {
        0 => TextWrap::None,
        2 => TextWrap::Character,
        _ => TextWrap::Word,
    };
    TextStyle::new(value.font_size)
        .with_line_height(value.line_height)
        .with_letter_spacing(value.letter_spacing)
        .with_align(align)
        .with_wrap(wrap)
}

fn sprite_from(value: &MiuSprite) -> Sprite {
    let mut sprite = Sprite::new(value.texture)
        .with_trs(transform_from(value.transform))
        .with_source(miu_graphics::SourceRect::new(
            value.source.x,
            value.source.y,
            value.source.width,
            value.source.height,
        ))
        .with_size(vec2_from(value.size))
        .with_pivot(vec2_from(value.pivot))
        .with_color(color_from(value.color))
        .with_blend(blend_from(value.blend))
        .with_layer(value.layer);
    sprite.uv = miu_graphics::UvRect::new(value.uv[0], value.uv[1], value.uv[2], value.uv[3]);
    sprite
}

/// Copies `text` into a caller buffer using the length-prefix convention.
///
/// When `buffer` is null or too small, nothing is written and
/// `MIU_STATUS_INVALID_ARGUMENT` / `MIU_STATUS_LIMIT_EXCEEDED` is returned. The
/// required byte count (excluding the terminating NUL) is always reported.
unsafe fn write_text(
    text: &str,
    buffer: *mut c_char,
    capacity: usize,
    out_length: *mut usize,
) -> u32 {
    let bytes = text.as_bytes();
    if !out_length.is_null() {
        unsafe { *out_length = bytes.len() };
    }
    if buffer.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    if capacity < bytes.len() + 1 {
        return status_of(ErrorCode::LimitExceeded);
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer as *mut u8, bytes.len());
        *buffer.add(bytes.len()) = 0;
    }
    STATUS_OK
}

unsafe fn bytes_from<'a>(data: *const u8, length: usize) -> Option<&'a [u8]> {
    if data.is_null() {
        return None;
    }
    if length == 0 {
        return Some(&[]);
    }
    Some(unsafe { slice::from_raw_parts(data, length) })
}

unsafe fn text_from<'a>(data: *const c_char, length: usize) -> Option<&'a str> {
    let bytes = unsafe { bytes_from(data as *const u8, length) }?;
    core::str::from_utf8(bytes).ok()
}

unsafe fn write_out<T>(target: *mut T, value: T) -> u32 {
    if target.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    unsafe {
        ptr::write(target, value);
    }
    STATUS_OK
}

fn error_code_scene(error: SceneError) -> ErrorCode {
    match error {
        SceneError::InvalidNode(_) | SceneError::InvalidParent(_) => ErrorCode::NotFound,
        SceneError::Cycle | SceneError::RootImmutable | SceneError::Camera(_) => {
            ErrorCode::InvalidArgument
        }
        SceneError::TooManyNodes => ErrorCode::LimitExceeded,
    }
}

// ---------------------------------------------------------------------------
// Version and diagnostics
// ---------------------------------------------------------------------------

/// Returns the ABI version implemented by the loaded library.
#[no_mangle]
pub extern "C" fn miu_abi_version() -> u32 {
    MIU_ABI_VERSION
}

/// Returns the oldest ABI version this library accepts from a caller.
#[no_mangle]
pub extern "C" fn miu_abi_min_supported_version() -> u32 {
    MIU_ABI_MIN_VERSION
}

/// Returns the crate version string, for example `0.1.0-dev`.
#[no_mangle]
pub extern "C" fn miu_version_string() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

/// Returns an English description of a status code.
#[no_mangle]
pub extern "C" fn miu_status_message(status: u32) -> *const c_char {
    let name = match ErrorCode::from_u32(status) {
        Some(code) => miu_core::error_message(code),
        None => "unknown status code",
    };
    // Leak a small, bounded set of static strings.
    let text: &'static str = match name {
        "ok" => "ok\0",
        "invalid argument" => "invalid argument\0",
        "unsupported by this device or format" => "unsupported by this device or format\0",
        "out of memory" => "out of memory\0",
        "resource not found" => "resource not found\0",
        "input/output failure" => "input/output failure\0",
        "image decoding failed" => "image decoding failed\0",
        "document parsing failed" => "document parsing failed\0",
        "operation is not valid in the current state" => {
            "operation is not valid in the current state\0"
        }
        "configured limit exceeded" => "configured limit exceeded\0",
        "font parsing failed" => "font parsing failed\0",
        "invalid geometry or transform" => "invalid geometry or transform\0",
        "incompatible ABI version" => "incompatible ABI version\0",
        _ => "unknown status code\0",
    };
    text.as_ptr() as *const c_char
}

/// Returns the stable name of a platform constant, or `NULL` when unknown.
#[no_mangle]
pub extern "C" fn miu_platform_name(platform: u8) -> *const c_char {
    let text: &'static str = match platform {
        1 => "windows\0",
        2 => "android\0",
        3 => "macos\0",
        4 => "ios\0",
        5 => "linux\0",
        6 => "web\0",
        _ => return ptr::null(),
    };
    text.as_ptr() as *const c_char
}

/// Returns the stable name of a backend constant, or `NULL` when unknown.
#[no_mangle]
pub extern "C" fn miu_backend_name(backend: u8) -> *const c_char {
    let Some(backend) = miu_core::Backend::from_u8(backend) else {
        return ptr::null();
    };
    backend_display(backend)
}

fn backend_display(backend: miu_core::Backend) -> *const c_char {
    let text: &'static str = match backend.id() {
        "direct2d-1.2" => "Direct2D 1.2\0",
        "direct3d-11.3" => "Direct3D 11.3\0",
        "direct3d-12" => "Direct3D 12\0",
        "opengles-3.0" => "OpenGL ES 3.0\0",
        "opengles-2.0" => "OpenGL ES 2.0\0",
        "metal-2" => "Metal 2\0",
        "metal-3" => "Metal 3\0",
        "metal-1" => "Metal 1\0",
        "vulkan-1.1" => "Vulkan 1.1\0",
        "vulkan-1.2" => "Vulkan 1.2\0",
        "vulkan-1.3" => "Vulkan 1.3\0",
        "vulkan-1.4" => "Vulkan 1.4\0",
        "opengl-3.3" => "OpenGL 3.3\0",
        "opengl-3.0" => "OpenGL 3.0\0",
        "webgl-2" => "WebGL 2\0",
        "webgl-1" => "WebGL 1\0",
        "webgpu" => "WebGPU\0",
        _ => return ptr::null(),
    };
    text.as_ptr() as *const c_char
}

// ---------------------------------------------------------------------------
// Renderer selection
// ---------------------------------------------------------------------------

/// Selects a render plan from a probed capability list.
///
/// `backends` points at `backend_count` `miu_backend_t` values. When
/// `request.best_effort` is non-zero the call may return an emergency or
/// below-minimum plan; otherwise only compliant plans are returned.
#[no_mangle]
pub unsafe extern "C" fn miu_select_renderer(
    platform: u8,
    backends: *const u8,
    backend_count: usize,
    request: *const MiuRenderRequest,
    out_plan: *mut MiuRenderPlan,
) -> u32 {
    let Some(platform) = Platform::from_u8(platform) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if request.is_null() || out_plan.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    if backend_count > 0 && backends.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let request = unsafe { &*request };
    let Some(minimum_tier) = RenderTier::from_u8(request.minimum_tier) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(preferred_tier) = RenderTier::from_u8(request.preferred_tier) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let preset = QualityPreset::from_u8(request.preset).unwrap_or(QualityPreset::Medium);
    let request_values = RendererRequest {
        minimum_tier,
        preferred_tier,
        preset,
    };
    if request_values.validate().is_err() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let reported = if backend_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(backends, backend_count) }
    };
    let mut device = miu_core::DeviceCapabilities::for_platform(platform);
    for backend in reported {
        if let Some(backend) = miu_core::Backend::from_u8(*backend) {
            device.add_backend(backend);
        }
    }
    let selection = if request.best_effort != 0 {
        select_best_effort(&device, &request_values)
    } else {
        select_renderer(&device, &request_values)
    };
    match selection {
        Ok(plan) => {
            let mut flags = 0u32;
            if plan.emergency_fallback {
                flags |= 1;
            }
            if plan.below_minimum {
                flags |= 2;
            }
            let value = MiuRenderPlan {
                backend: plan.backend as u8,
                profile_backend: plan.profile_backend as u8,
                tier: plan.tier as u8,
                preset: plan.preset as u8,
                flags,
            };
            unsafe { ptr::write(out_plan, value) };
            STATUS_OK
        }
        Err(_) => status_of(ErrorCode::Unsupported),
    }
}

/// Returns the warning a host must show for a plan, or an empty string.
#[no_mangle]
pub unsafe extern "C" fn miu_plan_warning(plan: *const MiuRenderPlan) -> *const c_char {
    if plan.is_null() {
        return b"\0".as_ptr() as *const c_char;
    }
    let plan = unsafe { &*plan };
    let some_plan = RenderPlan {
        backend: miu_core::Backend::from_u8(plan.backend).unwrap_or(miu_core::Backend::OpenGL30),
        profile_backend: miu_core::Backend::from_u8(plan.profile_backend)
            .unwrap_or(miu_core::Backend::OpenGL30),
        tier: RenderTier::from_u8(plan.tier).unwrap_or(RenderTier::Compatibility),
        preset: QualityPreset::from_u8(plan.preset).unwrap_or(QualityPreset::Low),
        emergency_fallback: plan.flags & 1 != 0,
        below_minimum: plan.flags & 2 != 0,
    };
    match plan_warning(&some_plan) {
        Some(_) => {
            // Warnings are fixed strings; expose the stable ones without allocation.
            if some_plan.emergency_fallback {
                b"Emergency graphics fallback selected; rendering may be unstable.\0".as_ptr()
                    as *const c_char
            } else {
                b"Device is below the project's minimum tier; forcing may crash.\0".as_ptr()
                    as *const c_char
            }
        }
        None => b"\0".as_ptr() as *const c_char,
    }
}

// ---------------------------------------------------------------------------
// Textures
// ---------------------------------------------------------------------------

/// Creates a texture from premultiplied RGBA8 pixels.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_create(
    width: u32,
    height: u32,
    pixels: *const u8,
    out_texture: *mut *mut MiuTexture,
) -> u32 {
    if out_texture.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let count = match (width as usize).checked_mul(height as usize) {
        Some(count) if count > 0 => count,
        _ => return status_of(ErrorCode::InvalidArgument),
    };
    let Some(bytes) = (unsafe { bytes_from(pixels, count * 4) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let mut texels = Vec::with_capacity(count);
    for chunk in bytes.chunks_exact(4) {
        texels.push(Rgba8::new(chunk[0], chunk[1], chunk[2], chunk[3]));
    }
    match Texture::new(width, height, texels) {
        Ok(texture) => {
            let handle = Box::into_raw(Box::new(MiuTexture { inner: texture }));
            unsafe { ptr::write(out_texture, handle) };
            STATUS_OK
        }
        Err(error) => status_of(match error {
            miu_graphics::TextureError::InvalidPremultipliedPixel => ErrorCode::InvalidArgument,
            miu_graphics::TextureError::OutOfMemory => ErrorCode::OutOfMemory,
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

/// Creates a texture from straight-alpha RGBA8 pixels, premultiplying them.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_create_straight(
    width: u32,
    height: u32,
    pixels: *const u8,
    out_texture: *mut *mut MiuTexture,
) -> u32 {
    if out_texture.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let count = match (width as usize).checked_mul(height as usize) {
        Some(count) if count > 0 => count,
        _ => return status_of(ErrorCode::InvalidArgument),
    };
    let Some(bytes) = (unsafe { bytes_from(pixels, count * 4) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match Texture::from_rgba8_bytes(width, height, bytes) {
        Ok(texture) => {
            let handle = Box::into_raw(Box::new(MiuTexture { inner: texture }));
            unsafe { ptr::write(out_texture, handle) };
            STATUS_OK
        }
        Err(error) => status_of(match error {
            miu_graphics::TextureError::OutOfMemory => ErrorCode::OutOfMemory,
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

/// Decodes PNG, BMP, TGA, or QOI bytes into a texture.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_load(
    bytes: *const u8,
    length: usize,
    out_texture: *mut *mut MiuTexture,
) -> u32 {
    if out_texture.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(data) = (unsafe { bytes_from(bytes, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match miu_graphics::decode_image(data).and_then(|image| image.to_texture()) {
        Ok(texture) => {
            let handle = Box::into_raw(Box::new(MiuTexture { inner: texture }));
            unsafe { ptr::write(out_texture, handle) };
            STATUS_OK
        }
        Err(error) => status_of(match error {
            miu_graphics::ImageError::Unsupported(_) => ErrorCode::Unsupported,
            miu_graphics::ImageError::TooLarge => ErrorCode::LimitExceeded,
            _ => ErrorCode::Decode,
        }),
    }
}

/// Reads a texture's size.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_size(
    texture: *const MiuTexture,
    out_width: *mut u32,
    out_height: *mut u32,
) -> u32 {
    let Some(texture) = (unsafe { texture.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if out_width.is_null() || out_height.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    unsafe {
        ptr::write(out_width, texture.inner.width());
        ptr::write(out_height, texture.inner.height());
    }
    STATUS_OK
}

/// Copies premultiplied RGBA8 pixels into a caller buffer.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_read(
    texture: *const MiuTexture,
    buffer: *mut u8,
    capacity: usize,
) -> u32 {
    let Some(texture) = (unsafe { texture.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if buffer.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let pixels = texture.inner.pixels();
    if capacity < pixels.len() * 4 {
        return status_of(ErrorCode::LimitExceeded);
    }
    for (index, pixel) in pixels.iter().enumerate() {
        unsafe {
            *buffer.add(index * 4) = pixel.r;
            *buffer.add(index * 4 + 1) = pixel.g;
            *buffer.add(index * 4 + 2) = pixel.b;
            *buffer.add(index * 4 + 3) = pixel.a;
        }
    }
    STATUS_OK
}

/// Releases a texture. Passing `NULL` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn miu_texture_release(texture: *mut MiuTexture) {
    if texture.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(texture));
    }
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// Parses TrueType bytes into a font handle.
#[no_mangle]
pub unsafe extern "C" fn miu_font_load(
    bytes: *const u8,
    length: usize,
    out_font: *mut *mut MiuFont,
) -> u32 {
    if out_font.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(data) = (unsafe { bytes_from(bytes, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if data.is_empty() {
        return status_of(ErrorCode::InvalidArgument);
    }
    match Font::from_bytes(data.to_vec()) {
        Ok(font) => {
            let handle = Box::into_raw(Box::new(MiuFont { inner: font }));
            unsafe { ptr::write(out_font, handle) };
            STATUS_OK
        }
        Err(error) => status_of(match error {
            miu_graphics::FontError::Unsupported(_) => ErrorCode::Unsupported,
            _ => ErrorCode::Font,
        }),
    }
}

/// Reports line metrics for a font at a given em size.
#[no_mangle]
pub unsafe extern "C" fn miu_font_metrics(
    font: *const MiuFont,
    font_size: f32,
    out_line_height: *mut f32,
    out_ascent: *mut f32,
    out_descent: *mut f32,
) -> u32 {
    let Some(font) = (unsafe { font.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if out_line_height.is_null() || out_ascent.is_null() || out_descent.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let scale = if font_size.is_finite() && font_size > 0.0 { font_size } else { 16.0 }
        / font.inner.units_per_em().max(1.0);
    unsafe {
        ptr::write(out_line_height, (font.inner.ascent() - font.inner.descent() + font.inner.line_gap()) * scale);
        ptr::write(out_ascent, font.inner.ascent() * scale);
        ptr::write(out_descent, font.inner.descent() * scale);
    }
    STATUS_OK
}

/// Measures UTF-8 text, optionally wrapping at `max_width`.
#[no_mangle]
pub unsafe extern "C" fn miu_font_measure(
    font: *const MiuFont,
    text: *const c_char,
    length: usize,
    style: *const MiuTextStyle,
    has_max_width: u8,
    max_width: f32,
    out_size: *mut MiuVec2,
) -> u32 {
    let Some(font) = (unsafe { font.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if style.is_null() || out_size.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(text) = (unsafe { text_from(text, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let style = unsafe { style.as_ref() };
    let layout = layout_text(
        &font.inner,
        text,
        &style_from(style),
        if has_max_width != 0 { Some(max_width) } else { None },
    );
    unsafe { ptr::write(out_size, vec2_to(layout.size())) };
    STATUS_OK
}

/// Number of glyphs in the font.
#[no_mangle]
pub unsafe extern "C" fn miu_font_glyph_count(font: *const MiuFont) -> u32 {
    match unsafe { font.as_ref() } {
        Some(font) => u32::from(font.inner.glyph_count()),
        None => 0,
    }
}

/// Releases a font. Passing `NULL` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn miu_font_release(font: *mut MiuFont) {
    if font.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(font));
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// Creates a CPU renderer with the given framebuffer size and quality preset.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_create(
    width: u32,
    height: u32,
    preset: u8,
    out_renderer: *mut *mut MiuRenderer,
) -> u32 {
    if out_renderer.is_null() || width == 0 || height == 0 {
        return status_of(ErrorCode::InvalidArgument);
    }
    let preset = QualityPreset::from_u8(preset).unwrap_or(QualityPreset::Medium);
    match SoftwareRenderer::with_settings(width, height, QualitySettings::for_preset(preset)) {
        Ok(renderer) => {
            let handle = Box::into_raw(Box::new(MiuRenderer { inner: renderer }));
            unsafe { ptr::write(out_renderer, handle) };
            STATUS_OK
        }
        Err(error) => status_of(match error {
            miu_graphics::RenderError::Texture(miu_graphics::TextureError::OutOfMemory) => {
                ErrorCode::OutOfMemory
            }
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

/// Releases a renderer. Passing `NULL` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_release(renderer: *mut MiuRenderer) {
    if renderer.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(renderer));
    }
}

/// Reports the framebuffer size.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_size(
    renderer: *const MiuRenderer,
    out_width: *mut u32,
    out_height: *mut u32,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if out_width.is_null() || out_height.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    unsafe {
        ptr::write(out_width, renderer.inner.width());
        ptr::write(out_height, renderer.inner.height());
    }
    STATUS_OK
}

/// Resizes the framebuffer, discarding its contents.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_resize(
    renderer: *mut MiuRenderer,
    width: u32,
    height: u32,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match renderer.inner.resize(width, height) {
        Ok(()) => STATUS_OK,
        Err(_) => status_of(ErrorCode::InvalidArgument),
    }
}

/// Changes the quality preset, which also updates sampling and cache budgets.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_set_preset(renderer: *mut MiuRenderer, preset: u8) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(preset) = QualityPreset::from_u8(preset) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    renderer.inner.set_preset(preset);
    STATUS_OK
}

/// Copies a texture into the renderer's asset store under `id` (cloned).
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_upload_texture(
    renderer: *mut MiuRenderer,
    id: u64,
    texture: *const MiuTexture,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(texture) = (unsafe { texture.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    renderer
        .inner
        .assets_mut()
        .insert_texture(id, texture.inner.clone());
    STATUS_OK
}

/// Copies a font into the renderer's asset store under `id` (cloned).
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_upload_font(
    renderer: *mut MiuRenderer,
    id: u64,
    font: *const MiuFont,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(font) = (unsafe { font.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    renderer.inner.assets_mut().insert_font(id, font.inner.clone());
    STATUS_OK
}

/// Sets or clears the device-space clip rectangle.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_set_clip(
    renderer: *mut MiuRenderer,
    has_clip: u8,
    clip: MiuRect,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    renderer.inner.set_clip(if has_clip != 0 { Some(rect_from(clip)) } else { None });
    STATUS_OK
}

/// Clears the framebuffer and resets statistics.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_begin_frame(
    renderer: *mut MiuRenderer,
    has_clear: u8,
    clear: MiuRgba8,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let color = if has_clear != 0 { Some(rgba8_from(clear)) } else { None };
    match renderer.inner.begin_frame(color) {
        Ok(()) => STATUS_OK,
        Err(_) => status_of(ErrorCode::InvalidArgument),
    }
}

/// Draws one textured sprite through a camera.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_draw_sprite(
    renderer: *mut MiuRenderer,
    camera: *const MiuCamera2d,
    sprite: *const MiuSprite,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if camera.is_null() || sprite.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let camera = camera_from(unsafe { *camera });
    let mut list = DisplayList::new();
    list.push(DrawOp::Sprite(sprite_from(unsafe { &*sprite })));
    match renderer.inner.draw(&camera, &list) {
        Ok(()) => STATUS_OK,
        Err(error) => status_of(match error {
            miu_graphics::RenderError::MissingTexture(_) => ErrorCode::NotFound,
            miu_graphics::RenderError::Camera(_) => ErrorCode::Geometry,
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

/// Clears a rectangle with one colour, ignoring the camera (device space).
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_fill_rect(
    renderer: *mut MiuRenderer,
    camera: *const MiuCamera2d,
    rect: MiuRect,
    color: MiuColor,
    blend: u8,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let camera = match unsafe { camera.as_ref() } {
        Some(camera) => camera_from(*camera),
        None => return status_of(ErrorCode::InvalidArgument),
    };
    let mut list = DisplayList::new();
    list.push(DrawOp::Fill(FillOp {
        path: Path::rect(rect_from(rect)),
        transform: Affine2::IDENTITY,
        paint: Paint::solid(color_from(color)),
        rule: miu_graphics::FillRule::NonZero,
        blend: blend_from(blend),
        cull_bounds: None,
    }));
    match renderer.inner.draw(&camera, &list) {
        Ok(()) => STATUS_OK,
        Err(_) => status_of(ErrorCode::InvalidArgument),
    }
}

/// Draws UTF-8 text with an uploaded font.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_draw_text(
    renderer: *mut MiuRenderer,
    camera: *const MiuCamera2d,
    font_id: u64,
    text: *const c_char,
    length: usize,
    style: *const MiuTextStyle,
    color: MiuColor,
    transform: MiuTransform2d,
    blend: u8,
    has_max_width: u8,
    max_width: f32,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if camera.is_null() || style.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(text) = (unsafe { text_from(text, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let camera = camera_from(unsafe { *camera });
    let style = style_from(unsafe { &*style });
    let Some(font) = renderer.inner.assets().font(font_id) else {
        return status_of(ErrorCode::NotFound);
    };
    let layout = layout_text(font, text, &style, if has_max_width != 0 { Some(max_width) } else { None });
    let mut list = DisplayList::new();
    list.push(DrawOp::Text(TextRun {
        font: font_id,
        size: style.em_size(),
        color: color_from(color),
        blend: blend_from(blend),
        transform: transform_from(transform).to_affine(),
        glyphs: layout.glyphs,
        cull_bounds: None,
    }));
    match renderer.inner.draw(&camera, &list) {
        Ok(()) => STATUS_OK,
        Err(error) => status_of(match error {
            miu_graphics::RenderError::MissingFont(_) => ErrorCode::NotFound,
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

/// Copies the most recent frame's statistics.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_stats(
    renderer: *const MiuRenderer,
    out_stats: *mut MiuRenderStats,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let stats: RenderStats = renderer.inner.stats();
    unsafe {
        write_out(
            out_stats,
            MiuRenderStats {
                sprites: stats.sprites,
                fills: stats.fills,
                strokes: stats.strokes,
                glyphs: stats.glyphs,
                skipped: stats.skipped,
                uncached_glyphs: stats.uncached_glyphs,
                covered_pixels: stats.covered_pixels,
            },
        )
    }
}

/// Copies premultiplied RGBA8 framebuffer pixels into a caller buffer.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_read_pixels(
    renderer: *const MiuRenderer,
    buffer: *mut u8,
    capacity: usize,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if buffer.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let pixels = renderer.inner.canvas().pixels();
    if capacity < pixels.len() * 4 {
        return status_of(ErrorCode::LimitExceeded);
    }
    for (index, pixel) in pixels.iter().enumerate() {
        unsafe {
            *buffer.add(index * 4) = pixel.r;
            *buffer.add(index * 4 + 1) = pixel.g;
            *buffer.add(index * 4 + 2) = pixel.b;
            *buffer.add(index * 4 + 3) = pixel.a;
        }
    }
    STATUS_OK
}

/// Encodes the framebuffer as a PNG file.
#[no_mangle]
pub unsafe extern "C" fn miu_renderer_encode_png(
    renderer: *const MiuRenderer,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> u32 {
    let Some(renderer) = (unsafe { renderer.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if out_length.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let encoded = match renderer.inner.encode_png() {
        Ok(bytes) => bytes,
        Err(_) => return status_of(ErrorCode::Decode),
    };
    unsafe { *out_length = encoded.len() };
    if buffer.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    if capacity < encoded.len() {
        return status_of(ErrorCode::LimitExceeded);
    }
    unsafe { ptr::copy_nonoverlapping(encoded.as_ptr(), buffer, encoded.len()) };
    STATUS_OK
}

// ---------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------

/// Creates an empty scene with a camera framed to `viewport`.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_create(
    viewport_width: f32,
    viewport_height: f32,
    out_scene: *mut *mut MiuScene,
) -> u32 {
    if out_scene.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    match Scene::new(Vec2::new(viewport_width, viewport_height)) {
        Ok(scene) => {
            let handle = Box::into_raw(Box::new(MiuScene {
                scene,
                assets: Assets::new(),
                animations: Vec::new(),
            }));
            unsafe { ptr::write(out_scene, handle) };
            STATUS_OK
        }
        Err(_) => status_of(ErrorCode::Geometry),
    }
}

/// Releases a scene. Passing `NULL` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_release(scene: *mut MiuScene) {
    if scene.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(scene));
    }
}

/// Copies a scene's assets into a renderer so it can draw the scene.
///
/// Call this once after creating or changing a scene's resources; textures and fonts
/// are cloned into the renderer's stores.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_attach(
    scene: *const MiuScene,
    renderer: *mut MiuRenderer,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let target = renderer.inner.assets_mut();
    for (id, texture) in scene.assets.textures().iter() {
        target.insert_texture(*id, texture.clone());
    }
    for (id, font) in scene.assets.fonts().iter() {
        target.insert_font(*id, font.clone());
    }
    STATUS_OK
}

/// Sets the scene clear colour, or clears it with `has_background = 0`.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_background(
    scene: *mut MiuScene,
    has_background: u8,
    color: MiuRgba8,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    scene.scene.set_background(if has_background != 0 {
        Some(rgba8_from(color))
    } else {
        None
    });
    STATUS_OK
}

/// Sets the scene camera.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_camera(
    scene: *mut MiuScene,
    camera: *const MiuCamera2d,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(camera) = (unsafe { camera.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let converted = camera_from(*camera);
    if converted.validate().is_err() {
        return status_of(ErrorCode::Geometry);
    }
    *scene.scene.camera_mut() = converted;
    STATUS_OK
}

/// Adds a grouping node and returns its id.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_add_group(
    scene: *mut MiuScene,
    parent: u32,
    name: *const c_char,
    position: MiuVec2,
    out_id: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let name = unsafe { c_string(name) }.unwrap_or_else(|| "group".to_string());
    let node = Node::group(name).with_position(vec2_from(position));
    match scene.scene.create(node_parent(parent), node) {
        Ok(id) => unsafe { write_out(out_id, id) },
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Adds a sprite node and returns its id.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_add_sprite(
    scene: *mut MiuScene,
    parent: u32,
    name: *const c_char,
    texture_id: u64,
    size: MiuVec2,
    pivot: MiuVec2,
    position: MiuVec2,
    color: MiuColor,
    blend: u8,
    layer: i32,
    out_id: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let name = unsafe { c_string(name) }.unwrap_or_else(|| "sprite".to_string());
    let mut visual = SpriteVisual::new(texture_id);
    visual.size = vec2_from(size);
    visual.pivot = vec2_from(pivot);
    visual.color = color_from(color);
    visual.blend = blend_from(blend);
    let node = Node::sprite(name, visual)
        .with_position(vec2_from(position))
        .with_layer(layer);
    match scene.scene.create(node_parent(parent), node) {
        Ok(id) => unsafe { write_out(out_id, id) },
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Adds a text node and returns its id.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_add_text(
    scene: *mut MiuScene,
    parent: u32,
    name: *const c_char,
    font_id: u64,
    text: *const c_char,
    length: usize,
    style: *const MiuTextStyle,
    color: MiuColor,
    position: MiuVec2,
    layer: i32,
    out_id: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if style.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(text) = (unsafe { text_from(text, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let name = unsafe { c_string(name) }.unwrap_or_else(|| "text".to_string());
    let style = style_from(unsafe { &*style });
    let mut visual = TextVisual::new(font_id, text.to_string(), style.em_size());
    visual.style = style;
    visual.color = color_from(color);
    let node = Node::text(name, visual)
        .with_position(vec2_from(position))
        .with_layer(layer);
    match scene.scene.create(node_parent(parent), node) {
        Ok(id) => unsafe { write_out(out_id, id) },
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Adds a filled rectangle node and returns its id.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_add_rect(
    scene: *mut MiuScene,
    parent: u32,
    name: *const c_char,
    rect: MiuRect,
    color: MiuColor,
    layer: i32,
    out_id: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let name = unsafe { c_string(name) }.unwrap_or_else(|| "rect".to_string());
    let visual = ShapeVisual::new(Path::rect(rect_from(rect)), Paint::solid(color_from(color)));
    let node = Node::shape(name, visual).with_layer(layer);
    match scene.scene.create(node_parent(parent), node) {
        Ok(id) => unsafe { write_out(out_id, id) },
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Replaces a node's local transform.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_transform(
    scene: *mut MiuScene,
    node: u32,
    transform: *const MiuTransform2d,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(transform) = (unsafe { transform.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match scene.scene.set_transform(node, transform_from(*transform)) {
        Ok(()) => STATUS_OK,
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Shows or hides a node and its subtree.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_visible(
    scene: *mut MiuScene,
    node: u32,
    visible: u8,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match scene.scene.node_mut(node) {
        Some(target) => {
            target.visible = visible != 0;
            STATUS_OK
        }
        None => status_of(ErrorCode::NotFound),
    }
}

/// Sets a node's opacity.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_opacity(
    scene: *mut MiuScene,
    node: u32,
    opacity: f32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if !opacity.is_finite() {
        return status_of(ErrorCode::InvalidArgument);
    }
    match scene.scene.node_mut(node) {
        Some(target) => {
            target.opacity = opacity.clamp(0.0, 1.0);
            STATUS_OK
        }
        None => status_of(ErrorCode::NotFound),
    }
}

/// Sets a node's painter layer.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_set_layer(
    scene: *mut MiuScene,
    node: u32,
    layer: i32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match scene.scene.node_mut(node) {
        Some(target) => {
            target.layer = layer;
            STATUS_OK
        }
        None => status_of(ErrorCode::NotFound),
    }
}

/// Removes a node and its subtree.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_remove(scene: *mut MiuScene, node: u32) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match scene.scene.remove(node) {
        Ok(()) => STATUS_OK,
        Err(error) => status_of(error_code_scene(error)),
    }
}

/// Reports the number of live nodes.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_node_count(
    scene: *const MiuScene,
    out_count: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    unsafe { write_out(out_count, scene.scene.node_count() as u32) }
}

/// Finds the topmost node containing a world-space point.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_hit_test(
    scene: *const MiuScene,
    world: MiuVec2,
    out_id: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match scene.scene.hit_test(&scene.assets, vec2_from(world)) {
        Some(id) => unsafe { write_out(out_id, id) },
        None => status_of(ErrorCode::NotFound),
    }
}

/// Reports how many animations the scene carries.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_animation_count(
    scene: *const MiuScene,
    out_count: *mut u32,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let count = scene.animations.len().min(u32::MAX as usize) as u32;
    unsafe { write_out(out_count, count) }
}

/// Applies every animation at an absolute time in seconds.
///
/// Animations are independent: each one loops or clamps on its own duration, and
/// each track writes a single node property. Call this once per frame before
/// `miu_scene_draw`; calling it twice with the same time is a no-op because the
/// tracks are absolute, not additive.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_update(scene: *mut MiuScene, time_seconds: f32) -> u32 {
    let Some(scene) = (unsafe { scene.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if !time_seconds.is_finite() {
        return status_of(ErrorCode::InvalidArgument);
    }
    for animation in &scene.animations {
        animation.apply_to(&mut scene.scene, time_seconds);
    }
    STATUS_OK
}

/// Draws a scene into a renderer using the scene's own camera and assets.
#[no_mangle]
pub unsafe extern "C" fn miu_scene_draw(
    scene: *const MiuScene,
    renderer: *mut MiuRenderer,
) -> u32 {
    let Some(scene) = (unsafe { scene.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(renderer) = (unsafe { renderer.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let list = scene.scene.build(&scene.assets);
    let camera = *scene.scene.camera();
    if let Err(error) = renderer.inner.begin_frame(scene.scene.background()) {
        let _ = error;
        return status_of(ErrorCode::InvalidState);
    }
    match renderer.inner.draw(&camera, &list) {
        Ok(()) => STATUS_OK,
        Err(error) => status_of(match error {
            miu_graphics::RenderError::MissingTexture(_) | miu_graphics::RenderError::MissingFont(_) => {
                ErrorCode::NotFound
            }
            miu_graphics::RenderError::Camera(_) => ErrorCode::Geometry,
            _ => ErrorCode::InvalidArgument,
        }),
    }
}

fn node_parent(parent: u32) -> NodeId {
    parent
}

unsafe fn c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe { core::ffi::CStr::from_ptr(value) }
        .to_str()
        .ok()
        .map(|text| text.to_string())
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// Parses a JSON scene document.
#[no_mangle]
pub unsafe extern "C" fn miu_document_parse(
    bytes: *const u8,
    length: usize,
    out_document: *mut *mut MiuDocument,
) -> u32 {
    if out_document.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let Some(data) = (unsafe { bytes_from(bytes, length) }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Ok(text) = core::str::from_utf8(data) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match Document::parse(text) {
        Ok(document) => {
            let handle = Box::into_raw(Box::new(MiuDocument {
                document,
                textures: HashMap::new(),
                fonts: HashMap::new(),
            }));
            unsafe { ptr::write(out_document, handle) };
            STATUS_OK
        }
        Err(_) => status_of(ErrorCode::Parse),
    }
}

/// Releases a document. Passing `NULL` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn miu_document_release(document: *mut MiuDocument) {
    if document.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(document));
    }
}

/// Reports the document viewport size.
#[no_mangle]
pub unsafe extern "C" fn miu_document_viewport(
    document: *const MiuDocument,
    out_size: *mut MiuVec2,
) -> u32 {
    let Some(document) = (unsafe { document.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    unsafe { write_out(out_size, vec2_to(document.document.viewport)) }
}

/// Number of assets the document declares.
#[no_mangle]
pub unsafe extern "C" fn miu_document_asset_count(
    document: *const MiuDocument,
    out_count: *mut u32,
) -> u32 {
    let Some(document) = (unsafe { document.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    unsafe { write_out(out_count, document.document.assets.len() as u32) }
}

/// Copies the path of the asset at `index` into a caller buffer.
#[no_mangle]
pub unsafe extern "C" fn miu_document_asset_path(
    document: *const MiuDocument,
    index: u32,
    buffer: *mut c_char,
    capacity: usize,
    out_length: *mut usize,
) -> u32 {
    let Some(document) = (unsafe { document.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let Some(asset) = document.document.assets.get(index as usize) else {
        return status_of(ErrorCode::NotFound);
    };
    unsafe { write_text(&asset.path, buffer, capacity, out_length) }
}

/// Reports whether an asset is a texture or a font.
#[no_mangle]
pub unsafe extern "C" fn miu_document_asset_kind(
    document: *const MiuDocument,
    index: u32,
    out_kind: *mut u32,
) -> u32 {
    let Some(document) = (unsafe { document.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    match document.document.assets.get(index as usize) {
        Some(asset) => unsafe { write_out(out_kind, asset.kind as u32) },
        None => status_of(ErrorCode::NotFound),
    }
}

/// Supplies a decoded texture for the asset declared at `path`.
#[no_mangle]
pub unsafe extern "C" fn miu_document_set_texture(
    document: *mut MiuDocument,
    path: *const c_char,
    texture: *const MiuTexture,
) -> u32 {
    let Some(document) = (unsafe { document.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let (Some(path), Some(texture)) = (unsafe { c_string(path) }, unsafe { texture.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    document.textures.insert(path, texture.inner.clone());
    STATUS_OK
}

/// Supplies a parsed font for the asset declared at `path`.
#[no_mangle]
pub unsafe extern "C" fn miu_document_set_font(
    document: *mut MiuDocument,
    path: *const c_char,
    font: *const MiuFont,
) -> u32 {
    let Some(document) = (unsafe { document.as_mut() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    let (Some(path), Some(font)) = (unsafe { c_string(path) }, unsafe { font.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    document.fonts.insert(path, font.inner.clone());
    STATUS_OK
}

/// Builds a scene from a document and the supplied assets.
#[no_mangle]
pub unsafe extern "C" fn miu_document_instantiate(
    document: *const MiuDocument,
    out_scene: *mut *mut MiuScene,
) -> u32 {
    let Some(document) = (unsafe { document.as_ref() }) else {
        return status_of(ErrorCode::InvalidArgument);
    };
    if out_scene.is_null() {
        return status_of(ErrorCode::InvalidArgument);
    }
    let instantiated = match document.document.instantiate_with(|asset| {
        match asset.kind {
            miu_scene::AssetKind::Texture => document
                .textures
                .get(&asset.path)
                .cloned()
                .map(ResolvedAsset::Texture)
                .ok_or_else(|| miu_scene::DocumentError::MissingAsset(asset.path.clone())),
            miu_scene::AssetKind::Font => document
                .fonts
                .get(&asset.path)
                .cloned()
                .map(ResolvedAsset::Font)
                .ok_or_else(|| miu_scene::DocumentError::MissingAsset(asset.path.clone())),
        }
    }) {
        Ok(instantiated) => instantiated,
        Err(miu_scene::DocumentError::MissingAsset(_)) => return status_of(ErrorCode::NotFound),
        Err(_) => return status_of(ErrorCode::Parse),
    };
    let handle = Box::into_raw(Box::new(MiuScene {
        scene: instantiated.scene,
        assets: instantiated.assets,
        animations: instantiated.animations,
    }));
    unsafe { ptr::write(out_scene, handle) };
    STATUS_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_exported() {
        assert_eq!(miu_abi_version(), MIU_ABI_VERSION);
        assert!(miu_abi_min_supported_version() <= miu_abi_version());
        assert!(!miu_version_string().is_null());
    }

    #[test]
    fn selection_requires_a_compliant_backend() {
        let backends = [miu_core::Backend::Vulkan11 as u8];
        let request = MiuRenderRequest {
            minimum_tier: RenderTier::Compatibility as u8,
            preferred_tier: RenderTier::Compatibility as u8,
            preset: QualityPreset::Medium as u8,
            best_effort: 0,
        };
        let mut plan = MiuRenderPlan::default();
        let status = unsafe {
            miu_select_renderer(
                Platform::Linux as u8,
                backends.as_ptr(),
                backends.len(),
                &request,
                &mut plan,
            )
        };
        assert_eq!(status, STATUS_OK);
        assert_eq!(plan.backend, miu_core::Backend::Vulkan11 as u8);

        let missing = unsafe { miu_select_renderer(Platform::Windows as u8, ptr::null(), 0, &request, &mut plan) };
        assert_eq!(missing, status_of(ErrorCode::Unsupported));
    }

    #[test]
    fn textures_round_trip_through_the_abi() {
        let pixels = [255u8, 0, 0, 255];
        let mut texture: *mut MiuTexture = ptr::null_mut();
        let status = unsafe { miu_texture_create(1, 1, pixels.as_ptr(), &mut texture) };
        assert_eq!(status, STATUS_OK);
        let mut width = 0u32;
        let mut height = 0u32;
        assert_eq!(unsafe { miu_texture_size(texture, &mut width, &mut height) }, STATUS_OK);
        assert_eq!((width, height), (1, 1));
        let mut buffer = [0u8; 4];
        assert_eq!(unsafe { miu_texture_read(texture, buffer.as_mut_ptr(), 4) }, STATUS_OK);
        assert_eq!(buffer, pixels);
        unsafe { miu_texture_release(texture) };
    }

    #[test]
    fn renderer_draws_a_filled_rectangle() {
        let mut renderer: *mut MiuRenderer = ptr::null_mut();
        assert_eq!(
            unsafe { miu_renderer_create(8, 8, QualityPreset::Low as u8, &mut renderer) },
            STATUS_OK
        );
        let camera = MiuCamera2d {
            position: MiuVec2::default(),
            viewport: MiuVec2 { x: 8.0, y: 8.0 },
            zoom: 1.0,
            rotation_radians: 0.0,
        };
        let rect = MiuRect { x: 0.0, y: 0.0, width: 8.0, height: 8.0 };
        let color = MiuColor { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        assert_eq!(
            unsafe { miu_renderer_fill_rect(renderer, &camera, rect, color, 0) },
            STATUS_OK
        );
        let mut pixels = [0u8; 8 * 8 * 4];
        assert_eq!(
            unsafe { miu_renderer_read_pixels(renderer, pixels.as_mut_ptr(), pixels.len()) },
            STATUS_OK
        );
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
        unsafe { miu_renderer_release(renderer) };
    }

    #[test]
    fn scenes_create_nodes_and_reject_bad_ids() {
        let mut scene: *mut MiuScene = ptr::null_mut();
        assert_eq!(unsafe { miu_scene_create(64.0, 64.0, &mut scene) }, STATUS_OK);
        let mut id = 0u32;
        let name = b"panel\0";
        assert_eq!(
            unsafe {
                miu_scene_add_group(
                    scene,
                    ROOT,
                    name.as_ptr() as *const c_char,
                    MiuVec2 { x: 4.0, y: 4.0 },
                    &mut id,
                )
            },
            STATUS_OK
        );
        assert_eq!(unsafe { miu_scene_remove(scene, 99) }, status_of(ErrorCode::NotFound));
        let mut count = 0u32;
        assert_eq!(unsafe { miu_scene_node_count(scene, &mut count) }, STATUS_OK);
        assert_eq!(count, 2);
        unsafe { miu_scene_release(scene) };
    }
}

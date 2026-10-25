#ifndef MIU_MIU_H
#define MIU_MIU_H

/*
 * Miu's stable C ABI.
 *
 * This file deliberately uses fixed-width integer fields instead of C enums in
 * ABI-bearing structures. That makes its layout identical for C, C++, Rust, and
 * C# P/Invoke consumers. Unknown values are rejected with MIU_STATUS_INVALID_ARGUMENT.
 */

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32) && defined(MIU_BUILD_SHARED)
#  if defined(MIU_FFI_EXPORTS)
#    define MIU_API __declspec(dllexport)
#  else
#    define MIU_API __declspec(dllimport)
#  endif
#elif defined(__GNUC__) || defined(__clang__)
#  define MIU_API __attribute__((visibility("default")))
#else
#  define MIU_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

/** Current ABI revision requested by newly compiled bindings. */
#define MIU_ABI_VERSION UINT32_C(2)
/** Oldest ABI revision whose legacy entry points remain available. */
#define MIU_ABI_MIN_SUPPORTED_VERSION UINT32_C(1)

/** Values accepted by miu_device_capabilities::platform. */
typedef uint8_t miu_platform_t;
enum {
    MIU_PLATFORM_WINDOWS = 1,
    MIU_PLATFORM_ANDROID = 2,
    MIU_PLATFORM_MACOS = 3,
    MIU_PLATFORM_IOS = 4,
    MIU_PLATFORM_LINUX = 5,
    MIU_PLATFORM_WEB = 6
};

/** Values accepted in a capability backend list and returned in a render plan. */
typedef uint8_t miu_backend_t;
enum {
    MIU_BACKEND_DIRECT2D_1_2 = 1,
    MIU_BACKEND_DIRECT3D_11_3 = 2,
    MIU_BACKEND_DIRECT3D_12 = 3,
    MIU_BACKEND_OPENGL_ES_3_0 = 4,
    MIU_BACKEND_OPENGL_ES_2_0 = 5,
    MIU_BACKEND_METAL_2 = 6,
    MIU_BACKEND_METAL_3 = 7,
    MIU_BACKEND_METAL_1 = 8,
    MIU_BACKEND_VULKAN_1_1 = 9,
    MIU_BACKEND_VULKAN_1_2 = 10,
    MIU_BACKEND_VULKAN_1_3 = 11,
    MIU_BACKEND_VULKAN_1_4 = 12,
    MIU_BACKEND_OPENGL_3_3 = 13,
    MIU_BACKEND_OPENGL_3_0 = 14,
    MIU_BACKEND_WEBGL_2 = 15,
    MIU_BACKEND_WEBGL_1 = 16,
    MIU_BACKEND_WEBGPU = 17
};

/** Values accepted by miu_renderer_request tier fields. */
typedef uint8_t miu_render_tier_t;
enum {
    MIU_RENDER_TIER_COMPATIBILITY = 1,
    MIU_RENDER_TIER_BALANCED = 2,
    MIU_RENDER_TIER_PREMIUM = 3
};

/** Function result. No API in this header throws C++ exceptions. */
typedef uint32_t miu_status_t;
enum {
    MIU_STATUS_OK = 0,
    MIU_STATUS_INVALID_ARGUMENT = 1,
    MIU_STATUS_UNSUPPORTED = 2,
    MIU_STATUS_OUT_OF_MEMORY = 3
};

/** Platform capabilities produced by a probe bridge.
 *
 * `backends` may be NULL only when `backend_count` is zero. The pointed-to list
 * need only remain alive for the duration of the call receiving this structure.
 */
typedef struct miu_device_capabilities {
    uint32_t abi_version;
    miu_platform_t platform;
    uint8_t reserved[3];
    const miu_backend_t* backends;
    size_t backend_count;
} miu_device_capabilities;

/** Project renderer policy.
 *
 * `preferred_tier` is attempted before stronger tiers; it must not be below
 * `minimum_tier`. Set `allow_best_effort` only after warning the end user that
 * their device does not meet the declared minimum and rendering may fail or be degraded.
 */
typedef struct miu_renderer_request {
    uint32_t abi_version;
    miu_render_tier_t minimum_tier;
    miu_render_tier_t preferred_tier;
    uint8_t allow_best_effort;
    uint8_t reserved;
} miu_renderer_request;

/**
 * ABI v1 selection result. This layout and `miu_renderer_get_plan` are retained
 * for binaries compiled against v1. New code must use `miu_render_plan_v2` and
 * `miu_renderer_get_plan_v2`; do not reinterpret one as the other.
 */
typedef struct miu_render_plan {
    miu_backend_t backend;
    miu_render_tier_t tier;
    uint8_t emergency_fallback;
    uint8_t below_minimum;
} miu_render_plan;

/** ABI v2 selection result with both the actual and catalog profile API. */
typedef struct miu_render_plan_v2 {
    miu_backend_t backend;
    miu_backend_t profile_backend;
    miu_render_tier_t tier;
    uint8_t emergency_fallback;
    uint8_t below_minimum;
} miu_render_plan_v2;

/** Opaque policy renderer handle. */
typedef struct miu_renderer miu_renderer;

/** Returns the newest ABI revision compiled into the linked library. */
MIU_API uint32_t miu_abi_version(void);

/** Returns the oldest ABI revision whose legacy entry points are supported. */
MIU_API uint32_t miu_abi_min_supported_version(void);

/**
 * Selects an API profile and creates an opaque renderer-policy handle.
 *
 * The caller owns a successful `out_renderer` and must eventually call
 * `miu_renderer_release`. The handle represents policy only in v0.1; a native
 * bridge is responsible for creating the corresponding graphics device.
 */
MIU_API miu_status_t miu_renderer_create(
    const miu_device_capabilities* capabilities,
    const miu_renderer_request* request,
    miu_renderer** out_renderer);

/** Copies an ABI v1 projection of a selected plan for existing v1 binaries. */
MIU_API miu_status_t miu_renderer_get_plan(
    const miu_renderer* renderer,
    miu_render_plan* out_plan);

/** Copies the complete ABI v2 selected plan. */
MIU_API miu_status_t miu_renderer_get_plan_v2(
    const miu_renderer* renderer,
    miu_render_plan_v2* out_plan);

/** Releases a renderer-policy handle. It is safe to pass NULL. */
MIU_API void miu_renderer_release(miu_renderer* renderer);

/** A premultiplied RGBA colour used by Miu's CPU reference renderer. */
typedef struct miu_rgba8 {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} miu_rgba8;

/** A float two-dimensional point or vector. */
typedef struct miu_vec2 {
    float x;
    float y;
} miu_vec2;

/** Premultiplied linear float tint colour. */
typedef struct miu_color {
    float r;
    float g;
    float b;
    float a;
} miu_color;

/** A 2D translation, clockwise rotation in radians, and per-axis scale. */
typedef struct miu_transform2d {
    miu_vec2 translation;
    float rotation_radians;
    miu_vec2 scale;
} miu_transform2d;

/** Normalised source rectangle. Reversed bounds intentionally mirror an axis. */
typedef struct miu_uv_rect {
    float left;
    float top;
    float right;
    float bottom;
} miu_uv_rect;

/** Camera transform for the CPU renderer's logical top-left-origin canvas. */
typedef struct miu_camera2d {
    miu_vec2 position;
    miu_vec2 viewport;
    float zoom;
    float rotation_radians;
} miu_camera2d;

/** Pixel source rectangle in a texture atlas. A zero width and height select the full texture. */
typedef struct miu_source_rect {
    uint32_t x;
    uint32_t y;
    uint32_t width;
    uint32_t height;
} miu_source_rect;

/**
 * ABI v1 sprite descriptor retained for old binaries and the unsuffixed draw
 * function. Its geometry uses the complete texture and centres its pivot.
 */
typedef struct miu_cpu_sprite {
    miu_transform2d transform;
    miu_uv_rect uv;
    miu_color color;
    int32_t layer;
} miu_cpu_sprite;

/**
 * ABI v2 sprite descriptor. Translation identifies `pivot` in world space.
 * `source` selects an atlas rectangle; `(0, 0, 0, 0)` alone selects the whole
 * texture. `size == (0, 0)` uses source dimensions. Reverse UV bounds mirror.
 */
typedef struct miu_cpu_sprite_v2 {
    miu_transform2d transform;
    miu_source_rect source;
    miu_uv_rect uv;
    miu_vec2 size;
    miu_vec2 pivot;
    miu_color color;
    int32_t layer;
} miu_cpu_sprite_v2;

/** Opaque CPU texture. Input texture pixels and canvas pixels are premultiplied RGBA8. */
typedef struct miu_cpu_texture miu_cpu_texture;
/** Opaque mutable CPU canvas. */
typedef struct miu_cpu_canvas miu_cpu_canvas;

/**
 * Creates a CPU texture from row-major premultiplied RGBA8 bytes.
 *
 * `rgba_bytes` must have exactly `width * height * 4` readable bytes. On success,
 * the caller owns `*out_texture` and must release it with `miu_cpu_texture_release`.
 */
MIU_API miu_status_t miu_cpu_texture_create_rgba8(
    uint32_t width,
    uint32_t height,
    const uint8_t* rgba_bytes,
    size_t rgba_byte_count,
    miu_cpu_texture** out_texture);


/**
 * Converts row-major straight-alpha RGBA8 bytes to Miu's premultiplied storage.
 * The input-size and ownership rules are identical to
 * `miu_cpu_texture_create_rgba8`.
 */
MIU_API miu_status_t miu_cpu_texture_create_straight_rgba8(
    uint32_t width,
    uint32_t height,
    const uint8_t* rgba_bytes,
    size_t rgba_byte_count,
    miu_cpu_texture** out_texture);

/** Releases a CPU texture. It is safe to pass NULL. */
MIU_API void miu_cpu_texture_release(miu_cpu_texture* texture);

/** Allocates a transparent CPU canvas. */
MIU_API miu_status_t miu_cpu_canvas_create(
    uint32_t width,
    uint32_t height,
    miu_cpu_canvas** out_canvas);

/** Releases a CPU canvas. It is safe to pass NULL. */
MIU_API void miu_cpu_canvas_release(miu_cpu_canvas* canvas);

/** Fills a CPU canvas with a premultiplied colour; invalid straight-alpha values are rejected. */
MIU_API miu_status_t miu_cpu_canvas_clear(miu_cpu_canvas* canvas, miu_rgba8 color);

/** Draws an ABI v1 sprite with nearest-neighbour source-over blending. */
MIU_API miu_status_t miu_cpu_canvas_draw_sprite(
    miu_cpu_canvas* canvas,
    const miu_camera2d* camera,
    const miu_cpu_sprite* sprite,
    const miu_cpu_texture* texture);

/** Draws an ABI v2 sprite with atlas/source-size/pivot support. */
MIU_API miu_status_t miu_cpu_canvas_draw_sprite_v2(
    miu_cpu_canvas* canvas,
    const miu_camera2d* camera,
    const miu_cpu_sprite_v2* sprite,
    const miu_cpu_texture* texture);

/**
 * Copies the canvas to row-major premultiplied RGBA8 bytes.
 *
 * `out_rgba_bytes` must provide at least `width * height * 4` writable bytes.
 */
MIU_API miu_status_t miu_cpu_canvas_copy_rgba8(
    const miu_cpu_canvas* canvas,
    uint8_t* out_rgba_bytes,
    size_t out_rgba_byte_count);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIU_MIU_H */

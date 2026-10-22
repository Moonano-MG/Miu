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

/** Increment only for an incompatible ABI change. */
#define MIU_ABI_VERSION UINT32_C(1)

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
 * Set `allow_best_effort` only after warning the end user that their device does
 * not meet the declared minimum and rendering may fail or be degraded.
 */
typedef struct miu_renderer_request {
    uint32_t abi_version;
    miu_render_tier_t minimum_tier;
    miu_render_tier_t preferred_tier;
    uint8_t allow_best_effort;
    uint8_t reserved;
} miu_renderer_request;

/** Immutable selection result held by a renderer handle. */
typedef struct miu_render_plan {
    miu_backend_t backend;
    miu_render_tier_t tier;
    uint8_t emergency_fallback;
    uint8_t below_minimum;
} miu_render_plan;

/** Opaque policy renderer handle. */
typedef struct miu_renderer miu_renderer;

/** Returns the ABI version compiled into the linked library. */
MIU_API uint32_t miu_abi_version(void);

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

/** Copies the immutable selected plan from a renderer handle. */
MIU_API miu_status_t miu_renderer_get_plan(
    const miu_renderer* renderer,
    miu_render_plan* out_plan);

/** Releases a renderer-policy handle. It is safe to pass NULL. */
MIU_API void miu_renderer_release(miu_renderer* renderer);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIU_MIU_H */

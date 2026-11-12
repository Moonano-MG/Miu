/*
 * Miu 2D rendering engine — C ABI (version 3).
 *
 * This header is the single supported binary boundary of the engine. It mirrors
 * `crates/miu-ffi/src/lib.rs` one-to-one: every struct field order, every
 * discriminant, and every function signature is part of the ABI. Bindings for C++,
 * C#, and JavaScript are generated from, or checked against, this file.
 *
 * Conventions
 * -----------
 * * Every function returns `miu_status_t` unless it returns a value directly.
 *   `MIU_STATUS_OK` is zero, so `if (status)` in C means "failed".
 * * Out-parameters are only written when the call succeeds, with the documented
 *   exception of the two-call buffer pattern: `miu_renderer_encode_png`,
 *   `miu_document_asset_path`, and every other length-reporting call always write
 *   `out_length` so a caller can size its buffer first.
 * * Opaque handles are owned by the caller and must be released with their
 *   matching `miu_*_release` function. Releasing `NULL` is a no-op.
 * * Byte buffers are copied on the way in, so the caller may free its data
 *   immediately. Text is UTF-8 and is passed with an explicit length; it never has
 *   to be NUL terminated.
 * * Pixels are premultiplied RGBA8 unless a function name says `straight`.
 * * No function is thread-safe for the same handle. Different handles may be used
 *   from different threads.
 * * Angles are radians. Document files authored for the editor use degrees; the
 *   loader converts them.
 */

#ifndef MIU_MIU_H
#define MIU_MIU_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------------- */
/* Version                                                                   */
/* ------------------------------------------------------------------------- */

/** ABI version implemented by this header and the library it describes. */
#define MIU_ABI_VERSION 3u

/** Oldest ABI version the library accepts at load time. */
#define MIU_ABI_MIN_VERSION 3u

/** Engine name, without a terminating NUL, for window titles and logs. */
#define MIU_ENGINE_NAME "Miu"

/* ------------------------------------------------------------------------- */
/* Status codes                                                              */
/* ------------------------------------------------------------------------- */

/**
 * Failure categories shared by every subsystem.
 *
 * The numeric values are part of the ABI. New codes may be appended; existing
 * codes are never renumbered.
 */
typedef enum miu_status {
    MIU_STATUS_OK = 0,
    MIU_STATUS_INVALID_ARGUMENT = 1,
    MIU_STATUS_UNSUPPORTED = 2,
    MIU_STATUS_OUT_OF_MEMORY = 3,
    MIU_STATUS_NOT_FOUND = 4,
    MIU_STATUS_IO = 5,
    MIU_STATUS_DECODE = 6,
    MIU_STATUS_PARSE = 7,
    MIU_STATUS_INVALID_STATE = 8,
    MIU_STATUS_LIMIT_EXCEEDED = 9,
    MIU_STATUS_FONT = 10,
    MIU_STATUS_GEOMETRY = 11,
    MIU_STATUS_ABI_MISMATCH = 12
} miu_status_t;

/* ------------------------------------------------------------------------- */
/* Enumerations                                                              */
/* ------------------------------------------------------------------------- */

/** Operating system or browser family that owns the device. */
typedef enum miu_platform {
    MIU_PLATFORM_WINDOWS = 1,
    MIU_PLATFORM_ANDROID = 2,
    MIU_PLATFORM_MACOS = 3,
    MIU_PLATFORM_IOS = 4,
    MIU_PLATFORM_LINUX = 5,
    MIU_PLATFORM_WEB = 6
} miu_platform_t;

/** Graphics API profile. */
typedef enum miu_backend {
    MIU_BACKEND_DIRECT2D_1_2 = 1,
    MIU_BACKEND_DIRECT3D_11_3 = 2,
    MIU_BACKEND_DIRECT3D_12 = 3,
    MIU_BACKEND_OPENGLES_3_0 = 4,
    MIU_BACKEND_OPENGLES_2_0 = 5,
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
} miu_backend_t;

/** Feature tier. Higher tiers may use more capable APIs and presets. */
typedef enum miu_tier {
    MIU_TIER_COMPATIBILITY = 1,
    MIU_TIER_BALANCED = 2,
    MIU_TIER_PREMIUM = 3
} miu_tier_t;

/** Quality preset: 90/10, 70/30, 40/60, and 10/90 performance-to-quality. */
typedef enum miu_preset {
    MIU_PRESET_LOW = 1,
    MIU_PRESET_MEDIUM = 2,
    MIU_PRESET_HIGH = 3,
    MIU_PRESET_UNLIMITED = 4
} miu_preset_t;

/** Compositing mode applied to sprites, fills, and text. */
typedef enum miu_blend {
    MIU_BLEND_SOURCE_OVER = 0,
    MIU_BLEND_ADD = 1,
    MIU_BLEND_MULTIPLY = 2,
    MIU_BLEND_SCREEN = 3,
    MIU_BLEND_COPY = 4
} miu_blend_t;

/** Horizontal alignment of laid-out text lines. */
typedef enum miu_text_align {
    MIU_TEXT_ALIGN_LEFT = 0,
    MIU_TEXT_ALIGN_CENTER = 1,
    MIU_TEXT_ALIGN_RIGHT = 2
} miu_text_align_t;

/** Line-breaking strategy. */
typedef enum miu_text_wrap {
    MIU_TEXT_WRAP_NONE = 0,
    MIU_TEXT_WRAP_WORD = 1,
    MIU_TEXT_WRAP_CHARACTER = 2
} miu_text_wrap_t;

/** Asset category reported by `miu_document_asset_kind`. */
typedef enum miu_asset_kind {
    MIU_ASSET_KIND_TEXTURE = 0,
    MIU_ASSET_KIND_FONT = 1
} miu_asset_kind_t;

/** Bit 0: the plan is an emergency fallback. Bit 1: below the declared minimum. */
#define MIU_PLAN_FLAG_EMERGENCY 1u
#define MIU_PLAN_FLAG_BELOW_MINIMUM 2u

/* ------------------------------------------------------------------------- */
/* Plain data structures                                                     */
/* ------------------------------------------------------------------------- */

/** Two-dimensional vector in logical pixels or world units. */
typedef struct miu_vec2 {
    float x;
    float y;
} miu_vec2_t;

/** Straight-alpha colour with components in `[0, 1]`. */
typedef struct miu_color {
    float r;
    float g;
    float b;
    float a;
} miu_color_t;

/** Premultiplied 8-bit texel. */
typedef struct miu_rgba8 {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} miu_rgba8_t;

/** Axis-aligned rectangle; `width` and `height` are never negative. */
typedef struct miu_rect {
    float x;
    float y;
    float width;
    float height;
} miu_rect_t;

/** Translation, rotation, and scale applied around a node's local origin. */
typedef struct miu_transform2d {
    miu_vec2_t translation;
    float rotation_radians;
    miu_vec2_t scale;
} miu_transform2d_t;

/** Independent 2D camera. `viewport` is the target size in pixels. */
typedef struct miu_camera2d {
    miu_vec2_t position;
    miu_vec2_t viewport;
    float zoom;
    float rotation_radians;
} miu_camera2d_t;

/**
 * Atlas source rectangle in texels.
 *
 * A source rectangle with all fields zero means "the whole texture", which is the
 * same sentinel the Rust API uses.
 */
typedef struct miu_source_rect {
    uint32_t x;
    uint32_t y;
    uint32_t width;
    uint32_t height;
} miu_source_rect_t;

/** One sprite to draw. */
typedef struct miu_sprite {
    /** Texture asset id registered with the renderer. */
    uint64_t texture;
    /** Local-to-world transform. */
    miu_transform2d_t transform;
    /** Atlas rectangle; all zero means the whole texture. */
    miu_source_rect_t source;
    /** Normalised sampling rectangle: `{u0, v0, u1, v1}`. */
    float uv[4];
    /** Destination size; zero means the source size. */
    miu_vec2_t size;
    /** Normalised origin inside the destination rectangle. */
    miu_vec2_t pivot;
    /** Premultiplied tint. */
    miu_color_t color;
    /** Painter layer; lower layers draw first. */
    int32_t layer;
    /** One of `miu_blend_t`. */
    uint8_t blend;
} miu_sprite_t;

/** Text layout parameters. */
typedef struct miu_text_style {
    /** Em size in pixels. */
    float font_size;
    /** Multiplier applied to the font's natural line height. */
    float line_height;
    /** Extra spacing between glyphs in pixels. */
    float letter_spacing;
    /** One of `miu_text_align_t`. */
    uint8_t align;
    /** One of `miu_text_wrap_t`. */
    uint8_t wrap;
    /** Reserved; must be zero. */
    uint8_t reserved[2];
} miu_text_style_t;

/** Renderer selection request declared by a project. */
typedef struct miu_render_request {
    /** Lowest tier the project supports. */
    uint8_t minimum_tier;
    /** Tier tried first. */
    uint8_t preferred_tier;
    /** One of `miu_preset_t`. */
    uint8_t preset;
    /** Non-zero selects the best-effort path, which may plan an emergency API. */
    uint8_t best_effort;
} miu_render_request_t;

/** Selected render plan returned by `miu_select_renderer`. */
typedef struct miu_render_plan {
    /** API the bridge must create. */
    uint8_t backend;
    /** API profile that implied the tier. */
    uint8_t profile_backend;
    /** Selected tier. */
    uint8_t tier;
    /** Preset after clamping to the tier. */
    uint8_t preset;
    /** `MIU_PLAN_FLAG_*` bits. */
    uint32_t flags;
} miu_render_plan_t;

/** Per-frame renderer counters, reset by `miu_renderer_begin_frame`. */
typedef struct miu_render_stats {
    uint32_t sprites;
    uint32_t fills;
    uint32_t strokes;
    uint32_t glyphs;
    uint32_t skipped;
    uint32_t uncached_glyphs;
    uint64_t covered_pixels;
} miu_render_stats_t;

/* ------------------------------------------------------------------------- */
/* Opaque handles                                                            */
/* ------------------------------------------------------------------------- */

/** CPU texture. */
typedef struct miu_texture miu_texture_t;
/** Parsed TrueType font. */
typedef struct miu_font miu_font_t;
/** Software renderer and its asset stores. */
typedef struct miu_renderer miu_renderer_t;
/** Scene graph and its decoded assets. */
typedef struct miu_scene miu_scene_t;
/** Parsed document plus the assets supplied for it. */
typedef struct miu_document miu_document_t;

/* ------------------------------------------------------------------------- */
/* Defaults                                                                  */
/* ------------------------------------------------------------------------- */

/** Identity transform: no translation, no rotation, unit scale. */
static inline miu_transform2d_t miu_transform2d_identity(void) {
    miu_transform2d_t value;
    value.translation.x = 0.0f;
    value.translation.y = 0.0f;
    value.rotation_radians = 0.0f;
    value.scale.x = 1.0f;
    value.scale.y = 1.0f;
    return value;
}

/** Camera framed to a `1x1` viewport at unit zoom. */
static inline miu_camera2d_t miu_camera2d_default(void) {
    miu_camera2d_t value;
    value.position.x = 0.0f;
    value.position.y = 0.0f;
    value.viewport.x = 1.0f;
    value.viewport.y = 1.0f;
    value.zoom = 1.0f;
    value.rotation_radians = 0.0f;
    return value;
}

/** Sprite defaults: identity transform, whole texture, white tint. */
static inline miu_sprite_t miu_sprite_default(void) {
    miu_sprite_t value;
    value.texture = 0;
    value.transform = miu_transform2d_identity();
    value.source.x = 0;
    value.source.y = 0;
    value.source.width = 0;
    value.source.height = 0;
    value.uv[0] = 0.0f;
    value.uv[1] = 0.0f;
    value.uv[2] = 1.0f;
    value.uv[3] = 1.0f;
    value.size.x = 0.0f;
    value.size.y = 0.0f;
    value.pivot.x = 0.0f;
    value.pivot.y = 0.0f;
    value.color.r = 1.0f;
    value.color.g = 1.0f;
    value.color.b = 1.0f;
    value.color.a = 1.0f;
    value.layer = 0;
    value.blend = MIU_BLEND_SOURCE_OVER;
    return value;
}

/** Text style defaults: 16 px, 1.2 line height, left aligned, word wrapped. */
static inline miu_text_style_t miu_text_style_default(void) {
    miu_text_style_t value;
    value.font_size = 16.0f;
    value.line_height = 1.2f;
    value.letter_spacing = 0.0f;
    value.align = MIU_TEXT_ALIGN_LEFT;
    value.wrap = MIU_TEXT_WRAP_WORD;
    value.reserved[0] = 0;
    value.reserved[1] = 0;
    return value;
}

/* ------------------------------------------------------------------------- */
/* Diagnostics                                                               */
/* ------------------------------------------------------------------------- */

/** Returns the ABI version of the loaded library. */
uint32_t miu_abi_version(void);

/** Returns the oldest ABI version the loaded library accepts. */
uint32_t miu_abi_min_supported_version(void);

/** Returns a NUL-terminated version string such as `0.1.0-dev`; never freed. */
const char *miu_version_string(void);

/** Returns a NUL-terminated English description of a status code. */
const char *miu_status_message(uint32_t status);

/** Returns the identifier of a platform (`windows`, `linux`, ...) or `NULL`. */
const char *miu_platform_name(uint8_t platform);

/** Returns the identifier of a backend (`vulkan-1.2`, ...) or `NULL`. */
const char *miu_backend_name(uint8_t backend);

/* ------------------------------------------------------------------------- */
/* Render plan selection                                                     */
/* ------------------------------------------------------------------------- */

/**
 * Chooses a graphics API for a device.
 *
 * `backends` lists the APIs the bridge probed successfully; pass `NULL` with
 * `backend_count == 0` to use the platform's full catalogue for the reported
 * platform. Miu always evaluates the host platform first, so a browser running on
 * Windows can report `MIU_PLATFORM_WINDOWS` and get the Direct3D catalogue.
 *
 * With `request->best_effort == 0` the call never drops below
 * `request->minimum_tier`: escalation walks upward only, and
 * `MIU_STATUS_UNSUPPORTED` means no catalogue entry met the request. With
 * `best_effort != 0` the call may return a plan flagged
 * `MIU_PLAN_FLAG_BELOW_MINIMUM` and/or `MIU_PLAN_FLAG_EMERGENCY`; pass such a plan
 * to `miu_plan_warning` and show the result to the player before running.
 */
miu_status_t miu_select_renderer(uint8_t platform,
                                 const uint8_t *backends,
                                 size_t backend_count,
                                 const miu_render_request_t *request,
                                 miu_render_plan_t *out_plan);

/**
 * Returns the warning a host must show before running on this plan, or `NULL`
 * when the plan meets the project's declared minimum without an emergency API.
 * The returned string is owned by the library and stays valid until the next call
 * that returns a string.
 */
const char *miu_plan_warning(const miu_render_plan_t *plan);

/* ------------------------------------------------------------------------- */
/* Textures                                                                  */
/* ------------------------------------------------------------------------- */

/**
 * Creates a texture from premultiplied RGBA8 pixels (`width * height * 4` bytes).
 *
 * Channels with `rgb > alpha` are rejected with `MIU_STATUS_INVALID_ARGUMENT`
 * because such a pixel cannot be premultiplied; use
 * `miu_texture_create_straight` for artwork authored with straight alpha.
 */
miu_status_t miu_texture_create(uint32_t width,
                                uint32_t height,
                                const uint8_t *pixels,
                                miu_texture_t **out_texture);

/** Creates a texture from straight-alpha RGBA8 pixels, premultiplying them. */
miu_status_t miu_texture_create_straight(uint32_t width,
                                         uint32_t height,
                                         const uint8_t *pixels,
                                         miu_texture_t **out_texture);

/** Decodes a PNG, BMP, TGA, or QOI image into a texture (format sniffed). */
miu_status_t miu_texture_load(const uint8_t *bytes,
                              size_t length,
                              miu_texture_t **out_texture);

/** Reports a texture's size in pixels. */
miu_status_t miu_texture_size(const miu_texture_t *texture,
                              uint32_t *out_width,
                              uint32_t *out_height);

/** Copies premultiplied RGBA8 pixels out of a texture; needs `capacity >= w*h*4`. */
miu_status_t miu_texture_read(const miu_texture_t *texture,
                              uint8_t *buffer,
                              size_t capacity);

/** Releases a texture. Passing `NULL` is a no-op. */
void miu_texture_release(miu_texture_t *texture);

/* ------------------------------------------------------------------------- */
/* Fonts                                                                     */
/* ------------------------------------------------------------------------- */

/** Parses a TrueType font from memory. The bytes may be freed afterwards. */
miu_status_t miu_font_load(const uint8_t *bytes, size_t length, miu_font_t **out_font);

/**
 * Reports metrics scaled to `font_size` pixels: line height including the gap,
 * ascent, and descent (positive values, measured downward).
 */
miu_status_t miu_font_metrics(const miu_font_t *font,
                              float font_size,
                              float *out_line_height,
                              float *out_ascent,
                              float *out_descent);

/**
 * Lays text out and reports its block size.
 *
 * `has_max_width != 0` enables wrapping at `max_width` pixels. The reported size
 * includes the last line's descent, so stacking two measured blocks leaves no
 * overlap.
 */
miu_status_t miu_font_measure(const miu_font_t *font,
                              const char *text,
                              size_t length,
                              const miu_text_style_t *style,
                              uint8_t has_max_width,
                              float max_width,
                              miu_vec2_t *out_size);

/** Number of glyphs the font maps. */
uint32_t miu_font_glyph_count(const miu_font_t *font);

/** Releases a font. Passing `NULL` is a no-op. */
void miu_font_release(miu_font_t *font);

/* ------------------------------------------------------------------------- */
/* Renderer                                                                  */
/* ------------------------------------------------------------------------- */

/** Creates a CPU renderer with a framebuffer of `width x height` pixels. */
miu_status_t miu_renderer_create(uint32_t width,
                                 uint32_t height,
                                 uint8_t preset,
                                 miu_renderer_t **out_renderer);

/** Releases a renderer. Passing `NULL` is a no-op. */
void miu_renderer_release(miu_renderer_t *renderer);

/** Reports the framebuffer size in pixels. */
miu_status_t miu_renderer_size(const miu_renderer_t *renderer,
                               uint32_t *out_width,
                               uint32_t *out_height);

/** Resizes the framebuffer, discarding its contents. */
miu_status_t miu_renderer_resize(miu_renderer_t *renderer, uint32_t width, uint32_t height);

/**
 * Changes the quality preset.
 *
 * The preset selects sampling mode, gradient dithering, and the glyph cache budget;
 * existing textures stay uploaded.
 */
miu_status_t miu_renderer_set_preset(miu_renderer_t *renderer, uint8_t preset);

/** Copies a texture into the renderer's store under `id`. */
miu_status_t miu_renderer_upload_texture(miu_renderer_t *renderer,
                                         uint64_t id,
                                         const miu_texture_t *texture);

/** Copies a font into the renderer's store under `id`. */
miu_status_t miu_renderer_upload_font(miu_renderer_t *renderer,
                                      uint64_t id,
                                      const miu_font_t *font);

/** Sets or clears (`has_clip == 0`) the scissor rectangle in framebuffer pixels. */
miu_status_t miu_renderer_set_clip(miu_renderer_t *renderer,
                                   uint8_t has_clip,
                                   miu_rect_t clip);

/** Starts a frame: resets statistics, applies the clip, and optionally clears. */
miu_status_t miu_renderer_begin_frame(miu_renderer_t *renderer,
                                      uint8_t has_clear,
                                      miu_rgba8_t clear);

/**
 * Draws one sprite.
 *
 * The sprite's transform maps its local space into world space; the camera maps
 * world space into the framebuffer. Sprites outside the clip rectangle are culled
 * and counted in `skipped`.
 */
miu_status_t miu_renderer_draw_sprite(miu_renderer_t *renderer,
                                      const miu_camera2d_t *camera,
                                      const miu_sprite_t *sprite);

/** Fills an axis-aligned rectangle in world space. */
miu_status_t miu_renderer_fill_rect(miu_renderer_t *renderer,
                                    const miu_camera2d_t *camera,
                                    miu_rect_t rect,
                                    miu_color_t color,
                                    uint8_t blend);

/**
 * Draws UTF-8 text.
 *
 * The transform positions the text block in world space: text starts at the
 * transform origin and flows right and down. Rotated text takes the exact outline
 * path instead of the cached-mask fast path, which costs more per glyph.
 */
miu_status_t miu_renderer_draw_text(miu_renderer_t *renderer,
                                    const miu_camera2d_t *camera,
                                    uint64_t font_id,
                                    const char *text,
                                    size_t length,
                                    const miu_text_style_t *style,
                                    miu_color_t color,
                                    miu_transform2d_t transform,
                                    uint8_t blend,
                                    uint8_t has_max_width,
                                    float max_width);

/** Copies the statistics accumulated since `miu_renderer_begin_frame`. */
miu_status_t miu_renderer_stats(const miu_renderer_t *renderer, miu_render_stats_t *out_stats);

/** Copies premultiplied RGBA8 framebuffer pixels; needs `capacity >= w*h*4`. */
miu_status_t miu_renderer_read_pixels(const miu_renderer_t *renderer,
                                      uint8_t *buffer,
                                      size_t capacity);

/**
 * Encodes the framebuffer as PNG.
 *
 * `out_length` always receives the encoded size. When `capacity` is smaller the
 * call returns `MIU_STATUS_LIMIT_EXCEEDED` without writing pixels, so a caller can
 * size its buffer with one probing call.
 */
miu_status_t miu_renderer_encode_png(const miu_renderer_t *renderer,
                                     uint8_t *buffer,
                                     size_t capacity,
                                     size_t *out_length);

/* ------------------------------------------------------------------------- */
/* Scenes                                                                    */
/* ------------------------------------------------------------------------- */

/** Creates an empty scene whose camera is framed to `viewport_width x viewport_height`. */
miu_status_t miu_scene_create(float viewport_width,
                              float viewport_height,
                              miu_scene_t **out_scene);

/** Releases a scene. Passing `NULL` is a no-op. */
void miu_scene_release(miu_scene_t *scene);

/**
 * Copies a scene's decoded assets into a renderer.
 *
 * Call this once after creating or loading a scene; textures and fonts are cloned
 * into the renderer's stores, so later changes to the scene assets are not
 * mirrored.
 */
miu_status_t miu_scene_attach(const miu_scene_t *scene, miu_renderer_t *renderer);

/** Sets the clear colour, or clears it with `has_background == 0`. */
miu_status_t miu_scene_set_background(miu_scene_t *scene,
                                      uint8_t has_background,
                                      miu_rgba8_t color);

/** Replaces the scene camera; invalid values return `MIU_STATUS_GEOMETRY`. */
miu_status_t miu_scene_set_camera(miu_scene_t *scene, const miu_camera2d_t *camera);

/** Adds a grouping node; `parent` is `0` for the scene root. */
miu_status_t miu_scene_add_group(miu_scene_t *scene,
                                 uint32_t parent,
                                 const char *name,
                                 miu_vec2_t position,
                                 uint32_t *out_id);

/** Adds a sprite node. `size` of zero draws the texture at its natural size. */
miu_status_t miu_scene_add_sprite(miu_scene_t *scene,
                                  uint32_t parent,
                                  const char *name,
                                  uint64_t texture_id,
                                  miu_vec2_t size,
                                  miu_vec2_t pivot,
                                  miu_vec2_t position,
                                  miu_color_t color,
                                  uint8_t blend,
                                  int32_t layer,
                                  uint32_t *out_id);

/** Adds a text node; layout happens when the scene is drawn. */
miu_status_t miu_scene_add_text(miu_scene_t *scene,
                                uint32_t parent,
                                const char *name,
                                uint64_t font_id,
                                const char *text,
                                size_t length,
                                const miu_text_style_t *style,
                                miu_color_t color,
                                miu_vec2_t position,
                                int32_t layer,
                                uint32_t *out_id);

/** Adds a filled rectangle node. */
miu_status_t miu_scene_add_rect(miu_scene_t *scene,
                                uint32_t parent,
                                const char *name,
                                miu_rect_t rect,
                                miu_color_t color,
                                int32_t layer,
                                uint32_t *out_id);

/** Replaces a node's local transform. */
miu_status_t miu_scene_set_transform(miu_scene_t *scene,
                                     uint32_t node,
                                     const miu_transform2d_t *transform);

/** Shows or hides a node and its subtree. */
miu_status_t miu_scene_set_visible(miu_scene_t *scene, uint32_t node, uint8_t visible);

/** Sets a node's opacity in `[0, 1]`; values are clamped. */
miu_status_t miu_scene_set_opacity(miu_scene_t *scene, uint32_t node, float opacity);

/** Sets a node's painter layer; children are re-sorted on the next draw. */
miu_status_t miu_scene_set_layer(miu_scene_t *scene, uint32_t node, int32_t layer);

/** Removes a node and its subtree, freeing their ids. */
miu_status_t miu_scene_remove(miu_scene_t *scene, uint32_t node);

/** Reports how many nodes the scene holds, including the root. */
miu_status_t miu_scene_node_count(const miu_scene_t *scene, uint32_t *out_count);

/** Finds the topmost node containing a world-space point. */
miu_status_t miu_scene_hit_test(const miu_scene_t *scene, miu_vec2_t world, uint32_t *out_id);

/** Reports how many animations the scene carries. */
miu_status_t miu_scene_animation_count(const miu_scene_t *scene, uint32_t *out_count);

/**
 * Applies every animation at an absolute time in seconds.
 *
 * Each animation loops or clamps on its own duration, and each track writes a
 * single node property, so repeated calls with the same time are idempotent. Call
 * this once per frame before `miu_scene_draw`.
 */
miu_status_t miu_scene_update(miu_scene_t *scene, float time_seconds);

/** Draws a scene into a renderer with the scene's camera, background, and assets. */
miu_status_t miu_scene_draw(const miu_scene_t *scene, miu_renderer_t *renderer);

/* ------------------------------------------------------------------------- */
/* Documents                                                                 */
/* ------------------------------------------------------------------------- */

/**
 * Parses a JSON scene document.
 *
 * Parsing validates structure and resolves asset ids, but decodes nothing. Supply
 * the referenced files with `miu_document_set_texture` / `miu_document_set_font`
 * before calling `miu_document_instantiate`.
 */
miu_status_t miu_document_parse(const uint8_t *bytes,
                                size_t length,
                                miu_document_t **out_document);

/** Releases a document. Passing `NULL` is a no-op. */
void miu_document_release(miu_document_t *document);

/** Reports the document's authored viewport size. */
miu_status_t miu_document_viewport(const miu_document_t *document, miu_vec2_t *out_size);

/** Number of assets the document declares. */
miu_status_t miu_document_asset_count(const miu_document_t *document, uint32_t *out_count);

/**
 * Copies the path of asset `index` into `buffer` as NUL-terminated UTF-8.
 *
 * `out_length` always receives the byte length excluding the NUL, so a caller can
 * size its buffer with one probing call.
 */
miu_status_t miu_document_asset_path(const miu_document_t *document,
                                     uint32_t index,
                                     char *buffer,
                                     size_t capacity,
                                     size_t *out_length);

/** Reports the kind of asset `index` as a `miu_asset_kind_t` value. */
miu_status_t miu_document_asset_kind(const miu_document_t *document,
                                     uint32_t index,
                                     uint32_t *out_kind);

/** Supplies a decoded texture for the asset declared at `path`. */
miu_status_t miu_document_set_texture(miu_document_t *document,
                                      const char *path,
                                      const miu_texture_t *texture);

/** Supplies a parsed font for the asset declared at `path`. */
miu_status_t miu_document_set_font(miu_document_t *document,
                                   const char *path,
                                   const miu_font_t *font);

/**
 * Builds a scene from the document and its supplied assets.
 *
 * Every declared asset must have been supplied: a missing one returns
 * `MIU_STATUS_NOT_FOUND` naming nothing in particular, which is why
 * `miu_document_asset_path` exists — call it first to report the gap to the user.
 */
miu_status_t miu_document_instantiate(const miu_document_t *document,
                                      miu_scene_t **out_scene);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIU_MIU_H */

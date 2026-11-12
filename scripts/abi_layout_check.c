/*
 * Compile-time ABI layout check.
 *
 * `miu.h` must stay binary compatible with the Rust structs in
 * `crates/miu-ffi/src/lib.rs`, because those are shared with C, C#, and JavaScript
 * hosts. This translation unit pins the exact size and field offset of every
 * `#[repr(C)]` structure: if a field moves, the build fails instead of producing a
 * library that silently misreads memory at runtime.
 *
 * Build:  cc -std=c11 -Iinclude scripts/abi_layout_check.c -o /dev/null
 */

#include <stddef.h>

#include "miu/miu.h"

#define MIU_CHECK_SIZE(type, expected) \
    _Static_assert(sizeof(type) == (expected), #type " size changed; bump MIU_ABI_VERSION")

#define MIU_CHECK_OFFSET(type, field, expected) \
    _Static_assert(offsetof(type, field) == (expected), #type "." #field " moved; bump MIU_ABI_VERSION")

/* Value types. */
MIU_CHECK_SIZE(miu_vec2_t, 8);
MIU_CHECK_OFFSET(miu_vec2_t, y, 4);

MIU_CHECK_SIZE(miu_color_t, 16);
MIU_CHECK_OFFSET(miu_color_t, a, 12);

MIU_CHECK_SIZE(miu_rgba8_t, 4);
MIU_CHECK_OFFSET(miu_rgba8_t, a, 3);

MIU_CHECK_SIZE(miu_rect_t, 16);
MIU_CHECK_OFFSET(miu_rect_t, height, 12);

MIU_CHECK_SIZE(miu_transform2d_t, 20);
MIU_CHECK_OFFSET(miu_transform2d_t, rotation_radians, 8);
MIU_CHECK_OFFSET(miu_transform2d_t, scale, 12);

MIU_CHECK_SIZE(miu_camera2d_t, 24);
MIU_CHECK_OFFSET(miu_camera2d_t, viewport, 8);
MIU_CHECK_OFFSET(miu_camera2d_t, zoom, 16);
MIU_CHECK_OFFSET(miu_camera2d_t, rotation_radians, 20);

MIU_CHECK_SIZE(miu_source_rect_t, 16);
MIU_CHECK_OFFSET(miu_source_rect_t, height, 12);

/* Sprite layout: `uv` and `color` are the fields v3 callers rely on most. */
MIU_CHECK_SIZE(miu_sprite_t, 104);
MIU_CHECK_OFFSET(miu_sprite_t, texture, 0);
MIU_CHECK_OFFSET(miu_sprite_t, transform, 8);
MIU_CHECK_OFFSET(miu_sprite_t, source, 28);
MIU_CHECK_OFFSET(miu_sprite_t, uv, 44);
MIU_CHECK_OFFSET(miu_sprite_t, size, 60);
MIU_CHECK_OFFSET(miu_sprite_t, pivot, 68);
MIU_CHECK_OFFSET(miu_sprite_t, color, 76);
MIU_CHECK_OFFSET(miu_sprite_t, layer, 92);
MIU_CHECK_OFFSET(miu_sprite_t, blend, 96);

/* Requests and results. */
MIU_CHECK_SIZE(miu_text_style_t, 16);
MIU_CHECK_OFFSET(miu_text_style_t, align, 12);
MIU_CHECK_OFFSET(miu_text_style_t, wrap, 13);

MIU_CHECK_SIZE(miu_render_request_t, 4);
MIU_CHECK_OFFSET(miu_render_request_t, best_effort, 3);

MIU_CHECK_SIZE(miu_render_plan_t, 8);
MIU_CHECK_OFFSET(miu_render_plan_t, flags, 4);

MIU_CHECK_SIZE(miu_render_stats_t, 32);
MIU_CHECK_OFFSET(miu_render_stats_t, covered_pixels, 24);

/* Discriminants are ABI surface too. */
_Static_assert(MIU_STATUS_OK == 0, "MIU_STATUS_OK must stay zero");
_Static_assert(MIU_STATUS_ABI_MISMATCH == 12, "status codes are append-only");
_Static_assert(MIU_BACKEND_WEBGPU == 17, "backend discriminants are append-only");
_Static_assert(MIU_TIER_PREMIUM == 3, "tier discriminants are append-only");
_Static_assert(MIU_PRESET_UNLIMITED == 4, "preset discriminants are append-only");
_Static_assert(MIU_BLEND_COPY == 4, "blend discriminants are append-only");
_Static_assert(MIU_TEXT_WRAP_CHARACTER == 2, "text wrap discriminants are append-only");
_Static_assert(MIU_ABI_VERSION == 3, "this check documents ABI 3");

int main(void) {
    return 0;
}

# C ABI version 3

`include/miu/miu.h` is the contract. `crates/miu-ffi` implements it and
`bindings/csharp/Miu.cs`, `include/miu/miu.hpp`, and `native/` consume it.
`scripts/abi_layout_check.c` pins every struct size and field offset at compile time,
so an accidental field reordering fails the build instead of corrupting memory in a
shipped game.

## Versioning rules

* `MIU_ABI_VERSION` is bumped whenever a struct layout changes or a function changes
  meaning. Adding a function or a new enum value does not require a bump.
* `MIU_ABI_MIN_VERSION` is the oldest ABI the library still serves. Bindings check it
  at load time and call `miu_abi_version()` to compare.
* Structures are append-only. A field that must change is added as a new function
  taking a `_v2` structure; the old function keeps working.
* Enum values are never renumbered. `MIU_STATUS_*`, `MIU_BACKEND_*`, and friends are
  all append-only.

## Status codes

| Value | Name | Meaning |
| --- | --- | --- |
| 0 | `MIU_STATUS_OK` | success |
| 1 | `MIU_STATUS_INVALID_ARGUMENT` | null, out of range, or inconsistent argument |
| 2 | `MIU_STATUS_UNSUPPORTED` | device, backend, or format cannot do this |
| 3 | `MIU_STATUS_OUT_OF_MEMORY` | allocation failed |
| 4 | `MIU_STATUS_NOT_FOUND` | resource does not exist |
| 5 | `MIU_STATUS_IO` | file or stream error |
| 6 | `MIU_STATUS_DECODE` | image could not be decoded |
| 7 | `MIU_STATUS_PARSE` | document could not be parsed |
| 8 | `MIU_STATUS_INVALID_STATE` | object is in the wrong state |
| 9 | `MIU_STATUS_LIMIT_EXCEEDED` | configured limit exceeded |
| 10 | `MIU_STATUS_FONT` | font could not be parsed |
| 11 | `MIU_STATUS_GEOMETRY` | transform or camera not finite/invertible |
| 12 | `MIU_STATUS_ABI_MISMATCH` | binding and library disagree |

## Conventions

* Functions return `miu_status_t` (`uint32_t`) unless they return data directly;
  `miu_status_message` turns a code into English text.
* Out-parameters are written only on success, except the documented two-call pattern:
  `miu_renderer_encode_png`, `miu_document_asset_path`, and `miu_font_measure`'s size
  query always report the required length/geometry so the caller can size a buffer.
* Buffers are copied on the way in. Text is UTF-8 with an explicit length and is never
  NUL-terminated at the call site; asset paths are NUL-terminated because they are
  looked up by string.
* Opaque handles belong to the caller. Releasing `NULL` is a no-op, and a handle must
  not be used from two threads at once.
* Panics abort: the release profile uses `panic = "abort"`, so the ABI never returns a
  half-initialised handle.

## Surface map

| Group | Functions |
| --- | --- |
| Diagnostics | `miu_abi_version`, `miu_abi_min_supported_version`, `miu_version_string`, `miu_status_message`, `miu_platform_name`, `miu_backend_name` |
| Policy | `miu_select_renderer`, `miu_plan_warning` |
| Textures | `miu_texture_create`, `miu_texture_create_straight`, `miu_texture_load`, `miu_texture_size`, `miu_texture_read`, `miu_texture_release` |
| Fonts | `miu_font_load`, `miu_font_metrics`, `miu_font_measure`, `miu_font_glyph_count`, `miu_font_release` |
| Renderer | `miu_renderer_create`, `miu_renderer_release`, `miu_renderer_size`, `miu_renderer_resize`, `miu_renderer_set_preset`, `miu_renderer_upload_texture`, `miu_renderer_upload_font`, `miu_renderer_set_clip`, `miu_renderer_begin_frame`, `miu_renderer_draw_sprite`, `miu_renderer_fill_rect`, `miu_renderer_draw_text`, `miu_renderer_stats`, `miu_renderer_read_pixels`, `miu_renderer_encode_png` |
| Scenes | `miu_scene_create`, `miu_scene_release`, `miu_scene_attach`, `miu_scene_set_background`, `miu_scene_set_camera`, `miu_scene_add_group`, `miu_scene_add_sprite`, `miu_scene_add_text`, `miu_scene_add_rect`, `miu_scene_set_transform`, `miu_scene_set_visible`, `miu_scene_set_opacity`, `miu_scene_set_layer`, `miu_scene_remove`, `miu_scene_node_count`, `miu_scene_animation_count`, `miu_scene_update`, `miu_scene_hit_test`, `miu_scene_draw` |
| Documents | `miu_document_parse`, `miu_document_release`, `miu_document_viewport`, `miu_document_asset_count`, `miu_document_asset_path`, `miu_document_asset_kind`, `miu_document_set_texture`, `miu_document_set_font`, `miu_document_instantiate` |

## Minimal C++ host

```cpp
#include "miu/miu.hpp"

miu::Renderer renderer(1280, 720, miu::Preset::High);
miu::Camera camera = miu::Camera::ForViewport(1280.0f, 720.0f);

renderer.begin_frame(miu::Rgba8{12, 14, 18, 255});
renderer.fill_rect(camera, miu::Rect(40, 40, 200, 80), miu::colors::Rgb8(90, 180, 255));
std::vector<std::uint8_t> png = renderer.encode_png();
```

## Render plan selection

```cpp
miu::Plan plan = miu::Plan::Select(miu::Platform::Linux,
                                   {miu::Backend::Vulkan12, miu::Backend::Vulkan13},
                                   miu::Tier::Compatibility, miu::Tier::Balanced,
                                   miu::Preset::High);
if (!plan.warning().empty()) {
    show_dialog(plan.warning());   // plan is below the minimum or emergency
}
```

`backends` is the list the platform probe created successfully. An empty list means
"trust the catalogue" and is what a bridge uses before it has probed anything.

## Layout facts pinned by `scripts/abi_layout_check.c`

| Structure | Size | Notable offsets |
| --- | --- | --- |
| `miu_vec2_t` | 8 | `y` 4 |
| `miu_color_t` | 16 | `a` 12 |
| `miu_rgba8_t` | 4 | `a` 3 |
| `miu_rect_t` | 16 | `height` 12 |
| `miu_transform2d_t` | 20 | `rotation_radians` 8, `scale` 12 |
| `miu_camera2d_t` | 24 | `viewport` 8, `zoom` 16, `rotation_radians` 20 |
| `miu_source_rect_t` | 16 | `height` 12 |
| `miu_sprite_t` | 104 | `transform` 8, `source` 28, `uv` 44, `size` 60, `pivot` 68, `color` 76, `layer` 92, `blend` 96 |
| `miu_text_style_t` | 16 | `align` 12, `wrap` 13 |
| `miu_render_request_t` | 4 | `best_effort` 3 |
| `miu_render_plan_t` | 8 | `flags` 4 |
| `miu_render_stats_t` | 32 | `covered_pixels` 24 |

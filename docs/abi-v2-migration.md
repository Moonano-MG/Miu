# ABI v2 migration and binary compatibility

ABI v2 adds source rectangles, destination size/pivot, and the catalog profile
backend to the public C surface. These additions must not cause an old binary to
have a shorter structure read as a new structure. Consequently, v2 does **not**
reuse the ABI v1 descriptor/function pairs.

## Runtime handshake

Before using any v2 descriptor or suffixed API, a native host must verify:

```c
miu_abi_min_supported_version() <= MIU_ABI_VERSION
    && miu_abi_version() >= MIU_ABI_VERSION
```

`miu::Renderer`/`miu::CpuCanvas` and the C# binding execute this check before
calling a v2 entry point. They also check the v2 layouts they marshal. A failed
handshake is reported as `MIU_STATUS_UNSUPPORTED` / `Status.Unsupported`; it is
never an invitation to pass a v2 structure to a v1 function.

## What remains ABI v1

Existing binaries may retain their old header declarations and use:

| ABI v1 descriptor/function | v2-safe behaviour in a current library |
|---|---|
| `miu_renderer_create` with ABI version `1` inputs | Accepted: v1 and v2 policy input layouts are identical. |
| `miu_render_plan` + `miu_renderer_get_plan` | Still exactly four bytes; returns the v1 projection without `profile_backend`. |
| `miu_cpu_sprite` + `miu_cpu_canvas_draw_sprite` | Still exactly 56 bytes; draws the full texture at legacy source-size geometry with a centred pivot. |

The v1 symbols remain intentionally unsuffixed. Do not modify their field order,
size, alignment, or parameter types in a future release.

## Required v2 substitutions

Newly compiled C code should use these pairs together:

| ABI v1 | ABI v2 |
|---|---|
| `miu_render_plan`, `miu_renderer_get_plan` | `miu_render_plan_v2`, `miu_renderer_get_plan_v2` |
| `miu_cpu_sprite`, `miu_cpu_canvas_draw_sprite` | `miu_cpu_sprite_v2`, `miu_cpu_canvas_draw_sprite_v2` |

`miu_cpu_sprite_v2` supports `source`, `size`, and `pivot`. Its full-texture
sentinel is exactly `{ x: 0, y: 0, width: 0, height: 0 }`; nonzero `x` or `y`
with zero width and height is rejected. New code must use the v2 function, even
when it happens to use legacy geometry values.

## Alpha and atlas contract

All normal CPU texture input, canvas clear input, and copied canvas output is
premultiplied RGBA8. `miu_cpu_texture_create_rgba8` rejects invalid RGB greater
than alpha. For conventional straight-alpha assets, call
`miu_cpu_texture_create_straight_rgba8`, which converts once at upload. The v2
rasterizer clamps UV samples to the selected source rectangle, preventing atlas
bleed; reversed bounds still mirror sampling.

## Validation

Run `./scripts/verify-manually.sh` on a supported development machine. Its C ABI
layout probe asserts the legacy/v2 structure sizes, and the Rust tests cover the
legacy conversion path, source sentinel, alpha conversion, selector input
validation, and rotated-camera mapping. This is intentionally a manual command:
the project does not use GitHub Actions for automatic compilation.

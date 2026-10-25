# Miu v0.1 architecture

## Ownership boundaries

```text
C# / C++ application
       │ public C# P/Invoke or C++17 RAII wrapper
       ▼
include/miu/miu.h  ────────── stable C ABI ─────────► crates/miu-ffi
                                                              │
                                                              ▼
                                                crates/miu-core (Rust)
                                      selection policy · Camera2D · CPU batches
                                                              │ selected plan
                                                              ▼
native/ C++ bridge ◄── host-specific probes / API adapters ──► GPU API
```

Rust does not own window handles or expose platform SDK handles across the ABI.
It can therefore be tested identically on every target. The C++ bridge owns
platform headers, C++ library compatibility adapters, and the actual
hardware-graphics-device lifetime. The ABI preserves a selected `RenderPlan` and
also exposes a deterministic CPU texture/canvas renderer. Future hardware command
encoders will be added behind this same boundary rather than leaking backend
objects.

## Selection sequence

1. The host creates its window/surface and probes APIs it can genuinely initialise.
2. It reports only successful probes as `DeviceCapabilities` / `CapabilityReport`.
3. The application supplies a minimum tier and an optional preferred tier.
4. `select_renderer` first tries the requested preferred tier (when it is at or
   above the minimum), choosing its strongest profile, and only then escalates to
   stronger tiers. Invalid requests are rejected by strict and best-effort paths.
   An unavailable result includes an ordered attempted list in Rust for diagnostics.
5. Only after user confirmation, `select_best_effort` may choose a lower profile or
   an emergency fallback. The returned plan is marked so the warning cannot be
   lost.
6. The C++ bridge creates the device for the selected backend. If that fails, remove
   the failed API from the capability report and select again. Do not silently
   change APIs beneath the application.

This separates a cheap, deterministic policy decision from API/device failures
that are necessarily platform-specific.

## Frame preparation

`Camera2D` maps world points to a logical top-left-origin viewport and has exact
inverse mapping for picking. It validates viewport, zoom, and rotation before a
transform is made.

`BatchBuilder` owns a single frame's sprites. Use `BatchMode::Stable` for alpha
content: it orders by layer and preserves submission order within a layer. Use
`BatchMode::Opaque` only where overlap ordering is irrelevant; it sorts texture
state within each layer and emits fewer `DrawBatch` ranges. A backend encoder
binds the batch texture once and writes the contiguous instances in each range.

`Canvas`, `Texture`, and `TextureStore` implement that same command stream with a
deterministic CPU backend. It supports nearest-neighbour UV sampling constrained to
the selected source rectangle, sprite translation/rotation/scale including
mirroring, premultiplied source-over blending, and a validated camera inverse.
Texture input and clear colours are rejected unless premultiplied; explicit
straight-alpha conversion helpers are provided. The C ABI, C++ wrapper, and C#
binding expose CPU texture/canvas lifetime and drawing calls for headless test
tools and previews.

## Compatibility contract

The C ABI uses only fixed-width scalar fields and opaque pointers. ABI v1
structures and unsuffixed functions remain binary-safe: `miu_render_plan` stays
four bytes and `miu_cpu_canvas_draw_sprite` consumes the original 56-byte sprite
descriptor. ABI v2 additions use `miu_render_plan_v2`,
`miu_renderer_get_plan_v2`, `miu_cpu_sprite_v2`, and
`miu_cpu_canvas_draw_sprite_v2`; they are never interpreted through v1 pointers.

Call `miu_abi_version()` and `miu_abi_min_supported_version()` before using v2.
The C++ and C# wrappers do this at runtime and also guard their v2 descriptor
sizes. The selector accepts the unchanged v1 policy input layout for legacy
callers, while new bindings send ABI v2 and use only suffixed v2 output calls.
The complete source/binary migration rules are in
[ABI v2 migration](abi-v2-migration.md).

Manual validation is deliberately tracked in `scripts/verify-manually.sh` rather
than GitHub Actions; it compiles ABI layout guards, C/C++ public surfaces, Rust,
and (when installed) the C# binding.

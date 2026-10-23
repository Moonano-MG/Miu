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

Rust must not own window handles or expose platform SDK handles across the ABI. It
can therefore be tested identically on every target. The C++ bridge owns platform
headers, C++ library compatibility adapters, and the actual graphics-device
lifetime. The ABI currently preserves a selected `RenderPlan`; command encoders
will be added behind this same boundary rather than leaking backend objects.

## Selection sequence

1. The host creates its window/surface and probes APIs it can genuinely initialise.
2. It reports only successful probes as `DeviceCapabilities` / `CapabilityReport`.
3. The application supplies a minimum tier and an optional preferred tier.
4. `select_renderer` selects only profiles at or above the minimum. An unavailable
   result includes an ordered attempted list in Rust for diagnostics.
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

## Compatibility contract

The C ABI uses only fixed-width scalar fields and an opaque pointer. Each inbound
struct includes `abi_version`; v0.1 rejects any other value. Adding functions is
backward-compatible, but changing a field layout requires an ABI-version bump and
a new structure/function rather than reinterpreting old memory. C++ and C# use
exactly this header layout.

# Delivery gates (not release claims)

The policy core and CPU rasterizer are implemented, but no hardware device or
presentation path is implemented yet. This document is a gap list, not a claim
that the listed features are available in a later version. A feature is only
complete once it has a real implementation, a runnable sample, and a documented
failure path.

## Gate 1 — one real presentation path

- Add hardware texture, render-target, and frame-lifecycle opaque handles to the
  C ABI alongside the shipped CPU texture/canvas handles.
- Define a compact sprite instance layout shared by Rust batch output and C++
  hardware encoders.
- Add a software/mock bridge conformance suite covering device creation failure
  and fallback retry tests.

## Gate 2 — portable desktop baseline

- Windows Direct2D 1.2 compatibility bridge and Direct3D 11.3 balanced bridge.
- Linux Vulkan 1.1/1.2 bridge, with explicit OpenGL 3.0 emergency bridge.
- Window/surface integrations that keep native handles entirely in the bridge.

## Gate 3 — premium and mobile backends

- Direct3D 12 and Vulkan 1.3/1.4 premium encoders.
- Metal 1/2/3 bridge covering macOS and iOS.
- Android Vulkan and OpenGL ES bridges.

## Gate 4 — editor/tooling hosts

- WebGPU/WebGL browser bridge with asynchronous capability probing.
- Asset/font pipelines, clipping, tile maps, text, and render-to-texture.
- Editor-facing diagnostics that display selected tier, API, and fallback reason.

Every milestone must preserve strict selection as the default. A lower tier or an
emergency backend may only be used through the explicit best-effort path and
must remain visible to the host UI.

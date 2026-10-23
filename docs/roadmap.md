# Development roadmap

The v0.1 foundation intentionally distinguishes *policy selection* from a GPU
implementation. That lets all hosts make the same requirement and fallback
decision now, while drivers are brought up incrementally without changing C# or
C++ public APIs.

## v0.2 — command encoding contract

- Add hardware texture, render-target, and frame-lifecycle opaque handles to the
  C ABI alongside the shipped CPU texture/canvas handles.
- Define a compact sprite instance layout shared by Rust batch output and C++
  hardware encoders.
- Add a software/mock bridge conformance suite covering device creation failure
  and fallback retry tests.

## v0.3 — desktop baseline

- Windows Direct2D 1.2 compatibility bridge and Direct3D 11.3 balanced bridge.
- Linux Vulkan 1.1/1.2 bridge, with explicit OpenGL 3.0 emergency bridge.
- Window/surface integrations that keep native handles entirely in the bridge.

## v0.4 — premium and mobile

- Direct3D 12 and Vulkan 1.3/1.4 premium encoders.
- Metal 1/2/3 bridge covering macOS and iOS.
- Android Vulkan and OpenGL ES bridges.

## v0.5 — tooling hosts

- WebGPU/WebGL browser bridge with asynchronous capability probing.
- Asset/font pipelines, clipping, tile maps, text, and render-to-texture.
- Editor-facing diagnostics that display selected tier, API, and fallback reason.

Every milestone must preserve strict selection as the default. A lower tier or an
emergency backend may only be used through the explicit best-effort path and
must remain visible to the host UI.

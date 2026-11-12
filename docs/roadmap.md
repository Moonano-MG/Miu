# Delivery gates

This file tracks what is implemented and what is not. A feature counts as delivered
only when it has a real implementation, a runnable sample, and a documented failure
path — not when it has a design note.

## Done

* **Engine core** — math, premultiplied colour, `Camera2D` and its controller, scene
  graph with layers/opacity/tags, hit testing, stable draw order.
* **2D rendering** — sprite batching, paths with curves and dashes, analytic AA
  rasteriser, solid/linear/radial paints, five blend modes, clipping.
* **Text** — TrueType parsing (simple and composite glyphs), kerning, wrapping,
  alignment, byte-budgeted glyph cache, exact-outline path for transformed text.
* **Images** — PNG decode/encode (all filters, palette, 16-bit, greyscale), BMP, TGA,
  QOI, hand-written zlib inflate and a stored/fixed-Huffman deflate.
* **Animation** — keyframe tracks, easing, looping, fades, absolute-time seeking.
* **Documents** — JSON loader with validation, asset resolution, node instantiation,
  and animation binding.
* **CPU renderer** — display-list consumption, culling, statistics, PNG output.
* **Policy** — per-platform catalogues, three tiers, four presets, strict and
  best-effort selection, warning text.
* **ABI v3** — 60+ functions, layout pinned by `scripts/abi_layout_check.c`, C++17 and
  C# bindings, document and animation support.
* **Tooling** — `miu plan`, `miu info`, `miu render`; sample documents and generated
  artwork.

## Gate 1 — presentation path for one GPU API

* Linux Vulkan 1.1/1.2 factory in `native/src/bridge_vulkan.cpp`: instance, device,
  swapchain, upload, and the sprite/text/fill command stream.
* Frame pacing hooks so `QualityGovernor` can drive preset changes.
* A conformance suite that runs the same display list through the CPU and GPU devices
  and compares pixels within tolerance.

## Gate 2 — Windows and Apple desktop

* Direct2D 1.2 compatibility factory, Direct3D 11.3 balanced factory, Direct3D 12
  premium factory.
* Metal 2/3 factory for macOS and iOS, including the CPU fallback for Metal 1.
* Window creation samples per platform, keeping native handles inside the bridge.

## Gate 3 — mobile and web

* Android Vulkan and OpenGL ES factories with surface lifecycle handling.
* WebGPU and WebGL 2 factories, plus `select_web_renderer` integration so a wrapped
  desktop build keeps using its native API.
* Asynchronous probe reporting, because browser capability detection is not synchronous.

## Gate 4 — editor host

* Render-to-texture and layer compositing needed by a visual-novel editor.
* Atlas packing and asset hot-reload for authoring.
* Diagnostics panel showing tier, backend, preset, plan flags, and the fallback reason.

Strict selection stays the default at every gate: a lower tier or an emergency API is
only reachable through the explicit best-effort path and stays visible to the host UI.

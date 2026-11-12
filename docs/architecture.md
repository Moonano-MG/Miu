# Miu architecture

Miu is a tiered, capability-driven 2D engine. The Rust crates hold the engine logic;
the C++ layer holds everything that has to talk to a window or a graphics API. Both
sides meet at a single versioned C ABI, so a game can be written in Rust, C++, or C#
without the engine growing three implementations.

```
        hosts                       engine                         platform
  ┌──────────────┐        ┌──────────────────────┐        ┌────────────────────┐
  │ Rust game    │───┐    │ miu-scene            │        │ miu::bridge        │
  │ C++ / C# app │───┼───▶│   scene, animation,  │        │   probe_backends   │
  │ Web (wasm)   │───┘    │   document, player   │        │   device factories │
  └──────────────┘        ├──────────────────────┤        └─────────┬──────────┘
         │                │ miu-graphics         │                  │
         │   C ABI v3     │   texture, path,     │        ┌─────────▼──────────┐
         └───────────────▶│   raster, text,      │        │ D3D12 / Vulkan /   │
                          │   codecs, renderer   │        │ Metal / D2D / GLES │
                          ├──────────────────────┤        └────────────────────┘
                          │ miu-core             │
                          │   math, colour,      │
                          │   camera, policy,    │
                          │   quality            │
                          └──────────────────────┘
```

## Crates

| Crate | Contents | Depends on |
| --- | --- | --- |
| `miu-core` | vectors, rectangles, affine transforms, premultiplied colour, `Camera2D` and its controller, the backend catalogue and render-plan policy, quality presets and the frame-time governor | nothing |
| `miu-graphics` | textures and texture stores, sprite batches, paths and stroking, an analytic anti-aliasing rasteriser, gradients, TrueType parsing, text layout, glyph caching, PNG/BMP/TGA/QOI codecs, the CPU renderer | `miu-core` |
| `miu-scene` | scene graph, animation tracks, the JSON document format, and the `Player` frame driver | `miu-core`, `miu-graphics` |
| `miu-ffi` | the C ABI v3 surface: `cdylib`, `staticlib`, and `rlib` | all of the above |
| `miu-cli` | `miu plan`, `miu info`, `miu render` | `miu-core`, `miu-scene` |

No crate depends on a third-party library. Windowing, GPU device creation, and
platform probing live in C++ (`native/`), which is also where a host adds the APIs it
can ship.

## Frame model

1. **Input** — the host updates the camera, the scene, or both. The camera is pure
   state: `position`, `viewport`, `zoom`, `rotation_radians`. `CameraController` adds
   pan/zoom interaction (`zoom_at`, `pan_screen_delta`, `fit_rect`, bounds clamping).
2. **Build** — `Scene::build` walks the graph in painter order and emits a
   `DisplayList` of `DrawOp`s. Opacity accumulates down the tree, layers sort first,
   and invisible subtrees are skipped. Text layout happens here, so a frame costs one
   layout per text node rather than one per glyph.
3. **Render** — a backend consumes the display list. The CPU renderer rasterises
   directly; a GPU bridge uploads the same ops as per-instance state. Both see the
   same culling rules, so `RenderStats` is comparable across backends.
4. **Present** — the host swaps or writes the framebuffer. The CPU renderer can encode
   PNG for tests, thumbnails, and tools.

## Coordinate spaces

* **Local** — a node's own space; sprite rectangles and path outlines live here.
* **World** — after the node chain's transforms. Positions in documents are world
  coordinates.
* **Screen** — after the camera: `translate(-position) → scale(zoom) → rotate(-θ) →
  translate(viewport / 2)`. `Camera2D::screen_to_world` inverts it for picking.

Angles are radians in every API, and degrees in document JSON, because artists author
rotation in degrees and code wants radians. The loader converts once.

## Colour and blending

All stored colour is premultiplied. `Color::straight`/`Color::from_rgba8` premultiply
on the way in; `to_straight` converts back for authoring tools and encoders. Blend
modes are `SourceOver`, `Add`, `Multiply`, `Screen`, and `Copy`. Interpolation happens
in premultiplied space, which is what keeps alpha edges from glowing.

## Tiering and backend selection

`miu-core::policy` maps a `Platform` to a catalogue of `(backend, tier, emergency)`
profiles:

| Tier | Windows | Android | Apple | Linux | Web |
| --- | --- | --- | --- | --- | --- |
| Compatibility | Direct2D 1.2 | OpenGL ES 3.0 | Metal 2 | Vulkan 1.1 | WebGL 2 |
| Balanced | Direct3D 11.3 | Vulkan 1.2 / 1.3 | Metal 3 | Vulkan 1.2 | WebGL 2 |
| Premium | Direct3D 12 | Vulkan 1.4 | Metal 3 | Vulkan 1.3 | WebGPU |
| Emergency | OpenGL 3.3 | OpenGL ES 2.0 | Metal 1 | OpenGL 3.0 | WebGL 1 |

A project declares `minimum_tier`, `preferred_tier`, and a preset. Selection tries the
preferred tier first, then escalates; it never descends below the minimum and never
picks an emergency API. `select_best_effort` is the separate, explicit path that may
break those rules, and any plan it returns carries flags that `plan_warning` turns
into the message a player must confirm.

A browser build reports `Platform::Web`, but embedded web runtimes often expose the
real device: `select_web_renderer` takes an optional native device and prefers it,
falling back to WebGPU/WebGL only when the native probe fails.

## Quality presets

| Preset | performance / quality | MSAA | sprites / frame | glyph cache | texture cache |
| --- | --- | --- | --- | --- | --- |
| Low | 90 / 10 | 1 | 4 096 | 2 MiB | 64 MiB |
| Medium | 70 / 30 | 2 | 16 384 | 8 MiB | 128 MiB |
| High | 40 / 60 | 4 | 65 536 | 16 MiB | 256 MiB |
| Unlimited | 10 / 90 | 8 | 1 048 576 | 64 MiB | 1 024 MiB |

`Preset::budget()` exposes the split to settings screens. `QualityGovernor` smooths
frame times (EMA, α = 0.1) and steps presets after 30 slow or 120 fast frames with a
90-frame cooldown, so a loading hitch does not permanently downgrade quality.

## Documents

`miu-scene::document` reads a JSON scene description: viewport, camera, assets, a node
tree, and animations. Parsing resolves asset names to ids and validates structure;
decoding is the host's decision, which is what lets an editor keep textures in GPU
memory instead of re-reading files. See [`scene-format.md`](scene-format.md).

## Determinism

The CPU renderer is deterministic: same scene, same camera, same bytes out. That makes
golden-image tests viable and lets CI diff frames (`miu render`) without a GPU.

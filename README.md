# Miu

**Miu** is a tiered, capability-driven 2D rendering engine. Engine logic lives in
dependency-free Rust; a small, versioned C ABI exposes it to C++, C#, and web hosts;
C++ owns the platform layer that creates windows, probes drivers, and talks to
graphics APIs.

```rust
use miu_scene::{Document, Player};

let text = std::fs::read_to_string("samples/scenes/visual_novel.miu.json")?;
let document = Document::parse(&text)?;
let player = Player::from_document(&document, &read_assets(&document)?)?;
std::fs::write("frame.png", player.render_at_size(1280, 720)?)?;
```

```cpp
miu::Renderer renderer(1280, 720, miu::Preset::High);
miu::Camera camera = miu::Camera::ForViewport(1280.0f, 720.0f);
renderer.begin_frame(miu::Rgba8{12, 14, 18, 255});
renderer.fill_rect(camera, miu::Rect(40, 40, 200, 80), miu::colors::Rgb8(90, 180, 255));
```

## What is in the box

* **Independent `Camera2D`** — position, viewport, zoom, rotation, plus
  `CameraController` for pan, zoom-at-cursor, fit-to-rect, and bounds clamping.
  Screen↔world conversion is exact and invertible.
* **Scene graph** — groups, sprites, text, and vector shapes with transforms, layers,
  accumulated opacity, tags, hit testing, and stable draw ordering.
* **Text** — a TrueType parser (composite glyphs included), shaping with kerning and
  letter spacing, word/character wrapping, and a byte-budgeted glyph mask cache.
* **Vectors** — paths with cubic/quadratic curves, dashes, joins and caps, an analytic
  anti-aliasing rasteriser, solid/linear/radial gradients, and five blend modes.
* **Images** — PNG (decode and encode), BMP, TGA, and QOI codecs, all hand-written.
  Textures are premultiplied, with straight-alpha entry points.
* **Animation** — keyframe tracks with easing, looping, fades, and absolute-time
  seeking.
* **Documents** — a JSON scene format shared by the runtime, the CLI, and the editor;
  see [`docs/scene-format.md`](docs/scene-format.md).
* **Deterministic CPU renderer** — same scene in, same bytes out, which makes
  golden-image tests possible without a GPU.
* **Tiered policy** — three tiers, four quality presets, per-platform API catalogues,
  and explicit emergency fallbacks that can never be selected silently.
* **Four surfaces** — Rust crates, a C ABI (v3), a C++17 RAII wrapper, and a C# binding.

## Renderer tiers

| Tier | Goal | Windows | Android | Apple | Linux | Web |
| --- | --- | --- | --- | --- | --- | --- |
| Compatibility | lowest overhead, legacy devices | Direct2D 1.2 | OpenGL ES 3.0 | Metal 2 | Vulkan 1.1 | WebGL 2 |
| Balanced | quality/performance balance | Direct3D 11.3 | Vulkan 1.2–1.3 | Metal 3 | Vulkan 1.2 | WebGL 2 |
| Premium | strongest image features | Direct3D 12 | Vulkan 1.4 | Metal 3 | Vulkan 1.3 | WebGPU |

Emergency candidates — OpenGL 3.3 (Windows), OpenGL ES 2 (Android), Metal 1 (Apple),
OpenGL 3.0 (Linux), WebGL 1 (web) — are reachable only through `select_best_effort`,
always arrive flagged, and come with a warning string the host must show. Quality
presets are `Low` (90/10 performance to quality), `Medium` (70/30), `High` (40/60), and
`Unlimited` (10/90).

## Repository layout

```text
crates/miu-core       math, colour, Camera2D, backend policy, quality presets
crates/miu-graphics   textures, paths, rasteriser, fonts, text, codecs, CPU renderer
crates/miu-scene      scene graph, animation, JSON documents, Player driver
crates/miu-ffi        C ABI v3 (cdylib + staticlib + rlib)
crates/miu-cli        `miu plan`, `miu info`, `miu render`
include/miu           miu.h (C11) and miu.hpp (C++17 RAII)
native                platform bridge: capability probe, device factories, CPU device
bindings/csharp       P/Invoke surface with owning handle wrappers
samples               C++ programs and a sample visual-novel scene
scripts               manual verification and ABI layout check
docs                  architecture, ABI, scene format, roadmap
```

## Building

Rust 1.75 or newer; no third-party crates, so `cargo` needs no network access.

```sh
cargo test --workspace          # unit tests for every crate
cargo build --release           # miu-core, miu-graphics, miu-scene, miu-cli
cargo build -p miu-ffi --release # produces libmiu.so / miu.dll / libmiu.dylib
```

C++ pieces use CMake 3.21+:

```sh
cargo build -p miu-ffi --release
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

The ABI layout check and the sample builds do not need Cargo, so a C++-only checkout
can still validate the boundary:

```sh
cc  -std=c11 -Iinclude scripts/abi_layout_check.c -o /dev/null
g++ -std=c++17 -Iinclude -Inative/include -c samples/cpp/minimal.cpp -o /dev/null
```

## Command line

```sh
miu plan --platform linux --backends vulkan-1.2,vulkan-1.3 --min compatibility --prefer balanced
miu info samples/scenes/visual_novel.miu.json
miu render samples/scenes/visual_novel.miu.json --out frame.png --time 1.5 --width 1280 --height 720
```

`miu plan` prints the selected API, tier, preset, quality settings, and the warning a
player would see. `miu render` decodes the document's assets next to it and writes a
PNG, which is how CI produces reference frames.

## Status

The engine core — cameras, scene graph, text, vectors, codecs, animation, documents,
the CPU renderer, the policy layer, and the ABI — is implemented and unit-tested. The
platform layer ships the capability probe, the factory contract, and a working CPU
device; the per-API GPU factories (Direct3D, Vulkan, Metal, and the browser bridges)
are the remaining work and are tracked in [`docs/roadmap.md`](docs/roadmap.md). Until
they land, a host that opens a bridge session gets the CPU device, and the reason the
planned backend was unavailable is reported rather than hidden.

## Licence

GPL-3.0-or-later. See [`LICENSE`](LICENSE).

中文简介见 [`README.zh-CN.md`](README.zh-CN.md)。

# Miu

**Miu** is a capability-driven, cross-platform 2D renderer. Its rendering policy is written in Rust; a deliberately small C ABI makes it usable from modern C++, C#, and other native hosts. C++ owns the optional platform bridge layer so existing graphics SDKs can be integrated without exposing them through Rust's public API.

> Status: **v0.1.0-dev, not yet a presentation renderer**. Miu currently provides selection policy, 2D camera/batching, and a deterministic CPU reference rasterizer. It does **not** yet create a window, GPU device, swapchain, or present pixels. It is therefore not yet sufficient on its own for a shipped game or visual-novel editor.

## Design goals

- **Pragmatic 2D:** transforms, independent `Camera2D`, sprites, draw ordering, and batching are first-class.
- **Tiered deployment:** compatibility, balanced, and premium renderer tiers select the strongest usable backend at or above an application's declared minimum.
- **Predictable failures:** unsupported hardware is reported before a renderer is created; an application may explicitly choose a best-effort run.
- **Portable boundary:** Rust owns policy and frame preparation. C++ owns platform probes and compatibility-library bridges. C++ and C# call the same versioned C ABI.
- **No hidden platform fallback:** a fallback is part of the returned render plan and can be displayed in application UI or telemetry.

## Renderer tiers

| Tier | Goal | Windows | Android | Apple | Linux | Web |
|---|---|---|---|---|---|---|
| `Compatibility` | lowest overhead / legacy devices | Direct2D 1.2 | OpenGL ES 3.0 | Metal 2 | Vulkan 1.1 | WebGL 2 |
| `Balanced` | quality/performance balance | D3D 11.3 | Vulkan 1.2–1.3 | Metal 3 | Vulkan 1.2 | WebGL 2 |
| `Premium` | strongest image features | D3D 12 | Vulkan 1.4 | Metal 3 | Vulkan 1.3 | WebGPU |

If the requested primary API cannot initialise, Miu's **emergency** candidates are OpenGL 3.3 (Windows), OpenGL ES 2 (Android), Metal 1 (Apple), OpenGL 3.0 (Linux), and WebGL 1 (web). Emergency candidates are never silently selected: `RenderPlan.emergency_fallback` is set.

## Repository layout

```text
crates/miu-core    Rust policy, Camera2D, batching, and CPU reference renderer
crates/miu-ffi     versioned C ABI, policy handle, CPU texture/canvas handles
include/miu        public C header and C++17 RAII wrapper
native             C++ platform-probe bridge contract and default probe
bindings/csharp    P/Invoke surface with explicit native handle lifetime
samples/cpp        minimal C++ consumer
```

## Using the policy core (Rust)

```rust
use miu_core::{DeviceCapabilities, Platform, RendererRequest, RenderTier, select_renderer};

let device = DeviceCapabilities::for_platform(Platform::Windows)
    .with_backend(miu_core::Backend::Direct3D12);
let request = RendererRequest::new(RenderTier::Balanced);
let plan = select_renderer(&device, &request).expect("supported device");
assert_eq!(plan.backend, miu_core::Backend::Direct3D12);
```

## Manual build policy

This repository intentionally contains **no GitHub Actions build workflow**. Builds,
tests, samples, and platform validation are run explicitly by maintainers. The Rust
workspace needs Rust 1.75 or newer:

```sh
cargo test --workspace
cargo build -p miu-ffi --release
```

Compile `native/src/miu_bridge.cpp` with C++17 for the default platform probe, or implement the API from `native/include/miu/miu_bridge.h` in an application-specific bridge. Add `include` to your C++ include paths and link the produced `miu_ffi` static/shared library. The C++ sample can be syntax-checked with:

```sh
c++ -std=c++17 -Iinclude -Inative/include -c samples/cpp/minimal.cpp
c++ -std=c++17 -Iinclude -Inative/include -c samples/cpp/software_canvas.cpp
```

The software renderer is fully deterministic and requires neither a window nor a
GPU driver. It is suitable for headless tests, tools, and visual-novel preview
surfaces. Generate a small composited scene with:

```sh
cargo run -p miu-core --example software_scene
# writes miu-software-scene.ppm
```

`bindings/csharp/` intentionally has no NuGet dependency. It resolves the native library as `miu_ffi`; package that platform library beside the managed assembly or through the platform loader path.

## Web policy

Web builds should probe WebGPU first for the premium tier and WebGL for lower tiers. In embedded browser environments, a host may populate native platform capabilities (for example a Windows adapter) and pass those to the same selector; the web candidates remain the portable fallback. Actual browser probing belongs in the web host/bridge because browser APIs are asynchronous.

## Project requirement policy

Choose the lowest tier your project supports (`Compatibility`, `Balanced`, or `Premium`). Miu selects the strongest compliant profile at or above the declared minimum and records both the profile requirement and the actual device API. If no compliant plan exists, the result contains all attempted candidates. Calling the explicit force/best-effort entry point permits an emergency candidate and marks the plan unsafe so the host can warn users that a crash or degraded rendering is possible.

## License

Miu is licensed under [GPL-3.0-or-later](LICENSE). If you distribute a game,
editor, or other program that links Miu (including `miu_ffi`), GPL obligations
can apply to that combined distribution. This repository does not provide a
commercial-license exception. Confirm your distribution model with qualified
legal advice before integrating Miu into proprietary software.

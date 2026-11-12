# Miu

**Miu** 是一个按能力分级、跨平台的 2D 渲染引擎。引擎主体使用无第三方依赖的 Rust
实现；一个带版本号的精简 C ABI 把它暴露给 C++、C# 与 Web 宿主；窗口、驱动探测与图形
API 设备创建放在 C++ 平台层。

## 已实现的能力

* **独立 `Camera2D`**：位置、视口、缩放、旋转，配套 `CameraController` 支持拖拽平移、
  以光标为中心缩放、按矩形取景、边界约束；屏幕与世界坐标可精确互转。
* **场景图**：分组、精灵、文本、矢量图形，支持变换、图层、逐级透明度、标签、命中测试，
  绘制顺序稳定。
* **文本**：自研 TrueType 解析（含复合字形）、字距与字间距、按词/按字符换行、
  带字节预算的字形掩码缓存。
* **矢量**：贝塞尔路径、虚线、端点与连接样式、解析式抗锯齿光栅化、线性/径向渐变、
  五种混合模式。
* **图像**：PNG（解码与编码）、BMP、TGA、QOI 编解码器，全部手写；纹理内部统一预乘
  alpha，并提供直通 alpha 入口。
* **动画**：关键帧轨道、缓动曲线、循环、淡入淡出、按绝对时间跳转。
* **文档**：运行时、命令行与编辑器共用的 JSON 场景格式，详见
  [`docs/scene-format.md`](docs/scene-format.md)。
* **确定性 CPU 渲染器**：相同场景输出相同字节，便于做黄金图像测试。
* **分级策略**：三档渲染层级、四档画质预设、各平台 API 目录，以及只能显式启用的应急
  回退。
* **四种接口**：Rust crate、C ABI v3、C++17 RAII 封装、C# 绑定。

## 渲染层级

| 层级 | 目标 | Windows | Android | Apple | Linux | Web |
| --- | --- | --- | --- | --- | --- | --- |
| 兼容 | 开销最低、兼容旧设备 | Direct2D 1.2 | OpenGL ES 3.0 | Metal 2 | Vulkan 1.1 | WebGL 2 |
| 均衡 | 画质与性能平衡 | Direct3D 11.3 | Vulkan 1.2–1.3 | Metal 3 | Vulkan 1.2 | WebGL 2 |
| 旗舰 | 最强图像能力 | Direct3D 12 | Vulkan 1.4 | Metal 3 | Vulkan 1.3 | WebGPU |

应急回退（Windows OpenGL 3.3、Android OpenGL ES 2、Apple Metal 1、Linux OpenGL 3.0、
Web WebGL 1）只能通过 `select_best_effort` 启用，结果一定带标记并提供需要向玩家展示的
警告文案。画质预设为 `Low`（性能 90 / 画质 10）、`Medium`（70/30）、`High`（40/60）、
`Unlimited`（10/90）。

## 构建

需要 Rust 1.75 或更高版本。Rust 侧无第三方依赖，`cargo` 不需要联网：

```sh
cargo test --workspace
cargo build -p miu-ffi --release
```

C++ 部分使用 CMake 3.21 及以上：

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

仅验证 C/C++ 接口边界（无需 Cargo）：

```sh
cc  -std=c11 -Iinclude scripts/abi_layout_check.c -o /dev/null
g++ -std=c++17 -Iinclude -Inative/include -c samples/cpp/minimal.cpp -o /dev/null
```

## 命令行

```sh
miu plan --platform linux --backends vulkan-1.2,vulkan-1.3 --min compatibility --prefer balanced
miu info samples/scenes/visual_novel.miu.json
miu render samples/scenes/visual_novel.miu.json --out frame.png --time 1.5
```

## 当前状态

引擎核心（相机、场景图、文本、矢量、编解码、动画、文档、CPU 渲染器、策略层、ABI）
已实现并带有单元测试。平台层已提供能力探测、工厂契约与可用的 CPU 设备；各 API 的
GPU 工厂（Direct3D、Vulkan、Metal 与浏览器桥）仍在开发中，进度见
[`docs/roadmap.md`](docs/roadmap.md)。在此之前，宿主打开桥接会话会拿到 CPU 设备，
并被告知计划中的后端为何不可用，而不是静默降级。

## 许可

GPL-3.0-or-later，见 [`LICENSE`](LICENSE)。

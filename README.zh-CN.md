# Miu（中文说明）

Miu 是一个跨平台的通用 2D 渲染器基础工程：**Rust** 负责可测试的渲染策略、相机和批处理；**C++** 负责平台 API 探测、图形设备与 C++ 库适配；通过稳定的 C ABI 为 **Rust、C++ 和 C#** 提供入口。

当前版本是 `v0.1.0-dev`，已经实现：

- 兼容 / 均衡 / 高级三档渲染策略，以及每个平台指定的 API 候选与紧急回退；
- 声明项目最低档位、优选档位、严格选择和必须显式开启的强制运行；
- `Camera2D` 的平移、缩放、旋转和屏幕/世界坐标反变换；
- 保持透明绘制顺序的批处理和适用于不透明内容的纹理排序批处理；
- C ABI、C++17 RAII 包装、无 NuGet 依赖的 C# P/Invoke 包装和 C++ 平台桥接契约。

它**还不是完整 GPU 命令编码器**。真实的 Direct2D、D3D、Vulkan、Metal、OpenGL 与 Web 实现应放在 `native/` 的平台桥接层中；当前桥接层不会伪造设备支持情况。应用只有在成功创建对应 API 后，才能把该 API 加入 capability report。

详细的英文构建、档位表与示例见 [README.md](README.md)；边界与选择流程见 [docs/architecture.md](docs/architecture.md)。

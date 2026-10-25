# Miu（中文说明）

Miu 是一个跨平台的通用 2D 渲染器基础工程：**Rust** 负责可测试的渲染策略、相机和批处理；**C++** 负责平台 API 探测、图形设备与 C++ 库适配；通过稳定的 C ABI 为 **Rust、C++ 和 C#** 提供入口。

当前版本是 `v0.1.0-dev`。它目前是策略与 CPU 参考光栅器，**还不能创建窗口、GPU 设备、交换链或 Present 到屏幕**；不能被描述为已经可独立发布游戏或成熟视觉小说编辑器。当前已经实现：

- 兼容 / 均衡 / 高级三档渲染策略，以及每个平台指定的 API 候选与紧急回退；
- 声明项目最低档位、优选档位（优先尝试，无法运行才向更高档升级）、严格选择和必须显式开启的强制运行；
- `Camera2D` 的平移、缩放、旋转和屏幕/世界坐标反变换；
- 保持透明绘制顺序的批处理和适用于不透明内容的纹理排序批处理；
- 支持预乘 RGBA8、显式直通 alpha 转预乘、旋转/缩放/UV/alpha 混合的确定性 CPU 参考渲染器；UV 被限制在 atlas 源矩形内，可用于无窗口测试、预览和工具；
- C ABI、C++17 RAII 包装、无 NuGet 依赖的 C# P/Invoke 包装和 C++ 平台桥接契约。

它**还不是完整 GPU 命令编码器**。CPU 渲染器已经可用；真实的 Direct2D、D3D、Vulkan、Metal、OpenGL 与 Web 实现应放在 `native/` 的平台桥接层中。当前桥接层不会伪造设备支持情况：应用只有在成功创建对应 API 后，才能把该 API 加入 capability report。

详细的英文构建、档位表与示例见 [README.md](README.md)；边界与选择流程见 [docs/architecture.md](docs/architecture.md)。


## 手动构建

仓库不包含 GitHub Actions 自动编译工作流。维护者自行运行 `./scripts/verify-manually.sh`（或其中列出的 `cargo test --workspace`、C/C++ 和 .NET 命令）进行验证。ABI v1/v2 的安全迁移说明见 [docs/abi-v2-migration.md](docs/abi-v2-migration.md)。

## 许可证说明

Miu 使用 [GPL-3.0-or-later](LICENSE)。分发链接 Miu（包括 `miu_ffi`）的游戏、编辑器或其他程序时，GPL 义务可能适用于该组合分发物。仓库目前**没有**商业授权例外；闭源集成前应由合格的法律专业人士确认分发方案。

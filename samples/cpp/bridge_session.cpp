// Opens a platform bridge session and renders through whatever device it returns.
//
//   c++ -std=c++17 -Iinclude -Inative/include samples/cpp/bridge_session.cpp
//       native/src/miu_bridge.cpp -Ltarget/debug -lmiu -o bridge_session
//   ./bridge_session /tmp/bridge.png
//
// On a build without GPU factories the bridge reports why the planned backend is
// unavailable and hands back the CPU device, which is what this sample renders with.
// The program is the reference for how a host should behave: show the plan, show the
// warning when the plan is below the project's minimum, then keep rendering.

#include <cstdio>
#include <cstdlib>
#include <vector>

#include "miu/miu.hpp"
#include "miu/miu_bridge.h"

int main(int argc, char **argv) {
    // A host would present the frame to its window; a headless build keeps a
    // reference to the destination it would write instead.
    const char *output = argc > 1 ? argv[1] : nullptr;
    try {
        constexpr std::uint32_t kWidth = 320;
        constexpr std::uint32_t kHeight = 180;

        miu::bridge::SurfaceConfig surface;
        surface.width = kWidth;
        surface.height = kHeight;
        surface.preset = miu::Preset::High;

        // An empty probe list means "trust the catalogue"; a real host reports the
        // APIs its loader actually created.
        const std::vector<miu::Backend> probed{miu::Backend::Vulkan12, miu::Backend::Vulkan13};
        miu::bridge::Session session = miu::bridge::Session::Open(
            miu::Platform::Linux, probed, miu::Tier::Compatibility, miu::Tier::Balanced,
            miu::Preset::High, surface, /*best_effort=*/false);

        if (!session) {
            std::fprintf(stderr, "no device: %s\n", session.reason().c_str());
            return EXIT_FAILURE;
        }
        if (!session.warning().empty()) {
            std::printf("warning: %s\n", session.warning().c_str());
        }
        if (!session.reason().empty()) {
            std::printf("note: %s\n", session.reason().c_str());
        }

        miu::bridge::Device &device = session.device();
        std::printf("backend %s (%s)\n", miu_backend_name(static_cast<std::uint8_t>(session.backend())),
                    device.is_software() ? "software" : "accelerated");

        miu::Camera camera = miu::Camera::ForViewport(static_cast<float>(kWidth),
                                                     static_cast<float>(kHeight));
        if (device.begin_frame(true, miu::Rgba8{0, 0, 0, 255}) != MIU_STATUS_OK) {
            std::fprintf(stderr, "begin_frame failed\n");
            return EXIT_FAILURE;
        }
        const float width = static_cast<float>(kWidth);
        const float height = static_cast<float>(kHeight);
        for (int index = 0; index < 12; ++index) {
            const float t = static_cast<float>(index) / 12.0f;
            const miu_status_t status =
                device.fill_rect(camera,
                                 miu::Rect(t * width, (1.0f - t) * height * 0.5f, width / 16.0f,
                                           height / 3.0f),
                                 miu::colors::Rgba(0.2f + 0.06f * index, 0.8f - 0.05f * index, 0.9f),
                                 miu::Blend::SourceOver);
            if (status != MIU_STATUS_OK) {
                std::fprintf(stderr, "fill failed: %s\n", miu::status_message(status).c_str());
                return EXIT_FAILURE;
            }
        }
        if (device.end_frame() != MIU_STATUS_OK) {
            std::fprintf(stderr, "end_frame failed\n");
            return EXIT_FAILURE;
        }

        miu_render_stats_t stats = device.stats();
        std::vector<std::uint8_t> pixels;
        if (device.read_pixels(pixels) == MIU_STATUS_OK) {
            // A real GPU device can leave readback unsupported; the CPU device always
            // provides it, which is what makes golden-image tests possible.
            std::printf("readback %zu bytes, fills=%u\n", pixels.size(), stats.fills);
        }
        if (output != nullptr) {
            std::printf("destination %s\n", output);
        }
        session.Close();
        return EXIT_SUCCESS;
    } catch (const miu::Error &error) {
        std::fprintf(stderr, "miu: %s\n", error.what());
        return EXIT_FAILURE;
    }
}

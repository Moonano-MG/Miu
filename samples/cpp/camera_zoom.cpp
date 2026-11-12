// Camera2D demonstration: pan, zoom, rotation, and screen-to-world picking.
//
//   c++ -std=c++17 -Iinclude samples/cpp/camera_zoom.cpp -Ltarget/debug -lmiu -o camera_zoom
//   ./camera_zoom /tmp/zoom-out.png
//
// The sample draws the same grid and marker from four camera states so the effect of
// each camera field is visible in one image. The engine's camera is independent of
// the scene graph, so a host can drive it from input in any way it likes; the Rust
// `CameraController` adds the usual pinch/scroll/wheel behaviour on top.

#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>

#include "miu/miu.hpp"

namespace {

constexpr std::uint32_t kTileSize = 256;

// Soft radial marker so camera rotation is easy to see.
std::vector<std::uint8_t> marker_texture() {
    std::vector<std::uint8_t> pixels(static_cast<std::size_t>(kTileSize) * kTileSize * 4u);
    const float centre = static_cast<float>(kTileSize) * 0.5f;
    const float radius = centre - 8.0f;
    for (std::uint32_t y = 0; y < kTileSize; ++y) {
        for (std::uint32_t x = 0; x < kTileSize; ++x) {
            const float dx = (static_cast<float>(x) + 0.5f) - centre;
            const float dy = (static_cast<float>(y) + 0.5f) - centre;
            const float distance = std::sqrt(dx * dx + dy * dy);
            const float coverage = std::fmax(0.0f, std::fmin(1.0f, radius - distance));
            const std::size_t index = (static_cast<std::size_t>(y) * kTileSize + x) * 4u;
            pixels[index + 0] = 255;
            pixels[index + 1] = static_cast<std::uint8_t>(190 + 40 * coverage);
            pixels[index + 2] = static_cast<std::uint8_t>(90 + 60 * coverage);
            pixels[index + 3] = static_cast<std::uint8_t>(coverage * 255.0f);
        }
    }
    return pixels;
}

void draw_grid(miu::Renderer &renderer, const miu::Camera &camera, float extent, float step,
               const miu::Color &color) {
    for (float offset = -extent; offset <= extent; offset += step) {
        renderer.fill_rect(camera, miu::Rect::New(offset, -extent, 1.0f, extent * 2.0f), color);
        renderer.fill_rect(camera, miu::Rect::New(-extent, offset, extent * 2.0f, 1.0f), color);
    }
}

} // namespace

int main(int argc, char **argv) {
    const char *output = argc > 1 ? argv[1] : "camera_zoom.png";
    try {
        constexpr std::uint32_t kWidth = 512;
        constexpr std::uint32_t kHeight = 512;
        miu::Renderer renderer(kWidth, kHeight, miu::Preset::High);

        const std::vector<std::uint8_t> marker = marker_texture();
        miu::Texture marker_texture_handle =
            miu::Texture::FromStraight(kTileSize, kTileSize, marker.data());
        renderer.upload(1, marker_texture_handle);

        renderer.begin_frame(miu::Rgba8{12, 12, 18, 255});

        // Four viewports, each showing a different camera state.
        const float half_width = static_cast<float>(kWidth) * 0.5f;
        const float half_height = static_cast<float>(kHeight) * 0.5f;
        struct View {
            miu::Rect rect;
            miu::Vec2 position;
            float zoom;
            float rotation;
            const char *label;
        };
        const std::array<View, 4> views{{
            {miu::Rect(0.0f, 0.0f, half_width, half_height), miu::Vec2{0.0f, 0.0f}, 1.0f, 0.0f,
             "identity"},
            {miu::Rect(half_width, 0.0f, half_width, half_height), miu::Vec2{0.0f, 0.0f}, 2.0f,
             0.0f, "zoom 2x"},
            {miu::Rect(0.0f, half_height, half_width, half_height),
             miu::Vec2{96.0f, -48.0f}, 1.0f, 0.35f, "rotated + panned"},
            {miu::Rect(half_width, half_height, half_width, half_height),
             miu::Vec2{0.0f, 0.0f}, 0.6f, 0.0f, "zoom 0.6x"},
        }};

        for (const View &view : views) {
            renderer.set_clip(view.rect);
            miu::Camera camera;
            camera.viewport = miu::Vec2{view.rect.width, view.rect.height};
            camera.position = view.position;
            camera.zoom = view.zoom;
            camera.rotation_radians = view.rotation;

            // The camera keeps world space independent of the viewport, so the same
            // world coordinates land in each quadrant.
            draw_grid(renderer, camera, 220.0f, 40.0f, miu::colors::Rgba(0.25f, 0.35f, 0.55f, 0.6f));

            miu_sprite_t sprite = miu_sprite_default();
            sprite.texture = 1;
            sprite.transform.translation = view.position;
            sprite.size = miu::Vec2{static_cast<float>(kTileSize), static_cast<float>(kTileSize)};
            sprite.pivot = miu::Vec2{0.5f, 0.5f};
            renderer.draw_sprite(camera, sprite);
        }
        renderer.clear_clip();

        const std::vector<std::uint8_t> png = renderer.encode_png();
        std::FILE *file = std::fopen(output, "wb");
        if (!file) {
            std::fprintf(stderr, "cannot open %s\n", output);
            return EXIT_FAILURE;
        }
        std::fwrite(png.data(), 1, png.size(), file);
        std::fclose(file);
        std::printf("%s: %zu bytes\n", output, png.size());
        return EXIT_SUCCESS;
    } catch (const miu::Error &error) {
        std::fprintf(stderr, "miu: %s\n", error.what());
        return EXIT_FAILURE;
    }
}

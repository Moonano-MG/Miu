// Minimal Miu program: create a texture, upload it, draw a frame, write a PNG.
//
// Build (once `libmiu` exists, either from `cargo build -p miu-ffi` or from the
// CMake target):
//
//   c++ -std=c++17 -Iinclude samples/cpp/minimal.cpp -Ltarget/debug -lmiu -o minimal
//
// The program keeps to the public C++ wrapper; the same calls exist in C in
// `miu.h`, and in Rust through `miu-graphics`.

#include <cstdio>
#include <cstdlib>
#include <vector>

#include "miu/miu.hpp"

namespace {

// Builds a checkerboard texture so the sample needs no asset files.
std::vector<std::uint8_t> checkerboard(std::uint32_t size, std::uint32_t cell) {
    std::vector<std::uint8_t> pixels(static_cast<std::size_t>(size) * size * 4u);
    for (std::uint32_t y = 0; y < size; ++y) {
        for (std::uint32_t x = 0; x < size; ++x) {
            const bool light = ((x / cell) + (y / cell)) % 2 == 0;
            const std::uint8_t value = light ? 230 : 40;
            const std::size_t index = (static_cast<std::size_t>(y) * size + x) * 4u;
            pixels[index + 0] = value;
            pixels[index + 1] = static_cast<std::uint8_t>(light ? 120 : 20);
            pixels[index + 2] = static_cast<std::uint8_t>(light ? 60 : 120);
            pixels[index + 3] = 255;
        }
    }
    return pixels;
}

} // namespace

int main(int argc, char **argv) {
    const char *output = argc > 1 ? argv[1] : "minimal.png";
    try {
        constexpr std::uint32_t kSize = 256;
        miu::Renderer renderer(kSize, kSize, miu::Preset::High);

        miu::Texture texture =
            miu::Texture::FromStraight(kSize, kSize, checkerboard(kSize, 16).data());
        renderer.upload(1, texture);

        miu::Camera camera = miu::Camera::ForViewport(
            static_cast<float>(kSize), static_cast<float>(kSize));
        renderer.begin_frame(miu::Rgba8{18, 18, 24, 255});

        // A sprite rotated by 30 degrees and tinted warm, drawn over a backdrop.
        miu_sprite_t sprite = miu_sprite_default();
        sprite.texture = 1;
        sprite.transform = miu_transform2d_identity();
        sprite.transform.translation = miu::Vec2{static_cast<float>(kSize) * 0.5f,
                                                 static_cast<float>(kSize) * 0.5f};
        sprite.transform.rotation_radians = 0.5235988f;
        sprite.transform.scale = miu::Vec2{0.7f, 0.7f};
        sprite.size = miu::Vec2{static_cast<float>(kSize), static_cast<float>(kSize)};
        sprite.pivot = miu::Vec2{0.5f, 0.5f};
        sprite.color = miu::colors::Rgba(1.0f, 0.85f, 0.7f, 1.0f);
        renderer.draw_sprite(camera, sprite);

        renderer.fill_rect(camera, miu::Rect::New(16.0f, 16.0f, 96.0f, 12.0f),
                           miu::colors::Rgb8(80, 200, 255), miu::Blend::Add);

        const std::vector<std::uint8_t> png = renderer.encode_png();
        const miu_render_stats_t stats = renderer.stats();

        std::FILE *file = std::fopen(output, "wb");
        if (!file) {
            std::fprintf(stderr, "cannot open %s\n", output);
            return EXIT_FAILURE;
        }
        std::fwrite(png.data(), 1, png.size(), file);
        std::fclose(file);

        std::printf("%s: %zu bytes, %u sprites, %u fills, %u glyphs\n", output, png.size(),
                    stats.sprites, stats.fills, stats.glyphs);
        return EXIT_SUCCESS;
    } catch (const miu::Error &error) {
        std::fprintf(stderr, "miu: %s\n", error.what());
        return EXIT_FAILURE;
    }
}

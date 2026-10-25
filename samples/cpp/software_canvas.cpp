#include "miu/miu.hpp"

#include <array>
#include <vector>

int main() {
    const std::vector<std::uint8_t> red_pixel{255U, 0U, 0U, 255U};
    miu::Status status{};
    miu::CpuTexture texture = miu::CpuTexture::create(1U, 1U, red_pixel, &status);
    miu::CpuCanvas canvas = miu::CpuCanvas::create(64U, 64U, &status);
    if (status != miu::Status::Ok || !texture.valid() || !canvas.valid()) {
        return 1;
    }

    miu::CpuCamera camera{};
    camera.viewport = {64.0F, 64.0F};
    camera.zoom = 1.0F;
    miu::CpuSprite sprite{};
    sprite.transform.scale = {1.0F, 1.0F};
    sprite.source = {0U, 0U, 1U, 1U};
    sprite.uv = {0.0F, 0.0F, 1.0F, 1.0F};
    sprite.size = {1.0F, 1.0F};
    sprite.pivot = {0.5F, 0.5F};
    sprite.color = {1.0F, 1.0F, 1.0F, 1.0F};

    if (canvas.draw(camera, sprite, texture) != miu::Status::Ok) {
        return 2;
    }
    std::vector<std::uint8_t> pixels;
    return canvas.copy_rgba8(&pixels) == miu::Status::Ok ? 0 : 3;
}

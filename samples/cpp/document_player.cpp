// Plays a scene document: parse JSON, supply its assets, animate, write PNG frames.
//
//   c++ -std=c++17 -Iinclude samples/cpp/document_player.cpp -Ltarget/debug -lmiu -o document_player
//   ./document_player samples/scenes/visual_novel.miu.json /tmp/frames 3 0.5
//
// Arguments: <document> <output-directory> [frame-count] [seconds-per-frame]
//
// Fonts are loaded when the document declares them; the sample documents shipped
// with the engine use textures only, so the asset loader below covers both cases
// and reports anything it cannot decode.

#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

#include "miu/miu.hpp"

namespace {

std::vector<std::uint8_t> read_file(const std::filesystem::path &path) {
    std::FILE *file = std::fopen(path.string().c_str(), "rb");
    if (!file) {
        throw std::runtime_error("cannot open " + path.string());
    }
    std::fseek(file, 0, SEEK_END);
    const long size = std::ftell(file);
    std::fseek(file, 0, SEEK_SET);
    std::vector<std::uint8_t> bytes(static_cast<std::size_t>(size > 0 ? size : 0));
    if (!bytes.empty() && std::fread(bytes.data(), 1, bytes.size(), file) != bytes.size()) {
        std::fclose(file);
        throw std::runtime_error("short read on " + path.string());
    }
    std::fclose(file);
    return bytes;
}

bool looks_like_font(std::string_view path) {
    const auto ends_with = [path](std::string_view suffix) {
        return path.size() >= suffix.size() &&
               path.substr(path.size() - suffix.size()) == suffix;
    };
    return ends_with(".ttf") || ends_with(".otf");
}

void write_file(const std::filesystem::path &path, const std::vector<std::uint8_t> &bytes) {
    std::FILE *file = std::fopen(path.string().c_str(), "wb");
    if (!file) {
        throw std::runtime_error("cannot write " + path.string());
    }
    std::fwrite(bytes.data(), 1, bytes.size(), file);
    std::fclose(file);
}

} // namespace

int main(int argc, char **argv) {
    if (argc < 3) {
        std::fprintf(stderr,
                     "usage: %s <document.miu.json> <output-directory> [frames] [seconds/frame]\n",
                     argv[0]);
        return EXIT_FAILURE;
    }
    const std::filesystem::path document_path = argv[1];
    const std::filesystem::path output_dir = argv[2];
    const int frames = argc > 3 ? std::atoi(argv[3]) : 1;
    const float frame_step = argc > 4 ? std::strtof(argv[4], nullptr) : 0.5f;

    try {
        std::filesystem::create_directories(output_dir);
        const std::vector<std::uint8_t> json = read_file(document_path);
        const std::string text(json.begin(), json.end());

        miu::Document document = miu::Document::Parse(text);
        const miu::Vec2 viewport = document.viewport();
        std::printf("document %s: %.0fx%.0f, %zu assets\n", document_path.filename().string().c_str(),
                    viewport.x, viewport.y, document.asset_paths().size());

        // Decode every declared asset once and hand it to the document. Keeping the
        // handles alive matters: the document clones them at instantiation time.
        std::vector<miu::Texture> textures;
        std::vector<miu::Font> fonts;
        const std::filesystem::path asset_root = document_path.parent_path();
        for (const std::string &relative : document.asset_paths()) {
            const std::filesystem::path asset_path = asset_root / relative;
            const std::vector<std::uint8_t> bytes = read_file(asset_path);
            if (looks_like_font(relative)) {
                fonts.push_back(miu::Font::Load(bytes));
                document.set_font(relative, fonts.back());
                std::printf("  font    %s (%u glyphs)\n", relative.c_str(),
                            fonts.back().glyph_count());
            } else {
                textures.push_back(miu::Texture::Load(bytes));
                document.set_texture(relative, textures.back());
                const miu::Vec2 size = textures.back().size();
                std::printf("  texture %s (%.0fx%.0f)\n", relative.c_str(), size.x, size.y);
            }
        }

        miu::Scene scene = document.instantiate();
        std::printf("scene: %u nodes, %u animations\n", scene.node_count(), scene.animation_count());

        miu::Renderer renderer(static_cast<std::uint32_t>(viewport.x),
                               static_cast<std::uint32_t>(viewport.y), miu::Preset::High);
        scene.attach_to(renderer);

        for (int frame = 0; frame < frames; ++frame) {
            const float time = static_cast<float>(frame) * frame_step;
            scene.update(time);
            scene.draw(renderer);

            const miu_render_stats_t stats = renderer.stats();
            const std::vector<std::uint8_t> png = renderer.encode_png();
            char name[64];
            std::snprintf(name, sizeof(name), "frame-%03d.png", frame);
            write_file(output_dir / name, png);
            std::printf("frame %d at t=%.2fs: %zu bytes, sprites=%u glyphs=%u skipped=%u\n", frame,
                        time, png.size(), stats.sprites, stats.glyphs, stats.skipped);
        }
        return EXIT_SUCCESS;
    } catch (const miu::Error &error) {
        std::fprintf(stderr, "miu: %s\n", error.what());
        return EXIT_FAILURE;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "%s\n", error.what());
        return EXIT_FAILURE;
    }
}

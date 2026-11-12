// Miu 2D rendering engine — C++17 interface.
//
// `miu.hpp` is a header-only RAII layer over `miu.h`. It adds:
//
//   * `miu::Error`, thrown when a call fails, carrying the C status code;
//   * owning wrappers (`Texture`, `Font`, `Renderer`, `Scene`, `Document`) that
//     release their handle in the destructor and move instead of copying;
//   * typed helpers for colour, geometry, and presets, so call sites read like
//     engine code rather than ABI code.
//
// The layer never hides the C API: `miu.h` stays available for hosts that prefer
// status codes, C# interop, or custom lifetime handling. Everything here is
// `inline`, so including this header needs no extra build step.
//
// Example
// -------
//     miu::Renderer renderer(320, 180, miu::Preset::High);
//     miu::Camera2D camera = miu::Camera2D::for_viewport(320, 180);
//     renderer.begin_frame(miu::Rgba8{20, 20, 28, 255});
//     renderer.fill_rect(camera, miu::Rect::new(32, 32, 64, 32), miu::Color::rgb(1.f, 0.6f, 0.2f));
//     std::vector<std::uint8_t> png = renderer.encode_png();

#ifndef MIU_MIU_HPP
#define MIU_MIU_HPP

#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "miu.h"

namespace miu {

/** Engine version reported by the loaded library. */
inline std::string version() { return std::string(miu_version_string()); }

/** ABI version of the loaded library; compare with `MIU_ABI_VERSION` at load time. */
inline std::uint32_t abi_version() { return miu_abi_version(); }

/** English text for a status code. */
inline std::string status_message(miu_status_t status) {
    const char *text = miu_status_message(static_cast<std::uint32_t>(status));
    return text ? std::string(text) : std::string("unknown status");
}

/**
 * Failure raised by the wrapper.
 *
 * The C API reports failures with status codes; the C++ layer converts them to
 * exceptions so that a host cannot forget to check one. `code()` keeps the original
 * status for callers that want to branch.
 */
class Error : public std::runtime_error {
public:
    Error(miu_status_t code, std::string_view action)
        : std::runtime_error(message_for(code, action)), code_(code) {}

    /** Original C status. */
    miu_status_t code() const noexcept { return code_; }

private:
    static std::string message_for(miu_status_t code, std::string_view action) {
        std::string text(action);
        text += ": ";
        text += status_message(code);
        return text;
    }

    miu_status_t code_;
};

/** Throws when `status` is not `MIU_STATUS_OK`. */
inline void check(miu_status_t status, std::string_view action) {
    if (status != MIU_STATUS_OK) {
        throw Error(status, action);
    }
}

/* ------------------------------------------------------------------------- */
/* Value types                                                               */
/* ------------------------------------------------------------------------- */

/** Two-dimensional vector. */
using Vec2 = miu_vec2_t;
/** Straight-alpha colour. */
using Color = miu_color_t;
/** Premultiplied 8-bit texel. */
using Rgba8 = miu_rgba8_t;
/** Axis-aligned rectangle. */
using RectBase = miu_rect_t;
/** Translation/rotation/scale. */
using Transform2D = miu_transform2d_t;
/** Independent 2D camera. */
using SourceRect = miu_source_rect_t;
/** Text layout parameters. */
using TextStyle = miu_text_style_t;

/** Quality presets. */
enum class Preset : std::uint8_t {
    Low = MIU_PRESET_LOW,
    Medium = MIU_PRESET_MEDIUM,
    High = MIU_PRESET_HIGH,
    Unlimited = MIU_PRESET_UNLIMITED
};

/** Feature tiers. */
enum class Tier : std::uint8_t {
    Compatibility = MIU_TIER_COMPATIBILITY,
    Balanced = MIU_TIER_BALANCED,
    Premium = MIU_TIER_PREMIUM
};

/** Platforms. */
enum class Platform : std::uint8_t {
    Windows = MIU_PLATFORM_WINDOWS,
    Android = MIU_PLATFORM_ANDROID,
    MacOS = MIU_PLATFORM_MACOS,
    IOS = MIU_PLATFORM_IOS,
    Linux = MIU_PLATFORM_LINUX,
    Web = MIU_PLATFORM_WEB
};

/** Graphics API profiles. */
enum class Backend : std::uint8_t {
    Direct2D12 = MIU_BACKEND_DIRECT2D_1_2,
    Direct3D113 = MIU_BACKEND_DIRECT3D_11_3,
    Direct3D12 = MIU_BACKEND_DIRECT3D_12,
    OpenGLES30 = MIU_BACKEND_OPENGLES_3_0,
    OpenGLES20 = MIU_BACKEND_OPENGLES_2_0,
    Metal2 = MIU_BACKEND_METAL_2,
    Metal3 = MIU_BACKEND_METAL_3,
    Metal1 = MIU_BACKEND_METAL_1,
    Vulkan11 = MIU_BACKEND_VULKAN_1_1,
    Vulkan12 = MIU_BACKEND_VULKAN_1_2,
    Vulkan13 = MIU_BACKEND_VULKAN_1_3,
    Vulkan14 = MIU_BACKEND_VULKAN_1_4,
    OpenGL33 = MIU_BACKEND_OPENGL_3_3,
    OpenGL30 = MIU_BACKEND_OPENGL_3_0,
    WebGL2 = MIU_BACKEND_WEBGL_2,
    WebGL1 = MIU_BACKEND_WEBGL_1,
    WebGPU = MIU_BACKEND_WEBGPU
};

/** Compositing modes. */
enum class Blend : std::uint8_t {
    SourceOver = MIU_BLEND_SOURCE_OVER,
    Add = MIU_BLEND_ADD,
    Multiply = MIU_BLEND_MULTIPLY,
    Screen = MIU_BLEND_SCREEN,
    Copy = MIU_BLEND_COPY
};

/** Horizontal text alignment. */
enum class TextAlign : std::uint8_t {
    Left = MIU_TEXT_ALIGN_LEFT,
    Center = MIU_TEXT_ALIGN_CENTER,
    Right = MIU_TEXT_ALIGN_RIGHT
};

/** Line-breaking strategy. */
enum class TextWrap : std::uint8_t {
    None = MIU_TEXT_WRAP_NONE,
    Word = MIU_TEXT_WRAP_WORD,
    Character = MIU_TEXT_WRAP_CHARACTER
};

/** Rect with convenience constructors. */
struct Rect : RectBase {
    /** Empty rectangle at the origin. */
    Rect() : RectBase{0.0f, 0.0f, 0.0f, 0.0f} {}

    /** Rectangle from position and size. */
    Rect(float x, float y, float width, float height) : RectBase{x, y, width, height} {}

    /** Rectangle from position and size. */
    static Rect New(float x, float y, float width, float height) { return Rect(x, y, width, height); }

    /** Rectangle covering `width x height` from the origin. */
    static Rect FromSize(float width, float height) { return Rect(0.0f, 0.0f, width, height); }

    /** Right edge. */
    float Right() const { return x + width; }
    /** Bottom edge. */
    float Bottom() const { return y + height; }
};

/** Camera with helpers for common setups. */
struct Camera : miu_camera2d_t {
    /** Camera framed to a `1x1` viewport at unit zoom. */
    Camera() : miu_camera2d_t{miu_camera2d_default()} {}

    /** Camera framed to `width x height` and centred on the viewport. */
    static Camera ForViewport(float width, float height) {
        Camera camera;
        camera.viewport = Vec2{width, height};
        camera.position = Vec2{width * 0.5f, height * 0.5f};
        return camera;
    }

    /** Camera centred on `world` with the given zoom. */
    static Camera CenteredOn(Vec2 world, float zoom, float viewport_width, float viewport_height) {
        Camera camera;
        camera.position = world;
        camera.zoom = zoom;
        camera.viewport = Vec2{viewport_width, viewport_height};
        return camera;
    }
};

/** Colour helpers. */
namespace colors {
/** White with full alpha. */
inline Color White() { return Color{1.0f, 1.0f, 1.0f, 1.0f}; }
/** Transparent black. */
inline Color Transparent() { return Color{0.0f, 0.0f, 0.0f, 0.0f}; }
/** Straight-alpha colour from unit components. */
inline Color Rgba(float r, float g, float b, float a = 1.0f) { return Color{r, g, b, a}; }
/** Straight-alpha colour from 8-bit components. */
inline Color Rgb8(std::uint8_t r, std::uint8_t g, std::uint8_t b, std::uint8_t a = 255) {
    return Color{static_cast<float>(r) / 255.0f,
                 static_cast<float>(g) / 255.0f,
                 static_cast<float>(b) / 255.0f,
                 static_cast<float>(a) / 255.0f};
}
} // namespace colors

/* ------------------------------------------------------------------------- */
/* Owning handles                                                            */
/* ------------------------------------------------------------------------- */

/** Owning texture handle. */
class Texture {
public:
    /** Wraps an existing handle; the wrapper takes ownership. */
    explicit Texture(miu_texture_t *handle = nullptr) noexcept : handle_(handle) {}

    ~Texture() { reset(); }

    Texture(Texture &&other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Texture &operator=(Texture &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    Texture(const Texture &) = delete;
    Texture &operator=(const Texture &) = delete;

    /** Creates a texture from premultiplied RGBA8 pixels. */
    static Texture FromPremultiplied(std::uint32_t width, std::uint32_t height,
                                     const std::uint8_t *pixels) {
        miu_texture_t *handle = nullptr;
        check(miu_texture_create(width, height, pixels, &handle), "texture create");
        return Texture(handle);
    }

    /** Creates a texture from straight-alpha RGBA8 pixels. */
    static Texture FromStraight(std::uint32_t width, std::uint32_t height,
                                const std::uint8_t *pixels) {
        miu_texture_t *handle = nullptr;
        check(miu_texture_create_straight(width, height, pixels, &handle), "texture create straight");
        return Texture(handle);
    }

    /** Decodes PNG, BMP, TGA, or QOI bytes. */
    static Texture Load(const std::uint8_t *bytes, std::size_t length) {
        miu_texture_t *handle = nullptr;
        check(miu_texture_load(bytes, length, &handle), "texture load");
        return Texture(handle);
    }

    /** Decodes an image from a byte vector. */
    static Texture Load(const std::vector<std::uint8_t> &bytes) {
        return Load(bytes.data(), bytes.size());
    }

    /** Raw handle, for direct C calls. */
    miu_texture_t *get() const noexcept { return handle_; }
    /** Whether the wrapper owns a handle. */
    bool valid() const noexcept { return handle_ != nullptr; }

    /** Size in pixels. */
    Vec2 size() const {
        std::uint32_t width = 0;
        std::uint32_t height = 0;
        check(miu_texture_size(handle_, &width, &height), "texture size");
        return Vec2{static_cast<float>(width), static_cast<float>(height)};
    }

    /** Copies premultiplied RGBA8 pixels out of the texture. */
    std::vector<std::uint8_t> pixels() const {
        const Vec2 extent = size();
        std::vector<std::uint8_t> buffer(static_cast<std::size_t>(extent.x) *
                                         static_cast<std::size_t>(extent.y) * 4u);
        check(miu_texture_read(handle_, buffer.data(), buffer.size()), "texture read");
        return buffer;
    }

    /** Releases the handle early. */
    void reset() noexcept {
        if (handle_) {
            miu_texture_release(handle_);
            handle_ = nullptr;
        }
    }

private:
    miu_texture_t *handle_;
};

/** Owning font handle. */
class Font {
public:
    /** Wraps an existing handle; the wrapper takes ownership. */
    explicit Font(miu_font_t *handle = nullptr) noexcept : handle_(handle) {}

    ~Font() { reset(); }

    Font(Font &&other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Font &operator=(Font &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    Font(const Font &) = delete;
    Font &operator=(const Font &) = delete;

    /** Parses a TrueType font from memory. */
    static Font Load(const std::uint8_t *bytes, std::size_t length) {
        miu_font_t *handle = nullptr;
        check(miu_font_load(bytes, length, &handle), "font load");
        return Font(handle);
    }

    /** Parses a TrueType font from a byte vector. */
    static Font Load(const std::vector<std::uint8_t> &bytes) {
        return Load(bytes.data(), bytes.size());
    }

    /** Raw handle, for direct C calls. */
    miu_font_t *get() const noexcept { return handle_; }
    /** Whether the wrapper owns a handle. */
    bool valid() const noexcept { return handle_ != nullptr; }

    /** Number of mapped glyphs. */
    std::uint32_t glyph_count() const { return miu_font_glyph_count(handle_); }

    /** Line height, ascent, and descent at `font_size`, measured downward. */
    void metrics(float font_size, float &line_height, float &ascent, float &descent) const {
        check(miu_font_metrics(handle_, font_size, &line_height, &ascent, &descent), "font metrics");
    }

    /** Lays text out and returns its block size. */
    Vec2 measure(std::string_view text, const TextStyle &style,
                 float max_width = 0.0f) const {
        Vec2 size{0.0f, 0.0f};
        check(miu_font_measure(handle_, text.data(), text.size(), &style,
                              max_width > 0.0f ? 1 : 0, max_width, &size),
              "font measure");
        return size;
    }

    /** Releases the handle early. */
    void reset() noexcept {
        if (handle_) {
            miu_font_release(handle_);
            handle_ = nullptr;
        }
    }

private:
    miu_font_t *handle_;
};

/** Selected render plan. */
struct Plan {
    /** API the bridge must create. */
    Backend backend = Backend::Vulkan12;
    /** API profile that implied the tier. */
    Backend profile_backend = Backend::Vulkan12;
    /** Selected tier. */
    Tier tier = Tier::Compatibility;
    /** Preset after clamping to the tier. */
    Preset preset = Preset::Medium;
    /** The plan uses an emergency API. */
    bool emergency = false;
    /** The plan is below the project's declared minimum tier. */
    bool below_minimum = false;

    /**
     * Asks the engine for a plan.
     *
     * `backends` lists the APIs the platform bridge probed successfully; passing an
     * empty vector uses the platform's full catalogue. Miu prefers the host platform
     * over a generic browser profile, so a WebAssembly build should still report the
     * real operating system when it can detect it.
     */
    static Plan Select(Platform platform, const std::vector<Backend> &backends, Tier minimum,
                       Tier preferred, Preset preset, bool best_effort = false) {
        std::vector<std::uint8_t> raw;
        raw.reserve(backends.size());
        for (Backend backend : backends) {
            raw.push_back(static_cast<std::uint8_t>(backend));
        }
        miu_render_request_t request{};
        request.minimum_tier = static_cast<std::uint8_t>(minimum);
        request.preferred_tier = static_cast<std::uint8_t>(preferred);
        request.preset = static_cast<std::uint8_t>(preset);
        request.best_effort = best_effort ? 1 : 0;
        miu_render_plan_t value{};
        check(miu_select_renderer(static_cast<std::uint8_t>(platform),
                                  raw.empty() ? nullptr : raw.data(), raw.size(), &request, &value),
              "select renderer");
        Plan plan;
        plan.backend = static_cast<Backend>(value.backend);
        plan.profile_backend = static_cast<Backend>(value.profile_backend);
        plan.tier = static_cast<Tier>(value.tier);
        plan.preset = static_cast<Preset>(value.preset);
        plan.emergency = (value.flags & MIU_PLAN_FLAG_EMERGENCY) != 0;
        plan.below_minimum = (value.flags & MIU_PLAN_FLAG_BELOW_MINIMUM) != 0;
        return plan;
    }

    /** Warning text to show before running on this plan, empty when none is needed. */
    std::string warning() const {
        miu_render_plan_t value{};
        value.backend = static_cast<std::uint8_t>(backend);
        value.profile_backend = static_cast<std::uint8_t>(profile_backend);
        value.tier = static_cast<std::uint8_t>(tier);
        value.preset = static_cast<std::uint8_t>(preset);
        value.flags = (emergency ? MIU_PLAN_FLAG_EMERGENCY : 0u) |
                      (below_minimum ? MIU_PLAN_FLAG_BELOW_MINIMUM : 0u);
        const char *text = miu_plan_warning(&value);
        return text ? std::string(text) : std::string();
    }
};

/** Owning software renderer. */
class Renderer {
public:
    /** Creates a renderer with a framebuffer of `width x height` pixels. */
    Renderer(std::uint32_t width, std::uint32_t height, Preset preset = Preset::High) {
        check(miu_renderer_create(width, height, static_cast<std::uint8_t>(preset), &handle_),
              "renderer create");
    }

    ~Renderer() { reset(); }

    Renderer(Renderer &&other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Renderer &operator=(Renderer &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    Renderer(const Renderer &) = delete;
    Renderer &operator=(const Renderer &) = delete;

    /** Raw handle, for direct C calls. */
    miu_renderer_t *get() noexcept { return handle_; }

    /** Framebuffer size in pixels. */
    Vec2 size() const {
        std::uint32_t width = 0;
        std::uint32_t height = 0;
        check(miu_renderer_size(handle_, &width, &height), "renderer size");
        return Vec2{static_cast<float>(width), static_cast<float>(height)};
    }

    /** Resizes the framebuffer, discarding its contents. */
    void resize(std::uint32_t width, std::uint32_t height) {
        check(miu_renderer_resize(handle_, width, height), "renderer resize");
    }

    /** Changes the quality preset. */
    void set_preset(Preset preset) {
        check(miu_renderer_set_preset(handle_, static_cast<std::uint8_t>(preset)), "renderer preset");
    }

    /** Uploads a texture under `id`. */
    void upload(std::uint64_t id, const Texture &texture) {
        check(miu_renderer_upload_texture(handle_, id, texture.get()), "upload texture");
    }

    /** Uploads a font under `id`. */
    void upload(std::uint64_t id, const Font &font) {
        check(miu_renderer_upload_font(handle_, id, font.get()), "upload font");
    }

    /** Sets the scissor rectangle in framebuffer pixels. */
    void set_clip(const Rect &clip) {
        check(miu_renderer_set_clip(handle_, 1, clip), "set clip");
    }

    /** Clears the clip rectangle. */
    void clear_clip() { check(miu_renderer_set_clip(handle_, 0, Rect()), "clear clip"); }

    /** Starts a frame and clears to `clear`. */
    void begin_frame(Rgba8 clear = Rgba8{0, 0, 0, 0}) {
        check(miu_renderer_begin_frame(handle_, 1, clear), "begin frame");
    }

    /** Starts a frame without clearing. */
    void begin_frame_no_clear() { check(miu_renderer_begin_frame(handle_, 0, Rgba8{0, 0, 0, 0}), "begin frame"); }

    /** Draws one sprite. */
    void draw_sprite(const Camera &camera, const miu_sprite_t &sprite) {
        check(miu_renderer_draw_sprite(handle_, &camera, &sprite), "draw sprite");
    }

    /** Fills a rectangle in world space. */
    void fill_rect(const Camera &camera, const Rect &rect, const Color &color,
                   Blend blend = Blend::SourceOver) {
        check(miu_renderer_fill_rect(handle_, &camera, rect, color,
                                    static_cast<std::uint8_t>(blend)),
              "fill rect");
    }

    /** Draws UTF-8 text positioned by `transform`. */
    void draw_text(const Camera &camera, std::uint64_t font_id, std::string_view text,
                   const TextStyle &style, const Color &color,
                   Transform2D transform = miu_transform2d_identity(),
                   Blend blend = Blend::SourceOver, float max_width = 0.0f) {
        check(miu_renderer_draw_text(handle_, &camera, font_id, text.data(), text.size(), &style,
                                    color, transform, static_cast<std::uint8_t>(blend),
                                    max_width > 0.0f ? 1 : 0, max_width),
              "draw text");
    }

    /** Statistics accumulated since the last `begin_frame`. */
    miu_render_stats_t stats() const {
        miu_render_stats_t value{};
        check(miu_renderer_stats(handle_, &value), "renderer stats");
        return value;
    }

    /** Copies the framebuffer as premultiplied RGBA8. */
    std::vector<std::uint8_t> read_pixels() const {
        const Vec2 extent = size();
        std::vector<std::uint8_t> buffer(static_cast<std::size_t>(extent.x) *
                                         static_cast<std::size_t>(extent.y) * 4u);
        check(miu_renderer_read_pixels(handle_, buffer.data(), buffer.size()), "read pixels");
        return buffer;
    }

    /** Encodes the framebuffer as a PNG file. */
    std::vector<std::uint8_t> encode_png() const {
        std::size_t length = 0;
        miu_status_t probe = miu_renderer_encode_png(handle_, nullptr, 0, &length);
        if (probe != MIU_STATUS_LIMIT_EXCEEDED && probe != MIU_STATUS_INVALID_ARGUMENT &&
            probe != MIU_STATUS_OK) {
            throw Error(probe, "encode png (size query)");
        }
        std::vector<std::uint8_t> buffer(length);
        check(miu_renderer_encode_png(handle_, buffer.data(), buffer.size(), &length), "encode png");
        buffer.resize(length);
        return buffer;
    }

    /** Releases the handle early. */
    void reset() noexcept {
        if (handle_) {
            miu_renderer_release(handle_);
            handle_ = nullptr;
        }
    }

private:
    miu_renderer_t *handle_ = nullptr;
};

/** Owning scene graph. */
class Scene {
public:
    /** Node identifier; `0` is the root. */
    using NodeId = std::uint32_t;

    /** Creates an empty scene framed to `width x height`. */
    Scene(float width, float height) {
        check(miu_scene_create(width, height, &handle_), "scene create");
    }

    /** Wraps an existing handle; the wrapper takes ownership. */
    explicit Scene(miu_scene_t *handle) noexcept : handle_(handle) {}

    ~Scene() { reset(); }

    Scene(Scene &&other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Scene &operator=(Scene &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    Scene(const Scene &) = delete;
    Scene &operator=(const Scene &) = delete;

    /** Raw handle, for direct C calls. */
    miu_scene_t *get() noexcept { return handle_; }

    /** Copies the scene's decoded assets into a renderer. */
    void attach_to(Renderer &renderer) const {
        check(miu_scene_attach(handle_, renderer.get()), "scene attach");
    }

    /** Sets the clear colour. */
    void set_background(Rgba8 color) { check(miu_scene_set_background(handle_, 1, color), "scene background"); }

    /** Clears the background. */
    void clear_background() {
        check(miu_scene_set_background(handle_, 0, Rgba8{0, 0, 0, 0}), "scene background");
    }

    /** Replaces the camera. */
    void set_camera(const Camera &camera) { check(miu_scene_set_camera(handle_, &camera), "scene camera"); }

    /** Adds a group node. */
    NodeId add_group(NodeId parent, std::string_view name, Vec2 position = Vec2{0.0f, 0.0f}) {
        NodeId id = 0;
        check(miu_scene_add_group(handle_, parent, name.empty() ? nullptr : std::string(name).c_str(),
                                  position, &id),
              "scene add group");
        return id;
    }

    /** Adds a sprite node. */
    NodeId add_sprite(NodeId parent, std::string_view name, std::uint64_t texture, Vec2 size,
                      Vec2 pivot, Vec2 position, const Color &color = colors::White(),
                      Blend blend = Blend::SourceOver, std::int32_t layer = 0) {
        NodeId id = 0;
        check(miu_scene_add_sprite(handle_, parent, name.empty() ? nullptr : std::string(name).c_str(),
                                   texture, size, pivot, position, color,
                                   static_cast<std::uint8_t>(blend), layer, &id),
              "scene add sprite");
        return id;
    }

    /** Adds a text node. */
    NodeId add_text(NodeId parent, std::string_view name, std::uint64_t font, std::string_view text,
                    const TextStyle &style, Vec2 position,
                    const Color &color = colors::White(), std::int32_t layer = 0) {
        NodeId id = 0;
        check(miu_scene_add_text(handle_, parent, name.empty() ? nullptr : std::string(name).c_str(),
                                 font, text.data(), text.size(), &style, color, position, layer, &id),
              "scene add text");
        return id;
    }

    /** Adds a filled rectangle node. */
    NodeId add_rect(NodeId parent, std::string_view name, const Rect &rect, const Color &color,
                    std::int32_t layer = 0) {
        NodeId id = 0;
        check(miu_scene_add_rect(handle_, parent, name.empty() ? nullptr : std::string(name).c_str(),
                                 rect, color, layer, &id),
              "scene add rect");
        return id;
    }

    /** Replaces a node's transform. */
    void set_transform(NodeId node, const Transform2D &transform) {
        check(miu_scene_set_transform(handle_, node, &transform), "scene transform");
    }

    /** Shows or hides a node. */
    void set_visible(NodeId node, bool visible) {
        check(miu_scene_set_visible(handle_, node, visible ? 1 : 0), "scene visibility");
    }

    /** Sets a node's opacity. */
    void set_opacity(NodeId node, float opacity) {
        check(miu_scene_set_opacity(handle_, node, opacity), "scene opacity");
    }

    /** Sets a node's painter layer. */
    void set_layer(NodeId node, std::int32_t layer) {
        check(miu_scene_set_layer(handle_, node, layer), "scene layer");
    }

    /** Removes a node and its subtree. */
    void remove(NodeId node) { check(miu_scene_remove(handle_, node), "scene remove"); }

    /** Node count including the root. */
    std::uint32_t node_count() const {
        std::uint32_t count = 0;
        check(miu_scene_node_count(handle_, &count), "scene node count");
        return count;
    }

    /** Topmost node containing a world-space point, if any. */
    bool hit_test(Vec2 world, NodeId &out_id) const {
        miu_status_t status = miu_scene_hit_test(handle_, world, &out_id);
        if (status == MIU_STATUS_NOT_FOUND) {
            return false;
        }
        check(status, "scene hit test");
        return true;
    }

    /** Number of animations the scene carries. */
    std::uint32_t animation_count() const {
        std::uint32_t count = 0;
        check(miu_scene_animation_count(handle_, &count), "scene animation count");
        return count;
    }

    /** Applies every animation at an absolute time in seconds. */
    void update(float time_seconds) {
        check(miu_scene_update(handle_, time_seconds), "scene update");
    }

    /** Draws the scene with its own camera, background, and assets. */
    void draw(Renderer &renderer) const { check(miu_scene_draw(handle_, renderer.get()), "scene draw"); }

    /** Releases the handle early. */
    void reset() noexcept {
        if (handle_) {
            miu_scene_release(handle_);
            handle_ = nullptr;
        }
    }

private:
    miu_scene_t *handle_ = nullptr;
};

/** Owning scene document. */
class Document {
public:
    /** Parses a JSON document. */
    static Document Parse(std::string_view json) {
        miu_document_t *handle = nullptr;
        check(miu_document_parse(reinterpret_cast<const std::uint8_t *>(json.data()), json.size(),
                                 &handle),
              "document parse");
        return Document(handle);
    }

    /** Wraps an existing handle; the wrapper takes ownership. */
    explicit Document(miu_document_t *handle) noexcept : handle_(handle) {}

    ~Document() { reset(); }

    Document(Document &&other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Document &operator=(Document &&other) noexcept {
        if (this != &other) {
            reset();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    Document(const Document &) = delete;
    Document &operator=(const Document &) = delete;

    /** Raw handle, for direct C calls. */
    miu_document_t *get() noexcept { return handle_; }

    /** Authored viewport size. */
    Vec2 viewport() const {
        Vec2 size{0.0f, 0.0f};
        check(miu_document_viewport(handle_, &size), "document viewport");
        return size;
    }

    /** Declared asset paths, in document order. */
    std::vector<std::string> asset_paths() const {
        std::uint32_t count = 0;
        check(miu_document_asset_count(handle_, &count), "document assets");
        std::vector<std::string> paths;
        paths.reserve(count);
        for (std::uint32_t index = 0; index < count; ++index) {
            std::size_t length = 0;
            miu_document_asset_path(handle_, index, nullptr, 0, &length);
            std::string path(length + 1, '\0');
            check(miu_document_asset_path(handle_, index, path.data(), path.size(), &length),
                  "document asset path");
            path.resize(length);
            paths.push_back(std::move(path));
        }
        return paths;
    }

    /** Supplies a decoded texture for an asset path. */
    void set_texture(std::string_view path, const Texture &texture) {
        check(miu_document_set_texture(handle_, std::string(path).c_str(), texture.get()),
              "document texture");
    }

    /** Supplies a parsed font for an asset path. */
    void set_font(std::string_view path, const Font &font) {
        check(miu_document_set_font(handle_, std::string(path).c_str(), font.get()), "document font");
    }

    /** Builds the scene from the supplied assets. */
    Scene instantiate() const {
        miu_scene_t *handle = nullptr;
        check(miu_document_instantiate(handle_, &handle), "document instantiate");
        return Scene(handle);
    }

    /** Releases the handle early. */
    void reset() noexcept {
        if (handle_) {
            miu_document_release(handle_);
            handle_ = nullptr;
        }
    }

private:
    miu_document_t *handle_ = nullptr;
};

} // namespace miu

#endif // MIU_MIU_HPP

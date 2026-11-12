// Platform bridge: turns a render plan into a device, with a CPU fallback.
//
// This translation unit is portable by design. It contains the backend catalogue,
// the factory registry, and the software device. Platform-specific files register
// real GPU factories; see the header for the list.

#include "miu/miu_bridge.h"

#include <algorithm>
#include <array>
#include <utility>

namespace miu::bridge {
namespace {

constexpr std::size_t kBackendSlots = 18;

std::array<Factory, kBackendSlots> &factories() {
    static std::array<Factory, kBackendSlots> table{};
    return table;
}

std::size_t slot(Backend backend) {
    return static_cast<std::size_t>(backend);
}

// Backends that belong to each platform catalogue, mirroring the Rust policy table.
const std::vector<Backend> &catalogue(Platform platform) {
    static const std::vector<Backend> windows{
        Backend::Direct2D12, Backend::Direct3D113, Backend::Direct3D12, Backend::OpenGL33};
    static const std::vector<Backend> android{Backend::OpenGLES30, Backend::Vulkan12,
                                              Backend::Vulkan13, Backend::Vulkan14,
                                              Backend::OpenGLES20};
    static const std::vector<Backend> apple{Backend::Metal2, Backend::Metal3, Backend::Metal1};
    static const std::vector<Backend> linux{Backend::Vulkan11, Backend::Vulkan12, Backend::Vulkan13,
                                            Backend::OpenGL30};
    static const std::vector<Backend> web{Backend::WebGL2, Backend::WebGPU, Backend::WebGL1};
    switch (platform) {
    case Platform::Windows:
        return windows;
    case Platform::Android:
        return android;
    case Platform::MacOS:
    case Platform::IOS:
        return apple;
    case Platform::Linux:
        return linux;
    case Platform::Web:
        return web;
    }
    return linux;
}

bool is_emergency(Backend backend) {
    switch (backend) {
    case Backend::OpenGL33:
    case Backend::OpenGL30:
    case Backend::OpenGLES20:
    case Backend::Metal1:
    case Backend::WebGL1:
        return true;
    default:
        return false;
    }
}

} // namespace

bool register_factory(Backend backend, Factory factory) {
    const std::size_t index = slot(backend);
    if (index >= kBackendSlots || factory == nullptr) {
        return false;
    }
    if (factories()[index] != nullptr) {
        return false;
    }
    factories()[index] = factory;
    return true;
}

Factory factory_for(Backend backend) {
    const std::size_t index = slot(backend);
    return index < kBackendSlots ? factories()[index] : nullptr;
}

const char *backend_requirement(Backend backend) {
    switch (backend) {
    case Backend::Direct2D12:
        return "Windows 8.1 or newer with the Direct2D 1.2 runtime";
    case Backend::Direct3D113:
        return "Windows 7 or newer with the Direct3D 11.3 runtime";
    case Backend::Direct3D12:
        return "Windows 10 or newer with a Direct3D 12 feature level 11_0 device";
    case Backend::OpenGLES30:
        return "OpenGL ES 3.0 driver";
    case Backend::OpenGLES20:
        return "OpenGL ES 2.0 driver (emergency fallback)";
    case Backend::Metal2:
        return "Apple A8 or newer, macOS 10.13 or iOS 11";
    case Backend::Metal3:
        return "Apple A13 or newer, macOS 13 or iOS 16";
    case Backend::Metal1:
        return "Metal 1 capable Apple GPU (emergency fallback)";
    case Backend::Vulkan11:
        return "Vulkan 1.1 loader and device";
    case Backend::Vulkan12:
        return "Vulkan 1.2 loader and device";
    case Backend::Vulkan13:
        return "Vulkan 1.3 loader and device";
    case Backend::Vulkan14:
        return "Vulkan 1.4 loader and device";
    case Backend::OpenGL33:
        return "desktop OpenGL 3.3 driver (emergency fallback)";
    case Backend::OpenGL30:
        return "desktop OpenGL 3.0 driver (emergency fallback)";
    case Backend::WebGL2:
        return "a browser with WebGL 2 enabled";
    case Backend::WebGL1:
        return "a browser with WebGL 1 only (emergency fallback)";
    case Backend::WebGPU:
        return "a browser with WebGPU enabled";
    }
    return "unknown backend";
}

bool backend_supported_on(Backend backend, Platform platform) {
    const auto &list = catalogue(platform);
    return std::find(list.begin(), list.end(), backend) != list.end();
}

std::vector<Backend> platform_backends(Platform platform) {
    return catalogue(platform);
}

/* ------------------------------------------------------------------------- */
/* Software device                                                           */
/* ------------------------------------------------------------------------- */

namespace {

/**
 * Device backed by the reference renderer in `miu-graphics`.
 *
 * It goes through the C ABI rather than calling Rust directly, so the CPU path
 * exercises exactly the same surface a third-party binding uses.
 */
class SoftwareDevice final : public Device {
public:
    explicit SoftwareDevice(const SurfaceConfig &config)
        : width_(config.width == 0 ? 1 : config.width),
          height_(config.height == 0 ? 1 : config.height), backend_(config.backend),
          preset_(config.preset) {}

    ~SoftwareDevice() override { Release(); }

    /** Creates the underlying renderer; returns a status instead of throwing. */
    miu_status_t Open() {
        if (renderer_ != nullptr) {
            return MIU_STATUS_OK;
        }
        return miu_renderer_create(width_, height_, static_cast<std::uint8_t>(preset_), &renderer_);
    }

    void Release() noexcept {
        if (renderer_ != nullptr) {
            miu_renderer_release(renderer_);
            renderer_ = nullptr;
        }
    }

    Backend backend() const override { return backend_; }
    Vec2 size() const override { return Vec2{static_cast<float>(width_), static_cast<float>(height_)}; }
    bool is_software() const override { return true; }

    miu_status_t resize(std::uint32_t width, std::uint32_t height) override {
        if (renderer_ == nullptr || width == 0 || height == 0) {
            return MIU_STATUS_INVALID_ARGUMENT;
        }
        const miu_status_t status = miu_renderer_resize(renderer_, width, height);
        if (status == MIU_STATUS_OK) {
            width_ = width;
            height_ = height;
        }
        return status;
    }

    miu_status_t begin_frame(bool clear, Rgba8 color) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_begin_frame(renderer_, clear ? 1 : 0, color);
    }

    miu_status_t upload_texture(std::uint64_t id, const Texture &texture) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_upload_texture(renderer_, id, texture.get());
    }

    miu_status_t upload_font(std::uint64_t id, const Font &font) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_upload_font(renderer_, id, font.get());
    }

    miu_status_t draw_sprite(const Camera &camera, const miu_sprite_t &sprite) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_draw_sprite(renderer_, &camera, &sprite);
    }

    miu_status_t fill_rect(const Camera &camera, const Rect &rect, const Color &color,
                           Blend blend) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_fill_rect(renderer_, &camera, rect, color,
                                      static_cast<std::uint8_t>(blend));
    }

    miu_status_t draw_text(const Camera &camera, std::uint64_t font_id, std::string_view text,
                           const TextStyle &style, const Color &color, Transform2D transform,
                           Blend blend, float max_width) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        return miu_renderer_draw_text(renderer_, &camera, font_id, text.data(), text.size(), &style,
                                      color, transform, static_cast<std::uint8_t>(blend),
                                      max_width > 0.0f ? 1 : 0, max_width);
    }

    miu_status_t end_frame() override { return renderer_ ? MIU_STATUS_OK : MIU_STATUS_INVALID_STATE; }

    miu_status_t read_pixels(std::vector<std::uint8_t> &out_pixels) override {
        if (renderer_ == nullptr) {
            return MIU_STATUS_INVALID_STATE;
        }
        out_pixels.assign(static_cast<std::size_t>(width_) * height_ * 4u, 0);
        return miu_renderer_read_pixels(renderer_, out_pixels.data(), out_pixels.size());
    }

    miu_render_stats_t stats() const override {
        miu_render_stats_t value{};
        if (renderer_ != nullptr) {
            miu_renderer_stats(renderer_, &value);
        }
        return value;
    }

private:
    std::uint32_t width_;
    std::uint32_t height_;
    Backend backend_;
    Preset preset_;
    miu_renderer_t *renderer_ = nullptr;
};

} // namespace

std::unique_ptr<Device> make_software_device(const SurfaceConfig &config) {
    auto device = std::make_unique<SoftwareDevice>(config);
    if (device->Open() != MIU_STATUS_OK) {
        return nullptr;
    }
    return device;
}

std::unique_ptr<Device> create_device(const SurfaceConfig &config, std::string &reason) {
    reason.clear();
    if (!config.allow_emergency && is_emergency(config.backend)) {
        reason = std::string("backend ") + miu_backend_name(static_cast<std::uint8_t>(config.backend)) +
                 " is an emergency API and was not authorised";
        return nullptr;
    }
    if (config.width == 0 || config.height == 0) {
        reason = "surface size must be non-zero";
        return nullptr;
    }
    if (Factory factory = factory_for(config.backend)) {
        std::unique_ptr<Device> device;
        const miu_status_t status = factory(config, device);
        if (status == MIU_STATUS_OK && device) {
            return device;
        }
        reason = std::string("backend ") + miu_backend_name(static_cast<std::uint8_t>(config.backend)) +
                 " factory failed: " + miu::status_message(status);
    } else {
        reason = std::string("no factory linked for ") +
                 miu_backend_name(static_cast<std::uint8_t>(config.backend)) + " (" +
                 backend_requirement(config.backend) + "); using the CPU renderer";
    }
    return make_software_device(config);
}

/* ------------------------------------------------------------------------- */
/* Session                                                                   */
/* ------------------------------------------------------------------------- */

Session Session::Open(Platform platform, const std::vector<Backend> &probed, Tier minimum,
                      Tier preferred, Preset preset, const SurfaceConfig &surface,
                      bool best_effort) {
    Session session;
    std::vector<std::uint8_t> probed_bytes;
    probed_bytes.reserve(probed.size());
    for (Backend backend : probed) {
        probed_bytes.push_back(static_cast<std::uint8_t>(backend));
    }

    miu_render_request_t request{};
    request.minimum_tier = static_cast<std::uint8_t>(minimum);
    request.preferred_tier = static_cast<std::uint8_t>(preferred);
    request.preset = static_cast<std::uint8_t>(preset);
    request.best_effort = best_effort ? 1 : 0;

    miu_render_plan_t plan{};
    const miu_status_t status =
        miu_select_renderer(static_cast<std::uint8_t>(platform),
                            probed_bytes.empty() ? nullptr : probed_bytes.data(),
                            probed_bytes.size(), &request, &plan);
    if (status != MIU_STATUS_OK) {
        session.reason_ = std::string("no graphics API satisfies the requested tier: ") +
                          miu::status_message(status);
        session.warning_ = session.reason_;
        return session;
    }

    if (const char *warning = miu_plan_warning(&plan)) {
        session.warning_ = warning;
    }

    SurfaceConfig config = surface;
    config.backend = static_cast<Backend>(plan.backend);
    config.preset = static_cast<Preset>(plan.preset);
    config.allow_emergency = best_effort || config.allow_emergency;

    session.backend_ = config.backend;
    session.device_ = create_device(config, session.reason_);
    return session;
}

} // namespace miu::bridge

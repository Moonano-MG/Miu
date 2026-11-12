// Miu platform bridge — C++17 compatibility layer between the Rust engine and a
// host's windowing/graphics stack.
//
// Responsibilities
// ----------------
// * Translate a `miu_render_plan_t` (chosen by the Rust policy code) into a device.
// * Own the per-backend factories. Shipping a new API means registering a factory;
//   nothing in Rust or in the public C ABI changes.
// * Report precisely why a backend is unavailable, so the player sees "your driver
//   does not expose Vulkan 1.2" instead of a generic failure.
//
// The bridge deliberately contains no platform SDK includes. A Windows build adds
// `bridge_d3d11.cpp`, `bridge_d3d12.cpp`, and `bridge_d2d.cpp`; an Apple build adds
// `bridge_metal.mm`; the Linux and Android builds add `bridge_vulkan.cpp` and
// `bridge_gles.cpp`. Each new translation unit calls `register_factory` from a
// static initialiser. When no GPU factory is linked, the bridge still runs: it falls
// back to the CPU device, which drives the reference renderer through the C ABI.

#ifndef MIU_MIU_BRIDGE_H
#define MIU_MIU_BRIDGE_H

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "miu/miu.hpp"

namespace miu::bridge {

/** A created device: one graphics API instance attached to one surface. */
class Device {
public:
    virtual ~Device() = default;

    /** Backend this device implements. */
    virtual Backend backend() const = 0;

    /** Surface size in pixels. */
    virtual Vec2 size() const = 0;

    /** Resizes the surface, discarding its contents. */
    virtual miu_status_t resize(std::uint32_t width, std::uint32_t height) = 0;

    /** Begins a frame and optionally clears it. */
    virtual miu_status_t begin_frame(bool clear, Rgba8 color) = 0;

    /** Uploads or replaces a texture under `id`. */
    virtual miu_status_t upload_texture(std::uint64_t id, const Texture &texture) = 0;

    /** Uploads or replaces a font under `id`. */
    virtual miu_status_t upload_font(std::uint64_t id, const Font &font) = 0;

    /** Draws one sprite. */
    virtual miu_status_t draw_sprite(const Camera &camera, const miu_sprite_t &sprite) = 0;

    /** Fills a rectangle in world space. */
    virtual miu_status_t fill_rect(const Camera &camera, const Rect &rect, const Color &color,
                                   Blend blend) = 0;

    /** Draws text. */
    virtual miu_status_t draw_text(const Camera &camera, std::uint64_t font_id,
                                   std::string_view text, const TextStyle &style,
                                   const Color &color, Transform2D transform, Blend blend,
                                   float max_width) = 0;

    /** Ends the frame and presents it. */
    virtual miu_status_t end_frame() = 0;

    /** Reads back the presented frame as premultiplied RGBA8, when supported. */
    virtual miu_status_t read_pixels(std::vector<std::uint8_t> &out_pixels) {
        out_pixels.clear();
        return MIU_STATUS_UNSUPPORTED;
    }

    /** Statistics for the frame in progress or most recently ended. */
    virtual miu_render_stats_t stats() const = 0;

    /** Whether the device renders on the CPU instead of a graphics API. */
    virtual bool is_software() const { return false; }
};

/** Context a factory needs to create a device. */
struct SurfaceConfig {
    /** Native window handle: `HWND`, `NSView*`, `ANativeWindow*`, `xcb_window_t`, ... */
    void *window = nullptr;
    /** Render target size in pixels. */
    std::uint32_t width = 0;
    /** Render target size in pixels. */
    std::uint32_t height = 0;
    /** Requested quality preset, already clamped to the selected tier. */
    Preset preset = Preset::Medium;
    /** API the device must implement. */
    Backend backend = Backend::Vulkan12;
    /** Allows the emergency APIs; the host asks the player first. */
    bool allow_emergency = false;
};

/** Creates a device for a surface, or explains why it cannot. */
using Factory = miu_status_t (*)(const SurfaceConfig &config, std::unique_ptr<Device> &out_device);

/**
 * Registers the factory for one backend.
 *
 * Returns `false` when a factory was already registered for that backend, so a
 * static initialiser cannot silently replace another platform file's factory.
 */
bool register_factory(Backend backend, Factory factory);

/** Factory registered for `backend`, or `nullptr`. */
Factory factory_for(Backend backend);

/** Human-readable prerequisite of a backend, for diagnostics screens. */
const char *backend_requirement(Backend backend);

/** Whether `backend` belongs to `platform`'s catalogue. */
bool backend_supported_on(Backend backend, Platform platform);

/** Catalogue of `platform`, normal APIs first and emergency APIs last. */
std::vector<Backend> platform_backends(Platform platform);

/**
 * Probes the running device and returns the backends that are usable.
 *
 * The probe only inspects loaders and versions; it never creates a device, so it is
 * cheap enough to run during start-up and safe to run on the render thread. Results
 * are ordered strongest-first, which is the order the Rust policy expects. Pass them
 * to `Session::Open` as `probed`.
 *
 * Version checks match the catalogue: Vulkan needs 1.1/1.2/1.3/1.4 as appropriate,
 * OpenGL ES 3.0 for the compatibility tier, and so on. When a loader exists but the
 * version is too old, that backend is simply absent from the result, and
 * `probe_detail` explains why.
 */
std::vector<Backend> probe_backends(Platform platform);

/** Last probe's human-readable notes, one per backend considered. */
const std::vector<std::string> &probe_detail();

/**
 * Creates a device for a plan.
 *
 * Order of preference: a registered factory for the planned backend, then the CPU
 * device. The CPU device always succeeds, so a host can keep rendering while the
 * player fixes a driver problem. `reason` receives an explanation whenever the
 * planned backend could not be created.
 */
std::unique_ptr<Device> create_device(const SurfaceConfig &config, std::string &reason);

/** Plan and device owned by a host window. */
class Session {
public:
    /**
     * Plans a backend and opens a device.
     *
     * `request` is the project's declared minimum/preferred tier and preset.
     * `probed` lists the APIs the platform probe found working; pass an empty list
     * to trust the catalogue. `best_effort` permits emergency APIs and plans below
     * the declared minimum, which the host must present as a warning.
     */
    static Session Open(Platform platform, const std::vector<Backend> &probed, Tier minimum,
                        Tier preferred, Preset preset, const SurfaceConfig &surface,
                        bool best_effort);

    /** Backend actually in use. */
    Backend backend() const { return backend_; }

    /** Whether the device falls back to the CPU renderer. */
    bool software() const { return device_ && device_->is_software(); }

    /** Warning to show the player, empty when the plan meets the requirement. */
    const std::string &warning() const { return warning_; }

    /** Why the planned backend was not created, empty when it was. */
    const std::string &reason() const { return reason_; }

    /** The device, never null after a successful `Open`. */
    Device &device() { return *device_; }

    /** Whether a device is open. */
    explicit operator bool() const { return device_ != nullptr; }

    /** Releases the device. */
    void Close() { device_.reset(); }

private:
    std::unique_ptr<Device> device_;
    Backend backend_ = Backend::Vulkan12;
    std::string warning_;
    std::string reason_;
};

/** CPU device: runs the reference renderer through the C ABI. */
std::unique_ptr<Device> make_software_device(const SurfaceConfig &config);

} // namespace miu::bridge

#endif // MIU_MIU_BRIDGE_H

#ifndef MIU_MIU_HPP
#define MIU_MIU_HPP

#include "miu/miu.h"

#include <cstdint>
#include <limits>
#include <new>
#include <utility>
#include <vector>

namespace miu {

using Platform = miu_platform_t;
using Backend = miu_backend_t;
using RenderTier = miu_render_tier_t;

enum class Status : std::uint32_t {
    Ok = MIU_STATUS_OK,
    InvalidArgument = MIU_STATUS_INVALID_ARGUMENT,
    Unsupported = MIU_STATUS_UNSUPPORTED,
    OutOfMemory = MIU_STATUS_OUT_OF_MEMORY,
};

/** A value-owning capability report suitable for the C ABI. */
class CapabilityReport final {
public:
    explicit CapabilityReport(Platform platform) noexcept : platform_(platform) {}

    /** Adds a successfully probed backend. Duplicate values are ignored. */
    CapabilityReport& add(Backend backend) {
        for (Backend candidate : backends_) {
            if (candidate == backend) {
                return *this;
            }
        }
        backends_.push_back(backend);
        return *this;
    }

    [[nodiscard]] miu_device_capabilities c_value() const noexcept {
        miu_device_capabilities result{};
        result.abi_version = MIU_ABI_VERSION;
        result.platform = platform_;
        result.backends = backends_.empty() ? nullptr : backends_.data();
        result.backend_count = backends_.size();
        return result;
    }

private:
    Platform platform_;
    std::vector<Backend> backends_;
};

/** Project-level selection policy. Best effort is opt-in and must be warned about. */
struct RendererRequest final {
    RenderTier minimum_tier = MIU_RENDER_TIER_COMPATIBILITY;
    RenderTier preferred_tier = MIU_RENDER_TIER_COMPATIBILITY;
    bool allow_best_effort = false;

    [[nodiscard]] miu_renderer_request c_value() const noexcept {
        miu_renderer_request result{};
        result.abi_version = MIU_ABI_VERSION;
        result.minimum_tier = minimum_tier;
        result.preferred_tier = preferred_tier;
        result.allow_best_effort = allow_best_effort ? 1U : 0U;
        return result;
    }
};

/** Immutable backend selection returned by a live renderer policy handle. */
struct RenderPlan final {
    Backend backend{};
    Backend profile_backend{};
    RenderTier tier{};
    bool emergency_fallback = false;
    bool below_minimum = false;

    [[nodiscard]] bool requires_force_run_warning() const noexcept {
        return emergency_fallback || below_minimum;
    }
};

/** Move-only RAII owner for a Miu policy renderer handle.
 *
 * v0.1 selects and retains a render plan. After creation pass `plan()` to a
 * `miu::bridge::BackendBridge` implementation to create actual GPU resources.
 */
class Renderer final {
public:
    Renderer() noexcept = default;
    Renderer(const Renderer&) = delete;
    Renderer& operator=(const Renderer&) = delete;

    Renderer(Renderer&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}

    Renderer& operator=(Renderer&& other) noexcept {
        if (this != &other) {
            reset();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    ~Renderer() { reset(); }

    /** Selects a plan and returns an invalid renderer if selection fails. */
    [[nodiscard]] static Renderer create(
        const CapabilityReport& capabilities,
        const RendererRequest& request,
        Status* out_status = nullptr) noexcept {
        const miu_device_capabilities c_capabilities = capabilities.c_value();
        const miu_renderer_request c_request = request.c_value();
        miu_renderer* handle = nullptr;
        const auto raw_status = miu_renderer_create(&c_capabilities, &c_request, &handle);
        if (out_status != nullptr) {
            *out_status = static_cast<Status>(raw_status);
        }
        return Renderer(raw_status == MIU_STATUS_OK ? handle : nullptr);
    }

    [[nodiscard]] bool valid() const noexcept { return handle_ != nullptr; }

    /** Reads the immutable selection plan. Returns InvalidArgument for an empty owner. */
    [[nodiscard]] Status plan(RenderPlan* out_plan) const noexcept {
        if (handle_ == nullptr || out_plan == nullptr) {
            return Status::InvalidArgument;
        }
        miu_render_plan raw_plan{};
        const auto raw_status = miu_renderer_get_plan(handle_, &raw_plan);
        if (raw_status == MIU_STATUS_OK) {
            out_plan->backend = raw_plan.backend;
            out_plan->profile_backend = raw_plan.profile_backend;
            out_plan->tier = raw_plan.tier;
            out_plan->emergency_fallback = raw_plan.emergency_fallback != 0;
            out_plan->below_minimum = raw_plan.below_minimum != 0;
        }
        return static_cast<Status>(raw_status);
    }

    void reset() noexcept {
        miu_renderer_release(std::exchange(handle_, nullptr));
    }

private:
    explicit Renderer(miu_renderer* handle) noexcept : handle_(handle) {}
    miu_renderer* handle_ = nullptr;
};

/** Direct C++ aliases for CPU rasterizer descriptors in the stable C ABI. */
using CpuPixel = miu_rgba8;
using CpuCamera = miu_camera2d;
using CpuSprite = miu_cpu_sprite;

/** Move-only CPU texture used by the deterministic reference renderer. */
class CpuTexture final {
public:
    CpuTexture() noexcept = default;
    CpuTexture(const CpuTexture&) = delete;
    CpuTexture& operator=(const CpuTexture&) = delete;

    CpuTexture(CpuTexture&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}

    CpuTexture& operator=(CpuTexture&& other) noexcept {
        if (this != &other) {
            reset();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    ~CpuTexture() { reset(); }

    /** Creates a texture from tightly packed premultiplied RGBA8 pixels. */
    [[nodiscard]] static CpuTexture create(
        std::uint32_t width,
        std::uint32_t height,
        const std::vector<std::uint8_t>& rgba_bytes,
        Status* out_status = nullptr) noexcept {
        miu_cpu_texture* handle = nullptr;
        const auto raw_status = miu_cpu_texture_create_rgba8(
            width,
            height,
            rgba_bytes.empty() ? nullptr : rgba_bytes.data(),
            rgba_bytes.size(),
            &handle);
        if (out_status != nullptr) {
            *out_status = static_cast<Status>(raw_status);
        }
        return CpuTexture(raw_status == MIU_STATUS_OK ? handle : nullptr);
    }

    [[nodiscard]] bool valid() const noexcept { return handle_ != nullptr; }

    void reset() noexcept {
        miu_cpu_texture_release(std::exchange(handle_, nullptr));
    }

private:
    friend class CpuCanvas;
    explicit CpuTexture(miu_cpu_texture* handle) noexcept : handle_(handle) {}
    miu_cpu_texture* handle_ = nullptr;
};

/** Move-only CPU canvas for deterministic headless 2D rendering. */
class CpuCanvas final {
public:
    CpuCanvas() noexcept = default;
    CpuCanvas(const CpuCanvas&) = delete;
    CpuCanvas& operator=(const CpuCanvas&) = delete;

    CpuCanvas(CpuCanvas&& other) noexcept
        : handle_(std::exchange(other.handle_, nullptr)),
          width_(std::exchange(other.width_, 0U)),
          height_(std::exchange(other.height_, 0U)) {}

    CpuCanvas& operator=(CpuCanvas&& other) noexcept {
        if (this != &other) {
            reset();
            handle_ = std::exchange(other.handle_, nullptr);
            width_ = std::exchange(other.width_, 0U);
            height_ = std::exchange(other.height_, 0U);
        }
        return *this;
    }

    ~CpuCanvas() { reset(); }

    /** Allocates a transparent canvas of `width` by `height` logical pixels. */
    [[nodiscard]] static CpuCanvas create(
        std::uint32_t width,
        std::uint32_t height,
        Status* out_status = nullptr) noexcept {
        miu_cpu_canvas* handle = nullptr;
        const auto raw_status = miu_cpu_canvas_create(width, height, &handle);
        if (out_status != nullptr) {
            *out_status = static_cast<Status>(raw_status);
        }
        return CpuCanvas(
            raw_status == MIU_STATUS_OK ? handle : nullptr,
            raw_status == MIU_STATUS_OK ? width : 0U,
            raw_status == MIU_STATUS_OK ? height : 0U);
    }

    [[nodiscard]] bool valid() const noexcept { return handle_ != nullptr; }
    [[nodiscard]] std::uint32_t width() const noexcept { return width_; }
    [[nodiscard]] std::uint32_t height() const noexcept { return height_; }

    [[nodiscard]] Status clear(CpuPixel color) noexcept {
        if (handle_ == nullptr) {
            return Status::InvalidArgument;
        }
        return static_cast<Status>(miu_cpu_canvas_clear(handle_, color));
    }

    [[nodiscard]] Status draw(
        const CpuCamera& camera,
        const CpuSprite& sprite,
        const CpuTexture& texture) noexcept {
        if (handle_ == nullptr || !texture.valid()) {
            return Status::InvalidArgument;
        }
        return static_cast<Status>(
            miu_cpu_canvas_draw_sprite(handle_, &camera, &sprite, texture.handle_));
    }

    /**
     * Copies canvas pixels into a caller-owned tightly packed RGBA8 vector.
     *
     * Allocation failure is converted to `Status::OutOfMemory`; `out_pixels` is
     * cleared on any failure and never exposes an uninitialised snapshot.
     */
    [[nodiscard]] Status copy_rgba8(std::vector<std::uint8_t>* out_pixels) const noexcept {
        if (handle_ == nullptr || out_pixels == nullptr) {
            return Status::InvalidArgument;
        }
        constexpr auto max_size = (std::numeric_limits<std::size_t>::max)();
        if (height_ != 0U && static_cast<std::size_t>(width_) > max_size / static_cast<std::size_t>(height_)) {
            out_pixels->clear();
            return Status::OutOfMemory;
        }
        const auto pixels = static_cast<std::size_t>(width_) * static_cast<std::size_t>(height_);
        if (pixels > max_size / 4U) {
            out_pixels->clear();
            return Status::OutOfMemory;
        }
        const auto byte_count = pixels * 4U;
        try {
            out_pixels->resize(byte_count);
        } catch (const std::bad_alloc&) {
            out_pixels->clear();
            return Status::OutOfMemory;
        }
        const auto status = static_cast<Status>(miu_cpu_canvas_copy_rgba8(
            handle_, out_pixels->data(), out_pixels->size()));
        if (status != Status::Ok) {
            out_pixels->clear();
        }
        return status;
    }

    void reset() noexcept {
        miu_cpu_canvas_release(std::exchange(handle_, nullptr));
        width_ = 0U;
        height_ = 0U;
    }

private:
    CpuCanvas(miu_cpu_canvas* handle, std::uint32_t width, std::uint32_t height) noexcept
        : handle_(handle), width_(width), height_(height) {}

    miu_cpu_canvas* handle_ = nullptr;
    std::uint32_t width_ = 0U;
    std::uint32_t height_ = 0U;
};

}  // namespace miu

#endif  // MIU_MIU_HPP

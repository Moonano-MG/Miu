#ifndef MIU_MIU_HPP
#define MIU_MIU_HPP

#include "miu/miu.h"

#include <cstdint>
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

}  // namespace miu

#endif  // MIU_MIU_HPP

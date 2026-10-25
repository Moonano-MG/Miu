#ifndef MIU_MIU_BRIDGE_H
#define MIU_MIU_BRIDGE_H

/*
 * Platform bridge contract.
 *
 * The Rust core selects policy; this layer is where a host probes platform APIs
 * and adapts a selected plan to Direct2D/D3D, Vulkan, Metal, OpenGL, or browser
 * APIs. The default implementation deliberately does not claim hardware support:
 * add a backend only after the host has successfully probed it.
 */

#include "miu/miu.h"

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MIU_BRIDGE_MAX_BACKENDS 24U

/** Stack-owning capability list for probes that do not need heap allocation. */
typedef struct miu_bridge_capability_buffer {
    miu_platform_t platform;
    miu_backend_t backends[MIU_BRIDGE_MAX_BACKENDS];
    size_t backend_count;
} miu_bridge_capability_buffer;

/** Returns the compile-target platform. It never performs a GPU probe. */
MIU_API miu_platform_t miu_bridge_target_platform(void);

/** Initialises an empty report with the target platform or the provided platform. */
MIU_API void miu_bridge_capabilities_init(
    miu_bridge_capability_buffer* buffer,
    miu_platform_t platform);

/** Adds a backend that successfully passed a host-side probe.
 * Returns MIU_STATUS_OUT_OF_MEMORY if the fixed buffer is full, or
 * MIU_STATUS_INVALID_ARGUMENT for invalid inputs. Duplicate additions succeed.
 */
MIU_API miu_status_t miu_bridge_capabilities_add(
    miu_bridge_capability_buffer* buffer,
    miu_backend_t backend);

/** Creates a non-owning Miu ABI view. It remains valid while `buffer` is alive. */
MIU_API miu_device_capabilities miu_bridge_capabilities_view(
    const miu_bridge_capability_buffer* buffer);

#ifdef __cplusplus
} /* extern "C" */
#endif

#ifdef __cplusplus
namespace miu::bridge {

/**
 * Application integration point for graphics-device creation.
 *
 * `create_device` must return false if API creation fails even though capability
 * probing had succeeded. In that event, re-probe without the failed API and ask
 * Miu for a new plan; emergency fallback selection will remain visible in plan.
 */
class BackendBridge {
public:
    virtual ~BackendBridge() = default;
    virtual bool create_device(miu_backend_t backend) = 0;
    virtual void destroy_device() noexcept = 0;
};

}  // namespace miu::bridge
#endif

#endif /* MIU_MIU_BRIDGE_H */

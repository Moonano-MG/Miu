#include "miu/miu_bridge.h"

#if defined(__APPLE__)
#  include <TargetConditionals.h>
#endif

namespace {

bool is_platform(const miu_platform_t platform) noexcept {
    return platform >= MIU_PLATFORM_WINDOWS && platform <= MIU_PLATFORM_WEB;
}

bool is_backend(const miu_backend_t backend) noexcept {
    return backend >= MIU_BACKEND_DIRECT2D_1_2 && backend <= MIU_BACKEND_WEBGPU;
}

}  // namespace

extern "C" miu_platform_t miu_bridge_target_platform(void) {
#if defined(__ANDROID__)
    return MIU_PLATFORM_ANDROID;
#elif defined(__APPLE__)
#  if TARGET_OS_IPHONE
    return MIU_PLATFORM_IOS;
#  else
    return MIU_PLATFORM_MACOS;
#  endif
#elif defined(_WIN32)
    return MIU_PLATFORM_WINDOWS;
#elif defined(__EMSCRIPTEN__)
    return MIU_PLATFORM_WEB;
#else
    return MIU_PLATFORM_LINUX;
#endif
}

extern "C" void miu_bridge_capabilities_init(
    miu_bridge_capability_buffer* const buffer,
    const miu_platform_t platform) {
    if (buffer == nullptr) {
        return;
    }
    buffer->platform = is_platform(platform) ? platform : miu_bridge_target_platform();
    buffer->backend_count = 0U;
}

extern "C" miu_status_t miu_bridge_capabilities_add(
    miu_bridge_capability_buffer* const buffer,
    const miu_backend_t backend) {
    if (buffer == nullptr || !is_backend(backend)) {
        return MIU_STATUS_INVALID_ARGUMENT;
    }
    for (size_t index = 0U; index < buffer->backend_count; ++index) {
        if (buffer->backends[index] == backend) {
            return MIU_STATUS_OK;
        }
    }
    if (buffer->backend_count == MIU_BRIDGE_MAX_BACKENDS) {
        return MIU_STATUS_OUT_OF_MEMORY;
    }
    buffer->backends[buffer->backend_count++] = backend;
    return MIU_STATUS_OK;
}

extern "C" miu_device_capabilities miu_bridge_capabilities_view(
    const miu_bridge_capability_buffer* const buffer) {
    miu_device_capabilities view{};
    view.abi_version = MIU_ABI_VERSION;
    if (buffer == nullptr) {
        return view;
    }
    view.platform = buffer->platform;
    view.backends = buffer->backend_count == 0U ? nullptr : buffer->backends;
    view.backend_count = buffer->backend_count;
    return view;
}

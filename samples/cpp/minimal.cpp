#include "miu/miu.hpp"
#include "miu/miu_bridge.h"

int main() {
    // Real applications add only APIs that their platform-specific probe created.
    miu::CapabilityReport capabilities(MIU_PLATFORM_WINDOWS);
    capabilities.add(MIU_BACKEND_DIRECT3D_11_3).add(MIU_BACKEND_DIRECT3D_12);

    miu::RendererRequest request{};
    request.minimum_tier = MIU_RENDER_TIER_BALANCED;
    request.preferred_tier = MIU_RENDER_TIER_BALANCED;

    miu::Status status{};
    miu::Renderer renderer = miu::Renderer::create(capabilities, request, &status);
    if (status != miu::Status::Ok) {
        return 1;
    }

    miu::RenderPlan plan{};
    if (renderer.plan(&plan) != miu::Status::Ok) {
        return 2;
    }

    // Pass `plan.backend` to an application-specific miu::bridge::BackendBridge.
    return plan.requires_force_run_warning() ? 3 : 0;
}

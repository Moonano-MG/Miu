// Capability probe: reports which graphics APIs this machine can actually use.
//
// The probe never creates a device. It inspects the platform's loaders and asks for
// their versions, which is what makes it safe to run during start-up and fast enough
// to re-run when the player changes displays or drivers.
//
// Supported targets:
//   * Linux, BSD, Android — `dlopen` on the Vulkan/OpenGL loaders.
//   * Windows            — `LoadLibraryW` on the Direct3D, Direct2D, OpenGL, Vulkan DLLs.
//   * macOS, iOS         — Metal device query through the Metal framework.
// Every other target reports nothing, and the caller falls back to the CPU device.

#include "miu/miu_bridge.h"

#include <string>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#elif defined(__APPLE__)
#include <dlfcn.h>
#else
#include <dlfcn.h>
#endif

namespace miu::bridge {
namespace {

std::vector<std::string> &detail_storage() {
    static std::vector<std::string> notes;
    return notes;
}

void note(const std::string &text) { detail_storage().push_back(text); }

#if !defined(_WIN32) && !defined(__APPLE__)

/** Handle wrapper that closes the library on scope exit. */
class Library {
public:
    explicit Library(const char *name) : handle_(dlopen(name, RTLD_LAZY | RTLD_LOCAL)) {}
    ~Library() {
        if (handle_) {
            dlclose(handle_);
        }
    }
    Library(const Library &) = delete;
    Library &operator=(const Library &) = delete;

    bool valid() const { return handle_ != nullptr; }
    void *symbol(const char *name) const { return handle_ ? dlsym(handle_, name) : nullptr; }

private:
    void *handle_;
};

/** Reads the Vulkan loader's instance version; `0` when unavailable. */
std::uint32_t vulkan_instance_version(const Library &loader) {
    using EnumerateVersion = int (*)(std::uint32_t *);
    auto enumerate = reinterpret_cast<EnumerateVersion>(loader.symbol("vkEnumerateInstanceVersion"));
    if (enumerate != nullptr) {
        std::uint32_t version = 0;
        if (enumerate(&version) == 0) {
            return version;
        }
    }
    // Loaders older than Vulkan 1.1 always implement 1.0.
    return (1u << 22);
}

std::vector<Backend> probe_unix() {
    std::vector<Backend> found;
    {
        Library loader("libvulkan.so.1");
        if (!loader.valid()) {
            note("vulkan: libvulkan.so.1 not found");
        } else {
            const std::uint32_t version = vulkan_instance_version(loader);
            const std::uint32_t major = version >> 22;
            const std::uint32_t minor = (version >> 12) & 0x3ffu;
            note("vulkan: loader reports " + std::to_string(major) + "." + std::to_string(minor));
            if (major > 1 || (major == 1 && minor >= 4)) {
                found.push_back(Backend::Vulkan14);
            }
            if (major > 1 || (major == 1 && minor >= 3)) {
                found.push_back(Backend::Vulkan13);
            }
            if (major > 1 || (major == 1 && minor >= 2)) {
                found.push_back(Backend::Vulkan12);
            }
            if (major > 1 || (major == 1 && minor >= 1)) {
                found.push_back(Backend::Vulkan11);
            }
        }
    }
    {
        Library loader("libGL.so.1");
        if (!loader.valid()) {
            note("opengl: libGL.so.1 not found");
        } else if (loader.symbol("glXGetProcAddress") != nullptr ||
                   loader.symbol("glXGetProcAddressARB") != nullptr) {
            // Context creation is required to learn the exact version; a present
            // loader with the GLX entry points is enough to offer the emergency path.
            found.push_back(Backend::OpenGL33);
            found.push_back(Backend::OpenGL30);
            note("opengl: loader present, emergency paths offered");
        } else {
            note("opengl: loader present without GLX entry points");
        }
    }
    {
        Library loader("libGLESv2.so");
        if (loader.valid()) {
            found.push_back(Backend::OpenGLES30);
            found.push_back(Backend::OpenGLES20);
            note("gles: libGLESv2.so present");
        }
    }
    return found;
}

#elif defined(_WIN32)

bool file_present(const wchar_t *name) {
    const HMODULE module = LoadLibraryW(name);
    if (module == nullptr) {
        return false;
    }
    FreeLibrary(module);
    return true;
}

std::vector<Backend> probe_windows() {
    std::vector<Backend> found;
    if (file_present(L"d3d12.dll")) {
        found.push_back(Backend::Direct3D12);
        note("direct3d-12: d3d12.dll present");
    } else {
        note("direct3d-12: d3d12.dll missing");
    }
    const bool d3d11 = file_present(L"d3d11.dll");
    if (d3d11) {
        found.push_back(Backend::Direct3D113);
        note("direct3d-11.3: d3d11.dll present");
    } else {
        note("direct3d-11.3: d3d11.dll missing");
    }
    if (file_present(L"d2d1.dll")) {
        found.push_back(Backend::Direct2D12);
        note("direct2d-1.2: d2d1.dll present");
    } else {
        note("direct2d-1.2: d2d1.dll missing");
    }
    if (file_present(L"opengl32.dll")) {
        found.push_back(Backend::OpenGL33);
        found.push_back(Backend::OpenGL30);
        note("opengl: opengl32.dll present, emergency paths offered");
    }
    if (file_present(L"vulkan-1.dll")) {
        found.push_back(Backend::Vulkan11);
        note("vulkan: vulkan-1.dll present");
    }
    return found;
}

#else // __APPLE__

std::vector<Backend> probe_apple() {
    std::vector<Backend> found;
    // Metal is always present on macOS 10.13+ and iOS 11+; the framework itself is
    // the reliable signal, and device features decide betweenMetal 2 and Metal 3 at
    // device-creation time.
    if (dlopen("/System/Library/Frameworks/Metal.framework/Metal", RTLD_LAZY) != nullptr) {
        found.push_back(Backend::Metal3);
        found.push_back(Backend::Metal2);
        found.push_back(Backend::Metal1);
        note("metal: framework present");
    } else {
        note("metal: framework not loadable");
    }
    return found;
}

#endif

} // namespace

std::vector<Backend> probe_backends(Platform platform) {
    detail_storage().clear();
    std::vector<Backend> found;
#if defined(_WIN32)
    if (platform == Platform::Windows) {
        found = probe_windows();
    }
#elif defined(__APPLE__)
    if (platform == Platform::MacOS || platform == Platform::IOS) {
        found = probe_apple();
    }
#else
    if (platform == Platform::Linux || platform == Platform::Android) {
        found = probe_unix();
    }
#endif
    if (found.empty()) {
        note("no graphics loader found; the CPU renderer will be used");
    }
    return found;
}

const std::vector<std::string> &probe_detail() { return detail_storage(); }

} // namespace miu::bridge

add_library(miu_bridge STATIC src/miu_bridge.cpp src/probe_platform.cpp)
add_library(Miu::bridge ALIAS miu_bridge)

target_compile_features(miu_bridge PUBLIC cxx_std_17)
target_include_directories(miu_bridge
    PUBLIC
        $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

# Device factories live in optional translation units so an integrator can ship only
# the APIs it has SDKs for. Each file registers itself with `register_factory` from a
# static initialiser; the bridge runs on the CPU device when none are present.
set(MIU_BRIDGE_FACTORY_SOURCES "")
foreach(candidate
        bridge_d2d bridge_d3d11 bridge_d3d12 bridge_metal bridge_vulkan bridge_opengl
        bridge_gles bridge_webgpu)
    if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/src/${candidate}.cpp)
        list(APPEND MIU_BRIDGE_FACTORY_SOURCES src/${candidate}.cpp)
    elseif(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/src/${candidate}.mm)
        list(APPEND MIU_BRIDGE_FACTORY_SOURCES src/${candidate}.mm)
    endif()
endforeach()
if(MIU_BRIDGE_FACTORY_SOURCES)
    target_sources(miu_bridge PRIVATE ${MIU_BRIDGE_FACTORY_SOURCES})
endif()

find_package(Threads REQUIRED)
target_link_libraries(miu_bridge PUBLIC Threads::Threads)

if(UNIX AND NOT APPLE)
    target_link_libraries(miu_bridge PUBLIC ${CMAKE_DL_LIBS})
endif()

# The C ABI library is produced by Cargo. It is linked by name when it is on the
# search path, which keeps CMake configuration working before a Cargo build exists.
find_library(MIU_ABI_LIBRARY NAMES miu)
if(MIU_ABI_LIBRARY)
    target_link_libraries(miu_bridge PUBLIC ${MIU_ABI_LIBRARY})
else()
    message(STATUS "libmiu not found; the bridge compiles, linking needs `cargo build -p miu-ffi`")
endif()

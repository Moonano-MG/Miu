add_library(miu_bridge STATIC src/miu_bridge.cpp)
add_library(Miu::bridge ALIAS miu_bridge)

target_compile_features(miu_bridge PUBLIC cxx_std_17)
target_include_directories(miu_bridge
    PUBLIC
        $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>)

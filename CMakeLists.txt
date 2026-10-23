cmake_minimum_required(VERSION 3.21)
project(Miu LANGUAGES CXX)

option(MIU_BUILD_NATIVE_BRIDGE "Build the C++ platform-bridge contract" ON)

if(MIU_BUILD_NATIVE_BRIDGE)
    add_subdirectory(native)
endif()

# The Rust ABI library is built with Cargo. See README.md for build/link steps.

cmake_minimum_required(VERSION 3.21)
project(Miu LANGUAGES C CXX)

# Miu ships two build systems on purpose:
#   * Cargo builds the engine (crates/*) and the C ABI library (`miu`).
#   * CMake builds the C++ pieces: the platform bridge and the samples.
# The CMake build links against the Cargo output, so configure it after running
# `cargo build` (see README.md for the exact commands).

option(MIU_BUILD_NATIVE_BRIDGE "Build the C++ platform bridge" ON)
option(MIU_BUILD_SAMPLES "Build the C++ samples" ON)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(MIU_BUILD_NATIVE_BRIDGE)
    add_subdirectory(native)
endif()

if(MIU_BUILD_SAMPLES)
    add_subdirectory(samples/cpp)
endif()

#!/usr/bin/env sh
# Deliberately manual: this repository has no GitHub Actions build workflow.
# Run this script on a development machine with Rust, a C11/C++17 compiler, and
# optionally the .NET SDK. It validates the source tree without creating a CI job.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

: "${CC:=cc}"
: "${CXX:=c++}"

echo '== C ABI layout guard =='
"$CC" -std=c11 -Wall -Wextra -Werror -pedantic -Iinclude \
    scripts/abi_layout_check.c -o /tmp/miu_abi_layout_check
/tmp/miu_abi_layout_check
rm -f /tmp/miu_abi_layout_check

echo '== C++17 public headers and samples =='
"$CXX" -std=c++17 -Wall -Wextra -Werror -pedantic -Iinclude -Inative/include \
    -fsyntax-only samples/cpp/minimal.cpp
"$CXX" -std=c++17 -Wall -Wextra -Werror -pedantic -Iinclude -Inative/include \
    -fsyntax-only samples/cpp/software_canvas.cpp
"$CXX" -std=c++17 -Wall -Wextra -Werror -pedantic -Iinclude -Inative/include \
    -fsyntax-only native/src/miu_bridge.cpp

echo '== Rust tests and release FFI build =='
cargo test --workspace
cargo build -p miu-ffi --release

if command -v dotnet >/dev/null 2>&1; then
    echo '== C# P/Invoke binding =='
    dotnet build bindings/csharp/Miu.csproj --configuration Release
else
    echo '== C# P/Invoke binding skipped: dotnet SDK not found =='
fi

echo 'Manual verification passed.'

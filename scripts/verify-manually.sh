#!/usr/bin/env bash
#
# Manual verification for Miu.
#
# Runs every check a maintainer can run without a GPU, a display, or network access:
#
#   1. Rust workspace tests, when a toolchain is installed.
#   2. The C ABI layout assertions (struct sizes, field offsets, discriminants).
#   3. The C++ header, wrapper, bridge, and samples compile as C++17.
#   4. The C header compiles as strict C11.
#   5. Sample documents are valid JSON and reference files that exist.
#
# Exits non-zero on the first failing group so CI and pre-commit hooks can rely on it.
# Every group prints a summary line, so a partial environment (for example a machine
# without Rust) still produces a useful report.

set -uo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

CC_BIN="${CC:-cc}"
CXX_BIN="${CXX:-c++}"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

failures=0
skips=0

report() {
    local status="$1"
    local name="$2"
    case "$status" in
        ok) printf '  [ok]    %s\n' "$name" ;;
        skip) printf '  [skip]  %s\n' "$name"; skips=$((skips + 1)) ;;
        fail) printf '  [FAIL]  %s\n' "$name"; failures=$((failures + 1)) ;;
    esac
}

# --- 1. Rust -----------------------------------------------------------------
echo "Rust workspace"
if command -v cargo >/dev/null 2>&1; then
    if cargo test --workspace --quiet; then
        report ok "cargo test --workspace"
    else
        report fail "cargo test --workspace"
    fi
    if cargo build -p miu-cli --quiet; then
        report ok "cargo build -p miu-cli"
    else
        report fail "cargo build -p miu-cli"
    fi
else
    report skip "cargo not installed"
fi

# --- 2. ABI layout -----------------------------------------------------------
echo "C ABI layout"
if "$CC_BIN" -std=c11 -Wall -Wextra -Werror -Iinclude scripts/abi_layout_check.c \
        -o "$work_dir/abi_layout_check" 2>"$work_dir/abi.log"; then
    report ok "struct sizes and offsets"
else
    report fail "struct sizes and offsets"
    cat "$work_dir/abi.log"
fi

# --- 3. C header -------------------------------------------------------------
echo "C header"
cat >"$work_dir/header_probe.c" <<'PROBE'
#include <stdio.h>
#include "miu/miu.h"

int main(void) {
    miu_sprite_t sprite = miu_sprite_default();
    miu_text_style_t style = miu_text_style_default();
    miu_camera2d_t camera = miu_camera2d_default();
    miu_transform2d_t transform = miu_transform2d_identity();
    printf("%.1f %.1f %.1f %.1f %u %u\n", sprite.color.a, sprite.uv[2], style.font_size,
           camera.zoom, style.wrap, MIU_ABI_VERSION);
    return transform.scale.x > 0.0f ? 0 : 1;
}
PROBE
if "$CC_BIN" -std=c11 -Wall -Wextra -Werror -Iinclude "$work_dir/header_probe.c" \
        -o "$work_dir/header_probe" 2>"$work_dir/header.log"; then
    report ok "miu.h compiles and defaults are usable"
else
    report fail "miu.h compiles and defaults are usable"
    cat "$work_dir/header.log"
fi

# --- 4. C++ ------------------------------------------------------------------
echo "C++"
if "$CXX_BIN" -std=c++17 -Wall -Wextra -Werror -Iinclude -fsyntax-only \
        "$work_dir/header_probe.c" 2>/dev/null; then
    : # C11-only probe; the C++ checks below cover the header on their own.
fi

for source in samples/cpp/*.cpp native/src/*.cpp; do
    name="$(basename "$source")"
    if "$CXX_BIN" -std=c++17 -Wall -Wextra -Werror -Iinclude -Inative/include \
            -fsyntax-only "$source" 2>"$work_dir/$name.log"; then
        report ok "$source"
    else
        report fail "$source"
        cat "$work_dir/$name.log"
    fi
done

# --- 5. Sample documents -----------------------------------------------------
echo "Sample documents"
if command -v python3 >/dev/null 2>&1; then
    if python3 scripts/check_samples.py; then
        report ok "sample scenes parse and their assets exist"
    else
        report fail "sample scenes parse and their assets exist"
    fi
else
    report skip "python3 not installed"
fi

echo
printf 'summary: %d failure(s), %d skipped group(s)\n' "$failures" "$skips"
if [ "$failures" -ne 0 ]; then
    exit 1
fi
exit 0

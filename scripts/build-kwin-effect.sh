#!/bin/sh
# Build the optional native KWin click effect. Requires KWin headers, Qt 6, ECM and CMake.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
build="$repo/target/kwin-effect"
cmake -S "$repo/kde/kwin-effect" -B "$build" -DCMAKE_BUILD_TYPE=Release
cmake --build "$build" --parallel
printf '%s\n' "$build/buddy_click.so"

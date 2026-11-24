#!/bin/sh
# Write a deterministic SHA256SUMS for every release file in a directory.
#   scripts/checksums.sh [DIR]       (default: dist/release)
set -eu
dir=${1:-dist/release}
[ -d "$dir" ] || { echo "no release directory: $dir" >&2; exit 1; }
cd "$dir"
tmp=$(mktemp)
find . -maxdepth 1 -type f ! -name SHA256SUMS -printf '%P\0' | LC_ALL=C sort -z | xargs -0 -r sha256sum > "$tmp"
mv "$tmp" SHA256SUMS
cat SHA256SUMS

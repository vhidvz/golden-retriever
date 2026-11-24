#!/bin/sh
# Build every locally available Linux format, then generate the release metadata.
# This is a local rehearsal of the GitHub Actions packaging jobs, not a substitute for
# running the workflow in GitHub.
#   scripts/release-rehearsal.sh [FORMAT...]   (default: tar deb rpm arch appimage)
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
formats=${*:-"tar deb rpm arch appimage"}
for format in $formats; do
    "$repo/scripts/package.sh" "$format"
done
python3 "$repo/scripts/sbom.py"
"$repo/scripts/checksums.sh" "$repo/dist/release"
"$repo/scripts/sign-release.sh" "$repo/dist/release"
echo "local release rehearsal complete: $repo/dist/release"

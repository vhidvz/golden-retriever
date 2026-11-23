#!/bin/sh
# Downloads the MANN dog motion-capture archive and parses one clip.
#
# This is a research spike. It never installs the data into Buddy and never makes it a
# runtime dependency. The archive is CC BY-NC 4.0; pass --accept-license to confirm that
# you are using it for non-commercial research.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dir="${BUDDY_AI4ANIMATION_DIR:-$HOME/.cache/buddy-spikes/reference/ai4animation-mann}"
url="https://starke-consult.de/AI4Animation/SIGGRAPH_2018/MotionCapture.zip"
sha="0ecd909b673cfca5e20c3f568376dcf27d7da3bd153957c2b034d95af366132a"
archive="$dir/MotionCapture.zip"
clip="D1_001_KAN01_001.bvh"

if [ "${1:-}" != "--accept-license" ]; then
    cat >&2 <<'EOF'
The MANN dog motion capture is licensed CC BY-NC 4.0 (non-commercial).
This spike only downloads it for research; it is never bundled or installed into Buddy.
Run again with --accept-license if that is acceptable.
EOF
    exit 2
fi

mkdir -p "$dir"
echo "Downloading the MANN archive (about 32 MiB) ..."
curl -fL --retry 6 --retry-all-errors --retry-delay 2 -C - --progress-bar -o "$archive" "$url"

actual=$(sha256sum "$archive" | awk '{ print $1 }')
if [ "$actual" != "$sha" ]; then
    echo "checksum mismatch: expected $sha, got $actual" >&2
    rm -f "$archive"
    exit 1
fi
echo "checksum ok"

unzip -p "$archive" "$clip" > "$dir/$clip"
python3 -I "$here/parse_bvh.py" "$dir/$clip"

echo
echo "The clip is under $dir. It is not part of the Buddy installation."
echo "The skeleton has 21 joints; Buddy has 25 bones. See README.md for the mapping."

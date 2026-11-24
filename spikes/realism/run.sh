#!/bin/bash
# A small, repeatable Phase 8 stills harness. It exports representative poses, trims each
# clip to one frame, bakes it with the real coat, and prints the cost. It covers the
# production frame and the 800 px close-up measurements; it does not decide photoreal.
#
#   spikes/realism/run.sh [--clips stand,sit,walk] [--samples 12] [--strands 60000]
#                         [--growth 1.0] [--focus 0|0.2] [--device CPU|GPU]
#                         [--size 2400x1600] [--super 1] [--crop 800]
#
# Outputs go under ${BUDDY_SPIKE_DIR:-$HOME/.cache/buddy-spikes/work}/realism.
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
clips=stand,sit,walk
samples=12
strands=60000
growth=1.0
focus=0
device=CPU
size=
super=
crop=
while [ $# -gt 0 ]; do
    case "$1" in
        --clips) clips=$2; shift 2 ;;
        --samples) samples=$2; shift 2 ;;
        --strands) strands=$2; shift 2 ;;
        --growth) growth=$2; shift 2 ;;
        --focus) focus=$2; shift 2 ;;
        --device) device=$2; shift 2 ;;
        --size) size=$2; shift 2 ;;
        --super) super=$2; shift 2 ;;
        --crop) crop=$2; shift 2 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done
run="${BUDDY_SPIKE_DIR:-$HOME/.cache/buddy-spikes/work}/realism"
rm -rf "$run"; mkdir -p "$run"
godot_bin=${BUDDY_GODOT:-}
if [ -z "$godot_bin" ]; then
    if command -v godot >/dev/null 2>&1; then godot_bin=$(command -v godot); elif command -v godot4 >/dev/null 2>&1; then godot_bin=$(command -v godot4); else
        godot_bin=$(find "${BUDDY_DATA_DIR:-$HOME/.local/share/buddy}/godot" -type f -name 'Godot_v*-stable_linux.*' ! -name '*.zip' -perm -u+x 2>/dev/null | sort | tail -1)
    fi
fi
[ -n "$godot_bin" ] && [ -x "$godot_bin" ] || { echo "Godot was not found; set BUDDY_GODOT" >&2; exit 1; }
"$godot_bin" --headless --path "$repo/renderer" -s res://export_poses.gd -- --growth "$growth" --size 420 --out "$run/poses.json" --only "$clips"
python3 - "$run/poses.json" "$run/poses-small.json" "$clips" <<'PY'
import json, sys
source, target, clips = sys.argv[1:]
keep = set(clips.split(','))
data = json.load(open(source))
data['clips'] = [dict(clip, frames=clip['frames'][:1]) for clip in data['clips'] if clip['name'] in keep]
json.dump(data, open(target, 'w'), separators=(',', ':'))
PY
if [ -n "$size" ] && [ -z "$super" ]; then super=1; fi
echo "baking $clips at samples=$samples strands=$strands growth=$growth focus=$focus device=$device size=${size:-window} super=${super:-1.6}"
bake_env=(DEVICE="$device" FOCUS="$focus")
if [ -n "$size" ]; then bake_env+=(SIZE="$size"); fi
if [ -n "$super" ]; then bake_env+=(SUPER="$super"); fi
started=$(date +%s)
env "${bake_env[@]}" blender -b --factory-startup --python "$repo/tools/bake_sprites.py" -- \
    "$run/poses-small.json" "$run/out" --samples "$samples" --strands "$strands" --only "$clips"
finished=$(date +%s)
if [ -n "$crop" ]; then
    python3 - "$run/out" "$crop" <<'PY'
import glob, sys
from PIL import Image
root, side = sys.argv[1], int(sys.argv[2])
for path in glob.glob(root + '/*/*.webp'):
    picture = Image.open(path)
    width, height = picture.size
    size = min(side, width, height)
    box = ((width - size) // 2, (height - size) // 2, (width + size) // 2, (height + size) // 2)
    picture.crop(box).save(path, quality=90)
    print(f"cropped {path} to {size}x{size}")
PY
fi
echo "elapsed=$((finished - started))s for $(find "$run/out" -name '*.webp' | wc -l) picture(s)"

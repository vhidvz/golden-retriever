#!/bin/bash
# Draws Buddy with a real coat of hair, for his present age, so that the renderer can
# show him as pictures instead of as the live 3D body. Runs at the lowest priority;
# expect a few hours for a full set. It can be stopped (Ctrl+C) and run again: what
# is already drawn is kept. The renderer starts using each movement as soon as all
# its pictures exist.
#
#   scripts/bake-pictures.sh            # for his age now (needs Buddy running)
#   scripts/bake-pictures.sh 0.30       # for a given growth (0 = newborn, 1 = adult)
#
# Needs Godot and Blender. Pictures go to ~/.local/share/buddy/sprites/growth-NNN/;
# the working files (the recorded poses) to ~/.cache/buddy/bake/.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"

for tool in godot blender; do
    command -v "$tool" >/dev/null || { echo "$tool is needed to draw the pictures and is not installed." >&2; exit 1; }
done

if [ $# -ge 1 ]; then
    growth="$1"
    size=420
else
    status="$(buddy status --json)" || { echo "Buddy is not running; give a growth instead (0 to 1)." >&2; exit 1; }
    growth="$(printf '%s' "$status" | python3 -c 'import json,sys; print(round(json.load(sys.stdin)["state"]["growth"], 4))')"
    size="$(printf '%s' "$status" | python3 -c 'import json,sys; print(json.load(sys.stdin)["state"]["size_px"])')"
fi
name="growth-$(python3 -c "print(f'{round(float($growth) * 100):03d}')")"
out="${XDG_DATA_HOME:-$HOME/.local/share}/buddy/sprites/$name"
work="${XDG_CACHE_HOME:-$HOME/.cache}/buddy/bake/$name"
mkdir -p "$out" "$work"

echo "Recording his movements at growth $growth (size $size px) …"
godot --headless --path "$repo/renderer" -s res://export_poses.gd -- \
    --growth "$growth" --size "$size" --out "$work/poses.json" 2>&1 | grep -E '^recorded' | tail -n 1

echo "Drawing them into $out (this takes hours; it is safe to stop and run again) …"
nice -n 19 blender -b --factory-startup --python "$repo/tools/bake_sprites.py" -- "$work/poses.json" "$out" 2>&1 \
    | grep --line-buffered -E '^BAKE' | tee "$work/bake.log"

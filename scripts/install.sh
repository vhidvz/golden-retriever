#!/bin/sh
# Installs Buddy for the current user only (no root): the binary, the renderer
# project and a systemd user service that starts him with the graphical session.
#   scripts/install.sh            install and start
#   scripts/install.sh --remove   stop and remove (his memory in ~/.local/share/buddy is kept)
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
bin="$HOME/.local/bin"
share="${XDG_DATA_HOME:-$HOME/.local/share}/buddy"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

if [ "${1:-}" = "--remove" ]; then
    systemctl --user disable --now buddy.service 2>/dev/null || true
    rm -f "$units/buddy.service" "$bin/buddy"
    rm -rf "$share/renderer"
    systemctl --user daemon-reload
    echo "Buddy removed. His memory is still in $share (delete it yourself if you really mean it)."
    exit 0
fi

command -v godot >/dev/null 2>&1 || command -v godot4 >/dev/null 2>&1 || [ -n "${BUDDY_GODOT:-}" ] ||
    echo "warning: Godot 4 was not found; Buddy will live but not be drawn (install godot, or set BUDDY_GODOT in ~/.config/buddy/env)" >&2

(cd "$repo" && cargo build --release)
mkdir -p "$bin" "$share" "$units"
systemctl --user stop buddy.service 2>/dev/null || true
install -m 755 "$repo/target/release/buddy" "$bin/buddy"
rm -rf "$share/renderer"
mkdir "$share/renderer"
cp "$repo"/renderer/project.godot "$repo"/renderer/*.tscn "$repo"/renderer/*.gd "$repo"/renderer/*.gdshader "$share/renderer/"
cp -r "$repo/renderer/sounds" "$share/renderer/sounds"
install -m 644 "$repo/dist/buddy.service" "$units/buddy.service"
systemctl --user daemon-reload
systemctl --user enable --now buddy.service
echo "Buddy is running and will start with your desktop session. Try: buddy status"

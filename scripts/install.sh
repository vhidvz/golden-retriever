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
extension="buddy@buddy.local"
extensions="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions"

if [ "${1:-}" = "--remove" ]; then
    systemctl --user disable --now buddy.service 2>/dev/null || true
    rm -f "$units/buddy.service" "$bin/buddy"
    rm -rf "$share/renderer" "$share/perception" "$extensions/$extension"
    gnome-extensions disable "$extension" 2>/dev/null || true
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
rm -rf "$share/perception"
mkdir "$share/perception"
cp "$repo"/perception/*.py "$share/perception/"

# The desktop sense is a GNOME Shell extension. On Wayland a newly installed extension
# is only loaded at the next login, so it is switched on now and starts working then.
if command -v gnome-extensions >/dev/null 2>&1; then
    rm -rf "$extensions/$extension"
    mkdir -p "$extensions/$extension"
    cp "$repo/shell-extension/$extension"/* "$extensions/$extension/"
    if ! gnome-extensions enable "$extension" 2>/dev/null; then
        current=$(gsettings get org.gnome.shell enabled-extensions)
        case "$current" in
            *"$extension"*) ;;
            "@as []") gsettings set org.gnome.shell enabled-extensions "['$extension']" ;;
            *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$extension']" ;;
        esac
        echo "Buddy's desktop sense will start working after you next log in."
    fi
fi
install -m 644 "$repo/dist/buddy.service" "$units/buddy.service"
systemctl --user daemon-reload
systemctl --user enable --now buddy.service
echo "Buddy is running and will start with your desktop session. Try: buddy status"

# A plain interactive bash does not always have ~/.local/bin on its PATH; say so rather than
# leave the user with "command not found".
if ! env -i HOME="$HOME" USER="${USER:-}" TERM=dumb bash -ic 'command -v buddy' >/dev/null 2>&1; then
    echo
    echo "Note: in a new bash terminal the 'buddy' command will not be found, because $bin is not on bash's PATH."
    echo "To fix that once and for all, run:"
    echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
    echo "Until then, use the full path: $bin/buddy"
fi

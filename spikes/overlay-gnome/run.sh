#!/bin/bash
# Overlay harness: starts an isolated, headless gnome-shell (its own D-Bus session,
# runtime dir and settings, so your real session is untouched), loads the test
# extension, and lets it drive three kinds of overlay window with a virtual pointer.
#   ./run.sh        click-through / stacking / focus tests  -> out/harness-result.json
#   ./run.sh fur    the GL fur client through the nested XWayland
# Note: X11 GL clients do not present in the headless shell on NVIDIA, so GL
# appearance has to be checked on a real session (./furtest on your desktop).
set -eu
D="$(cd "$(dirname "$0")" && pwd)"
OUT="$D/out"; R="$(mktemp -d)"; RUN="/run/user/$(id -u)/buddy-spike"
trap 'rm -rf "$R" "$RUN"' EXIT
mkdir -p "$OUT" "$R"/{home,config,cache} "$R/data/gnome-shell/extensions/buddyspike@local" "$RUN"; chmod 700 "$RUN"
cp "$D"/extension/* "$R/data/gnome-shell/extensions/buddyspike@local/"
[ -x "$D/furtest" ] || gcc -O2 "$D/furtest.c" -o "$D/furtest" -lX11 -lXext -lXrender -lGL -lm
rm -f "$OUT"/harness-* "$OUT"/log-* "$OUT"/shot-*
env -i HOME="$R/home" PATH=/usr/bin:/usr/local/bin USER="$USER" LANG=en_US.UTF-8 \
  XDG_RUNTIME_DIR="$RUN" XDG_CONFIG_HOME="$R/config" XDG_DATA_HOME="$R/data" XDG_CACHE_HOME="$R/cache" \
  XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME GSETTINGS_BACKEND=keyfile \
  BUDDY_SPIKE_DIR="$D" BUDDY_SPIKE_OUT="$OUT" BUDDY_MODE="${1:-all}" \
  timeout 150 dbus-run-session -- bash -c '
    gsettings set org.gnome.shell enabled-extensions "[\"buddyspike@local\"]"
    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.shell welcome-dialog-last-shown-version "999"
    exec gnome-shell --headless --wayland --virtual-monitor 1920x1080 --wayland-display wl-buddy' > "$OUT/shell.log" 2>&1 || true
cat "$OUT"/harness-error.txt 2>/dev/null || cat "$OUT"/harness-result*.json

#!/bin/bash
# Spike: can Buddy notice what is done inside applications, and do it again, through
# the accessibility bus (AT-SPI)? Everything runs in an isolated headless gnome-shell
# with its own home, settings and buses; nothing appears on the real desktop.
#   spikes/workflow-atspi/run.sh [probe arguments]     results in out/
set -u
D="$(cd "$(dirname "$0")" && pwd)"
OUT="$D/out"; R="$(mktemp -d)"; RUN="/run/user/$(id -u)/buddy-workflow-spike"
mkdir -p "$OUT" "$RUN" "$R/hands" "$R"/{home,config,cache} "$R/data/gnome-shell/extensions/buddyhands@spike"
chmod 700 "$RUN"
cleanup() { fusermount3 -uz "$RUN/doc" 2>/dev/null; fusermount3 -uz "$RUN/gvfs" 2>/dev/null; rm -rf "$R" "$RUN"; }
trap cleanup EXIT
cp "$D/hands/"* "$R/data/gnome-shell/extensions/buddyhands@spike/"
rm -f "$OUT"/*.log "$OUT"/*.json "$OUT"/*.txt
env -i HOME="$R/home" PATH=/usr/bin:/usr/local/bin USER="$USER" LANG=en_US.UTF-8 \
  XDG_RUNTIME_DIR="$RUN" XDG_CONFIG_HOME="$R/config" XDG_DATA_HOME="$R/data" XDG_CACHE_HOME="$R/cache" \
  XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME GSETTINGS_BACKEND=keyfile \
  SPIKE_DIR="$D" SPIKE_HANDS="$R/hands" SPIKE_HOME="$R/home" FORCE="${FORCE:-}" SETTLE="${SETTLE:-2}" A11Y="${A11Y:-true}" \
  timeout 600 dbus-run-session -- bash -c '
    gsettings set org.gnome.shell enabled-extensions "[\"buddyhands@spike\"]"
    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.shell welcome-dialog-last-shown-version "999"
    gsettings set org.gnome.desktop.interface toolkit-accessibility "$A11Y"
    # The accessibility bus here is a dbus-broker, which can only start its registry
    # through systemd, and this session has none; so both are started by hand.
    /usr/lib/at-spi-bus-launcher --launch-immediately > "$SPIKE_DIR/out/a11y-bus.log" 2>&1 &
    for i in $(seq 50); do gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus --method org.a11y.Bus.GetAddress >/dev/null 2>&1 && break; sleep 0.1; done
    /usr/lib/at-spi2-registryd >> "$SPIKE_DIR/out/a11y-bus.log" 2>&1 &
    sleep 0.5
    gnome-shell --headless --wayland --virtual-monitor 1920x1080 --wayland-display wl-buddy-spike > "$SPIKE_DIR/out/shell.log" 2>&1 &
    shell=$!
    for i in $(seq 200); do [ -e "$SPIKE_HANDS/ready" ] && break; sleep 0.1; done
    WAYLAND_DISPLAY=wl-buddy-spike python3 "$SPIKE_DIR/probe.py" "$@"; status=$?
    kill $shell 2>/dev/null; wait $shell 2>/dev/null; exit $status' probe "$@"

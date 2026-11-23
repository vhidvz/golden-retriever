#!/bin/bash
# End-to-end test of the desktop sense: a real (headless, isolated) gnome-shell runs
# Buddy's extension next to a test driver that opens windows, goes fullscreen, opens
# the overview, shows a notification and moves the pointer. A daemon on a private
# socket must hear all of it — and none of it once privacy mode is on.
#   tests/shell-sense/run.sh            exits 0 on success
set -u
D="$(cd "$(dirname "$0")" && pwd)"; REPO="$(cd "$D/../.." && pwd)"
OUT="$D/out"; R="$(mktemp -d)"; RUN="/run/user/$(id -u)/buddy-sense-test"
mkdir -p "$OUT" "$RUN"; chmod 700 "$RUN"
export BUDDY_SOCKET="$RUN/buddy.sock" BUDDY_DATA_DIR="$R/buddy-data"
BUDDY="$REPO/target/debug/buddy"
(cd "$REPO" && cargo build -q) || exit 1
cleanup() { "$BUDDY" quit >/dev/null 2>&1; rm -rf "$R" "$RUN"; }
trap cleanup EXIT

desktop() {  # runs the headless shell once; everything it does is over when this returns
  for name in "buddy@buddy.local:$REPO/shell-extension/buddy@buddy.local" "buddydriver@test:$D/driver"; do
    mkdir -p "$R/data/gnome-shell/extensions/${name%%:*}"
    cp -r "${name#*:}"/. "$R/data/gnome-shell/extensions/${name%%:*}/"
    if [ -d "$R/data/gnome-shell/extensions/${name%%:*}/schemas" ] && command -v glib-compile-schemas >/dev/null 2>&1; then
        glib-compile-schemas "$R/data/gnome-shell/extensions/${name%%:*}/schemas" 2>/dev/null || true
    fi
  done
  mkdir -p "$R"/{home,config,cache}
  env -i HOME="$R/home" PATH=/usr/bin:/usr/local/bin USER="$USER" LANG=en_US.UTF-8 \
    XDG_RUNTIME_DIR="$RUN" XDG_CONFIG_HOME="$R/config" XDG_DATA_HOME="$R/data" XDG_CACHE_HOME="$R/cache" \
    XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME GSETTINGS_BACKEND=keyfile \
    BUDDY_SOCKET="$BUDDY_SOCKET" BUDDY_TEST_DIR="$D" \
    timeout 90 dbus-run-session -- bash -c '
      gsettings set org.gnome.shell enabled-extensions "[\"buddy@buddy.local\", \"buddydriver@test\"]"
      gsettings set org.gnome.shell disable-user-extensions false
      gsettings set org.gnome.shell welcome-dialog-last-shown-version "999"
      exec gnome-shell --headless --wayland --virtual-monitor 1920x1080 --wayland-display wl-buddy-test' > "$OUT/shell.log" 2>&1
}

failures=0
expect() {  # expect DESCRIPTION PATTERN FILE [none]
  local found; found=$(grep -cE "$2" "$3")
  if { [ "${4:-}" = none ] && [ "$found" -eq 0 ]; } || { [ "${4:-}" != none ] && [ "$found" -gt 0 ]; }; then echo "  ok    $1"
  else echo "  FAIL  $1 (matched $found times)"; failures=$((failures + 1)); fi
}

rm -f "$OUT"/*.txt "$OUT"/*.log
BUDDY_TRACE_SENSES=1 "$BUDDY" daemon --no-renderer 2> "$OUT/heard.log" &
sleep 1
echo "With the desktop sense allowed:"
desktop
"$BUDDY" status --json > "$OUT/status.json"
expect "application focus is reported, by application only" 'sense shell focus \{"app":"[^"]+"\}$' "$OUT/heard.log"
expect "no window title is sent while titles are off" 'sense shell focus .*title' "$OUT/heard.log" none
expect "fullscreen on and off" 'sense shell fullscreen \{"on":true\}' "$OUT/heard.log"
expect "overview on and off" 'sense shell overview \{"on":true\}' "$OUT/heard.log"
expect "a notification, by application name only" 'sense shell notification \{"app":' "$OUT/heard.log"
expect "pointer position" 'sense shell pointer \{"x":900' "$OUT/heard.log"
expect "a click, by button and coarse region only" 'sense shell click .*"button":1' "$OUT/heard.log"
if [ -e "$OUT/driver-error.txt" ]; then echo "  FAIL  the test driver crashed: $(head -1 "$OUT/driver-error.txt")"; failures=$((failures + 1)); fi

echo "With window titles switched on:"
"$BUDDY" sense window_titles on >/dev/null; cp "$OUT/heard.log" "$OUT/heard-1.log"
desktop
tail -n +$(( $(wc -l < "$OUT/heard-1.log") + 1 )) "$OUT/heard.log" > "$OUT/heard-2.log"
expect "titles are now included" 'sense shell focus .*"title":"(alpha|beta)"' "$OUT/heard-2.log"

echo "In privacy mode:"
"$BUDDY" privacy on >/dev/null; cp "$OUT/heard.log" "$OUT/heard-12.log"
desktop
tail -n +$(( $(wc -l < "$OUT/heard-12.log") + 1 )) "$OUT/heard.log" > "$OUT/heard-3.log"
expect "the extension sends nothing at all" 'sense shell' "$OUT/heard-3.log" none

[ "$failures" -eq 0 ] && echo "desktop sense: all checks passed" || { echo "desktop sense: $failures check(s) failed; see $OUT"; exit 1; }

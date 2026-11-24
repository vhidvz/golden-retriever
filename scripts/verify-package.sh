#!/bin/sh
# Check that a built artifact contains the complete installed payload.
#   scripts/verify-package.sh FILE
set -eu
file=${1:?usage: scripts/verify-package.sh FILE}
[ -f "$file" ] || { echo "not a file: $file" >&2; exit 1; }
file=$(cd "$(dirname "$file")" && pwd)/$(basename "$file")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
root="$tmp"
case "$file" in
    *.deb)
        command -v dpkg-deb >/dev/null 2>&1 || { echo "dpkg-deb is required" >&2; exit 1; }
        mkdir "$tmp/root"
        dpkg-deb -x "$file" "$tmp/root"
        root="$tmp/root"
        ;;
    *.rpm)
        command -v rpm2cpio >/dev/null 2>&1 || { echo "rpm2cpio is required" >&2; exit 1; }
        command -v cpio >/dev/null 2>&1 || { echo "cpio is required" >&2; exit 1; }
        (cd "$tmp" && rpm2cpio "$file" | cpio -idm --quiet)
        ;;
    *.pkg.tar.zst|*.tar.zst)
        mkdir "$tmp/root"
        tar -C "$tmp/root" -xf "$file"
        root="$tmp/root"
        ;;
    *.tar.gz|*.tgz)
        mkdir "$tmp/root"
        tar -C "$tmp/root" -xzf "$file"
        root="$tmp/root"
        ;;
    *.AppImage)
        (cd "$tmp" && "$file" --appimage-extract >/dev/null)
        root="$tmp/squashfs-root"
        ;;
    *)
        echo "unknown package format: $file" >&2
        exit 2
        ;;
esac
required="usr/bin/buddy usr/bin/buddy-ui usr/bin/buddy-tray usr/share/buddy/ui/buddy_ui.py usr/share/buddy/ui/buddy_tray.py usr/share/buddy/ui/buddy_tray.css usr/share/buddy/renderer/project.godot usr/share/buddy/perception/camera_watch.py usr/share/buddy/tools/bake_sprites.py usr/lib/systemd/user/buddy.service usr/lib/systemd/user/buddy-tray.service usr/share/applications/buddy.desktop usr/share/icons/hicolor/scalable/apps/buddy.svg"
for path in $required; do
    [ -e "$root/$path" ] || { echo "missing from package: $path" >&2; exit 1; }
done
[ -x "$root/usr/bin/buddy" ] || { echo "the daemon is not executable" >&2; exit 1; }
[ -x "$root/usr/bin/buddy-ui" ] || { echo "the control panel launcher is not executable" >&2; exit 1; }
[ -x "$root/usr/bin/buddy-tray" ] || { echo "the tray launcher is not executable" >&2; exit 1; }
grep -q '../share/buddy/ui/buddy_ui.py' "$root/usr/bin/buddy-ui" || { echo "the UI launcher does not use a relocatable path" >&2; exit 1; }
grep -q '../share/buddy/ui/buddy_tray.py' "$root/usr/bin/buddy-tray" || { echo "the tray launcher does not use a relocatable path" >&2; exit 1; }
echo "verified $file"

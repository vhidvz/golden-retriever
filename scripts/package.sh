#!/bin/sh
# Build one release artifact from a clean checkout.
#
#   scripts/package.sh tar       portable .tar.zst
#   scripts/package.sh deb       Debian package (dpkg-deb required)
#   scripts/package.sh rpm       RPM package (rpmbuild required)
#   scripts/package.sh arch      Arch package (makepkg required)
#   scripts/package.sh appimage  AppImage (appimagetool required)
#
# The package carries the daemon, CLI, renderer project, perception helpers, control
# panel, systemd unit and desktop entry. Godot and the AI runtime remain managed tools
# installed on first run; they are not silently bundled by this script.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
format=${1:-tar}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "could not read the version" >&2; exit 1; }
out="$repo/dist/release"
stage="$repo/target/package/buddy-$version"
rm -rf "$stage"
mkdir -p "$out"

echo "building Buddy $version ($format)"
(cd "$repo" && cargo build --release)
binary="$repo/target/release/buddy"
[ -x "$binary" ] || { echo "the release binary is missing" >&2; exit 1; }

mkdir -p "$stage/usr/bin" "$stage/usr/share/buddy" "$stage/usr/share/applications" \
    "$stage/usr/share/icons/hicolor/scalable/apps" "$stage/usr/lib/systemd/user" "$stage/usr/share/doc/buddy"
install -m 755 "$binary" "$stage/usr/bin/buddy"
cat > "$stage/usr/bin/buddy-ui" <<'WRAPPER'
#!/bin/sh
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 "$here/../share/buddy/ui/buddy_ui.py" "$@"
WRAPPER
chmod 755 "$stage/usr/bin/buddy-ui"
for part in ui perception renderer tools shell-extension kde; do
    cp -r "$repo/$part" "$stage/usr/share/buddy/$part"
    find "$stage/usr/share/buddy/$part" -type d -name __pycache__ -prune -exec rm -rf {} +
done
cp "$repo/README.md" "$repo/LICENSE" "$stage/usr/share/doc/buddy/"
cp "$repo/ui/buddy.svg" "$stage/usr/share/icons/hicolor/scalable/apps/buddy.svg"
sed 's|@BUDDY_UI@|/usr/bin/buddy-ui|g' "$repo/dist/buddy-ui.desktop" > "$stage/usr/share/applications/buddy.desktop"
cat > "$stage/usr/lib/systemd/user/buddy.service" <<'UNIT'
[Unit]
Description=Buddy, a dog who lives on the desktop
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=/usr/bin/buddy daemon
Restart=on-failure
RestartSec=3
EnvironmentFile=-%h/.config/buddy/env

[Install]
WantedBy=graphical-session.target
UNIT

artifact=""
case "$format" in
    tar)
        if command -v zstd >/dev/null 2>&1; then
            artifact="$out/buddy-$version-linux-x86_64.tar.zst"
            rm -f "$artifact"
            tar -C "$stage" -cf - . | zstd -19 -T0 -o "$artifact"
        else
            artifact="$out/buddy-$version-linux-x86_64.tar.gz"
            tar -C "$stage" -czf "$artifact" .
        fi
        ;;
    deb)
        command -v dpkg-deb >/dev/null 2>&1 || { echo "dpkg-deb is required" >&2; exit 1; }
        mkdir -p "$stage/DEBIAN"
        cat > "$stage/DEBIAN/control" <<CONTROL
Package: buddy
Version: $version
Section: misc
Priority: optional
Architecture: amd64
Maintainer: Buddy project <noreply@example.invalid>
Depends: python3, python3-gi, gir1.2-gtk-4.0, gir1.2-adw-1, systemd
Description: Buddy, a Golden Retriever who lives on your desktop
 Buddy is a local-first desktop dog: a daemon simulates his life, a Godot
 renderer draws him, and an optional local model helps him understand language.
CONTROL
        artifact="$out/buddy_${version}_amd64.deb"
        rm -f "$artifact"
        dpkg-deb --build --root-owner-group "$stage" "$artifact" >/dev/null
        ;;
    rpm)
        command -v rpmbuild >/dev/null 2>&1 || { echo "rpmbuild is required" >&2; exit 1; }
        top="$repo/target/package/rpm"
        rm -rf "$top"; mkdir -p "$top/BUILD" "$top/RPMS" "$top/SPECS" "$top/SOURCES" "$top/BUILDROOT"
        tar -C "$stage" -czf "$top/SOURCES/buddy-$version.tar.gz" .
        cat > "$top/SPECS/buddy.spec" <<SPEC
Name: buddy
Version: $version
Release: 1%{?dist}
Summary: Buddy, a Golden Retriever who lives on your desktop
License: MIT
Source0: buddy-$version.tar.gz
Requires: python3 python3-gobject gtk4 libadwaita systemd
%description
Buddy is a local-first desktop dog: a daemon simulates his life, a Godot renderer draws him, and an optional local model helps him understand language.
%prep
%setup -q -c -T
%build
%install
mkdir -p %{buildroot}
tar -xzf %{SOURCE0} -C %{buildroot}
%files
/usr/bin/buddy
/usr/bin/buddy-ui
/usr/share/buddy
/usr/share/applications/buddy.desktop
/usr/share/icons/hicolor/scalable/apps/buddy.svg
/usr/lib/systemd/user/buddy.service
/usr/share/doc/buddy
%changelog
* $(date +%Y-%m-%d) Buddy project <noreply@example.invalid> - $version
- Release $version
SPEC
        rpmbuild --define "_topdir $top" -bb "$top/SPECS/buddy.spec" >/dev/null
        built=$(find "$top/RPMS" -type f | head -1)
        [ -n "$built" ] || { echo "rpmbuild produced no package" >&2; exit 1; }
        cp "$built" "$out/"
        artifact="$out/$(basename "$built")"
        ;;
    arch)
        command -v makepkg >/dev/null 2>&1 || { echo "makepkg is required" >&2; exit 1; }
        work="$repo/target/package/arch"
        rm -rf "$work"; mkdir -p "$work"
        tar -C "$stage" -czf "$work/buddy-$version.tar.gz" .
        source_sum=$(sha256sum "$work/buddy-$version.tar.gz" | cut -d' ' -f1)
        cat > "$work/PKGBUILD" <<ARCH
pkgname=buddy
pkgver=$version
pkgrel=1
pkgdesc='Buddy, a Golden Retriever who lives on your desktop'
arch=('x86_64')
license=('MIT')
depends=('python' 'python-gobject' 'gtk4' 'libadwaita' 'systemd')
source=("buddy-$version.tar.gz")
sha256sums=('$source_sum')
package() {
  cp -a "\$srcdir/." "\$pkgdir/"
  rm -f "\$pkgdir/buddy-$version.tar.gz"
}
ARCH
        (cd "$work" && makepkg --nodeps --noconfirm --cleanbuild)
        built=$(ls -t "$work"/*.pkg.tar.zst | head -1)
        cp "$built" "$out/"
        artifact="$out/$(basename "$built")"
        ;;
    appimage)
        command -v appimagetool >/dev/null 2>&1 || { echo "appimagetool is required" >&2; exit 1; }
        appdir="$repo/target/package/Buddy.AppDir"
        rm -rf "$appdir"; mkdir -p "$appdir"
        cp -a "$stage/usr" "$appdir/usr"
        cp "$stage/usr/share/applications/buddy.desktop" "$appdir/buddy.desktop"
        cp "$stage/usr/share/icons/hicolor/scalable/apps/buddy.svg" "$appdir/buddy.svg"
        cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
export BUDDY_RENDERER_DIR="$APPDIR/usr/share/buddy/renderer"
export BUDDY_PERCEPTION_DIR="$APPDIR/usr/share/buddy/perception"
if ! "$APPDIR/usr/bin/buddy" status >/dev/null 2>&1; then
    "$APPDIR/usr/bin/buddy" daemon >/dev/null 2>&1 &
fi
exec "$APPDIR/usr/bin/buddy-ui" "$@"
APPRUN
        chmod 755 "$appdir/AppRun"
        artifact="$out/Buddy-$version-x86_64.AppImage"
        rm -f "$artifact"
        ARCH=x86_64 appimagetool "$appdir" "$artifact"
        ;;
    *)
        echo "usage: scripts/package.sh [tar|deb|rpm|arch|appimage]" >&2
        exit 2
        ;;
esac
[ -n "$artifact" ] || { echo "no artifact was produced" >&2; exit 1; }
"$repo/scripts/verify-package.sh" "$artifact"
"$repo/scripts/checksums.sh" "$out" >/dev/null
echo "artifact written to $out"

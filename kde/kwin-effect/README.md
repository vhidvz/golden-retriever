# Native KDE click effect

KWin 6 scripts cannot observe mouse buttons. This small native effect calls the existing
`local.buddy.Sense` session-bus bridge with the button and a coarse region. It does not
read window contents, keys, clipboard data or precise coordinates beyond the region.

## Build on KDE

```sh
cmake -S kde/kwin-effect -B target/kwin-effect -DCMAKE_BUILD_TYPE=Release
cmake --build target/kwin-effect --parallel
```

Requires KWin development headers/CMake config, Qt 6 and ECM. On Arch, that is the
`kwin` package plus the usual base-devel/CMake toolchain. Then install the resulting
`buddy_click.so` in the user KWin effect plugin directory and enable `buddy_clickEnabled`
in `kwinrc`.

```sh
install -Dm755 target/kwin-effect/buddy_click.so \
  ~/.local/share/kwin/effects/plugins/buddy_click.so
kwriteconfig6 --file kwinrc --group Plugins --key buddy_clickEnabled true
qdbus6 org.kde.KWin /KWin reconfigure
```

The bundled installer attempts this build only when KDE/KWin development tools are
present. It does not ship a prebuilt binary; effect binaries are tied to the KWin ABI.

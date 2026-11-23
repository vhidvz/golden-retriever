# Release engineering and updates

**Status:** requirements record, 2026-11-23. This is **not an approved decision** and
no release automation has been implemented. The owner asked for this to be added as a
roadmap phase; the shape below is the minimum design that fits the project's rules and
must still pass a spike, a written plan and explicit approval.

## Goal

A tagged Buddy release should produce complete, installable artifacts from GitHub
Actions without a hand-built machine, and users should be able to ask whether a newer
Buddy or a newer managed tool exists. Local-first and explicit consent remain the rule:
no background network polling, no silent self-update, no paid signing service assumed.

## Artifacts the phase should produce

| Target | Format | Notes |
|---|---|---|
| Debian/Ubuntu | `.deb` | daemon and CLI, control panel, renderer assets, perception helpers, user systemd units, desktop file and icons |
| Fedora/RHEL and compatible | `.rpm` | same payload; SELinux and systemd user-unit policy checked |
| Arch and other rolling distributions | `PKGBUILD` / `.pkg.tar.zst` | maintain a real package recipe, not only a tarball |
| Portable Linux | AppImage | decide whether Godot/Ollama are bundled or fetched on first run |
| Generic Linux | `.tar.zst` | checksummed fallback |
| Windows | installer (MSI/EXE) | **blocked on a daemon port**: Unix sockets, systemd, GNOME extension and the managed Ollama service are Linux-specific today |
| Release metadata | `SHA256SUMS`, release notes, SBOM, optional signature | artifacts and checksums attached to the GitHub Release |

The current installer's model is user-local (`~/.local/bin`, `~/.local/share/buddy`, a
systemd user unit). System packages must not silently take over that data; package
maintainer scripts need an explicit migration/coexistence policy.

## CI shape

- Trigger on `v*` tags and a manual `workflow_dispatch`; pull requests run tests and
  package validation but never receive release secrets.
- Matrix: Ubuntu LTS, Fedora latest, an Arch container, and a Windows runner (once the
  port exists). Each job runs `cargo test`, the UI/static checks documented in
  `docs/development.md`, builds the release profile, packages, and uploads artifacts.
- Pin every action by commit SHA, grant least privilege, and keep signing credentials in
  environment-protected secrets. Do not run untrusted PR code with write tokens.
- Use one reproducible packaging layer (for example `cargo-deb`/`nfpm`, `makepkg`,
  `appimagetool`, Inno Setup or WiX) and verify the installed payload in a clean
  container. A package that only contains the binary is not complete.
- Decide whether the AppImage bundles the pinned Godot renderer and Ollama runtime or
  fetches them on first run. The project already downloads Godot and Ollama at install
  time; the package must preserve checksums and the "unavailable" states.

## Version and update checks

- Embed the crate version and the git tag/commit in `buddy version` and the panel.
- `buddy update check` (name open) queries the GitHub Releases API only when the user
  asks; it prints current/latest version and the release URL. No background polling.
- `buddy upgrade` is guided: download the correct artifact, verify checksum and optional
  signature, show the commands, and never overwrite a package-managed install.
- Managed tools get their own checks: the pinned Godot version, the managed Ollama
  version, and installed model digests. Existing `buddy ai status`, `buddy ai doctor`
  and the runtime manifest are the starting point; add a `--check` path rather than
  auto-updating a model or runtime.
- Data compatibility: schema migrations and the daily database backup stay authoritative;
  an upgrade refuses to proceed if the data directory cannot be backed up.

## Open decisions

1. Windows: a native port (Unix socket/IPC, services, no GNOME extension, Windows audio
   and camera) or an explicitly unsupported target. A Windows installer alone cannot make
   the current daemon work.
2. AppImage: bundle Godot/Ollama, fetch them on first run, or make them optional peers.
3. Signing: unsigned checksums first, or a free/open signing arrangement; do not assume a
   paid certificate.
4. Arch: AUR recipe maintained by the project, an official repository, or both.
5. Update channels: stable only, or opt-in beta; no telemetry.
6. Package ownership of `~/.local/share/buddy` versus a system data directory.

## Why this is a separate phase

It changes distribution, permissions, installers, secrets and the meaning of a release;
it does not change Buddy's brain. It must not be smuggled into a normal feature branch.
The project rule is still: spike results, written plan, explicit approval, then
implementation. This record is the durable input to that plan.

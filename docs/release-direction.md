# Release engineering and updates

**Status:** Linux implementation complete and verified locally and in GitHub Actions on
2026-11-24 with relocatable package paths, all five formats, SHA-256 manifests, a
CycloneDX SBOM, package payload verification, optional GPG signing and commit-pinned
Actions. The manual workflow run (45322812499) passed tests and every Linux package job
plus the metadata job; the tag-only publish job was skipped because no tag/release was
created. Windows is explicitly unsupported for now; guided upgrade downloads/verifies
but does not install.

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
| Windows | — | **not built**: Unix sockets, systemd, GNOME extension and the managed Ollama service are Linux-specific. This release line explicitly does not ship Windows; a native port is a separate project. |
| Release metadata | `SHA256SUMS`, release notes, SBOM, optional signature | `scripts/checksums.sh` and `scripts/sbom.py` generate the manifest and CycloneDX SBOM; `scripts/sign-release.sh` adds a detached GPG signature when a key is configured |

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
  environment-protected secrets. Do not run untrusted PR code with write tokens. The
  current workflow pins every `uses:` reference and gives only the publish job
  `contents: write`.
- Use one reproducible packaging layer (for example `cargo-deb`/`nfpm`, `makepkg`,
  `appimagetool`, Inno Setup or WiX) and verify the installed payload in a clean
  container. A package that only contains the binary is not complete.
- `scripts/verify-package.sh` extracts each format and checks the daemon, UI launcher,
  renderer project, perception helpers, baker tools, systemd unit, desktop entry and
  icon. `scripts/package.sh` runs it before writing an artifact.
- The AppImage does **not** bundle the pinned Godot renderer or Ollama runtime; the
  decision is recorded below. They are supplied separately on first run, preserving the
  existing checksums and the "unavailable" states.

## Version and update checks

- `buddy version` embeds the crate version and the git tag/commit (or `unknown` when
  built from a source tarball without `.git`). The panel's About dialog runs
  `buddy version` and shows the daemon build string alongside its own UI version.
- `buddy update check` (name open) queries the GitHub Releases API only when the user
  asks; it prints current/latest version and the release URL. No background polling.
- `buddy upgrade` downloads the portable user-local artifact for a user-local install,
  verifies it against the release's `SHA256SUMS`, and prints the manual replacement
  commands. It never installs, never touches a system-package copy, and refuses a
  development checkout. Signatures remain unimplemented.
- Managed tools get their own checks: the pinned Godot version, the managed Ollama
  version, and installed model digests. Existing `buddy ai status`, `buddy ai doctor`
  and the runtime manifest are the starting point; add a `--check` path rather than
  auto-updating a model or runtime.
- Data compatibility: schema migrations and the daily database backup stay authoritative;
  an upgrade refuses to proceed if the data directory cannot be backed up.

## Decisions taken (2026-11-24)

1. **Windows is explicitly unsupported for this release line.** A Windows installer
   cannot make the Unix-socket/systemd/GNOME daemon work; a native port is a separate
   project, not a packaging task. No MSI/EXE is produced.
2. **AppImage does not bundle Godot or Ollama.** It carries the daemon, UI, renderer
   project, helpers and tools; the managed runtimes are fetched/installed on first run
   through the existing installer or supplied with `BUDDY_GODOT`. This keeps the
   AppImage small and the managed-runtime checksum policy in one place.
3. **Signing is optional and starts unsigned.** No paid certificate or signing service
   is assumed. `scripts/sign-release.sh` adds a detached GPG signature when a key is
   configured; without one the release carries only `SHA256SUMS`. The workflow reads
   `BUDDY_SIGNING_PRIVATE_KEY` (ASCII-armored private key), `BUDDY_SIGNING_KEY`
   (fingerprint or email) and optional `BUDDY_SIGNING_PASSPHRASE` from repository
   secrets; absent secrets produce an unsigned release, not a failure.
4. **Arch uses the project-generated PKGBUILD** with a real source checksum. AUR
   maintenance and an official repository are future distribution work, not done here.
5. **Stable channel only; no telemetry and no self-update.** Update commands are explicit
   and report; they do not install.
6. **System packages install under `/usr`; user data stays in `~/.local/share/buddy`.**
   Package manager files and the user-local installer must not overwrite each other.
   There is no automatic data migration.

## Why this is a separate phase

It changes distribution, permissions, installers, secrets and the meaning of a release;
it does not change Buddy's brain. It must not be smuggled into a normal feature branch.
The project rule is still: spike results, written plan, explicit approval, then
implementation. This record is the durable input to that plan.


## Current implementation (2026-11-24)

- `scripts/package.sh tar|deb|rpm|arch|appimage` stages the release binary, renderer
  project, perception helpers, control panel, systemd user unit, desktop entry, icon and
  docs, then produces the requested artifact. It verifies the payload with
  `scripts/verify-package.sh` and writes a `SHA256SUMS` beside it. The portable tar and
  Arch packages were built and verified locally on 2026-11-24.
- The installed binary now finds `/usr/share/buddy/renderer` and
  `/usr/share/buddy/perception` from `/usr/bin/buddy`; the `buddy-ui` launcher uses a
  path relative to itself, so the same wrapper works in `/usr` and in an AppImage.
  `scripts/verify-package.sh` checks the full payload, including the renderer project,
  helper scripts and baker tools.
- `.github/workflows/release.yml` runs tests and the five Linux packaging jobs on a `v*`
  tag or manual dispatch. Every action is pinned by commit SHA, the default token is
  read-only, and only the publish job receives `contents: write`. The publish job
  regenerates a single `SHA256SUMS`, generates a CycloneDX SBOM with
  `scripts/sbom.py`, and uploads both to the GitHub Release.
- `buddy version` now includes the git tag/commit from `crates/buddy/build.rs`.
  `buddy update check` and `buddy update tools` still only report.
- All five Linux formats were built and payload-verified locally on 2026-11-24: tar,
  DEB, RPM, Arch and AppImage. `scripts/release-rehearsal.sh` builds them and generates
  the SBOM/checksum/signature metadata in one command. The RPM spec disables the
  debuginfo package and uses a valid changelog date; the AppImage sources the
  managed-runtime env file and starts the daemon before the panel.
- Still open: the tag-only publish/upload path has not been exercised (the manual run
  skips it by design), and AppImage's first-run managed-runtime UX beyond the env/status
  path. Windows is not an open Phase 9 task because this release line explicitly does not
  target it. `buddy upgrade` is download/verify only; there is no self-install.
- The existing installer remains the supported way to install Godot/Ollama and the
  systemd user service on Linux; packages include the daemon/UI but not those managed
  runtimes.

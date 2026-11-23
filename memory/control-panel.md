---
name: control-panel
description: Native GTK4/libadwaita control panel installed as Buddy/buddy-ui; a socket client covering status, training, senses, AI, skills, workflows, journal and settings
metadata:
  type: project
---

Implemented 2026-11-23 at the owner's request: a visual interface alongside the CLI,
inspired by LM Studio's sidebar-and-dashboard layout, plus a Buddy application icon and
desktop launcher. The owner asked that nearly everything Buddy can do be discoverable
and usable from the window, and that the icon appear in the operating system's
application list.

The durable shape:

- `ui/buddy_ui.py` is a GTK4/libadwaita client of the daemon's Unix socket, using the
  same protocol as `crates/buddy/src/client.rs`. It owns no simulation state, never opens
  SQLite, and asks the daemon for every change. Closing it does not stop Buddy.
- Pages: Home (status, needs, quick actions, waiting approvals, why, snapshot), Talk,
  Train (name, call, words, memories, praise/correction), Senses (all seven sources and privacy),
  Local AI (runtime, model dropdowns, capabilities, voice, doctor, setup commands),
  Skills (policies, folders, undo/result), Workflows (record, guided capture, text and manual steps, replay,
  continue, abort, delete), Journal/export, and Settings (name, size, growth, fullscreen, pictures, quiet,
  privacy, data).
- `ui/buddy.svg` is the application icon; `dist/buddy-ui.desktop` is the launcher.
  `scripts/install.sh` installs the files under the Buddy data directory, `buddy-ui` in
  `~/.local/bin`, the desktop entry in the user applications directory, and SVG/PNG
  icons in hicolor. The window class is `buddy-ui`.
- The panel requires Python 3, PyGObject, GTK 4 and libadwaita; this is optional for the
  daemon, CLI and renderer. Long network operations (model setup, repair, prune) remain
  copyable CLI commands in the panel; model choice, voice, doctor and everything else
  are interactive. See `docs/decisions.md` "Control panel (approved 2026-11-23)".

**Why:** the owner wants configuration and connection to be approachable without
memorising the CLI, while keeping the daemon the single source of truth and keeping
Buddy local-first and free.

**How to apply:** add new user-facing daemon commands to the panel only through the
socket; do not add direct database or file access to the UI. Keep `ui/` and the
installer/desktop files in step. Verify with `python3 -m py_compile ui/buddy_ui.py`,
`desktop-file-validate dist/buddy-ui.desktop`, a page launch with `BUDDY_UI_PAGE`, and
the smoke path that clicks real buttons. Related: [[workflow-recording]],
[[tracked-docs-no-untracked-refs]], [[merge-completed-tasks-into-main]].

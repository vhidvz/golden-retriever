---
name: tray-notifications
description: Buddy's tray icon uses StatusNotifierItem and a custom GTK panel, not the OS notification service; needs/offers drive interactive messages
metadata:
  type: project
---

Implemented 2026-11-24 at the owner's request. The tray icon is a standard
**StatusNotifierItem** (`ui/buddy_tray.py`), so GNOME's AppIndicator extension or
KDE's tray places it where the desktop keeps its own tray icons. The panel is Buddy's
own GTK4 UI; it does **not** use `org.freedesktop.Notifications` or any native toast
daemon.

- **Notifications:** the daemon's existing `waiting` list supplies his own offers
  (including pattern/habit offers); the tray adds need messages for hunger, thirst,
  boredom, company and fatigue above thresholds, with cooldowns. A returning-user message is
  information-only and auto-closes. Quiet mode records the message but does not pop the
  panel open.
- **Actions:** every button is a normal daemon command. `feed`, `water`, `pet`,
  `approve`/`deny`, and the new `play` command. `play` makes him bow, lowers boredom and
  lifts mood. Fatigue offers a "Let him nap" action (`sleep`) rather than forcing rest.
- **Presentation:** cards in a warm, opaque panel with a per-notice action row,
  dismiss/clear, a small Buddy avatar and status line. Interactive notices remain in
  the panel until answered/dismissed; the panel auto-hides after twelve seconds when the
  pointer is not over it, while the tray count remains. A native DBusMenu provides the
  conventional single-click tray menu (`Open Buddy messages`, Quiet, Privacy, Quit);
  double-click opens the custom panel directly on GNOME.
- **Host requirement:** GNOME needs the AppIndicator/KStatusNotifierItem extension
  (enabled on the development machine); KDE provides the host natively. On hosts with
  no SNI watcher the app retries registration. The icon is not a floating window.
  GNOME's extension uses single-click-for-menu/double-click-for-activate; KDE activates
  directly according to its own tray settings.
- **Install:** `scripts/install.sh` installs `buddy-tray`, its user service and the UI
  files; `--no-tray` opts out. Packaging includes the launcher and unit; a system
  package user enables `buddy-tray.service` explicitly. The AppImage starts the tray
  from `AppRun`.

**Why:** the owner wants a modern, non-intrusive channel that feels like Buddy, works
without a native notification daemon, and still uses the OS tray placement.

**How to apply:** keep notification policy visual and client-side; the daemon remains
the only decision-maker and every action goes through it. Do not add a second
notification transport or a raw keyboard listener.

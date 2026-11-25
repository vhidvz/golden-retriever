---
name: tray-notifications
description: Buddy's tray icon uses StatusNotifierItem and a custom GTK panel, not the OS notification service; needs/offers drive interactive messages
metadata:
  type: project
---

Implemented 2026-11-24 at the owner's request. On GNOME the project's Shell extension
adds a top-bar button that calls `local.buddy.TrayControl` directly on one click; on
KDE/other hosts `ui/buddy_tray.py` registers a standard **StatusNotifierItem**. Either
way the icon lives in the desktop's own tray. The panel is Buddy's own GTK4 UI; it does
**not** use `org.freedesktop.Notifications` or any native toast daemon.

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
  pointer is not over it, while the tray count remains. GNOME's top-bar button opens the
  panel directly on one click; non-GNOME SNI hosts get the native DBusMenu and their own
  activation behaviour.
- **Host requirement:** GNOME needs the project's `buddy@buddy.local` Shell extension
  (it also provides the desktop sense); KDE provides the StatusNotifier host natively.
  On hosts with no SNI watcher the app retries registration. The icon is not a floating
  window.
- **Verification:** manual GitHub Actions run 45533077343 on 2026-11-25 passed tests,
  tar, DEB, RPM, Arch, AppImage and metadata with the tray files in every package; the
  tag-only publish job was intentionally skipped. The installed user service is active
  and its SNI/DBusMenu answered an `Activate`/`Event` smoke test locally.
- **Install:** `scripts/install.sh` installs `buddy-tray`, its user service and the UI
  files; `--no-tray` opts out. Packaging includes the launcher and unit; a system
  package user enables `buddy-tray.service` explicitly. The AppImage starts the tray
  from `AppRun`.

**Why:** the owner wants a modern, non-intrusive channel that feels like Buddy, works
without a native notification daemon, and still uses the OS tray placement.

**How to apply:** keep notification policy visual and client-side; the daemon remains
the only decision-maker and every action goes through it. Do not add a second
notification transport or a raw keyboard listener.

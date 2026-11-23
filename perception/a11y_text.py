#!/usr/bin/env python3
"""Replace one accessible editable text field and verify a semantic text event.

The daemon calls this only for a saved `type_text` workflow step while the
`accessibility` sense is allowed. The text arrives on stdin, never on the command
line, so it is not exposed in the process list. No keystrokes, clipboard, mouse
coordinates or password fields are used.

Prints one JSON line:
  {"ok": true, "verified": true, "why": "text changed in 'Search'"}
  {"ok": false, "verified": false, "why": ...}
"""
import argparse
import json
import sys

BLOCKED_APPS = {
    "gnome-shell",
    "kgx",
    "gnome-terminal",
    "org.gnome.Console",
    "org.gnome.Terminal",
    "konsole",
    "alacritty",
    "kitty",
    "xterm",
}


def say(ok, verified, why):
    print(json.dumps({"ok": ok, "verified": verified, "why": why}, ensure_ascii=False))
    sys.exit(0 if ok and verified else 1)


try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, GLib
except Exception as error:  # noqa: BLE001
    say(False, False, f"AT-SPI is not available: {error}")


def matches(node, role, name, found):
    try:
        if node.get_role_name() == role and (node.get_name() or "") == name:
            found.append(node)
        for index in range(node.get_child_count()):
            matches(node.get_child_at_index(index), role, name, found)
    except Exception:
        pass


def frame_of(node):
    current = node
    while current is not None:
        try:
            if current.get_role_name() in ("frame", "window"):
                return current
            current = current.get_parent()
        except Exception:
            return None
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True)
    parser.add_argument("--frame", required=True)
    parser.add_argument("--role", required=True)
    parser.add_argument("--name", default="")
    args = parser.parse_args()

    if args.app in BLOCKED_APPS or args.role == "password text":
        say(False, False, "that application or text field is not allowed")

    text = sys.stdin.read(2001)
    if not text:
        say(False, False, "no text was supplied")
    if len(text) > 2000:
        say(False, False, "the text is longer than 2000 characters")

    desktop = Atspi.get_desktop(0)
    app = None
    for index in range(desktop.get_child_count()):
        candidate = desktop.get_child_at_index(index)
        if candidate.get_name() == args.app:
            app = candidate
            break
    if app is None:
        say(False, False, f"application '{args.app}' was not found")

    frame = None
    for index in range(app.get_child_count()):
        candidate = app.get_child_at_index(index)
        if candidate.get_role_name() in ("frame", "window") and candidate.get_name() == args.frame:
            frame = candidate
            break
    if frame is None:
        say(False, False, f"frame '{args.frame}' was not found")

    found = []
    matches(frame, args.role, args.name, found)
    if not found:
        say(False, False, f"text field '{args.role}' named '{args.name}' was not found")
    if len(found) > 1:
        say(False, False, f"{len(found)} text fields match; give the target an accessible name")
    control = found[0]
    try:
        if control.get_role_name() == "password text" or not control.is_editable_text():
            say(False, False, "the target is not an editable text field")
    except Exception as error:  # noqa: BLE001
        say(False, False, f"the text field could not be inspected: {error}")

    verified = [False]
    loop = GLib.MainLoop()

    def on_event(event):
        try:
            source = event.source
            if source is None:
                return
            if source.get_application().get_name() != args.app:
                return
            frame = frame_of(source)
            if frame is None or frame.get_name() != args.frame:
                return
            verified[0] = True
            loop.quit()
        except Exception:
            return

    listener = Atspi.EventListener.new(on_event)
    # Only an insertion proves the new text was accepted; the clear before it emits a deletion.
    try:
        listener.register("object:text-changed:insert")
    except Exception:
        pass

    def replace():
        try:
            if not control.set_text_contents(""):
                say(False, False, "the field refused to be cleared")
        except Exception as error:  # noqa: BLE001
            say(False, False, f"the field could not be cleared: {error}")
        try:
            if not control.set_text_contents(text):
                say(False, False, "the field refused the text")
        except Exception as error:  # noqa: BLE001
            say(False, False, f"the text could not be entered: {error}")
        return False

    GLib.timeout_add(150, replace)
    GLib.timeout_add_seconds(3, loop.quit)
    loop.run()

    if verified[0]:
        say(True, True, f"text changed in '{args.name or args.role}'")
    say(True, False, "the text interface accepted the value but no text-change event followed")


if __name__ == "__main__":
    main()

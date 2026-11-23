#!/usr/bin/env python3
"""Invoke one accessible action and verify that a semantic event followed.

The daemon locates a saved workflow step by app, frame, role and accessible name and
runs this helper only while the `accessibility` sense is allowed. It never sends mouse
coordinates, keystrokes, clipboard data or text.

Prints one JSON line:
  {"ok": true, "verified": true, "why": "click on 'Press me' changed the frame"}
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
ALLOWED_ACTIONS = {"click", "press", "activate", "toggle", "select"}
EVENTS = [
    "object:property-change:accessible-name",
    "object:state-changed:checked",
    "object:state-changed:selected",
    "object:state-changed:pressed",
    "object:text-changed:insert",
    "object:text-changed:delete",
    "window:activate",
]


def say(ok, verified, why):
    print(json.dumps({"ok": ok, "verified": verified, "why": why}, ensure_ascii=False))
    sys.exit(0 if ok and verified else 1)


try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, GLib
except Exception as error:  # noqa: BLE001
    say(False, False, f"AT-SPI is not available: {error}")


def find(node, role=None, name=None):
    try:
        if (role is None or node.get_role_name() == role) and (name is None or node.get_name() == name):
            return node
        for index in range(node.get_child_count()):
            found = find(node.get_child_at_index(index), role, name)
            if found is not None:
                return found
    except Exception:
        pass
    return None


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
    parser.add_argument("--name", required=True)
    args = parser.parse_args()

    if args.app in BLOCKED_APPS or args.role == "password text":
        say(False, False, "that application or control is not allowed")

    desktop = Atspi.get_desktop(0)
    app = None
    for index in range(desktop.get_child_count()):
        candidate = desktop.get_child_at_index(index)
        if candidate.get_name() == args.app and find(candidate, "frame", args.frame) is not None:
            app = candidate
            break
    if app is None:
        say(False, False, f"application '{args.app}' with frame '{args.frame}' was not found")

    frame = find(app, "frame", args.frame)
    if frame is None:
        say(False, False, f"frame '{args.frame}' was not found")
    control = find(frame, args.role, args.name)
    if control is None:
        say(False, False, f"control '{args.role}' named '{args.name}' was not found")

    count = control.get_n_actions()
    index = None
    for i in range(count):
        try:
            if control.get_action_name(i) in ALLOWED_ACTIONS:
                index = i
                break
        except Exception:
            continue
    if index is None:
        say(False, False, f"control '{args.name}' has no allowed action")

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
    for kind in EVENTS:
        try:
            listener.register(kind)
        except Exception:
            pass

    def invoke():
        try:
            control.do_action(index)
        except Exception as error:  # noqa: BLE001
            say(False, False, f"the action failed: {error}")
        return False

    GLib.timeout_add(150, invoke)
    GLib.timeout_add_seconds(3, loop.quit)
    loop.run()

    if verified[0]:
        say(True, True, f"{control.get_action_name(index)} on '{args.name}' produced a semantic event")
    say(True, False, "the action ran but no semantic event followed")


if __name__ == "__main__":
    main()

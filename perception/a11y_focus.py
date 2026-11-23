#!/usr/bin/env python3
"""Report the focused accessible control as one workflow step.

Buddy uses this only while the `accessibility` sense is allowed. It reads the
semantic accessibility tree, never screen pixels, keys, clipboard or text content.
The caller (the daemon) decides whether capture is allowed and persists the step.

Prints one JSON line:
  {"ok": true, "step": {"kind": "activate", "app": ..., "frame": ..., "role": ..., "name": ...}}
  {"ok": false, "why": ...}
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


def fail(why):
    print(json.dumps({"ok": False, "why": why}, ensure_ascii=False))
    sys.exit(1)


try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
except Exception as error:  # noqa: BLE001 - the helper must report why it cannot run
    fail(f"AT-SPI is not available: {error}")


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


def matching_controls(node, role, name, found):
    try:
        if node.get_role_name() == role and (node.get_name() or "") == name:
            found.append(node)
        for index in range(node.get_child_count()):
            matching_controls(node.get_child_at_index(index), role, name, found)
    except Exception:
        return


def focused_controls(node):
    try:
        state = node.get_state_set()
        if state is not None and state.contains(Atspi.StateType.FOCUSED):
            yield node
        for index in range(node.get_child_count()):
            yield from focused_controls(node.get_child_at_index(index))
    except Exception:
        return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--text", action="store_true", help="capture the focused editable text field instead of an action")
    args = parser.parse_args()

    desktop = Atspi.get_desktop(0)
    for node in focused_controls(desktop):
        try:
            app = node.get_application().get_name()
            role = node.get_role_name()
            name = node.get_name() or ""
            if app in BLOCKED_APPS or role == "password text":
                continue
            if role in ("frame", "window", "application", "desktop frame"):
                continue
            if args.text:
                if not node.is_editable_text():
                    continue
            elif node.get_n_actions() < 1:
                continue
            frame = frame_of(node)
            frame_name = frame.get_name() if frame is not None else ""
            if not frame_name:
                continue
            if args.text:
                candidates = []
                matching_controls(frame, role, name, candidates)
                if len(candidates) > 1:
                    fail(f"{len(candidates)} text fields in this frame match role '{role}' and name '{name}'; give the target an accessible name")
            print(
                json.dumps(
                    {
                        "ok": True,
                        "step": {
                            "kind": "type_text" if args.text else "activate",
                            "app": app,
                            "frame": frame_name,
                            "role": role,
                            "name": name,
                        },
                    },
                    ensure_ascii=False,
                )
            )
            return
        except Exception:
            continue
    fail(
        "focus an editable text field (passwords and terminals are not allowed)"
        if args.text
        else "focus a button, menu item or other control that has an accessible action"
    )


if __name__ == "__main__":
    main()

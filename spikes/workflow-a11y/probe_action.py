#!/usr/bin/env python3
"""Find the probe app's button and invoke its accessible action.

This proves the path a future replay step would use: locate by role and name, call
do_action, then observe the resulting semantic change. It deliberately does not send
synthetic mouse or keyboard input.
"""
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi


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


desktop = Atspi.get_desktop(0)
app = None
for index in range(desktop.get_child_count()):
    candidate = desktop.get_child_at_index(index)
    if candidate.get_name() == "python3" and find(candidate, "frame", "Buddy A11y Probe") is not None:
        app = candidate
        break
if app is None:
    raise SystemExit("probe app not found; start probe_app.py first")

button = find(app, "button", "Press me")
label = find(app, "label", "ready")
entry = find(app, "text", "")
print("button actions:", button.get_n_actions(), [button.get_action_name(i) for i in range(button.get_n_actions())])
print("label before:", label.get_name())
button.do_action(0)
time.sleep(0.5)
print("label after pressed:", find(app, "label", "pressed") is not None)
print("entry editable:", entry.is_editable_text())
entry.set_text_contents("hello from atspi")
print("text entry attempted; verify by the object:text-changed:insert event")

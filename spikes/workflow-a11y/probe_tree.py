#!/usr/bin/env python3
"""Print the accessible tree of the probe app (or every AT-SPI application)."""
import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi


def walk(node, depth=0, limit=7):
    if depth > limit:
        return
    try:
        print("  " * depth, node.get_role_name(), repr(node.get_name()), "children=", node.get_child_count())
        for index in range(node.get_child_count()):
            walk(node.get_child_at_index(index), depth + 1, limit)
    except Exception as error:
        print("  " * depth, "ERR", error)


desktop = Atspi.get_desktop(0)
for index in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(index)
    print("APP", repr(app.get_name()), "children=", app.get_child_count())
    if app.get_name() == "python3":
        walk(app)

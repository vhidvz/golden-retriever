#!/usr/bin/env python3
"""Listen for the semantic events a workflow recorder could use.

Run it before probe_action.py. It prints name changes, text changes and focus/state
changes for four seconds. The result shows both what is available and what is not:
a programmatic button action changed the label's accessible name, but no dedicated
action event was emitted.
"""
import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib


def on_event(event):
    try:
        source = event.source
        print(
            "EVENT",
            event.type,
            "app=",
            source.get_application().get_name() if source else "?",
            "role=",
            source.get_role_name() if source else "?",
            "name=",
            repr(source.get_name()) if source else "?",
            "detail=",
            event.detail1,
            event.detail2,
            flush=True,
        )
    except Exception as error:
        print("EVENT-ERR", error, flush=True)


listener = Atspi.EventListener.new(on_event)
for kind in [
    "object:state-changed:focused",
    "object:state-changed:pressed",
    "object:state-changed:checked",
    "object:state-changed:selected",
    "object:property-change:accessible-name",
    "object:text-changed:insert",
    "object:text-changed:delete",
    "window:activate",
]:
    try:
        listener.register(kind)
    except Exception as error:
        print("register failed", kind, error, flush=True)

loop = GLib.MainLoop()
GLib.timeout_add_seconds(4, loop.quit)
loop.run()

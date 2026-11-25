#!/usr/bin/env python3
"""Measure semantic text-change events from a real GTK application.

This is a research probe, not part of Buddy's runtime. It registers the same
AT-SPI signal a context observer would use, changes the text of the focused
editable control through the accessibility interface, and reports how many
events arrived, how they were shaped and what the listener cost.

It never reads the clipboard, never synthesises keys and never inspects a
password field. Text stays in this process.
"""
import argparse
import json
import resource
import sys
import time

try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, GLib
except Exception as error:  # noqa: BLE001
    print(json.dumps({"ok": False, "why": f"AT-SPI is unavailable: {error}"}))
    sys.exit(1)


def walk(node):
    try:
        yield node
        for index in range(node.get_child_count()):
            yield from walk(node.get_child_at_index(index))
    except Exception:
        return


def largest_editable(app):
    found = []
    for node in walk(app):
        try:
            if node.get_role_name() == "text" and node.is_editable_text():
                extents = node.get_extents(Atspi.CoordType.SCREEN)
                found.append((extents.width * extents.height, node))
        except Exception:
            continue
    return max(found, default=(0, None))[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", default="gnome-text-editor")
    parser.add_argument("--turns", type=int, default=40)
    parser.add_argument("--pause", type=float, default=0.08)
    parser.add_argument("--idle", type=float, default=8.0)
    args = parser.parse_args()

    desktop = Atspi.get_desktop(0)
    app = None
    for index in range(desktop.get_child_count()):
        candidate = desktop.get_child_at_index(index)
        if candidate.get_name() == args.app:
            app = candidate
            break
    if app is None:
        print(json.dumps({"ok": False, "why": f"application {args.app!r} is not running"}))
        return

    control = largest_editable(app)
    if control is None:
        print(json.dumps({"ok": False, "why": "no editable text control"}))
        return
    try:
        control.grab_focus()
    except Exception:
        pass

    events = []
    started = time.monotonic()

    def on_event(event):
        try:
            source = event.source
            app_name = source.get_application().get_name() if source else ""
            role = source.get_role_name() if source else ""
            state = source.get_state_set() if source else None
            focused = bool(state and state.contains(Atspi.StateType.FOCUSED))
            frame = ""
            current = source
            while current is not None:
                try:
                    if current.get_role_name() in ("frame", "window"):
                        frame = current.get_name() or ""
                        break
                    current = current.get_parent()
                except Exception:
                    break
            events.append(
                {
                    "at": time.monotonic() - started,
                    "type": event.type,
                    "detail1": event.detail1,
                    "detail2": event.detail2,
                    "app": app_name,
                    "role": role,
                    "focused": focused,
                    "frame": frame,
                }
            )
        except Exception:
            return

    listener = Atspi.EventListener.new(on_event)
    for signal in ("object:text-changed", "object:state-changed:focused"):
        try:
            listener.register(signal)
        except Exception:
            pass

    loop = GLib.MainLoop()

    def run_turns():
        for turn in range(args.turns):
            text = f"Buddy semantic event probe, turn {turn + 1}. The quick brown fox jumps over the lazy dog."
            try:
                control.set_text_contents(text)
            except Exception:
                pass
            time.sleep(args.pause)
        return False

    GLib.timeout_add(100, run_turns)
    GLib.timeout_add_seconds(int(args.turns * args.pause) + 2, loop.quit)
    loop.run()

    before = resource.getrusage(resource.RUSAGE_SELF)
    idle_loop = GLib.MainLoop()
    GLib.timeout_add_seconds(int(args.idle), idle_loop.quit)
    idle_loop.run()
    after = resource.getrusage(resource.RUSAGE_SELF)

    # Some applications expose the same buffer as two accessible text objects; only
    # the focused one is the control the user is actually typing into.
    relevant = [event for event in events if event["focused"]]
    inserts = [event for event in relevant if event["type"] == "object:text-changed:insert"]
    deletes = [event for event in relevant if event["type"] == "object:text-changed:delete"]
    focus = [event for event in relevant if event["type"] == "object:state-changed:focused"]
    per_turn = len(inserts) / args.turns if args.turns else 0
    cpu = (after.ru_utime + after.ru_stime) - (before.ru_utime + before.ru_stime)
    print(
        json.dumps(
            {
                "ok": True,
                "app": args.app,
                "turns": args.turns,
                "pause_secs": args.pause,
                "events": len(events),
                "focused_events": len(relevant),
                "inserts": len(inserts),
                "deletes": len(deletes),
                "focus_events": len(focus),
                "inserts_per_turn": round(per_turn, 2),
                "duplicate_insert_ratio": round(max(0.0, per_turn - 1.0), 2),
                "frames": sorted({event["frame"] for event in events if event["frame"]}),
                "roles": sorted({event["role"] for event in events if event["role"]}),
                "idle_cpu_secs": round(cpu, 4),
                "idle_secs": args.idle,
                "cpu_percent_idle": round(100.0 * cpu / args.idle, 2) if args.idle else None,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()

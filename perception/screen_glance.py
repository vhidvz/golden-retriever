#!/usr/bin/env python3
"""Takes one look at the screen for Buddy: a single frame, scaled down, written
as a JPEG to a private file, and nothing else.

It goes through the desktop portal (the same consent-based route screen sharing
uses), so the first run asks the user which screen to share. The portal's
restore token is passed back so later looks do not ask again. The session is
closed as soon as the frame is saved, so the "screen is being shared" indicator
shows only for the moment of the look.

Prints one JSON line: {"ok": true, "path", "restore_token"} or {"ok": false, "error"}.
"""
import argparse, json, os, sys
import gi
gi.require_version("Gst", "1.0")
from gi.repository import Gio, GLib, Gst

PORTAL = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"
CAST = "org.freedesktop.portal.ScreenCast"


class Refused(Exception):
    pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--restore-token", default="")
    ap.add_argument("--width", type=int, default=1720)
    ap.add_argument("--wait", type=int, default=90, help="seconds to wait for the user to answer the sharing prompt")
    args = ap.parse_args()

    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    sender = bus.get_unique_name()[1:].replace(".", "_")
    counter = [0]

    def request(method, params, options):
        """Calls a portal method and waits for its Response signal."""
        counter[0] += 1
        token = f"buddy{os.getpid()}_{counter[0]}"
        options["handle_token"] = GLib.Variant("s", token)
        loop, answer = GLib.MainLoop(), {}

        def responded(_bus, _sender, _path, _iface, _signal, parameters):
            answer["code"], answer["results"] = parameters.unpack()
            loop.quit()

        sub = bus.signal_subscribe(PORTAL, "org.freedesktop.portal.Request", "Response", f"/org/freedesktop/portal/desktop/request/{sender}/{token}", None, Gio.DBusSignalFlags.NO_MATCH_RULE, responded)
        bus.call_sync(PORTAL, PATH, CAST, method, GLib.Variant.new_tuple(*params, GLib.Variant("a{sv}", options)), None, Gio.DBusCallFlags.NONE, -1, None)
        timer = GLib.timeout_add_seconds(args.wait, loop.quit)
        loop.run()
        GLib.source_remove(timer) if "code" in answer else None
        bus.signal_unsubscribe(sub)
        if "code" not in answer:
            raise Refused("nobody answered the screen-sharing prompt")
        if answer["code"] != 0:
            raise Refused("screen sharing was declined")
        return answer["results"]

    session = None
    try:
        created = request("CreateSession", [], {"session_handle_token": GLib.Variant("s", f"buddy{os.getpid()}")})
        session = created["session_handle"]
        select = {"types": GLib.Variant("u", 1), "multiple": GLib.Variant("b", False), "cursor_mode": GLib.Variant("u", 1), "persist_mode": GLib.Variant("u", 2)}
        if args.restore_token:
            select["restore_token"] = GLib.Variant("s", args.restore_token)
        request("SelectSources", [GLib.Variant("o", session)], select)
        started = request("Start", [GLib.Variant("o", session), GLib.Variant("s", "")], {})
        node, properties = started["streams"][0]
        width, height = properties.get("size", (0, 0))

        reply, fds = bus.call_with_unix_fd_list_sync(PORTAL, PATH, CAST, "OpenPipeWireRemote", GLib.Variant("(oa{sv})", (session, {})), None, Gio.DBusCallFlags.NONE, -1, None, None)
        fd = fds.get(reply.unpack()[0])

        Gst.init(None)
        scale = ""
        if width and height and width > args.width:
            scale = f"! videoscale ! video/x-raw,width={args.width},height={max(2, round(height * args.width / width / 2) * 2)} "
        pipeline = Gst.parse_launch(f"pipewiresrc fd={fd} path={node} ! videoconvert {scale}! jpegenc snapshot=true quality=82 ! filesink location={GLib.shell_quote(args.out)}")
        pipeline.set_state(Gst.State.PLAYING)
        message = pipeline.get_bus().timed_pop_filtered(15 * Gst.SECOND, Gst.MessageType.EOS | Gst.MessageType.ERROR)
        pipeline.set_state(Gst.State.NULL)
        if message is None:
            raise Refused("no frame arrived from the screen within 15 seconds")
        if message.type == Gst.MessageType.ERROR:
            raise Refused(f"could not read the screen: {message.parse_error()[0].message}")
        os.chmod(args.out, 0o600)
        print(json.dumps({"ok": True, "path": args.out, "restore_token": started.get("restore_token", "")}))
    except Refused as e:
        print(json.dumps({"ok": False, "error": str(e)}))
    except GLib.Error as e:
        print(json.dumps({"ok": False, "error": f"the desktop portal failed: {e.message}"}))
    finally:
        if session:
            try:
                bus.call_sync(PORTAL, session, "org.freedesktop.portal.Session", "Close", None, None, Gio.DBusCallFlags.NONE, 2000, None)
            except GLib.Error:
                pass


if __name__ == "__main__":
    main()

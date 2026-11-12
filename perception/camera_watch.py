#!/usr/bin/env python3
"""Buddy's camera sense: is someone there?

Looks only at how much a tiny greyscale version of the picture (64x48, four
frames a second) changes from one frame to the next. Movement means someone is
there; two minutes of stillness means they have probably gone. No frame is
stored, sent anywhere or shown to any model.

Prints one JSON line per observation: {"kind": "presence", "data": {"on": bool}}
or, when there is no camera, {"kind": "unavailable", "data": {"why": ...}}.
"""
import argparse, glob, json, sys, time
import gi
gi.require_version("Gst", "1.0")
from gi.repository import Gst

MOVING = 0.012        # mean change per pixel (0..1) that counts as movement
GONE_AFTER = 120.0    # seconds without movement before "nobody there"


def say(kind, **data):
    print(json.dumps({"kind": kind, "data": data}), flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pipeline", help="a GStreamer source to use instead of the camera (for tests)")
    ap.add_argument("--gone-after", type=float, default=GONE_AFTER)
    ap.add_argument("--seconds", type=float, default=0, help="stop after this long (for tests)")
    args = ap.parse_args()
    if not args.pipeline and not glob.glob("/dev/video*"):
        say("unavailable", why="no camera found")
        return
    Gst.init(None)
    source = args.pipeline or "v4l2src"
    pipeline = Gst.parse_launch(f"{source} ! videoconvert ! videoscale ! videorate ! video/x-raw,format=GRAY8,width=64,height=48,framerate=4/1 ! appsink name=sink max-buffers=2 drop=true sync=false")
    sink = pipeline.get_by_name("sink")
    pipeline.set_state(Gst.State.PLAYING)
    previous, present, last_movement, started = None, None, 0.0, time.monotonic()
    try:
        while not args.seconds or time.monotonic() - started < args.seconds:
            sample = sink.emit("try-pull-sample", Gst.SECOND)
            if sample is None:
                if pipeline.get_bus().pop_filtered(Gst.MessageType.ERROR | Gst.MessageType.EOS):
                    say("unavailable", why="the camera stopped")
                    return
                continue
            ok, info = sample.get_buffer().map(Gst.MapFlags.READ)
            if not ok:
                continue
            frame = bytes(info.data)
            sample.get_buffer().unmap(info)
            now = time.monotonic()
            if previous is not None and len(previous) == len(frame):
                change = sum(abs(a - b) for a, b in zip(frame, previous)) / (255.0 * len(frame))
                if change > MOVING:
                    last_movement = now
                    if present is not True:
                        present = True
                        say("presence", on=True)
                elif present is not False and now - max(last_movement, started) > args.gone_after:
                    present = False
                    say("presence", on=False)
            previous = frame
    finally:
        pipeline.set_state(Gst.State.NULL)


if __name__ == "__main__":
    main()

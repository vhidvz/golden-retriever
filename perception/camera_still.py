#!/usr/bin/env python3
"""Take one webcam frame for Buddy and write it to a private JPEG.

This is a one-shot, explicit look, not continuous camera vision. The daemon
pauses the presence helper, runs this script, reads and deletes the image, and
sends it only to the local vision model. No frame is kept.

Prints one JSON line:
  {"ok": true, "path": "..."}
  {"ok": false, "error": "..."}
"""
import argparse
import glob
import json
import os
import sys

try:
    import gi

    gi.require_version("Gst", "1.0")
    from gi.repository import Gst, GLib
except Exception as error:  # noqa: BLE001
    print(json.dumps({"ok": False, "error": f"GStreamer is not available: {error}"}))
    sys.exit(1)


DEFAULT_PIPELINE = (
    "v4l2src num-buffers=1 ! videoconvert ! videoscale ! "
    "video/x-raw,width=1280,height=720 ! jpegenc quality=85 ! "
    "filesink location={out}"
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("--pipeline", help="a full GStreamer pipeline with {out}; for tests")
    args = parser.parse_args()

    pipeline_text = args.pipeline or os.environ.get("BUDDY_CAMERA_PIPELINE") or DEFAULT_PIPELINE
    if "{out}" not in pipeline_text:
        print(json.dumps({"ok": False, "error": "the camera pipeline has no {out} placeholder"}))
        sys.exit(1)
    if not args.pipeline and not os.environ.get("BUDDY_CAMERA_PIPELINE") and not glob.glob("/dev/video*"):
        print(json.dumps({"ok": False, "error": "no camera found"}))
        sys.exit(1)

    Gst.init(None)
    pipeline_text = pipeline_text.replace("{out}", args.out)
    try:
        pipeline = Gst.parse_launch(pipeline_text)
    except Exception as error:  # noqa: BLE001
        print(json.dumps({"ok": False, "error": f"the camera pipeline is invalid: {error}"}))
        sys.exit(1)

    pipeline.set_state(Gst.State.PLAYING)
    try:
        message = pipeline.get_bus().timed_pop_filtered(
            12 * Gst.SECOND, Gst.MessageType.EOS | Gst.MessageType.ERROR
        )
        if message is None:
            print(json.dumps({"ok": False, "error": "the camera did not deliver a frame within 12 seconds"}))
            sys.exit(1)
        if message.type == Gst.MessageType.ERROR:
            error, debug = message.parse_error()
            print(json.dumps({"ok": False, "error": f"the camera failed: {error.message}"}))
            sys.exit(1)
    finally:
        pipeline.set_state(Gst.State.NULL)

    if not os.path.isfile(args.out) or os.path.getsize(args.out) == 0:
        print(json.dumps({"ok": False, "error": "the camera delivered no image"}))
        sys.exit(1)
    os.chmod(args.out, 0o600)
    print(json.dumps({"ok": True, "path": args.out}))


if __name__ == "__main__":
    main()

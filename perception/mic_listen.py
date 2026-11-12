#!/usr/bin/env python3
"""Buddy's hearing: was there a sound, how loud, and was someone speaking?

Measures loudness in tenth-of-a-second slices against the room's own
background level. A sudden loud sound is reported as such. A stretch that
sounds like speech (half a second to fifteen seconds, ended by a pause) is
written to a private file for the daemon to have understood on this machine;
the daemon deletes it straight afterwards. Nothing else is kept.

Prints one JSON line per observation:
  {"kind": "sound", "data": {"level": 0..1}}
  {"kind": "utterance", "data": {"path": ..., "secs": ...}}   (only with --utterances DIR)
  {"kind": "unavailable", "data": {"why": ...}}               (no microphone)
"""
import argparse, array, json, math, os, subprocess, time, wave
import gi
gi.require_version("Gst", "1.0")
from gi.repository import Gst

STANDS_OUT_DB = 18.0     # above the background level
AUDIBLE_DB = -38.0       # quieter than this is never reported
MIN_GAP = 2.0            # seconds between reports


def say(kind, **data):
    print(json.dumps({"kind": kind, "data": data}), flush=True)


def has_microphone():
    try:
        sources = subprocess.run(["pactl", "list", "short", "sources"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.TimeoutExpired):
        return False
    return any(line.strip() and ".monitor" not in line for line in sources.splitlines())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pipeline", help="a GStreamer source to use instead of the microphone (for tests)")
    ap.add_argument("--seconds", type=float, default=0, help="stop after this long (for tests)")
    ap.add_argument("--utterances", help="a private directory to write stretches of speech into")
    args = ap.parse_args()
    args.pipeline = args.pipeline or os.environ.get("BUDDY_MIC_PIPELINE")   # the daemon's tests feed it a recording this way
    if not args.pipeline and not has_microphone():
        say("unavailable", why="no microphone found")
        return
    Gst.init(None)
    source = args.pipeline or "pulsesrc"
    pipeline = Gst.parse_launch(f"{source} ! audioconvert ! audioresample ! audio/x-raw,format=S16LE,channels=1,rate=16000 ! appsink name=sink max-buffers=20 drop=true sync=false")
    sink = pipeline.get_by_name("sink")
    pipeline.set_state(Gst.State.PLAYING)
    background, last_report, started, pending = -60.0, 0.0, time.monotonic(), array.array("h")
    speech, quiet_slices, recent, count = None, 0, [], 0   # the utterance being collected, if any
    try:
        while not args.seconds or time.monotonic() - started < args.seconds:
            sample = sink.emit("try-pull-sample", Gst.SECOND)
            if sample is None:
                if pipeline.get_bus().pop_filtered(Gst.MessageType.ERROR | Gst.MessageType.EOS):
                    say("unavailable", why="the microphone stopped")
                    return
                continue
            ok, info = sample.get_buffer().map(Gst.MapFlags.READ)
            if not ok:
                continue
            pending.frombytes(bytes(info.data)[: len(info.data) // 2 * 2])
            sample.get_buffer().unmap(info)
            while len(pending) >= 1600:    # a tenth of a second
                piece, pending = pending[:1600], pending[1600:]
                rms = math.sqrt(sum(s * s for s in piece) / len(piece)) / 32768.0
                level_db = 20.0 * math.log10(max(rms, 1e-6))
                now = time.monotonic()
                if args.utterances:
                    voiced = level_db > background + 10.0 and level_db > AUDIBLE_DB
                    if speech is None:
                        recent = (recent + [piece])[-3:]          # a little lead-in, so the first syllable is not clipped
                        if voiced:
                            speech, quiet_slices = array.array("h", [s for p in recent for s in p]), 0
                    else:
                        speech.extend(piece)
                        quiet_slices = 0 if voiced else quiet_slices + 1
                        secs = len(speech) / 16000.0
                        if quiet_slices >= 8 or secs >= 15.0:
                            if 0.8 <= secs - 0.8:                 # at least half a second of it besides the trailing pause
                                count += 1
                                path = os.path.join(args.utterances, f"heard-{os.getpid()}-{count}.wav")
                                with wave.open(path, "wb") as out:
                                    out.setnchannels(1); out.setsampwidth(2); out.setframerate(16000); out.writeframes(speech.tobytes())
                                os.chmod(path, 0o600)
                                say("utterance", path=path, secs=round(secs, 1))
                            speech, recent = None, []
                if level_db > background + STANDS_OUT_DB and level_db > AUDIBLE_DB and now - last_report > MIN_GAP:
                    last_report = now
                    say("sound", level=round(min(max((level_db + 40.0) / 35.0, 0.0), 1.0), 2))
                else:
                    # The background follows quiet slices quickly and loud ones slowly.
                    background += (level_db - background) * (0.05 if level_db < background + 6.0 else 0.005)
    finally:
        pipeline.set_state(Gst.State.NULL)


if __name__ == "__main__":
    main()

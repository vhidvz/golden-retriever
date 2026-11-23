#!/usr/bin/env python3
"""Read a BVH file and report its skeleton and motion block.

This is a spike tool, not part of Buddy's runtime. It uses only the standard library so
it can run on the development machine without installing AI4Animation or its Python
framework.
"""
import argparse
import json
import sys


def parse_bvh(text):
    joints = []
    joint_stack = []
    blocks = []
    mode = "hierarchy"
    frames = None
    frame_time = None
    motion_lines = []

    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line:
            continue
        if mode == "hierarchy":
            if line.startswith("ROOT ") or line.startswith("JOINT "):
                name = line.split()[1]
                parent = joint_stack[-1] if joint_stack else None
                joints.append({"name": name, "parent": parent, "offset": None, "channels": []})
                joint_stack.append(name)
                blocks.append("joint")
            elif line == "End Site":
                blocks.append("end")
            elif line in ("HIERARCHY", "{"):
                continue
            elif line == "}":
                if not blocks:
                    raise ValueError(f"line {number}: unexpected closing brace")
                if blocks.pop() == "joint" and joint_stack:
                    joint_stack.pop()
            elif line.startswith("OFFSET"):
                values = [float(v) for v in line.split()[1:4]]
                if len(values) != 3:
                    raise ValueError(f"line {number}: OFFSET needs three numbers")
                if blocks and blocks[-1] == "joint":
                    joints[-1]["offset"] = values
            elif line.startswith("CHANNELS"):
                parts = line.split()
                if len(parts) < 2:
                    raise ValueError(f"line {number}: CHANNELS needs a count")
                count = int(parts[1])
                channels = parts[2:]
                if len(channels) != count:
                    raise ValueError(f"line {number}: expected {count} channels, got {len(channels)}")
                if not blocks or blocks[-1] != "joint":
                    raise ValueError(f"line {number}: CHANNELS outside a joint")
                joints[-1]["channels"] = channels
            elif line == "MOTION":
                mode = "motion"
            else:
                raise ValueError(f"line {number}: unexpected hierarchy line: {line}")
        else:
            if line.startswith("Frames:"):
                frames = int(line.split(":", 1)[1])
            elif line.startswith("Frame Time:"):
                frame_time = float(line.split(":", 1)[1])
            else:
                motion_lines.append(line)

    if not joints:
        raise ValueError("no joints found")
    if frames is None or frame_time is None:
        raise ValueError("MOTION block is missing Frames or Frame Time")
    if len(motion_lines) != frames:
        raise ValueError(f"expected {frames} frames, found {len(motion_lines)}")
    expected = sum(len(joint["channels"]) for joint in joints)
    for index, line in enumerate(motion_lines, 1):
        if len(line.split()) != expected:
            raise ValueError(f"frame {index}: expected {expected} values, found {len(line.split())}")

    return {
        "joints": joints,
        "frames": frames,
        "frame_time": frame_time,
        "fps": (1.0 / frame_time) if frame_time else None,
        "channels": expected,
    }


def selftest():
    text = """HIERARCHY
ROOT Hips
{
 OFFSET 0 0 0
 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
 JOINT Spine
 {
  OFFSET 1 0 0
  CHANNELS 3 Zrotation Xrotation Yrotation
  End Site
  {
   OFFSET 2 0 0
  }
 }
}
MOTION
Frames: 2
Frame Time: 0.5
0 0 0 0 0 0 0 0 0
1 2 3 0 0 0 0 0 0
"""
    parsed = parse_bvh(text)
    assert [joint["name"] for joint in parsed["joints"]] == ["Hips", "Spine"]
    assert parsed["joints"][1]["parent"] == "Hips"
    assert parsed["frames"] == 2
    assert parsed["channels"] == 9
    assert parsed["fps"] == 2.0
    print("parse_bvh selftest: ok")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bvh", nargs="?", help="a BVH file to inspect")
    parser.add_argument("--json", action="store_true", help="print the parsed skeleton as JSON")
    parser.add_argument("--selftest", action="store_true", help="run the built-in check")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return
    if not args.bvh:
        parser.error("a BVH path is required (or use --selftest)")
    with open(args.bvh, encoding="utf-8", errors="replace") as handle:
        parsed = parse_bvh(handle.read())
    if args.json:
        print(json.dumps(parsed, indent=2))
        return
    print(f"joints:    {len(parsed['joints'])}")
    print(f"frames:    {parsed['frames']}")
    print(f"frame time:{parsed['frame_time']} ({parsed['fps']:.2f} fps)")
    print(f"channels:  {parsed['channels']} per frame")
    print("names:     " + ", ".join(joint["name"] for joint in parsed["joints"]))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"parse_bvh: {error}", file=sys.stderr)
        sys.exit(1)

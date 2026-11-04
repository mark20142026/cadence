#!/usr/bin/env python3
"""Which sprites the real game shows that Cadence doesn't (and the reverse), for one frame.

Usage: sprite_diff.py <real.png> <ours.dump> [n]

`real.png.scene` / `.txt` come from the harness's screenshots; `ours.dump` from a Cadence run with
CADENCE_DUMP_DRAWS=<file> at the same step. Counts visible sprites by frame name inside the view
(objects at world positions; their children counted when the object is in view).
"""

import re
import sys
from collections import Counter


def real_counts(png, view=None):
    """`view`: the view's size in world units (zoomed); the .txt has no zoom, so by default 320 tall."""
    info = open(png + ".txt").read()
    cam = re.search(r"camera ([-\d.e]+) ([-\d.e]+)", info)
    cx, cy = float(cam.group(1)), float(cam.group(2))
    w, h = map(int, re.search(r"size (\d+) (\d+)", info).groups())
    vw, vh = view if view else (320.0 * w / h, 320.0)
    out = Counter()
    # ancestors: (indent, shown, object-in-view) — a node shows only if all its ancestors do
    stack = []
    for line in open(png + ".scene"):
        indent = len(line) - len(line.lstrip())
        m = re.search(r"pos=([-\d.]+),([-\d.]+).* vis=(\d) op=(\d+).* frame=(\S+)", line)
        if not m:
            continue
        while stack and stack[-1][0] >= indent:
            stack.pop()
        x, y, vis, op, frame = float(m.group(1)), float(m.group(2)), int(m.group(3)), int(m.group(4)), m.group(5)
        shown = vis == 1 and op > 0 and all(a[1] for a in stack)
        is_obj = "GameObject" in line or "PlayerObject" in line or "RingObject" in line
        if is_obj:
            in_view = cx - 30 <= x <= cx + vw + 30 and cy - 30 <= y <= cy + vh + 30
        else:
            in_view = next((a[2] for a in reversed(stack) if a[2] is not None), None)
        if shown and in_view and frame not in "-?":
            out[frame] += 1
        stack.append((indent, shown, in_view if is_obj else (stack[-1][2] if stack else None)))
    return out


def our_view(dump):
    _, cx, cy, vw, vh = open(dump).readline().split()
    return float(vw), float(vh)


def our_counts(dump):
    lines = open(dump).read().splitlines()
    _, cx, cy, vw, vh = lines[0].split()
    cx, cy, vw, vh = map(float, (cx, cy, vw, vh))
    out = Counter()
    for l in lines[1:]:
        if l.startswith("#"):
            continue
        name, x, y, a = l.split()[:4]
        if float(a) > 0 and cx - 30 <= float(x) <= cx + vw + 30 and cy - 30 <= float(y) <= cy + vh + 30:
            out[name] += 1
    return out


def main():
    real, ours = real_counts(sys.argv[1]), our_counts(sys.argv[2])
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 40
    diff = sorted(set(real) | set(ours), key=lambda k: -abs(real[k] - ours[k]))
    print(f"{'frame':45} real ours")
    for k in diff[:n]:
        if real[k] != ours[k]:
            print(f"{k:45} {real[k]:4} {ours[k]:4}")


if __name__ == "__main__":
    main()

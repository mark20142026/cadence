#!/usr/bin/env python3
"""Sprites the real game shows that Cadence doesn't (or the reverse), over every real frame.

Usage: sprite_audit.py [level,level,...]

For each level with real frames (shots/<level>/real-*.png + .scene + .txt), renders ours at
those steps in one run (CADENCE_SHOTS, CADENCE_DUMP_DRAWS) and counts visible sprites by frame
name in view on both sides (tools/sprite_diff.py). Prints, per level and over all, the frames with
the biggest shortfall: what's missing first. Report: shots/sprite_audit.txt.
"""
import glob
import os
import re
import subprocess
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from sprite_diff import our_counts, our_view, real_counts  # noqa: E402


def routes():
    out = {}
    for log in ["shots/compare_all.log", "shots/compare_all-before-dense.log"]:
        path = os.path.join(ROOT, log)
        if os.path.exists(path):
            for line in open(path):
                m = re.match(r"(\d+): (\S+) to step", line)
                if m:
                    out.setdefault(m.group(1), os.path.join(ROOT, m.group(2)))
    return out


def main():
    want = sys.argv[1].split(",") if len(sys.argv) > 1 and sys.argv[1] else None
    tmp = os.path.join(ROOT, "shots/audit")
    os.makedirs(tmp, exist_ok=True)
    total_real, total_ours = Counter(), Counter()
    lines = []
    for level, route in sorted(routes().items(), key=lambda kv: int(kv[0])):
        if want and level not in want:
            continue
        reals = {}
        for png in glob.glob(os.path.join(ROOT, "shots", level, "real-*.png")):
            if not (os.path.exists(png + ".scene") and os.path.exists(png + ".txt")):
                continue
            info = open(png + ".txt").read()
            step = int(re.search(r"step (\d+)", info).group(1))
            w, h = map(int, re.search(r"size (\d+) (\d+)", info).groups())
            reals[step] = (png, w, h)
        if not reals:
            continue
        steps = sorted(reals)
        w, h = reals[steps[0]][1], reals[steps[0]][2]
        env = dict(os.environ, CADENCE_SHOTS=",".join(map(str, steps)), CADENCE_SPEC=route, CADENCE_SIZE=f"{w}x{h}",
                   CADENCE_SCREENSHOT=os.path.join(tmp, f"{level}-%d.png"), CADENCE_DUMP_DRAWS=os.path.join(tmp, f"{level}-%d.dump"), CADENCE_BALL="1")
        subprocess.run([os.path.join(ROOT, "target/release/cadence-game"), level], env=env, capture_output=True)
        lr, lo = Counter(), Counter()
        for step in steps:
            dump = os.path.join(tmp, f"{level}-{step}.dump")
            if not os.path.exists(dump):
                continue
            lr += real_counts(reals[step][0], our_view(dump))
            lo += our_counts(dump)
        total_real += lr
        total_ours += lo
        short = sorted(((lr[k] - lo[k], k) for k in set(lr) | set(lo)), reverse=True)
        lines.append(f"level {level} ({len(steps)} frames): missing " + ", ".join(f"{k} {n}" for n, k in short[:8] if n > 0)
                     + " | extra " + ", ".join(f"{k} {-n}" for n, k in short[::-1][:5] if n < 0))
        print(lines[-1], flush=True)
    short = sorted(((total_real[k] - total_ours[k], k) for k in set(total_real) | set(total_ours)), reverse=True)
    lines.append("ALL missing: " + ", ".join(f"{k} {n}" for n, k in short[:40] if n > 0))
    lines.append("ALL extra: " + ", ".join(f"{k} {-n}" for n, k in short[::-1][:25] if n < 0))
    print("\n".join(lines[-2:]))
    open(os.path.join(ROOT, "shots/sprite_audit.txt"), "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()

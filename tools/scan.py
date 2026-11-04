#!/usr/bin/env python3
"""Render every level along a completed route and flag frames that look broken, no real game
needed: white-outs, black-outs, sudden flashes against both neighbors, and frozen frames.

Usage: scan.py [level,level,...] [every_steps]

Routes come from shots/compare_all.log. Frames go to shots/scan/<level>/<step>.png (568 × 320);
the report (worst first) to shots/scan/report.txt. Flags are suspects to look at: the game has
real flashes too.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from compare_shots import read_png  # noqa: E402


def routes():
    out = {}
    for line in open(os.path.join(ROOT, "shots/compare_all.log")):
        m = re.match(r"(\d+): (\S+) to step (\d+)", line)
        if m:
            out[m.group(1)] = (os.path.join(ROOT, m.group(2)), int(m.group(3)))
    return out


def stats(path):
    w, h, rows = read_png(path)[:3]
    n = white = black = 0
    lum = 0.0
    for y in range(0, h, 4):
        row = rows[y]
        for x in range(0, w, 4):
            r, g, b = row[x * 4], row[x * 4 + 1], row[x * 4 + 2]
            n += 1
            lum += 0.299 * r + 0.587 * g + 0.114 * b
            if r > 235 and g > 235 and b > 235:
                white += 1
            if r < 12 and g < 12 and b < 12:
                black += 1
    return white / n, black / n, lum / n, bytes(rows[h // 2][::64])


def main():
    want = sys.argv[1].split(",") if len(sys.argv) > 1 and sys.argv[1] else None
    every = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    report = []
    for level, (route, end) in sorted(routes().items(), key=lambda kv: int(kv[0])):
        if want and level not in want:
            continue
        out = os.path.join(ROOT, "shots/scan", level)
        os.makedirs(out, exist_ok=True)
        steps = list(range(every, end - 60, every))
        env = dict(os.environ, CADENCE_SHOTS=",".join(map(str, steps)), CADENCE_SPEC=route,
                   CADENCE_SCREENSHOT=os.path.join(out, "%d.png"), CADENCE_SIZE="568x320", CADENCE_BALL="1")
        run = subprocess.run([os.path.join(ROOT, "target/release/cadence-game"), level], env=env, capture_output=True, text=True)
        died = re.search(r"died at step (\d+)", run.stdout + run.stderr)
        if died:
            report.append((1000.0, level, int(died.group(1)), "the run DIED here (the real one completes)"))
        frames = [(s, stats(os.path.join(out, f"{s}.png"))) for s in steps if os.path.exists(os.path.join(out, f"{s}.png"))]
        for i, (s, (white, black, lum, sig)) in enumerate(frames):
            if white > 0.35:
                report.append((white * 100, level, s, f"white-out: {white:.0%} of the frame is white"))
            if black > 0.97:
                report.append((black * 50, level, s, f"black-out: {black:.0%} black"))
            if 0 < i < len(frames) - 1:
                before, after = frames[i - 1][1][2], frames[i + 1][1][2]
                if lum - max(before, after) > 60:
                    report.append((lum - max(before, after), level, s, f"flash: brightness {lum:.0f} vs {before:.0f} / {after:.0f} around"))
            if i > 0 and sig == frames[i - 1][1][3] and frames[i - 1][1][2] == lum:
                report.append((5.0, level, s, "frozen: same as the frame before"))
        print(f"{level}: {len(frames)} frames to step {end}" + (f", died at {died.group(1)}" if died else ""), flush=True)
    report.sort(key=lambda r: -r[0])
    with open(os.path.join(ROOT, "shots/scan/report.txt"), "w") as f:
        for score, level, step, what in report:
            f.write(f"level {level} step {step}: {what}\n")
    print(f"{len(report)} suspect frames; worst:")
    for score, level, step, what in report[:25]:
        print(f"  level {level} step {step}: {what}")


if __name__ == "__main__":
    main()

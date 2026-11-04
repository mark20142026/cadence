#!/usr/bin/env python3
"""Redo the real-vs-Cadence comparisons from the real screenshots already taken, without the
real game: crop each real frame's blank top rows (the notch band of a full-screen window; older
captures kept it), render ours at the real frame's size, and compare.

Usage: recompare.py [level,level,...]

Routes come from shots/compare_all.log ("<level>: <route.spec> to step N: done"). Writes
shots/<level>/<step>.png as compare_level.py does, and shots/summary.csv.
"""

import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from compare_shots import read_png, write_png  # noqa: E402


def crop_blank_top(path, full_h):
    """Crop the real frame's blank top rows in place (if not done yet). Returns (w, rows cut).

    The game lays the scene out over the whole viewport (`full_h` rows, the .txt size); the
    notch only hides the top rows. So ours is rendered at the full size and cropped the same."""
    w, h, rows = read_png(path)[:3]
    if h < full_h:
        return w, full_h - h
    top = 0
    while top < h and not any(rows[top]):
        top += 1
    if top:
        write_png(path, w, h - top, rows[top:])
    return w, top


def crop_rows(path, top):
    w, h, rows = read_png(path)[:3]
    write_png(path, w, h - top, rows[top:])


def routes():
    out = {}
    log = os.path.join(ROOT, "shots/compare_all.log")
    for line in open(log):
        m = re.match(r"(\d+): (\S+) to step", line)
        if m:
            out[m.group(1)] = os.path.join(ROOT, m.group(2))
    return out


def main():
    want = sys.argv[1].split(",") if len(sys.argv) > 1 and sys.argv[1] else None
    rows = []
    for level, route in sorted(routes().items(), key=lambda kv: int(kv[0])):
        if want and level not in want:
            continue
        frames = []
        for real in sorted(glob.glob(os.path.join(ROOT, "shots", level, "real-*.png"))):
            info = open(real + ".txt").read()
            step = int(re.search(r"step (\d+)", info).group(1))
            full_w, full_h = map(int, re.search(r"size (\d+) (\d+)", info).groups())
            w, top = crop_blank_top(real, full_h)
            frames.append((step, real, w, full_h, top))
        # ours, per frame size, in one run each (CADENCE_SHOTS)
        for (w, h) in sorted({(f[2], f[3]) for f in frames}):
            steps = sorted({f[0] for f in frames if (f[2], f[3]) == (w, h)})
            env = dict(os.environ, CADENCE_SIZE=f"{w}x{h}", CADENCE_SHOTS=",".join(map(str, steps)),
                       CADENCE_SCREENSHOT=os.path.join(ROOT, "shots", level, "ours-%d.png"), CADENCE_SPEC=route, CADENCE_BALL="1")
            subprocess.run([os.path.join(ROOT, "target/release/cadence-game"), level], env=env, capture_output=True)
        for step, real, w, h, top in sorted(frames):
            ours = os.path.join(ROOT, "shots", level, f"ours-{step}.png")
            if not os.path.exists(ours):
                continue
            if top:
                crop_rows(ours, top)
            cmp = subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tools/compare_shots.py"), real, ours,
                                  os.path.join(ROOT, "shots", level, f"{step}.png"), "32"], capture_output=True, text=True)
            m = re.search(r"mean abs diff \(r,g,b\): ([\d.]+), ([\d.]+), ([\d.]+)", cmp.stdout)
            if m:
                rows.append((int(level), step, *map(float, m.groups())))
                print(f"{level} {step}: {m.group(1)} {m.group(2)} {m.group(3)}", flush=True)
    with open(os.path.join(ROOT, "shots/summary.csv"), "w") as f:
        f.write("level,step,r,g,b\n")
        for r in rows:
            f.write(",".join(map(str, r)) + "\n")
    print("worst:")
    for r in sorted(rows, key=lambda r: -(r[2] + r[3] + r[4]))[:20]:
        print(f"level {r[0]} step {r[1]}: {r[2]:.2f} {r[3]:.2f} {r[4]:.2f}")


if __name__ == "__main__":
    main()

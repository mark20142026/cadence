#!/usr/bin/env python3
"""Real above ours, cropped and halved, plus the mean difference: a cheap look at a GUI match.
gui_pair.py real.png ours.png out.png [x0 y0 x1 y1 in 1280x720 pixels]"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from compare_shots import read_png, write_png
real, ours, out = sys.argv[1:4]
x0, y0, x1, y1 = map(int, sys.argv[4:8]) if len(sys.argv) > 7 else (0, 0, 1280, 720)
wa, ha, ra = read_png(real)[:3]
wb, hb, rb = read_png(ours)[:3]
rows, diff, n = [], 0, 0
for src, w in ((ra, wa), (rb, wb)):
    for y in range(y0, y1, 2):
        row = bytearray()
        for x in range(x0, x1, 2):
            row += bytes(src[y][4 * x:4 * x + 3]) + b"\xff"
        rows.append(row)
for y in range(y0, y1, 2):
    for x in range(x0, x1, 2):
        diff += sum(abs(ra[y][4 * x + k] - rb[y][4 * x + k]) for k in range(3)); n += 3
write_png(out, (x1 - x0 + 1) // 2, len(rows), rows)
print(f"mean abs diff {diff / n:.1f}")

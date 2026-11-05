#!/usr/bin/env python3
"""Cadence's own app icon (no game art): a dark rounded square, a green "C" ring and a cube
in its opening. make_icon.py out.png [size]"""
import math, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from compare_shots import write_png

out = sys.argv[1]
n = int(sys.argv[2]) if len(sys.argv) > 2 else 1024
rows = []
r_out, r_in = 0.36 * n, 0.24 * n
c = n / 2
for y in range(n):
    row = bytearray()
    for x in range(n):
        # the rounded square (corner radius 22%)
        m, rad = 0.06 * n, 0.2 * n
        qx = max(m + rad - x, x - (n - m - rad), 0)
        qy = max(m + rad - y, y - (n - m - rad), 0)
        if qx * qx + qy * qy > rad * rad or x < m or y < m or x > n - m or y > n - m:
            row += b"\0\0\0\0"
            continue
        t = y / n
        col = [int(14 + 20 * t), int(22 + 30 * t), int(48 + 60 * t), 255]
        dx, dy = x - c, y - c
        d = math.hypot(dx, dy)
        ang = math.degrees(math.atan2(-dy, dx))
        # the ring, open on the right (the C)
        if r_in <= d <= r_out and not (-38 < ang < 38):
            g = 1 - (d - r_in) / (r_out - r_in) * 0.35
            col = [int(70 * g), int(230 * g), int(80 * g), 255]
        # the cube in the opening
        s = 0.09 * n
        cx2 = c + 0.27 * n
        if abs(x - cx2) <= s and abs(y - c) <= s:
            inner = abs(x - cx2) <= s * 0.62 and abs(y - c) <= s * 0.62
            col = [40, 210, 255, 255] if inner else [255, 255, 255, 255]
        row += bytes(col)
    rows.append(row)
write_png(out, n, n, rows)

#!/usr/bin/env python3
"""Compact view of a harness .scene dump: scene_brief.py <file.scene> [ClassSubstring] [maxdepth]
Prints the subtree under the first node whose class contains ClassSubstring, with world
positions, skipping font glyphs and 9-slice internals. One line per node."""
import re, sys
path = sys.argv[1]; want = sys.argv[2] if len(sys.argv) > 2 else None
maxd = int(sys.argv[3]) if len(sys.argv) > 3 else 99
pat = re.compile(r"(\s*)(\S+) tag=(\S+) z=(\S+) pos=([-\d.]+),([-\d.]+) anchor=([-\d.]+),([-\d.]+) size=([-\d.]+),([-\d.]+) scale=([-\d.]+),([-\d.]+) rot=(\S+) vis=(\d) op=(\S+) col=(\S+) blend=(\S+) frame=(\S+)")
nodes = []
for line in open(path):
    m = pat.match(line)
    if not m: continue
    d = len(m.group(1)) // 2
    cls = re.sub(r"^N?\d*(cocos2d\d*)?(extension\d*)?\d*", "", m.group(2)).rstrip("E")
    cls = re.sub(r"^\d+", "", cls)
    nodes.append((d, cls, m))
stack = []  # (depth, world origin x, y, scale x, y)
root = None; skip = None
for d, cls, m in nodes:
    while stack and stack[-1][0] >= d: stack.pop()
    px, py, psx, psy = (stack[-1][1:] if stack else (0, 0, 1, 1))
    x, y = float(m.group(5)), float(m.group(6))
    ax, ay, w, h = float(m.group(7)), float(m.group(8)), float(m.group(9)), float(m.group(10))
    sx, sy = float(m.group(11)), float(m.group(12))
    wx, wy = px + x * psx, py + y * psy
    # children are placed relative to the node's bottom-left corner (layers ignore anchors)
    lay = "Layer" in cls or "Scene" in cls
    ox, oy = (wx, wy) if lay else (wx - ax * w * sx * psx, wy - ay * h * sy * psy)
    stack.append((d, ox, oy, sx * psx, sy * psy))
    if root is None:
        if want and want not in cls: continue
        root = d
    elif d <= root:
        break
    if skip is not None and d > skip: continue
    skip = d if ("Label" in cls or "Scale9" in cls or "FontSprite" in cls or m.group(14) == "0") else None
    if d - root > maxd: continue
    if m.group(14) == "0": continue
    vis = ""
    fr = m.group(18)
    print(f"{'  '*(d-root)}{cls} t{m.group(3)} z{m.group(4)} @({wx:.1f},{wy:.1f}) a({ax:g},{ay:g}) {w:g}x{h:g} s{sx*psx:.3g},{sy*psy:.3g} op{m.group(15)} col{m.group(16)}{(' '+fr) if fr not in '-?' else ''}{vis}")

#!/usr/bin/env python3
"""Turn a harness .scene dump into a layout Cadence can draw (`ui/replay.rs`):
scene_to_layout.py in.scene out.txt [RootClass]

One line per drawable, in draw order, in screen points (569 x 320, y up):
  S frame x y sx sy rot ax ay r g b a blend      a sprite (from a sheet, or tex:file)
  N tex x y w h r g b a                           a 9-slice (its pieces are skipped)
  T font x y sx sy ax ay r g b a text             a label (its glyphs are skipped)
  R x y w h r g b a                               a CCLayerColor
  B index x y w h                                 a menu item's hit area (depth-first order)
Invisible subtrees and the first-launch notice are left out."""
import math, re, sys

pat = re.compile(r"(\s*)(\S+) tag=(\S+) z=(\S+) pos=([-\d.]+),([-\d.]+) anchor=([-\d.]+),([-\d.]+) size=([-\d.]+),([-\d.]+) scale=([-\d.]+),([-\d.]+) rot=(\S+) vis=(\d) op=(\S+) col=(\S+) blend=(\S+) frame=(\S+)")

def mul(a, b):
    return [a[0]*b[0] + a[2]*b[1], a[1]*b[0] + a[3]*b[1], a[0]*b[2] + a[2]*b[3], a[1]*b[2] + a[3]*b[3],
            a[0]*b[4] + a[2]*b[5] + a[4], a[1]*b[4] + a[3]*b[5] + a[5]]

def apply(m, x, y):
    return (m[0]*x + m[2]*y + m[4], m[1]*x + m[3]*y + m[5])

def main():
    src, out = sys.argv[1], sys.argv[2]
    root_cls = sys.argv[3] if len(sys.argv) > 3 else None
    lines = open(src).read().split("\n")
    nodes = []  # (depth, cls, fields, label)
    for i, line in enumerate(lines):
        m = pat.match(line)
        if not m:
            continue
        label = None
        flip = (False, False)
        ignore = None
        for extra in lines[i + 1:i + 4]:
            if pat.match(extra):
                break  # the next node: its flags aren't this one's
            if extra.strip() == "#ignoreanchor":
                ignore = True
            if extra.strip().startswith("#label"):
                lm = re.match(r"\s*#label font=(\S*) text=(.*)", extra)
                if lm:
                    label = (lm.group(1), lm.group(2))
            elif extra.strip().startswith("#flip"):
                fx, fy = extra.split()[1:3]
                flip = (fx == "1", fy == "1")
        if flip != (False, False):
            label = label or ("", None)
            label = (label[0], label[1], flip)
        if ignore is None and any(l.strip().startswith("#") for l in lines[1:50]):
            ignore = False  # a dump that records the flag
        nodes.append((len(m.group(1)) // 2, re.sub(r"^N?\d*(cocos2d\d*)?(extension\d*)?\d*", "", m.group(2)).rstrip("E"), m, label, ignore))
    res = []
    stack = []  # (depth, matrix, visible, class)
    started = root_cls is None
    root_depth = None
    item_index = 0
    skip_below = None
    opacity_stack = []
    for depth, cls, m, label, ignore in nodes:
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if root_cls and not started:
            if cls == root_cls:
                started, root_depth = True, depth
            else:
                continue
        if root_depth is not None and depth <= root_depth and cls != root_cls:
            break
        if skip_below is not None:
            if depth > skip_below:
                continue
            skip_below = None
        parent = stack[-1][1] if stack else [1, 0, 0, 1, 0, 0]
        parent_cls = stack[-1][3] if stack else ""
        if "SpriteBatchNode" in cls and "Scale9Sprite" in parent_cls:
            # a 9-slice's pieces (drawn as one N line); its other children (labels) stay
            skip_below = depth
            continue
        x, y = float(m.group(5)), float(m.group(6))
        ax, ay = float(m.group(7)), float(m.group(8))
        w, h = float(m.group(9)), float(m.group(10))
        sx, sy = float(m.group(11)), float(m.group(12))
        rot = float(m.group(13))
        vis = m.group(14) == "1"
        if not vis or "TOSPopup" in cls or cls == "FLAlertLayer" or "TOSPopup" in m.group(2):
            skip_below = depth
            continue
        if ignore is not None:
            layer = ignore
        else:
            layer = any(k in cls for k in ("Layer", "Scene", "Menu")) and "Sprite" not in cls and "Label" not in cls and "LayerColor" not in cls
        c, s_ = math.cos(math.radians(-rot)), math.sin(math.radians(-rot))
        local = [c * sx, s_ * sx, -s_ * sy, c * sy, x, y]
        if not layer:
            # move the anchor point to the position
            local = mul(local, [1, 0, 0, 1, -ax * w, -ay * h])
        world = mul(parent, local)
        stack.append((depth, world, vis, cls))
        op = int(m.group(15))
        col = m.group(16).split(",")
        r, g, b = (int(v) if int(v) >= 0 else 255 for v in col) if len(col) == 3 else (255, 255, 255)
        a = op if op >= 0 else 255
        frame = m.group(18)
        blend = m.group(17)
        wsx = math.hypot(world[0], world[1])
        wsy = math.hypot(world[2], world[3])
        if world[0] * world[3] - world[1] * world[2] < 0:
            # a mirror: written as flip x, so the rotation comes from the un-flipped x axis
            wsx = -wsx
            wrot = -math.degrees(math.atan2(-world[1], -world[0]))
        else:
            wrot = -math.degrees(math.atan2(world[1], world[0]))
        if "MenuItem" in cls and "Toggler" not in cls:
            cx, cy = apply(world, w * 0.5, h * 0.5)
            res.append(f"B {item_index} {cx:.2f} {cy:.2f} {w * abs(wsx):.2f} {h * abs(wsy):.2f}")
            item_index += 1
        if "Scale9Sprite" in cls:
            # the piece sprites under it name its texture
            tex = None
            for d2, c2, m2, _, _ in nodes[nodes.index((depth, cls, m, label, ignore)) + 1:]:
                if d2 <= depth:
                    break
                if m2.group(18).startswith("tex:"):
                    tex = m2.group(18)[4:]
                    break
            if tex:
                cx, cy = apply(world, w * 0.5, h * 0.5)
                tex = re.sub(r"-u?hd(\.png)$", r"\1", tex)
                res.append(f"N {tex} {cx:.2f} {cy:.2f} {w * abs(wsx):.2f} {h * abs(wsy):.2f} {r} {g} {b} {a}")
            continue
        if "LabelBMFont" in cls and label and label[1] is not None:
            px, py = apply(world, ax * w, ay * h)
            font = label[0].replace("-hd", "").replace("-uhd", "").replace(".fnt", "")
            res.append(f"T {font} {px:.2f} {py:.2f} {wsx:.4f} {wsy:.4f} {ax} {ay} {r} {g} {b} {a} {label[1]}")
            skip_below = depth
            continue
        if "LayerColor" in cls and w > 0 and h > 0:
            px, py = apply(world, 0, 0)
            res.append(f"R {px:.2f} {py:.2f} {w * abs(wsx):.2f} {h * abs(wsy):.2f} {r} {g} {b} {a}")
            continue
        if frame not in ("-", "?") and ("Sprite" in cls or "GameObject" in cls) and w > 0:
            if frame.startswith("tex:"):
                frame = "tex:" + re.sub(r"-u?hd(\.png)$", r"\1", frame[4:])
                if "bigFont" in frame or "goldFont" in frame or "chatFont" in frame or "cc_2x2" in frame:
                    continue
            px, py = apply(world, ax * w, ay * h)
            if label and len(label) == 3:
                if label[2][0]:
                    wsx = -wsx
                if label[2][1]:
                    wsy = -wsy
            res.append(f"S {frame} {px:.2f} {py:.2f} {wsx:.4f} {wsy:.4f} {wrot:.2f} {ax} {ay} {r} {g} {b} {a} {blend}")
    open(out, "w").write("\n".join(res) + "\n")
    print(f"{out}: {len(res)} items")

main()

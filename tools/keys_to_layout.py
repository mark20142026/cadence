#!/usr/bin/env python3
"""Add a trigger popup's property keys to its layout: keys_to_layout.py in.keys layout.txt

The harness writes <shot>.keys for a SetupTriggerPopup (`dumpTriggerKeys`): which property key
each input, toggle and slider edits, in screen points. This appends, for `ui/replay.rs`:
  I key x y w h        a text input (its placeholder/value label inside is removed)
  K key x y w h        a toggle (CCMenuItemToggler)
  L key x y min max    a slider (its thumb's track is 0..1 between min and max)
  V key value          the value the popup showed for a fresh trigger
Hidden nodes (other pages) are left out."""
import sys


def main():
    keys_path, layout_path = sys.argv[1], sys.argv[2]
    inputs, toggles, sliders, values, mins, maxs = [], [], [], {}, {}, {}
    for line in open(keys_path):
        w = line.split()
        if not w:
            continue
        if w[0] in ("input", "toggle", "control") and len(w) >= 7:
            key, x, y, cw, ch, vis = int(w[1]), float(w[2]), float(w[3]), float(w[4]), float(w[5]), w[6] == "1"
            if not vis:
                continue
            if w[0] == "input":
                # CCTextInputNode ignores its anchor: convertToWorldSpaceAR is its center plus
                # half its size
                inputs.append((key, x - cw / 2, y - ch / 2, cw, ch))
            elif w[0] == "toggle":
                toggles.append((key, x, y, cw, ch))
            elif "Slider" in w[-1]:
                sliders.append((key, x, y))
        elif w[0] in ("min", "max", "value") and len(w) == 3:
            {"min": mins, "max": maxs, "value": values}[w[0]][int(w[1])] = w[2]
    out = []
    for line in open(layout_path).read().split("\n"):
        f = line.split()
        if f and f[0] == "T":
            x, y = float(f[2]), float(f[3])
            if any(abs(x - ix) <= iw / 2 and abs(y - iy) <= ih / 2 - 4 for _, ix, iy, iw, ih in inputs):
                continue
        if line:
            out.append(line)
    for key, x, y, cw, ch in inputs:
        out.append(f"I {key} {x:.2f} {y:.2f} {cw:.2f} {ch:.2f}")
    for key, x, y, cw, ch in toggles:
        out.append(f"K {key} {x:.2f} {y:.2f} {cw:.2f} {ch:.2f}")
    for key, x, y in sliders:
        out.append(f"L {key} {x:.2f} {y:.2f} {mins.get(key, '0')} {maxs.get(key, '1')}")
    for key, v in sorted(values.items()):
        out.append(f"V {key} {v}")
    open(layout_path, "w").write("\n".join(out) + "\n")
    print(f"{layout_path}: {len(inputs)} inputs, {len(toggles)} toggles, {len(sliders)} sliders, {len(values)} values")


main()

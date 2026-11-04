#!/usr/bin/env python3
"""Turn the harness's trigger popup captures (shots/gui-real/editpop/t<id>.png + .scene + .keys)
into Cadence layouts (crates/cadence-game/src/ui/scenes/t<id>.txt) and list them in
ui/replay.rs (TRIGGER_POPUPS and trigger_source). The popup is the scene's last top-level node."""
import glob, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHOTS = os.path.join(ROOT, "shots/gui-real/editpop")
SCENES = os.path.join(ROOT, "crates/cadence-game/src/ui/scenes")
REPLAY = os.path.join(ROOT, "crates/cadence-game/src/ui/replay.rs")

done = {"t": [], "s": []}
for scene in sorted(glob.glob(os.path.join(SHOTS, "[ts][0-9]*.png.scene"))):
    kind, tid = re.search(r"([ts])(\d+)\.png", scene).groups()
    tid = int(tid)
    keys = scene[: -len(".scene")] + ".keys"
    top = [re.sub(r"^\d+", "", l.split()[0]) for l in open(scene) if re.match(r"^  [^\s#]", l)]
    root = top[-1] if top else None
    if not root or root in ("LevelEditorLayer", "EditLevelLayer", "CustomizeObjectLayer"):
        print(f"{kind}{tid}: no popup on top ({root})")
        continue
    out = os.path.join(SCENES, f"{kind}{tid}.txt")
    subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tools/scene_to_layout.py"), scene, out, root], check=True, capture_output=True)
    if os.path.exists(keys):
        r = subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tools/keys_to_layout.py"), keys, out], capture_output=True, text=True)
        print(f"{kind}{tid} {root}: {r.stdout.strip()}")
    else:
        print(f"{kind}{tid} {root}: no keys")
    # hand-written fields for popups without the dictionaries (tools/popup_extras/<name>.txt):
    # "drop x y" removes the label at that point, other lines are added
    extra = os.path.join(ROOT, "tools/popup_extras", f"{kind}{tid}.txt")
    if os.path.exists(extra):
        drops, adds, maps, dels = set(), [], {}, set()
        for l in open(extra):
            w = l.split()
            if not w or w[0].startswith("#"):
                continue
            if w[0] == "drop":
                drops.add((w[1], w[2]))
            elif w[0] == "map":
                # an input keyed by the popup's own tag: the property it really edits
                maps[w[1]] = w[2]
            elif w[0] == "del":
                dels.add(w[1])
            else:
                adds.append(l.strip())
        lines = []
        for l in open(out).read().split("\n"):
            if not l or (l.startswith("T ") and tuple(l.split()[2:4]) in drops):
                continue
            w = l.split()
            if w[0] in ("I", "K", "L", "V") and w[1] in dels:
                continue
            if w[0] in ("I", "K", "L", "V") and w[1] in maps:
                w[1] = maps[w[1]]
                l = " ".join(w)
            lines.append(l)
        open(out, "w").write("\n".join(lines + adds) + "\n")
        print(f"  + {len(adds)} hand-written lines")
    done[kind].append(tid)

src = open(REPLAY).read()
for kind, const, func in (("t", "TRIGGER_POPUPS", "trigger_source"), ("s", "SPECIAL_POPUPS", "special_source")):
    ids = done[kind]
    src = re.sub(rf"pub const {const}: \[u16; \d+\] = \[[^\]]*\];", f"pub const {const}: [u16; {len(ids)}] = [{', '.join(map(str, ids))}];", src)
    arms = "".join(f'        {t} => include_str!("scenes/{kind}{t}.txt"),\n' for t in ids)
    src = re.sub(rf"(fn {func}\(id: u16\) -> &'static str \{{\n    match id \{{\n)(?:        \d+ => include_str!\(\"scenes/{kind}\d+\.txt\"\),\n)*",
                 lambda m: m.group(1) + arms, src)
open(REPLAY, "w").write(src)
print(f"{len(done['t'])} trigger and {len(done['s'])} special popups listed")

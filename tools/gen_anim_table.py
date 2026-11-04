#!/usr/bin/env python3
"""Generate crates/cadence-game/src/animobj_table.rs: every animated level object.

Sources (GD 2.2091 macOS arm64):
- Frame animations: `GameManager`'s animation table, filled by FUN_1001f2b60 (called from
  GameManager::init) with 136 `addGameAnimation(id, frames, frameTime, name, colorName,
  defaultFrame)` calls (0x1f93ec: dicts +0x178 main frames "%s_%03d.png", +0x180 color frames,
  +0x188 frame time, +0x190 frame count) and one `addCustomAnimationFrame` (0x1f965c). The
  strings are std::strings built inline, so this script runs a tiny straight-line arm64
  emulator over llvm-objdump text of that function and records the call arguments.
- Per-id behavior: `EnhancedGameObject::updateSyncedAnimation` 0x1efde4 (which sprite gets
  which frame), `GameObject::usesFreezeAnimation` 0x1f152c (play, hide 0.2–0.7 s, replay),
  `EnhancedGameObject::customSetup` 0x5bb8dc (random speed 0.8–1.2 and start offsets); the id
  sets below are decoded from their bit masks and jump tables.
- Part animations (AnimatedGameObject, `AnimatedGameObject::init` 0x156c04): objectDefinitions.plist
  (delay, frame count, looped per animation).

Usage: gen_anim_table.py <gd.dis> <Resources dir> > crates/cadence-game/src/animobj_table.rs
       (gd.dis: `llvm-objdump -d --no-show-raw-insn re/gd-2.2091-arm64 > gd.dis`)
"""
import re
import struct
import sys

BIN = "re/gd-2.2091-arm64"
START, END = 0x1001F2B60, 0x1001F7070
ADD_ANIM, ADD_CUSTOM, OP_NEW = 0x1001F93EC, 0x1001F965C, 0x100882674


def emulate(dis_path):
    """Runs FUN_1001f2b60 straight-line; returns the addGameAnimation / addCustomAnimationFrame calls."""
    b = open(BIN, "rb").read()
    ncmds = struct.unpack_from("<I", b, 16)[0]
    off, segs = 32, []
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", b, off)
        if cmd == 0x19:
            vm, _vs, fo, fs = struct.unpack_from("<QQQQ", b, off + 24)
            segs.append((vm, fo, fs))
        off += size
    mem, regs, heap = {}, {"sp": 0x7000_0000}, [0x5000_0000]

    def rbyte(a):
        if a in mem:
            return mem[a]
        for vm, fo, fs in segs:
            if vm <= a < vm + fs:
                return b[fo + a - vm]
        return 0

    def rd(a, n):
        return int.from_bytes(bytes(rbyte(a + i) for i in range(n)), "little")

    def wr(a, v, n):
        for i in range(n):
            mem[a + i] = (v >> (8 * i)) & 0xFF

    def rn(r):
        if r in ("sp", "wsp"):
            return "sp", 8
        if r in ("xzr", "wzr"):
            return "zr", 8
        k, n = r[0], r[1:]
        return {"x": ("x" + n, 8), "w": ("x" + n, 4), "q": ("v" + n, 16), "d": ("v" + n, 8), "s": ("v" + n, 4)}[k]

    def get(r):
        name, w = rn(r)
        return 0 if name == "zr" else regs.get(name, 0) & ((1 << (8 * w)) - 1)

    def put(r, v):
        name, w = rn(r)
        if name != "zr":
            regs[name] = v & ((1 << (8 * w)) - 1)

    def imm(s):
        return int(s.strip().lstrip("#"), 0)

    def addr(m):
        parts = [p.strip() for p in m.strip().strip("[]!").split(",")]
        return (get(parts[0]) + (imm(parts[1]) if len(parts) > 1 else 0)) & ((1 << 64) - 1)

    def cstr(a, n):
        return bytes(rbyte(a + i) for i in range(n)).decode("latin1")

    def stdstr(a):
        flag = rbyte(a + 23)
        if flag & 0x80:
            return cstr(rd(a, 8), rd(a + 8, 8))
        return cstr(a, flag)

    line_re = re.compile(r"^([0-9a-f]+):\s+(\S+)\s*(.*)$")
    calls = []
    for line in open(dis_path):
        m = line_re.match(line)
        if not m:
            continue
        pc = int(m.group(1), 16)
        if pc < START or pc >= END:
            continue
        op = m.group(2)
        args = m.group(3).split(";")[0].split("<")[0].strip()
        a = [x.strip() for x in re.split(r",(?![^\[]*\])", args)] if args else []
        if op == "mov":
            put(a[0], imm(a[1]) if a[1].startswith("#") else get(a[1]))
        elif op == "movk":
            sh = int(a[2].split()[-1].lstrip("#"), 0) if len(a) > 2 else 0
            put(a[0], (get(a[0]) & ~(0xFFFF << sh)) | (imm(a[1]) << sh))
        elif op == "adrp":
            put(a[0], int(a[1], 16))
        elif op in ("add", "sub"):
            v2 = imm(a[2]) if a[2].startswith("#") else get(a[2])
            if len(a) > 3 and "lsl" in a[3]:
                v2 <<= int(a[3].split("#")[1], 0)
            put(a[0], get(a[1]) + v2 if op == "add" else get(a[1]) - v2)
        elif op in ("ldr", "ldur") and a[1].startswith("["):
            put(a[0], rd(addr(a[1]), rn(a[0])[1]))
        elif op in ("str", "stur", "strb", "strh"):
            w = {"strb": 1, "strh": 2}.get(op, rn(a[0])[1])
            wr(addr(a[1]), get(a[0]), w)
        elif op == "stp" and a[2].startswith("["):
            w = rn(a[0])[1]
            wr(addr(a[2]), get(a[0]), w)
            wr(addr(a[2]) + w, get(a[1]), w)
        elif op == "fmov":
            if a[1].startswith("#"):
                put(a[0], struct.unpack("<I", struct.pack("<f", float(a[1][1:])))[0])
            else:
                put(a[0], get(a[1]))
        elif op == "bl":
            t = int(a[0], 16)
            if t == OP_NEW:
                heap[0] += 0x100
                regs["x0"] = heap[0]
            elif t == ADD_ANIM:
                delay = struct.unpack("<f", struct.pack("<I", get("s0")))[0]
                calls.append(("anim", get("w1"), get("w2"), delay, stdstr(get("x3")), stdstr(get("x4"))))
            elif t == ADD_CUSTOM:
                calls.append(("custom", get("w1"), get("w2"), stdstr(get("x3")), stdstr(get("x4"))))
    return calls


def f32(x):
    """Shortest decimal that rounds to the same f32."""
    target = struct.unpack("<f", struct.pack("<f", x))[0]
    for digits in range(1, 10):
        t = float("%.*g" % (digits, x))
        if struct.unpack("<f", struct.pack("<f", t))[0] == target:
            return repr(t)
    return repr(x)


def bits(base, mask, n):
    return {base + i for i in range(n) if (mask >> i) & 1}


# updateSyncedAnimation 0x1efde4, label LAB_1001f047c: the main sprite takes the main frames and
# the color sprite's first child (scale 2: the color art is half resolution) the color frames.
COLOR_CHILD = bits(0x7E4, 0x87BFFFFFF, 0x24) | bits(0xB30, 0x4FFFFFFB, 0x1F) | {920, 921, 923, 924} | set(range(1936, 1940))
# GameObject::usesFreezeAnimation 0x1f152c: play once, hide 0.2–0.7 s, play again.
FREEZE = {0x399, 0x5EF, 0x652} | bits(0x73B, 0x23B, 10) | bits(0x7E4, 0xFFF9FAFF7, 0x24) | bits(0xB33, 0x1DAF3F, 0x15)
# customSetup 0x5bb8dc: speed = rand·0.4 + 0.8 (+0x55c) for these ...
PIXEL = {2223, 2246, 2605, 2629, 2630, 2694, 3119, 3120, 3121, 3219, 3303, 3304, 3482, 3483, 3484, 3492, 3493, 4211, 4300}
RANDOM_SPEED = ({920, 921, 923, 924, 1518, 1519, 1583, 1591, 1593, 1618, 1697, 1698, 1699} | set(range(1936, 1940))
                | set(range(2020, 2056)) | set(range(1849, 1861)) | (set(range(2864, 2895)) - {2866}) | (PIXEL - {2605, 2694}))
# ... and with "randomize start" (key 106) their start offset: 70% rand·0.5 s, else 0 (HALF70);
# rand·0.5 (HALF); others take saveActiveColors' rand() in [0, 1) (UNIT).
HALF70 = {920, 921, 923, 924, 1518, 1519, 1583} | set(range(1936, 1940)) | set(range(2020, 2056)) | set(range(1849, 1861)) | (set(range(2864, 2895)) - {2866})
HALF = {1591, 1593, 1618, 1697, 1698, 1699}


def kind(i):
    if i in (1050, 1051, 1052):
        return "Kind::Wave { color: false, first: %s }" % ("true" if i == 1050 else "false")
    if i in (3000, 3001, 3002):
        return "Kind::Wave { color: true, first: %s }" % ("true" if i == 3000 else "false")
    if i in (1516, 1518):
        return "Kind::ChildFrames"
    if i == 1591:
        return "Kind::Lava { random: true }"
    if i == 1593:
        return "Kind::Lava { random: false }"
    if i in (2892, 2893):
        return "Kind::Spin { step: %s, frames: %d, quarter: %s }" % ("0.025" if i == 2892 else "0.033333335", 2 if i == 2892 else 4, "false" if i == 2892 else "true")
    if i in COLOR_CHILD:
        return "Kind::ColorChild"
    return "Kind::Main"


def part_anims(resources):
    """objectDefinitions.plist: (type, animation) -> (delay, frames, looped)."""
    s = open(resources + "/objectDefinitions.plist").read()
    out = {}
    for name in ["BlackSludge", "GJBeast01", "GJBeast02", "GJBeast03", "GJBeast04", "GJBeast05"]:
        i = s.find("<key>%s</key>" % name)
        blk = s[i:s.find("<key>usePA</key>", i)]
        for a in re.findall(r"<key>(\w+)</key>\s*<dict>\s*<key>delay</key>\s*<string>([^<]*)</string>\s*<key>frames</key>\s*<string>([^<]*)</string>\s*<key>looped</key>\s*<string>([^<]*)</string>", blk):
            out[(name, a[0])] = (float(a[1]), int(a[2]), a[3] == "1")
    return out


def main():
    calls = emulate(sys.argv[1])
    anims, custom = {}, {}
    for c in calls:
        if c[0] == "anim":
            _, i, n, delay, name, color = c
            if i >= 0x80000000:
                continue  # -142 / -1329: the secret coins' alternate art (not drawn by this module)
            anims[i] = (n, delay, name, color)
        else:
            custom[(c[1], c[2])] = (c[3], c[4])
    print("// @generated by tools/gen_anim_table.py from the GD 2.2091 binary and objectDefinitions.plist. Do not edit.")
    print("// Frame-animated objects (GameManager's animation table, FUN_1001f2b60 → addGameAnimation")
    print("// 0x1f93ec) with how updateSyncedAnimation 0x1efde4 plays them, and the part animations")
    print("// AnimatedGameObjects play (objectDefinitions.plist).")
    print()
    print("use super::{FrameAnim, Kind, PartAnimDef, Start};")
    print()
    print("pub static FRAME_ANIMS: &[FrameAnim] = &[")
    for i in sorted(anims):
        n, delay, name, color = anims[i]
        if n < 2 or i in (142, 1329):
            continue  # one-frame entries (lava placeholders) and the secret coins (app.rs spins those)
        frames = ["%s_%03d.png" % (name, k) for k in range(1, n + 1)]
        colors = ["%s_%03d.png" % (color, k) for k in range(1, n + 1)] if color else []
        for k in range(1, n + 1):
            if (i, k) in custom:
                frames[k - 1], c2 = custom[(i, k)]
                if colors and c2:
                    colors[k - 1] = c2
        start = "Start::Half70" if i in HALF70 else "Start::Half" if i in HALF else "Start::Unit"
        print(f"    FrameAnim {{ id: {i}, delay: {f32(delay)}, kind: {kind(i)}, freeze: {'true' if i in FREEZE else 'false'}, "
              f"random_speed: {'true' if i in RANDOM_SPEED else 'false'}, start: {start}, "
              f"frames: &[{', '.join(repr(f).replace(chr(39), chr(34)) for f in frames)}], "
              f"colors: &[{', '.join(repr(f).replace(chr(39), chr(34)) for f in colors)}] }},")
    print("];")
    print()
    print("/// (type, animation) → frame delay, frame count, looped.")
    print("pub static PART_ANIMS: &[PartAnimDef] = &[")
    for (t, a), (d, n, looped) in sorted(part_anims(sys.argv[2]).items()):
        print(f'    PartAnimDef {{ kind: "{t}", anim: "{a}", delay: {d!r}, frames: {n}, looped: {"true" if looped else "false"} }},')
    print("];")


if __name__ == "__main__":
    main()

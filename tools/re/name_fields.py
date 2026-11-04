#!/usr/bin/env python3
"""Rewrite raw field offsets in Ghidra output as field names.

Usage: name_fields.py <offsets.txt> <in_dir> <out_dir>

offsets.txt comes from the trace mod's `dumpoffsets` mode ("Class field 0xOFF" lines).
The class of `this` comes from the file name (Class__method@addr.c); base-class fields are
included. Only `param_N` pointers are rewritten (other locals aren't typed).
"""
import re
import sys
from pathlib import Path

BASES = {
    "PlayerObject": ["GameObject"],
    "EnhancedGameObject": ["GameObject"],
    "EffectGameObject": ["EnhancedGameObject", "GameObject"],
    "PlayLayer": ["GJBaseGameLayer"],
    "LevelEditorLayer": ["GJBaseGameLayer"],
}
# Unnamed functions known to belong to a class.
EXTRA = {"FUN_100368e30": "PlayerObject"}


def load(path):
    table = {}
    for line in open(path):
        cls, field, off = line.split()
        table.setdefault(cls, {}).setdefault(int(off, 16), field)
    return table


# Embedded structs: (class, member, member's class, member offset).
EMBEDDED = [("GJBaseGameLayer", "m_gameState", "GJGameState", 0x1a8)]


def fields_for(table, cls):
    merged = {}
    for c in [cls] + BASES.get(cls, []):
        for off, name in table.get(c, {}).items():
            merged.setdefault(off, name)
        for owner, member, inner, base in EMBEDDED:
            if owner == c:
                for off, name in table.get(inner, {}).items():
                    merged.setdefault(base + off, f"{member}.{name}")
    return merged


def rewrite(text, fields):
    def name(off, fallback):
        return fields.get(off, fallback)

    # *(type *)((long)param_2 + 0x977)
    text = re.sub(
        r"\*\((?:[\w ]+?\s*\*+)\)\(\(long\)(param_\d+) \+ (0x[0-9a-f]+)\)",
        lambda m: f"{m.group(1)}->{name(int(m.group(2), 16), m.group(0))}" if int(m.group(2), 16) in fields else m.group(0),
        text,
    )
    # *(type *)(param_2 + 0x135)  -- long* arithmetic, 8-byte stride
    text = re.sub(
        r"\*\((?:[\w ]+?\s*\*+)\)\((param_\d+) \+ (0x[0-9a-f]+)\)",
        lambda m: f"{m.group(1)}->{fields[int(m.group(2), 16) * 8]}" if int(m.group(2), 16) * 8 in fields else m.group(0),
        text,
    )
    # param_2[0xf1]  -- long* indexing
    text = re.sub(
        r"\b(param_\d+)\[(0x[0-9a-f]+|\d+)\]",
        lambda m: f"{m.group(1)}->{fields[int(m.group(2), 0) * 8]}" if int(m.group(2), 0) * 8 in fields else m.group(0),
        text,
    )
    return text


def main():
    table = load(sys.argv[1])
    out_dir = Path(sys.argv[3])
    out_dir.mkdir(parents=True, exist_ok=True)
    for path in sorted(Path(sys.argv[2]).glob("*.c")):
        stem = path.name.split("@")[0]
        cls = EXTRA.get(stem, stem.split("__")[0])
        fields = fields_for(table, cls)
        text = path.read_text()
        (out_dir / path.name).write_text(rewrite(text, fields) if fields else text)


if __name__ == "__main__":
    main()

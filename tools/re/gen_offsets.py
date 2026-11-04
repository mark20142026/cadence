#!/usr/bin/env python3
"""Generate an offsetof() table for named fields of key classes from Geode's .bro bindings.

The trace mod includes the output and, in `dumpoffsets` mode, writes "Class field offset" lines,
which tools/re/name_fields.py uses to rewrite raw offsets in Ghidra output as field names.

Usage: gen_offsets.py <bindings/2.2091/GeometryDash.bro> > harness/trace-mod/src/offsets.inc
"""
import re
import sys

CLASSES = ["GameObject", "EnhancedGameObject", "EffectGameObject", "PlayerObject",
           "GJBaseGameLayer", "PlayLayer", "GJGameState"]
FIELD_RE = re.compile(r"^\s+(?!static\b|virtual\b)[\w:<>,\s\*&\[\]]+?\s\*?(m_\w+)\s*;")


def main():
    text = open(sys.argv[1]).read().splitlines()
    current = None
    seen = set()
    for line in text:
        m = re.match(r"^class (\w+)\b", line)
        if m:
            current = m.group(1) if m.group(1) in CLASSES else None
            continue
        if line.startswith("}"):
            current = None
            continue
        if not current or "(" in line or "PAD" in line:
            continue
        f = FIELD_RE.match(line)
        if f and (current, f.group(1)) not in seen:
            seen.add((current, f.group(1)))
            print(f"OFF({current}, {f.group(1)})")


if __name__ == "__main__":
    main()

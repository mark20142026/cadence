#!/usr/bin/env python3
"""Extract `m1` (macOS arm64) function addresses from Geode .bro bindings.

Usage: bro_names.py <bindings/2.2091> > names.tsv
Writes "0xADDR<TAB>Class::method" lines, one per address, for Ghidra's ApplyNames.java.
"""
import re
import sys
from pathlib import Path

CLASS_RE = re.compile(r"^\s*class\s+([\w:]+)")
ADDR_RE = re.compile(r"\bm1\s+(0x[0-9a-fA-F]+)")
NAME_RE = re.compile(r"(~?[\w]+|operator\s*\S+?)\s*\(")


def names(path):
    cls = None
    for line in path.read_text(errors="replace").splitlines():
        m = CLASS_RE.match(line)
        if m:
            cls = m.group(1)
            continue
        if line.startswith("}"):
            cls = None
            continue
        a = ADDR_RE.search(line)
        if not a or "=" not in line:
            continue
        sig = line.split("=", 1)[0]
        n = NAME_RE.search(sig)
        if not n:
            continue
        name = re.sub(r"\s+", "", n.group(1))
        yield int(a.group(1), 16), f"{cls}::{name}" if cls else name


def main():
    root = Path(sys.argv[1])
    seen = {}
    for bro in sorted(root.glob("*.bro")):
        for addr, name in names(bro):
            seen.setdefault(addr, name)  # first binding wins for shared addresses
    for addr in sorted(seen):
        print(f"0x{addr:x}\t{seen[addr]}")


if __name__ == "__main__":
    main()

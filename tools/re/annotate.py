#!/usr/bin/env python3
"""Replace radare2 `sym.func.1XXXXXXXX` labels with Geode names. Usage: annotate.py names.tsv < asm"""
import re
import sys

names = {}
for line in open(sys.argv[1]):
    addr, name = line.rstrip("\n").split("\t")
    names[int(addr, 16) + 0x100000000] = name

def sub(m):
    addr = int(m.group(1), 16)
    return names.get(addr, m.group(0))

for line in sys.stdin:
    sys.stdout.write(re.sub(r"(?:sym\.)?func\.([0-9a-f]{9})", sub, line))

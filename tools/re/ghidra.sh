#!/bin/bash
# Headless Ghidra for the GD 2.2091 arm64 binary. Project lives in re/ghidra (gitignored).
#   ghidra.sh analyze                 import + name functions + full analysis (slow, once)
#   ghidra.sh decompile <regex> [out] decompile matching functions into re/decomp/
#   ghidra.sh referencing <text> [out] decompile the functions using a string containing <text>
# Uses the libjitfix wrapper JDK: plain JVMs crash at startup on this Mac.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
app="$HOME/Library/Application Support/Steam/steamapps/common/Geometry Dash/Geometry Dash.app"
bin="$root/re/gd-2.2091-arm64"
proj="${GHIDRA_PROJECT:-$root/re/ghidra}"   # another copy when two sessions decompile at once

export JAVA_HOME="${GHIDRA_JAVA_HOME:-$HOME/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home}"
export PATH="$JAVA_HOME/bin:$PATH"   # Ghidra tries PATH before JAVA_HOME; /usr/bin/java is a stub
headless="${GHIDRA_HEADLESS:-/opt/homebrew/opt/ghidra/libexec/support/analyzeHeadless}"
names="$here/names-2.2091.tsv"       # gitignored: rebuilt from the Geode bindings in third_party/

mkdir -p "$proj"
case "${1:-}" in
analyze)
    [ -f "$bin" ] || lipo "$app/Contents/MacOS/Geometry Dash" -thin arm64 -output "$bin"
    [ -f "$names" ] || python3 "$here/bro_names.py" "$root/third_party/geode-bindings/bindings/2.2091" > "$names"
    "$headless" "$proj" gd -import "$bin" -overwrite \
        -scriptPath "$here/ghidra_scripts" \
        -preScript ApplyNames.java "$names"
    ;;
callers)
    "$headless" "$proj" gd -process "$(basename "$bin")" -noanalysis -readOnly \
        -scriptPath "$here/ghidra_scripts" \
        -postScript FindCallers.java "$2" "$3"
    ;;
decompile)
    "$headless" "$proj" gd -process "$(basename "$bin")" -noanalysis -readOnly \
        -scriptPath "$here/ghidra_scripts" \
        -postScript DecompileMatching.java "$2" "${3:-$root/re/decomp}"
    ;;
referencing)
    "$headless" "$proj" gd -process "$(basename "$bin")" -noanalysis -readOnly \
        -scriptPath "$here/ghidra_scripts" \
        -postScript DecompileReferencing.java "$2" "${3:-$root/re/decomp}"
    ;;
*)
    echo "usage: $0 analyze | decompile <regex> [outdir] | referencing <string fragment> [outdir] | callers <regex> <out>" >&2
    exit 2
    ;;
esac

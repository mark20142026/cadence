#!/bin/bash
# Run the real game (a copy, with an isolated home) on a trace spec and wait for the trace.
#   run-trace.sh <spec> [timeout-seconds]
# Builds and installs the trace mod first. Never touches the real save folder; checks it.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
re="$root/re"
app="$re/gd-copy/Geometry Dash.app"
spec="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
timeout="${2:-120}"
real="$HOME/Library/Application Support/GeometryDash"

export PATH="$re/tools/bin:$PATH" GEODE_SDK="$re/geode-sdk"
cmake --build "$re/build/trace-mod" > "$re/build/trace-mod.log" 2>&1 || { tail -20 "$re/build/trace-mod.log"; exit 1; }
cp "$re/build/trace-mod/cadence.trace.geode" "$app/Contents/geode/mods/"

out="$(awk '$1=="out"{print $2}' "$spec")"
rm -f "${out//%d/*}"
before="$(stat -f '%m %z %N' "$real"/*)"

cd "$app/Contents/MacOS"
HOME="$re/home" CFFIXED_USER_HOME="$re/home" CADENCE_TRACE_SPEC="$spec" \
    DYLD_INSERT_LIBRARIES="$app/Contents/Frameworks/GeodeBootstrapper.dylib" \
    ./"Geometry Dash" > "$re/last-run.log" 2>&1 &
pid=$!
for _ in $(seq 1 "$timeout"); do
    sleep 1
    kill -0 "$pid" 2>/dev/null || break
done
if kill -0 "$pid" 2>/dev/null; then
    kill "$pid"; sleep 1; kill -9 "$pid" 2>/dev/null || true
    echo "timed out after ${timeout}s" >&2
fi

[ "$before" = "$(stat -f '%m %z %N' "$real"/*)" ] || { echo "WARNING: real save folder changed" >&2; exit 3; }
shopt -s nullglob
files=( ${out//%d/*} )
[ ${#files[@]} -gt 0 ] || { echo "no trace written" >&2; exit 1; }
for f in "${files[@]}"; do echo "$f: $(grep -vc '^#' "$f") lines, $(tail -1 "$f")"; done

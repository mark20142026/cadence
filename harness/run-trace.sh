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
# The game copy's home: re/home (a copy of a played save) unless CADENCE_HARNESS_HOME names another,
# e.g. re/home-anon, a fresh logged-out save for account screens (nothing there reaches a real account).
home="${CADENCE_HARNESS_HOME:-$re/home}"
mkdir -p "$home"

export PATH="$re/tools/bin:$PATH" GEODE_SDK="$re/geode-sdk"
cmake --build "$re/build/trace-mod" > "$re/build/trace-mod.log" 2>&1 || { tail -20 "$re/build/trace-mod.log"; exit 1; }
cp "$re/build/trace-mod/cadence.trace.geode" "$app/Contents/geode/mods/"

out="$(awk '$1=="out"{print $2}' "$spec")"
rm -f "${out//%d/*}"
before="$(stat -f '%m %z %N' "$real"/*)"

# Through LaunchServices in the background (-g), parked in the screen's corner and muted (the
# mod's `corner` / `mute`), so a run never takes the focus or the screen from whoever is using
# the Mac. CADENCE_HARNESS_FRONT=1 runs it in front instead.
run_spec="$spec"
if [ -z "${CADENCE_HARNESS_FRONT:-}" ]; then
    run_spec="$(mktemp -t cadence-spec)"
    cat "$spec" > "$run_spec"
    grep -qx corner "$spec" || echo corner >> "$run_spec"
    grep -qx mute "$spec" || echo mute >> "$run_spec"
fi
# CADENCE_HTTP_LOG / CADENCE_HTTP_REDIRECT (the mod's request log and local recorder) pass through.
extra=()
for v in CADENCE_HTTP_LOG CADENCE_HTTP_REDIRECT CADENCE_LB_VALUES; do
    [ -n "${!v:-}" ] && extra+=(--env "$v=${!v}")
done
started="$(pgrep -x "Geometry Dash" || true)"
open -g -n -W -a "$app" --stdout "$re/last-run.log" --stderr "$re/last-run.log" \
    --env HOME="$home" --env CFFIXED_USER_HOME="$home" --env CADENCE_TRACE_SPEC="$run_spec" \
    --env DYLD_INSERT_LIBRARIES="$app/Contents/Frameworks/GeodeBootstrapper.dylib" ${extra[@]+"${extra[@]}"} &
waiter=$!
for _ in $(seq 1 "$timeout"); do
    sleep 1
    kill -0 "$waiter" 2>/dev/null || break
done
if kill -0 "$waiter" 2>/dev/null; then
    # stop only the copy this run started, by its PID
    for pid in $(pgrep -x "Geometry Dash" || true); do
        grep -qx "$pid" <<< "$started" || kill -9 "$pid"
    done
    wait "$waiter" 2>/dev/null || true
    echo "timed out after ${timeout}s" >&2
fi

[ "$before" = "$(stat -f '%m %z %N' "$real"/*)" ] || { echo "WARNING: real save folder changed" >&2; exit 3; }
shopt -s nullglob
files=( ${out//%d/*} )
[ ${#files[@]} -gt 0 ] || { echo "no trace written" >&2; exit 1; }
for f in "${files[@]}"; do echo "$f: $(grep -vc '^#' "$f") lines, $(tail -1 "$f")"; done

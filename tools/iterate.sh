#!/bin/bash
# One verification round for a level: plan routes in the simulation, play them in the real game,
# compare. Usage: tools/iterate.sh <level> <suite-name> [routes=4] [timeout=900]
set -euo pipefail
cd "$(dirname "$0")/.."
level=$1 name=$2 n=${3:-4} timeout=${4:-900}
dir="$PWD/re/traces/$name"
mkdir -p "$dir"
./target/release/cadence route "$level" "$n" "$dir/r%d.csv" > "$dir/route.spec" 2> "$dir/route.log"
tail -"$n" "$dir/route.log"
harness/run-trace.sh "$dir/route.spec" "$timeout" | tail -"$n"
./target/release/cadence verify "$dir/route.spec" | tail -6 || true

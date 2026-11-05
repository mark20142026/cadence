#!/bin/bash
# Verify every recorded trace suite against the current build. Prints suites that differ (or
# all with -v) and a summary line.
cd "$(dirname "$0")/.."
verbose=0; [ "${1:-}" = "-v" ] && { verbose=1; shift; }
specs=$(cd re/traces && ls -d */*.spec */spec multi.spec 2>/dev/null | grep -v -E '^(shots|sprites)')
total=0; bad=0; cam_ok=0; cam_bad=""
for s in $specs "$@"; do
    [ -f "re/traces/$s" ] || continue
    ls re/traces/$(dirname $s)/*.csv >/dev/null 2>&1 || [ "$s" = multi.spec ] || continue
    out=$(./target/release/cadence verify "re/traces/$s" 2>&1)
    line=$(echo "$out" | grep -E 'attempts match|rotation|^attempt' | head -3 | tr '\n' ' ')
    total=$((total + 1))
    # the camera is tallied separately (a suite can match bit for bit apart from it)
    if echo "$out" | grep -E '^camera: ' | python3 -I -c "
import re,sys
m=re.search(r'(\d+)/(\d+) bit-exact', sys.stdin.read())
sys.exit(0 if not m or m.group(1)==m.group(2) else 1)"; then
        cam_ok=$((cam_ok + 1))
    else
        cam_bad="$cam_bad $(dirname "$s")"
    fi
    # suites with a watch.args file also compare their watched groups (cmp_watch.py options)
    watch_ok=0
    d="re/traces/$(dirname "$s")"
    if [ -f "$d/watch.args" ]; then
        ./target/release/cadence watch "re/traces/$s" > /dev/null 2>&1
        for w in "$d"/g*.csv; do
            out=$(python3 -I tools/cmp_watch.py "$w" $(cat "$d/watch.args")) || { watch_ok=1; line="$line watch: $(echo "$out" | grep -v 'first past [0-9.e-]*: None' | head -1)"; break; }
        done
    fi
    if [ $watch_ok = 0 ] && echo "$line" | python3 -I -c "
import re,sys
l=sys.stdin.read(); m=re.search(r'(\d+)/(\d+) attempts match exactly.*?(\d+)/(\d+) bit-exact', l)
sys.exit(0 if m and m.group(1)==m.group(2) and m.group(3)==m.group(4) else 1)"; then
        [ $verbose = 1 ] && printf '%-20s %s\n' "$s" "$line"
    else
        bad=$((bad + 1)); printf '%-20s %s\n' "$s" "$line"
    fi
done
echo "$((total - bad))/$total suites match bit for bit"
echo "camera: $cam_ok/$total bit for bit${cam_bad:+ (not:$cam_bad)}"

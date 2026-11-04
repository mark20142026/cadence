#!/bin/bash
# Build the web version and deploy it with the relay to Cloudflare (wrangler.toml).
#   tools/deploy-web.sh          build target/site and deploy
#   tools/deploy-web.sh --dry    build target/site only
# Only web/index.html and web/pkg go up: web/gd (the player's own game files) and web/bench are
# local links and are never copied.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
./tools/build-web.sh
site="$root/target/site"
rm -rf "${site:?}"
mkdir -p "$site/pkg"
cp web/index.html "$site/"
cp web/pkg/cadence_game.js web/pkg/cadence_game_bg.wasm "$site/pkg/"
find "$site" -type l -print -quit | grep -q . && { echo "symlink in $site: refusing" >&2; exit 1; }
ls -la "$site" "$site/pkg"
[ "${1:-}" = "--dry" ] && exit 0
npx --yes wrangler@4 deploy

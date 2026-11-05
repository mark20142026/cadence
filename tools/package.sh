#!/bin/bash
# Builds Cadence's player package for this computer into dist/:
#   macOS:   dist/Cadence.app (the launcher + the game), zipped and as a .dmg
#   Windows: dist/Cadence-windows.zip (cadence.exe = the launcher, cadence-game.exe)
#   Linux:   dist/Cadence-linux.tar.gz
# The release workflow (.github/workflows/release.yml) runs this on each platform.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(awk -F'"' '/^version/{print $2; exit}' crates/cadence-launcher/Cargo.toml)"
target="${CARGO_TARGET:-}"
targs=()
[ -n "$target" ] && targs=(--target "$target")
cargo build --release -p cadence-game -p cadence-launcher ${targs[@]+"${targs[@]}"}
bin="target/${target:+$target/}release"
mkdir -p dist
case "$(uname -s)" in
Darwin)
    app="dist/Cadence.app"
    rm -rf "${app:?}"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    # the launcher builds as cadence-launcher (cadence-tools has the `cadence` command); the
    # packages call it cadence
    cp "$bin/cadence-launcher" "$app/Contents/MacOS/cadence"
    cp "$bin/cadence-game" "$app/Contents/MacOS/"
    iconset="$(mktemp -d)/Cadence.iconset"
    mkdir -p "$iconset"
    for s in 16 32 128 256 512; do
        sips -z $s $s packaging/icon-1024.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
        sips -z $((s * 2)) $((s * 2)) packaging/icon-1024.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$app/Contents/Resources/Cadence.icns"
    sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Contents/Info.plist"
    # ad-hoc signature, so macOS runs it after "Open" (no Apple developer certificate here)
    codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
    (cd dist && rm -f Cadence-macos.zip && ditto -c -k --keepParent Cadence.app Cadence-macos.zip)
    rm -f dist/Cadence-macos.dmg
    hdiutil create -volname Cadence -srcfolder "$app" -ov -format UDZO dist/Cadence-macos.dmg >/dev/null
    echo "dist/Cadence.app, dist/Cadence-macos.zip, dist/Cadence-macos.dmg"
    ;;
MINGW*|MSYS*|CYGWIN*|Windows_NT)
    out="dist/Cadence"
    rm -rf "${out:?}"
    mkdir -p "$out"
    cp "$bin/cadence-launcher.exe" "$out/cadence.exe"
    cp "$bin/cadence-game.exe" "$out/"
    cp README.md LICENSE "$out/"
    (cd dist && rm -f Cadence-windows.zip && powershell -NoProfile -Command "Compress-Archive -Path Cadence -DestinationPath Cadence-windows.zip")
    echo "dist/Cadence-windows.zip"
    ;;
*)
    out="dist/Cadence"
    rm -rf "${out:?}"
    mkdir -p "$out"
    cp "$bin/cadence-launcher" "$out/cadence"
    cp "$bin/cadence-game" "$out/"
    cp packaging/icon-1024.png "$out/cadence.png"
    cp packaging/cadence.desktop README.md LICENSE "$out/"
    tar -C dist -czf dist/Cadence-linux.tar.gz Cadence
    echo "dist/Cadence-linux.tar.gz"
    ;;
esac

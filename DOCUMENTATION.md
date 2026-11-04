# Cadence documentation

Cadence is a from-scratch reimplementation of Geometry Dash 2.2091 in Rust, for desktop and the
browser. It ships none of the original game's files: you supply your own copy.

> Status: playable from start to finish on the official levels, online levels download and play,
> physics bit-exact against the real game on 111 of 126 recorded suites. This file grows as
> features land; `NOTES.md` has how the original game works, `JOURNAL.md` the day-by-day log,
> `BENCHMARKS.md` the head-to-head numbers against the original.

## What you need

- A legal copy of Geometry Dash 2.2091 (Steam).
- To build: Rust 1.90+ (`rustup target add wasm32-unknown-unknown` for the web build).

## Building and playing

Native (macOS, Metal):

```bash
cargo run --release -p cadence-game              # main menu
cargo run --release -p cadence-game -- 1         # straight into a level; Space/Up/W/click to jump, R restart
```

Menus: the main menu, the level select (levels 1–22, The Tower's door starts The Tower, the
hidden door on the "Coming Soon!" page starts The Challenge), the pause menu (Esc or P; Space
resumes, Esc leaves) and the level-complete screen. Clubstep, Theory of Everything 2 and
Deadlocked unlock with 10/20/30 secret coins, as in the game.

Progress (normal-mode best %, attempts, jumps, secret coins per level) is saved in Cadence's own
file, never the game's: `~/Library/Application Support/Cadence/progress.txt` on macOS
(`$XDG_DATA_HOME/cadence` elsewhere; `CADENCE_SAVE_DIR` overrides), `localStorage` on the web.

Environment: `CADENCE_GD_RESOURCES` (the game's Resources folder, or its .app / install folder;
defaults to the Steam path; if the game's files aren't there Cadence says where it looked and
exits), `CADENCE_ONLINE=<level id>` (download and play an online level), `CADENCE_NOCLIP=1`
(debug: deaths don't count),
`CADENCE_QUALITY` = low | medium | high, `CADENCE_SCREENSHOT=<file.png>` + `CADENCE_STEPS=<n>`
(+ `CADENCE_SPEC=<spec>` for inputs, `CADENCE_SIZE=WxH`) renders one frame offscreen and exits.
`CADENCE_FX=<seconds>` then runs the effects that much longer (e.g. to see a death explosion).
Menu screenshots: `CADENCE_SCREEN=menu|levels|locked` (`CADENCE_PAGE=<0..23>` for the level
select, `CADENCE_UI_TIME=<seconds>` to advance its animations), or `pause|end` with a level id
to draw that menu over the level. Headless runs never read or write the progress save.

Effects: particles are the game's own effect files (`*.plist` in Resources) run by a port of
cocos2d's particle system (`crates/cadence-game/src/particles.rs`): death explosion, icon
shatter and circle, orb/pad/portal circles, the particles orbs, pads and portals carry, ground
drag, ship fire and sparks, landings. The physics reports what happened each step in
`Player::events`; the game turns those into effects (they never affect the simulation).

Sound: each attempt starts the level's song from the player's own files at the level's song
offset and stops it on death with the game's death sound (rodio natively; WebAudio on the web,
decoded straight from the dropped folder). Screenshots and benchmarks run silent.

Web (WASM; WebGPU, falling back to WebGL2):

```bash
tools/build-web.sh                 # builds web/pkg
python3 -m http.server -d web 8796 # any static server
```

Open the page, drop or choose the Geometry Dash folder (or its `Resources` folder), pick a level
and quality, press Play. Files are read locally; nothing is uploaded. Test switches: `?webgl=1`
forces the WebGL2 path; `?dev=1` reads game files from `web/gd/` (a local symlink to Resources,
gitignored, never deployed).

## Online levels and accounts

Cadence talks to the game's own servers (`https://www.boomlings.com/database`; `CADENCE_SERVER`
overrides it) for search, downloads, daily/weekly levels, map packs, gauntlets, lists, scores,
comments, profiles, news and quests. Requests are built in `crates/cadence-game/src/gjapi.rs` the
way the game builds them, so they look the same on the wire:

- every body starts with the game's base string
  `gameVersion=22&binaryVersion=51&udid=…&uuid=…&dvs=8`, then `&accountID=…&gjp2=…` when logged in;
- each endpoint's fields follow in the binary's order, with the same checksums (`chk`), the same
  `secret`, and quirks such as search sending `diff` twice;
- sent through the system libcurl like the game, with the same headers (no User-Agent).

The test `gjapi::capture_diff` rebuilds every request recorded from the real game
(`CADENCE_CAPTURE=<log>`, recorded by the harness with `CADENCE_HTTP_LOG`) and compares them byte
for byte: 19 of 19 identical (login, download, like with and without account, comments, rewards,
quests, search, gauntlets, upload).

Identity: Cadence keeps its own `identity.txt` next to its progress save (`udid`, `uuid`, the
account). On first run it copies the device and player ids from the game's save, read-only, so the
servers see the same player. Logging in turns the password into GJP2 (SHA-1 of the password and
the game's salt, what the game itself stores) and keeps only that; the password is never written
or logged. Writes (likes, comments, uploads, rewards) will be opt-in and paced like the real game.
Cloud save backup/sync is manual, as in the game.

Logging in: Settings → Account → Log in opens the game's login popup (`ui/login.rs`, from
`AccountLoginLayer`): username and password fields (Tab switches, Enter submits), the game's own
error messages in the field labels, Forgot Username / Password open RobTop's pages. On success the
account and its user id become Cadence's identity, as in the game, and the page shows "Linked to
account"; More → Unlink logs out. Save and Load (cloud backup) stay greyed: Save would replace the
account's cloud backup with Cadence's progress, which isn't in the game's save format yet.

Not done yet: registering, the rest of the account UI (chests, comment/like/rating buttons,
level upload). On the web, browsers can't call the servers directly (no CORS), so the online functions
are stubs there until a small relay server is deployed; the relay must never log credentials.

## Developer switches

Each works as an environment variable natively and, where a web name is given, as a URL query on
the web page (`?noclip&nocache`).

| Native | Web | What it does |
|---|---|---|
| `CADENCE_NOCLIP` | `?noclip` | Deaths don't happen (benchmarks and tests; skips `die` like the harness's noclip). |
| `CADENCE_NO_OCCLUSION` | `?noocclusion` | Turns off the occlusion pre-pass (see Rendering). |
| `CADENCE_NOCACHE` | `?nocache` | Turns off the static draw cache (see Rendering). |
| `CADENCE_NO_REGROUP` | | Draws each z layer in plain object order instead of glow sheet, fire sheet, game sheet. |
| `CADENCE_NO_COLUMNS` | | Culling checks every object every frame (as if all could move). |
| `CADENCE_CHECK_CULL` | `?check_cull` | Compare the culled set with a brute-force one every frame (logs `CULL MISMATCH`). |
| | `?nodraw` | Web measurement: run everything but skip submitting the frame. |
| `CADENCE_POLL` | `?poll` | Old busy loop (ControlFlow::Poll) instead of waiting for the next frame. |
| `CADENCE_FPS_TITLE` | `?fps_title` | FPS in the window title (off by default: retitling caused missed frames). |
| `CADENCE_DEBUG_SLOW` | `?debug_slow` | Per-frame timing: phases, per-object blocks, emitters, culling counts. |
| `CADENCE_NOVSYNC` | | Ask for no vsync. |
| `CADENCE_WINDOW=WxH` | | Window size in points. |
| `CADENCE_CORNER` | | Benchmarks while the Mac is in use: the window never takes focus, stays on top, ignores the mouse and is parked past the main screen's bottom-right corner (only a 320×200-point piece shows, above the Dock). |
| `CADENCE_BENCH_AUDIO` | | A benchmark keeps its audio running, muted (the real game's bench plays its music too). |
| `CADENCE_QUALITY` | | low / medium / high textures; unset picks like the game's "auto" (UHD from 3× 480×320, low under 1.5×, else HD). |
| `CADENCE_LEVEL_FILE=<file>` | `?level=<id>` | Play a saved level string (web: `bench/<id>.txt`). |
| `CADENCE_DEBUG_CAMERA`, `_COLORS=<ids>`, `_FX`, `_ORDER`, `_WORLD`, `_COUNTS`, `_OBJ=<id>` | | Logs for camera triggers, channel colors, shader/gradient state, the order-channel walk, world stats, physics counters, one object type. |
| `CADENCE_DUMP_DRAWS=<file>` | | Every sprite drawn in the screenshot frame (for `tools/sprite_diff.py`). |
| `CADENCE_DEATH_EFFECT=<1..20>` | | Screenshots: the death effect to play (headless runs read no save). |
| `CADENCE_CHECK_DEATH=<dir>` | | Compare the 20 death effects with the harness's `deathshot` captures in `<dir>` and exit. |
| `CADENCE_EDITOR_LEVEL=<file>`, `CADENCE_EDITOR_CAM=x,y` | | Editor screenshots: these objects (a level string), nothing selected, BPM markers on, the view's bottom left. |

## Benchmarks

`CADENCE_BENCH=<seconds>` plays a level in a window for that long (silent, never touching the
progress save) and prints frame-time statistics, the simulate + compose time per frame (Cadence's
own CPU work), sprites and draw calls per frame; `CADENCE_BENCH_OUT=<file>` writes every frame
interval. Use with
`CADENCE_NOCLIP=1`, `CADENCE_WINDOW=640x360` (1280×720 pixels on a Retina screen) and
`CADENCE_LEVEL_FILE=bench-levels/<id>.txt`:

```bash
CADENCE_BENCH=20 CADENCE_NOCLIP=1 CADENCE_WINDOW=640x360 CADENCE_LEVEL_FILE=bench-levels/<id>.txt ./target/release/cadence-game <id>
```

Every number needs the original game measured the same way, one run at a time: the harness's
`bench <seconds> <file>` (noclip, same window size) records the real game's frame intervals.

`tools/bench_pair.py [ids] [--modes vanilla,nocache,cache]` does both, level by level, without
getting in the way of someone using the Mac: the real game (`corner` and `mute` in its spec,
launched through LaunchServices in the background so it is never activated) and Cadence
(`CADENCE_CORNER`, `CADENCE_BENCH_AUDIO`) each run parked past the screen's corner with their
music playing muted. A watchdog checks the app in front twice a second and marks any run where
one of them came to the front. CPU and footprint are the kernel's counters for both
(`/usr/bin/time -l` for Cadence, the mod's `# rusage` line for the real game). The windows must
stay partly on screen: macOS doesn't pace a hidden or fully covered window by vsync, and the real
game stops drawing altogether.
`bench-levels/` holds the saved stress levels (pc killer, Lag Machine, the heavy demons), so
benchmarks don't download again; it is gitignored and never part of the game folder. Results:
`BENCHMARKS.md`.

## Rendering

The renderer (`render.rs`, wgpu: Metal natively, WebGPU or WebGL2 on the web) draws every sprite as
an instance of one quad; sprite instances are in world space and the camera is a uniform. Per
frame, `app.rs` builds the sprite list in the game's draw order:

1. **Culling**: static objects sit in 64-unit x columns built at load; objects that triggers can
   move are kept in their own columns and re-placed when they move (`World::moved_log`). Objects
   pinned to the screen are always checked.
2. **Per object**: position, scale, rotation, colors (channel, HSV, group pulses, alpha), edge fade
   and enter effects, glow, then its sprite tree (types' trees resolved once per frame).
3. **Static draw cache** (`DrawCache`): an object nothing moves, turns, scales, tints, animates,
   pulses or spins, at least 70 units inside both view edges (so no fade or enter effect), keeps
   the sprites it produced and replays them while its color channels are unchanged. Channels get a
   version each, bumped when a frame finds their color, opacity or blending changed.
   Verified pixel-identical with it off (`CADENCE_NOCACHE`) on every compared frame: 614 frames
   along the official levels' recorded routes and 210 across the 15 heavy levels.
4. **Batching**: within a z layer, sprites from the glow sheet, then the fire sheet, then the game
   sheet, like the game's batch nodes; a frame is about 15 draw calls.
5. **Occlusion pre-pass**: opaque texels (alpha exactly 1) of normal-blended sprites write depth
   first, front to back; the main pass then skips anything hidden behind them. The output is
   pixel-identical with it off.

Textures load at the quality the game would pick for the window, and their pixels are freed after
upload to the GPU.

## Command-line tools

The `cadence` binary (crate `cadence-tools`) inspects game data:

| Command | What it does |
|---|---|
| `cadence level <file>` | Decode a level file and print a summary (settings, object count, length). |
| `cadence level-raw <file>` | Print a level's decoded object string. |
| `cadence save <file>` | Decode a save (macOS AES or Windows format) and print a summary. |
| `cadence replay <spec> <out.csv>` | Simulate a spec and write traces (`%d` in the path writes every attempt). |
| `cadence diff <real.csv> <ours.csv>` | First step where two traces differ. |
| `cadence verify <spec>` | Simulate every attempt of a spec and compare with the real game's traces: position, velocity, flags, rotation and camera, bit for bit. |
| `cadence features <level or file>` | List what a level (a main level's id, or a level file) uses that Cadence doesn't handle yet (object types, triggers). |
| `cadence watch <spec>` | Simulate a spec's first attempt (a main level or its `levelstring` file) and write each `watchgroup` next to the harness's file as `<file>.ours`: per step the group's first objects with their drawn position, rotation, f64 position, enter-effect opacity and scale. |
| `cadence route <level> <n> <out%d.csv>` | Search inputs that survive in the simulation; prints a harness spec. |
| `cadence rects <level>` | Every collidable object's collision rect and radius (compare with the harness's `dumplevel`). |

## Verifying against the real game

`harness/` runs a copy of the game with a Geode mod (`harness/trace-mod`) that plays scripted
attempts with a fixed timestep and records the player after every physics step. The copy uses
an isolated home folder, so your real saves are never touched (`run-trace.sh` checks).
Besides inputs, a spec can ask for `screenshot <step> <file>`, `dumplevel <file>` (every
object with its rect), `particles <step> <file.csv>` (every live particle system: position in
the level, settings, plus `<file>.layers` with the object layer's z order), `triggers
<file.csv>` (every trigger the game fires, with its settings and the player's position) and
`deatheffect N` (per attempt), `playericon <cube> <c1> <c2>`, `deathshot <frame> <file>` (frames after player 1 dies: screenshot, scene, particles, circle waves), `fastseed N` (`fast_srand(N)` after every reset; `cadence verify` honors it too), `gui editlevel:0/edit:string:<file>` (a level string's objects in the editor), `watchgroup <group> <file.csv>` (positions of a group's first objects each step, with their node
opacity, scale, color and visibility; `watchmax N` objects, 4 by default). For
benchmarks: `bench <seconds> <file>`, `noclip`, `corner` (never activates itself; the window is
parked on top past the screen's bottom-right corner, ignoring the mouse; launch it with
`open -g` as `tools/bench_pair.py` does, since AppKit activates a directly started app before
the mod loads) and `mute` (music and effects at volume 0, still played).

```bash
python3 tools/fuzz_spec.py 1 60 "$PWD/re/traces/fuzz/a%d.csv" > re/traces/fuzz/spec
harness/run-trace.sh re/traces/fuzz/spec 400
cargo run --release -p cadence-tools -- verify re/traces/fuzz/spec
```

Visual checks: `tools/recompare.py [levels]` renders Cadence at the step of every real screenshot
(`shots/<level>/real-*.png`, with the real scene dump in `.scene`) and writes a stacked comparison
(real, ours, difference) per frame plus `shots/summary.csv`. `tools/sprite_diff.py <real.png>
<ours.dump>` lists sprites one side draws and the other doesn't. The harness can also record the
real game's server requests (`CADENCE_HTTP_LOG=<file>`, credentials masked) and its frame times
(`bench`).

`tools/verify_all.sh` re-verifies every recorded suite under `re/traces` (prints only suites
that differ, plus a summary; `-v` for all). `tools/rot_diff.py <spec>` shows the first rotation
mismatch per attempt with context. Traces record step, dt, x, y, y velocity, rotation, flags
(on ground, upside down, dead, ship, ball, UFO, ...), speed, size, gravity, camera, and the
game's own m_touchingGround, m_isRotating, m_rotationSpeed and the player's rects.

Setup (once): Geode SDK v6.0.0-alpha.1 cloned into `re/geode-sdk` with the loader dylibs in
`re/geode-sdk/bin/6.0.0/`, Geode CLI in `re/tools/bin`, the game copied to `re/gd-copy/`, the mod
configured with CMake into `re/build/trace-mod` (`GEODE_SDK`, `GEODE_BINDINGS_REPO_PATH`).

## Where the game's files are

| Platform | Game resources |
|---|---|
| macOS (Steam) | `~/Library/Application Support/Steam/steamapps/common/Geometry Dash/Geometry Dash.app/Contents/Resources` |
| Windows (Steam) | `<Steam>\steamapps\common\Geometry Dash\Resources` |

## Project layout

| Path | What |
|---|---|
| `crates/cadence-core` | Game logic with no platform code (formats, physics, triggers). |
| `crates/cadence-tools` | The `cadence` CLI. |
| `crates/cadence-game` | Renderer (wgpu), assets, app loop; native binary and the WASM library. |
| `web/` | The web page (start screen) and the built `pkg/`. |
| `tools/re` | Reverse-engineering scripts (Ghidra). Outputs stay in the gitignored `re/`. |
| `PLAN.md` | Scope, rules, milestones. |
| `NOTES.md` | Technical findings about the original game. |
| `JOURNAL.md` | Day-by-day log of the project. |

# Cadence: a Rust reimplementation of Geometry Dash 2.2091

Cadence plays Geometry Dash levels with physics that match the original step for step.
It runs natively and in the browser (WASM). It contains no RobTop code or assets:
players point it at their own Geometry Dash install (or drag the folder into the web page).

## Scope (agreed with Mark, 2026-11-02)

- All official levels: main levels 1–22, 3001, Tower levels 5001–5004.
- Every game mode: cube, ship, ball, UFO, wave, robot, spider, swing, platformer.
- The level editor with all its functions, except uploading.
- Online levels: search and play, read-only, through a pass-through relay. No login, no uploads,
  no likes or comments, nothing stored server side.
- Progress saved locally (native: a file; web: browser storage).
- Physics one-to-one with the real game, proven by differential tests.

Added the same day:

- Every menu, setting, screen and button from the real game. Features that need RobTop's
  servers for writing (upload, rate, comment, like, account) are shown but greyed out.
- Local levels: create, edit, save and play your own levels. Online levels can be saved to the
  "saved levels" list, as in the base game.
- Must look identical to the real game, but run far faster: the target is ~60 FPS on an old
  low-end Chromebook in the browser.
- Texture packs, and levels from older game versions.
- Modding: support modding frameworks now; later a built-in runtime mod API, so mods load
  without recompiling the game. Extras beyond vanilla may come later.
- Deploy to Mark's Cloudflare (static site + relay Worker). Testing happens in the built-in
  browser pane, not Chrome.

## Working rules

- At most one agent alongside me (any model except Fable). A second needs Mark's explicit OK
  and must be Sonnet or Haiku. Fable is never used. I do the work, not orchestrate.
- Downloads of tools and libraries are allowed.

## Performance plan (old-Chromebook target)

- Renderer on `wgpu` (Mark, 2026-11-02): **Metal** natively on macOS, **WebGPU** in browsers
  that have it, and **WebGL2** as the automatic fallback. The renderer only uses features that
  exist on the WebGL2 backend, so the old-hardware path is the same code and gets tested too.
- Last resort on the web: a **CPU software renderer** (own 2D sprite rasterizer drawing into a
  canvas) when neither WebGPU nor WebGL2 works.
- No quality loss on any backend: same output, only speed differs.

## Platforms

- **Web (main version):** WASM; WebGPU → WebGL2 → CPU.
- **Native macOS:** Metal. Later: sign in with your own Steam account inside the app and have it
  download the game files you own (like Horizon). The player types their own credentials.
  No launcher for now; the game comes first.
- Windows: maybe later.
- Everything batched: each sprite sheet becomes one texture, objects drawn in as few draw calls
  as possible, static geometry built once per section and reused.
- Spatial sections (like GD's own 100-unit sections) so only visible objects and active
  triggers cost anything per frame.
- Physics on a fixed step, independent of frame rate; rendering interpolates.
- No per-frame allocation in the hot loop; measured with a frame-time budget on every milestone.

## Modding plan

- Phase 1: Rust hooks (compile-time) with a stable `cadence-mod-api` crate.
- Phase 2: runtime mods as WASM modules loaded by the game (in the browser through the page,
  natively through an embedded WASM runtime), so mods work without recompiling.

## Rules

- Never commit anything from the game: levels, sprites, audio, saves, decompiled code.
  `re/`, `third_party/` and `gd-data/` are gitignored.
- Rust code is written from understanding, not transliterated line by line from decompiled output.
- Never write to Mark's real save folder (`~/Library/Application Support/GeometryDash`); read copies only.
- Don't call it "Geometry Dash" in names or UI.
- The relay is read-only and pass-through.

## Reference target

- Geometry Dash 2.2091, Steam app 322170, macOS universal binary (arm64 slice used).
- Function names come from Geode's bindings (`third_party/geode-bindings/bindings/2.2091`,
  `m1` addresses), applied to a local Ghidra project by `tools/re/ghidra.sh`.

## Architecture

| Path | What |
|---|---|
| `crates/cadence-core` | Platform-free game logic: level/save formats, objects, physics, triggers. Builds for native and wasm32. |
| `crates/cadence-tools` | CLI: decode levels and saves, dump objects, replay inputs, compare traces. |
| `crates/cadence-game` (later) | Renderer, audio, input, menus, editor UI. Native + web. |
| `harness/` (later) | Code injected into a *copy* of the real game to record per-step player state for given inputs. |
| `tools/re/` | Ghidra scripts and name extraction. |

## Verification (the core of the project)

1. The harness runs the real game on a level with a scripted input sequence and a fixed timestep,
   recording player state (position, velocity, rotation, mode, flags) after every physics step.
2. Cadence replays the same inputs on the same level and records the same fields.
3. `cadence-tools diff` reports the first step where they differ. A mismatch is a failing test.
4. Coverage grows by level and by mode; the parity table below is only "done" when backed by traces.

## Milestones

| # | Milestone | Status |
|---|---|---|
| M0 | Project, RE setup, level decoding | done |
| M1 | Real-game harness records traces | done (multi-attempt, fast-forward, object/sprite dumps) |
| M2 | Cube physics matches on Stereo Madness | done (82/82 attempts exact, incl. first ship section) |
| M3 | All game modes, portals, pads, orbs match | |
| M4 | Triggers (move, rotate, alpha, color, pulse, toggle, spawn, ...) match | |
| M5 | Renderer: levels look right (native) | first version (sheets, sprite trees, colors, camera, rotation) |
| M6 | Web build with bring-your-own-folder loading | done (WebGPU + WebGL2 fallback; CPU fallback pending) |
| M7 | Menus, progress saving, official levels complete | |
| M8 | Online level browser via relay | |
| M9 | Editor | |
| M10 | Every remaining menu, setting and button | |
| M11 | Optimization pass on a low-end target; texture packs; older-version levels | |
| M12 | Cloudflare deploy (site + relay) | |
| M13 | Mod API (compile-time, then runtime WASM) | |

## v0.0.2: nothing left "not yet" (Mark, 2026-11-04)

Mark: every item the README listed as missing gets done for 0.0.2. Tick items here as they land.

- [x] Web online play: Cloudflare Worker relay + async requests (`net.rs`).
- [x] Accounts: register, account help, cloud Load/Save (merge), profile button, own profile and
      posts, Friends / Blocked / Following / Muted (getGJUserList20 types 0–3), follow /
      unfollow / mute, Friend Requests, Account Settings, Old Messages (list, read, reply,
      delete, sent), likes, comments, reports, rate stars / demon, level upload with Share
      (options, rules), comment history, quests, chests, level leaderboards (getGJLevelScores211,
      chk pinned from a recorded request), Scores ranked by stat.
- [x] Currencies: orbs, diamonds (stat 13 total, 29 to spend), keys, shards; shops (all 283
      store items, Buy Item popup, GS_6); Paths of Power (unlock, active path, ranks, chest);
      chests open with RewardUnlockLayer; Treasure Room (5 pages, 634 chests, bonuses, gold
      chest, ropes to the shops); Shards of Power (4 pages); achievement banners; gauntlet
      chests.
- [x] Keybindings: the game's 214 actions in its tabs, defaults, Edit Keybind capture, search,
      export / import / reset; play and the editor follow them.
- [x] Editor: pause menu options, edit tools, level page tools, level settings (channels with
      the picker), music, 126 trigger popups with their sliders, Edit Special popups, the
      particle editor (all four tabs, probed in the real game), decimal inputs, guide overlays
      (2016 / 3642), Edit Object / Edit Special do nothing on objects without them (grayed in
      the game).
- [x] Online menus: User Search, Lists search, Advanced Options, level page tools, My Levels
      tools, The Safe, web import.
- [x] Icon kit: trails, ship fires, the special items, death effects (1–20, ported and checked
      frame by frame) with Explode.
- [x] Triggers: advanced follow, keyframes, enter effects, the game's random generator, camera
      key 370 (portals), area HSV, area triggers by position.
- [x] Simulation (agent, merged): platformer slopes (forced slide past 40°, slope launch;
      pfslope bit-exact), keyframe curves / reference-only / closed loops (kfcurve), Edit
      SFX/Song fades and proximity, dual-mode deaths and the 0.05 s fade, camera mode 370
      (cam370t), glows tinted and additive (the pale blocks), editor guides under the objects
      and the green playback line, ship fires 2–6 in play, Explode off (no shatter).
      verify_all 122/133.
- [x] My Lists: your own lists (browser, list page, edit mode, difficulty, duplicate, delete),
      levels added from a level page's list button, upload / delete (uploadGJLevelList body
      from the decompile: seed2 = base64(xor(sha1(levels sample + account), seed)); not yet
      recorded from the real game — its call:list harness command didn't send).
- [x] Launcher and packages (`cadence-launcher`, `tools/package.sh`, the release workflow):
      find the game, or get it with a Steam login (SteamCMD), Play, update check.
- [x] Group parents / linked objects in advanced follow and area triggers (gparent suite),
      SFX speed and per-sound proximity (agent, merged). verify_all 123/134.
- [x] Notifications (getGJNotifications, `mask=65535` as recorded): rows parsed like the
      game's parser (FUN_100493430: `key:value` rows, `#total:offset:count`) and drawn like
      GJNotificationCell (emote per kind, title + age, body, new dot, bell = mute). GUI diff
      vs the real page with the same made-up rows 5.0.
- [x] The Intel Mac release build: libcurl from the system on macOS (no OpenSSL to cross-build).
- [x] SFX leftovers (Edit SFX 414 stop-looping, pitch 405 as FMOD's pitch shift, random
      ranges 596–599, start/end, fades, loops, unique id / interval rules, reverb presets
      approximated) and spawn remaps in advanced follow + main objects for rotate / follow /
      move (agent, merged; new trace gremap). verify_all 124/135.
- [x] Message threads list (getGJThreads → MessageThreadsPage): rows by position as the
      game's parser reads them, drawn like the real page with made-up threads.
- [x] A thread's view (getGJThreadMessages → ThreadViewPopup): messages, players, the
      thread's state (earlier, locked, who can write), day separators and local times; sending
      (sendGJThreadMessage, recorded from the real game: base64 body + a random token).
- [x] "Load earlier" (beforeID = the oldest message shown) and new threads (NewThreadPopup,
      both modes as captured; the request in GameLevelManager::sendThreadMessage's order:
      `&isDM=1`, `&toAccountID`, `&subject`, `&targets`, `&description`, `&token`, `&body`).
- [ ] Still open: thread options in Cadence (modes recorded: 1 clear, 2 rename, 5 leave,
      10 mute, 15 pin), members (answer format/handler not found yet), invites, search,
      report (thread's type not read yet), and the "+" (share) and emote buttons.

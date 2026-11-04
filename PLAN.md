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

- [x] Web online play: Cloudflare Worker relay (pass-through, no logging, CORS) + requests as
      async (`net::post`): desktop runs them on a thread, the browser through the relay.
- [ ] Accounts in the menus:
      [x] register, account help, cloud Load/Save (merge, values only go up)
      [x] main-menu profile button; own profile (stats, icons, posts: post/delete/like),
          Friends (+ blocked), Friend Requests (received/sent: accept, remove), Account
          Settings (update), message to a player (legacy uploadGJMessage20)
      [x] level page: like/dislike, comment (with your best %), report
      [x] chests: state, timers, open (orbs/diamonds into Cadence's stats)
      [x] level upload: Share popup (request stars, notify), verify by completing from the
          level's page, empty/unverified/account checks, update keeps the id; [ ] its Options
          (unlisted / friends only) and "!" buttons
      [x] rate difficulty (unrated levels: 1–10 stars → rateGJStars211), [x] user search,
      [ ] rate demon,
          comment history, quests claim, the chest-opening animation (RewardUnlockLayer),
          [x] Messages (the legacy "Old Messages": list, read, reply, delete, sent);
          [ ] the newer threads page (getGJThreads answer format), [ ] notifications' rows
          (the page and its states are there), Following / Muted lists, level leaderboards (getGJLevelScores211's
          chk: salt yPg6pUrtWn0J + s7, values and key not pinned yet)
- [x] Triggers (agent, merged): Advanced Follow + Edit + Re-target, Keyframes, enter effects
      (3017–3023, 1915, old 22–28/55–59), Gameplay Offset, Camera Edge/Mode, Reverse, Random,
      Area Stop, Reset, Persistent Item, Progress, BG Effect, BG/MG speed, Touch, Spawn Particle,
      Checkpoint, Edit SFX/Song; 1931/3655 confirmed inert; 2016/3642 editor-only. Checked
      against real-game traces (advanced follow, keyframes, enter effects, camera, reverse).
      [x] the game's random rolls (fast_rand, traced with a fixed seed), [x] camera key 370,
      [x] area tint HSV mode (enter tint's was ported), [x] area triggers fire by position,
      [x] the 2016/3642 guide overlays in the editor. Still open: group parents/linked objects
      in advanced follow & area, keyframe curves / reference-only / closed loops, edit
      SFX/song speed and proximity.
- [ ] Editor: [x] pause menu options (settings, export, auto-save, get audio, …), [x] edit
      tools (grid, wrench, color), [x] level page tools, [x] level settings buttons (font,
      song type, 2-player, [x] the channels' colors with the picker), [x] music in the
      editor, [x] trigger popups for the triggers without one (64 more, 126 in all),
      [x] Edit Special popups (1594 1704 1751 2063 2902 2903 3027), [ ] the particle
      editor's fields (2065), [x] decimal inputs shown as "1.00" (D lines),
      [x] the color popups' picker drags (hue ring, saturation/brightness square → keys 7/8/9),
      [ ] trigger popups' sliders (each trigger's own slider ↔ value mapping).
- [ ] Online menus: [x] User Search, [x] Lists search, [x] Advanced Options (filters;
      [ ] layout from a capture), [x] level page Clone / Options / Delete / Refresh /
      favorite / folder (saved levels kept), [x] My Levels My Server / Import (.gmd drop) /
      folders / find / select all, [x] The Safe. [ ] My Lists (level lists of your own),
      [x] Import on the web (a .gmd dropped on the page while playing).
- [ ] Currencies: orbs, diamonds, keys, shards earned like the game's (GameStatsManager's
      awards per level / chest / quest), then quests' progress and Claim, the Paths
      (shards unlock and level them), the shops buying with orbs/diamonds, the chest
      opening animation (RewardUnlockLayer).
- [x] Icon kit: the effects pages (trails, death effects). [ ] ship fire / animations pages,
      [x] the chosen death effect used when dying (PlayerObject::playDeathEffect 0x1a176c:
      all 20 checked against real captures, `CADENCE_CHECK_DEATH`).
- [x] Odds: the flying-corridor camera (cadence_core::camera's corridor slide-in and static y,
      ported from the decompile), platformer slope angle (m_slopeAngle from the slope ridden;
      verify_all unchanged at 111/126, [x] platformer-slope trace recorded: re/traces/pfslope,
      [ ] it differs from step 259: uphill the real speed falls faster, see JOURNAL).
- [ ] Each piece checked against the real game (harness in corner mode only), tests, docs.

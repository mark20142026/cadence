# Physics: Cadence next to the original (2.2091)

What Cadence's physics does the same as the original game, bit for bit; what is close but not
identical; what is ported from the decompile but not yet checked against the real game; and
what is approximated or missing. The details (addresses, field offsets, derivations) are in
[NOTES.md](NOTES.md); this page is the overview.

## How "bit for bit" is checked

- **The real game is recorded.** A Geode mod in a copy of the game (`harness/trace-mod`) plays a
  level with scripted presses and writes every physics step to a CSV: x, y, y velocity,
  rotation, a flags word (on ground, upside down, dead, each game mode, sideways, platformer,
  dashing, on slope, accelerating), speed, size, gravity, camera, and the slope fields. The real
  game is deterministic under the harness: the same script gives byte-identical traces.
- **Cadence replays the same script** (`cadence verify <spec>`) and compares step by step:
  - x and y are written with 9 significant digits (enough to round-trip a 32-bit float), the
    velocity with 17 (a 64-bit double), so equal text means equal bits;
  - the flags word must be equal;
  - the visual rotation is compared bit for bit as well (reported separately).
- **A suite passes** (`tools/verify_all.sh`) only if every attempt matches every step *and* the
  rotation is bit-exact in every attempt. The camera (x and y) is compared bit for bit too and
  tallied on its own line.
- **The recorder's own timing is part of the comparison.** The harness writes a step's row at
  the end of the step, except the step a player dies: that row is written inside
  `PlayLayer::destroyPlayer`, during the collision pass, before either player's rotation update.
  Cadence reproduces that (a dying step leaves the rotation as it was, for both players in dual
  mode, which is also what the exit-dual copy shows). Older dual-mode traces, recorded before
  the harness ended an attempt on player 2's death, sampled that row at the end of the step;
  `verify` recognises them (player 2's file runs on past its death) and compares accordingly.
- **Moving groups** (triggers) are compared with `tools/cmp_watch.py` on the watched objects'
  f64 positions and rotations, with a tolerance (1e-4 in the suites that use it), because the
  game refreshes some node values only once per drawn frame.

**Current result: 134 of 135 suites bit for bit; the camera bit for bit in all 135.** In the
remaining suite, positions, velocities and flags still match every step; only the visual
rotation differs for a few steps (see "What isn't fully exact" below).

## At a glance

| Area | Status | Evidence |
|---|---|---|
| Step rate, step order, dt | Bit for bit | every trace |
| Cube (move, gravity, jump, rounding) | Bit for bit | levels 1–22 |
| Ship, UFO, ball, wave, robot, spider, swing | Bit for bit | levels 3–22, Dash |
| Mini, dual, mirror, speed portals | Bit for bit | levels 5–22, player-2 traces |
| Orbs, pads, dash orbs, teleport portals | Bit for bit | Polargeist, Deadlocked, Dash |
| Slopes (classic) | Bit for bit | v2–v5-16, Dash |
| Collisions (solids, hazards, rotated objects, saws) | Bit for bit | all levels; object rects match the game's own dumps |
| Platformer (walk, stop, turn, jump, slopes) | Bit for bit | pf1, pfslope, The Tower t1–t7 |
| Player rotation (visual) | Bit for bit in 134 suites; close in 1 | see below |
| Camera (follow, corridors, dual ground, key 370, level end, teleports, platformer band, spawned camera triggers, zoom) | Bit for bit in all 135 suites | camera columns, cam370t |
| libm calls (`sinf`, `cosf`, `exp2f`, `powf`) | Apple's, now and then an ulp from the correctly rounded value | The system's on macOS; elsewhere correctly rounded (f64, rounded) | Bit for bit on macOS (Dash's elastic zoom needed it) |
| Move / rotate / follow / spawn / advanced follow / area triggers, group parents | Within 1e-4 (watched groups) or bit for bit (player) | Geometrical Dominator, gparent, gremap, kfcurve, areahsv |
| Follow (1347), item triggers, the 2.2 move modes (player targets, direction, dynamic, silent, camera lock) and rotate modes (aim, follow, dynamic, bounds) | Ported from the decompile, not traced | unit tests |
| Random triggers (`fast_rand`) | Bit for bit | rand |
| Audio, particles, shaders | Not physics; ported or approximated separately | NOTES.md |

## Side by side: the player

| Item | Original game | Cadence | Status |
|---|---|---|---|
| Physics rate | 240 steps per second; `processCommands` gets dt = f32(1/240); `PlayerObject::update` runs with dt = 0.25 (time in 1/60 s) | Same, same order | Bit for bit |
| Steps per frame | `GJBaseGameLayer::update`: total = f32(extra + frameDt), steps = (int)(processed·240); at 60 Hz, 4 per frame | Same arithmetic; frame times within 25% of the display period snap to it (wall-clock jitter otherwise shakes the view) | Same steps; the snapping only changes when frames are drawn |
| Step order | buttons → spawn triggers → updateTimeMod → processCommands → resets → preCollisionCheck → move/rotate/follow actions → updateInternalActions → checkCollisions → updateSpecial → updateRotation → save position → postCollisionCheck → checkRepellPlayer → state triggers → tweens → updateCamera | Same | Bit for bit |
| Velocity rounding | `round3`: v unchanged if whole, else round((v − i)·1000)/1000 + i, round half away from zero (`frinta`); inlined ~25 times | Same function at the same places | Bit for bit |
| x per step | x = f32(x + base·speed·dt), base = (double)5.770002f at 1x, speed 0.9f | Same | Bit for bit |
| y per step | y = f32(y + yVel·dt·speed) — vertical motion ignores the speed portal | Same | Bit for bit |
| Cube gravity | g = 0.958199f·gravityMod (per-speed m_gravity); yVel = round3(yVel + f·−(g·dt)), f = 0.9 | Same (the f32 product matters) | Bit for bit |
| Jump | yVel = round3(f32(m_yStart)·size·dir); 11.18 at 1x; size 0.8 mini | Same | Bit for bit |
| Speed constants | per speed (0.7 / 0.9 / 1.1 / 1.3 / 1.6): base x, m_yStart, m_gravity from `updateTimeMod` | Same table | Bit for bit |
| Jump state machine | updateJump: buffered jump, rising branch, gravity branch, ±15 clamp, spin start past 0.25, "past 2g" tests not mirrored upside down (`m_fixGravityBug` off) | Same, quirks included | Bit for bit |
| Ship | thrust 0.5/0.4 by speed band, release 0.8/1.2, clamp [−6.4, 8] (÷ size), not while accelerating; velocity ×0.5 entering/leaving | Same | Bit for bit |
| UFO | fresh press sets 7·dir (mini 8·0.85), ×0.8 / ×1.2 gravity bands, size 0.85 mini | Same | Bit for bit |
| Ball | gravity ×0.6 of the fixed 0.958199; flipGravity on the ground; orbs ×0.7, pads ×0.6 | Same | Bit for bit |
| Wave, robot, spider, swing | from the decompile (NOTES: wave/bounds, robot, spider, swing sections) | Same | Bit for bit on levels 16–22 |
| Mini | hitbox 18 at once; sprite eases (CCEaseElasticOut, visual) | Same, elastic ease with the binary's exp2f/sinf/fmadd | Bit for bit |
| Dual mode | second player, shared input rules | Same | Bit for bit (player-2 columns compared step by step) |
| Corridors | heights 300 / 240 / 270; floor = max(90, ⌊(portal.y − h/2)/30⌋·30); ground slides in with a tween | Same, tween included | Bit for bit |
| Orbs | press/ring flags copied in one 2-byte store at the end of update; one orb per step; factors per orb and mode | Same | Bit for bit (Polargeist onward) |
| Pads | powers per pad and mode; gravity pads only when facing the gravity | Same | Bit for bit |
| Dash orbs, teleport portals | from the decompile | Same | Bit for bit (Deadlocked, Dash) |
| Collision order | collect solids and hazards; specials (portals, pads, rings) at once; solids first (player rect recomputed after each), hazards second | Same | Bit for bit |
| Hitboxes | object table dumped from the real game (4,093 ids); spikes 0.2 × 0.4 of the sprite etc.; flip then rotate | Generated from that dump | Exact (all 1,401 collidable Stereo Madness objects; all of levels 19 and 20 against the game's own rects) |
| Rotated objects, saws | OBB per object (rect = its bounding box); SAT against the player's box rotated by its visual rotation; circle test for saws | Same | Bit for bit (302 rotated objects of Hexagon Force match) |
| Slopes | `collidedWithSlopeInternal`, slope velocity, exit launch, jump bonus, ball roll; directions 0–7 | Same | Bit for bit (617 Hexagon Force slopes' directions match; v2-16 rides bit for bit) |
| Platformer | walk acceleration, stop, turn, jump, slides on slopes over 40° | Same | Bit for bit (pf1, pfslope, t1–t7-5001) |
| Death | hazard, crush, out of bounds; the step that kills skips updateRotation | Same | Bit for bit (death suite, every route's last step) |

## Side by side: rotation (visual)

| Item | Original game | Cadence | Status |
|---|---|---|---|
| Easing | quaternion slerp of half angles with FMAs (m1 0x216db8); angles in (−360°, 360°] | Same, FMAs included (without them, 1-ulp differences) | Bit for bit |
| Cube on the ground | ease to the nearest 90° by min(dt, speed·0.175·3·dt) while touching | Same | Bit for bit |
| Spin | +180° per 0.4333 s at 1x; sign fixed at the start | Same | Bit for bit |
| Ship, UFO, wave, swing (`updateShipRotation`) | target atan2f(−dy, dx) (sideways atan2f(dx, dy); going left the forward step is taken as \|·\| and the target mirrored), t = 0.15·dt (wave 0.25, mini 0.4; UFO −0.4 of the angle, clamped to 0.1 rad, 0.07); only if the move is long enough, except a platformer ship or UFO; snapped back onto a moving block (+0x658) the target is level and the rate a quarter; platformer ship ±30° / ±45° by input | Same | Bit for bit |
| Robot, spider off the ground | back to 0° at half the step time; the rate ×3 while touching the ground (`updateRotation` 0x36ccac) | Same | Bit for bit |
| Ball, swing, sideways, slopes | from the decompile | Same | Bit for bit in most attempts; the exceptions are below |

## Side by side: camera

| Item | Original game | Cadence | Status |
|---|---|---|---|
| x | holds at 15, then player.x − (view/2 − 75) | Same | Bit for bit |
| y | follow band 70 above / 40 below the center, eased /40 per step | Same | Bit for bit |
| Corridors | the center tweens to floor + h/2 over 0.5 s (0.4 between corridors), ease-in-out via the system powf | Same (LLVM must not fold powf(x, 2) into x·x) | Bit for bit |
| Camera triggers (static, offset, zoom, rotate) | `updateCamera` (~1,000 lines) | Ported from the decompile | Not all traced |
| Camera key 370 | the corridor isn't snapped to the 30-point grid | Same | Bit for bit, camera included (cam370t) |
| Dual ground | `animateInDualGroundNew`: 0.4 s in dual mode or from another corridor, else 0.5 | Same | Bit for bit (Hexagon Force) |
| Level end | the end portal's limit (+0x3138 − view / zoom − offset), eased in like an edge | Same | Bit for bit |
| After teleports | key 55 sets a 0.5 s teleport ease; y eased by 30 while it runs | Same | Bit for bit (Deadlocked, Fingerdash) |
| Left edge, platformer band and easing | edge max 15; band 55 / 27.5, x easing 8; speed-based easing min(30, 30 − 0.2·distance); zooming rescales the internal offset and moves the camera with it | Same | Bit for bit (pf1, pfslope, The Tower t1–t7) |

## Side by side: triggers and moving objects

| Item | Original game | Cadence | Status |
|---|---|---|---|
| Move, rotate, alpha, toggle, spawn, pulse, color | from the decompile | Ported | Player bit for bit on Geometrical Dominator; moved objects follow |
| Group parents, linked objects, main objects | special group keys, parent / only object as the center | Ported | gparent: watched groups within 1e-4 |
| Spawn remaps in advanced follow and area clusters | `FUN_1000d64d0` remaps 51 / 71 | Ported | gremap: all 9 watched groups within 5e-6 |
| Keyframes (curves) | keyframe animation | Ported | kfcurve: positions to the trace's print precision |
| Area HSV | area effects | Ported | areahsv passes |
| Random triggers | `fast_rand` (seeded) | Same generator | Bit for bit (rand) |
| Follow (1347), item triggers | from the decompile | Ported | Not yet checked against a trace |
| Move trigger 2.2 modes: player 1 / 2 as target (138 / 200), direction mode (394, 396), dynamic (397: aimed again every step, its own eased clock), silent (544: last positions too), camera lock (141 / 142) | `triggerMoveCommand`, `getMoveTargetDelta`, `triggerDynamicMoveCommand`, `processDynamicObjectActions`, `moveObjectsSilent`, `prepareMoveActions` | Ported | Not yet checked against a trace (Tower floors 2–4 use them) |
| Rotate trigger aim / follow (100 / 394), target 401 or players, offset 402, bounds 516–519, dynamic (397, 1/403 of the angle per step) | `triggerRotateCommand`, FUN_1000f0c34, `triggerDynamicRotateCommand` | Ported | Not yet checked against a trace (The Tower's suites, whose level has one, still pass) |
| Scale and teleport centers on multi-object groups without a parent | — | The group's first object | — |

## Coverage by level

Each level has one or more suites (routes through the level, random-press "fuzz" runs,
different gravity and speed starts). All of them compare every step.

| Level | Suites | Fully bit for bit |
|---|---|---|
| 1 Stereo Madness | 6 | 6 |
| 2 Back on Track | 2 | 2 |
| 3 Polargeist | 3 | 3 |
| 4 Dry Out | 5 | 5 |
| 5 Base After Base | 4 | 4 |
| 6 Can't Let Go | 4 | 4 |
| 7 Jumper | 5 | 5 |
| 8 Time Machine | 3 | 3 |
| 9 Cycles | 3 | 3 |
| 10 xStep | 3 | 3 |
| 11 Clutterfunk | 3 | 3 |
| 12 Theory of Everything | 3 | 3 |
| 13 Electroman Adventures | 3 | 3 |
| 14 Clubstep | 4 | 4 |
| 15 Electrodynamix | 3 | 3 |
| 16 Hexagon Force | 6 | 6 |
| 17 Blast Processing | 6 | 6 |
| 18 Theory of Everything 2 | 4 | 4 |
| 19 Geometrical Dominator | 8 | 8 |
| 20 Deadlocked | 5 | 5 |
| 21 Fingerdash | 6 | 6 |
| 22 Dash | 20 | 19 |
| 3001 The Challenge | 2 | 2 |
| 5001 The Tower (platformer) | 7 | 7 |
| Targeted suites (camera key 370 ×2, death, object dumps of levels 19–22 and 3001, group parents, remaps, keyframes, area HSV, platformer walk and slopes, random, multi-attempt, debug) | 17 | 17 |
| **Total** | **135** | **134** |

## What isn't fully exact

One suite, s19-22 (Dash), from step 20170: a wave turns into a cube through a portal in the same
collision pass that lands it on two slopes. Cadence eases the cube's rotation toward the slope
once more than the game (2° for a few steps). The game evidently meets the first slope before
the portal in that pass (the portal's mode switch then resets the rotation); Cadence walks the
portal first. Positions match throughout.

Earlier gaps (Hexagon Force's ball roll and dual deaths, Geometrical Dominator's ship
on a moving block, Dash's sideways ship and falling robot, Fingerdash's last bits, the ship
start at step 0) are fixed; see JOURNAL.md for each cause.

One rule in the moving-block snap is empirical: a player already moving away from the ground
isn't snapped back onto a block that moved away (r5-19 step 5820, s17-22 step 16129). The
decompile of the snap (0x36e834) doesn't show where the game decides this.

The rotation is visual only: it feeds nothing else in the physics except the hazard test's
rotated player box.

## Approximated or not ported (physics-related)

- Off macOS the libm calls are correctly rounded rather than Apple's: an ulp apart now and then
  (the game on Windows uses Microsoft's, which differs from both in the same way).
- Scale and teleport centers of groups with several objects and no parent (these use the
  group's first object).
- Ported from the decompile but not yet traced: follow (1347), item triggers, the move
  trigger's 2.2 modes (player targets, direction, dynamic, silent, camera lock) and the rotate
  trigger's aim / follow / dynamic modes. Recording The Tower's upper floors (5002–5004), which
  use them, would check them.

## Not physics, for completeness

Drawing, particles, shaders, audio and the menus are compared with screenshots and scene dumps
rather than traces; see NOTES.md and JOURNAL.md. Two of them are approximations by necessity:
the SFX reverb (FMOD's reverb isn't public; built on the game's preset values) and the
pixelate shader.

## Reproducing

```bash
tools/verify_all.sh          # every suite; prints the ones that differ and the total
tools/verify_all.sh -v       # every suite with its line
./target/release/cadence verify re/traces/v2-16/route.spec
```

The traces live in `re/traces/` (not in the repository: they're recordings of the original
game). Recording new ones needs a copy of the game and the harness (`harness/trace-mod`).

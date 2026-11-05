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
  rotation is bit-exact in every attempt.
- **Moving groups** (triggers) are compared with `tools/cmp_watch.py` on the watched objects'
  f64 positions and rotations, with a tolerance (1e-4 in the suites that use it), because the
  game refreshes some node values only once per drawn frame.

**Current result: 124 of 135 suites bit for bit.** In the other 11, positions, velocities and
flags still match every step; only the visual rotation differs, in some attempts (see "The 11
suites that aren't fully exact" below).

## At a glance

| Area | Status | Evidence |
|---|---|---|
| Step rate, step order, dt | Bit for bit | every trace |
| Cube (move, gravity, jump, rounding) | Bit for bit | levels 1–22 |
| Ship, UFO, ball, wave, robot, spider, swing | Bit for bit | levels 3–22, Dash |
| Mini, dual, mirror, speed portals | Bit for bit | levels 5–22, player-2 traces |
| Orbs, pads, dash orbs, teleport portals | Bit for bit | Polargeist, Deadlocked, Dash |
| Slopes (classic) | Bit for bit, except ball-on-slope rotation in one Hexagon Force attempt | v2–v5-16 |
| Collisions (solids, hazards, rotated objects, saws) | Bit for bit | all levels; object rects match the game's own dumps |
| Platformer (walk, stop, turn, jump, slopes) | Bit for bit | pf1, pfslope, The Tower t1–t7 |
| Player rotation (visual) | Bit for bit in 124 suites; close in 11 | see below |
| Camera (normal, corridors, key 370) | Bit for bit where traced | camera columns, cam370t |
| Camera at the level end, after teleports, edges (2062) | Approximated / not ported | — |
| Move / rotate / follow / spawn / advanced follow / area triggers, group parents | Within 1e-4 (watched groups) or bit for bit (player) | Geometrical Dominator, gparent, gremap, kfcurve, areahsv |
| Follow (1347), item triggers | Ported from the decompile, not traced | — |
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
| Ship | target atan2f(−dy, dx), t = 0.15·dt, only if the move is long enough | Same | Bit for bit |
| Ball, swing, sideways, slopes | from the decompile | Same | Bit for bit in most attempts; the exceptions are below |

## Side by side: camera

| Item | Original game | Cadence | Status |
|---|---|---|---|
| x | holds at 15, then player.x − (view/2 − 75) | Same | Bit for bit |
| y | follow band 70 above / 40 below the center, eased /40 per step | Same | Bit for bit |
| Corridors | the center tweens to floor + h/2 over 0.5 s (0.4 between corridors), ease-in-out via the system powf | Same (LLVM must not fold powf(x, 2) into x·x) | Bit for bit |
| Camera triggers (static, offset, zoom, rotate) | `updateCamera` (~1,000 lines) | Ported from the decompile | Not all traced |
| Camera key 370 | the corridor isn't snapped to the 30-point grid | Same | Bit for bit, camera included (cam370t) |
| Level end | static y over 1.2 s; x settles at length − view width | Approximated | Close, not exact |
| After teleports | a bezier transition in `updateCamera` | Not ported | Differs for a while after such teleports |
| Camera edges (2062), platformer camera branch | in `updateCamera` | Not ported | — |

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
| Rotate / move aim modes, player targets for move-to-target, scale and teleport centers on multi-object groups | — | Not ported | — |

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
| 16 Hexagon Force | 6 | 3 |
| 17 Blast Processing | 6 | 5 |
| 18 Theory of Everything 2 | 4 | 4 |
| 19 Geometrical Dominator | 8 | 7 |
| 20 Deadlocked | 5 | 5 |
| 21 Fingerdash | 6 | 4 |
| 22 Dash | 20 | 17 |
| 3001 The Challenge | 2 | 2 |
| 5001 The Tower (platformer) | 7 | 7 |
| Targeted suites (camera key 370 ×2, death, object dumps of levels 19–22 and 3001, group parents, remaps, keyframes, area HSV, platformer walk and slopes, random, multi-attempt, debug) | 17 | 16 |
| **Total** | **135** | **124** |

## The 11 suites that aren't fully exact

In every one of them, x, y, velocity and flags match every step. The difference is the drawn
rotation, in some attempts:

| Suite | Where it differs | Size |
|---|---|---|
| v3-16, v4-16, v5-16 (Hexagon Force) | attempt 3 from step 5376: the ball rolling on a slope while accelerating | ~1.3° off, then a half-turn (the roll direction) |
| v5-16 attempt 0 | step 7467, ship on a slope | ~1.7° |
| s17-22 (Dash) | from step 16439: upside-down ship going sideways | mean 43°, max 73° |
| s19-22 (Dash) | step 20170: ship rotation | max 2° |
| s18-22 (Dash) | rotation | max 0.07° |
| s3-21, s4-21 (Fingerdash) | rotation | under 0.0005° (last-bit differences) |
| r5-19 (Geometrical Dominator) | one attempt | max 0.25° |
| w3-17 (Blast Processing) | attempt 3, step 12015: the wave's rotation on the step it dies | ~2.8° |
| cam370 | the ship start's rotation at step 0 (real −3.35°, ours 0) | 3.3° at step 0 |

These are visual only: the rotation feeds nothing else in the physics except the hazard test's
rotated player box, and the positions after these steps still match.

## Approximated or not ported (physics-related)

- The camera at the level end (approximated), after teleports (bezier not ported), camera edges
  (2062) and the platformer camera branch (not ported).
- Rotate / move aim modes, player targets for move-to-target, scale and teleport centers of
  groups with several objects and no parent (these use the group's first object).
- Follow (1347) and item triggers: ported from the decompile, not traced.
- The game forcing level setting kA40 on in official level 22 (Dash) isn't done.

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

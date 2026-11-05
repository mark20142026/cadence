# Journal

## 2026-11-02: Day 0

**Decided with Mark:** port Geometry Dash to Rust, compiled to both native and WASM. Players bring
their own GD folder; nothing from RobTop ships. Full scope: all official levels, every mode,
the editor (minus upload), read-only online levels, local progress, and physics proven one-to-one.
Mark has 2,332 hours in GD and is the final judge of "feel".

**The install.** Steam app 322170, `CFBundleShortVersionString` 2.209, universal binary with a
native arm64 slice. Levels are in `Resources/levels/*.txt` (1–22, 3001, 5001–5004), each a
base64 + gzip blob (starts with `H4sI`). Audio is FMOD; 24 main-level songs ship as mp3.

**The binary is stripped** (zero defined symbols) but keeps C++ RTTI class names.
Geode's bindings repo has `m1` (macOS arm64) addresses for 2.209 and 2.2091. Disassembling at
both versions' addresses for four functions settled it: 2.2091 addresses land on real function
prologues, 2.209's land mid-function. **The install is 2.2091.** 10,647 names extracted and applied
to a Ghidra project with no failures.

**Saves are AES-encrypted on macOS.** Both `CCGameManager.dat` and `CCLocalLevels.dat` start with
the same 32 bytes: ECB mode encrypting the same XML header. The key is in the binary; to be read
from the decompiled save code rather than trusted from the internet.

**Scope grew** (Mark, same day): every menu and button (server-write features greyed out),
saved online levels, texture packs, older-version levels, modding (compile-time now, runtime WASM
mods later), a low-end Chromebook as the performance target, and a Cloudflare deploy. Added
DOCUMENTATION.md (how to use and build) and NOTES.md (findings about the original game).
Agent rule: at most one agent alongside me, never Fable.

**Rendering stack** (Mark): native Metal, WebGPU in the browser, WebGL2 fallback. That's `wgpu`,
which does all three from one renderer.

**First code: level decoding.** `cadence-core` gets a codec (lenient base64, gzip or zlib) and a
level-string parser that keeps every raw key/value pair in order. Two surprises from real data:
start positions embed settings keys inside an object (level 22), and two Tower level files carry
junk after a NUL terminator. After handling both, **all 27 official levels decode and write back
byte-identical.**

**The harness works.** A Geode mod (`harness/trace-mod`, Geode v6.0.0-alpha.1, the first with
2.2091 support) runs inside a *copy* of the game with an isolated home folder
(`CFFIXED_USER_HOME`), so Mark's real saves are never touched; `run-trace.sh` checks their
timestamps after every run. Lessons on the way: Geode keeps its folder *inside the app bundle*;
switching scenes from inside `MenuLayer::init` is overwritten by the loading screen (fixed with
`queueInMainThread`); and the field I first recorded (`m_position`) only updates once per frame.
The real per-step position is the node position. **Two runs give byte-identical traces**, so the
reference is deterministic.

**First physics facts from the real game:** 240 steps/s, player `update` dt 0.25, jump velocity
11.18, gravity 0.216 per step, y += yVel × 0.225. One open question: x moves 1.29825044 per step,
a few float ulps more than the obvious formula. Waiting on Ghidra to settle it.

**Saves:** the CCCrypt call shows AES-256-ECB with PKCS#7. The documented key decrypts Mark's
saves (copies) to valid plists, so the real data verifies it. Saves now decode and write back
byte-identical. Found a wrong address in Geode's bindings (`PlayerObject::updateJump`).

**First 1:1 physics.** Black-box tests against traces found x = 5.7700018882751465 × speed × dt
(the field at +0x788 holds 5.770002f, not 5.77) and a strange per-step velocity rounding. The
`frinta` instruction in `updateJump` gave it away: the game rounds only the *fractional* part of
the velocity to 3 decimals and adds the integer part back. With that, Cadence's Rust physics
reproduces the real game's jump **bit for bit**, and after loading the object table it **dies on
the first spike of Stereo Madness at exactly step 390**, like the real game. `cadence replay` +
`cadence diff` are the loop from now on.

**Ghidra finished** (~35 min). 220 player/collision functions decompiled. Its output showed my
gravity constant was a lucky stand-in (0.216 per step) for the real formula
(0.9 × 0.958199 × dt in f32, then rounded), so I replaced it; traces still match exactly. Field
offsets dumped from Geode's headers (1,389 fields) let `name_fields.py` turn
`*(char *)((long)param_2 + 0x977)` into `param_2->m_isUpsideDown`.

**Object data from the real game**: types and hitboxes for every id, and each object's full sprite
tree (frames, offsets, z, blend), both from a synthetic level holding every object. A bare
`createWithKey` lies (spikes say "solid 30×30"); only objects inside a level get their real
hitboxes and child sprites.

**The verification loop.** The harness now runs many attempts per launch (a fresh PlayLayer
scene per attempt, so no state or pending timers leak) and fast-forwards several updates per
frame; repeated attempts are byte-identical to single runs. `tools/fuzz_spec.py` writes random
inputs, `cadence route` beam-searches inputs that survive in Cadence's own simulation, and
`cadence verify` replays every attempt and reports the first differing step. Each mismatch was
one concrete piece of missing physics:

1. Press vs hold jumps: a press jumps in `processCommands` (gravity still applies that step); a
   held button jumps inside `updateJump`, which skips gravity that step.
2. Movement is a float delta added to the float position (y matched from the ground but drifted
   by 1 ulp when jumping from a block until this was right).
3. Cube block landing: snap onto the top when falling within 10 units; otherwise the 9×9 inner
   box (30% of the player) touching the block kills.
4. Jump snapping (`checkSnapJumpToObject`): landing on a solid at a standard jump offset from the
   previous one nudges x by up to 1 unit (tables per speed).
5. Terminal fall speed −15, rounded.
6. Solid order: collected by 100-unit x-section, ascending, sorted by unique id (creation order)
   inside a section, then processed **backwards**. The section index uses the f32 factor
   0.00999999978, so x = 3300 is section 32.

**Result: 66/66 attempts match the real game exactly** (60 random + 6 searched routes,
~50,000 steps), covering the whole cube section of Stereo Madness up to the ship portal.

**The ship.** Searched routes now reach the ship portal in the real game, which produced ~800
steps of real ship flight per route. Porting it showed one misreading of mine: `updateJump`
receives `dt × playerSpeed` (0.225 at 1x), and the "0.9 cube gravity factor" I had found was
really that speed term (the cube's own factor is 1.0; 0.9 is the robot's). Every ship delta
then matched (+0.108/+0.086 thrust, −0.069/−0.103 falling, clamp [−6.4, 8]). The ship's
on-ground flag is set by gentle floor/ceiling hits (velocity ≤ 5) and never cleared in flight;
landing on blocks doesn't set it. Corridor: floor = (portal.y − 150) floored to 30, ≥ 90.

**Result: 82/82 attempts match exactly** (60 random + 22 searched routes, ~155,000 real steps),
covering the cube and the first ship section of Stereo Madness. Not modeled yet: the 0.4–0.5 s
ground slide-in when the corridor appears (collisions use the animated ground + camera), ship
block undersides beyond what routes have touched, other modes, triggers.

**First light (native, Metal).** New crate `cadence-game`: wgpu renderer (one instanced draw per
sheet run, gamma-space blending like cocos2d, offscreen screenshot mode), cocos2d plist sheet
loader (trimmed/rotated frames, SD/HD/UHD), a sprite table generated from the real game's sprite
trees, legacy and 2.0 color channels, fixed 240 Hz physics with keyboard/mouse input. Stereo
Madness renders from the player's own files on Metal (Apple M2). Known gaps: icon secondary
layer missing, camera and cube rotation are approximations, no glow, no ground line color, no
music yet.

**Mark's first play test: "jumping feels off, the camera shakes; it should glide."** Causes:
my loop stepped physics from a jittery wall clock (3–5 steps per frame instead of GD's steady 4),
the camera followed instantly, and the cube snapped to 90° on landing. Fixes: frame times snapped
to the display period; GD's camera (70/40 dead zone, distance-based easing) fitted to recorded
camera traces; GD's rotation (eased landing, 180°/104 steps) verified at 0.000° on jump traces.
Also found why the icon's cyan center was missing: sprites need their own frame size as content
size, not 30×30.

**Web version runs.** The game crate is now a library shared by the native binary and a WASM
build. Files come through a `Files` trait (disk, or files dropped into the page, kept in memory).
On the web the renderer is created asynchronously and handed to the event loop as a user event;
wgpu's WebGPU detection checks for a real adapter before choosing WebGPU over WebGL2. Tested in
the built-in browser: Stereo Madness runs on WebGPU and on forced WebGL2 (ANGLE/Metal), identical
frames. Found that color triggers were being drawn (editor-only markers); hidden during play.
Start screen passes the design inspector (0 FAIL). wasm is 5.7 MB unoptimized (debug info + the
visual table); to slim before deploy.

**"Something looks off, grainy, greyish."** Instead of guessing, the harness now saves the
real game's framebuffer at a given step (hooked `CCEGLView::swapBuffers`) plus a dump of its scene
graph, and `tools/compare_shots.py` diffs it against Cadence's frame of the same step. What it
found, in order of impact: textures must be premultiplied with the game's blend modes (straight
alpha gave grey fringes on every soft edge); missing glows; background placement/scale; ground
details (y 91, the 444-wide fading line, edge shadows); color triggers (the level turns purple —
first fade checks out to the exact RGB values); the camera (exact now: constant easing 10 at
dt 0.25 — the 10..30 easing was platformer-only); and web frame pacing (median-of-frames snapped
to standard refresh rates). Frame difference went from 8–80 to 2–3.6 out of 255.

**"It's stuttering, I can feel it in most areas."** Measured with a new benchmark mode (plays a
route, reports frame intervals and steps per frame): frames reached the screen evenly, but 13 of
774 ran the wrong number of physics steps because CPU-side timing jitter slipped past my 25%
snap. Two attempted fixes were themselves wrong and the benchmark caught both (a hidden window
ran the game 240× fast; carrying the remainder alternated 0/8 steps). Final: round each frame to
whole display periods, with a float tolerance. Now ~1 irregular frame per 10 s (real missed
display frames). Cadence's own work is 0.06 ms per frame.

**Stereo Madness complete, bit for bit.** A wider beam search (512) finds routes through the
whole level; in the real game they match step for step until the level end at x 26,425. Two
last fixes: a ship landing on a block top does set on-ground (my earlier "it doesn't" came from a
case that was really the ship hitting a block's underside), and physics stops at
`level length − 300` with GD's length = furthest object x + 340 (then the game animates the player
into the end portal — not physics). All 89 attempts (~237,000 real steps) match exactly.

Next, from a survey of all 27 levels: Back on Track (pads, gravity), Polargeist (orbs), …,
Hexagon Force (617 slopes, dual), …, Dash and the Tower (spider, swing, 2.0+ triggers).

**Mistake:** `analyzeHeadless -analysisTimeoutPerFile 0` means a zero-second timeout, not "no
limit". The first analysis run did nothing. Removed the flag and reran.

## 2026-11-02 (later): Polargeist and exact rotation

**Orbs.** Read `pushButton`, `releaseButton`, `ringJump`, `playerTouchedRing`,
`resetTouchedRings` and the game layer's step loop. First version matched 50/60 random attempts;
the misses fired an orb while holding a press that had already jumped. The real rule sits in a
2-byte store at the end of `PlayerObject::update` that copies two flags at once. My field-naming
script had labeled only the first byte. With it: 60/60. Planned routes then showed orbs fire
once per attempt (the object is marked activated), not once per press. Fixed that, re-planned:
**all 6 routes complete Polargeist in the real game and match bit for bit** (139,472 steps).
The trace writer also gained the upside-down flag (no earlier trace had flipped gravity).

**Rotation, now bit-exact everywhere.** The landing ease isn't a shortest-arc lerp. It's a
quaternion slerp on half angles with fused multiply-adds, so angles range over (−360°, 360°].
My old "normalize at spin start" rule had been papering over that. Also: the spin direction is
fixed when the spin starts, entering a cube portal starts a spin, the ship eases at a fixed
0.15·dt toward atan2(−dy, dx), and a fatal step doesn't rotate. Result: all 227 recorded
attempts (SM, BoT, Polargeist; ~545k steps) match position, velocity, flags and rotation bit
for bit. `cadence verify` now reports bit-exact rotation too; `tools/rot_diff.py` finds the
first rotation mismatch.

## 2026-11-02 (night): levels 4–15 and the camera

Ported ball, mini, UFO, speed portals, breakable blocks, sawblades and gravity-pad facing, and
rebuilt the cube's ground/rotation state around GD's real flags (m_isRising, m_touchingGround,
the unmirrored "past 2g" tests upside down). Every recorded attempt of levels 1–15 now matches
the real game in position, velocity and flags: 45 suites, 600+ attempts. The real game
completed 10+ full routes of levels 4–10 that Cadence planned.

Lessons: (1) zsh doesn't word-split `$var`, and with MULTIOS `2>&1 >/dev/null` duplicates
stdout instead of discarding it. (2) The route search merged states that differed only in
hidden flags; the key now includes them. (3) Rust folded `powf(x, 2.0)` into `x*x`; the game
calls the system powf, which is an ulp off sometimes — `black_box` the exponent. (4) Asking the
game directly (extra trace columns: touching-ground, rotating, rotation speed, the player's
rects) settled questions the decompiler couldn't: the inner box is 9×9 even when mini.

The camera moved into cadence-core and `cadence verify` checks it too: bit-exact everywhere
except the last ~2 s of a level (end camera).

## 2026-11-02 (late night): sound, mirror, level end, rotation everywhere

Rotation is now bit-exact on all 45 suites too (cube spin rate fixed at spin start; ball spin
in doubles with the speed table, as the binary does; UFO tilt clamped to −0.1 rad). Levels
1–15 are complete physics-wise.

Added what playing needs: music and the death/complete sounds (rodio natively, WebAudio on the
web decoding straight from the player's folder; a song that decodes late starts further in so
it stays in time), mirror portals (the 0.4 s flip, drawn as a horizontal mirror), mini/upside-down
icon scaling, and a level-end sequence (before, a finished level just froze). The end camera is
an approximation; everything else here follows the binary.

## 2026-11-02 (late night): particles

Mark noticed the game felt empty without particles. Added a cocos2d particle system port that
runs the game's own effect files, and every effect a normal run shows: the death explosion with
its circle and the icon shattering into bouncing pieces (the icon is rendered to a texture, like
the game's CCRenderTexture), the circles when orbs, pads and portals fire, the particles orbs,
pads and portals carry, the ground drag, ship fire and sparks, and landing bursts. Effect
settings came from the decompiled code where readable and otherwise from a new trace-mod
command that dumps every live particle system in the real game (a test level with one of each
orb, pad and portal gave the whole color table). Physics still matches (47/48; the open one is
the slope work). Particles are random, so they match the game's look, not frame for frame.
Not yet: UFO click burst, speed portal screen effect, robot/spider/swing/wave effects.

## 2026-11-02 (night): dual, wave, levels 16–18

Mark asked for every main level, so levels come before menus now. Added dual mode (both players
traced from the real game; player 2 is written to its own trace file), the wave, legacy id
conversion (Blast Processing's old sawblades), spike slopes, the out-of-bounds rules, the rect
cache, and a long list of collision details found by routing each level in the simulation,
playing the routes in the real game and fixing the first difference each time
(`tools/iterate.sh <level> <suite>`). All 58 trace suites match on physics, including deep runs
through Hexagon Force (dual), Blast Processing (wave, dual wave) and Theory of Everything 2 (mini
wave, UFO); the routes reach the ends of 17 and 18. The ball head-bump rule is observed, not yet
derived from `collidedWithObjectInternal`, which still needs a full port.

## 2026-11-03: group triggers, Geometrical Dominator

Moving platforms are bit-exact. The first attempt at move triggers was off by a few ulps; the
decompiled `GroupCommandObject2` showed why: moves run every physics step (the 60 Hz look is
just when the game syncs the sprite), elapsed time is f32, and the offset is computed with a
fused multiply-subtract. Added move, toggle and spawn triggers (touch- and spawn-activated
ones too), all 18 easings ported from the instructions, objects in f64 like the game, the
moving-platform snap (the player rides a platform down), drawing of moved and hidden objects,
and exact object rects for scaled and rotated objects (every rect in levels 19 and 20 now
matches the game's own dump). Robot fixes: release ends the hold boost, pads don't on the
official levels, upside-down boost sign, rotation reset. Level 19 suites r2–r5 match bit for
bit, 52,000 steps deep to x 18,070. Teleport portals are in for level 20 (first real-game round
running). The route search now keeps a world per set of touched touch-triggers.

Later: **Geometrical Dominator (19) is verified start to finish.** A route planned in Cadence
completed the level in the real game with every one of its 25,144 steps matching bit for bit
(rotation within 1.1°, end camera approximate). Fixes on the way: robot rotation reset and the
trace's robot flag, a same-mode portal still re-centers the flying corridor
(`playerWillSwitchMode` runs for every portal), oriented boxes use the player's current size
(a mode portal earlier in the pass counts), and route-search fixes (seeding from an existing
route with `CADENCE_ROUTE_FROM`, a world per set of touched touch-triggers, tunable beam
diversity). Deadlocked (20): teleport portals verified; a full real run matches 5,343 steps,
then needs the moving-block collision rules of `collidedWithObjectInternal` (a moving ceiling
block pushes the player out even right after a pad) — porting that function next.

Also today: rotate triggers (verified against the real game on Fingerdash: every orbit position
and every object angle per step), the robot drawn from its own animation file
(`Robot_AnimDesc.plist`: parts per frame, far limbs at 0.7 color; frame timing approximate),
and the reverse-engineering notes for dash orbs, D/J/S/H blocks and the spider jump. A full
port of `collidedWithObjectInternal` (moving blocks) is in progress. Not yet drawn: pulse
triggers (1006) — Geometrical Dominator's platforms show the wrong color without them.

**Deadlocked (20) verified start to finish:** a route planned in Cadence completed it in the
real game, all 25,592 steps bit for bit including rotation (the camera after "smooth"
teleports is the one known gap). Needed: the moving-block collision port (agent), teleports
skipping the corridor bounds once, ceiling head-bump rules, disabled triggers not firing, and
the ball's roll-rate timing at speed portals. Also: pulse triggers drawn (color mode).

**Fingerdash (21) verified start to finish** (a completed real run, 22,328 steps, physics
bit for bit): dash orbs, black orbs, type-40 special blocks and the spider (the last two
ported by the agent). Rotation differs only after dashes (the game restores the cube's spin
from a wall-clock sprite timer). Also: "no touch" objects aren't collided, round hazards swap
their box on quarter turns, item/count/stop triggers. Suites: 72/82, all physics matching.

**The Challenge (3001) verified start to finish** (completed real run, 7,288 steps). Orbs
never get an oriented box in the game: a rotated orb is touched by its bounding rect.
Every level from 1 to 21 plus 3001 is now verified on physics against the real game.

The spider is drawn from `Spider_AnimDesc.plist` with the robot's rig code (run on the ground,
fall loop in the air; frame timing approximate). Dash (22) is blocked on the camera: when the
spider jump finds nothing it falls back to the playfield edge, which the game takes from the
animated ground and the zoomed camera (`getMinPortalY`). The agent is porting the ground
tween, zoom and camera triggers next.

Camera and ground port (agent): zoom (1913), static camera (1914 incl. exit), the dual-ground
slide, ground nodes through the camera, `getMinPortalY`/`getMaxPortalY` for spider limits and
the corridor edges, the kA32 gravity-bug fix, and the mid-pass box refresh. 14 new harness
columns (camera, zoom, ground, portal limits). Dash (22) now verifies to step 1744, where the
rotate-gameplay trigger (2900, sideways/going-left) begins — the agent's next task. The game app
now uses the player-owned camera. Not yet drawn: zoom in the renderer.

Rotate gameplay (2900, agent): sideways and going-left movement, the trigger channels
(key 170 waits for a channel, 173 switches it), the velocity edit (582/583), spin rules.
Triggers fire along the forward axis. Dash's s2-22 run (both rotations, 2,627 steps) matches
the real game bit for bit. Next: collisions while sideways.

Dash (22) continued (agent + me): ordered spawns, delayed spawns at the step start, spawn-once
for non-multi triggers, move-to-target, scaled touch areas, time warp with the game's frame
split, the dynamic level top (kA37). Real runs s6/s7-22 match fully; s8-22 matches to step 3321,
where the swing portal starts (next). Known app gap: the game app runs 4 steps per frame even
under time warp.

Swing mode, free-mode portals, next-step speed portals and more 2900 rules (agent). Dash runs
s2–s8-22 match fully; s9-22 matches to step 4888 (x 6914) in the second sideways section.

## 2026-11-03 night: Dash (22) verified start to finish

**Dash is verified start to finish**: a route planned in Cadence completed the level in the
real game (s19-22, 21,408 steps) and every step matches bit for bit. Every level 1–22 plus
3001 is now verified on physics. verify_all: 87/104 suites bit-exact, all 104 matching on
physics (the rest differ only in visual rotation).

What it took (agent), roughly in route order: passable blocks (key 134); spawn remaps (key
442); the moving-platform snap while sideways; the slope hit-head state; a port of
`updateCamera` from the decompile (static exits that ease out, smooth-velocity beziers,
follow, zoom keeping the center, the gameplay offset, camera offsets 1916, free-mode portals
animating the ground out, portal camera padding/easing) — the spider's jump limits and the
corridor edges come from it; teleport portals' gravity/static force and target groups; orbs
touched through their oriented box; a deterministic cocos scheduler in the harness so the dash
icon's spin (the rotation a dash ends with) is reproducible, modelled per rendered frame; ball
and swing rotation fixes; **order channels** — 2.2's position-trigger activation (per-channel
sorted lists walked before updateRotation; replaces "triggers pause while sideways"); grouped
camera triggers; move-lock multipliers; the accelerating ship; options and end triggers;
going-left slopes; custom orbs. The route planner now splits worlds by the triggers each route
fired, kills routes that stall for 10 s, and can be steered (`CADENCE_REQUIRE=<mode>:<x>`,
`CADENCE_MAX_Y_FROM`). Suites s10–s19-22.

## 2026-11-03 morning: time warp in the app, Tower survey

Re-ran verify_all myself: 87/104 bit-exact, all 104 on physics. The game app now paces steps
by the world's frame split (`World::real_step_secs`: 1/60 s over the frame's step count), so
time-warp sections play at the game's speed instead of a fixed 240 steps a second; unit test
`time_warp_runs_more_steps_per_frame`. Fixed `one_x_speed_per_step`, which stepped a derived
`World::default()` (time warp 0, no frame) without `pre_step`. `cadence features` now knows
the triggers ported since (camera, time warp, order, teleport, end) and lists shader/gradient/
sound triggers as look-only. The Towers (5001–5004) still need: platformer input in the
harness and planner, collectibles/keys (type 30), area triggers (type 45, 3006–3011), item
edit/compare/time triggers (3614–3620), advanced follow 3016, keyframes 3033, collision blocks
(1815/1817), events 3604, camera edges 2062.

Color channels ported from the decompile (`triggers.rs`): per-channel color actions stepped
once per frame (new `World::frame_end`), copy channels resolved parents first, exact
`transformColor`/HSV conversions, HSV pulses, object HSV, channel opacity and alpha triggers
(1007), and blending (additive parts in each z layer's blend pass under the layer). Level 19
frames look right along r8-19; not yet compared pixel by pixel with the real game (the harness
is the Towers agent's). The 2026-11-03 Stereo Madness background check still matches.

## 2026-11-03 (Tower agent): platformer mode, The Tower to x 3,900

Platformer input in the harness spec (`press <step> <button>`, 2 left / 3 right — the mod already
passed buttons through), `Spec::buttons`, `Player::set_direction`. Ported from the binary:
`updateMove` (platformer.rs), the platformer branches of updateJump (mirrored 2g, 0.05 s ground
grace), `runRotateAction` (no spin unless a direction is held), the air rotation easing, the
spawn group start (kA36), platformer block contacts (snap 5, narrowed rect, side test, no inner
kill), time-based position triggers (`posForTime(levelTime)`), spawn chains passing remaps and
overshoot on, area move triggers (3006, area.rs), 2-D section order for moving solids. New
`cadence proute <level> <waypoints> <out>`: a beam search over left/right/jump steered by
waypoints. `tools/levelmap.py`: PNG map of a level's rects plus traces.

Real runs, all bit for bit (positions, velocities, flags, rotation): pf1 (747 steps), t1-5001
(2,304), t2-5001 (3,388: crushers, orbs, the climb), t3-5001 (3,635: the crumbling bridge).
verify_all: 92/109 bit-exact, all 109 on physics (87/104 before; the 17 rotation-only suites are
unchanged). Harness: `watchmax N` for `watchgroup`; `dumpsprites` writes `color_z_related`.

Later the same day: rotate triggers don't turn solids (`canRotateFree`, kA41), platformer
moving-platform carry, non-stick x/y and extra-sticky blocks, the snap's skip-check, spawned end
triggers. New real runs t4-5001 (4,644 steps), t5 (5,510), t6 (6,068: the slab wheel and the
climb to the upper floor), t7 (7,256: to x 8,061, just before the pressure plate) — all bit for
bit. The Tower is verified to x 8,061 of ~10,150. verify_all: 96/113 bit-exact, all 113 on
physics. Next for 5001: the pressure-plate cutscene (2899 disable/enable controls in platformer,
1932 player control, the door group 200 opened by spawn 202 remapped), then the door touch at
(10151, 681) and the 10 s wait for the end trigger. See NOTES "Platformer" and the handoff in the
agent report.

App (main session): platformer controls (←/→ or A/D → `Player::set_direction`, re-sent on
restart), spawned end triggers end the level, screenshots replay every spec button, and
m_colorZLayerRelated per id (`color_z.rs`, from `gen_sprite_table.py --color-z`). Open: The
Tower renders large white areas below the floor: object 94 (block009c_base, white, channel
1004) with key 155 = 5 on all 2,690 copies. Key 155's meaning is in `GameObject::objectFromVector`
(0x5b1424, decompiled to the session scratchpad); the platformer camera (y 94.5 vs 0) also shows
more of it than the game does.

## 2026-11-03 (Tower agent 2): stopped at once on a usage limit; 5001 cutscene survey only

No code changed. verify_all re-run at the start: 96/113 bit-exact, all 113 on physics (same as
before). Facts found for the next agent (level-raw y values are +90 in world space; `cadence
rects` y is world space; `cadence rects` "order" = level index):
- Pressure plate = object 6469 (id 215, solid, group 194, world rect 8193..8223 x 679.5..693.5).
  Touch spawn trigger 6451 (1268 at world (8209, 679), 441 ordered) spawns **group 192**: the
  cutscene. In it (ordered by x from 7785): 2899 obj 6477 (x 7785, 165=1 199=1: disable P1/P2
  controls), 1932 obj 6478 (x 7785, 138=1 200=1 58=1 59=1: player control, stop jump/move;
  `activatePlayerControlTrigger` 0xe5748, not ported), 901 obj 6471 (x 8085: group 194 = the
  plate moves 0,−6 over 2 s — a solid, physics!), 1268 obj 6561 (x 13335, spawns 202) and 6562
  (x 13425, spawns 202 with remap 199→200), 2899 obj 7322 (x 13605, 165=−1 199=−1: enable) —
  (13605−7785)/311.58 = 18.68 s. Everything else in 192 is visual (3602/1006/899/1520/2910/
  2923/3033 keyframes on non-solid groups 195/197/193/196/198/201/203/204/206/210 — checked:
  none of those groups has a collidable object).
- Group 202 (ordered): 901s at x 13245..13335 shake target ±1 x, 901 at 13365 moves the target
  0,−90 over 8 s. Only the remapped spawn matters for physics: group 200 has one solid, obj 7492
  (id 1, world rect 8280..8310 x 692.89..761.11) = the door. Group 199 has no solids.
- After the door: flat floor (top ≈ 679) to the end. Pillars of solids at x ≈ 9940 and ≈ 10210
  span y ≈ 90..840 (cross the floor) — the path to the end plate (touch 1268 obj 8587 at world
  (10151, 681), spawns 235 → 3600 at x 13215, 1612 at 12375) must get past the 9940 pillar
  somehow: items/keys section at 9255..9787 (1615 counters 8883/8886/8890, 3620 item edits
  8906/8909/8912, 3619 at (8611, 725) touch, 3617, 3614 time, 1817 at −75, collectibles
  142 type 22 at (9389/9479/9569, 901), touch spawns 8900/8904/9352 at 9345/9525/9435 with
  remaps, 1268 9367/9369 at 9787/9697, rotate 1346 obj 8811 at (9787, 717) touch: target 283
  around 240, 180°, keys 397/402/403 = aim/dynamic mode?). Unknown which of these moves the
  pillar; find the pillars' groups first (python over level-raw + rects as below).
- Suggested next step: record a real run in the real game first (extend
  re/traces/t7-5001/route.spec: hold right into the plate, wait ~19 s, keep right through the
  door, `watchgroup 200`, `watchgroup 194`, `triggers <file>`), so the real cutscene behaviour
  (does 2899 in platformer release the direction buttons; does re-enable re-press held ones;
  does 1932 zero the x velocity) is visible in the trace before porting from 0xe5748 and
  processOptionsTrigger. Then the spawn path must run 2899/1932 (currently only the order walk
  handles 2899, order.rs:108 → physics.rs ~2559).
- Side request (fired_effects: Vec<u32> on World for spawn/touch-fired 899/1006/1007): NOT done.
- Scratch helper (not in the project): python parse of `cadence level-raw` output — split on
  ';', each object `k,v,k,v` pairs, key 57 groups '.'-separated, 62 spawn-triggered,
  11 touch-triggered, 441 ordered spawn, 442 remaps.

## 2026-11-03: missing visuals (Mark's playtest)

Mark found levels visually bare. Fixed in the app: ~1,500 editor-only icons were drawn in play
(objects whose sprite is an `edit_` button: particle emitters 2065/1700/1586, text 914,
collision/force/damage blocks, area triggers, counters) and are now hidden; custom particle
objects (2065, key 145) emit their particles (`ParticleDef::from_gd_string`, field types from
`particleStringToStruct` 0x44fd80 / `FUN_100450ca4`; GJ_ParticleSheet is now loaded); secret
coins (142/1329) draw (4 frames, 0.05 s) and are taken on touch; 82 object ids spin
(`spin.rs` from the sprite dump's +0x578; keys 97/98); objects with an audio scale pulse with
a port of `FMODAudioEngine::updateMetering` fed by a 1024-frame peak meter on the music
(rings min(v + 0.3, 1.2), custom ranges min + (max − min)(v − 0.1)). Sprite dump gained
spin/audio columns (harness). Not yet: text objects, counters, presets 1700/1586, coin
pickup animation and sound, the ship feel.

Next (from a real-game screenshot of Theory of Everything at step 900,
scratchpad real12/real-900.png vs ours shots/12-900b.png): the wavy slabs (object 61) are
black with a white top edge in the game, white in ours, so object 61's colors are wrong; the
yellow face balls (pulsing decorations) show in the game (ours scale them by the meter, which
is ~0 in silent screenshot runs, so check with audio); faint vertical light beams rise from
the ground in the game and are missing in ours. Also: levels 17/18 use old color key 19
(ignored); preset particle objects 1700/1586; text objects; ship feel.

## 2026-11-03 evening: one-to-one visual comparison (Mark away; autonomous)

`tools/compare_level.py <level> <route.spec> <steps>` screenshots the real game (isolated
copy) and Cadence at the same steps into shots/<level>/ (real on top). Found and fixed:
m_isObjectBlack objects keep a black main sprite (black.rs; ToE slabs, black blocks/saws,
Tower fill); edge fade (updateVisibility 0x7ae88: opacity int(clamp(d/70)·255) from the far
edge, key 64 exempt); enter effects (applyEnterEffect 0x7c570, codes from
updateActiveEnterEffect 0xd918c: 22 none, 23 bottom, 24 top, 25 left, 26 right, 27 scale in,
28 scale down, 55 random angle, 56/57 diagonals, 58/59 nearest/far vertical edge; t from the
object's center / 70; key 67 exempt); glitter (m_glitterParticles, toggleGlitter 0x7d860, on
in ship/UFO/wave/swing, centered on the view, settings from a real particle dump). Found
missing, in progress: player trails — the regular CCMotionStreak (streak_01, stroke 10,
fade 0.3, P2 color; activateStreak 0x36d614 after pads/orbs), the ship streak and the wave
trail (HardStreak draw nodes, updateStroke 0x5d1dc, quadCornerOffset 0x5dfcc). A GUI agent
is building menus in crates/cadence-game/src/ui (see its report).

## 2026-11-03 (GUI agent): menus, pause, level complete, progress save

New `crates/cadence-game/src/ui/` (mod, canvas, font, menu, levels, pause, end, save), laid
out from the decompile (a copy of re/ghidra in the agent's scratchpad; NOTES "GUI" has every
address and number): the main menu over MenuGameLayer (scrolling background and ground, the
5 s color cycle, a random-colored cube running the real physics on an empty level and jumping
like tryJump), the level select (24 looping pages with elastic paging, per-page gradient and
ground colors, level boxes with face/name/stars/coins/orbs, both progress bars, locks for
Clubstep/ToE2/Deadlocked with the "Locked" alert, the Tower door, the hidden Coming Soon door),
the pause menu (bars, buttons, sliders, coins) and EndLevelLayer (drop-in with bounce, stats,
coin entrance, replay/menu) after the "Level Complete!" text. BMFont labels (bigFont, goldFont,
chatFont) laid out like CCLabelBMFont; CCScale9Sprite, cropped bars, CCMenuItemSpriteExtra press
animation; GJ_LaunchSheet and single images (gradient falls back to -hd) loaded by
`ui::load_assets`. App changes are confined to the App struct, setup, event dispatch and three
hooks in `App::draw` (no tick while paused, end-time hold under the end screen, the overlay);
level id 0 (or no argument) starts at the menu. Audio gained `set_paused` and `play_loop`
(menu music; web: AudioContext suspend/resume, looping buffer). Progress saves to Cadence's
own app-support file / localStorage. Headless: `CADENCE_SCREEN=menu|levels|locked|pause|end`.

Stubbed: icon kit, creator, options, stats, achievements, news, daily, profile, socials,
practice mode (button inert), the pause menu's option toggles and volume sliders (drawn at
full), level-info/death-effect buttons, The Tower's inner layer (the door starts 5001),
SecretLayer4 (the hidden door starts 3001 after it is revealed), particles/circles in the end
screen, the menu player's other vehicles. Needs a real-game check: screenshots of the main
menu, level select page 1, the pause menu and the end screen at 1136×640 to compare.

## 2026-11-03 night (Mark away; "fix everything")

Fixed: sheets PixelSheet_01/FireSheet_01 were never loaded (Dash/Tower art); hidden objects
(key 135) were drawn; child sprites of m_isColorSpriteBlack objects stay black (block fills);
grounds follow the camera's ground nodes (free mode hides them); middle ground (fitted);
spawned/touched color, pulse and alpha triggers (Action::Effect); benchmarks no longer pause
on focus loss (the GUI's focus pause also paused Mark's game whenever the harness's test copy
of GD stole focus — never run the harness while he plays). Physics 126/126 suites.
Open (from shots/21, shots/22): animated objects (AnimatedGameObject / CCAnimatedSprite /
CCPartAnimSprite with *_AnimDesc.plist, e.g. 919 black sludge, gj22_anim_*) draw only their
static first-frame parts untinted; d_gradient decorations / Dash sky colors; 1700/1586 preset
particles; MG edit triggers 3020/3021.
Added: text objects (914, bigFont), the attempt label ("Attempt N", start + half a view + 15,
+140, z 1400). Comparison notes: the harness runs 8 game updates per rendered frame while the
music plays in real time, so headless screenshots now advance the song at 1/speed (music-pulse
sizes still differ: the real meter often sits at its 0.1 floor in harness runs). Open: Dash
shaders (2904–2923, chromatic etc.), blackCogwheel 1734–1736 children swap to `_color_` frames
in play (the dump caught the creation frames), Electrodynamix color pattern, level fonts kA18.

## 2026-11-03 night (animated-objects agent): animated level objects

New `crates/cadence-game/src/animobj.rs` + generated `animobj_table.rs`
(`tools/gen_anim_table.py`, which emulates the GameManager animation-table builder
FUN_1001f2b60 over the disassembly). 125 frame-animated ids (fire, waves, lava, waterfall,
drops/smoke/bubbles, gj22_anim monsters, pixel art...) now animate with the game's frame
times, per-id sprite layout (color child at scale 2, ping-pong waves with flips, lava's random
cycles, freeze-and-hide), random speeds/offsets like customSetup and the level keys 106/107/
122/123/126/462/592; the AnimatedGameObjects (918/1327/1328/1584/2012 beasts, 919 sludge) play
their AnimDesc part animations with main/secondary colors and glows tinted like
updateChildSpriteColor. Black cogwheels 1734–1736 get their in-play tree (mirrored color
quarters). App changes: a field on Game, its construction, a two-line hook in the draw loop
(`AnimObjects::sprites` → `View`) and one line tinting glow parts. Checked against real frames
in shots/19–22: d_animWave frames match the dumps (with a fitted 0.075 s layer-time lead), lava,
waterfall and GJBeast02 look like the real ones; most decorations have random phases in the
real game, so they can't match frame for frame. Details and what's left in NOTES "Animated
objects".

## 2026-11-03 night (shader agent): shader and gradient triggers

Ported the 2.2 shader triggers (2904–2924) and gradient triggers (2903) for Dash. The
ShaderLayer fragment shader isn't a Resources file: it's embedded in the binary (0x91ab52) and
now lives as `crates/cadence-game/src/postfx.wgsl`; the trigger → uniform math is in
`crates/cadence-core/src/shaders.rs` (parse, `GJValueTween` tweens stepped per frame, the
`pre*Shader`/visit uniforms) and gradients in `crates/cadence-core/src/gradients.rs` (unit tests
for both). The renderer gained `render_frame` (draws before the layer, the layer into a texture,
the post pass, the rest) and four gradient pipelines (`gradient.wgsl`, colors per corner).
`crates/cadence-game/src/fx.rs` records where each game z layer starts in the draw list,
splices gradients in and picks the shader range. Hooks in app.rs: a `fx` field, its
construction, reset, `fire`/`frame` next to the color triggers (tick and the screenshot loop),
and in `draw` the layer marks + `render_frame`. Checked against the real frames in shots/22:
step 312 (shockline wave on the pillar, chromatic split on the attempt text and icon, the
additive UI gradient) went from 77.5/46.6/9.5 to 8.6/40.8/9.6 mean abs diff; step 4480's
gradients (B3 + B2, pink sky) match the scene dump (37.7/21.6/21.3 → 17.3/20.5/17.2); 8640
unchanged within noise (its gradients are off-screen in the real dump too). Later Dash frames
(12800+) can't be compared: Cadence's route run dies near x 13190 (pre-existing). Details in
NOTES "Shader and gradient triggers".

## 2026-11-03 evening: menus 1:1, practice, icon kit, full-frame captures, color sprites

- Online levels (search/download/custom songs, Online screen) work natively; put on hold by Mark:
  base game first. Web build compiles again (online stubs).
- Practice mode: auto checkpoints per `tryPlaceCheckpoint` (pending snapshot committed after
  surviving 500 more steps), Z/X, practice music, practice best saved.
- Pause: practice toggle, draggable volume sliders (saved, applied natively and on the web).
- Settings dropdown, the 7 option pages (56 toggles under their game-variable keys; faster
  reset, hide attempts, auto/quick checkpoints, menu music applied), Stats (game's rows/titles),
  Achievements (all 549 from the real AchievementManager via `dumpachievements`, unlocks from
  the save), Creator layer, icon kit (all icons; choice drives the level's player), menu links.
- Real-game captures were missing the notch's 33 rows: the harness save now runs the test copy
  windowed at 1280×720 (0025 = 1, resolution 8); trace mod crops blank rows and records `crop N`;
  `tools/recompare.py` redoes old comparisons. GUI comparisons in shots/gui-cmp/.
- Screenshot loop now sets `world.sched_every` like `cadence verify`: it used to die in Dash at
  step 9499 (and drift elsewhere) because timed triggers ran on another schedule.
- Color sprites the synthetic dump missed (rock/brick `_color` layers etc.) are mined from real
  scene dumps (`tools/gen_color_children.py` → color_children.rs) and added per object signature;
  the color sprite takes main opacity × its channel's (`GameObject::setOpacity`).
- `tools/sprite_diff.py` + `CADENCE_DUMP_DRAWS`: per-frame sprite counts, real vs ours.
- Open: Fingerdash 17688 shows gradients where the real game has moved rock blocks (move
  triggers?); glow frame choice for multi-part objects (15new); grayscale sprites; level select
  real capture (scene change blocked in the harness); editor not started.

## 2026-11-03 late: Mark's Dash report, automated checks

- Dash white-out at the spider part: the invert-color shader turned the shader layer's
  transparent pixels (premultiplied) white. Color effects now run on straight color
  (postfx.wgsl) — only the layer's sprites invert.
- Glows took only the edge fade: invisible spikes left white outlines. Glow alpha = object alpha.
- Green/black orbs spun whole: only their child copy and glow turn (real RingObjects never do).
- Rotation vs real traces: airborne cube spin resumes after a dash on the first falling step;
  robot/spider ease back to 0° off slopes (updateRotation 0x3753e8 / 0x36ccac). 111/126 suites
  bit-exact (positions: all 126 exact); Fingerdash rotation error 10–23° → <0.01°.
- Scene fades (CCTransitionFade 0.5 s) between menus and levels.
- Automated checks: `tools/verify_all.sh` (movement), `tools/scan.py` (every level rendered
  every 200 steps via CADENCE_SHOTS multi-shot mode; flags white-outs, black-outs, flashes,
  frozen frames; all 23 levels clean except expected end-portal holds), `tools/recompare.py` and
  `tools/sprite_diff.py` (against real frames). Real frames are still sparse (6–16 per level):
  denser real captures need the test copy (windows pop up).

## 2026-11-03 night (Mark away): icon kit, menus, dense references, game-wide sprite fixes

- Icon kit: per-type grid scales, cube preview, rig-assembled robots/spiders, the real color page
  (slot table + offsets from the binary), all compared on full real frames (shots/gui-cmp/).
- Real references for every menu: menu, settings, options, stats, achievements, creator, level
  select (LevelSelectLayer::create by address, pushScene), icon kit tabs 1–8, color page
  (onPaint by address), pause (`pausegui`), end screen (`endgui`); each with a scene dump.
- Pause: the five side buttons (death / markers / quests / options / hide), hide button; options
  pages drawn over the pause menu. End screen: first-completion rewards, hide eye, chains behind.
- Orbs: base currency × 1.25 (Stereo Madness 50).
- Dense real level frames (22–37 per level; Dark Paradise 6) via COMPARE_REAL_ONLY captures.
- Sprite audit across all levels → fixed: glow frames from the real objects (dumped glow sprites),
  ground spike variants, color sprites at z −100, the end portal (wall, glow bar, particles).
- Headless multi-shot runs no longer pause on focus loss (that put the pause menu into frames).

## 2026-11-04 early: Mark's Dash report — spider, "spinning", animation

- **Spider spun / lay on its side**: two rotation bugs, both visible in Dash's spider part.
  `toggleSpiderMode`/`toggleRobotMode` leave the other modes first and set rotation 0 *last*;
  we reset before leaving the swing, so an upside-down swing → spider kept 180°. And
  `convertToClosestRotation` returns 0 for robot/spider (and any mode while dashing): they
  settle upright, we eased to the nearest 90° (after a dash the spider ended at 90°). Dash route
  s19-22 rotation: max 90° → 1.96°, mean 3.55° → 0.005°. Positions still 126/126.
- **Spider "no animation"**: the teleport effect was never ported. `playSpiderDashEffect`
  0x376a20 now: shrinking circle at the start, filled 0.42-opacity circle + 2-pt ring at the
  end (`CCCircleWave` outline +0x160 / opacity mod +0x168 added), the 8-frame additive
  `spiderDash` streak, the spider's 0.4 → 1 elastic pop; secondary color unless option 0061.
- **Rig animation**: delays from `objectDefinitions.plist` (robot run 0.0275, spider run 0.04,
  walk 0.03, loops 0.05); `playDynamicSpiderRun` (walk up to speed 0.9, run above);
  `updateRobotAnimationSpeed` factors; spider `jump` pose after an orb until it falls. The
  screenshot loop never advanced the rig (frozen on frame 0 in every test) — fixed.
- `cadence verify` prints the first rotation divergence; `CADENCE_ROT_LOG=1` lists every step
  more than 5° off. Draw dumps now carry rotation (5th column) and a `# player` state line.
- Open: s17-22 (stuck sideways ship run) rotation; Dash real frame "4823" was captured late
  (player elsewhere though trace + camera match) — that reference is unusable.

## 2026-11-04 night: Settings 1:1 (Mark asleep, autonomous)

- **Why Settings looked wrong:** in this build Settings > Options opens `GameSettingsLayer`
  (0x543c5c), a tabbed popup (Gameplay, Visual, Practice, Editor, Audio, Other + Keys), not the
  old paged `MoreOptionsLayer` we had copied. It is now rebuilt from `buildTabContent` and the
  real frames (harness `gui gs<tab>p<page>`). Mean abs diff vs real: 4.7–8.0 per tab.
- New harness gui names: `gs<t>p<p>`, `keys`, `videoadv`, `opt<Account|Help|Video|Parental|
  Support|Songs|Vault>` (the OptionsLayer button handlers). removeNotices now only drops the
  plain FLAlertLayer, and skips the second pass for `opt*`.
- Loose UI textures (GJ_square01–07, GJ_button_01–06, square02_small, ..., tutorial_01–05)
  are loaded as single images; GJ_square06/07 and the tab buttons were simply missing.
- Account (greyed Log in / Register), Support (links, logos, low detail), Soundtrack list
  (23 songs, rows 60), How To Play (5 tutorial pages with tagged text), Video Options,
  Parental Control: all new, compared with real frames (diff 10–30, mostly the menu's
  background color behind them).
- FLAlertLayer rebuilt from `FLAlertLayer::init` (0x3fc534): height = text + 120 (≥ 140), chatFont
  TextArea with `<cX>` color tags (`canvas::rich_lines` / `draw_rich`), modal.
- Tools: `tools/scene_brief.py` (compact scene dumps; menu items print relative to the screen
  center), `tools/gui_pair.py` (real over ours + mean diff), `tools/montage.py`.
- Note: macOS has no `timeout`; headless screenshot runs exit by themselves.

## 2026-11-04 night: online screens 1:1

- Harness: `creator:N[/M/K...]` presses the Creator menu's Nth button (depth first), then
  more buttons on the top layer every 150 frames, logging each item's world position (Geode
  log in re/gd-copy/.../geode/logs). A `GJSearchObject::create` hook logs search types:
  quick searches are Downloads 1, Likes 2, **Hyped 31**, Trending 3, Recent 4, Magic 7,
  Awarded 11, Followed 12, Friends 13. Real captures in shots/gui-real/creator/ (c0..c19 =
  every creator button, levelinfo, search, qs3..qs11).
- `ui/online.rs` rewritten: LevelSearchLayer (quick search, difficulty/length/star filters,
  search box), LevelBrowserLayer (LevelCell rows 90 high, "1 to 10 of N", page arrows,
  Featured's label image, Hyped title), LevelInfoLayer (play, stats with thousands
  separators, progress bars, side buttons, song widget), DailyLevelPage (daily/weekly/event,
  GJ_square05 gray / tinted purple for events). Mean diff vs real: search 5.9, featured 10.4,
  level page 17.7, weekly 22, event 32.
- online.rs: `Search` (type, query, diff, len, page, star), `Page` (total/offset from the
  response's 4th section), more LevelInfo fields (featured, epic tier, verified coins,
  objects, song artist/size, songs/sfx counts), `daily()` (getGJDailyLevel can answer −1 —
  the real game's daily page also spins forever then — but downloading −1/−2/−3 still works;
  the download response carries "playerID:name:accountID" and the song's fields).
- Creator buttons: Create → My Levels (empty, "Tap New..."), Saved → Saved Levels (empty),
  Versus / The Map → their notices, Daily/Weekly/Event → DailyLevelPage, Featured → list,
  Search → search screen. Still placeholders: Scores, Quests, Gauntlets, Lists, Paths,
  Map Packs, the vault and treasure room.

## 2026-11-04 ~03:00: the rest of the creator and main menu

- Creator: Map Packs (getGJMapPacks21, MapPackCell 100 high, View → the pack's levels by
  type 10), Gauntlets (getGJGauntlets21 special=1; names/islands/colors decoded from the
  binary's tables at 0x100a60d00 / 0x100a60ef0 and GauntletNode::generateNode's switch; 3 a
  page), Scores (getGJScores20 top/relative/creators, GJScoreCell rows 60, player icons via
  wanted_sheets), Lists (getGJLevelLists type 6, topListsLabel), Quests (getGJChallenges:
  XOR "19847" + base64 behind 5 salt chars; three ChallengeNodes), Paths of Power (local,
  10 locked paths on pathSelectBG), Versus / vault / treasure door → DialogLayer with the
  Keymaster (dialogIcon_002; his 11 vault lines and the Versus conversation from the binary;
  delay tags <d040> etc. stripped), The Map → mapPromo.png.
- Main menu: Quit and Newgrounds ask first (FLAlertLayer with two ButtonSprites, gap ≤ 15),
  Daily chests → RewardsPage (claiming would write to the servers: explained, not done),
  News → NewsPopup from geometrydashfiles.b-cdn.net/news/news.dat (base64+gzip,
  "1:id;2:date;3:title;4:blocks"; pictures are JPEGs: placeholders until a decoder is in).
- Mean diffs vs real: packs 11.8, gauntlets 8.9, scores 20.3, lists 17.7, quests 8.3, paths
  8.5, rewards 16.0, news 41 (pictures missing).
- Real main-menu captures: shots/gui-real/menu/m0..m21 (`gui menu:N`); the pentagon button
  in real captures is Geode's mod list.

## 2026-11-04 ~03:30: level editor v1, scripted UI tests

- **Editor** (`ui/editor.rs`, Screen::Editor): the real EditorUI layout (measured from a real
  editor frame: shots/gui-real/editor/editor.png; menus' printed coords + (569, 320) are
  absolute), all 13 build tabs with every object id in the real order (`ui/editor_tabs.rs`,
  dumped from the real editor's m_createButtonBars via harness `creator:0/4/0/dumpbuild`),
  Build/Edit/Delete/View, edit buttons (move 1 block / 2 units, flip, rotate), undo/redo,
  trash, copy/paste/duplicate, zoom, swipe/drag panning, arrow keys, playtest (plays the level
  as an online-style level and returns to the editor), EditorPauseLayer (resume, save and
  play, save and exit, save, exit). Level y is from the ground's top (world y 90).
- The level is drawn by the game's own renderer: `Game.editor` (no stepping, no player, no
  end portal or attempt text, triggers and editor-only objects shown, camera from the editor,
  ground at world 90). Any change rebuilds the Game from `Editor::level_string()`.
- Local levels: saved in Cadence's own save as `set mylevel<N> <base64 level string>`;
  My Levels lists them (Edit) with New and the real side buttons (server/import/backup
  explained, folders/lists "coming").
- **Scripted UI tests**: `CADENCE_UI_SCRIPT=<file>` runs headless and replays `click X Y`
  (points), `key Name`, `wheel D`, `text ...`, `wait N` (frames), `shot PATH`. Used for
  menu → creator → My Levels → New → place → edit → playtest, and save → exit → reopen.
  Found and fixed: New button hidden under the side art, refresh button firing a search from
  a local list, editor ground drawn off-screen, end portal / attempt text in the editor.
- Not yet: editor music, object settings popups (triggers' Edit Object), groups, layers,
  colors, level settings, swipe selection, the EditLevelLayer page (name/description).
- Replayed screens (`ui/replay.rs` + `tools/scene_to_layout.py`): the harness scene dump now
  records label text/font, loose-texture file names (`tex:`) and sprite flips (`#flip`), and
  dumps deeper trees. A layout is one line per sprite / 9-slice / label / color layer /
  button in screen points; Cadence draws it with the player's own textures. Used for the
  Tower's inside (doors open as 5001–5003 are beaten), the five shops, the four vault layers
  and the treasure room. Mean diff vs real: tower 0.7, vault 0.7, treasure 1.6, shop 2.9.
- Sweeps with UI scripts across every main-menu, settings and creator button found: Escape
  on a DialogLayer advanced it instead of closing (fixed: Escape closes, Space/Enter go on);
  the loading circle texture was never loaded. The full online flow works end to end:
  search "bloodbath" → results → level page → download → play → pause.
- Level page popups from real layouts (harness `creator:10/0/<button>`; the dump now marks
  `#ignoreanchor` so popups' inner layers transform right): InfoLayer (title, creator,
  description, version, ID, Load Comments → getGJComments21 rows), LevelLeaderboard ("create
  an account"), Vote (explained: rating writes), SongInfoLayer, AudioAssetsBrowser and the
  creator's ProfilePage (getGJUserInfo20 + getGJAccountComments20: name, rank, the seven stats,
  the nine icons in their colors via wanted_sheets, the latest post). Unregistered creators
  are green and have no profile (LevelInfoLayer). Escape on online screens goes back one step.
- `scan.py` over every level route: no route dies; 16 flagged frames (dark sections and
  flashes in 3–6, 18, 19, 21, 22), to be checked against the real frames in `recompare.py`.

## 2026-11-04 ~04:20: triggers online levels use

- Audit (`scratchpad/olv/audit.py`): trigger ids across 45 featured / awarded / trending
  levels, against the engine. Unhandled and common: scale 2067 (34 levels), area fade/tint
  3009/3010 (31/24), UI 3613 (31), hide/show player 1612/1613 (26/24), shake 1520 (29), gravity
  2066 (22), advanced follow 3016 (25), keyframes 3033, collision 1815/1816, pickup 1817, item
  compare 3620, on death 1812, song 1934, SFX 3602/3603, camera rotate/edge 2015/2062, ...
- **Rotate triggers' turns were never drawn** (only the hitboxes turned): `World::turned` now
  carries each object's turned degrees to the renderer. Levels 21, 22 and 5001 use 1346.
- Scale trigger 2067 (`TransformTriggerGameObject`: 51 target, 71 center, 150/151 scale,
  153/154 divide, 133 only move, 10/30/85 duration/easing/rate): a MoveCommand on (scale − 1)
  whose per-step factor is (1 + reached)/(1 + reached before); positions scale about the center
  group's main object, objects and their hitboxes scale with them (`World::scaled`).
- Gravity 2066 (key 148 → PlayerObject m_gravityMod 0xb24; updateJump uses 0.958199·mod
  when mod ≠ 1, asm 0x100368f70), pickup 1817 (80 item, 77 count, 139 set; re-checks count
  triggers), advanced random 2068 (152 "group.weight…", xorshift pick), hide/show player
  1612/1613, shake 1520 (75 strength, 84 interval, 10 duration; a random view offset).
- verify_all still 111/126 bit-exact (all positions match).
- Area Fade / Rotate / Scale (3009 / 3007 / 3008): they share Area Move's distance falloff and easing (`area.rs`), but instead of moving objects they set per-step visual state: `AreaState.opacity / turn / scale`, cleared at the start of each step. The renderer multiplies alpha by the opacity, adds the turn to the rotation and multiplies the scale. Fade opacity is `to + v·(from − to)` with v the falloff weight. verify_all still 111/126. Area Tint (3010) still missing (needs color blending against the channel).
- Follow Player Y (1814, `processPlayerFollowActions`): each object of group 51 moves a min(1, speed·dt·60) share of the gap to player y (delay seconds ago, from a step history like `getOldPosition`'s 0.01 s ring) + offset, capped at max speed·dt·60. Keys 90 speed, 91 delay, 92 offset, 105 max speed, 10 duration. Test added; verify_all 111/126.
- Area Tint (3010): keys 260 channel, 265 amount, 65 main only, 66 detail only (`processAreaTintGroupAction`, `customObjectSetup` key table). The object's color moves toward the channel's active color by tint·(1 − eased v); stored per object in `AreaState.tint`, applied in the renderer after pulses/HSV. HSV mode (278) and the parent-group branch aren't ported. verify_all 111/126.
- Collision blocks / triggers: 1816 blocks (80 id, 94 dynamic, 30×30 scaled) and the player as block −1 (P2 −2). Collision (1815: 80/95 blocks, 138/200/201 players, 51 target, 56 activate, 93 on exit) arms once, then toggles its group when the pair starts (or stops) touching; touching pairs are each dynamic block against every block with another id (`checkCollisionBlocks`). Instant Collision (3609) spawns 51 or 71 now. Test added.
- Item Compare (3620) / Item Edit (3619) from `activateItemCompareTrigger` / `activateItemEditTrigger` and the `ItemTriggerGameObject` property order (476/477 kinds, 478 target kind, 479/484 mods, 480/481/482 ops, 483 tolerance, 485/486 rounding, 578/579 sign). Compare: a + tol > b etc. exactly as the binary; timers/points read 0. Test added. verify_all 111/126.
- Recompare finished (635 frames). Per-level means 1.6 (level 1) … 22.8 (level 22). Worst: level 22 step 4832 at 65.9 — our player (6859.19, 580.41) and camera (6501.03, 480.26) match the real frame's .txt exactly, but the real picture shows another moment: a harness capture-timing artifact, not a Cadence bug.
- Teleport trigger (3022): the world records the target group's main position (352/353 keep x/y, 354 gravity) and the next player step applies it like a teleport portal with a target (touching ground cleared, m_wasTeleported set).
- On Death (1812): armed once, toggles 51 (56 activate) when the player dies (`World::player_died`, called by the app before its death handling).
- Player Control (1932, `activatePlayerControlTrigger`): 540 stop jump releases the button, 541 stop move releases platformer directions, 542 stop rotation clears the spin (`stopRotation`). Applied at the start of the next player step. Stop slide (543) not ported. verify_all 111/126.
- Camera Rotate (2015): keys 68 degrees, 70 add, 394 snap 360 (key choice from the editor's property names; not checked against a capture). `Camera.angle` tweens like the zoom; the renderer turns every world draw (not the background) clockwise about the view center. Test added.
- The vaults' codes (`SecretLayer::onSubmit`, `SecretLayer2::onSubmit`, `SecretLayer4::onSubmit`): the input is lower-cased with spaces removed; the guard (button 1) or Enter submits. Codes are packed little-endian constants in the decompile (0x796e6e656c = "lenny"); the replies are the strings that follow each code in the binary's cstring table (`strings` order: code, reply, achievement id). Vault 1: lenny, sparky (secret coin "secret06" → counted in secret coins), spooky, blockbite, robotop, ahead, mule, neverending, gandalfpotter, the player's name, finalboss, and the doubled Lost numbers 8 16 30 32 46 84 one per submit (orange from the 4th, secret16 at the end). Vault of Secrets: brainpower, glubfub (coin secretB03), octocube, seven, gimmiethecolor, thechickenisonfire, d4shg30me7ry, thechickenisready, thechallenge, unicorn, rubrubpowah123, battop, kappa, robtop. Chamber of Time: hunger, (a)volcano, backontrack, givemehelper. Achievements are kept as save flags (`achievements::unlocked` checks them). Not ported: the guard's own idle chatter (built from shared string fragments in registers; to pull with the harness), vault 2's diamond/shard number guesses, the chamber's first riddles, showing The Challenge after "thechallenge".
- Vault input label: the capture put "..." at (194.5, 185), off by half the 180×50 CCTextInputNode (its anchor ignored); moved to the box center (284.5, 210) in all three layouts.
- Options' lock opens the Vault with 10+ user coins (stat 12, `OptionsLayer::onSecretVault`); otherwise the Keymaster talks. The creator's lock (stat 13: 50 diamonds) and the chamber's door (UGV 4) stay closed: Cadence doesn't award diamonds yet.
- Editor keeps every object key and the level header: `EdObject.extra` holds all keys besides 1/2/3/4/5/6 and writes them back; `Editor.header` keeps the loaded header. A level string now round-trips byte for byte (test). Before, loading a level and saving dropped groups, colors and all trigger settings.
- Edit Group popup (`SetGroupIDLayer`), from a real capture (harness `creator:0/4/0/edit:group:1`: in the editor, place object 1, select it, `EditorUI::editGroup`). Editor L (20), Editor L2 (61), Z Order (25, default from the object's z order), Add Group ID with ±1 / next free / Add (57, at most 10), Z Layer buttons B5…T4 = −5 −3 −1 1 3 5 7 9 11 (24; the effective layer highlighted with GJ_button_02, the key's value in parentheses), Default. The group list (a click removes one) is laid out by guess until a capture with groups. Replay screens are now one method (`Ui::draw_replay`) also drawn over the editor (the editor UI goes through `overlay`, which never drew them: the popup was invisible and clicks fell through to the level).
- `scene_to_layout.py` bug: the flag scan read up to three lines after a node without stopping at the next node, so a node took its next sibling's `#flip` and `#ignoreanchor`. That mirrored the left arrows in Edit Group and placed some buttons/sprites half their size off (shards' bottom-right corner, editlevel's info icon and side art, info/leaderboard/profile buttons). Fixed; shards, editlevel, profile, leaderboard and info regenerated (info without its particles). Shards vs real: 7.5.
- Harness: `.../edit:<group|object|special>:<id>` token. Trigger popup captures (edit:object on triggers) not done: after one harness crash (typeinfo_cast in the press walk, mid-transition), later launches exit during Steam's startup check and the relaunched child can't load Geode.dylib. To retry.
- UI monkey test: seeded random clicks/keys/text/wheel (2,500 actions per seed, 6 seeds) from the menu in an isolated save: no panics. Scripts now ignore Quit (log "script: quit ignored"); script keys gained Backspace, A, D, Z, X. Found: a level list's song URL without a scheme ("relative URL without a base" for song 207054); song downloads now ask getGJSongInfo when the list's URL has no scheme.
- Harness: `editlevel:N[/...]` opens a fresh editor level's page (`EditLevelLayer::scene(GJGameLevel::create())`, level type Editor) and presses from there: `editlevel:0/edit:object:901` is the editor with a move trigger's popup open. (The My Levels path crashed twice in `CCDirector::setNextScene` on a freed running scene; LevelEditorLayer::scene isn't bound on arm64.) Each capture with a `SetupTriggerPopup` on top also writes `<shot>.keys`: every input / toggle / slider / label from m_inputNodes, m_valueToggles, m_valueControls, m_inputLabels by property key, with world position, size and visibility, plus the slider ranges and shown values (m_minSliderValues / m_maxSliderValues / m_triggerValues). The Move popup: X 28, Y 29, time 10, target 51, lock toggles 58 / 59, etc.
- `tools/keys_to_layout.py` appends I/K/L/V lines (inputs, toggles, sliders, default values) to a popup's layout and drops the placeholder labels inside inputs; `tools/trigger_popups.py` converts every `t<id>` capture and lists them in `replay.rs` (TRIGGER_POPUPS, trigger_source).
- Editor: Edit Object (side button) opens the trigger's captured popup (inputs take digits . -, Enter or a click elsewhere commits to the key; toggles flip "1"; OK closes), or for plain objects the color popup (`CustomizeObjectLayer`, from a capture): P-Col 1/2 (1005/1006), Light BG (1007), Default (removes the key), channels 1–10, Next Free (lowest unused), Base/Detail (21/22) for objects with a detail color; the chosen channel highlighted with GJ_button_02.
- News pictures: the posts' image blocks (`news_9_3_b903b61e.jpg`, ...) download once from `geometrydashfiles.b-cdn.net/news/<file>` (a read-only GET, cached in ~/Library/Application Support/Cadence/news), decode with zune-jpeg (new dependency, 0.5) or png, and upload as one-frame sheets `news:<file>` (`Sheets::add_image`, `App::upload_pictures`, on both the level and menu frame paths). The popup draws them at 300 wide in place of the dark placeholder.
- The Wraith (`SecretLayer5`, the "wraith codes" the news mentions) checks codes on the game's server (getGJSecretReward, a reward claim): out of scope under the read-only rule.
- Editor: Copy Values / Paste State / Paste Color side buttons (state = every key but id/position; color = 21, 22 and the HSV keys 41–46).
- Editor pause menu: the captured `EditorPauseLayer` replaces the hand-drawn one (harness `edit:pause`); its first five buttons keep the old actions, the object count and length ("Length: N seconds (Tiny/Short/Medium/Long/XL)", last object x at 311.58 units/s) are drawn live. vs real: 14.6 (the level behind differs).
- Level settings (`LevelSettingsLayer`, harness `edit:settings`, the editor's top gear): Classic / Platformer (kA22), official song arrows (kA1, 0–21, name "NN: <level>"), start speed (kA4, portal icon boost_01..05) and mode (kA2, gj_*Btn_off icons) cycle, OK. `Editor::setting` / `set_setting` edit the header. vs real: 8.2.
- `GJ_GameSheetEditor` is now loaded with the menu sheets (the BG/G/MG art previews and other editor frames were missing).
- `scene_to_layout.py`: a 9-slice's children are kept except its piece batch (button labels inside ButtonSprite's CCScale9Sprite were dropped: the editor pause's Settings/Export/... texts).
- 43 trigger popups converted, including tonight's triggers (follow player Y, collision, instant collision, item edit/compare, teleport, on death, player control, camera rotate, gravity, scale, area move/rotate/scale/fade/tint, song, enter, camera edge, random, options).
- Song trigger (1934; keys from the popup's dump: 392 song, 408 start ms, 406 volume, 404 speed, 413 loop, 432 channel, 409–411 fades): the world queues it, the app plays the custom song from the song cache at the start offset (`Game::play_trigger_songs`). Online levels now download their other songs (key 52) with the level through getGJSongInfo. Volume, speed, fades, channels and SFX triggers aren't ported.
- Edit Special popups (`EditorUI::editObject2`, harness `edit:special:<id>`): item label (1615, `SetItemIDLayer`) and teleport portal (747, `SetupTeleportPopup`) captured; `tools/trigger_popups.py` handles `s<id>` captures as `Scene::Special(id)` (SPECIAL_POPUPS), drawn and edited like trigger popups. The text object's popup opens through Edit Object instead (not captured yet).
- Robustness: a frame in a level (or the editor) without a built game (a failed load or rebuild) returned early without asking for the next redraw, freezing the window; it now draws the menus and logs once. Editor monkey (3 seeds × 2,000 actions from the editor, including the new popups): no panics.
- Animate trigger (1585): the world records when each object of group 51 was animated (`World::animated_at`); "animate on trigger" frame-animated objects, hidden until then, play from that moment (`AnimObjects::sprites` takes the start). Which animation (key 76) isn't used yet.
- UI trigger (3613, `positionUIObjects` / `addUIObject`): at level start each UI trigger pins its target group (51) to the screen relative to its reference group's main object (71): x anchor 385 (2 center, 3 left, 4 right; ≤ 1 by side), y anchor 386 (6/7/8; 0 or 5 by side), 387/388 scale the offset by (w/2)/240 and (h/2)/160. `UiPin::point` gives the screen point at draw time (the window's own width); the object draws at camera + point/zoom (moves still apply), is always in the visible list, and is never collided.
- Trigger audit rerun on the 45 cached levels: the big remaining ones are advanced follow 3016 (25 levels, ~1,900 decompiled lines; not ported), SFX 3602 (16), BG effect off 1819, keyframes, camera edge 2062.
- SFX trigger (3602; keys from its popup dump and real levels: 392 sound, 406 volume, 404 speed, 405 pitch, 407 reverb, 413 loop, 433 preload): the world queues (sound, volume), the app plays `s<id>.ogg` from the SFX cache at that volume (`Audio::play_effect_volume`). Library sounds come from the files CDN (`/sfx/s<id>.ogg`, as `sfxlibrary.dat` lists; a read-only GET) when an online level downloads: the ids come from the level's own 1934 / 3602 triggers (key 392) as well as keys 52/53 (absent from the responses checked). Speed, pitch, loop, reverb and fades aren't ported.
- The song list's URL is "-" for some songs (the "relative URL without a base" bug): the getGJSongInfo fallback covers it.
- Timers (keys from the popups' dumps and real levels): Timer 3614 (80 id, 467 start, 474 + 473 stop time, 470 rate, 51 target spawned at the stop, 471 start paused, 468 don't override a running one), Time Event 3615 (80, 473 time, 51 target, 475 multi), Time Control 3617 (80, 472: 1 stop / 0 start). `World::timers` advance in `pre_step` before the delayed spawns; Item Compare / Item Edit read them as kind 2. Test added. Time warp (469) and timer display labels aren't ported.
- 62 trigger popups captured and converted (+ SFX, SFX edit, advanced follow, keyframe, event, sequence, spawn particle, BG/MG speed, UI, timers, collision state, set target ID…).
- Counter labels (1615, `LabelGameObject`; popup keys 80 item, 466 timer, 389 seconds only): in play they show the live value in bigFont (item count, or the timer's seconds with two decimals / whole), colored by their channel; the editor keeps the icon. Main time / points / attempts modes aren't ported.
- Release prep for the v0.0.1 decision: README.md (what works, read-only online, bring your own copy, no writes to the game's folder or servers); `.gitignore` now also excludes `/shots/` (real-game screenshots and scene dumps). An untracked-file scan finds 204 files, 5.8 MB, no png/ogg/mp3/plist/fnt/dat. No LICENSE yet (Mark's choice). For Mark to weigh: the captured layouts in `ui/scenes/*.txt` and a few hard-coded lines (vault replies, the Keymaster's lines, gauntlet names) carry the game's own short UI strings.
- Advanced follow (3016) left out on purpose: the per-step function is ~1,200 decompiled lines, mostly inlined hash lookups around the float math, and there are no real-game traces of a 3016 level to check a port against; an approximate follower could misplace objects the real game keeps steady.
- Color trigger popup (899, `ColorSelectPopup`): it builds its own controls (no SetupTriggerPopup dictionaries), so its fields are hand-written in `tools/popup_extras/t899.txt` (R/G/B 7/8/9, fade 10, channel 23, player color 15/16, blending 17, touch 11, spawn 62; `drop x y` removes the placeholder labels), applied by `trigger_popups.py`. The swatch takes the trigger's color; `CCControlColourPickerSpriteSheet` is loaded for the wheel. vs real: 22.0 → 11.9. Dragging on the wheel and HEX/opacity aren't ported.
- `CADENCE_NOCLIP` (debug): deaths are cleared so headless runs cover whole levels (stress runs).
- Editor swipe (`EditorUI` touch with swipe on): Edit mode drags a selection box (drawn) and selects the objects whose centers are inside on release; Build / Delete place / delete once per grid cell along the drag; View still pans. Test added.
- Sequence (3607: 435 'group.count...', 436 mode stop/loop/last, 437 min interval) and Collision State (3640: a sensor sized by 32/128/129; the player entering spawns 51, leaving spawns 71). Sequence test added; verify_all 111/126.
- Stress pass: the 62 cached online levels (audit set), each downloaded and run 6,000 steps headless with CADENCE_NOCLIP: 61 clean, one (150391318) never finished. Its zero-delay spawn loops (group 86 holds two spawn triggers that both spawn 86) doubled through all 16 levels of our instant-spawn chain every step: ~2 million move commands by step 420. Now a trigger fires once per chain of instant spawns, and reached again it waits for the next step (once), so such loops run once a step. That level peaks near 100k commands and finishes 6,000 steps in 52 s (still slow for real time there). verify_all 111/126. `CADENCE_DEBUG_COMMANDS` logs the command count.
- `scene_to_layout.py`: mirrored sprites (negative scale on one axis) were written as flip x with the wrong rotation (a vertical flip came out as a horizontal one); the rotation now comes from the un-flipped axis. Shards and profile layouts patched.
- Daily / weekly / event pages recaptured with the current harness (`shots/gui-real/daily/`), for the next pass on `draw_daily`.
- Daily / weekly / event pages redone from the fresh captures: per-page 9-slices (daily GJ_square01 + GJ_square02 box; weekly GJ_square05 + square05 at 175; event GJ_square05 in (190, 47, 242) + GJ_square04), three corners (dailyLevelCorner, weekly gauntletCorner at 0.63), crown at +62, "Bonus:" at −92, the LevelCell's content raised (face +69, stars +39, diamonds "0/(stars+2)", coins under the face, shortened counts "787K", high-object icon), Play at 0.7, "New!", the page number "#N" (key 41 mod 100000), and the event page's random bottom line (28 lines from the binary's strings). vs real: daily 9.7, weekly 22 → 10.6, event 32 → 9.4.
- Web: `std::time::SystemTime::now()` panics on wasm32; the three wall-clock reads (random picks) use `web_time`.
- Scores (`LeaderboardsLayer`, recaptured): GJScoreCell's icon and rank at +30 (rank goldFont 0.65), the name from +60 (≤ 120 wide, ≤ 0.8), stars/moons right after the name (each ≤ 58 wide), the second line's counts shrunk to their slots with icons 9.5 after, creator points only when non-zero (the scales drop to 0.45 then); the four stat buttons on the right (stars, moons, demons, coins; 40 × 40 at 0.6, the chosen one cyan). Picking another stat only highlights it (the request with that stat isn't ported). vs real: 16.5 → 10.7.
- Level page (recaptured `LevelInfoLayer`): the song box is GJ_square01 (brown) for a custom song, as captured; the dark box stays for official songs. vs real: 13.8 → 5.6. (The real page's "SFX: 10" and the size with SFX come from data the list response doesn't carry: not shown.)
- Keys (Settings): the captured KeybindingsLayer carried Geode's own additions (a "Mods" tab and a settings gear: geode.loader is the only mod in the harness copy besides ours); removed from the layout. The real rows' labels aren't in the dump (to look into); the page stays informational.
- Lists (recaptured): LevelListCell redone (face from the list's difficulty 1–10, its diamond reward under it, name ≤ 200 wide with an info icon after it, "By X" at 0.7, the progress bar "0/needed" with a diamond, downloads / likes / completes "0/levels" with shortened numbers, View 63.1 wide). The header counted only level rows ("1 to 0 of 9999" on lists and map packs); it now counts the rows shown. vs real: 14.0 → 10.6.
- Featured / level cells: one shortening rule for counts (`short_count`: 9.4K under 10,000, 17K, 276K, 1.4M, truncated) used by level, list and daily cells; user coins under the stars 10 apart, gray (165) until verified (captured), instead of after the name. vs real: 10.5 → 9.5 (what's left is mostly the real list's clipped third row, which needs clipping).
- Clipping: `Canvas::clip` cuts axis-aligned quads (and their UVs) to a rect (`clip_instance`; test added). Level, list and map pack lists draw every row that touches the list, cut to it, like the real stencil: a third row shows half under the frame. Featured vs real: 9.5 → 8.9.
- First run without the game: Cadence used to open an empty window (even its fonts come from the game). It now checks for the game's sheets first (the Resources folder, or the .app / install folder around it, from CADENCE_GD_RESOURCES or the Steam path), and if none are there explains where it looked and how to set CADENCE_GD_RESOURCES: on stderr and, on macOS, in a dialog; then exits 1.
- In a level, Pause → settings → Keys opened an invisible screen (the level overlay never drew captured screens); it now draws them over the pause, and Escape returns to the settings (scripted check).
- Event trigger (3604; 51 target, 430 event list): armed once, it spawns its group when the player has one of its events. The physics events map to `GJGameEvent` ids (Geode's Enums.hpp): orbs 8 + 34–44 by orb, pads 9 + 45–49, portals 26–33 / 50–59 by portal id, landing 4 (the landing strength classes 1–5 aren't split), spider teleport 18. Jump push/release (69/70) come from the input; left/right and the extra IDs 447/525 aren't ported. verify_all 111/126.
- Editor first screen vs a fresh real capture (`editlevel:0`, empty level): the real object layer sits at (0, 156), so the default camera is now (0, −156) (world x 0 at the left edge, the ground top at screen y 246; it was (−90, −60)); the build tab starts with nothing picked (clicking the level places nothing until an object button is pressed). vs real: 34.1 → 19.7. Tab 0's objects 69–71 still draw flat (their icons' shading isn't reproduced).
- Editor chrome vs the real capture: the mode buttons' icons carry their labels (we drew them a second time); Paste greys out with nothing selected or copied; the position slider uses slidergroove_02 with GJ_moveBtn as its thumb (loose textures, now loaded); the bottom bar reaches behind the tab row (117 high) and takes touches there. Empty editor vs real: 19.7 → 14.6.
- Web build checked in the Browser pane (`.claude/launch.json` "cadence-web": a static server for web/, `?dev=1` reads the local gd/ link): the main menu, Rewards, Settings and the Keymaster's line work with no panic (the last would have panicked before the web_time change).
- Editor object buttons draw the object's visible child sprites over its main one (objects like 69–71 are a lighting overlay plus a shape child; they showed as flat squares). Empty editor vs real: 14.6 → 14.0.
- Text objects (914): harness `.../edit:object:914/then:25` presses a button once the popup is up (here the Text toggle; the walk counts hidden items, so indexes differ from the layout's) and captured CustomizeObjectLayer's text mode ("Insert Text", the text field, Kerning, Split). In Cadence the color popup shows a Text button for text objects; the text mode (`Scene::EditText`) types into key 31 (base64) live, Backspace deletes, the trash button clears, Text goes back to the colors, OK closes. Kerning and Split aren't ported.
- Map packs (recaptured): the star and coin counts are white bigFont (were goldFont), and the page has no refresh button (Featured and Lists do). vs real: 10.6 → 9.6.
- Shake (1520) and Camera Offset (1916) popups: hand-written fields in tools/popup_extras (75 strength / 84 interval / 10 duration; 28 / 29 offsets, 10 time), like the color popup.
- Trigger popup key safety: older popups key their input dictionaries by internal tags, not property keys. Alpha and Touch Toggle used 3 for Group ID (3 is the object's y: editing it would have moved the trigger), Instant Count 0/2, Advanced Random 1, Sequence −1/−2. `tools/popup_extras` gained `map a b` (Alpha / Touch 3 → 51, Instant Count 0 → 80, 2 → 51) and `del a` (the Random and Sequence list builders, which aren't keys). Every remaining input/toggle key is a real property key.
- Level Settings' BG / ground art pickers: not captured. Pressing buttons by walk index after opening the popup hit Live Edit (16), which crashes the real game outside a playtest; a frame-name based press in the harness would be the safe way. They stay "not yet".
- Harness `.../thenframe:<frame>` presses the visible button whose sprite shows that frame (index presses count hidden items and once hit Live Edit, which crashes the real game). Captured Level Settings' art pickers (`SelectArtLayer`): background (59 icons, kA6) and ground (22 icons, kA7, line 1–3 → kA17) and middleground (4, kA25), the chosen icon white over the grid at 125 gray, OK back to the settings, whose BG / G previews now show the chosen art.
- Editor layers: the layer arrows change the shown editor layer (the gray arrow: all, shown "All"); objects off it draw at 30% and can't be picked; new objects go on it (key 20); Go To Layer shows the selected object's layer. Test added.
- 07:22 Editor mouse wheel like EditorUI::scrollWheel: the wheel moves the view up/down (was sideways), shift (or a trackpad's sideways swipe) moves it sideways, ctrl/cmd + wheel zooms in 0.1 steps. App now tracks held modifiers (ModifiersChanged).
- 07:26 Editor keyboard shortcuts (desktop defaults): WASD move a block (shift: 2 units), Q/E rotate, R/F/T/G toggle rotate/free move/swipe/snap, ctrl/cmd Z / shift-Z / Y / D / C / V. Off while a popup, the pause menu or a trigger input is open. Decompile (re/decomp-editor) shows macOS EditorUI::keyDown only handles Enter/Esc; scrollWheel pans (y from the wheel's first axis) and zooms with ctrl.
- 07:27 Popup stress: all 62 trigger popups, 0 panics. Final regression: 60 tests pass, verify_all 111/126 bit-exact (unchanged), release + web build clean. Mark is up; overnight run ends here.

## 2026-11-04 morning (Mark's playtest feedback)
- Popup open animations everywhere, from FLAlertLayer::show (decompiled): main layer setScale(0.1) → CCEaseElasticOut(CCScaleTo 0.5, 1), period 0.6; backdrop CCFadeTo 0.14 s; closing instant (keyBackClicked removes it). One `open_anim` pass over the popup's draws (canvas.mark / draw list) — the full-screen backdrop fades, the rest scales about the middle. Applied to every replay popup (trigger/group/color/text/settings/art/info/profile/song/assets/keys/shards), small popups, news, paths, quests, daily, map promo, more-options pages, the level page's popups, alerts (was 0.7→1, now 0.1→1). EditorPauseLayer is a CCBlockLayer: no animation (as vanilla). GJDropDownLayer (settings/stats/achievements) already slid 0.5 s EaseInOut 2.
- Editor: black under the ground layer (one ground tile tall, then nothing — vanilla), camera clamped by EditorUI::constrainGameLayerPosition(-100, -100): 100 blocks left/below, 80 above the ground.
- Search: demon filter. The hidden GJ_plus2Btn at menu (202.5, −77) shows with Demon on (toggleDifficulty: N/A, demon, auto are exclusive; the star difficulties go together). DemonFilterSelectLayer: GJ_square01 380×180, "Demon Filter", faces (all, easy 07, medium 08, hard 06, insane 09, extreme 10; btn2 frames at 1.1, 18 apart), OK. The search's demon icon becomes the chosen face; the request sends demonFilter 1..5 with diff=-2.
- **Memory leak (cause of Mark's Mac freezing at 0.1 FPS, force restart 08:20):** the on-screen render path never polled the wgpu device, so every frame's `write_buffer` staging memory stayed alive: +47 IOAccelerator regions/s, ~2 MB/s, 1.6 GB after ~10 min on an 8 GB Mac. Hidden-window tests never saw it (an occluded surface returns before drawing). Fix: `device.poll(PollType::Poll)` after each present; measured flat (1191 regions, 1127 MB) over a minute in Mark's window.
- Start settings from the level header (GJBaseGameLayer::setupLevelStart): kA2 start mode (Bloodbath starts in the ship), kA3 mini, kA11 flipped gravity. Not yet: kA8 dual at the start. Debug: CADENCE_DUMP_HEADER. (Note: CADENCE_ONLINE also needs the id as the first argument.)
- Online views fade like scenes (CCTransitionFade 0.5 s): search → results → level page and back, gauntlets, scores; the switch happens at the fade's middle (Ui::update watches online.view).
- Bloodbath "shake" at the start: starting in the ship went through switch_mode like a portal, so the camera and ground slid into the corridor over 0.5 s. Camera::start_in_corridor puts them in place at once (setupLevelStart). Shake triggers: none in Bloodbath's first 3000 units (CADENCE_DUMP_IDS=1520,... lists objects by id with their keys).
- OK buttons (alerts, the demon filter) scale as one node when pressed (border + text), like a CCMenuItemSpriteExtra around a ButtonSprite; DemonFilterSelectLayer::onClose removes the popup at once (no close animation in vanilla).

## 2026-11-04 — accounts and server writes (Mark's decision)
- Mark wants login, chests, comments, likes, ratings and level uploads, with requests identical to the real game's (so nothing looks odd). Memory updated: server writes allowed, opt-in, GJP2 only, password never stored or logged.
- Every request format string from the 2.2091 binary (`strings | grep '=%'`): the base `gameVersion=%i&binaryVersion=%i&udid=%s&uuid=%i&dvs=%i` (+ `&accountID=%i&gjp2=%s`), login `udid=%s&userName=%s&gjp2=%s`, rewards `&rewardType=%i&secret=%s&chk=%s&r1=%i&r2=%i`, comment `&comment=%s&gameID=%i&extra=%s&secret=%s`, like `&itemID=%i&like=%i&type=%i&secret=%s&special=%i`, upload `&userName=%s&levelID=%i&levelName=%s&...&ts=%i&notify=%i`, user score, search (`&type=%i&str=%s&diff=%s&len=%s&page=%i&total=%i&uncompleted=...` — ours differs today), and the rest.
- Both saves on this Mac (real + harness copy) are logged out; both have a 36-char playerUDID; the real one a 9-digit playerUserID.
- Harness: `CADENCE_HTTP_LOG=<file>` logs every GameLevelManager/GJAccountManager::ProcessHttpRequest (credentials masked to their length); `CADENCE_HTTP_REDIRECT=http://127.0.0.1:<port>` sends them to a local recorder instead (headers).
- `src/gjapi.rs`: Identity (udid/uuid/account) and the base string, GJP2, cycled XOR, URL-safe base64, chk, random strings, a UUID-style udid; getGJRewards form + response parser (5 junk chars, base64 XOR 59182, '|' hash; ≥12 ':' fields, udid checked like onGetGJRewardsCompleted); login form + answer codes. binaryVersion (45), dvs (3) and the exact chk recipe still to confirm from a capture before anything is sent.

## 2026-11-04 day — performance on stress levels (Mark away)
- Stress levels found by search: pc killer 35756838 (65535+ objects, 79k colliders), Lag Machine 30267854 (39552 objects, up to 121k sprites a frame), stress test 73965779 (37k). Bench: CADENCE_BENCH now reports sprites and draw calls per frame; CADENCE_WINDOW=WxH sets the window; CADENCE_NOVSYNC (no effect on macOS so far).
- Fixes (visuals unchanged: 546/634 real-vs-ours frames identical in score, 63 better, the rest traced to older causes; 53/53 frames pixel-identical with/without occlusion; physics 111/126 unchanged):
  - Effects::sync_objects/sync_custom were quadratic (a linear search per emitter over the visible list) → hash set.
  - Draw loop parsed every object's fields from text every frame and culling walked all objects: ObjStatic table at load + 64-unit x columns (movable = grouped/UI-pinned objects checked every frame); an env read per object (CADENCE_DEBUG_OBJ) hoisted.
  - Per object type: resolved frames, z order and portal flags cached per frame (TypeSprites); frames pushed by Frame, not by name.
  - Draw calls 18,299 → ~15 a frame on Lag Machine: each z layer's sprites go glow sheet, fire sheet, then the rest, like setupLayers' batch nodes (glow ≈ layer−230, fire ≈ layer−100, game sheet blend/normal).
  - Exact occlusion: depth per instance (m1.w); a pre-pass writes depth where normal/opaque sprites' texels have alpha exactly 1 (not ≥ 1: tints over 1 subtract); the main pass tests LessEqual. Off with CADENCE_NO_OCCLUSION.
  - Object emitters drew in HashMap order (random per run) → sorted by (z, object index). Rod balls pick a random frame per load like the game (CADENCE_BALL fixes it for tests).
- Lag Machine mean frame 116 → 22 ms (p50 16.9), pc killer 32 → 23 ms (physics spikes remain), stress test and Bloodbath locked 60.
- Found (older, not from today): Dash (22) steps ~7500–8500 differ from the real game (camera and zoom match; the real frame has a ShaderLayer active — our post effect seems to place things differently).
- Web, no GPU (Chrome --disable-gpu = SwiftShader), full resolution 2560×1426: our own code is ~2% of the work; SwiftShader ~900 ms/s, the canvas readback ~900 ms/s, the software compositor ~270–700 ms/s. Even a frame that only clears costs ~2.4 cores per second there, so "one core, no GPU, Retina, locked 60" is out of reach for any page. Web fixes: winit ControlFlow::Poll spun a browser task per turn (~27% of the main thread) → Wait with request_redraw in about_to_wait. `taskpolicy -b` (macOS background QoS) throttles timers to ~110 ms frames and is not a weak-device stand-in; use DevTools CPU throttling. Dev flags on the web: ?noclip, ?noocclusion, ?nodraw, ?level=<id> (bench/<id>.txt).
- web/gd is a symlink to the Steam install: test levels I had saved there were moved to ~/cadence/bench-levels (web/bench links to it). Nothing of the game's own was changed.
- The real "hard" levels (by object count/triggers): Sonic Wave Infinity 247k/8.9k, Tidal Wave 220k, Zodiac 197k, Bloodlust 171k, Firework 165k, Avernus 164k, Kenos 157k, Slaughterhouse 147k, Abyss of Darkness 113k (bench-levels/).
- Harness bench mode (`bench <s> <file>`: real frame times, noclip, intervals + frame size), `noclip` for traces. The game copy draws 640×360 points at 2× = 1280×720 px; Cadence benches at CADENCE_WINDOW=640x360 to match. Vanilla holds 60 on every heavy demon on the M2 (8–21% CPU, 360–990 MB) and drops on Lag Machine (47.6 FPS) and pc killer (55.7).
- Memory: sheet pixels freed after GPU upload; one copy of the level's objects (moved, not cloned); RawObject stored as its text + field offsets; texture quality chosen like CCDirector::updateContentScale "auto" (UHD from 3× 480×320, low under 1.5×, else HD) — Cadence had always loaded UHD (4× the texture memory of what vanilla used at 1280×720). Bloodbath 1106 → 198 MB (vanilla 363), Sonic Wave Infinity 1203 → ~460–650 MB (vanilla 993).
- Leak: instance/camera uploads happened before the surface acquire; a covered window returned without submitting, stranding staging memory (16 MB/s on Sonic Wave Infinity) → upload only for drawn frames, poll on early returns.
- pc killer: used orbs/pads/portals were a Vec searched linearly per touch (up to 1,719 touches a step) → list + set (UsedList; same order and length). A y prefilter on special objects broke 3 physics suites and was reverted.
- Noclip now skips `die` entirely (Player::noclip), like the harness's destroyPlayer hook; before, death ended the step's collision pass and paths diverged.
- Open: at the same x in pc killer the game runs ~543 particle systems, Cadence ~22,000 (all portals get emitters here). That is a visual difference and pc killer's main cost; the game's activation rule for object particles still has to be found (createParticle runs per object at setup, so it isn't the pool size).
- Head-to-head round 2 (same 1280×720 px, 20 s, noclip both, one at a time): on all heavy demons Cadence holds 60 with no frame over 20 ms (vanilla has 33–50 ms spikes) and uses 30–50% less memory (e.g. Sonic Wave Infinity 580 vs 999 MB, Tidal Wave 495 vs 986). Lag Machine 56.3 vs 47.6 FPS. pc killer was worse (47.6 vs 55.7).
- Culling: "movable" was every grouped object (most objects in 2.2 levels) → only objects in groups that move/follow/rotate/scale/area triggers move (World::can_move; the only write to `moved` is reached from those). CADENCE_CHECK_CULL compares with the brute-force set every frame: 0 mismatches on main levels and all 12 heavy levels. CPU: Tidal Wave 24→13% (vanilla 9), Sonic Wave Infinity 32→13% (vanilla 20), Avernus 31→18% (vanilla 20), Firework 29→18% (vanilla 12).
- Particle limit found: `createParticle` makes a system only while the pool for its key holds < GJBaseGameLayer +0x2ff0 (read from the running game: 40, every level); key = getParticleKey(plist, two ints) — the real dumps show exactly 40 systems per portal colour. Cadence gave every visible portal an emitter (22,000 at one spot of pc killer vs the game's 543). Now 40 per (file, layer, start colour), claimed left to right as objects come into view, none until an object leaves and returns (480 at that spot). pc killer 47.6 → ~53.5 FPS (vanilla 55.7).
- Also: portal back frames cached per frame name (a string search + allocation per portal per frame), object draw lists pre-sized from the last frame, unrotated quads skip sin_cos (bit-identical), FxHash for per-frame maps, used orbs/pads/portals as list + set.
- Clean head-to-head (my test Chrome had been loading the earlier rounds; this one ran with nothing of mine alongside): BENCHMARKS.md. Cadence: same 60 FPS on the heavy demons, 35–50% less memory, worst frames lower on most; pc killer 59.5 vs 57.7 FPS, Lag Machine 57.4 vs 49.7; but more CPU per second than vanilla on the heavy demons (≈32% vs 22%) and more small >20 ms frames on six levels. Visuals vs real frames: 509 closer than this morning, 96 same, 9 marginally further (≤0.37).

## 2026-11-04 evening — Dash visual differences
- Steps 7552/8448 (scores 62/46): ours was the real frame turned 180°. Dash rotates the camera to 90° and 180° with **touch-triggered** camera rotate triggers (x 6703/6883, key 11, scaled 3.278× so they act as tall walls), then adds 180° at x 7995 (real ends at 360°). Camera triggers skipped touch ones on purpose. Now `CameraTriggers::touch` holds them with the world's scaled 30×30 square; `Player::camera_touch_pass` fires each once when either player's rect meets it (after the order walk; toggled-off ones stay armed; used flags live in the player, so checkpoints restore them). 7552 → 14, 8448 → 12.
- Background (`GJBaseGameLayer::updateCameraBGArt`, decompiled into re/decomp-bg): the background is a node moved every frame by (old view center − new view center) × m_cameraZoom × 0.1 (x by the mirror factor), starting at 0, 0; no clamp (`visit` only crops its texture rect between ground and ceiling, picture unchanged). Cadence used −0.1·camera y and x without the zoom. Now `bg_pos` integrates it (`Game::scroll_bg`), saved in checkpoints. 7 Dash frames closer, none worse beyond noise.
- Middle ground: y = (mgY + offset)·((1 − f) + f·zoom) − f·camera y with f = 0.5 (not the fitted 0.4) and the offset from **middle-ground triggers (2999, key 29, tweened, `updateMGOffsetY`)**, which Cadence didn't run. With base 25 and Dash's offsets (24 at x 945, 48 at x 9075, 180/228 on channel 10) the formula gives every real Dash frame's MG y to 0.001. Trigger 2999 is now a camera trigger kind (position, order channel or touch); the camera tweens `mg_offset`. The MG uses the camera without shake.
- Not yet run: BG/ground/MG change triggers (3029–3031; Dash has three, x 15105/15765/21405 — the frames after them, 18400/21120, still differ).
- Open: step 4832 (score 40): real's background fades to near-black on the right with lighter purple streaks; ours is bright magenta with the cloud row visible. Ruled out: camera/zoom (match, 0.9091), background texture and placement, middle ground (off screen in both), culling (one real object outside our window, op 0), occlusion (identical with it off), gradient triggers (both real layers vis=0; ours off screen too), d_gradient decorations (same positions and opacity). Next idea: the background color channel 1000 over time (real col 199,0,213) and pulse triggers on it.
- Static draw cache (`DrawCache` in app.rs): sprite instances are world-space, so an object that nothing moves/turns/scales/tints/animates/pulses/spins and that is ≥ 70 units inside both view edges (no fade, no enter effect) produces the same instances every frame. Its finished sprites are kept (with the list each went to) and replayed while its channels' versions are unchanged; versions bump when a once-per-frame snapshot of every channel's (color, opacity, blending) differs. Entries unused for 64 frames are dropped. Off with CADENCE_NOCACHE / `?nocache`.
- A/B (cache vs CADENCE_NOCACHE, full runs, same steps as every real frame): first 573/614 identical — rods' balls (15–17) are drawn after the per-object record and pulse with the music, so hits lost them. Rods excluded → 614/614 identical on the main levels.
- Docs: DOCUMENTATION.md rewritten for the current state (online requests and identity, developer switches, benchmarks, rendering pipeline, visual tools); README status corrected (requests now match the game's; writes built but not reachable from menus yet); NOTES.md: background/middle-ground/touch-camera-trigger findings.
- Change BG / ground / MG triggers (3029–3031, key 533) ported as camera trigger kinds: `Camera::art` (0 = the level's kA6/kA7/kA25), names recomputed by `Game::sync_art` when it changes, every image the level's change triggers can reach preloaded with the level's own (no mid-level loads). Unit tests: touch triggers' scaled square (Dash's 90° rotation), art ids, MG offset tween.
- Dash's second half had the wrong background colors: color/pulse/alpha triggers on an order channel other than 0 (key 170) were fired by x in `Colors::step`, ignoring the channel walk, so e.g. channel 14's background pulses (x 21855/22185) ran while the real game was still on channel 13 (real channel 1000 = 27,0,56 flat; ours pulsed up to 138,0,145). They now go through the order walk like spawned ones (`groups.rs` gives them an Effect action; `Colors::new` keeps them in `effects`, fired by level order). Dash frames: 18400 30.8 → 16.3, 11168 27.3 → 14.4, 12064 21.5 → 9.8, 12800 33.1 → 22.2; Dash mean 18.75 → 16.26 (≈19 this morning). No other official level uses order channels, MG, zoom or change triggers.
- Screen shake counted down per drawn frame (1/60 each), so in screenshot/bench runs (few frames drawn) a shake fired long before was still applied: Deadlocked step 4600 drew the scene ~5 units off (real's object layer at exactly −camera). Shake now advances per game step (`Game::step_shake`, at both step sites). That frame 30.5 → 13.7.
- Cache A/B on the 15 heavy levels too (noclip, 14 frames each through the level): 210/210 identical.
- Settings screen (`OptionsLayer`) vs the real dump: buttons are fixed-width ButtonSprites (label squeezed to 130 / 78 at most scale 1, sprite +16 wide, 31.5 tall, label 2 up) — ours were label-sized at 0.8; the sliders are scale 1 (track 210, thumb 38), ours 0.7; the thumb travels ±100 around the center (pause menu's: ±70 at 0.7; ours went +103); the vault lock carries "10" (bigFont 0.4) and a user coin (0.5). Score 42/23/21 → 28/8/14 (the rest is the menu's color cycle and its random running icon). Screenshot popups were caught at their first open-animation frame (`CADENCE_UI_TIME` advanced before any popup existed) → `Ui::skip_open_animations`.
- Account login (`ui/login.rs`, `AccountLoginLayer` decompiled + disassembled for the part Ghidra dropped): GJ_square01 400 × 300, fields at +80/+30 (square02_small 300 × 30, opacity 100, labels bigFont 0.4, placeholders 0.5 in 120,170,240), Forgot buttons (GJ_button_05 120 × 25, goldFont 0.8) at −45, Cancel/Login at −128; the game's checks (username ≥ 3, password ≥ 6) and failure texts per code in the field labels; "Login Success" alert. `gjapi::login`: GJP2 only; on success the identity takes the account and its user id as uuid (the recorded traffic: uuid 377181558 → 299760057 after logging in); another account than the linked one → −10. Account page: Not logged in / Linked to account (Login required) / logged in (Save, Load greyed — Save would overwrite the cloud backup with Cadence progress — and More → Unlink). Test with a stand-in local server: request byte-identical, password never sent or saved.
- Benchmarks that don't get in the way (Mark working on the laptop): both programs park their window on top past the main screen's bottom-right corner (only a 320 × 200-point piece shows above the Dock, mouse passes through), never activated, music playing muted. A fully hidden or covered window can't be used: macOS stops pacing it by vsync (Cadence ran 240× fast once) and the real game stops drawing. The real game's window is 1280 × 720 points (it draws 1280 × 720 pixels and the system scales them up), so shrinking it would change the work. Cadence: CADENCE_CORNER (winit with_active(false), AlwaysOnTop, activate_ignoring_other_apps(false), then AppKit setFrameOrigin/setIgnoresMouseEvents via objc2) and CADENCE_BENCH_AUDIO. Harness mod: `corner` (run loop timer from mod load parks the window, since no frame is drawn while it's covered; NSApplication activate calls no-op) and `mute`. A directly started copy was still activated when the Claude app was in front (AppKit activates at the end of launching, before mods load), so tools/bench_pair.py launches it with `open -g -n -W --env …` and the mod writes `# rusage` (getrusage CPU, proc start → exit wall, lifetime peak footprint) in place of /usr/bin/time. A watchdog marks any run whose program came to the front and gives the front back.
- Corner benchmark, all 15 heavy levels, Mark working meanwhile (BENCHMARKS.md, second table): no run came to the front. Mean CPU 39.5% vanilla / 29.0% Cadence / 28.9% Cadence + draw cache; MB 701 / 593 / 606; >20 ms frames 1351 / 21 / 37; Cadence 60 FPS everywhere, vanilla drops on 7 levels (Lag Machine 48.3). The draw cache saves no measurable CPU (kept: pixel-identical); Lag Machine's cache-only slow frames were run-to-run noise (repeats 75/62 off vs 82/86 on). The real game's CPU is much higher here than in the morning's focused runs (~22%), so the two tables aren't comparable to each other.
- Ready for a first push (nothing committed): `git add -n .` is 223 files, 5.7 MB, none from the game, the RE workspace, screenshots or bench levels. Checked for secrets, account data and personal paths (none; the server salts in gjapi.rs are the publicly documented ones). Added LICENSE (MIT, as Cargo.toml already said); `.gitignore` gains `tools/re/names-*.tsv` (function names copied from the unlicensed Geode bindings; ghidra.sh now rebuilds it with bro_names.py), `__pycache__/`, `*.pyc`, `.DS_Store`; `.claude/launch.json` serves `web` by relative path; ghidra.sh takes GHIDRA_HEADLESS / GHIDRA_JAVA_HOME. README: performance line matches the corner benchmark (the old "35–50% less memory" no longer held on 4 of 15 levels), what's left out of the repository, license section. `cargo test --workspace --release` 71 passed, release and web builds clean.

## 2026-11-04 night (trigger agent): the remaining 2.2 triggers

Worked in a worktree branch; the decompiles from a scratch copy of re/ghidra, with helper
prototypes (float arguments in s registers, CCPoint results through x8) so Ghidra keeps the
float maths. What the game does is in NOTES ("The remaining 2.2 triggers").

- **Advanced follow 3016 / 3660 / 3661** (`advfollow.rs`): ported from the instructions of
  `processAdvancedFollowAction`, fused multiply-adds and all. Checked with the harness on two test
  levels (watchgroup, 800 steps each, 9 object groups): mode 0 easing, mode 1 friction/accel with x
  only and a start speed, mode 2 steering with low/high/break forces, target direction and rotate
  to direction, a 0.5 s delay, following the camera, a re-target, an edit (speed × 0.5), two
  instances with priority and exclusivity: every object's f64 position equals the real game's to
  print precision (≤ 5e-7, 5e-6 past x 1000) every step, rotations to 3e-5.
- **Keyframes 3032 / 3033** (`keyframes.rs`): two test levels (two and four keyframes, easing 2 /
  3, scale 2 and 0.5 × 1.5, a revolution, direction, position / rotation / time mods, a spawn on
  reaching a keyframe): positions, rotations and scales match the real game exactly. Findings:
  absent mods are 0 (a 3033 without key 520 finishes instantly), y mods default to x, rotation and
  scale act on each object in place. The spawned move on a decoration shows the per-frame batching
  of decoration moves (real values change every 4 steps; the totals agree) — not keyframe-specific.
  Not ported: curves, reference-only runs, close loop.
- **Enter effects** (`enter.rs`; the renderer now asks the world instead of a list by x): two test
  levels with custom move / fade / scale / rotate, exit-only effects, legacy 23 / 56 / 58, enter
  channels, enter stop and 1915: drawn position, opacity and scale match the real game on every
  visible step (2,300 object-steps; the real node values lag a rendered frame). The rotate effect's
  extra 1° (the game's accumulator starts at 1) shows in the real node rotation too. Found on the
  way: the edge fade margin is width / 2 − 15 over 30 wide, else 0 (Cadence faded from the far side,
  15 units early for 30-unit blocks) — fixed for every level's fade.
- **Camera**: gameplay offset 2901 (twice, one "don't zoom"), camera edge 2062 (right edge): the
  camera is bit-exact with the real game over 1,500 steps (`cadence verify`). Camera mode 2925 is
  unit-tested only (parsing, clamps).
- **Reverse 1917**: the player trace matches until the real player dies past x 0 (488 steps); the
  going-left flag is set on the trigger's step (applied before the camera).
- Unit-tested, not traced: random 1912 (own random sequence), area stop 3024, reset 3618,
  persistent item 3641, UI progress 3663, BG effect 1818 / 1819, BG / MG speed 3606 / 3612, touch
  1595, spawn particle 3608 (copies emitters in the app), platformer checkpoint 2063 (app respawn),
  edit song / SFX 3605 / 3603 (volume fades, speed, stop of trigger sounds; the speed index is
  taken as semitones). 1931 and 3655 are inert in play (their handlers are empty), 2016 / 3642 are
  editor guides (not drawn as guides: the editor overlay is ui/editor.rs), 3662 link visible needs
  nothing in Cadence's culling.
- Tools: `cadence watch <spec>` (group traces from Cadence, now with `levelstring` specs),
  `cadence features` takes a level file and knows every trigger; the harness's watchgroup falls back
  to the whole group (decorations aren't in static groups) and records opacity / scale / color /
  visibility.
- `cargo test --workspace --release` green (52 core tests, 18 new); verify_all 111/126 as before.
- Published: first commit 316b45c pushed to https://github.com/mark20142026/cadence (public, branch main, 223 files). Commits use the account's GitHub noreply address (set in this repo's config only). This entry is not committed yet.

## 2026-11-04 night — v0.0.2: web online play, account screens
- Mark: nothing may stay "not yet" for 0.0.2 (PLAN.md has the full checklist). A worktree agent ports the missing triggers in cadence-core meanwhile; this session does accounts, online menus, editor.
- Web online play: `net.rs` (post / post_url / get / spawn): requests are async now; desktop runs them on a thread with pollster, the browser through `fetch` to the relay. `relay/worker.js` (Cloudflare Worker: static site + `/relay/database/*`, `/relay/post?u=` for the account server, `/relay/get?u=` for news/SFX/songs, host allowlists, no logging, observability off) and `relay/dev.mjs` (the same Worker under Node for local testing). Downloaded songs/SFX on the web live in the page's sound store (`net/songs/<id>.mp3`). Checked in the Browser pane: news + pictures, Featured list, a level download with its Newgrounds song and 10 library SFX, all through the relay. Deployed: https://cadence.mark201120142025.workers.dev (tools/deploy-web.sh builds target/site from index.html + pkg only, never web/gd); boomlings answers requests from the Worker.
- Harness: run-trace.sh now launches through LaunchServices in the background (corner + mute added to the spec) so captures never take the focus; CADENCE_HARNESS_HOME picks the game copy's home. re/home is linked to Mark's real account, so account screens use re/home-anon (the save with the GJA_ keys, player id and udid replaced) and re/home-fake (a made-up account "CadenceTest"), with CADENCE_HTTP_REDIRECT to tools/http_recorder.py; the mod's `acct*` commands (Register, Help, More, Login, Backup, Sync, RegisterSubmit) refuse the request-sending ones without the redirect.
- Recorded from the real game: getAccountURL.php `accountID=%i&type=1|2&secret=Wmfd2893gb7` (no base string; the live answer is https://www.robtopgames.net); backupGJAccountNew.php base + `&saveData=<gz+b64url CCGameManager>;<gz+b64url CCLocalLevels>&secret=Wmfv3899gc9`; syncGJAccountNew.php base + `&secret=Wmfv3899gc9`; openedRegister.php `secret=Wmfv3899gc9` when Register opens; registerGJAccount.php `userName&password&email&secret&rsession=` (rsession comes out empty in the game).
- `cloud.rs`: Load imports GLM_01/GLM_03 progress (k19/k20/k18/k36), coins (`unique_<id>_<n>` and `unique_secretNN` in GS_value, `<id>_<n>` in GS_3/GS_4), gv_ vars and volumes. Save downloads the cloud save first and merges Cadence's progress in, only raising values, adding c_/star_ completions and stats 3/6/8 for new ones; everything else stays as the game left it. Tested end to end against stand-in servers (gjapi login test).
- Account page: Log in / Help / Register when logged out; Save / Load / More when logged in (10-star and 30-second rules, the game's questions and result texts); Account Help (Refresh Login, Manage Account, Unlink Account; Cadence's unlink keeps local data, the game's deletes it). Register popup (`ui/register.rs`) from the real popup's dump: the game's checks (username ≥ 3, password ≥ 8 printable and not on its common list after folding case/leet/repeats, matching fields, email shape), the strength label, server error texts. GUI diffs vs real: register 6.3, account help ~20 (menu color cycle).
- Every server action recorded from the real game in one go: the mod's `call:all` / `call:more` (fake-account home, recorder) call GameLevelManager's request functions with made-up ids. Bodies now in `gjapi` (tests `recorded_forms`, byte for byte): profile posts (`uploadGJAccComment20`, chk = name+comment+"001"), level comments, delete comment (`&commentID&secret&levelID`; the binding's argument order is swapped), likes (types 1 level, 2 comment, 3 profile post), rate stars (chk like a like's), rate demon (its own secret Wmfp3879gc3), report (no base string), friend request, delete request (`isSender`), message (body XOR "14251"), user lists, friend requests, own user info (`notifMask=65535`), description, challenges, rewards, global scores, level scores (s1…s20; its chk salt is built at run time — being worked out). Two bindings are wrong on this build (acceptFriendRequest calls an admin request; getUserList stops the batch): those use the format strings.
- Opening your own profile makes the real game upload your stats (`updateGJUserScore22.php`). Cadence doesn't: it doesn't keep every stat (online stars, for one), so it would lower the player's leaderboard numbers.
- The main menu's profile button (logged in) and your own pages (`social.rs` data/requests, `ui/social.rs` pages): ProfilePage (rank, stats row laid out like the game's, icons, posts with delete/like, the five buttons with their count badges), Friends (Friend/Block/Follow/Muted tabs, rows), Friend Requests (received/sent, accept by opening a request, remove), Account Settings (the account's own choices from keys 18/19/50), a post/message box. Each is the real layout (myprofile/friends/requests/acctsettings.txt from fake-account captures, `replay::draw_where` leaves out the captured rows) with this account's rows drawn at the captured positions. GUI diffs vs real with the same made-up data: profile 11.5, friends 21.5 (the real one has an achievement banner over it), requests 15.2, settings 12.8.
- Level page: like/dislike (LikeItemLayer, once per level, no account needed), comment (the post box, your best % on the level), report ("Confirm Report"); chests (RewardsPage: state, h:mm:ss timers, open → orbs/diamonds into Cadence's stats 14/13).
- Level upload: the real ShareLevelLayer captured (a verified level's page in the fake-account home, via the mod's `sharelevel` + /thenframe) → `scenes/share.txt`, `ui/share.rs` (request stars with the difficulty face, notify friends/followers). uploadGJLevel21 recorded (`call:upload`) and matched in `gjapi::upload_form` (seed2 = chk of 50 evenly spaced level-string characters, salt xI25fpAapCQg, key 41274; ct = time; bs as the game sent it). Uploading needs an account, objects ("You cannot share an empty level.") and verification: completing the level in normal mode from its page; any edit unverifies it. The level's id and version are kept for updates. GUI diff vs real 11.0.
- Saved levels (`saved.rs`): every played online level is kept (desktop: Cadence's `saved/` folder; web: localStorage) with its page's details, and plays from there unless Refresh asks for a new download. The creator menu's Saved list shows them (favorites first); the level page's Delete (confirmed), Refresh, Clone (only when the level's copy password, download key 27 XOR "26364", is the free "1"), Options (low detail per level), favorite and folder work on them.
- Online search: User Search (getGJUsers20 → the best match's profile), Lists (level lists by name), Advanced Options (filters: uncompleted/completed/featured/original/2-player/coins/no star/epic/legendary/mythic → the search's fixed fields plus &epic=1 … &completedLevels=(…)); the popup isn't laid out from a capture yet (the harness pressed the wrong button). The Safe: past dailies/weeklies/events (search types 21/22/23).
- My Levels: My Server (your uploaded levels, search type 5 by your player id), Import (drop a `.gmd` on the window; desktop), folder filter, find, Select All → delete (confirmed), Verified / Unverified per level.
- Editor: pause menu side buttons (Settings → the options' Editor page, Export → a `.gmd` in Downloads / a browser download, Auto-Save every 5 min, Get Audio, Editor SFX / Death Stats / Help texts); the music button plays the level's song from the camera's x (311.58 units/s); grid toggle, wrench (editor options), the color button (Edit Object on the selection). Your level's page: delete (confirmed), folder down/up, duplicate, export, death stats, options (LDM per level), info, emotes. Level settings: font (kA18 cycles gjFont01…59), Normal/Custom song (custom id typed in the post box, sent with uploads), 2-player (kA10), the color channels explain color triggers (a working color picker is still to do). Verification only from the level page's Play (never the editor's playtest), using the level's own song.
- Icon kit: the trail (`player_special_NN`) and death effect (`explosionIcon_NN`) pages draw their icons and select like the others.
- Trigger popups: 66 triggers had none (the enter effects, camera mode/edge, area stop, edit/re-target advanced follow, keyframes' neighbours, old enter-effect objects 22–33 / 55–59, …): captured from the real editor in one harness run (`editlevel:0/edit:object:<id>`, logged-out home) for `tools/trigger_popups.py`. Edit Special in the real game (`FUN_100015d34`, EditorUI::editObject2's body) opens popups for 1594, 1704, 1751, 2063, 2065 (CreateParticlePopup), 2902, 2903, 2904, 3027, 747, 1615: the missing ones are next.
- Color popups: the picker drags like cocos2d-x's CCControlColourPicker (the capture's knobs fix its numbers: hue ring 150 across at (208.5, 121), knob 60 from its center, angle + 180°; saturation/brightness square 110 across at (227.5, 141), s and v over a 55 box 35 in, held to the 55 circle) and writes the trigger's R/G/B (keys 7/8/9); the square takes the hue's tint.
- Level page: Rate Difficulty for unrated levels (1–10 stars → rateGJStars211, its chk verified). Web: a `.gmd` dropped on the page while playing goes into your levels (web::import_level_file → the app's next frame).
- Legacy messages data (`social::messages` / `message`, `parse_messages`: body base64 of text XOR "14251"); the page itself next, from a capture (harness: `/then2label:`, `/then3label:` press labelled buttons, for Messages → the rules notice's OK → Legacy).
- Trigger popups: 64 more converted from the real editor's popups (TRIGGER_POPUPS 62 → 126: enter effects 3017–3023 / 55–59, camera mode 2925, edit/re-target advanced follow 3660/3661, area stop 3024, shaders 2904–2924, MG 2999, art switch 3030/3031, end 3600, reset 3618, persistent item 3641, BPM 3642, collision state 3655, progress 3663, start pos 31, the color-select ones 30/744/900/915 …). Screenshot hook `CADENCE_SCREEN=trig<id>`; GUI diffs vs real ~8–9 (the real shows decimal inputs as "1.00": the converter drops the captured value labels, so Cadence can't tell decimal fields yet).
- Edit Special popups for 1594, 1704, 1751, 2063, 2902, 2903, 3027 (SPECIAL_POPUPS 2 → 10) and the particle editor's layout (2065, no fields mapped). 2904 opened no popup in the harness.
- Messages: the real "Old Messages" page (MessagesProfilePage, captured with the harness's new `/then2label:` + `/then3label:` presses through the threads page's rules notice and Legacy button): rows as captured (check box, subject with the unread "!", From:, age, View, X), the envelope switches to sent ones, View opens the text with Reply, X deletes (deleteGJMessages20). GUI diff vs real 16.6. Notifications: the real NotificationsPage frame, getGJNotifications.php, its "Something went wrong" / empty states (the answer's fields aren't mapped; the page shows how many).
- Merged the triggers branch (advanced follow, keyframes, enter effects, camera triggers, the small ones; real-game trace checks; the edge-fade margin fix: objects wider than 30 fade from half their width − 15). Conflicts only in app.rs's song/SFX trigger playing (kept the web-ready names and the branch's music channels / trigger effects) and this journal. After the merge: 52 core + 43 game tests pass, web builds, verify_all 111/126 bit for bit (the same 15 visual-rotation drifts as before).
- Share: Options cycles public / unlisted / friends only (the upload's unlisted 0/1/2), "!" shows the game's rules text. Rate Demon on a rated demon's page (easy … extreme → rateGJDemon21 1–5).
- Comment History from your profile (getGJCommentHistory with your user id → the comments popup's rows); its own layout wasn't captured (the harness press didn't open it), so the frame is the comments popup's.
- Platformer: `m_slopeAngle` is the ridden slope's signed angle (slopes.rs sets it on landing), used for the speed cap on slopes; verify_all unchanged at 111/126 (no platformer-slope trace yet).

## 2026-11-05 night (death agent): death effects, editor guides, trigger residuals, slope trace

Worktree branch; decompiles from a scratch copy of re/ghidra; harness runs with this agent's own
build of the mod and its own copy of the game app (so the main session's copy and mod stayed
untouched), home re/home (re/home-anon for the editor).

- **Death effects 2–20** (`death.rs`): `playDeathEffect`'s 19 cases read from the instructions
  with a small symbolic tracer over the disassembly (calls with their float arguments, the
  per-case tails they jump into, CCCircleWave field stores). Sprite (its sheet loaded only when
  chosen; the web reads all 19 sheets' files), frames, delay, fade, tint, scale, rotation, blend;
  particle system settings; both circle waves (first at z 1000 over the T layers, the second a
  10-pixel ring); the shatter (not for 20). Found: 12–15 build a fade they never run; the
  default effect's particles have node scale = size (Cadence had 1); cocos actions start a frame
  late; ring widths are framebuffer pixels (spider dash rings now 2 pixels too). Practice mode
  uses the default unless option 0100. Checked: harness `deatheffect` / `deathshot` captures of
  all 20 on Stereo Madness, `CADENCE_CHECK_DEATH` steps each effect with the real rolls:
  **20/20 match** in sprite frame, opacity, color, position, blend, both waves' radius / alpha /
  color / ring / opacity mod and the particle settings at every captured frame. Screenshots
  (`compare_shots.py`) differ only by the random rolls and particles (mean diff 2–6 per channel).
- **Editor guides**: Camera Guide 2016 rectangles and player line, BPM 3642 beat lines (with
  `show_bpm_markers`, off by default as GameManager's `showBPMMarkers`; nothing in Cadence toggles
  it yet: the real toggle is the edit tab's `edit_eBPMBtn`). Real editor captured with the mod's
  new `edit:string:<file>` (objects from a level string, nothing selected, BPM markers on);
  Cadence with `CADENCE_EDITOR_LEVEL` / `CADENCE_EDITOR_CAM`: the guides land on the same pixels
  (gui_pair mean diff 1.9 over the guide's area). Not drawn: the real editor's orange
  (255, 150, 0) playback-position line at the level's stored x (LevelEditorLayer +0x38f8), and
  Cadence draws its guides over the objects (the real DrawGridLayer is under them).
- **Trigger residuals**: the game's `fast_rand` for random 1912 / advanced random 2068 /
  advanced follow (the game seeds from the clock; `fastseed N` in specs fixes it in both: a test
  level of three random triggers matches for seeds 1 and 2, and the triggers log shows the
  predicted picks); camera key 370 (corridors unsnapped: real corridor centers 305 / 245 with
  370 portals, Cadence's trace matches step for step; the camera mode trigger's own 370 only
  matters when no portal sets it after, not traced); area tint HSV mode (278) ported and area
  triggers now fire by position (they're object type 45 and the order walk only took type 20:
  position-placed area triggers never ran in Cadence). The HSV-tinted blocks were captured
  (re/traces/areahsv): the shift is there in both, the comparison is muddied because Cadence
  draws P1-colored blocks paler than the game even outside the area (not looked into).
  Enter tint's HSV mode was already ported. Not done: group parents / linked objects, keyframe
  curves / reference-only / close loop, edit SFX / song speed and proximity.
- **Platformer-slope trace** (re/traces/pfslope: 45° and 2:1 slopes up and down, two attempts):
  Cadence matches until step 259, the second step on the first uphill slope: the real game slows
  faster (x per step 1.306 → 1.237 → 1.170 → 1.104, about −5 % a step and rising; Cadence
  −1.1 %, its 0.05 decel). The max-speed cap by cos(m_slopeAngle) is right (both settle toward
  0.924); the real decel there is about 0.23 per 60 fps unit, as if the momentum path (0.2) or
  something else applies on landing on an uphill slope. Not fixed. Also seen: the real
  m_slopeAngle stays set after leaving a slope.
- verify_all: **114/131** (the original 126 at 111 as before; new suites death, rand, areahsv
  pass; cam370 matches every position but its ship start's visual rotation at step 0 (as other
  ship-start levels); pfslope fails as above). `cargo test --workspace --release` green (53 core,
  47 game).

## 2026-11-04/05 — v0.0.2: shops, paths, chests, treasure room, keybindings, particle editor

- Shops: `ui/shop.rs` — all 283 items from `GameStatsManager::createStoreItems` (insertion
  order = the shop's order), eight a page; `PurchaseItemPopup` (captured) for Mana Orbs (stat
  14) or Diamond Shards (stat 29, the spendable diamonds; 13 stays the total); bought items
  darkened in the shop's color (`darkenStoreItem`'s table) and kept like `GS_6`. The community
  shop's room is `CCSpriteWithHue`: a hue turn in the sprite shader (`Look::hue`, 1000 + degrees
  in the triangle-flag slot); the real pixels are the channels turned by exactly −120°.
- Paths of Power (`ui/paths.rs`): store items 121–130 (25,000 orbs, "unlock" wording), path
  stats 30–39 up to 1000, the active path (`trySelectActivePath`), ranks = the path
  achievements' rewards, the chest at 1000 (`pr_N`, dumped). Harness `dump:chests` writes every
  special and treasure-room chest; `chests_table.rs` is generated from it.
- `RewardUnlockLayer` (`ui/reward.rs`): drop with bounce, shakes, open, rewards out one by one,
  the sounds at their times; `AchievementBar` banners for newly unlocked achievements.
- Treasure Room (`ui/treasure.rs`, `scenes/treasure0–4.txt` built from five captures): tiers
  cost 1/5/10/25/50/100 keys, bonuses at 50/100/200 opened, the gold chest (stat 43), ropes to
  the shops. The door opens at 5 keys.
- Keybindings (`ui/keybinds.rs`): `dump:keybinds` gave the 214 actions, names and default
  chords; the tab table is `actionsToShow`'s at 0x1008fa5d8. Capture popup, settings popup
  (Export / Import / Reset). Play and the editor go through the bindings.
- Shards of Power (`ui/shards.rs`): the shard achievements (5/15/35/65/100) are the icons.
- Trigger popups' sliders: the n-th L line runs along the n-th groove (the L coordinates from
  the keys dump are not the groove's).
- Particle editor (`ui/particle.rs`): the harness `probe:N` sets every input to 100 + index and
  flips the toggles with bit N, presses OK and reads the object back — the field map for all
  four tabs (Extra: 41–44, 59–62, 68–71; toggles 52–56, 58, 63–67; keys 146/147/123/214/211).
- Level leaderboards: the chk is account, level, percent, attempt time, clicks, attempts, three
  zeros (time / plat / points on classic levels), the daily id, then `yPg6pUrtWn0J` + s7, xor
  "39673" — pinned with a recorded request whose fields were all different
  (`CADENCE_LB_VALUES`).
- Following / Muted are `getGJUserList20` types 2 / 3; follow, unfollow and mute have their own
  endpoints (strings at 0x1008… next to `getGJFollowing.php`).
- Harness: `/late:N`, `@last` for frame presses, `/then3frame:`, presses skip achievement
  banners, `paths…`, `shop…$`, `shards$`, `secretrewards/…`, `keys:add/edit/settings`,
  `/toggleall`, `/probe:N`, `dump:chests`, `dump:keybinds`.

## 2026-11-05 (gameplay agent): slopes, keyframes, camera 370, dual deaths, glows, editor guides, ship fires

Worktree branch on top of main (bc818c1); own harness build, game copy and home in this
session's scratch (corner, muted); decompiles from the scratch Ghidra copy.

- **Platformer slopes**: forced slides past 40°, the platformer slope launch, the slide bonus
  ending, platformer slope direction by velocity, preSlopeCollision's platformer walls, no
  rotation easing the step after a slope. re/traces/pfslope matches (2/2, rotation bit-exact);
  the rotation fix also fixed s12–s16-22.
- **Keyframes**: curves (tk::spline), reference-only runs and time modes, close loop. New real
  trace re/traces/kfcurve (watchgroups 5/6/10/12): positions to print precision; a turning object
  drifts ≤ 1.5e-4 in the real game (also in the old kf2 trace); the spawned move's every-4-steps
  batching as known. tools/cmp_watch.py compares watchgroup files.
- **Camera mode 370**: updateDualGround on a free-mode change, the ceiling store after the
  trigger walk; re/traces/cam370t bit-exact incl. camera.
- **Dual deaths**: player 2's death explodes player 2; the other leaves with the exit-dual copy;
  0.05 s death fade. Checked against real deathshot frames (re/traces/dualdeath, harness:
  deathshot on player 2's death too). The physics capture of that level diverges at the dual
  portal at 60 Hz (yvel −0 vs 0 for player 2, then player 1 falls) — not looked into; the capture
  isn't a suite.
- **Glows** take the object color and blend additively (area HSV blocks no longer pale).
- **Editor**: guides drawn under the objects; music playback line (green, per the decompile).
- **Explode toggle** (0153: no shatter), **ship fires 2–6** (motion streak), **edit song / SFX**
  tweens and proximity volume.
- Harness additions: `gamevar <key> <0|1>`, `shipfire N`, deathshot on player 2's death.
- Not done: group parents / linked objects (findings in NOTES); SFX speed and per-sound
  proximity (audio backend); the editor's preview-lock line.
- verify_all **122/133** (was 114/131): +pfslope, s12..s16-22, +kfcurve, +cam370t;
  `cargo test --workspace --release` green (57 core, 55 game).

## 2026-11-05 (parents agent): group parents and linked objects in advanced follow and area triggers

Worktree branch on main (f0f3f77); own harness build, game copy and home in the session scratch
(corner, muted); decompiles from a scratch Ghidra copy (`FUN_1000d64d0`, rotateObjects,
rotateAreaObjects, transformAreaObjects, resetAreaObjectValues, `FUN_1000fb3a4`, tryGetMainObject,
tryGetObject, loadGroupParentsFromString).

- `parents.rs`: keys 274 / 34 / 279 / 108 and the clusters per (target group, 280, 281);
  `World::main_object` (tryGetMainObject), `rotate_objects` (rotateObjects), `unmodified_pos`.
- Advanced follow: alone objects, then leaders moving their clusters; a turning cluster moves
  twice its step (game code, confirmed by the trace). Targets are the group's main object;
  560 / 565 use tryGetObject.
- Area triggers: clusters for move / rotate / scale / fade / tint; scale, rotate, move order;
  stale offsets reset at the end of the pass; the center is the main object or (0, 0).
- New real trace `re/traces/gparent` (watchgroups 10, 11, 20, 21, 30, 31, 41–44: linked
  cluster with a 34 leader, linked without a flag, a 279 leader, an explicit 274 parent, 280,
  a child-group parent pulling an object outside the target group, area move with linked / 274
  clusters, area rotate with a 274 cluster and alone): every position to print precision
  (≤ 6.6e-6), rotations ≤ 1.5e-5 at frame steps. verify_all compares watch files now for
  suites with a `watch.args` (cmp_watch.py options; cmp_watch.py got --fields, --rot-frames and
  an exit status).

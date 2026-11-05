# Notes: how Geometry Dash 2.2091 works

Technical findings, each with where it came from (binary analysis, level data, or traces).
Facts from the community that haven't been checked against the binary or traces yet are marked
**unverified**.

## The macOS build

- Universal binary (x86_64 + arm64). The arm64 slice is used for analysis.
- Stripped: no defined symbols. C++ RTTI names remain (`12PlayerObject`, `15GJBaseGameLayer`, ...).
- Engine: cocos2d-x 2.x (statically linked; kazmath build files even ship in Resources).
  Audio: FMOD (`libfmod.dylib`). Steam: `libsteam_api.dylib`, app id 322170.
- Version check: Geode's 2.2091 `m1` addresses for `PlayerObject::updateJump`,
  `PlayerObject::pushButton`, `PlayerObject::playerDestroyed` and `GJBaseGameLayer::update`
  all land on function prologues; 2.209's land mid-function.

## Files in Resources

- `levels/<id>.txt`: official levels. IDs 1–22 (main), 3001, 5001–5004 (Tower).
- Songs for main levels: `*.mp3` named after the song (e.g. `StereoMadness.mp3`).
- `songs/` (9 `.ogg`, numeric IDs) and `sfx/` (260 `.ogg`, `s<id>.ogg`): bundled library audio.
- `LevelStats.dat`: plain text `id,1,id,1,...` pairs.

## Save files (macOS)

- `~/Library/Application Support/GeometryDash/CCGameManager.dat`, `CCLocalLevels.dat`.
- `-[NSData(AES256) makeCryptedVersionWithKeyData:ofLength:decrypt:]` (m1 0x226da4) calls
  `CCCrypt(op, kCCAlgorithmAES=0, options=3 (PKCS7|ECB), key, 32, iv=NULL, ...)`: AES-256-ECB.
- Key `ipu9TUv54yv]isFMh5@;t.5w34E2Ry@{` (community-documented; not stored as a plain C string).
  **Verified:** decrypts Mark's real saves to valid plist XML, and re-encrypts byte-identical.
- No inner layer on macOS: the plaintext is the XML directly (Windows: XOR 11 → base64 → gzip).
- Plist dialect `<plist version="1.0" gjver="2.0">`: `<k>` key, `<s>` string, `<i>` int, `<r>` real,
  `<t />` true, `<d>`/`<d />` dict. Arrays are dicts with `_isArr` = true and keys `k_0`, `k_1`...
- `CCGameManager` top level has 90 keys (`valueKeeper`, `playerFrame`, `bgVolume`, ...).

## Physics (from harness traces, Stereo Madness, 2026-11-02)

- **240 physics steps per second.** `processCommands` receives dt = 0.00416666688 (f32 1/240).
- `PlayerObject::update(dt)` runs once per step with **dt = 0.25** (time in 1/60 s units).
- Per-step loop in `GJBaseGameLayer::update` (m1 0x10663c), in order: button handling
  (`buttonIsRelevant`), `updateSpawnTriggers`, `PlayerObject::updateTimeMod`, **`processCommands`**,
  ring/collision-log resets, `preCollisionCheck`, move/rotate/follow trigger actions,
  `PlayerObject::updateInternalActions`, **`checkCollisions`** (per player), `updateSpecial`,
  `updateRotation`, `updateSavePositionObjects`, `postCollisionCheck`, **`checkRepellPlayer`**
  (end of step), state triggers, tweens, `updateCamera`.
- **True physics position = the node position (`getPosition()`)**, updated every step.
  `PlayerObject::m_position` is a once-per-rendered-frame copy; `GameObject::m_positionX/Y`
  are unused for the player.
- Cube at 1x speed: x += **1.29825044** per step (f32). Not exactly 5.77·0.9·0.25 = 1.29825;
  the exact formula is still open (needs the decompiled `updateMove`).
- Jump: y velocity **11.18** on press (first recorded step shows 10.964 = 11.18 − 0.216).
  Gravity −**0.216** per step (double; values are exact decimals). y += yVel × **0.225** per step.
- Rotation while airborne: +1.7307692° per step.
- Standing on the ground each step: gravity pulls y to 104.9514, then collision snaps it to 105.
- Without input the cube dies on Stereo Madness at step 390, x = 507.615.
- The real game is deterministic under the harness: repeated runs give byte-identical traces.

### Exact formulas (verified bit-for-bit against traces; source: decompiled updateJump/updateMove)

- **Velocity rounding** (inlined ~25 times): `i = (int)v; v = (i == v) ? v : round((v - i) * 1000) / 1000 + i`
  with `round` = `frinta` (half away from zero). Ghidra shows it as a `(long)` cast; it is a round.
- **x per step** (`updateMove`): `x = f32(x + m_unk788 * (double)m_playerSpeed * dt)` where
  m_unk788 = (double)5.770002f = 5.7700018882751465 and m_playerSpeed = 0.9f at 1x.
- **Cube gravity** (`updateJump`): `g = 0.958199f * m_gravityMod` (f32);
  `yVel = round3(yVel + (double)(f * -(g * dt)))` with f = 0.9 for the cube (1.0 / 0.6 in other
  states; the f32 product matters). The obvious `0.216` per step only matches because rounding hides
  the difference.
- **Jump**: `yVel = round3(f32(jumpAccel) * sizeFactor * gravityDir)`; sizeFactor 0.8 when mini.
  Cube normal gives 11.18.
- **y per step**: `y = f32(y + yVel * dt * (double)m_playerSpeed)`.

### Collision order (`GJBaseGameLayer::checkCollisions`, decompiled)

1. `collisionCheckObjects` walks nearby objects: skips decoration (type 7) and no-touch objects;
   **collects solids** (type 0 and 0x15 breakable) and **hazards** (type 2 and 0x2f animated
   hazard) into lists; handles everything else (portals, pads, rings) immediately with rect /
   circle / OBB tests, then a switch on type.
2. **Solids first**: each solid whose rect touches the player rect (inclusive edges) →
   `PlayerObject::collidedWithObject`; the player rect is recomputed after each one.
3. **Hazards second**, against the updated player rect (OBB overlap for rotated hazards).

### Object table (`crates/cadence-core/src/objects/table.rs`)

- Generated from the real game: a synthetic level containing every id (4,093 from
  `createWithKey`) loaded into PlayLayer and dumped. `createWithKey` alone is not enough: types and
  hazard hitboxes are applied when objects join a level (a bare spike reports type 0, 30×30).
- Hazard hitboxes are a fraction of the sprite: spikes 0.2 × 0.4, ground spikes 0.3 × 0.4.
- Flip then rotate (90° steps, clockwise positive) transforms the rect; matches all 1,401
  collidable Stereo Madness objects. Level y + 90 = game y.
- Secret coins (142) are dropped from non-official levels, so the table takes them from a main
  level dump. Id 0 is the game's own end-of-level object.

### Sprites

- `sprites-level.tsv` (harness `dumpsprites` on the synthetic level): every object's sprite tree
  with frame names, offsets, anchors, scale, rotation, z, flips, blend. Children are only added
  when objects join a level (4,092 main, 151 detail, 1,408 child sprites).
- Default color channel for most objects is 1004 (OBJ).

### Ship (in progress)

- Ship portal (type 5): `canBeActivatedByPlayer` → `playerWillSwitchMode` →
  `switchToFlyMode(layer, player, portal, 0, 5)` → `PlayerObject::toggleFlyMode(true)`:
  sets m_isShip, clears other modes, **yVel *= 0.5 (unrounded)**, rotation 0, not on ground.
  Then `updateDualGround` (vtable +0x508).
- Corridor height by mode (`updateDualGround`): 300 ship/UFO/wave/swing, 240 ball, else none.
  `animateInDualGroundNew`: bottom = portal.y − h/2, floored to a multiple of 30 (unless free
  mode), at least 90; layer+0x308 = bottom + h/2 (center). The ground slides in with a GJGameState
  tween (id 0x19, `m_gameState.m_tweenActions` at layer+0x3f8, progress float at +0x3f4) over
  0.4 or 0.5 s, and the camera moves to the corridor center (`updateStaticCameraPos`).
- Floor/ceiling collision (`checkCollisions`) reads the ground layers' *current* node positions
  through the camera: ceiling = cameraY + (groundLayer2.y + 1) / zoom, floor similar with −1.
  So exact ship-corridor collisions need the camera and the tween ported.
- Tweens (`GJGameState::updateTweenActions`): elapsed += dt; t = min(elapsed/duration, 1);
  eased by `GameToolbox::getEasedValue(t, type, rate)`; value = from + t·(to − from); dispatched
  by tween id 1..25. Advanced once per physics step.
- **Ship velocity** (`updateJump`, normal gravity, size factor s = 1 or 0.8 mini, g = 0.958199):
  - holding: yVel += k·g·dt / s with k = 0.5 while yVel < 2g, else 0.4 (dir −1.0 when not
    `m_isAccelerating`; 0.8/−1.0 by sign of yVel when accelerating)
  - released: yVel −= 0.4·f·g·dt / s with f = 0.8 while yVel < 2g, else 1.2
  - each result rounded (round3); then clamp yVel to [−0.8·8/s, 8/s] = [−6.4, 8] and round,
    unless `m_isAccelerating` (pads/orbs beyond range; cleared when back in range).
- UFO (`m_isBird`) shares the 0.8/1.2 factors with its own ring-jump handling.
- Non-cube modes use the literal 0.958199 for gravity; the cube uses the `m_gravity` field.

### Frame timing (`GJBaseGameLayer::update`)

- total = f32(extraDelta + frameDt); processed = (1/240)·(int)(total·240); extraDelta = total −
  processed; steps = (int)(processed·240). frameDt comes from the display link on macOS, so at
  60 Hz every frame runs exactly 4 steps. Cadence snaps frame times within 25% of the display
  period to it, for the same steady stepping (wall-clock jitter caused visible shaking).

### Rotation (visual; `PlayerObject::updateRotation`) — bit-exact

- Runs once per step after collisions (`GJBaseGameLayer::update`: player update → checkCollisions
  → updateSpecial → updateRotation(dt 0.25) → save position as the "last position"). Not on a
  step that kills the player.
- Easing is a **quaternion slerp of half angles** (m1 0x216db8): sincos(a/2), sincos(b/2),
  dot = fmadd(ca, cb, sa·sb), flip sign if negative, plain lerp if 1 − dot ≤ 0.0001, else
  sin-weights from acosf; result `2·atan2f(fmadd(wa, sa, sb·wb), fmadd(wa, ca, cb·wb))`, so angles
  live in (−360°, 360°]. Inputs in radians (deg × 0.017453292), output × 57.29578. The FMAs
  matter: without them ~1 ulp differences appear.
- Cube on the ground (touching, yVel·dir ≥ −0.25): ease toward `convertToClosestRotation` by
  t = min(dt, speed·0.175·3·dt) = 0.118125.
- Spin (`runRotateAction`): +180°/0.43333 s (1.7307692°/step at 1x), sign fixed when the spin
  starts — a later gravity flip doesn't reverse it. Started by: a ground jump (press or held),
  an orb, a pad (propellPlayer), entering cube mode from a portal, and falling past |v| 0.25.
  No angle normalization anywhere (my earlier "normalize at spin start" was a workaround for a
  wrong easing function).
- Ship (`updateShipRotation`): only if dx² + dy² ≥ 1.2·dt (dx, dy since the saved position);
  target atan2f(−dy, dx) in radians; t = 0.15·dt = 0.0375 — no speed factor.
- Verified: every recorded trace (SM, BoT, Polargeist; 227 attempts) has bit-identical rotation.

### updateJump state machine (cube and ball; from decomp + real-game field dumps)

- Harness traces now also record `m_touchingGround`, `m_isRotating`, `m_rotationSpeed`
  (columns 15–17) — use them when rotation or ground state is unclear.
- Paths: jump_buffered && on_ground → jump (sets m_isRising, clears on_ground and
  m_touchingGround and m_stateRingJump); else if m_isRising → *rising branch*: gravity only, no
  terminal clamp, no spin check; the rise ends (and m_touchingGround clears) once the velocity
  is past 2g toward gravity. Else → *gravity branch*: on_ground cleared unless rising faster
  than 2g; gravity; clamp ±15; cube starts its spin if falling faster than 0.25 and not
  spinning; m_touchingGround clears when falling faster than 4.
- The "past 2g" tests aren't mirrored upside down (`m_fixGravityBug` off, all official levels):
  upside down they compare `2g < v` instead of `v < -2g`. Consequences: an upside-down cube that
  walks off a block keeps its on-ground flag (and touching flag) until v > 1.916; after an
  upside-down jump the rise lasts until v > 1.916.
- m_isRising is set by jumps, ball flips, orbs and pads.
- Cube rotation (`updateRotation`): eases to the nearest 90° only while m_touchingGround and
  not moving away (v ≥ −0.25, upside down v ≤ 0.25); otherwise only m_rotationSpeed turns it.
  hitGround stops the cube's spin (then it eases).
- Upside-down cube touching the bottom ground (checkCollisions' isSafeFlip branch): placed on
  the floor, velocity 0, on-ground if gentle, spin stopped, m_touchingGround NOT set, and the
  rest of that collision pass is skipped.
- hitGround's "gentle landing" test is velocity·dir ≤ 5 (along gravity).

### Mode switches, ball, mini

- Velocity halves (unrounded) when entering or leaving ship/UFO/wave/swing (toggleFlyMode etc.),
  not for ball/robot/spider. Leaving the ship: rotation 0, off the ground, and (ship → cube) the
  cube spins at once. Leaving the ball: rotation 0 (180 upside down), rotation stopped.
- Corridor heights (`updateDualGround`): ship/UFO/wave/swing 300, ball 240, spider 270; floor =
  max(90, (int)((portal.y − h/2)/30)·30).
- Ball (`updateJump`, `flipGravity`, `runRotateAction`): gravity factor 0.6 of the fixed
  0.958199 (cube uses the per-speed m_gravity). Jump on the ground (press, or a buffered press
  on landing): v = round3(sizeFactor·m_yStart·dir), flipGravity (×0.5, ball spin), ×0.6
  (unrounded), m_jumpBuffered cleared (holding doesn't flip again; orbs need a new press).
  Rotation: rolling ±120°/0.2 s (mini 0.16) set on landing; after a flip or orb ∓340°/(0.8·size)
  per second (m_ballSpinning) until the next landing. Orbs ×0.7 and clear m_jumpBuffered; pads
  ×0.6. Pink pad power 0.7.
- Mini (types 17/18, ids 99/101; `togglePlayerScale`): m_vehicleSize 0.6; the hitbox scales at
  once (sprite eases 0.5 s). Size factor 0.8 for jumps, pads and orbs; the ship's thrust and
  limits divide by it. Cube spin 180°/0.33333334 s. Jump snapping uses the mini table.

### Speed, UFO, hazards, camera (levels 8–15; all positions bit-exact)

- Speed portals (ids 200/201/202/203/1334 → 0.7/0.9/1.1/1.3/1.6) are touch-triggered modifiers
  (type 20) fired once. `updateTimeMod` sets three doubles per speed: base x speed (0x788),
  m_yStart, m_gravity — 0.7: 5.980002 / 10.620032 / 0.940199; 0.9: 5.7700018882751465 /
  11.180031776428223 / 0.9581990242004395; 1.1: 5.870002 / 11.420032 / 0.957199; 1.3 and 1.6:
  6.000002 / 11.230032 / 0.961199. Level start speed: kA4 (0 normal, 1 slow, 2 fast, 3, 4).
- **Vertical motion ignores speed**: PlayerObject::update calls updateJump(dt·0.9) and moves y
  by v·(dt·0.9) always; only x uses base·speed·dt. The cube's gravity is the per-speed m_gravity;
  ball/ship/UFO use the fixed 0.958199.
- UFO (type 19, id 111; `updateJump` bird branch): a fresh press (m_stateRingJump &&
  m_jumpBuffered) sets v to 7·dir (mini 8·0.85) if below it; gravity ×0.8, or ×1.2 once past
  2g (upside down: ×1.2 unless past +2g); delta = (k·g·dt·−0.5)/s with s = 0.85 mini.
  Flying modes clamp to [−6.4/s, 8/s] unless m_isAccelerating (red orb), and use size factor
  **0.85** (not 0.8) when mini. The ship's "below 2g" test upside down is `2g < v` (unmirrored).
- Block contacts in flying modes (ship/UFO): snap 6 (else 10). Landing side as usual; the far
  side slides when moving into it and is harmless when moving away. The deadly inner box is
  always 9×9 (getObjectRect(0.3, 0.3) — confirmed from the game's own rects, mini included).
- Mode switch velocity: ×0.5 leaving and again entering a flying mode (UFO → ship ×0.25).
- Gravity pads only act if facing the current gravity (`isFacingDown == upside_down`;
  facing down = flipY XOR rotated 180°). Yellow/pink pads have no facing check. Pad powers:
  pink ship 0.35, UFO 0.4, ball/spider 0.7, else 0.65; red pad 1.25 (ship 0.63, UFO 0.6; mini
  0.95/0.98).
- Breakable blocks (type 21, id 143) collide as solids; where a solid would kill, the block is
  destroyed instead.
- Round hazards (sawblades: radius > 0, scaled by max(scaleX, scaleY)) use
  `objectIntersectsCircle`: center inside the player rect, or a rect corner strictly within the
  radius (edge crossings don't count). Hazards are gathered by section, so the lookup must
  reach a radius past the rect.
- Mini (togglePlayerScale): the hitbox is 18 at once; the sprite scale eases
  CCEaseElasticOut(CCScaleTo(0.5)) with period 0.3 (cocos step: first tick at elapsed 0;
  exp2f/sinf/fmadd as in the binary) — visual only.
- Camera: x = player.x − (view_w/2 − 75), ≥ 15. y follow band 70 above / 40 below the center,
  swapped upside down (targets y−230 / y−120, upside down y−200 / y−90), eased by /40 per step.
  Corridor modes: `updateStaticCameraPos` tweens the camera *center* to floor + height/2 over
  0.5 s (0.4 s when switching between corridor modes), ease-in-out rate 2 via the system powf
  (LLVM must not fold powf(x, 2) into x·x — 1-ulp difference), value = fma(to−from, t, from),
  elapsed += f32(1/240). Leaving a corridor needs no tween. Bit-exact on all traces except the
  level-end camera (eases to a fixed end position; not ported yet).

### Mirror portals, music, level end

- Mirror portals (types 14/15, ids 45/46) are touch-activated once and only visual:
  `toggleFlipped` tweens the flip (0→1) over 0.4 s, ease in-out rate 1.2; the world is drawn
  scaled by 1 − 2·flip around the screen center, the background parallax accumulates camera
  movement × (1 − 2·flip). Starting from exactly 0/1 the tween starts at 1e-5 / 0.99999.
- Music: `LevelTools::getAudioFileName` — level N plays track N−1 (StereoMadness.mp3 …
  Dash.mp3; xStep.mp3 for 10), The Challenge track 26 (DJRubRub.mp3), the Tower levels
  songs/10003039.ogg, 10003129, 10002875, 10006086. Song offset = level setting kA13. Death:
  explode_11.ogg; level complete: endStart_02.ogg.
- Level end (`PlayLayer::playEndAnimationToPos`): static camera y over 1.2 s (rate 1.8); the
  player flies a 1 s CCBezierTo (ease-in 1.2) from its position via (x + 40, end.y + 150) to
  (end.x + 50, end.y − 20) while rotating 360° (ease-in 1.5), then levelComplete. The camera x
  settles at level_length − view width (real traces: about 1% of the distance per step; the
  exact end-camera code isn't ported — Cadence approximates it).

### Slopes (in progress — Hexagon Force onward)

- Slope objects are type 25. `determineSlopeDirection` (flipX, flipY, rotation) → direction
  0–7 (m_slopeDirection, 0x43c) and m_slopeUphill (0x438); ported as `world::slope_direction`
  and checked against the game's own values for all 617 slopes of Hexagon Force (0 mismatches).
  slopeFloorTop: directions 1/3/5/6 (0x6a mask); slopeWallLeft: 2/3/4/6 (0x5c).
  m_slopeIsHazard (0x440) adds ±4 to the surface height (−4 for floor-top types).
- Surface (`GameObject::slopeYPos(x)`, doubles): k = rect.h/rect.w; uphill: minY + k·(x − minX)
  (else maxY − k·(x − minX)); x past maxX uses the other edge formula.
- Slopes are processed in collisionCheckObjects (the special-object pass, section order),
  case 0x19 → PlayerObject::collidedWithSlope → collidedWithSlopeInternal (0x370af4, ~900
  lines; preSlopeCollision first). Decompiled copies: re/decomp-named/PlayerObject__*Slope*.
- Per step: FUN_10036e6dc at the start of checkCollisions copies m_isOnSlope → m_wasOnSlope and
  clears m_isOnSlope (also clears collidedTop/BottomMaxY at 0x910..0x928); FUN_10036e834 at the
  end handles leaving a slope: launch with m_slopeVelocity × time factor (10·(t − slopeStart),
  clamped to 0.4..1, 1 after 0.1 s) via FUN_10037013c, or on downhill exits set
  yVel = round3(±m_currentSlopeYVelocity), drop the ground flag and start the spin.
- Fields to model: m_isOnSlope, m_wasOnSlope, m_slopeVelocity, m_currentSlopeYVelocity,
  m_slopeRotation, m_isCurrentSlopeTop, m_collidingWithSlopeId, m_slopeStartTime,
  m_yVelocityBeforeSlope, m_lastSlopeSnap*, m_slopeIsDownhill. Trace columns 23–30 record the
  real values (on_slope, was_on_slope, slope_vel, slope_yvel, colliding_slope, slope_angle,
  slope_top, rising).
- Real data: re/traces/v2-16, v3-16 (Hexagon Force). v2-16 is bit-exact (ship and ball slope
  rides, launches, slope jumps, ball roll).
- Ported (crates/cadence-core/src/slopes.rs): snap height (surface at the player's x ±
  (h/2)/cos(angle), clamped to the slope's flat ends), landing side vs far side, the far side
  pushing the velocity away (ship: v=min(v,0); −2 push when "downhill"), hitGround zeroing the
  velocity (hard landings keep it), slope velocity = (h/w)·xspeed·1.4·min(0.8/angle, 1.1)
  (×0.75 flying/ball), slope rotation, the exit launch (FUN_10037013c: rising, accelerating,
  velocity, cube back-spin 180°/0.8667 s), the jump bonus (+¼ slope velocity, cap 1.4×), ball
  roll ×1/cos(angle) (flipGravity clears it).
- A slope's flat wall/base meeting the player first (preSlopeCollision strips) only kills the
  inner box — never a landing surface (real ships ride past them).
- Blocks the current slope runs into (slope surface within 2 of the block's side, slope rect
  grown 1.2×1.1 touching both): their far side neither pushes nor kills.
- Block far-side slide needs velocity strictly into it (v > 0 / v < 0 upside down).
- Rotated objects (not quarter turns) have an OBB; the rect is its bounding box (matches the
  game's rects for all 302 rotated objects of Hexagon Force). Hazards test SAT both ways against
  the player's box **rotated by the player's visual rotation** (PlayerObject::getObjectRotation
  = getRotation).

### Orbs (`playerTouchedRing`, `PlayerObject::pushButton` / `ringJump`) — bit-exact on Polargeist

- Flags: press sets m_jumpBuffered, m_stateRingJump, m_stateJumpBuffered, m_statePreJump, clears
  m_touchedRing and m_usedRings. Release clears m_jumpBuffered and m_stateRingJump. A ground jump
  (cube; also UFO jump, ball flip) clears m_stateRingJump. **End of PlayerObject::update copies
  {m_stateJumpBuffered, m_statePreJump} ← {m_jumpBuffered, m_stateRingJump} in one 2-byte store**
  and zeroes m_touchedRing.. (4 bytes). Ghidra shows it as `undefined2`; name_fields.py labeled
  only the first byte, so read raw decomp for multi-byte stores.
- An orb fires (ringJump) if statePreJump && stateJumpBuffered && !touchedRing && not dashing and
  it isn't in usedRings. So: a press (or a hold that started in the air) fires the next orb; a
  hold already spent on a ground jump doesn't; one orb per step.
- Contact: rings are 36×36 rects. Ground modes call ringJump on contact; flying modes
  (ship/UFO/wave/swing) only from pushButton, for rings in m_touchingRings (rings touched in
  the previous pass; `resetTouchedRings` after processCommands keeps only those). When touching
  rings at press time, the press fires them instead of the cube's ground jump.
- After firing: added to usedRings, marked activated by the player (vtable 0x600) — collisions
  skip activated objects (0x608), so each orb fires once per attempt (no 2.2 multi-activate yet).
- Velocity: v = f32(m_yStart) × factor, round3(size × ±v). Factors: blue (13) 0.8; green (29)
  1.0 (ship 0.7); pink (12) ship 0.37, UFO 0.42, ball 0.77, else 0.72; red (35) ship 1.0 (mini
  1.4), UFO 1.02 (mini 1.36), ball 1.34, robot 1.28, spider 1.34, else 1.38; yellow/other: robot
  0.9 else 1.0. Size factor 0.8 when mini. Ball and spider then ×0.7, swing ×0.6 (unrounded).
  Green flips gravity before computing the sign; blue flips after (flipGravity halves velocity).
- `m_yStart` and gravity per speed (`updateTimeMod`, constants at 0x1008e5960..): 0.7 →
  10.620032 / 0.940199; 0.9 → 11.180031776428223 (f32 11.18) / 0.9581990242; 1.1 → 11.420032 /
  0.957199; 1.3 and 1.6 → 11.230032 / 0.961199. Base speeds (0x788) per speed too (0.9 →
  5.770002).

### Camera (basic modes; `GJBaseGameLayer::updateCamera`, ~1,000 lines) — exact

(2026-11-03 night: ported from the decompile, see "Camera port" below; this paragraph is the
original zoom-1 summary.)

- Runs once per physics step, after collisions, with dt = 0.25 (the step in 1/60 s units).
- x: holds at 15 until the player is far enough in, then cam_x = player.x − (view_width/2 − 75).
- y: center = cam_y + 160; target = player.y − 230 if player.y > center + 70, player.y − 120 if
  player.y < center − 40, else cam_y; target ≥ 0; cam_y = f32(cam_y + (target − cam_y)/(10/0.25)).
  Verified on a full route: max error 0.00002. The distance-based easing (10..30) seen in the
  code belongs to the platformer branch only.
- Traces record the camera before that step's update, so they show it one step behind.

### Drawing (from scene-graph dumps of the real game; `screenshot` + `.scene` in the harness)

- Textures are premultiplied (cocos2d: c·(a+1)>>8). Blend modes: objects (ONE, ONE_MINUS_SRC_ALPHA),
  glows/additive objects/ground line (SRC_ALPHA, ONE), ground edge shadows (DST_COLOR,
  ONE_MINUS_SRC_ALPHA), background (ONE, ZERO).
- Background: screen-space sprite, scale view_width/480, x = −((cam_x − 15)·0.1) wrapped by its
  scaled width; tinted by channel 1000.
- Ground (GJGroundLayer): top edge at y 91 (physics floor 90 + 1); 128-wide tiles at
  −((cam_x − 15) mod 128); `floorLine_01_001.png` 444 wide, centered, additive, at +0.5;
  `groundSquareShadow_001.png` at both screen edges, scale 0.7×1, opacity 100, DST_COLOR blend.
- Glows: `<frame>_glow_001.png` (e.g. spike_01_glow_001.png), white, normal blend, behind objects.
- Detail ("_color_") sprites default to black (channel 1010) when an object has no detail channel.
- Color triggers 29/30/104/105/221/717/718/743/744/899/900/915: linear fade over key 10 seconds
  from the current color, displayed truncated to whole 0..255 values. Matches real frames.
- Frame comparison (`tools/compare_shots.py`): mean abs diff vs real frames 2.0–3.6 (of 255) at
  steps 300/1200/3480 after these fixes (from 8–80). Remaining: enter effects (triggers 22–27,
  objects animate in at the screen edge), particles, attempt text, slight glow intensity.


### Color channels (2026-11-03, from the decompile; screenshots not yet compared)

Ported in `triggers.rs` (`Colors`); decompiles in the session scratchpad came from
`tools/re/ghidra.sh decompile` on a copy of the project.
- Each channel has a `ColorAction` (from/to color +0x48/+0x4b, from/to opacity +0x60/+0x64,
  duration +0x50, elapsed +0x44, blending +0x54, player color +0x58, copy id +0x78, HSV +0x68,
  copy opacity +0x7d) and a `ColorActionSprite` (base +0x140, active +0x143, opacity 0..255
  +0x13c). Channel strings: 1/2/3 rgb, 4 player color, 5 blending, 6 id, 7 opacity (if 8 ≠ 0),
  9 copy, 10 HSV "h a s a v a absS a absV", 17 copy opacity. Legacy kS29..kS37 use the same keys.
- Color trigger (`GJBaseGameLayer::updateColor` 0xf4990): from = the sprite's base color and
  opacity/255, to = trigger color (player color for 15/16) and opacity 35; blending 17, copy 50
  + HSV 49 + copy opacity 60 take effect at once; duration ≤ 0 sets the base now.
- Effects step **once per frame after the steps**, by the frame's game seconds as f32
  (`GJBaseGameLayer::update` → `GJEffectManager::updateEffects((float)local)` 0x269138), via
  `World::frame_end`. Fade: t = elapsed/duration (f32); color `multipliedColorValue` (trunc of
  fmadd per byte), opacity fmadd(to − from, t, from); non-copy channels copy the action's color
  and opacity×255 to the sprite every frame (`updateColorEffects` 0x269234).
- `processColors` 0x268250: active = base (HSV-shifted for copies), then channel pulses (color
  mode, or HSV without a source channel); then copies parents first (`processInheritedColors`
  0x268500 + `FUN_100268c28`): parent's active color, faded from the action's from color while
  the action runs, HSV, own pulses; opacity = (copy opacity ? parent's : 255) × action opacity.
  Copy loops are skipped (+0x7e).
- `GameToolbox::transformColor` 0x44a668: bytes × 0.003921569f → f64 `HSVfromRGB` 0x31cc98
  (gray → hue 0, black → (−1,0,0)); hue + shift, < 0 → +360, > 360 → (int)h % 360; s, v added
  (abs) or multiplied, then min 1, max 1e-4f; `RGBfromHSV` 0x31cd44 (sector = h·(float)(1/60),
  q/t with fused fmsub); bytes = (int)(x·255). Identity (0,1,1) returns the color untouched.
- Pulses (`colorForPulseEffect` 0x268e84): value ≤ 0 → unchanged; color mode → multiplied
  toward the color; HSV mode → shift the source channel's (key 50) or own color, multiplied by
  the value unless it is 1.
- Objects (`GameObject::colorForMode` 0x5b9e88): channel active color (0/1011 white, 1010 black,
  1012 = main color with HSV (0, ×0.65, +0.15)), then its groups' pulses in group order, then
  the object's HSV (main 41/43, detail 42/44). Opacity (`updateMainColor` 0x5b9d90):
  trunc(channel opacity)·0.004 (1 above 249) × each group's alpha factor
  (`opacityModForGroup`; alpha trigger 1007 fades group 51 to 35 over 10 s with fmadd,
  dropped once finished at 1).
- Blending (`GameObject::shouldBlendColor` 0x5af5c0): a part blends if its channel's action
  blends; 1005/1006/1007 always, 0 and 1012 never. Each z layer has a normal and an additive
  (SRC_ALPHA, ONE) batch node per sheet; the additive one is 20 below the normal one in z
  (`setupLayers` 0xce858, `parentForZLayer` 0xda980), so blending parts draw under the layer.
  When exactly one of main/detail blends, that one goes a layer up (main if
  m_colorZLayerRelated +0x41d is 0, detail if 1; set per id in `customSetup` 0x5a8b10 and
  `setupCustomSprites` 0x3aebc4). The app takes +0x41d as 0 until the sprite dump records it.
- Not yet: color/pulse/alpha triggers fired by spawn or touch (they need the group trigger
  system), pulses' main-only/detail-only keys, the HSV-lerp flags (+0x88, pulse +0x3a), light
  BG 1007 (`calculateLightBGColor`).

### Frame pacing (native and web)

- The game's frame delta comes from the display link, so every frame is a whole number of
  display periods (60 Hz: exactly 4 steps). Cadence rounds each frame's measured time to whole
  periods on its own. Lessons: snapping only within 25% let CPU-side jitter through (13 of 774
  frames ran 2/3/5/6 steps — felt as stutter); carrying the remainder between frames oscillates
  0/8 steps when it settles near half a period; float error needs a tolerance (1e-9) or a 4-step
  frame becomes 3 + 5.
- Browsers don't report the refresh rate: the median of recent frame times is snapped to a
  standard rate (60/75/90/100/120/144/165/240 Hz).
- Measured (`CADENCE_BENCH=12 CADENCE_SPEC=<route>`): 661/662 frames exactly 4 steps natively;
  simulation + composition 0.06 ms per frame on an M2.

## Particles and hit effects (2026-11-02)
All of the game's effects blend SRC_ALPHA, ONE (additive) on `square.png` (white, Resources,
one resolution). Emitters follow cocos2d-x 2.2 `CCParticleSystem` (emission rate = total/life,
so life 0 means one burst; RANDOM_MINUS1_1 variances). Position types: Free(0) pins particles
to the screen, Relative(1) to the parent (the level), Grouped(2) moves them with the emitter.
Vtable slots used by GD's code (offset from `setGravity` = 0x418): 0x3c0 setPosVar, 0x3e8/0x3f0
get/setAngle, 0x3f8/0x400 angleVar, 0x410/0x418 gravity, 0x420/0x428 speed, 0x430/0x438 speedVar,
0x508/0x510 startSize, 0x518/0x520 startSizeVar, 0x550 setStartColor, 0x570 setEndColor,
0x620 setPositionType. Settings below were recorded from live emitters (trace mod `particles`).
- Death (`playDeathEffect` default): `explodeEffect` grouped at z 100 in the player's main color
  (alpha 1), a `CCCircleWave` size·10 → size·90 over 0.5 s (size = vehicle size, ×0.9 when ≥ 1)
  at z 99, and unless game variable 0153 is on, `ExplodeItemNode` at z 101.
- `CCCircleWave(r0, r1, t, fadeIn, easeOut)`: a *filled* additive polygon (10/15/20/30/50
  segments for r < 10/20/40/200/else). Normal: radius and opacity 255 → 0 both `CCEaseOut` rate
  2 (√t) when easeOut (the 4-argument `create` at 0x123860 sets it). fadeIn: opacity 0 → 255 over
  the first half, 255 → 0 over the second, radius linear, nothing eased.
- `ExplodeItemNode`: the icon is captured to a (int)(size·40)² render texture, cut into cols ×
  rows pieces (2–3 each from rand; 5% one column, 5% one row); cut widths round(W/n·(0.55 +
  0.9r)), the last takes the rest. Each piece: vx = 0.36v ± 0.72v (v = playerSpeed·speed·0.72),
  vy = 6 ± 3, life 1.4, fade over the last min(life, 0.5) s, spin ±6°. update(dt): f = 54dt,
  vy −= f/2, vx ·= 0.98 + 0.02(1 − f), pos += f·v, bounce off y = 90 + min(w, h)/2 with vy·−0.8
  (at least 1.5), rotation += f·spin.
- Hit circles (all at z 0 in the object layer, following the object): orb `ringJump` fadeIn
  35 → 5 (red orb 42) over 0.35 s; pad `playBumpEffect` 10 → 40 (12 for a normal-size red pad)
  over 0.25 s, eased; portals `spawnPortalCircle` fadeIn 50 → 5 (gravity and size 45) over 0.3 s.
  Colors (RGB): orbs yellow 255,200,0 · pink 255,0,255 · blue 0,255,255 · green/dash 0,255,0 ·
  red 255,100,0 · grav-dash 255,0,255 · spider 255,50,255; pads by type (8 yellow 255,200,0,
  9 pink, 10 blue 0,255,255, 34 red as yellow, 44 spider 255,50,255); portals: ship 255,0,255,
  UFO/wave/swing 255,200,0, ball/robot/spider 255,50,50 (to the cube: the color of the mode
  left), gravity up 255,200,0 / down 0,150,255, normal size 0,255,150 / mini 255,0,150,
  mirror on 255,150,0 / off 0,255,255, dual 0,255,100.
- Object effects (`GameObject::customSetup`, claimed while visible): orbs `ringEffect` z 4,
  pads `bumpEffect` z 0, portals `portalEffect01` (size/dual `portalEffect08`) z 4 at (−5, 0)
  (size −3, dual −7) in the object's rotated frame; all grouped, colors per id (table in
  particles.rs). Speed portals have none. Each portal's back half (`portal_NN_back_001.png`) is a separate
  white object in the last B batch (z −5, under the particles); its front is in T1 (z 220), so the
  player (z 59) passes between them. T-layer objects in general draw over the player. The emitters sit in the object layer between the B
  layers (batches at negative z, portal backs in B1) and the player (59); T layers are ≥ 120.
- Player (`PlayerObject::init`/`update`/`setPosition`): ground drag (z 39, relative) at
  (−10, −13)·size, mirrored upside down, runs while touching the ground and stops 0.06 s after
  leaving it (cube, ball). Ship fire and the held boost (z 39, relative) at the vehicle sprite +
  (−12·size, −4) in the rotated player frame. Ship sparks (z 61, grouped) at (size, −15·size)
  while touching a surface and not moving away (|vy| < 1 toward it). Landing (`hitGround`, only
  coming from the air): two `landEffect` emitters alternate, skipped while the next one still
  has ≥ 3 particles; at (0, −15·size), angle/gravity mirrored upside down, rotated to the slope.

## Dual mode (2026-11-02, verified with player 2 traces)
- The dual portal (type 23) runs `toggleDualMode(on)`: player 2 = `copyAttributes(p1)` (position,
  total time, gravity, direction, modes, speed, size, velocity, the 2-byte jump flags; not rotation,
  not `accelerating`), then gravity flipped (`m_lastFlipTime` = now) and velocity = round3(−p1's)
  with `frinta` (round to nearest, like every 3-decimal velocity copy). Not on the ground. It's
  created at the end of player 1's collisions and not stepped that step. Its rotation is whatever
  the reused object had (0 the first time).
- Inputs go to player 1 then player 2 (`handleButton`); each step updates player 1 then player 2.
  Orbs/portals are used per player. Either player dying ends the attempt.
- Corridor while dual: centered on the dual portal, height max(own new mode, other's mode) with
  flying 300 and everything else 270 (`updateDualGround`; even two cubes get floor and ceiling).
  `playerWillSwitchMode` also flips the switching player to the other's opposite gravity when
  both end up in the same mode (unless the level flag at +0x778 is set).
- Solo portal (type 24): touched by player 1, player 2 just goes; touched by player 2, player 1
  takes `copyAttributes(p2)` (keeping its own rotation). The corridor then follows player 1's mode
  around the last mode portal (`m_lastPortal`), none for the cube.
- Trace mod: player 2 goes to `<attempt>.p2` while dual is on; `cadence verify` compares it.

## Slope edges (2026-11-02)
- `preSlopeCollision` wall strip: for a wall on the left, only while the player is left of the
  slope; a wall on the right only counts going left, in platformer, or for a slope moving into
  the player (x step < the slope's x movement) — never for a static slope run left to right. The
  base strip is checked after the wall too; "beyond" is `<=` on both sides. Strip rects are full
  height/width (no 1-unit insets) and go through `collidedWithObjectInternal`.
- 0.1 s grace: a non-flying, non-ball player meeting a slope's far side within 0.1 s of a mode
  change (`m_gameModeChangedTime`) or a gravity flip (`m_lastFlipTime`) snaps to it and moves away
  at ≥ 2 (round3) instead of dying. `collidedWithObjectInternal` has the same grace for blocks.
- Known gaps (Hexagon Force routes v4-16 1–3): a ball bumping a block's underside just after a
  gravity flip is snapped (y = block − h/2, v 0) by `collidedWithObjectInternal`; a ship meeting a
  slope at x 5268; a ship at x 8648. Needs the full port of `collidedWithObjectInternal` (planned
  with move triggers, which need its moving-block path).

## Wave, bounds and collision details (2026-11-02 night, verified on 16/17/18)
- Wave (`m_isDart`): moves by displacement, dy = ±(that step's x step), up while held, flipped
  by gravity, doubled when mini; the stored velocity is ±round3(x speed) (not doubled). Box
  10×10 (`m_width`), inner 3×3; any block contact with the inner box kills (no landing/sliding
  without the dart-slide state); any slope snap kills. Rotation like the ship's, eased 0.25·dt
  (mini 0.4). Entering halves velocity, rotation 0, leaves the ground.
- Floor/ceiling (`checkCollisions`): half height = `m_unkAngle1`·size/2 with that field 30, wave
  20, spider 27. Tests are strict: y < floor + half, y > ceiling − half (touching exactly isn't a
  contact). Upside-down cube below the ground floor is put back only within 0.1 s of a flip
  (`isSafeFlip`); otherwise it's out of bounds. Above `m_maxGameplayY` (2790 for official
  levels; else highest object + 390, at least 1590) is also out of bounds. Out of bounds two
  steps in a row → destroyed.
- The object rect is cached until `setPosition` (rect dirty flag): within one collision pass a
  size or mode portal doesn't change the rect (nor the half-height used to snap).
- Rotated portals/specials are touched through their oriented box (as hazards).
- Legacy ids converted on load: 104→915, 221/717/718→899 (color triggers keep their old ids'
  channels), 675/676/677→1734/1735/1736 (sawblades).
- Spike slopes (`m_slopeIsHazard`): ids 363, 364, 366, 367, 1717, 1718, 1723, 1724.
- In dual mode updateJump's right-side-up 2g tests use −2g (`m_isDual` at +0xa39); upside down
  keeps +2g.
- UFO press on a slope (moving up): + half the slope velocity, capped 1.4× the boost.
- A press-jump's slope bonus is timed with the coming step's `m_totalTime` (the game advances it
  before processing the press).
- `flipGravity` toggles `m_slopeIsDownhill` when on a slope.
- Ball (interim rule, observed): within 0.1 s of a mode change or flip (including the same step),
  moving into a block's far side and reaching it this step: snapped against it, velocity 0,
  hitGround's effects (roll). The cube doesn't (it passes).
- Slope strips are solid (`collidedWithObjectInternal`, no object → no jump snap). A strip that a
  neighbouring potential slope meets (two slopes back to back) is passed. Landing on a block is
  normal unless riding/leaving a slope, where a block the slope meets is passed
  (`m_potentialSlopes`; registered in `preSlopeCollision` when no strip was hit and the player
  isn't on a slope).
- Dual: player 2 dying also marks player 1 dead; the solo portal touched by player 2 gives player 1
  its rotation too, through `flipGravity` (which clears on-ground).
- Known visual-only gaps: player 1's rotation on the step player 2 dies; the ball's roll rate
  across a slope-to-slope change.

## Group triggers and moving objects (2026-11-03, verified on Geometrical Dominator)

Step order (`GJBaseGameLayer::update`, per physics step): `prepareMoveActions` and
`processMoveActions` (objects move), then the player update and collisions, then trigger
activation by x. A trigger passed in step s gets its first tick in step s + 1.

- **Move (901)**: keys 51 group, 28/29 x/y, 10 duration, 30 easing, 85 rate, 58/59 lock to
  player x/y. `GroupCommandObject2::step` / `updateAction` (0x4463e4 / 0x4464b4): elapsed is f32
  (+0x1ac), not advanced on the first tick; t = (f64)elapsed / max(duration, 2^-23) as f64,
  clamped, rounded to f32; e = getEasedValue(t, rate, easing); offset =
  `fmsub(amount, 1 − e, amount)` (one rounding); the step's delta is offset − previous offset in
  f64. Done once elapsed ≥ duration (that tick still applies). With nothing to ease (pure lock)
  it runs until the f64 elapsed (+0x20, advanced every tick) reaches the duration.
- Moves run at **240 Hz** (every step); a group's CCNode position only syncs once per frame,
  so the watchgroup samples look like 60 Hz updates. Per group the step's deltas are summed in
  f64 (`getTempGroupCommand`), rounded to f32, and added to each object's **f64** position
  (+0x3b0/+0x3b8). Lock to player adds the player's movement over the previous step (position −
  last position, f32).
- **getEasedValue** (0x45155c): all 18 easings ported from the instructions (`easing.rs`); the
  bounce segments compare in f64 (`t < 1/2.75`), use fmadd; libm calls computed in f64.
- Position triggers are activated by the **order-channel walk** (2.2, `order.rs`, see "Order
  channels" below). (Superseded: "position triggers pause while sideways and catch up by x" —
  that was what the walk looks like for channel 0 sorted by x.)
- A trigger whose own object is toggled off doesn't fire (x triggers are passed over, touch
  triggers stay armed, spawned ones are skipped) — Deadlocked depends on it.
- Speed portals act through trigger activation, after the step's rotation update: a ball's
  new roll rate (runRotateAction) starts the next step.
- **Toggle (1049)**: key 56 = on; disabled objects neither draw nor collide
  (`getGroupDisabled` in every collision loop). **Spawn (1268)**: activates the spawn-triggered
  (key 62) triggers in its group, key 63 delay. Key 11 = touch triggered: the 30×30 trigger
  square must touch the player.
- **Moving platforms** (post-collision 0x36e834 + `testForMoving` 0x3704f4): objects keep a
  last position (+0x4c8, f32) taken before their first move in a step (stamped), never reset.
  If the block last landed on (`m_lastGroundObject`) moved away (down, or up upside down)
  since that last position, or nothing was landed on this step, it's forgotten; if it moved
  away and the player is within 5·dt (dt = 0.25 frame units) of it, the player is snapped onto
  it with a forced `collidedWithObjectInternal` (no overlap test). While snapped (+0x658), the
  solid pass runs `testForMoving` on grouped blocks (the block grown 5·dt toward the player) to
  pick the block to snap to (`m_collidedObject`). Horizontal carry is platformer-only.
- Last positions aren't kept forever: a finished move command stays one more step (+0x72),
  and that step `processMoveActions` calls `moveObjects` with (0, 0), which still records
  the current position as the last one.

## collidedWithObjectInternal (ported 2026-11-03, `collide.rs`)

Block collision for the non-platformer, non-sideways case, from the instructions. Snap
distance 10 (6 for ship/UFO/wave), signed by gravity, plus the slope center offset just after
leaving a slope. A block that moved toward the player's gravity side since its last position
widens it by its speed ((cur − last)/dt), that speed becomes m_groundYVelocity, "fast" = > 5.
Head side worked out per gravity; top side tested before bottom, f64 from f32 as the game.
Outcomes: landing, head snap, a rising block carrying the player (max of velocities), a block
coming down onto an upside-down player (min, 3-decimal rounding), pushes from blocks moving
away, flyers taking a block's velocity on head hits. Deep contacts allowed only for the same
block as last step's collision on that side (ids reset each step and on gravity flips).
Inner-box kill with the 0.1 s flip grace. hitGround grounds only when moving away at ≤ 5.
The potential-slope scan starts with the ridden slope plus slopes met this pass. It replaced
the earlier hand-derived rules (wave early return, block_continues_slope, the slope-strip
skip, the ball head-hit grace). Not modelled: m_currentPotentialSlope seeding, platformer,
sideways, swing/spider, passable objects.

- **Rotate (1346)**: keys 51 group, 71 center group, 68 degrees (+ 69 full turns × 360), 10
  duration, 30 easing, 85 rate, 70 lock the objects' own rotation. Same command machinery as
  a move (mode 3, eased degrees, per-step delta). `processRotationActions` runs before the
  moves and claims the group's move for the step (`claimMoveAction`). Each object orbits the
  center object (first of the center group; often a decoration, so positions are kept for
  every grouped object): p = (f32)(pos − center) in f64, then `CCPointApplyAffineTransform`
  with the node transform for rotation r (radians = −r·0.017453292f, a = cos, b = sin, c = −sin,
  d = cos; doubles inside, f32 out), plus the claimed move; pos = pos + ((f64)(f32 result) −
  pos). Unless locked the object turns by the delta (hitbox rebuilt). Verified on Fingerdash
  group 19: every orbit position and every rotation per step equal the game's. The node's
  rotation updates every step; positions sync to the sprite once per frame.

- **Move to target** (901 key 100, `getMoveTargetDelta`): when the trigger fires the move's
  offset becomes (target group 71's main object − center group 395's main object, else the
  moved group's), f32, axis 101 (1 x only, 2 y only); then it runs as a normal move. Not yet:
  player targets (138/200), dynamic mode (397), direction mode (394), silent moves.
- **Spawn ordered** (1268 key 441, `spawnObjectsInOrder`): the group's spawnable triggers sorted
  by x; the first fires now, each later one waits (x − first x) / 311.58 s. Delayed spawns (key
  63 and ordered ones) come due at the start of a step (`updateSpawnTriggers`, before the
  moves). Verified on Dash group 265.
- `getMoveTargetDelta` reads live positions (vtable 0x550 = `getRealPosition`, the f64
  position). Sprite (`getPosition`) positions stop syncing off screen, but only drawing uses
  them. A spawn-triggered trigger without key 87 ("multi trigger") fires only once; later
  spawns skip it (Dash 18055).
- Time warp (1935, key 120) and the frame split (`GJBaseGameLayer::update`): each 1/60 s frame
  (+ carry) runs int(t·60/min(tw,1)·4) steps of tw/240 s; a warp change ends the frame early
  and returns the rest to the carry (`World::start_frame`). On screen each frame is still 1/60 s,
  so the app paces each step at `World::real_step_secs` = (1/60)/steps (1/480 s at tw 0.5).
- Level top with kA37: max(1200, highest object y) + 390 (else 2790); out of bounds above it
  + g·(1 − size)/2, twice in a row kills.
- Touch triggers are touched in their scaled square: 30·|key 128| × 30·|key 129| (else 32).
- **Follow (1347)** (from `processFollowActions`, not yet checked against traces): keys 51
  group, 71 followed group, 72/73 x/y mods, 10 duration. After the moves each step, the target
  group moves by mod × (followed main object's position − its last position); the main
  object is stamped first if it hasn't moved this step (so no movement = 0). x skips objects
  with +0x2c4 set.
- **Items** (2026-11-03, from the decompile; not yet checked against traces): a
  collectible (type 30, e.g. 1614) with item id key 80 adds 1 (−1 with key 78) to the item's
  counter when touched (`collectedObject` → `addCountToItem`). Instant count (1811,
  `testInstantCountTrigger`): when passed, compares the counter (80) with key 77 by mode 88
  (0 equal, 1 larger, 2 smaller) and toggles group 51 (on with key 56). Count (1611,
  `runCountTrigger`): armed when passed, toggles when the counter reaches 77 (104 = multi).
  Stop (1616): cancels the actions started by triggers in group 51.

## Object rects (2026-11-03, all of levels 19 and 20 match the real dump)

`getObjectRect`: a w×h box, w/h = hitbox × |scale| (keys 32, 128/129) in f32, around position +
center offset: `x − w·0.5` (centered for all but ~15 ids; the table's offsets are measured and
noisy, centers snap to halves). Quarter turns swap w/h and turn the offset. Other angles: an
`OBB2D` whose corners are computed in **absolute f32 coordinates** (center ± X ± Y), so the
bounding rect is quantized to the ulp at that x. Round hazards (radius > 0) keep an unturned
box. `getObjectRadius` = radius × max(scaleX, scaleY) unless both are 1. `cadence rects
<level>` prints ours; the harness's `dumplevel` gives the game's.

## Robot details (2026-11-03)

- `releaseButton` sets `m_touchedPad` (0x954) along with clearing jumpBuffered/stateRingJump,
  so releasing ends the hold boost until the next jump. Pads set it only without
  `m_fixRobotJump` (0xb58), which `loadLevelSettings` sets to !settings[+0x1c4]: on for the
  official levels, so a pad doesn't end the boost there.
- The boost's push is gravity·dt with gravity already signed (upside down too).
- Entering robot mode resets the rotation to 0. Trace flag bit 7 is `m_isRobot`.

## Teleport portals (747, 2026-11-03; verified on Deadlocked)

- `checkCollisions` reads and clears m_wasTeleported (+0x558) first; when set, the
  flying/ball corridor's floor/ceiling check is skipped for that pass (the max-height check
  still runs). So a teleport out of the corridor isn't clamped on the next step.
- Corridor ceiling for a player with normal gravity is a head bump: it counts as ground only
  when |v| <= 5 (a ball falling away from it at 7.9 isn't grounded, nor a wave pushing into it
  at 7.8). Upside down the ceiling is the ground (v·dir <= 5, as the floor).
- A fast head bump on the corridor ceiling (normal gravity, |v| > 5) isn't a ground hit at
  all: no hitGround, so a rolling ball keeps its roll.
- Key 55 on a teleport (0x769) sets the camera's +0x6b8/+0x6b9 flags: the camera eases into
  the new position (the bezier transition code in `updateCamera`); not ported, so the camera
  differs for a while after such teleports.

`teleportPlayer` (0xdbcc0): x stays; y = the blue portal's current y + (orange start y − blue
start y); the orange portal (749, decoration) starts at blue y + key 54. Sets m_wasTeleported,
m_lastPortalPos, clears m_touchingGround and m_lastGroundedPos.

## Spider and special blocks (2026-11-03, ported; Fingerdash verified start to finish)

- Type-40 blocks (`touchedObject`, specials pass) set a state counter to 2; the end of
  `PlayerObject::update` (before collisions) decrements all four, so a state lasts the step
  it's touched in and the next. Uses in `collidedWithObjectInternal`: hit-head / flip-gravity
  let the player meet a block's far side (stop, v 0, no hitGround); flip-gravity on a head
  contact flips with v ±2 and rising (no-auto-jump also drops ground and the buffer);
  no-auto-jump drops the buffer of a cube that used an orb or gravity pad since its press;
  dart-slide sends the wave through the normal block logic. S block 1829 stops dashing and
  clears jumpBuffered. Not ported: didHitHead/isSafeHeadTest in the bounds, slope hit-head,
  force blocks 2069/3645.
- Spider (`spider.rs`): 27×27 box, bounds half 27·size/2, corridor 270; the ball's gravity
  (0.6 × 0.958199, clamp 15); never rotates; collides like the cube without jump snapping.
  Pads ×0.6 (pink 0.7), orbs ×0.7 (drop the buffer), red orb 1.34, black orb 16.5. The jump
  (`spiderTestJumpInternal`): solids/slopes/breakables in a column toward the other side,
  nearest first; a block starting right of the center counts only if the player wouldn't
  overlap another block there; a hazard in an 8·size column nearer than the surface stops the
  jump at it (one wider retry); else the corridor edge; then gravity flips, v ±1.
  `pushButton` runs updateJump(0) at once on the ground, so the teleport happens before
  PlayerObject::update saves the last position.
- A dash ending gives the cube/ball the icon sprite's rotation (`stopDashing`). `updateDashArt`
  (dash start, rotate gameplay, mode toggles, redirectDash) resets the icon to 0 and runs
  CCRepeatForever(CCRotateBy(0.3, ±288·gravityMod)) on it (− when sideways ≠ upside down). Cocos
  actions step once per rendered frame, before the frame's game updates; the first step only
  marks the start, each loop restarts from the current angle with the overshoot (f32, the
  update an fma). The harness now feeds the scheduler a fixed speed·1/60 s per frame (it used
  wall-clock time: old traces can't reproduce this) and logs `# sched speed=N index=I` (the
  first recorded update's index in its frame: 2 so far). `IconSpin`, `World::sched_every/
  sched_phase`, `Player::begin_step` before each step's inputs. For traces without the header,
  verify/replay take the real rotation at each dash end (`legacy_dash_ends`).
- Ball after a dash: rolls again (runBallRotation). runBallRotation (120°/0.2 s) is reversed
  going left and negated sideways; runBallRotation2 (flipGravity, 340°) is reversed going left
  only. rotateGameplay always calls doReversePlayer → the ball's runRotateAction.
- Rotate-gameplay triggers fire in their own pass (`FUN_1000e7d4c`) right after the collision
  pass and before `updateRotation` (the other camera triggers after it).
- Leaving the swing (toggleSwingMode) or the ball sets rotation 180 upside down, else 0; the
  swing tilts like the ship (updateShipRotation, 0.15·dt).
- Orbs DO get an oriented box when rotated (PlayLayer::addObject → updateIsOriented; only type 7
  decorations and unflagged solids/slopes skip it) and are touched through it against the
  player's box turned by its rotation, like hazards (dumped real rect: 50.9 bounding; the
  real touches stop as the turned boxes separate). The old "bounding rect" rule only worked
  with wall-clock dash rotations.
- "No touch" objects (key 121) are decoration to the game: never collided.
- Round hazards (radius > 0) ignore other angles for their rect but quarter turns still swap
  it.

## Swing, free mode, speed timing (2026-11-03, Dash)

- Swing (portal type 41, 1933): flying; entering halves v; corridor 300. updateJump
  (FUN_100368e30): a fresh press flips gravity and keeps 0.8 of v (f32); gravity 0.4 of
  0.958199 (0.6 mini); limit ±8 (also mini) unless accelerating; rise ends below 2g
  (mirrored). Flyer in collidedWithObjectInternal (snap 6, both sides, head-hit v) and the
  slope flyer branches. Orbs keep 0.6 and drop the press; black orb −14; force 0.4 (0.615
  mini). Trace flag bit 9. Tilts like the ship (updateShipRotation).
- Free mode (portal key 111): playerWillSwitchMode copies it to layer+0x311 and the corridor
  floor/ceiling are skipped (Player.free_mode, from the last mode portal).
- Speed portals: updateTimeMod stores the speed; the step loop applies it at the next step
  before processCommands (a slope later in the same pass uses the old speed).
- 2900: firing sets the channel to key 173 (0 when absent); after a rotation the trigger pass
  re-runs with the turned gameplay; key 169 without an edit (582) sets v to 0.

## Dash orbs and special blocks (2026-11-03, from the decompile)

- **Dash** (`startDashing` 0x379490, orbs 1704 green / 1751 pink, types 37/38): the angle comes
  from the orb's rotation, wrapped to ±180, mirrored past ±90, limited to ±70° (classic mode);
  stores m_dashX = |tan|, m_dashY (0x570) = tan(angle). While dashing (`m_isDashing` 0x99c)
  `PlayerObject::update` zeroes the y velocity and moves y by (x step, f64) × m_dashY, no
  gravity. Ends on release (`releaseButton` → `stopDashing`), on an S block, and in
  `collidedWithObjectInternal` on blocks unless the dash orb persists (+0x740).
- **Type 40 blocks** (`touchedObject` 0x383c28, by id, set a state counter to 2): 1755 D →
  m_stateDartSlide (0xb18, the wave slides on blocks), 1813 J → m_stateNoAutoJump (0xb14),
  1829 S → stop dashing and clear jumpBuffered, 1859 H → m_stateHitHead (0xb1c), 2866 →
  m_stateFlipGravity (0xb20), 2069/3645 → force blocks (m_stateForce, force vector).
  The states are read by `collidedWithObjectInternal` / the slope code.

## Geode bindings caveats (2.2091, m1)

- `PlayerObject::updateJump` is bound to m1 0x1b28e4, which is actually
  `GJChestSprite::switchToState`. The real `updateJump` is the unnamed function at **0x368e30**,
  called from `PlayerObject::update` (matches the imac layout where it directly follows update).

## Level string format

- Encoded: URL-safe base64 of gzip data (`H4sI` prefix).
- Decoded: sections separated by `;`. Section 0 = level settings as `key,value` pairs with
  string keys (`kS38`, `kA13`, ...). Every later section is one object: `key,value` pairs with
  integer keys. Key 1 = object ID, 2 = x, 3 = y.
- Exception: **start positions (object 31) embed level settings** under the header's string
  keys (`kA2`, `kA3`, ...) inside the object section. Seen in level 22.
- Files are read as C strings: 5002 and 5003 end with `\0` + one stray byte (`0xcd`), which the
  game never sees.
- All 27 official levels decode and re-serialize byte-identical (cadence `level`, 2026-11-02).
- Sizes: Stereo Madness 2,291 objects; Explorers (22) 18,329; level 21 is the largest main level
  at 27,283; The Tower (5001) 9,438 objects across 192 object IDs.
- The older main levels use settings keys `kS29`–`kS37` (pre-2.0 color format); 3001 and the Tower
  levels use `kS38` (2.0+ color channels).

## Camera port (2026-11-03, `camera.rs`, from updateCamera 0x1083f8 and its helpers)

Fields follow `GJGameState` at layer+0x1a8 (geode's binding names line up with the offsets):
zoom +0x1a8, offset +0x1b0 (1916 triggers, tweens 15/16), last target +0x1c0, velocity +0x1d0
(= (cam − last)·240), static start/position/target +0x1d8/+0x1e0 (tweens 10/11)/+0x1e8, exit
offset +0x1f0 (tweens 12/13) and applied +0x1f8, start velocity +0x200, duration +0x208, bezier
progress +0x210 (tweens 21/22), follow +0x220/+0x228/+0x230, static on +0x238, exit pending
+0x240 (duration +0x248, rate +0x250, easing +0x258, smooth +0x260, mod +0x268, start +0x270),
gameplay offset follow +0x280/+0x288/+0x290, snap +0x2b4, last zoom +0x2f0, last ease +0x4ac,
last camera +0x4b4. Per step: cam −= applied exit offset; += offset change; zoom keeps the
view's center ((w/lz − w/z)/2); per axis either static (position, or follow/smooth-velocity
bezier with the fma pattern from the instructions, ease 1 / follow easing·progress; y keeps the
corridor ground in view) or follow: the band axis (y; x while sideways) moves only outside
70/40 above/below the center (swapped upside down; other modes: camera padding/easing), the
other axis sits at player − half view + gameplay offset (75/z, eased toward its target by the
player's movement per step, snapped on zoom changes), x ≥ 15; min y 0; ease 1 (x) / 10 (y),
swapped sideways; instant on exit steps. An exit (1914 key 110, also any sideways change)
makes the camera jump to follow and keeps the jump as an offset that tweens to 0 over the
exit's duration/easing (or a bezier with smooth velocity). Static trigger keys: 10, 30, 85
(≤0 → 2), 71, 101 axis, 110 exit, 212 follow, 213 follow easing, 453 smooth velocity, 454 its
modifier, 465 (exit flag). `updateStaticCameraPos` starts from the last camera + half view −
offset, at the zoom of the moment. A free-mode portal animates the ground out (updateDualGround).
Not ported: camera edges (2062) and the end portal's limit (step_end still approximates), the
teleport ease timer (+0x4a8, key 55) and teleport snap, gameplay offset triggers (2901), the
"last grounded at y 105" zoom exception (bVar13), camera guide 1915, shake.

## Teleport portal options (747, `TeleportPortalObject::customObjectSetup`, `teleportPlayer`)

Keys: 54 offset, 55 smooth camera (+0x748: ease timer 0.5), 345 static force (+0x749), 346 force,
347 additive, 348/349/350/351 redirect force, 352/353 keep x/y, 354 gravity (1 normal, 2 flipped,
3 toggle), 443, 464 instant camera, 510 set m_lastGroundedPos, 591 redirect dash. Order: move,
gravity flip, then `updateStaticForce(angle, force)`: angle = (180 for a 749 target) − its
rotation (+180 if flipX), v = force·sin (x component sideways), accelerating on. Dash's only one
(rot 90, flipX) pushes up 18: taken as the orange end not being flipped.

## Passable blocks, remapped spawns, misc (2026-11-03 night)

- Key 134 "passable" (`m_isPassable` +0x50d) in collidedWithObjectInternal (bVar32): no
  kill test, no head-state, flyers don't take its head side, no updateCollideTop — the player
  only lands on its feet side.
- Spawn remap (1268 key 442, `a.b.c.d`): spawned triggers' target groups go through it (Dash's
  touch-spawned platform templates, group 100 → 494 …).
- Moving-platform snap while sideways uses the x axis (the block isn't turned there);
  `testForMoving` stays y-only in the game.
- Slopes: the hit-head state (H block) lets every mode take a slope's far side, and blocks the
  switching/wave kills (joined_r0x0001003711b4).
- Custom orbs (1594, type 36): pressed while touching they spawn (key 504) or toggle (56 on)
  group 51 (`activateCustomRing`; Cadence: `Player::custom_fired`, run in `World::post_step`)
  and clear m_stateRingJump (2.2 changes); they use their own m_touchedCustomRing. pushButton
  fires custom orbs first, then the others; a ring with key 445 (+0x730) alone.

## Order channels (2026-11-03 night; `order.rs`, FUN_1000e7d4c, sortChannelOrderObjects)

Every trigger that isn't touch- (11) or spawn-activated (62) goes into its order channel (key
170, +0x6f0) list (PlayLayer::addObject → orderSpawnObjects). Sort: ord (key 115, +0x6ec), then
int(position + ord) along the channel's direction, then unique id. The direction comes from
the first 2900 with key 171 whose target (173) is that channel: slope direction of its
rotation/flips → table [3,3,2,1,1,2] for slope dirs 2..7 (1 y ascending, 2 y descending, 3 x
descending, else x ascending). Each step, after the collision pass and before updateRotation:
walk the current channel (+0x33c) from its index: the next trigger fires when the player has
passed it — x ≥ its x, or y ≥ its y while sideways; the opposite comparisons when the channel
is reversed — else stop. Toggled-off triggers are passed over. A 2900: with 171 sets the
channel to 173 (default 0) and the channel's reversed flag to (167 & ~1) == 2; without 172
it turns the gameplay (PlayerObject::rotateGameplay); the walk continues in the new channel.
Camera triggers fire in the walk too (applied in the camera step). Dash uses channels 1–15.
Cadence: the player owns the walk (`Player::order_pass`, `order_state`) and reports the world
triggers it fired (`fired_triggers`), which `World::post_step` runs; the planner splits worlds
by that list. Speed portals stay rect-collided; grouped camera triggers are still left out.
- Grouped camera triggers fire like any other (the old skip of grouped ones was wrong: Dash's
  zoom at x 4535 is in group 58).
- Mode portals with key 112 set the camera padding (114, [0,1]) and easing (113, [1,40])
  (+0x2cc/+0x2d0; resetCamera: 0.5 and 10). Non-basic modes (not cube/robot) follow y in a
  band of (h/2 − (padding·(h/2 − 32) + 30))/zoom with that easing.
- Move lock to player (58/59) is scaled by keys 143/144 (0 → 1): Dash's camera-follow group 304
  moves with 0.05 of the player's movement (duration −1 = forever).
- Teleports other than 747 (2902 …) go to their target group's main object (key 51), keeping x
  / y with 352 / 353.
- PlayerObject::rotateGameplay changes the velocity (swap/edit), the collision ids, rising,
  accelerating and the cube spin only when the sideways state changes; a gravity or direction
  change alone only flips/reverses.
- Ship accelerating (m_isAccelerating, past the limits): the push is −1 (thrust) while moving
  toward gravity and 0.8 (gravity-like) while moving away from it, held or not (×0.5/0.4 held,
  0.4 released; released past 2g 1.2). Ship 2g test mirrored (f32 2g) when sideways, swing or
  with kA32.
- Options trigger 2899 (processOptionsTrigger; keys 159 streak blend, 160 unlink dual gravity,
  161 hide ground, 162/163 hide P1/P2, 165/199 P1/P2 controls 1 disable −1 enable, 195, 532,
  573/574 respawn time, 575/576 audio, 593): disablePlayerControls releases the button and
  ignores input; the physical state is still recorded and enabling presses again if held.
- End trigger 3600 (in the order walk): the level is complete at once (no updateRotation that
  step). Dash ends at x 21853 (channel 15), not at its last object.
- Going left on slopes: downhill = uphill (instead of !uphill) ^ gravity; the slope rotation's
  sign flips; preSlopeCollision also builds the right wall strip (going left from beyond it) and
  insets the left wall strip by 1.
- A mode switch that changes the rotation (setRotation) marks the player's object rect dirty:
  slopes later in the pass see the new mode's m_width (wave → cube next to a ceiling slope,
  s19-22); the solid pass keeps the old rect (w2/w3-18, r5-20).

## Platformer (2026-11-03, Tower work; `platformer.rs`, collide.rs, physics.rs)

Verified on real runs re/traces/pf1 (walk/stop/turn/jump, 747 steps) and t1-5001 (planned
route, 2,304 steps): positions, velocities, flags and rotation bit for bit.

- **Input**: harness spec `press <step> <button>` with button 1 jump, 2 left, 3 right
  (`handleButton(down, button, p1)`; the mod already passed the button through). Cadence
  `Spec::buttons` keeps every event in order; `Player::set_direction`. `pushButton(2/3)` →
  `switchedDirTo` (0x37b9d4): right sets m_holdingRight (+0xaa0), clears m_leftPressedFirst
  (+0xaa2); left sets +0xaa1 and +0xaa2 (2-byte store 0x101); a slide the other way stops
  (m_isSliding, m_slopeSpeedBonus 0); pressing against the current velocity clears m_isMoving
  (+0xaf8). `releaseButton` (0x37cc88): clears the side; m_isMoving cleared if moving that way.
  Unlike the jump, a direction pressed on the attempt's first step counts. Buttons 2/3 are
  ignored outside platformer levels (handleButton, layer+0x2fce).
- **Spawn**: `GJBaseGameLayer::resetPlayer` (0xdef64): default (0, 105); with a spawn group
  (level settings +0x1bc = kA36 ≥ 1) the first object of that group (x kept only in
  platformer). The Towers: kA36 = 1 → (45, 225) on 5001. The player starts touching the ground.
- **PlayerObject::update** (0x367eb4): updateJump(dt·0.9) first, then (platformer)
  `updateMove(dt·0.9)` (0x36a7c4); x += f32(m_platformerXVelocity (+0xa98, f64) · dt), not
  flipped by m_isGoingLeft (facing only).
- **updateMove** (ported from the instructions; FMAs as compiled): max = m_speedMultiplier
  (0x788) · m_playerSpeed (cos(slope angle) on an uphill slope); direction: both held → the one
  pressed last; acceleration factor 1 (0.2 on ice / just after a slope jump the other way,
  (2t·0.8f+0.2f) within 0.5 s of m_slopeJumpResetTime); step = dt·0.5·factor, ×1.5 turning
  around; holding: idle timer (+0x9d0) = 0, v = fma(step, ±1, v) if below ±max, slow-down rate
  0.05f; nothing held: rate = min(timer + 0.2f, 1) and timer += dt·(0.2 ground, 0.05 cube air,
  0.1 robot/spider, 0.02 ship/ball/UFO) (f32 fma). Moving/forces/momentum paths use 0.05f (held
  or decreaseBoostSlide) / 0.2f. Slow-down when nothing held or |v| > max: v = fma(−(rate·v)·dt,
  k, v), k = 0.15 on ice or m_isMoving else 1; |v| < 0.01f or a sign change → 0 (and the
  momentum/forces/slide flags clear). Then facing (`doReversePlayer`: m_isGoingLeft; the ball
  restarts its roll), a cube turning around with a sideways-ish angle (45–135, 225–315) gets
  +180, ship/UFO rotation negated; a cube moving with a direction held, |vy| > 1 and not spinning
  starts its spin. (Slopes/ice/forced slides ported but untested; the idle-air rotation easing
  and the ball roll speed are visual and not ported.)
- **runRotateAction** (0x36cab4) in platformer: always stops the spin first; the cube starts one
  only while a direction (or forced slide) is held; the ball never. So a jump with no direction
  held doesn't spin, and walking off a ledge doesn't either.
- **updateJump** in platformer: the 2g tests are the mirrored f32 ones (as with kA32); the ground
  flag stays for 0.05 s after the last landing (`m_lastLandTime` +0x9c8, set by hitGround when it
  grounds) unless sliding into a third slope jump; the falling spin only when not touching the
  ground; the slope jump bonus only with |vx| > 4. hitGround clears m_isOnIce and m_isSliding.
- **collidedWithObjectInternal** in platformer: snap distance 5 (10 on/just off a slope; 15 with
  the no-wall-push state), also for flyers; a moving block doesn't set m_groundYVelocity up
  front; both sides are met like the hit-head state (head hits stop, velocity 0); a block counts
  for top/bottom only if the player's entry rect narrowed by 2.5 on each side (x + 2.5, w − 5)
  touches it (0x100372844); no inner-box kill. After the top/bottom part every contact runs the
  **side test** (0x1003732a0): the entry rect shrunk 2.5 top and bottom touching the block (or the
  block being m_collidingWithLeft/Right +0x608/+0x610, which persist), and (y + h/2 − snap) or
  (y − h/2 + snap) strictly inside the block's y range (on/near slopes: y ± h/2 and y) → the
  player is put beside it: right of its center (or it's m_collidingWithLeft) → x = maxX + w/2,
  v<0 → 0; else x = minX − w/2, v>0 → 0 (unless the block moves away).
- **Position triggers in platformer follow time**: the order walk (0x1000e7d4c) compares with
  `posForTime(m_levelTime)` = 311.58011f · (float)levelTime (`LevelTools::posForTimeInternal`
  0x45c32c, platformer branch), not the player. m_levelTime (layer+0x3a0) += step time / time
  warp right before the walk (GJBaseGameLayer::update 0x107400).
- **Spawn chains** (not platformer-specific, found on 5001): a spawn trigger started by a
  remapping spawn passes the remap on (unless it has its own); a delayed or ordered spawn that
  fires mid-step passes its overshoot on (what it spawns waits that much less) — the 5001 bobbing
  loop runs 139/130/138/130 steps per half cycle.
- Level end: the classic x ≥ length − 300 finish is off in platformer (Towers end by trigger;
  not ported yet). Camera: the platformer branch of updateCamera isn't ported (traces' camera y
  starts at 94.5, ours 0) — no physics effect seen yet.
- **Area move (3006)** (`area.rs`; verified: every bridge block of 5001 per step equals the game's
  f64 positions, t3-5001): `addAreaEffect` (0xf84a0) keeps one instance per trigger, an effect
  id (225) replaces the older one; every step after moves and follows (0x1000fb3a4, order
  transform/rotate/move) `processAreaEffects` (0xf8d7c): center = the center group's main object
  (71; negative ids are players/screen, not ported) + (offset 220, offset y 252); both
  directions (283) sets modFront = −|modFront|; a section window of radius
  (int)(length + offset)/min(|front|, |back|) (x only for direction 1 unless inbound); for each
  target-group object in it `processAreaMoveGroupAction` (0xfa334): `resetAreaObjectValues`
  (0xf992c: once per step, stamps the last position, f64 pos −= the f32 offset applied);
  `getAreaObjectValue` (0xfb7fc): direction 1/2 → d = (obj − center) on x/y × (d < 0 ? front :
  back), 0 → distance; ratio = d / (float)(int)length; deadzone (282) → (r − dz)/(1 − dz);
  clamp 0..1; inbound (276) → 1 − v. v ≥ 1 → nothing; else offset = (1 − eased(v)) · moveDistance
  (218) along ccpForAngle((angle 231 − 90)·0.017453292f), added in f64 to the position (y first)
  and to the stored f32 offset. Easing (`getEasedAreaValue` 0xfba2c, buffer index from 0x1000d9ec0):
  type 0 linear, types 3/7/8/9 direct getEasedValue, others linear between a 101-entry table of
  getEasedValue(i/100) (fma(trunc(t·100), −0.01f, t), fma(frac·(b−a), 100, a)). Key → field map
  (customObjectSetup 0x13c89c; absent keys read 0; rate 243/249 truncated to hundredths): 217
  enter type, 222 length, 223 length var, 220 offset, 221 offset var, 252/253 offset y (var),
  218/219 move distance (var), 231/232 move angle (var), 237–240 move x/y (var), 233–236 area
  scale, 242/243 ease in, 248/249 ease out, 241 XY mode, 261 ease out on, 262 direction, 263/264
  mod front/back, 265 tint, 270 rotation, 275/286 to/from opacity, 276 inbound, 282 deadzone,
  283 both directions, 287/288 relative (fade), 225 effect id, 260 tint channel, 51/71 target/
  center, 10 duration (edit transitions). Not ported: variances, XY/relative modes, ease out,
  rotate/scale/fade/tint kinds (3007–3010, visual except scale/rotate on solids), edit area
  (3011–3015, `EnterEffectInstance::loadTransitions/updateTransitions`).
- **Solid pass order with sections**: `checkCollisions` (0xe18e8) gathers per 2-D section (x then
  y, 100 units, each section sorted by unique id, FUN_1000cbbd8) and tests solids in reverse.
  Interim rule that fits every trace: solids an area move has offset sort by (x section, y
  section, id), all others by (x section, id) only (a y section breaks level 18's jump snaps for
  static blocks and Fingerdash's move-trigger blocks; 5001's bridge needs it). Probably only
  `reorderObjectSection` (vtable 0x558, called by area moves) refreshes an object's y section —
  unconfirmed. Specials stay (x section, id).
- **Rotate triggers don't turn solids** (`rotateObject` 0xfe7a4 needs m_canRotateFree +0x364;
  `GameObject::canRotateFree` 0x5afcf8: false for types 0/21/25 unless no-touch; the level's kA41
  (settings +0x1cb → layer +0x7bc) turns it on for all): 5001's wheel of passable slabs orbits
  unturned (watchgroup: rotation 0, rect unrotated).
- **Platformer moving platforms** (FUN_10036e834, before the moving-away test): standing on
  m_lastGroundObject (touching the ground) carries the player by the block's f32 x movement
  (cur − last) — not for non-stick-x blocks (key 136, +0x50f); key 289 non-stick y (+0x510) makes
  a block's movement never count as moving away; the snap reach is 10·dt in platformer (5
  classic, 20 with key 495 extra sticky +0x514); the snap's forced `collidedWithObjectInternal`
  passes skipCheck, which skips the platformer narrowed-rect test.
- **Spawned end trigger** (3600 with key 62, the Towers' doors: 5001's touch trigger at
  (10151, 681) spawns group 235 ordered → 3600 at x 13215, i.e. 10 s later): World::level_end.

## GUI (2026-11-03, `crates/cadence-game/src/ui/`; decompiles from a copy of re/ghidra)

Coordinates are screen points: the view is 320 tall, width = 320 × aspect (568.9 at 16:9);
`getScreenTop/Bottom/Left/Right` are FUN_1003e3e60/68/70/78. CCMenu sits at the screen center
(items positioned in its space). `alignItemsHorizontallyWithPadding` is FUN_10076a8b8,
`CCMenu::createWithItem` FUN_100769c98, `ccp` FUN_10021e210, `ccpAdd` FUN_10021e2a8.
- **CCMenuItemSpriteExtra** (ctor 0x1edf80, selected 0x20ac34, unselected 0x20aec4; `+0x194` base scale, `+0x190` press multiplier 1.26,
  durations `+0x1dc` 0.3 / `+0x1e0` 0.4): `selected` ScaleTo(0.3, base·mult) CCEaseBounceOut;
  `unselected` ScaleTo(0.4, base) EaseBounceOut; `activate` snaps to base. `setSizeMult`
  (+0x174) widens the touch rect. LevelPage's box uses mult 1.1.
- **MenuLayer::init 0x3066e0**: MenuGameLayer (z 0); GJ_logo_001 (GJ_LaunchSheet) z2 at
  (w/2, top − 50); menu at (w/2, h/2 + 10): play (0,0), garage −110, creator +110; hints
  GJ_chrSel_001 at garage − (50, 50), GJ_lvlEdit_001 at creator + (50, −50) (until used,
  GameManager +0x39f/+0x3a0); bottom row ach/options/stats/ng (scale 1, padding 5) at
  (w/2, bottom + 45); robtoplogo_small 0.8 at (left + 50, bottom + 25), size mult 1.1; social
  icons 0.8 (size mult 1.5) from (left + 22, bottom + 55) +29 each, Discord = YouTube + (29, −29);
  news 0.85 at (right − 43, bottom + 45); close 0.7 at (18, top − 18); profile at (45, 105), its
  gold label at (47, 141) limited to 70 wide at 0.7; daily chest at (right − 40, h/2 + 20) mult
  1.5. Controller hints only with a controller.
- **MenuGameLayer** (init 0x4557c4, update 0x456798, updateColor 0x455f74, resetPlayer
  0x456158, tryJump 0x455cfc): background game_bg_01 (repeat), starts (0, 102, 255), ground at
  y 90 (z 100), player z 60 at y 105. Every 5 s `updateColor` tints the background to the next
  of (0,102,255), idx 7, 8, 9, 10, 11, 1, 3 over 4 s and ground01 to 0.8× (0.6× when the ground
  has a second layer). Scroll += dt·60·5.770002·0.9; background at −0.1× scroll; player reset
  past w + 100 to x = −100 − 500·rand, colors `colorForIdx(rand·108)`, time mod 1.3/1.1/0.7/
  1.6/0.9 at rand < .2/.4/.6/.65/else, random vehicle (ship 12%, ball, ufo, wave, robot 12%
  bands, spider/swing 10%) and icon; tryJump every 0.25 s presses with p 0.2 when on ground.
  Cadence runs only the cube (one icon loaded per vehicle) with the real physics on an empty
  level.
- **GameManager::colorForIdx 0x2f6528**: the 107-color palette (table in ui/menu.rs).
- **LevelSelectLayer::init 0x39707c**: GJ_gradientBG (no -uhd file: falls back to -hd) at
  (−5, −5) anchor 0, scaled to (w + 10, h + 10), z −2, starting (40, 125, 255); GJGroundLayer at
  y = min(h/2 − 110, 128); GJ_topBar_001 anchor (0.5, 1) at (w/2, top + 1); GJ_sideArt_001 at
  both bottom corners (−1, −1) / (right + 1, −1) flipped; pages = levels 1–22 + GJGameLevel −2
  (Tower) + −1 (Coming Soon) in a looping BoomScrollLayer; dots at (w/2, bottom + 15), 16 apart,
  current white, others (125,125,125) (`updateDots` 0x319624; sprite uiDot_001 assumed);
  "Download the soundtrack" bigFont 0.5 at (w/2, bottom + 35); nav arrows navArrowBtn_001
  (prev flipped) at (left + 25, h/2)/(right − 25, h/2), size mult 2, z 5; back GJ_arrow_01 at
  (left + 25, top − 22) mult 1.6; info at (right − 20, top − 20), sideBtn_death 40 below
  (hidden on special pages).
- **Page colors** (`colorForPage` 0x399c60, `getColorValue` 0x3999bc, `scrollLayerMoved`
  0x3997f4): page % 22 == 0 (≠ 0) → (37,44,52), else colorForIdx of table 0x8e5e80 =
  (5,7,8,9,10,11,1,3,4)[page % 9]; between pages f = frac/0.6666667 − 0.2 clamped, k = 0 below
  0.2, (d − 0.2)/0.6 to 0.8, then 1; ground01 = 0.8×, ground02 = 0.9×.
- **BoomScrollLayer::moveToPage 0x31a1d0**: CCMoveTo(0.96 / 0.72 / 0.48 by drag state) with
  CCEaseElasticOut(period 0.5); onNext/onPrev = moveToPage(current ± 1) (looping).
- **LevelPage::init 0x39a850 / updateDynamicPage 0x398008**: box 340 × 95 (square02_001 9-slice,
  opacity 125) in a menu item at (w/2, h/2 + 60); name bigFont anchor (0, 0.5), scale
  min(1, 250/w); face `diffIcon_%02d_btn_001` scale 1.1, x = face/2 + (170 − (face/2 + name +
  35)/2), y 50.5; name at face.x + 30; stars GJ_starsIcon 0.7 at (325, 82), count 0.5 anchor
  (1, 0.5) at (313, 82.5), (255,255,50) when completed; coins at (324 − 52/26/0, 16): gold or
  GJ_coinsIcon_gray tinted (225,225,225); orbs "got/total" 0.4 at (8, 12), currencyOrbIcon 0.7
  at label end + 9; locked (stat "8" secret coins < +0x454: Clubstep 10, ToE2 20, Deadlocked 30):
  GJLargeLock at box center, coin icon at (325, 15), "n/m" 0.7 anchor (1, 0.5) 15 left.
  Bars: GJ_progressBar_001 black opacity 125 at (w/2, h/2 − 30) and −80; fill scale 0.992 ×
  0.86 anchor (0, 0.5) at ((W − 0.992W)/2, H/2), texture rect width W·pct, normal (0,255,0)
  (0x8e5e40), practice (0,255,255); "%i%%" 0.5 on the bar, "Normal Mode"/"Practice Mode" 0.5
  20 above. Tower page: theTowerDoor_001 at (w/2, h/2 − 5) bobbing MoveBy(0, ±5) 2.5 s
  EaseInOut 1.8, theTowerLabel_001 23 above its top. Coming Soon: bigFont at (w/2, h/2 + 50);
  `addSecretDoor` secretDoorBtn2_closed_001 at (w/2, min(h/2 − 110, 128) + 30), opacity 0 until
  first clicked (UGV 7), then `onSecretDoor` opens SecretLayer4 (needs UGV 8).
- **Box click (FUN_100399eb0)**: page −2 → onTheTower; locked → FLAlertLayer "Locked",
  "Collect %i more <cy>Secret Coins</c> to unlock this <cl>level</c>!"; else play.
- **LevelTools::getLevel 0x457480**: +0x258 difficulty (1 easy … 6 demon), +0x25c audio track,
  stars +0x3bc: 1,2,3,4,5,6,7,8,9,10,11,12,10,14,12,12,10,14,10,15,12,12 (levels 1–22),
  difficulties 1,1,2,2,3,3,4,4,4,5,5,5,5,6,5,5,4,6,4,6,5,5; 3001: difficulty 3, 3 stars.
  `getBaseCurrencyForLevel` 0x301f0: main levels 400 orbs for 14/18/20, else 20·stars + 20.
- **PauseLayer::customSetup 0x344754**: CCBlockLayer black opacity 75; square04_001 9-slice
  (w − 20) × (h − 20), black opacity 175; name bigFont at (w/2, top − 30); buttons in add order
  practice (GJ_practiceBtn / GJ_normalBtn), GJ_playBtn2 (resume), GJ_menuBtn (quit),
  GJ_replayBtn (restart), padding 15 (10 for editor levels) at h/2 − 30; GJ_practiceTxt_001 at
  (w/2 − 190, h/2 − 12); sliders (0.7) at (w/2 ∓ 100, h/2 − 90) with gj_songIcon/GJ_sfxIcon 1.2
  90 left; coins gjCoinUI_01 1.1 at (w/2, 33) 20 apart (saved: "collected", dimmed ×0.5 opacity
  125 unless taken now; else off/on, ×0.75 opacity 191); 10 option toggles in two columns from
  (w/2 ± (w/2 − 10 − 26), top − 36), 34 apart (names built at runtime; not ported); hide button
  invisible. Progress (FUN_1003474bc → FUN_10044a43c/FUN_10044a4d4): bar at (w/2, h/2 + 55),
  black 125, best fill (40,40,40), current fill (0,255,0); "%i%%" 0.5 +1, mode label 0.5 +21,
  "Best: %i%%" goldFont 0.48 18 above (if best > 0). Keys: Esc → tryQuit, Space → resume.
- **UILayer::init 0x42556c**: GJ_pauseBtn_clean_001 opacity 75 at (right − 15, top − 15), size
  mult 4, visible only with game variable 0024 (show cursor in game).
- **Slider::init 0x2854a8**: bar (repeat texture) at (2, 4) in the groove, max width groove − 4.
- **Level end** (`PlayLayer::levelComplete` 0x77624 → showCompleteText 0x78784): GJ_levelComplete
  at (w/2, h/2 + 35) scale 0.01 → 1.1 (0.66 s EaseElasticOut 0.6), hold 0.88 s, → 0.01 (0.22 s
  EaseIn 2); `showEndLayer` 1.5 s later. Cadence shows the text 1 s after the end starts.
- **EndLevelLayer** (customSetup 0x41d404, GJDropDownLayer::init 0x5d466c with height 230,
  showLayer 0x41f698): GJListLayer 356 × 230 at ((w − 356)/2, (h − 230)/2 − 5), color (0,0,0,180),
  GJ_table_top_001 at (178, 245), bottom at (178, −10), GJ_table_side_001 at x −20 / 376 scaled
  to the height (0x5bf304); chain_01_001 anchor (0.5, 0) at (w/2 ∓ 156, list top + 12); main
  layer drops from y h to 0 (CCMoveTo 1 s CCEaseBounceOut), color fades to opacity 100 over 1 s.
  GJ_levelComplete 0.8 at (w/2, h/2 + 75); goldFont 0.8 "Attempts: %i" (w/2, h/2 + 35), "Jumps:
  %i" −24, "Time: %s" (mm:ss, limit 150 wide) −48; sideBtn_death left of attempts; coins
  secretCoin_b_01_001 at (w/2, h/2 − 64) 20 apart, secretCoinUI_001 on saved ones, the ones taken
  this run enter at scale 3 → 1 (0.4 s bounce) + fade in 0.1 s after 0.7 + 0.35·k s, then
  starEffect + "highscoreGet02.ogg" + circle (10→90, 0.4 s); replay/menu at (w/2 ∓ 100,
  h/2 − 125). End text (`getEndText`, random) only for levels without coins.

## Level visuals vs the real game (2026-11-03, from real screenshots, scene and particle dumps)

Tools: `tools/compare_level.py <level> <route.spec> <steps>` and `tools/compare_all.py` (real on
top, Cadence below, difference; shots/). Harness spec commands added: `drawnodes <step> <file>`
(CCDrawNode triangles and CCMotionStreak points/vertices), and `dumpsprites` now records
color_z_related, spin (rotation_base/rate), audio scale, object_black, invisible.
- Editor-only objects (main sprite an `edit_` button: particle emitters, collision/force/damage
  blocks, area triggers, text, counters...) are never drawn in play.
- `m_isObjectBlack` (+0x281): `setObjectColor` 0x5b8708 never tints the main sprite (pits,
  black blocks, saws, Tower fill block 94...).
- Edge fade (`PlayLayer::updateVisibility` 0x7ae88): opacity int(clamp(d/70)·255), d from the
  object's far edge to the view edge (left half: left edge); key 64 exempt.
- Enter effects (`applyEnterEffect` 0x7c570; trigger→code in `updateActiveEnterEffect` 0xd918c,
  mode 0 both sides / 1 right / 2 left, tables reset to −1): t = clamp(center distance / 70);
  −8/−7 x ±100(1−t), −5/−6 y ±100(1−t), −3 scale t, −4 scale 1 + 0.75(1−t), −9..−13 offset
  100(1−t) along an angle (−12/−13 from the near/far vertical edge by the object's half of the
  screen, −10/−9 diagonals, −11 random); key 67 and audio-scaled objects skip the scale.
- Background x = −((camera x − 15)·0.1) mod (512·view/480); y = −camera y·0.1. Art per level:
  kA6 background (`game_bg_NN`, 1–59), kA7 ground (`groundSquare_NN`, 1–22; 8+ add `_2` in
  the ground 2 color). Middle grounds (kA25, `fg_NN`) not drawn yet.
- Glitter (`m_glitterParticles`, `toggleGlitter` 0x7d860): on in ship/swing/wave (UFO assumed),
  centered on the view (`updateEffectPositions` 0x7e6a8); settings from a real particle dump.
- Rods 15–17 carry a ball object (37) the game adds: (rod height/2 + 10) up the rod, player
  color 1 (additive), music-pulsed; style `rod_ball_NN` random 1–3 per session (PlayLayer +0x37ec,
  `getBallFrame` 0x5b850c).
- Regular trail: cocos2d CCMotionStreak (fade 0.3, minSeg 5, stroke 10, `streak_01_001.png`,
  player color 2, additive, z −2); on after orbs/pads/gravity flips and in ship/UFO/swing, off
  on landing (`activateStreak` 0x36d614 / `deactivateStreak` 0x367cf4 callers). Follows the
  player a frame behind (ship: 10.74 lower, fitted).
- Wave trail: HardStreak, band 6 × size × pulse wide (pulse = 0.4 + (audio − 0.1)·2.1,
  `PlayerObject::update` 0x36875c), mitered, square ends, player color 1 then white α165 at a
  third of the width, additive, z −3. Points where the wave turns (fitted; the exact addPoint
  calls not traced).
- Music meter: `FMODAudioEngine::updateMetering` 0x350390 (beat detector over the peak of the
  last mixed block); headless screenshots decode the song to drive it.
- Light background (1007): `calculateLightBGColor` 0x2746b4 (background shifted s−0.2 v+0.2
  absolute; on dark backgrounds blended toward player color 1 by (r+g+b)/150).
- Pre-2.0 object color key 19: 1/2 player colors, 3/4/6/7 color 1–4, 5 LBG, 8 3D line.
- Sheets: object art also lives in PixelSheet_01 (1,847 frames the objects use, Dash's pixel
  art) and FireSheet_01 (122, 2.2 animated objects); both are loaded now.
- Hidden objects (key 135 = 1, m_isHide): collide, never drawn (Dash 2,330, Towers 700–1,900).
- m_isColorSpriteBlack (+0x280): child sprites (e.g. a block's fill) stay black, opacity
  m_blackChildOpacity (+0x284) when set.
- Ground layers drawn where the camera puts them (`ground_bottom` / `ground_top` from
  `updateCameraBGArt`); off-screen in free mode (both GJGroundLayer vis=0 in the real dump).
- Middle ground (kA25, GJMGLayer z −39): scale (1 − (1 − zoom)·0.3)·1.2 (exact, matches dumps);
  y = mgY·((1 − f) + f·zoom) − f·camera y with mgY 25/30/30 (`defaultYOffsetForBG2`, table at
  0x8f9cc4) and f fitted 0.4 (4 real frames, ~3.5 units rms; +0x320 not traced); x scroll
  accumulated per frame by camera dx·0.55/scale (+0x31c fitted). MG edit triggers 3020/3021
  not ported.
- Spawned/touched color, pulse and alpha triggers: `Action::Effect { order }` →
  `World::fired_effects` → `Colors::fire`.
- Text objects (914): key 31 is the text (base64); drawn as a bigFont label (ui/font.rs) centered
  on the object with its scale/rotation/flips and main color. Level font kA18 > 0 (gjFontNN,
  the Tower uses 19) is not loaded yet; bigFont stands in.

## Animated objects (2026-11-03, `animobj.rs` + generated `animobj_table.rs`)

Generator: `tools/gen_anim_table.py <gd.dis> <Resources>` (gd.dis = `llvm-objdump -d
--no-show-raw-insn re/gd-2.2091-arm64`). It runs a tiny straight-line arm64 emulator over
FUN_1001f2b60 (GameManager's animation table; its std::strings are built inline, Ghidra can't
follow them) and records 136 `addGameAnimation(id, frames, frameTime, name, colorName,
defaultFrame)` calls (0x1f93ec → dicts +0x178 main frames `%s_%03d.png`, +0x180 color frames,
+0x188 frame time, +0x190 count, +0x198 default frame) and one `addCustomAnimationFrame`
(1583 frame 4 = fireball_02_002).
- **Frame animations** (`EnhancedGameObject::updateSyncedAnimation` 0x1efde4, called from
  `PlayLayer::updateVisibility` 0x7b638 with the layer time +0x398 as f32): delay = frameTime /
  speed (`setupAnimationVariables` 0x1f1394); frame = ⌊(t + offset) / delay⌋ mod n + 1
  (reverse: n − i). Speed +0x55c: customSetup 0x5bb8dc gives most ids rand·0.4 + 0.8; key 122 =
  1 with key 107 uses |107| (negative plays backwards). Offset +0x548 only with key 106
  ("randomize start", +0x580; customObjectSetup 0x5bb490 overwrites customSetup's default):
  rand·0.5 (70%, else 0) for most ids, rand·0.5 for 1591/1593/1618/1697–1699, else rand() in
  [0,1) (`saveActiveColors` 0x5bb414). Key 462 single frame, 592 offset anim, 123 animate on
  trigger (hidden, opacity 0, until animate trigger 1585 — not ported, so they stay hidden),
  126 disables the freeze, 214 animate-active-only (ignored).
- Which sprite shows what (by id, from 0x1efde4's branches): generic = main frames on the main
  sprite, color frames on the color sprite (child, z −100; lava 1591/1593: z +100);
  LAB_1001f047c ids (920/921/923/924, 1936–1939, 2020–2055 mask 0x87bffffff, 2864–2894 mask
  0x4ffffffb) = main frames on the main sprite, the color sprite stays on `_color_001` and its
  first child (scale 2: the color art is half resolution) shows the color frames; 1516/1518 = the
  main sprite's child (tag 1); 1050–1052 / 3000–3002 = ping-pong of 6 frames with flipX on the
  way back (1050/3000: 4,5,9,10 → 3,2,8,7; others 7–10 → 5–2), on the tag-1 child (3000–3002
  also the color sprite's child); 2892/2893 rotate their child by −22.5° per step / −90° per 4.
  Parent sprites that only hold animated children aren't drawn (GD's dontDraw; the real
  screenshots show no static copy).
- Freeze (`GameObject::usesFreezeAnimation` 0x1f152c: 921, 1519, 1618, 1851/1852/1854–1856/1860,
  most 2020–2055 and 2867–2887): the cycle is n + 1 frames; on frame n + 1 the object hides
  (+0x306 → `GameObject::setOpacity` 0x5adee8 gives 0) for one more frame time plus rand·0.5 +
  0.2 s (+0x54c/+0x550/+0x554), then replays. 1855 (+0x561) holds its last frame instead.
- Lava 1591: each cycle picks 2058 lava_top (8 × 0.13 s) 70%·70%, 2060 bubble02 (6 × 0.12)
  70%·30%, 2059 bubble (8 × 0.12) 30%, starting with 2058 (+0x594 = 1); 1593 always 2058. Lava
  frame times aren't scaled by speed.
- Layer time vs steps: d_animWave frames in 7 real dumps (levels 19–21) fit +0x398 = step time
  + 0.067–0.083 s; Cadence uses +0.075 (`LAYER_TIME_LEAD`, fitted; one level-22 sample wants ≥
  0.086, how +0x398 runs ahead isn't traced — 0x106738 adds a frame delta outside the step loop).
- **Part animations** (`AnimatedGameObject::init` 0x156c04): 918 GJBeast01, 919 BlackSludge,
  1327/1328 GJBeast02/03, 1584 GJBeast04, 2012 GJBeast05: `CCAnimatedSprite` (centered, content
  size from `setupAnimatedSize` 0x156da0) → `CCPartAnimSprite` playing `<type>_AnimDesc.plist`;
  delays/frames/looped from objectDefinitions.plist (sludge `loop` 11 × 0.06). Parts are tagged;
  `transformSprite` 0x2ec9dc sets position, scale (flips = negative scale), rotation, z, texture;
  parts missing from a frame are hidden. No tweening (real part scales equal keyframes).
  `updateObjectAnimation` 0x157898 starts the default animation at a random point
  (`offsetCurrentAnimation(rand)`); non-looped idles pick idle01/idle02 at random on
  `animationFinished` 0x157c1c (probabilities 0.75/0.9 approximated). Colors: every part takes
  the main color (`setObjectColor` 0x157be8); the color sprite (`setupChildSprites` 0x156f30:
  part 0 for 1327/1328/1584, part 1 for 2012, a `GJBeast01_03_001` child of part 1 for 918) the
  secondary color (default 1011 white). Glows (`<type>_NN_glow_001`) follow parts (position and
  rotation) under them, tinted by `updateChildSpriteColor` 0x157818 with the color
  updateVisibility computes: transformColor(background 1000, s −0.3, v +0.4 absolute) — the
  real (238,76,255) on Fingerdash. **That same color is probably every object glow's tint**
  (blackCogwheel glows are (169,101,255) in level 20; the app still draws object glows white).
- Black cogwheels 1734–1736: in play the main and color sprites only hold spinning children (the
  sprite dump caught them before setup): the main frame copies from the dump plus the same
  number of `_color_001` copies in the detail color (1734: four mirrored quarters, flips (0,0)
  (1,0) (1,1) (0,1)). animobj.rs builds that tree.
- Not ported: animate trigger 1585 (key 123 objects stay hidden), scale-fade rings 1839–1842
  (children scale ((p−3)(p−1)−1)·−0.7 / p²·0.7 + 0.1 and fade, p = 2k/n, n = 40/speed steps of
  0.02/speed), 1855/1858's extra child frames, 1592's color mapping, coin 1614's special frames,
  GJBeast05's third colored part (+0x5c8, `setChildColor`), the glows' second copies.

## Real-game captures, settings, practice, icon kit (2026-11-03 evening)

**Full frames only.** Full screen on a notched MacBook hides the top 33 of 956 rows (the scene is
still laid out over all 956; the notch covers them, and they read back as zero alpha). Older
captures and the level sweep had that band. Now:
- the harness's own save (`re/home/.../CCGameManager.dat`, never the real one) has game variable
  0025 = 1 (windowed, `VideoOptionsLayer::onApply` 0x7765b4) and `resolution` 8 = 1280 × 720
  (`GameManager::resolutionForKey` 0x303498: 1 640×480, 2 720×480, 3 720×576, 4 800×600,
  5 1024×768, 6 1152×864, 7 1176×664, 8 1280×720, ...). GD applies it at launch; changing it at
  run time makes GD relaunch itself, which fails under the harness (Geode.dylib not found).
  `cadence save-xml` / `save-write` edit that save. The window is 1x: 1280 × 720 pixels.
- the trace mod still crops blank top rows (`blankTopRows`) and its .txt keeps the full size plus
  `crop N`; `tools/recompare.py` redoes old comparisons by rendering ours at the full size and
  cropping the same rows (no real game needed). `compare_level.py` crops ours by `crop N` too.
- `gui <name> <path>` in a spec: menu screenshots (menu, settings, options, levels, garage, stats,
  creator, achievements, search), each from a fresh `MenuLayer` with the first-launch terms
  `FLAlertLayer` removed. Real ones in shots/gui-real/, comparisons in shots/gui-cmp/.
- Steam must be running for the test copy (after a reboot it isn't).

**Settings** (`OptionsLayer::customSetup` 0x7705b0): a `GJDropDownLayer` (0x5d466c: list 356 wide
at ((w−356)/2, (h−H)/2 − 5), chains at w/2 ± 156, back arrow at (24, h−23); slides from y = h in
0.5 s, ease in-out 2, background to 125; fast menu 0168 skips it). Menu at (w/2, h/2+80):
Account (−79, 0), How To Play (79, 0), Options (−79, −40), Graphics (79, −40) at width 130; Rate
(−105, −80), Parent (0, −80), Help (105, −80) at width 78. Music label (0.6) at (w/2, h/2−34),
slider 20 below; SFX 40 below Music; "Menu Music" toggle (0122 = off) at Music + (145, −35).
Vault lock at (w−20, h−18). Matches the real frame within a point.

**Options pages** (`MoreOptionsLayer::init` 0x771f04): GJ_square01 400 × 280, toggles in pages of
ten (`addToggle` pads the count to the next ten between groups), `nextPosition` 0x77432c: left
column w/2 − 160, right w/2 + 32, rows 48 apart from h/2 + 80. `createToggleButton` 0x4497f0:
check box 0.8, label left-anchored 8 right of the box, limited to 130 wide at 0.4. Info button
(0.5) at (−18, +16). Titles Gameplay ×3, Practice, Performance, Audio, Other. Defaults on: 0026
(auto-retry), 0027 (auto checkpoints). Applied in play: 0052 faster reset (0.5 s), 0135 / 0134
hide attempts, 0027 / 0068 auto / quick checkpoints, 0122 menu music.

**Practice** (`PlayerObject::tryPlaceCheckpoint` 0x36d0d8): a pending checkpoint is snapshotted
500 steps (100 with quick checkpoints, +0x5f5) after the last one, on the ground or flying, and
committed once the player survives another 500; a death drops it. Z places, X removes
(`markCheckpoint` / `removeCheckpoint`). Music: `StayInsideMe.mp3`, looping through deaths.
Leaving practice restarts the level. Checkpoint sprite `checkpoint_01_001`.

**Icon kit** (`GJGarageLayer`; init's decompile is cut short, so measured from the real 1280 ×
720 frame): gradient tinted 150, side art in three corners, tabs at (cx + (i−5)·34, cy + 30) in
the order cube, ship, ball, UFO, wave, robot, spider, swing, jetpack, trail, death effect; grid
12 × 3 from (cx − 165, cy − 35), 30 apart; panel square02 385 × 100 at (cx, cy − 65); arrows at
cx ∓ 220; dots 24.4 apart at y 25; stats at (w − 29 / w − 18, h − 12.5 − 15i). Counts: 485 cubes,
169 ships, 118 balls, 149 UFOs, 96 waves, 68 robots, 69 spiders, 43 swings, 8 jetpacks. Every
icon is selectable (Cadence has no unlocks; the real frame shows locks because the harness save
has none). The choice (save keys icon0..icon10, color1, color2) drives the level's player:
sprites, rig textures (`robot_01` → the chosen robot), player color channels, effects.

## Shader and gradient triggers (2026-11-03, `shaders.rs`, `gradients.rs`, game `fx.rs`)

Ported for Dash (22: 2903 ×18, 2904–2923 ×94); levels 20/21 use none. Decompiles in re/decomp
(`ShaderLayer__*`, `GJShaderState__*`, `ShaderGameObject__*`, `GradientTriggerObject__*`,
`GJBaseGameLayer__{triggerShaderCommand,updateShaderLayer,triggerGradientCommand,
updateGradientLayers}`); member names/offsets from the Geode bindings in
re/build/trace-mod/bindings (`GJShaderState` starts at ShaderLayer +0x198).
- **Shader source**: not in Resources — the fragment shader is a C string at 0x91ab52 (vertex
  shaders and cocos ones around 0x917db0..0x91e317). Ported line by line to
  `crates/cadence-game/src/postfx.wgsl` (GL texcoords, y up; `textureSampleLevel` everywhere
  since samples sit in non-uniform branches; own smoothstep since the game calls it with
  edge0 > edge1).
- **Keys** (`ShaderGameObject::customObjectSetup` 0x145f0c; absent keys are 0): floats 175
  +0x730, 176 +0x734, 177 +0x73c, 179 +0x740, 180 +0x744, 181 +0x74c, 182 +0x750, 183 +0x760,
  189 +0x748, 191 +0x738, 290/291 +0x754/8, 512 +0x764 (±1e9 clamp); bools 184 +0x75c, 185
  +0x768, 186 +0x769, 187 +0x76a, 188 +0x76b, 190 +0x76c, 192 +0x76f, 194 +0x76d, 513 +0x778,
  514 +0x779, 515 +0x76e, 531; 10 duration, 30 easing, 85 rate, 51 target (0..9999), 71 center/
  tint/ref channel, 138/200/201 → target −1/−2/−3, 196/197 min/max layer.
- **Per trigger** (`triggerShaderCommand` 0xf2cfc → `ShaderLayer::trigger*`, the argument
  order checked in the disassembly since Ghidra mixes float/int args): shockwave speed 175,
  thickness 180, strength 176, wave width 179, fade in/out 181/182, time offset 177, invert 184,
  inner 183, outer 191, max size 512, offsets 290/291, target 188, follow 190, relative 514,
  animate 513 (without it the values are set at once and the wave restarts); shockline the same
  plus rotate(axis) 186, flip 185, dual 187, offset 290 for both axes; chromatic x 180 (if 188)
  / y 189 (if 190); chromatic glitch speed 175, strength 176, rgb offset 180, line thickness
  179, line strength 191, segment height 189 (stored 1/max(0.1, h)), disable 192; lens circle
  size 179, fade 181, strength 176 (clamped 0..1), tint channel 71; radial blur size 179,
  intensity 176, fade 181, ref channel 71, empty only 515; bulge 176, radius 180; pinch x 180/y
  189, radius 512, modifier 179; grayscale 176, use luminance 188, tint channel 51 if 190; sepia
  176; invert 176 with RGB 179/180/189 (edit 188, tween 190, clamp 194); hue 176 degrees; edit
  color multiply (176, 191, 175) add (179, 180, 189); split cols 180 / rows 189; setup: layers
  196..197 (min < 2 → BG, max ≤ 0 → Max), 192 resets every shader.
- **Tweens** (`tweenValueAuto` 0x33d730, `GJValueTween::step` 0xcbdc0): from the current value,
  t = elapsed/duration (1 once elapsed ≥ duration), eased if easing > 0; −909190 means "leave".
  Stepped once a frame by the same frame seconds as `GJEffectManager::updateEffects`
  (`updateShaderLayer` is called right after it in `GJBaseGameLayer::update`), with m_time.
  Reset values (`GJShaderState::reset` 0x33a4d4): invert RGB 1, edit color multipliers 1, lens
  size 1, segment height 1.
- **Uniforms** (`performCalculations` 0x340d4c and the `pre*` functions, `FUN_10033c488` at
  visit): mult (+0x47c) = 1.2019706 (`DAT_100ad0890`, static init 0x1003416d4) / screen scale
  W (winSize.w / 480); × |zoom| when "relative". Shockwave: t += speed·Δtime; thick =
  0.2·thickness (0.0002 if ≤ 0.001); t2 = t + 0.1·time offset; r = t2·(invert ? 1 : t2) −
  fade in·thick (the ring grows with t², checked in the asm: fnmsub with fcsel); times = (r +
  fade in·thick + 0.001, r, min(r, r − thick) + 0.001, … − fade out·thick)·mult; uniform time =
  fmod(t2·22/width, 2π), 0 when the ring can't reach the screen (±√2 test); strength
  0.1·mult·s; waves 22/width/mult; max dist 1/(max size·0.5·mult). Shockline: same with
  0.01·strength and the inverted t2 = (dual ? 0.5 : 1)·(fade out + thick + fade in) − t2.
  Chromatic offset = target/(1.20197·winSize). Lens: start/end = max(0, size·mult − fade) /
  max(0, size·mult), origin y × h/w, tint = channel color (black without one), additive if the
  channel blends. Radial: value size/9·0.2, fade min(0.2·fade, 0.2), intensity uniform =
  intensity + 1, ref color = channel or the background sprite's color, used only when the
  layer doesn't start at BG. Hue: cos/sin of the degrees. Edit color: off (C = 0) when C = 1
  and B = 0. Invert: rgb·value (min 1 with clamp), a = value.
- **Layers** (`updateShaderLayer` reparents the layers into the ShaderLayer;
  `zLayerToString` 0x25d2ec): 1 BG, 2 MG, 3–7 B5–B1, 8 P, 9–12 T1–T4, 13 G, 14 UI, 15 Max. The
  layer renders into a render texture cleared to (0,0,0,0) and is drawn back with (ONE,
  ONE_MINUS_SRC_ALPHA). Cadence records where each layer starts in the draw list
  (`fx::LayerMarks`) and `Renderer::render_frame` draws draws[..start] to the frame,
  draws[start..end] to a texture, the post pass, then the rest. Dash's setups use max 15, 14,
  12, 11, 8.
- **Gradients** (2903; `GradientTriggerObject::customObjectSetup` 0x141f44): 174 blending,
  202 layer (0 if absent), 203/204/205/206 U/D/L/R groups, 207 vertex mode, 208 disable, 508
  disable all, 209 id (≤ 999), colors = the trigger's own main/detail channels (21/22, with
  41/43, 42/44 HSV), rotation 6, z = key 25 clamped ±5. `triggerGradientCommand` 0xf1794: one
  GJGradientLayer per id (a new trigger with the same id replaces it); blend: 0 (SRC_ALPHA,
  1−SRC_ALPHA), 1 (SRC_ALPHA, ONE), 2 (DST_COLOR, 1−SRC_ALPHA, premultiplied +0x231), 3
  (1−DST_COLOR, 1−SRC_COLOR, premultiplied); parents/z per layer (BG −54, MG −14, B5 −1201,
  B4 −901, B3 −601, B2 −301, B1 38, P 61, T1 399, T2 699, T3 999, T4 1399, G 46, UI 56, Max
  1400). `updateGradientLayers` 0xf1e28: vector = (cos, sin)(−rotation); rect = view (camera
  − 20 .. camera + winSize/zoom + 20 for world layers; −20..win + 20 screen points for BG, MG,
  G, UI, Max) with D → bottom y, L → left x, U → top y, R → right x (main objects' current
  positions); vertex mode: U bottom-left, D bottom-right, L top-left, R top-right; hidden when
  both opacities are 0 or additive with both colors black. `CCLayerGradient::updateColor`
  0x412a38 (compressed interpolation: u·√2/(|ux| + |uy|), corner weight (√2 ± ux ± uy)/2√2).
  Matches the real scene dumps (shots/22/real-*.scene): positions, sizes and blend funcs of
  the GJGradientLayers at steps 300 and 4480.
- Cadence: triggers fire through the existing paths (`groups.rs` maps 2903–2924 to
  `Action::Effect`, so position ones go through the order-channel walk and spawn/touch ones
  through the group triggers) → `World::fired_effects` → `LevelFx::fire`; `LevelFx::frame`
  next to `Colors::frame`. Groups' main objects = first object by level order with the group,
  at `World::level_position` (the world only tracks groups that move triggers use).
- Not ported / approximations: pixelate (the game renders the layer into a smaller texture;
  Cadence snaps UVs), motion blur target speeds (`speedForShaderTarget`), the glitch's exact
  random sequence, chromatic glitch "relative position" y offset, shader-state restore on
  practice checkpoints (a checkpoint load keeps the reset state), `m_noPlayerParticles`
  (188 on the setup trigger), camera rotation (+0x478) in the centers, gradient z within a
  layer (drawn under the layer's objects, the game's z sits between its blend and normal
  batches), the 0x231 premultiply only matters for modes 2/3.

## Icon kit, color page, glows, end portal, audits (2026-11-03 night)

- **Icon kit grid** (`GJItemIcon`): per-type scales measured on the real kit against the cube's
  0.8: ship 0.64, ball 0.68, UFO 0.72, wave 0.67, robot/spider 0.6, swing 0.59, jetpack 0.57.
  The preview is always the cube. Robots/spiders are assembled from their rig's `idle01` frame
  (`SimplePlayer`).
- **Color page** (`CharacterColorPage::init` 0x649ac0, `createColorMenu` 0x64aac0): GJ_square05
  450 × 300; close at the top left; Col1/Col2/Glow (bigFont on GJ_button_01 / 04, 0.85) from
  (w/2 + 88, top − 17), 50 apart; strip square02b 420 × 60 at top − 62 with the 8 vehicles 52 apart
  (1.15); swatches: 143 slots in rows of 18, 24 apart, palette index per slot from the table at
  0x1008fbc08 (`colorForIndex`), offset ×12 from `offsetForIndex` (0x64b60c: blocks 2 and 4 of a
  row +1 x, rows 3–7 −0.8 y, the gray row +0.4 y), around (w/2, h/2 − 59); ColorChannelSprite =
  GJ_colorBtn_001 at 0.65, both chosen colors bracketed (GJ_select_001 0.7). A popup over the kit.
- **Grayscale sprites** (`CCSpriteGrayscale`): sprite.wgsl takes a negative tint alpha as
  "grayscale": luminance × tint. Creator's Versus / The Map: color 175.
- **Glow frames**: the dump now includes each object's `m_glowSprite` (another batch node, not a
  child), so visuals.rs has the real glow frame per id (e.g. block 1 → blockOutline_01_glow) and the
  app uses it. Detached color sprites of the synthetic level are skipped (often empty).
- **Color sprites in levels** (`color_children.rs`): mined per object signature (frame + children's
  frames) from real scenes; any `_color_` direct child (z −100 behind or 100 above) or a z 100
  copy of the main frame holding the parts' color versions. The color sprite's opacity is the
  main's × its channel's (`GameObject::setOpacity` 0x5adee8). Glows follow the object's alpha.
- **Ground spikes** (9): one of pit_01..03 at random per object at load (real runs differ).
- **End portal** (`EndPortalObject` 0x310cb4 / `updateEndPos` 0x311280): at x = level length; a
  column of square_02 (−90°, 30 apart, 1.5 views tall), the additive gradientBar (player color 1,
  1.5 views tall) 15/zoom + 14 to the left, endEffectPortal particles at (−15, 0); y = the view's
  middle, at least 225 unless flying.
- **Checks**: `tools/scan.py` (anomalies, every level), `tools/sprite_audit.py` (sprites by
  frame name over every real frame; ancestors' visibility and the zoomed view respected),
  `tools/recompare.py` (one multi-shot run per level), `COMPARE_REAL_ONLY=1 compare_all.py` (real
  frames only). Harness: `gui garageN|colors|levels|...`, `pausegui <step> <path>`,
  `endgui <path>`, `dumpachievements <path>`.
- Open: Fingerdash's colorSpike_04 shows no glow in the real level (other levels do; not key 96);
  Dash's last section: some move-trigger groups end ~20 units off.

## Background, middle ground, touch camera triggers (2026-11-04; decompiled into re/decomp-bg)

- **Background** (`GJBaseGameLayer::updateCameraBGArt` 0x10f978, called by `updateCamera` with
  m_cameraZoom +0x1a8 as its first argument): the background sprite (+0xf68, created at 0, 0 by
  `createBackground`) is moved every frame, not placed from the camera:
  `delta = old view center − new view center`, where a center is camera position + win size ÷ zoom
  ÷ 2 (old: last position +0x4b4 at last zoom +0x2f0; new: +0x3e0 at +0x1a8). Then
  `bg.x += mirror · delta.x · speedX · zoom` (wrapped into (−width, 0], width = texture width ×
  scale, +0x484) and `bg.y += delta.y · speedY · zoom`. mirror = 1 − 2·flip (+0x3ec). speedX/Y
  (+0x314/+0x318) are 0.1 unless set (`updateBGArtSpeed`: |v| > 999 means 0.1). Flag +0x314b zeroes
  one frame's delta (reset), +0x313d every frame's. There is no clamp: `GJBaseGameLayer::visit`
  only crops the texture rect to between the ground and ceiling layers for drawing (position and
  rect move together, so the picture stays put). Real captures that start from a start position
  begin at 0 there, which is why some real frames show y ≈ 0 with the camera high.
- **Middle ground** (+0xf88, `GJMGLayer`), outside the "follow object layer" mode (+0x2fcc):
  `y = (mgY + offset) · ((1 − f) + f · zoom) − f · camera y`, f = +0x320 (0.5 in every Dash frame),
  mgY the layer's base (+0x1e4: 25 for fg_01), offset = +0x2bc, set by **middle-ground triggers
  (2999)**: key 29 offset y, 10 duration, 30 easing, 85 rate (`updateMGOffsetY` 0x104c14, tween id
  20 from the current offset). With Dash's offsets (24 from x 945, 48 from x 9075, 180 then 228 on
  order channel 10) the formula gives all 28 real Dash frames' MG y to 0.001. Its scale is
  `((1 − |1 − zoom|) · −0.3 + 1) · 1.2`. The camera position used is without shake.
- **Touch-triggered camera triggers** (key 11, e.g. Dash's rotations at x 6703/6883, scaled 3.278
  so they span the corridor): fire once when a player's rect meets the trigger's 30×30 square times
  its scale (keys 128/129, else 32), like group touch triggers. Missing them left Dash's camera at
  180° where the real game reaches 360°.
- **Gradient layers** (`updateGradientLayers` 0xf1e28): a layer is shown only when its quad
  overlaps the view's bounding circle; otherwise `vis=0` in the scene dump.
- **Change BG / ground / MG** (3029 / 3030 / 3031): key 533 is the art id; the art switches at
  once. Cadence keeps them as camera triggers (position, order channel, touch), the art in force in
  `Camera::art` (so deaths reset it and checkpoints keep it), and loads every image they can switch
  to with the level's own. Dash uses BG 53 → 25 → 53 (x 15105, channel 10 at 15765, 21405).

## The remaining 2.2 triggers (2026-11-04 night, trigger agent; decompiles from a copy of re/ghidra)

Dispatch is `EffectGameObject::triggerObject` (0x1599a8): one switch on the object id; camera ids go
through `CameraTriggerGameObject::triggerObject` (0x17820c), items through
`ItemTriggerGameObject::triggerObject` (0x17de74). Keys come from each class's
`customObjectSetup`: the value strings are an array indexed by key (`*param_2 + key·0x18`), the
"present" flags another (`*param_3 + key·8`); an absent float or int is 0 unless the code sets a
default. Helper prototypes for the decompiler (floats in s registers, CCPoint results through x8)
are in the scratch `DecompileTyped.java`; with them Ghidra keeps the float expressions
`processAdvancedFollowAction` otherwise loses. Ids and what they do in play:

- **Advanced follow (3016)** (`triggerAdvancedFollowCommand` 0xf1584, `processAdvancedFollowActions`
  0xfdfe8 / `processAdvancedFollowAction` 0xff8e4, run in `GJBaseGameLayer::update` after the
  player follows and before the follow triggers). An instance (trigger, group 51, target: group
  71, −1 / −2 players 138 / 200, −3 the camera 201, control id 534) per trigger; the same trigger
  again restarts it. Instances sort by priority 365 (highest first). Per object of the group, a
  state (velocity, direction, eased angle, smoothed steer force); the target's position, with a
  delay 292 from a history recorded each step (`updateSavePositionObjects`): i = ceil(delay /
  (1/240) in f64), the sample i steps back moved toward the next newer one by i − f; not enough
  history means no move. Skipped if out of range 308 or (336) the object is off. The start speed
  300 / direction 563 (`getFollowSpeedVal` 0xfef2c: angle (90 − dir)°, turned by group 560's
  movement or away from group 565) goes to the instance's first object only (its +0x1f flag is set
  after one object), set the first time (572: 1 always, 2 add). Acceleration 334 · 0.01. Mode 367:
  0 velocity = distance / easing 361 (≤ 1: snap, moved by 1 not dt); 1 friction 558 · 0.01 then
  acceleration along the normalized distance (`ccpNormalize` 0x1002d2fa8 turns (0, 0) into (1, 0)),
  both blended toward 357 / 561 inside 359 by t = 1 − d / near (fused); 2 steering: the direction
  turns toward the target by at most the smoothed steer force (316, or 318 below speed 322 with
  337, 320 from speed 324 with 338, 330 when braking: 326 > 0, speed > 332 and the turn > 328°),
  smoothed by force · 0.01 − s over 10 per step; velocity rotated with fused sin/cos; braking
  takes |326 · dir · dt| off each axis and the step's acceleration; acceleration along the target
  direction with 305. Modes 0/1: the direction eases toward the velocity's angle (frozen under
  364) by 363. Angles wrap with n = ceil(floor(|a| / π) · 0.5) turns (fma). Speed cap 298 (vx alone
  with x only 306, vy with y only 307). Rotate (339): a quarter of wrap180(fma(angle, −57.29578, 90)
  + offset 340 − rotation) per step, for objects that can rotate freely. 571: an object it moved
  isn't moved again by lower priorities that step. Edit (3660, `modifyObjectPhysics` 0xff7fc): the
  velocity × 566 / 568 plus a start speed; re-target (3661): the instances of the advanced follow
  triggers in group 51 (or with control id 51 when 535) follow the new target. Variances use
  `m_varianceValues` (+0x100c) at the object's `m_varianceIndex` (+0x3ec) + k; Cadence hashes.
  Group parents / linked objects moving as one (`m_targetGroups` +0xf00, `getSpecialKey` keys
  280 / 281) aren't ported.
- **Edge fade margin** (`m_fadeMargin` +0x34c, set at 0x1005ad3d4): not half the width — the size
  (content size × |scale|, swapped when turned a quarter, the diagonal at other angles) over 30
  gives width / 2 − 15, else 0. A 30-unit block fades from its center. (Real traces: Cadence's old
  half-width margin was 15 units early.)
- **Keyframes (3032 objects, 3033 trigger)** (`FUN_1000e6070`, `createKeyframeCommand` 0x26a6fc,
  `prepareMoveActions` mode 5). Keyframe objects: 373 group, 374 index, 10 duration to the next,
  30 / 85 easing, 71 / 557 the group spawned when reached and its delay, 377 spawn at 95%, 536
  direction (1 / 2 force the turn's sign), 537 revolutions, 375 / 379 reference-only time modes,
  378 curve (`KeyframeObject::setupSpline`), 376 close loop. The trigger: 76 animation group (the
  keyframe objects), 51 target (else the first keyframe's 51), 71 center, mods 520 time, 521 /
  545 position, 522 rotation, 523 / 546 scale: absent mods are 0, a y mod of 0 takes the x one
  (end of `customObjectSetup` 0x15c158), the time mod is at least 0.0001 — a 3033 without 520
  finishes at once. Positions and scales are taken relative to the first keyframe and scaled by
  the mods; each keyframe keeps its change from the previous one; per step, finished segments
  count fully and the current one by its eased progress; the summed offset (f32), rotation and
  1 + summed scale changes become this step's move (into the group's sum), and a rotation and a
  scale of each object in place (real traces: nothing orbits). Groups reached spawn at the next
  step's start. Not ported: curves, reference-only runs, close loop, the center object's own
  rotation turning the moves (the +0x628 list).
- **Enter effects** (`updateActiveEnterEffect` 0xd918c, `PlayLayer::updateVisibility` 0x7ae88,
  `applyEnterEffect` 0x7c570, `applyCustomEnterEffect` 0x7bdd0, `stopCustomEnterEffect` 0xd9c10).
  Codes per enter channel (344, objects choose theirs with 343) and side (217: 0 both, 1 enter =
  right of the view's middle, 2 exit): 22 → −2 (none, fade only), 23 −6, 24 −5, 25 −7, 26 −8, 27 −3,
  28 −4, 55 −11, 56 −10, 57 −9, 58 −12, 59 −13, 1915 −14 (no effect and no edge fade), 3017–3021 −15
  (custom). Every channel starts at −2 each attempt. The old codes lock on the object
  (`m_enterType` +0x3f0 / `m_exitType` +0x3f2) when it starts entering and release at t = 0 or 1;
  UI-pinned objects (+0x541) are −14. Custom effects: a list per channel and side, the same effect
  id replacing; t = side · ((x − (edge + 220)) + var) / int(222 + var), deadzone 282, t ≥ 1 skips;
  move 218 at angle 231 − 90 (or 237 / 239 × side with 241), rotate 270, scale 233 / 235, fade (the
  eased t times the opacity), tint 260 / 265 (not eased: the channel's share is 265 · (1 − t)), all
  eased by 242 / 243 except the tint. The rotate accumulator starts at 1 (0x10007bf24 stores 1.0
  into both the opacity and the rotation slot): an entering object with a rotate effect is 1°
  off. Custom effects have no edge fade. Enter stop (3023) removes the channel's effects with its
  id (all for 0); an emptied channel is −2 again. Not ported: `m_intrinsicDontFade` (+0x409, no fade
  under −2 for some ids), the transitions of the edit-enter triggers.
- **Camera**: gameplay offset 2901 (`updateGameplayOffsetX/Y` 0x104bec: +0x2a4 / +0x2ac, ints of
  28 / 29, axis 101, 58 / 59 "don't divide by the zoom"; 75 by default); camera mode 2925
  (`updateCameraMode` 0xe0c78: free mode 111, with 112 the padding 114 (0..1) and easing 113
  (1..40), like a mode portal); camera edge 2062 (`updateCameraEdge` 0x104a94: edge 164 → +0x420
  left, +0x424 right, +0x428 up, +0x42c down = group 51). In `updateCamera` the limits are max x =
  right.x − w/z, max y = up.y − h/z (else m_maxGameplayY − 150 − h/z), min x = max(left.x, 15),
  min y = down.y if > 0 (else 0); the target clamps to them inside the per-axis loop, the easing
  grows toward 24 over the last 60 units (an edge set by a trigger, or in platformer mode) unless
  moving away, then clamps again after the loop. Camera guide 2016 returns at once (editor
  guide).
- **Small ones**: random 1912 (fast_rand · 100 ≤ chance 10 → spawn 51, else 71); BG effect on /
  off 1818 / 1819 (`PlayLayer::toggleBGEffectVisibility` 0x82ca8: the glitter, +0x399d, never reset
  between attempts); reverse 1917 (`reversePlayer` → `doReversePlayer(!left)` at once, not in
  platformer: the going-left flag is set on the trigger's step, before the camera);
  end 1931 calls `PlayLayer::activateEndTrigger`, which is empty (`ret`) — inert, as is 3655
  (`FUN_1000e58fc` is empty); the end trigger with effects is 3600 (`activatePlatformerEndTrigger`);
  area stop 3024 (`controlAreaEffectWithID(51, −1, 0)`); reset 3618 (`activateResetTrigger`
  0x102164: group 51's collectibles (type 30) and breakables (21) re-arm, checkpoints too); persistent
  item 3641 (80 item, 491 on, 492 all, 493 reset, 494 timer: values survive restarts,
  `transferPersistentItems`); UI progress 3663 (`FUN_1000e5900`, platformer only: progress
  +0x340 = 602 or item 95's count, added with 603 else at least the old value, 0..99, then item 80);
  BG speed 3606 / MG speed 3612 (`updateBGArtSpeed` / `updateMGArtSpeed`: keys 143 / 144, |v| > 999
  → 0.1 / 0.1 and 0.3 / 0.5); touch 1595 (`runTouchTriggerCommand` + `GJEffectManager::playerButton`
  0x2713b4: 81 hold, 82 toggle mode 0 flip / 1 on / 2 off, 198 1 = player 2 only, 2 = player 1
  only, 89 dual mode; with an active dual-mode touch trigger player 1's presses only drive the
  triggers); spawn particle 3608 (`spawnParticleTrigger` 0x110b0c / 0xe4648: copies of the 2065
  objects of group 51 around group 71's main object (or the trigger), offsets 547 / 548 ± 549 / 550,
  rotation 552 ± 553 (+ the position object's with 551), scale 554 ± 555; each copy keeps its offset
  from the particle group's main object, turned and scaled; never-ending systems get duration 0);
  checkpoint 2063 (platformer: `PlayLayer::processCheckpoints` 0x7e75c spawns 604, marks a
  checkpoint at the trigger, the player (138) or group 71's main object, spawns 51; respawns spawn
  448); edit song 3605 / edit SFX 3603 (`activateSongEditTrigger` 0x10eddc, `applySFXEditTrigger`
  0x112928: channel 432, 417 stop, 418 volume 406 over 10, 419 speed 404 via `pitchForIdx`); link
  visible 3662 (`generateVisibilityGroups` 0xd8030: group 51's objects activate together in
  `preUpdateVisibility`; Cadence's culling already draws anything in view, so nothing to do);
  BPM 3642 is an editor guide.

## Death effects, editor guides, fast_rand, camera 370, area HSV (2026-11-05, death agent)

- **playDeathEffect** (0x1a176c): the effect id is GameManager +0x368 − +0x36c (the seeded
  value); in practice mode it's the default unless the player's `m_practiceDeathEffect` (+0x7fb,
  game variable 0100) is set. The player fades out over 0.05 s (`CCFadeTo` tag 4). A switch on
  id − 2 (jump table at 0x1008e14c0) holds one case per effect 2–20, the default case is effect
  1. Each case: a `CCSprite` of `playerExplosion_NN_001` (NN = id − 1; 15, 17–20 start from
  `_002`) added to the object layer (`m_objectLayer`, PlayLayer +0xf50) at z 10000, at the
  player (6 higher for 9, 13–16), scale = size · roll (size = vehicle size × 0.9 when ≥ 1;
  rolls 1.5 ± 0.2, 1.2 ± 0.2, 1.4 ± 0.2, 1.2 ± 0.1, 1.25 ± 0.15, 1.3 ± 0.15, 1.3 ± 0.2,
  0.8 + 0.1R, 1 ± 0.1, 1.5 ± 0.3, 1.25 ± 0.1, 0.9 + 0.2R, 1 + 0.4R, 1 + 0.2R, 1.4 + 0.4R,
  1 + 0.3R, 1 + 0.2R, 1 + 0.3R, 1 + 0.3R for 2..20), rotation ±45 (8: ±90, 9: −45R, 16: 360R,
  18/19: ±180, 7/11/17: none, 14: 90/180/270 at a quarter each in turn), additive
  (`setBlendFunc(GL_SRC_ALPHA, GL_ONE)`) for 9, 11, 14, 15, 18; a `CCAnimation` of frames
  start..last at a rolled delay (e.g. 2: frames 1 or 2 (coin) to 11 at 0.05 + 0.01R), then
  `removeMeAndCleanup`; beside it `CCDelayTime(d · a)` + `CCFadeOut(d · b)` — for 12–15 the case
  builds that sequence but never runs it, 16 and 20 have none, 7 uses fixed 0.44 / 0.33.
  Effect 11 tints white → main color over 2 delays, then → secondary over 8. The particles:
  `explodeEffect`, `explodeEffectGrav` or `explodeEffectVortex` grouped, at z 99 / 101 / 1000,
  with their own start / end colors (vars 0,0,0,1), speed × 0.8–1.2, node scale (size, the
  sprite's scale or both), position spread × 1.2, start / end size × 0.5–1.2 / × 0.1–0.5, angle
  spread 30 and normal blending for the fire ones (4, 7, 17, 19, 20). Two `CCCircleWave`s:
  size·10 → size·(85..125) over 0.4–0.6 s (or delay × 10 / 14), the first at z 1000 (over the T
  layers), the second at z 99 drawn as a ring (+0x160 = 1, line width +0x164 = 10 pixels:
  `glLineWidth` counts framebuffer pixels), colors and opacity mods per effect (table in
  `crates/cadence-game/src/death.rs`). Every effect but 20 ends with the shatter
  (`ExplodeItemNode`, game variable 0153 off). The sheet is loaded by
  `PlayLayer::setupHasCompleted` → `GameManager::loadDeathEffect` only for the chosen effect.
  Actions (sprite animation, fades, the waves' tweens) start one frame late: cocos2d's first
  action step runs at elapsed 0, so frame 1 after death still shows the created frame / the
  start radius. The death sound is `explode_11.ogg` for every effect.
- **Checked**: the harness's new `deatheffect N` (per attempt), `playericon` and `deathshot
  <frame> <path>` (frames after death: screenshot, scene dump, particle systems, every
  CCCircleWave's radius / opacity / color / outline / width / opacity mod) recorded all 20 on
  Stereo Madness's first spike; `CADENCE_CHECK_DEATH=<dir>` reads the real rolls back (sprite
  scale, rotation, first frame and delay fitted to the captured frames) and steps Cadence's
  effect with them: 20/20 agree in every frame, opacity, color, position, blend, wave radius /
  alpha and particle setting.
- **Editor guides**: Camera Guide 2016 (`LevelEditorLayer::updateDebugDraw` 0xba678, keys 371
  zoom held to 0.4..3, 28 / 29 offset, 506 preview opacity; drawn only above 0): a green
  480 × 320 ÷ zoom rectangle, and an orange (255, 128, 0) one of the window's size ÷ zoom when
  the window isn't 3:2, both centered on the object plus the offset with the center held at
  least half the height up; a yellow line at the object's x (minus the layer's gameplay offset
  +0x2a4 ÷ zoom, 0 in the editor) across the green one; CCDrawNode lines of radius 0.25 ÷ the
  layer scale. BPM 3642 (`DrawGridLayer::draw` 0xbda9c, only with GameManager's
  `showBPMMarkers` +0x39a, saved, off by default, toggled by the edit tab's `edit_eBPMBtn`):
  keys 498 BPM, 499 speed index (0 normal 311.58, 1 slow 251.16, 2 fast 387.42, 3 faster 468,
  4 fastest 576 units/s), 501 beats per bar, 500 disabled, 10 duration; a 1-pixel line every
  speed · 60 / (BPM · max(1, 501)) from the object's x for duration · speed, every 501st yellow
  and the rest orange (all yellow when 501 < 1).
- **fast_rand** (`GameToolbox::fast_rand` 0x44d2b8): seed = seed · 214013 + 2531011 (64-bit),
  result (seed >> 16) & 0x7fff; `fast_rand_0_1` = that / 32767. Seeded by `fast_srand` in
  `PlayLayer::resetLevel` from `gettimeofday` (seconds · microseconds) unless a stored seed
  (+0x3208) is used. Random 1912: rand01 · 100 ≤ chance; advanced random 2068
  (`getRandomGroupID` 0x161e14): k = round(sum of weights · rand01) and the first entry whose
  running sum reaches k. Advanced follow, spawn triggers' variance and `tryGetObject` draw from
  the same generator.
- **Camera key 370** (+0x6e0 → GJBaseGameLayer +0x312, set by `updateCameraMode` and by the
  mode portal in `playerWillSwitchMode`): `animateInDualGroundNew` doesn't floor the corridor
  to the 30 grid.
- **Area tint HSV** (`processAreaTintGroupAction` 0xfaafc, key 278): per target with weight
  v < 1, the object's current color through `transformColor(getMultipliedHSV(1 − v, hsv 49))`;
  `getMultipliedHSV(k)` (0x44aa84): h · k, s and v · k when absolute, else 1 + (x − 1) · k.
  Area triggers (3006–3015) are object type 45 and fire by position like type 20 triggers.
- **Platformer slopes**: `m_slopeAngle` (+0x5f0) keeps its value off slopes in the real traces
  (0.785 after leaving a 45° slope); `updateMove` only reads it while on the slope.

## Gameplay leftovers for v0.0.2 (2026-11-05, gameplay agent)

- **Platformer slope slides** (`performSlideCheck` 0x3702e8, inlined in FUN_10036e834's platformer
  part): with `m_currentSlope2` (+0x5e0, the slope landed on through collidedWithSlopeInternal's
  hitGround path) and the player on or just off a slope, not resnapped, the slope not a grip
  slope (key 193, +0x512) and |m_slopeAngle (+0x5f0, atan(h/w), kept after leaving) · 57.29578|
  > 40: m_forcedSlideLeft/Right (+0xaf9/+0xafa) = left on an uphill slope right side up (right
  on a downhill one, mirrored upside down; uphill is +0x5f4 = the slope's uphill flag xor
  sideways), m_changedDirectionsTime when it changes, m_slideSlopeAngle (+0xb00, degrees),
  m_forcedSlopeSlide = 1; else both flags clear. updateMove (already ported) then pushes the
  velocity down the slope (the 45° slope in re/traces/pfslope: −0.1617 a step against the
  +0.1125 acceleration, so a held direction loses 0.049 a step and slides back at the top).
  Also: collidedWithSlopeInternal's "left" in platformer is the x velocity's sign (not the
  facing), so are the slope rotation's sign and the downhill test; a landing faster than 5 down
  on a slope over 40° halves a rightward speed; an uphill slope of ≥ 1 rad sets the slope jump
  count to 3; hitGround's platformer fields (ice, sliding, m_lastLandTime) on slopes too;
  preSlopeCollision in platformer: the right-hand wall strip counts too, no 1-unit inset, strips
  5 shorter at both ends on/just off a slope; the exit launch (FUN_10037013c) in platformer is
  ±|vx·tan(m_slopeAngle)|·1.1, only with a direction held and |vx| ≥ 5·cos(angle) − 1;
  leaving a slope holding the way it slid ends the slide bonus and sets m_isMoving (k 0.15 in
  the slow-down); updateRotation's dispatcher (0x3753e8) doesn't ease a grounded cube / robot /
  spider the step after a slope (m_wasOnSlope). pfslope matches (positions and rotation); the
  last also fixed s12–s16-22's rotation.
- **Keyframes** (`createKeyframeCommand` 0x26a6fc, `setupSpline` 0xcb680, mode 5 of
  prepareMoveActions): see keyframes.rs and spline.rs. The spline is tk::spline's cspline with
  zero second derivatives at both ends (set_points 0x10043abc0: band-matrix LU with fma in
  l_solve / r_solve / elimination, b = fma(fma(c_i, 2, c_{i+1})·(−1/3), h, Δy/h); operator()
  0x10043c490 Horner with fma), knots t = cumulative max(duration, 1e-5), values cumulative
  position deltas from the run's first keyframe. Two keyframes at the same place: the first
  can't curve; a run needs two curve keyframes. Reference-only time modes split the run start's
  own duration (mode 1 evenly over the segments including its own, 2 by f32 ccpDistance), mode 0
  keeps them; the run is eased as one over refRunDuration (own + refs' in mode 0, own in 1/2)
  when the start keyframe eases. Close loop (the last keyframe's 376) appends the first keyframe
  again (base reset, scale 1). The previous keyframe's rotation and scale are kept as f32.
  Real re/traces/kfcurve matches to print precision except while an object turns: the real
  object drifts up to 1.5e-4 units from its place while rotating in place (also in the older kf2
  trace) — not explained.
- **Camera mode trigger 370** (`updateCameraMode` 0xe0c78): a change of free mode calls
  updateDualGround → the corridor around the last mode portal with the trigger's 370; and
  GJBaseGameLayer::update stores the step's ceiling (+0x6bc) after the trigger walk (the
  "moving ceiling" test compares with it). re/traces/cam370t matches bit for bit (camera too).
- **Dual deaths** (`PlayLayer::destroyPlayer` 0x7d960, `playerDestroyed` 0x37b6d8,
  `playExitDualEffect` 0xe5200): the dying player's death effect; the other player is hidden at
  once (opacity 0 in the real scene) and a SimplePlayer copy of it fades over 0.4 s, moves
  124.632 screen units back (±, mirrored), scales to ±0.1, turns 180°, with an outlined
  CCCircleWave 10 → 30 (0.4 s) in its main color following it. playDeathEffect fades the dying
  player out with CCFadeTo(0.05) a frame late (real opacities 255/255/170/84/0).
- **Glows**: `GameObject::setObjectColor` (0x5b8708) calls setGlowColor with the object's color
  unless m_customGlowColor (+0x431); m_cantColorGlow (+0x433, set only in setupCustomSprites for
  some types — not modelled) blocks it. Black objects keep a black main sprite but the glow gets
  the channel color. The glow batch blends 770,1 (SRC_ALPHA, ONE). Mean frame diff over 20 real
  frames of levels 1/9/16/20: 7.43 → 6.89.
- **Editor**: DrawGridLayer::draw (0xbda9c) draws the music playback line in green (50,255,50)
  at posForTime(song time), glLineWidth 5 while playing; after stopping it stays at +0x1e4 with
  alpha 100, width 3, until a playtest. The orange (255,150,0) width-2 line is
  LevelEditorLayer +0x38f8 = m_previewPosition, set by EditorUI's "Preview Locked." action and
  saved with the level (GJGameLevel +0x238) — not the music. Cadence has no preview lock, so it
  isn't drawn.
- **Explode toggle**: GJGarageLayer shows it as !0153; in playDeathEffect 0153 only skips the
  ExplodeItemNode (shatter) — real capture re/traces/noexplode.
- **Ship fires** (GameManager m_playerShipFire +0x35c; PlayerObject m_shipStreakType +0x8e4,
  m_shipStreak +0x778): see `ShipFire` in app.rs (textures, frame times, offsets, the fade /
  stroke table FUN_100364938). Only shown in ship mode, not in platformer (toggleFlyMode).
- **Edit song / SFX** (`applySFXEditTrigger` 0x112928): +0x7a4 stop (417), +0x7a5 volume (418,
  value +0x75c = 406), +0x7a6 speed (419, value +0x750 if > 0 else pitchForIdx(+0x754 = 404)),
  both FMOD updateChannel tweens over +0x5ac (10). (The bindings name 418/419 the other way
  round.) Proximity: see groups::Proximity; the SFX trigger (3602) itself also has it
  (activateSFXTrigger: initial volume × volumeForProximityEffect, then the same effect) — not
  wired for 3602, nor SFX speed (the audio backend has no per-sound rate).
- **Group parents / linked objects (not ported)**: special group keys `getSpecialKey(g, ignore
  parent 280, ignore linked 281)` = (280 ? 110000000 : 100000000) + g + (281 ? 1000000 : 0).
  FUN_1000d64d0 (after sortStickyGroups, which renumbers linked groups key 108 → +0x368 and
  fills m_linkedGroupDict +0xf28) builds, for each target group of an advanced follow (3016,
  0xbc8) or area trigger (object type 45) with its 280/281 combination, four entries in
  m_targetGroupsArray (+0xef8) and m_targetGroups[key] = (index + 1, count): A = objects moved
  alone, B = cluster leaders, D = leader uid → its cluster's objects (a linked group's array, or
  for a parent cluster −g the group g's objects), E = leader uid → the cluster's group-parent
  object (prefers +0x47e, key 34 "group parent"; leaders prefer +0x47f, key 279 "area parent").
  Clusters: an object's linked group; unless 280, objects that are group parents (+0x492, child
  groups in m_parentGroups +0xe70) pull their child groups' objects into cluster −child. No
  clusters at all → (0, count) and the plain group is used. processAdvancedFollowAction then
  runs pass 0 over A (each object alone) and pass 1 over B, moving D[leader] by the leader's
  step and rotating D[leader] about the leader's unmodified position; addAreaEffect stores the
  same (index, count) in the effect instance. tryGetGroupParent(g) = m_parentGroupsDict[g][0],
  else the first object of g with +0x47e.

## Group parents, linked objects, main objects (2026-11-05, parents agent; ported, traced)

- **Keys**: 274 = the groups this object is the parent of ("." separated; `loadGroupParentsFromString`
  0xf6578 → `FUN_1000f5770` appends it to m_parentGroupsDict(+0xe68)[g] and the group to
  m_parentGroups(+0xe70)[uid], +0x492 set; a "+" item (`FUN_1000f58d8`) moves an object already
  listed to the front). 34 = m_hasGroupParent (+0x47e), 279 = m_hasAreaParent (+0x47f), 108 =
  linked group (+0x368, renumbered by sortStickyGroups). Triggers: 280 ignore group parent
  (+0x728), 281 ignore linked (+0x729).
- **tryGetMainObject(g)** (0xe6328): m_parentGroupsDict[g][0], else the group's object when it
  has exactly one, else null. Used by advanced follow's target (`FUN_1000e7310`), area centers,
  rotate centers, move targets, follow. `tryGetObject` (0xdf090, getFollowSpeedVal's 560 / 565):
  the main object else a random object of the group (fast_rand). An area center without a main
  object stays at (0, 0) (the CCPoint is zero-constructed), it doesn't skip.
- **Clusters** (`FUN_1000d64d0`, decompiled again, details): the per-object cluster map starts with
  every object's linked group; the parent pass (unless 280) walks the group's objects with
  +0x492: for each child group c of one (only the group's explicit parent and c == g when g has
  one) every object of c leaves the map and, if it is the parent object or has 34 / 279, is
  offered as leader (replaces the current one when it has 279 and the current hasn't, or it's
  the parent object) and as parent object (same with 34); the leader then enters the map as
  −c (even if it's not in g). The linked pass (unless 281) offers objects with 34 / 279 for
  their linked cluster — a linked cluster with neither flag has no leader and its objects move
  alone. Final walk over g: cluster 0 or no leader → A (alone); the leader → B, D[uid] = the
  linked group's every object (m_linkedGroupDict, not only those in g) or group c's objects,
  E[uid] = the parent object; other members are skipped (their leader moves them).
- **Advanced follow** (`processAdvancedFollowAction`, checked in the disassembly at 0x1000ffeb8..
  0x1000fff50): pass 1 moves D[leader] by the leader's step (`moveObjects`), then, when the
  leader turns, `rotateObjects(D, turn, leader's unmodified position after the move, offset =
  the step)` — the offset moves the cluster a second time, so a turning cluster moves 2× the
  step (the real trace shows it: a rotating linked cluster trails the player by half the solo
  object's distance). `rotateObjects` (0xfe820): per object, p = f32(pos − area offset), child
  node at p − center under a pivot node at center turned by the angle; pos += (concat.t + offset
  − p) in doubles; objects with m_canRotateFree turn. The concat's translation is
  fma(dx, cos, dy·−sin) + cx and fma(dx, sin, dy·cos) + cy (only this contraction matches the
  trace to print precision; plain f32 or the other order drift 1e-3 over 1000 steps).
- **Area effects** (`FUN_1000fb3a4` 0xfb3a4): processAreaEffects for the scale list, then rotate,
  then move (fades / tints in processAreaVisualActions). Each call: A with the plain action,
  then B with the cluster flag: the leader's value applies to D[leader] (move: each member reset
  and offset by the leader's offset; rotate: `rotateAreaObjects` about E[leader] after resetting
  it, the offset into the area offset, +angle on free-rotating ones; scale: `transformAreaObjects`
  about E with the pivot scaled; fade / tint: setAreaOpacity / tint on every member). After the
  three passes the objects reset the step before (`resetAreaObjectValues` appends to +0xd48,
  moved to +0xd60) whose reset step is older than this one are reset (offsets and rotations
  off): an object whose leader or itself left the section window doesn't keep a stale offset.
  The center is reset first only for move / rotate / scale (`param_4 − 5 < −2`).
- **Visual timing**: a node's area rotation (and the node position) only changes once per drawn
  frame in the real watch files (steps 423, 427, … at speed 4) while the f64 positions change
  every step; the suite compares rotations at step % 4 == 3.
- **Not ported**: spawn remaps of the target groups (`FUN_1000d64d0` also gathers 1268's remap
  pairs), +0x518 objects (skipped by cluster moves), rotated objects' skew under an uneven
  cluster scale, and the parent-first main object for the other triggers (rotate / move / follow
  still use the group's first object).

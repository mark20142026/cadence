//! Group triggers: move (901), toggle (1049) and spawn (1268). They act on every object carrying
//! the target group (key 57, `a.b.c`).
//!
//! Timing, from `GJBaseGameLayer::update`: each physics step first runs the queued move
//! commands (`GJEffectManager::prepareMoveActions` → `processMoveActions`), then the player
//! update and collisions, then activates the triggers the player has passed. A move trigger
//! passed in step s therefore gets its first tick in step s + 1, and that tick moves nothing
//! (elapsed time starts counting on the second tick).
//!
//! A move is `GroupCommandObject2::step` / `updateAction`: elapsed time in f32, t =
//! elapsed / duration in f64 rounded to f32, the eased value e, then the offset so far is
//! `amount − amount·(1 − e)` with one rounding (an `fmsub`). Each step moves the group by the
//! change of that offset (f64), rounded to f32 once per group, and objects keep their position
//! in f64 (`GameObject` +0x3b0/+0x3b8). Checked against the real game on Robot's first moving
//! platform: all 168 sampled steps bit for bit.

use std::collections::HashMap;

use crate::easing;
use crate::level::{LevelString, RawObject};
use crate::world::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveSpec {
    pub group: i32,
    pub dx: f32,
    pub dy: f32,
    /// Seconds (the trigger's f32 duration; the command keeps it as f64).
    pub duration: f32,
    pub easing: i32,
    pub rate: f32,
    /// Keys 58/59: also move by the player's movement over the last step.
    pub lock_x: bool,
    pub lock_y: bool,
    /// Keys 143/144: the lock's multipliers on the player's movement (0 means 1;
    /// `runMoveCommand` +0x80/+0x88).
    pub lock_mod: (f32, f32),
    /// Move to target (key 100): (target group 71, center group 395 or the moved group, axis
    /// 101: 0 both, 1 x only, 2 y only). The offset is worked out when the trigger fires.
    pub target: Option<(i32, i32, i32)>,
}

/// A rotate trigger (1346): keys 51 group, 71 center group, 68 degrees plus 69 full turns,
/// 10 duration, 30 easing, 85 rate, 70 lock the objects' own rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotateSpec {
    pub group: i32,
    pub center: i32,
    pub degrees: f32,
    pub duration: f32,
    pub easing: i32,
    pub rate: f32,
    pub lock: bool,
}

/// Scale trigger (2067, `TransformTriggerGameObject`): keys 51 target, 71 center, 150/151
/// scale x/y (0 → 1), 153/154 divide by the value, 133 only move (positions scale, the
/// objects keep their size), 10 duration, 30 easing, 85 rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleSpec {
    pub group: i32,
    pub center: i32,
    pub sx: f32,
    pub sy: f32,
    pub duration: f32,
    pub easing: i32,
    pub rate: f32,
    pub only_move: bool,
}

/// An item trigger's fields: 80 / 95 item ids, 476 / 477 their kinds (0 none, 1 item, 2 timer,
/// 3 points, 4 level time, 5 attempts), 478 the target's kind, 479 / 484 mods, 480 / 481 / 482
/// operations (1 add, 2 subtract, 3 multiply, 4 divide; Item Edit's 480 is the assignment: 0 =,
/// 1 +=, 2 −=, 3 ×=, 4 ÷=), 483 tolerance, 485 / 486 rounding (1 round, 2 floor, 3 ceiling),
/// 578 / 579 sign (1 absolute, 2 negative), 51 true / target group, 71 false group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemSpec {
    pub id1: i32,
    pub id2: i32,
    pub kind1: i32,
    pub kind2: i32,
    pub target_kind: i32,
    pub mod1: f32,
    pub mod2: f32,
    pub op1: i32,
    pub op2: i32,
    pub op3: i32,
    pub tolerance: f32,
    pub round1: i32,
    pub round2: i32,
    pub sign1: i32,
    pub sign2: i32,
    pub target: i32,
    pub other: i32,
}

impl ItemSpec {
    pub fn from_raw(raw: &RawObject) -> ItemSpec {
        let i = |k: u32| raw.get_i32(k).unwrap_or(0);
        ItemSpec {
            id1: i(80),
            id2: i(95),
            kind1: i(476),
            kind2: i(477),
            target_kind: i(478),
            mod1: raw.get_f32(479).unwrap_or(1.0),
            mod2: raw.get_f32(484).unwrap_or(1.0),
            op1: i(480),
            op2: i(481),
            op3: i(482),
            tolerance: raw.get_f32(483).unwrap_or(0.0),
            round1: i(485),
            round2: i(486),
            sign1: i(578),
            sign2: i(579),
            target: i(51),
            other: i(71),
        }
    }
}

/// `x op y` for an item trigger's operation (1 add, 2 subtract, 3 multiply, 4 divide; dividing
/// by zero gives 0).
pub fn item_op(op: i32, x: f64, y: f64) -> f64 {
    match op {
        1 => x + y,
        2 => x - y,
        3 => x * y,
        4 if y != 0.0 => x / y,
        _ => 0.0,
    }
}

/// Rounding (1 round, 2 floor, 3 ceiling) then sign (1 absolute, 2 negative absolute).
pub fn item_shape(x: f64, round: i32, sign: i32) -> f64 {
    let x = match round {
        1 => x.round(),
        2 => x.floor(),
        3 => x.ceil(),
        _ => x,
    };
    match sign {
        1 => x.abs(),
        2 => -x.abs(),
        _ => x,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// A color (29/30/104/105/221/717/718/743/744/899/900/915), pulse (1006) or alpha (1007)
    /// trigger fired by a spawn or a touch: the app's color system runs the object at this level
    /// order (`World::fired_effects`).
    Effect { order: u32 },
    Move(MoveSpec),
    Rotate(RotateSpec),
    Scale(ScaleSpec),
    /// Follow trigger (1347): for `duration` seconds group 51 moves by the movement of group
    /// 71's main object times (72, 73).
    Follow { group: i32, followed: i32, mod_x: f32, mod_y: f32, duration: f32 },
    /// Follow Player Y (1814): for `duration` seconds each object of group 51 eases its y toward
    /// the player's y `delay` seconds ago plus `offset` (`processPlayerFollowActions`): a
    /// `speed`·dt·60 share of the gap per step (at most 1), capped at `max_speed`·dt·60 if set.
    FollowY { group: i32, speed: f32, delay: f32, offset: i32, max_speed: f32, duration: f32 },
    Toggle { group: i32, on: bool },
    /// Spawn (1268): key 63 delay; key 441 "ordered": the group's triggers fire in x order,
    /// each delayed by (x − first x) / 311.58 s (`spawnObjectsInOrder`). `remap`: index into
    /// `GroupTriggers::remaps` (0 = none) of key 442's group remap (`a.b.c.d`: a → b, c → d),
    /// which the spawned triggers' target groups go through.
    Spawn { group: i32, delay: f32, ordered: bool, remap: u16 },
    /// Collision trigger (1815): once armed, toggles `group` (on if 56 "activate group") when
    /// collision blocks `a` and `b` (keys 80 / 95) start touching, or stop if 93 "on exit".
    /// Player 1 / 2 (keys 138 / 200, 201 both players) are blocks −1 / −2.
    Collision { a: i32, b: i32, group: i32, on: bool, exit: bool },
    /// Instant Collision (3609): spawns 51 if blocks `a` and `b` touch now, else 71.
    InstantCollision { a: i32, b: i32, yes: i32, no: i32 },
    /// Song trigger (1934): play custom song 392 from 408 ms (406 volume, 404 speed, 413 loop,
    /// 432 channel); the app plays it.
    Song { id: i32, start_ms: i32, volume: f32, speed: i32, looped: bool, channel: i32 },
    /// Event trigger (3604): once armed, spawns 51 whenever one of its events (key 430,
    /// `GJGameEvent` ids) happens to the player.
    EventLink { order: u32, target: i32 },
    /// Sequence (3607): each activation spawns the next group of 435 ("group.count..."; a
    /// group is spawned `count` activations in a row); 436 mode 0 stop at the end, 1 loop,
    /// 2 keep the last; 437 the least seconds between activations.
    Sequence { order: u32, min_interval: f32, mode: i32 },
    /// Timer (3614): timer 80 starts at 467 seconds, counting at 470× (469 ignores time warp),
    /// paused if 471; with 474 it stops at 473 and spawns 51. 468: don't restart a running one.
    Timer { id: i32, start: f32, stop: Option<f32>, rate: f32, target: i32, paused: bool, keep: bool },
    /// Time Event (3615): spawns 51 when timer 80 reaches 473 (475: every time, else once).
    TimeEvent { id: i32, time: f32, target: i32, multi: bool },
    /// Time Control (3617): starts (472 = 0) or stops timer 80.
    TimeControl { id: i32, stop: bool },
    /// SFX trigger (3602): play library sound 392 at volume 406 (404 speed, 405 pitch, 413 loop
    /// and the fades aren't used).
    Sfx { id: i32, volume: f32 },
    /// Animate (1585): group 51's animated objects play animation 76 from now (objects set to
    /// animate only on a trigger start playing).
    Animate { group: i32, anim: i32 },
    /// On Death (1812): once armed, toggles 51 (on if 56 "activate group") when the player dies.
    OnDeath { group: i32, on: bool },
    /// Player Control (1932, `activatePlayerControlTrigger`): 540 stop jump (releases the
    /// button), 541 stop move (platformer directions), 542 stop rotation, 543 stop slide.
    PlayerControl { stop_jump: bool, stop_move: bool, stop_rotation: bool },
    /// Teleport trigger (3022): player 1 to group 51's main object (352 / 353 keep x / y, 354
    /// gravity after: 1 normal, 2 flipped, 3 toggled).
    Teleport { group: i32, keep_x: bool, keep_y: bool, gravity: i8 },
    /// Item Compare (3620) / Item Edit (3619), `ItemTriggerGameObject`.
    ItemCompare(ItemSpec),
    ItemEdit(ItemSpec),
    /// Count trigger (1611): once armed, toggles `group` when item `item` reaches `target`.
    Count { item: i32, target: i32, group: i32, on: bool, multi: bool },
    /// Instant count trigger (1811): compares now (mode 0 equal, 1 larger, 2 smaller).
    InstantCount { item: i32, target: i32, mode: i32, group: i32, on: bool },
    /// Stop trigger (1616): cancels the actions started by triggers in `group`.
    Stop { group: i32 },
    /// A spawn-triggered trigger Cadence doesn't run yet: kept so ordered spawns see it.
    Noop,
    /// Time warp (1935, key 120): the game-time scale (from the next frame).
    TimeWarp { value: f32 },
    /// Area move (3006, `EnterEffectObject`): see `area.rs`.
    Area(crate::area::AreaSpec),
    /// End trigger (3600) fired by a spawn (the Towers' doors): the level is complete.
    End,
    /// Gravity trigger (2066, key 148): the player's gravity factor (`m_gravityMod`, 0xb24),
    /// which updateJump multiplies 0.958199 by.
    Gravity { value: f32 },
    /// Pickup trigger (1817): key 80 item, 77 count, 139 set instead of add.
    Pickup { item: i32, count: i32, set: bool },
    /// Advanced random (2068): key 152 "group.weight.group.weight…", an index into
    /// `GroupTriggers::randoms`; spawns one of the groups.
    Random { table: u16 },
    /// Hide (1612) / show (1613) player.
    PlayerVisible { on: bool },
    /// Shake trigger (1520): key 75 strength, 84 interval, 10 duration.
    Shake { strength: f32, interval: f32, duration: f32 },
    /// Random trigger (1912, `EffectGameObject::triggerObject` 0x778): spawns `a` (51) when
    /// fast_rand_0_1() · 100 ≤ the chance (key 10), else `b` (71).
    Chance { a: i32, b: i32, chance: f32 },
    /// Area stop (3024): `controlAreaEffectWithID(key 51, −1, 0)` removes the area effects with
    /// that effect id (225).
    AreaStop { id: i32 },
    /// Reset trigger (3618, `activateResetTrigger` 0x102164): the collectibles, breakable blocks
    /// and checkpoints of group 51 can be taken again.
    Reset { group: i32 },
    /// Persistent item (3641, `activatePersistentItemTrigger` 0x103264): 80 item (or timer with
    /// 494), 491 persistent on/off, 492 every item, 493 also reset it to 0.
    Persistent { item: i32, on: bool, all: bool, reset: bool, timer: bool },
    /// UI progress (3663, `FUN_1000e5900`, platformer only): the level's platformer progress
    /// becomes key 602 (or item 95's count with 605), added to it with 603, else at least the
    /// old value; 0..99. Item 80 then counts it.
    Progress { value: i32, add: bool, item: Option<i32>, counter: i32 },
    /// BG effect on (1818) / off (1819) (`PlayLayer::toggleBGEffectVisibility`): the glitter.
    BgEffect { on: bool },
    /// BG speed (3606) / MG speed (3612) (`updateBGArtSpeed` / `updateMGArtSpeed`): keys 143 /
    /// 144 the x / y speed factors; |v| > 999 means the default (0.1 / 0.1 BG, 0.3 / 0.5 MG).
    ArtSpeed { mg: bool, x: f32, y: f32 },
    /// Edit song (3605) / edit SFX (3603): see `AudioEdit`.
    AudioEdit(AudioEdit),
    /// Touch trigger (1595, `runTouchTriggerCommand` / `GJEffectManager::playerButton`): from now
    /// on button presses toggle group 51: 81 hold mode (release too), 82 toggle mode (0 flip, 1
    /// on, 2 off), 198 1 = player 2 only, 2 = player 1 only, 89 dual mode.
    Touch { group: i32, hold: bool, mode: i32, only: i32, dual: bool },
    /// Reverse (1917, `reverseDirection`): the players turn around (classic mode only).
    Reverse,
    /// Spawn particle (3608, `spawnParticleTrigger`): by level order (`World::particle_specs`).
    SpawnParticle { order: u32 },
    /// Checkpoint (2063, platformer only: `PlayLayer::checkpointActivated`): by level order.
    Checkpoint { order: u32 },
    /// Enter effect triggers (22–28, 55–59, 1915, 3017–3021) and enter stop (3023): see
    /// `enter.rs`.
    Enter { order: u32 },
    EnterStop { channel: i32, mode: i32, effect_id: i32 },
    /// Advanced follow (3016) and its edit / re-target triggers (3660 / 3661), by level order
    /// (`advfollow.rs`).
    AdvFollow { order: u32 },
    AdvFollowEdit { order: u32 },
    /// Keyframe animation trigger (3033), by level order (`keyframes.rs`).
    KeyframeAnim { order: u32 },
}

/// Edit song (3605) / edit SFX (3603) (`activateSongEditTrigger`, `applySFXEditTrigger`): on
/// song channel 432 (song) or the SFX group 457 / unique id (SFX): 417 stop, 418 change volume
/// to 406 over the duration 10, 419 change speed to 404 (`pitchForIdx`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioEdit {
    pub song: bool,
    pub channel: i32,
    pub stop: bool,
    pub volume: Option<f32>,
    pub speed: Option<i32>,
    pub duration: f32,
    /// With a target group (51): the channel's volume follows the listener's distance to that
    /// group (`addProximityVolumeEffect`, updated every step by `updateProximityVolumeEffects`).
    pub proximity: Option<Proximity>,
}

/// An SFX / song (edit) trigger's proximity settings (`volumeForProximityEffect` 0x112c60,
/// `getMinDistance` 0x112f88): the distance from the listener — the camera's center (428), player
/// 1 (138, and player 2 too with 200 in dual mode), player 2 (200) or group 71's main object — to
/// the nearest object of group 51, measured by mode 458 (0 distance, 1 |dx|, 2 obj − x,
/// 3 x − obj, 4 |dy|, 5 obj − y, 6 y − obj). Within near (424 × 3) the volume is 421; it goes
/// linearly to 422 at near + medium (425 × 3) and to 423 at near + medium + far (426 × 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proximity {
    pub group: i32,
    pub center: i32,
    pub p1: bool,
    pub p2: bool,
    pub camera: bool,
    pub volumes: [f32; 3],
    pub distances: [i32; 3],
    pub mode: i32,
}

impl Proximity {
    pub fn from_raw(raw: &crate::level::RawObject) -> Option<Proximity> {
        let group = raw.get_i32(51).unwrap_or(0);
        (group > 0).then(|| Proximity {
            group,
            center: raw.get_i32(71).unwrap_or(0),
            p1: raw.get(138) == Some("1"),
            p2: raw.get(200) == Some("1"),
            camera: raw.get(428) == Some("1"),
            volumes: [raw.get_f32(421).unwrap_or(0.0), raw.get_f32(422).unwrap_or(0.0), raw.get_f32(423).unwrap_or(0.0)],
            distances: [raw.get_i32(424).unwrap_or(0), raw.get_i32(425).unwrap_or(0), raw.get_i32(426).unwrap_or(0)],
            mode: raw.get_i32(458).unwrap_or(0),
            // (absent keys are 0, SFXTriggerGameObject::customObjectSetup 0x16c0f0)
        })
    }

    /// The volume for the listener at `at` given the group's object positions (none: 1). With
    /// `second` (player 2 also listens) its distance counts when nearer than player 1's beyond
    /// near.
    pub fn volume(&self, at: (f32, f32), second: Option<(f32, f32)>, objects: &[(f32, f32)]) -> f32 {
        if objects.is_empty() {
            return 1.0;
        }
        let near = (self.distances[0] * 3) as f32;
        let min_distance = |p: (f32, f32)| {
            let mut best = 2.1474836e9f32;
            for &o in objects {
                let d = match self.mode {
                    0 => ((o.0 - p.0) * (o.0 - p.0) + (o.1 - p.1) * (o.1 - p.1)).sqrt(),
                    1 => (o.0 - p.0).abs(),
                    2 => o.0 - p.0,
                    3 => p.0 - o.0,
                    4 => (o.1 - p.1).abs(),
                    5 => o.1 - p.1,
                    6 => p.1 - o.1,
                    _ => return best,
                };
                if d < best {
                    best = d;
                }
                if best <= near {
                    return best;
                }
            }
            best
        };
        let mut d = min_distance(at);
        if let Some(q) = second.filter(|_| near < d) {
            d = d.min(min_distance(q));
        }
        let [v0, v1, v2] = self.volumes;
        let (mut from, mut to) = (v0, v2);
        let mut k = 0.0f32;
        if near < d {
            let mid = near + (self.distances[1] * 3) as f32;
            let far = mid + (self.distances[2] * 3) as f32;
            if far <= d {
                k = 1.0;
            } else {
                let (mut lo, mut hi) = (near, far);
                if 0.0 < mid {
                    if mid <= d {
                        (from, to, lo) = (v1, v2, mid);
                    } else {
                        (from, to, hi) = (v0, v1, mid);
                    }
                }
                if lo != hi {
                    k = (d - lo) / (hi - lo);
                }
            }
        }
        from + k * (to - from)
    }
}

#[derive(Clone, Debug)]
pub struct Trigger {
    pub x: f32,
    pub y: f32,
    /// Level order (the game's activation order among triggers at the same x).
    pub order: u32,
    pub action: Action,
    /// Groups the trigger itself is in (spawn triggers activate spawn-triggered ones by group).
    pub groups: Vec<i32>,
    /// Key 62: only a spawn trigger can activate it.
    pub spawn_triggered: bool,
    /// The trigger's scale (keys 128/129, else 32): touch triggers are touched in their scaled
    /// 30×30 square.
    pub scale: (f32, f32),
    /// Key 11: activated by the player touching it rather than passing its x.
    pub touch_triggered: bool,
    /// Key 87 ("multi trigger"): a spawn-triggered trigger without it activates only once
    /// (real s6-22: move 18055 spawned again at step 2622 stayed idle).
    pub multi: bool,
    /// Spawn triggers: key 442's remap pairs (from, to).
    pub remap: Vec<(i32, i32)>,
}

impl Trigger {
    pub fn parse(raw: &RawObject, order: u32) -> Option<Trigger> {
        let group = raw.get_i32(51).unwrap_or(0);
        let action = match raw.id() {
            901 => Action::Move(MoveSpec {
                group,
                dx: raw.get_f32(28).unwrap_or(0.0),
                dy: raw.get_f32(29).unwrap_or(0.0),
                duration: raw.get_f32(10).unwrap_or(0.0),
                easing: raw.get_i32(30).unwrap_or(0),
                rate: raw.get_f32(85).unwrap_or(2.0),
                lock_x: raw.get(58) == Some("1"),
                lock_y: raw.get(59) == Some("1"),
                lock_mod: {
                    let m = |k| raw.get_f32(k).filter(|&v| v != 0.0).unwrap_or(1.0);
                    (m(143), m(144))
                },
                target: (raw.get(100) == Some("1")).then(|| {
                    let center = raw.get_i32(395).filter(|&c| c > 0).unwrap_or(group);
                    (raw.get_i32(71).unwrap_or(0), center, raw.get_i32(101).unwrap_or(0))
                }),
            }),
            1346 => Action::Rotate(RotateSpec {
                group,
                center: raw.get_i32(71).unwrap_or(0),
                degrees: raw.get_f32(68).unwrap_or(0.0) + raw.get_f32(69).unwrap_or(0.0) * 360.0,
                duration: raw.get_f32(10).unwrap_or(0.0),
                easing: raw.get_i32(30).unwrap_or(0),
                rate: raw.get_f32(85).unwrap_or(2.0),
                lock: raw.get(70) == Some("1"),
            }),
            2067 => {
                let get = |k: u32| raw.get_f32(k).filter(|v| *v != 0.0).unwrap_or(1.0);
                let (mut sx, mut sy) = (get(150), get(151));
                if raw.get(153) == Some("1") {
                    sx = 1.0 / sx;
                }
                if raw.get(154) == Some("1") {
                    sy = 1.0 / sy;
                }
                Action::Scale(ScaleSpec {
                    group,
                    center: raw.get_i32(71).unwrap_or(0),
                    sx,
                    sy,
                    duration: raw.get_f32(10).unwrap_or(0.0),
                    easing: raw.get_i32(30).unwrap_or(0),
                    rate: raw.get_f32(85).unwrap_or(2.0),
                    only_move: raw.get(133) == Some("1"),
                })
            }
            2066 => Action::Gravity { value: raw.get_f32(148).unwrap_or(1.0) },
            1817 => Action::Pickup { item: raw.get_i32(80).unwrap_or(0), count: raw.get_i32(77).unwrap_or(0), set: raw.get(139) == Some("1") },
            2068 => Action::Random { table: 0 },
            1912 => Action::Chance { a: group, b: raw.get_i32(71).unwrap_or(0), chance: raw.get_f32(10).unwrap_or(0.0) },
            3024 => Action::AreaStop { id: group },
            3618 => Action::Reset { group },
            3641 => Action::Persistent {
                item: raw.get_i32(80).unwrap_or(0),
                on: raw.get(491) == Some("1"),
                all: raw.get(492) == Some("1"),
                reset: raw.get(493) == Some("1"),
                timer: raw.get(494) == Some("1"),
            },
            3663 => Action::Progress {
                value: raw.get_i32(602).unwrap_or(0),
                add: raw.get(603) == Some("1"),
                item: (raw.get(605) == Some("1")).then(|| raw.get_i32(95).unwrap_or(0)),
                counter: raw.get_i32(80).unwrap_or(0),
            },
            1818 | 1819 => Action::BgEffect { on: raw.id() == 1818 },
            3606 | 3612 => Action::ArtSpeed { mg: raw.id() == 3612, x: raw.get_f32(143).unwrap_or(0.0), y: raw.get_f32(144).unwrap_or(0.0) },
            3603 | 3605 => Action::AudioEdit(AudioEdit {
                song: raw.id() == 3605,
                channel: if raw.id() == 3605 { raw.get_i32(432).unwrap_or(0) } else { raw.get_i32(457).unwrap_or(0) },
                stop: raw.get(417) == Some("1"),
                volume: (raw.get(418) == Some("1")).then(|| raw.get_f32(406).unwrap_or(1.0)),
                speed: (raw.get(419) == Some("1")).then(|| raw.get_i32(404).unwrap_or(0)),
                duration: raw.get_f32(10).unwrap_or(0.0),
                proximity: Proximity::from_raw(raw),
            }),
            1595 => Action::Touch {
                group,
                hold: raw.get(81) == Some("1"),
                mode: raw.get_i32(82).unwrap_or(0),
                only: raw.get_i32(198).unwrap_or(0),
                dual: raw.get(89) == Some("1"),
            },
            1917 => Action::Reverse,
            3608 => Action::SpawnParticle { order },
            2063 => Action::Checkpoint { order },
            id if crate::enter::is_enter_trigger(id) => Action::Enter { order },
            3023 => Action::EnterStop {
                channel: raw.get_i32(344).unwrap_or(0).clamp(0, 100),
                mode: raw.get_i32(217).unwrap_or(0),
                effect_id: raw.get_i32(225).unwrap_or(0),
            },
            3016 => Action::AdvFollow { order },
            3660 | 3661 => Action::AdvFollowEdit { order },
            3033 => Action::KeyframeAnim { order },
            1612 => Action::PlayerVisible { on: false },
            1613 => Action::PlayerVisible { on: true },
            1520 => Action::Shake { strength: raw.get_f32(75).unwrap_or(0.0), interval: raw.get_f32(84).unwrap_or(0.0), duration: raw.get_f32(10).unwrap_or(0.0) },
            1347 => Action::Follow {
                group,
                followed: raw.get_i32(71).unwrap_or(0),
                mod_x: raw.get_f32(72).unwrap_or(1.0),
                mod_y: raw.get_f32(73).unwrap_or(1.0),
                duration: raw.get_f32(10).unwrap_or(0.0),
            },
            1814 => Action::FollowY {
                group,
                speed: raw.get_f32(90).unwrap_or(1.0),
                delay: raw.get_f32(91).unwrap_or(0.0),
                offset: raw.get_i32(92).unwrap_or(0),
                max_speed: raw.get_f32(105).unwrap_or(0.0),
                duration: raw.get_f32(10).unwrap_or(0.0),
            },
            1815 | 3609 => {
                let mut a = raw.get_i32(80).unwrap_or(0);
                let mut b = raw.get_i32(95).unwrap_or(0);
                if raw.get(201) == Some("1") {
                    (a, b) = (-1, -2);
                } else if raw.get(138) == Some("1") {
                    b = -1;
                } else if raw.get(200) == Some("1") {
                    b = -2;
                }
                if raw.id() == 1815 {
                    Action::Collision { a, b, group, on: raw.get(56) == Some("1"), exit: raw.get(93) == Some("1") }
                } else {
                    Action::InstantCollision { a, b, yes: group, no: raw.get_i32(71).unwrap_or(0) }
                }
            }
            1932 => Action::PlayerControl { stop_jump: raw.get(540) == Some("1"), stop_move: raw.get(541) == Some("1"), stop_rotation: raw.get(542) == Some("1") },
            1934 => Action::Song {
                id: raw.get_i32(392).unwrap_or(0),
                start_ms: raw.get_i32(408).unwrap_or(0),
                volume: raw.get_f32(406).unwrap_or(1.0),
                speed: raw.get_i32(404).unwrap_or(0),
                looped: raw.get(413) == Some("1"),
                channel: raw.get_i32(432).unwrap_or(0),
            },
            3604 => Action::EventLink { order, target: group },
            3607 => Action::Sequence { order, min_interval: raw.get_f32(437).unwrap_or(0.0), mode: raw.get_i32(436).unwrap_or(0) },
            3614 => Action::Timer {
                id: raw.get_i32(80).unwrap_or(0),
                start: raw.get_f32(467).unwrap_or(0.0),
                stop: (raw.get(474) == Some("1")).then(|| raw.get_f32(473).unwrap_or(0.0)),
                rate: raw.get_f32(470).unwrap_or(1.0),
                target: group,
                paused: raw.get(471) == Some("1"),
                keep: raw.get(468) == Some("1"),
            },
            3615 => Action::TimeEvent { id: raw.get_i32(80).unwrap_or(0), time: raw.get_f32(473).unwrap_or(0.0), target: group, multi: raw.get(475) == Some("1") },
            3617 => Action::TimeControl { id: raw.get_i32(80).unwrap_or(0), stop: raw.get(472) == Some("1") },
            3602 => Action::Sfx { id: raw.get_i32(392).unwrap_or(0), volume: raw.get_f32(406).unwrap_or(1.0) },
            1585 => Action::Animate { group, anim: raw.get_i32(76).unwrap_or(0) },
            1812 => Action::OnDeath { group, on: raw.get(56) == Some("1") },
            3022 => Action::Teleport { group, keep_x: raw.get(352) == Some("1"), keep_y: raw.get(353) == Some("1"), gravity: raw.get_i32(354).unwrap_or(0) as i8 },
            3620 => Action::ItemCompare(ItemSpec::from_raw(raw)),
            3619 => Action::ItemEdit(ItemSpec::from_raw(raw)),
            1049 => Action::Toggle { group, on: raw.get(56) == Some("1") },
            1611 => Action::Count {
                item: raw.get_i32(80).unwrap_or(0),
                target: raw.get_i32(77).unwrap_or(0),
                group,
                on: raw.get(56) == Some("1"),
                multi: raw.get(104) == Some("1"),
            },
            1811 => Action::InstantCount {
                item: raw.get_i32(80).unwrap_or(0),
                target: raw.get_i32(77).unwrap_or(0),
                mode: raw.get_i32(88).unwrap_or(0),
                group,
                on: raw.get(56) == Some("1"),
            },
            1616 => Action::Stop { group },
            1935 => Action::TimeWarp { value: raw.get_f32(120).unwrap_or(1.0) },
            1268 => Action::Spawn { group, delay: raw.get_f32(63).unwrap_or(0.0), ordered: raw.get(441) == Some("1"), remap: 0 },
            3006..=3010 => Action::Area(crate::area::AreaSpec::from_raw(raw, order)),
            // Only spawn-activated end triggers here; position ones are the order walk's.
            3600 if raw.get(62) == Some("1") => Action::End,
            // Color, pulse and alpha triggers fired by a spawn, a touch, or the order walk of a
            // channel other than 0 (channel 0's run by x in `Colors::step`, which is the same).
            29 | 30 | 104 | 105 | 221 | 717 | 718 | 743 | 744 | 899 | 900 | 915 | 1006 | 1007
                if raw.get(62) == Some("1") || raw.get(11) == Some("1") || raw.get_i32(170).unwrap_or(0) != 0 =>
            {
                Action::Effect { order }
            }
            // Shader (2904–2924) and gradient (2903) triggers, by position, spawn or touch:
            // `crate::shaders::Shaders::fire` / `crate::gradients::Gradients::fire`.
            id if id == crate::gradients::GRADIENT || crate::shaders::is_shader_trigger(id) => Action::Effect { order },
            _ => return None,
        };
        Some(Trigger {
            x: raw.x(),
            y: raw.y(),
            order,
            action,
            groups: raw.groups(),
            spawn_triggered: raw.get(62) == Some("1"),
            touch_triggered: raw.get(11) == Some("1"),
            multi: raw.get(87) == Some("1"),
            remap: {
                let v: Vec<i32> = raw.get(442).map(|r| r.split('.').filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
                v.chunks_exact(2).map(|c| (c[0], c[1])).collect()
            },
            scale: {
                let s = raw.get_f32(32).unwrap_or(1.0);
                (raw.get_f32(128).unwrap_or(s).abs(), raw.get_f32(129).unwrap_or(s).abs())
            },
        })
    }

    /// Touch-triggered triggers are touched within the object's 30×30 square.
    pub fn touch_rect(&self) -> Rect {
        let (w, h) = (30.0 * self.scale.0, 30.0 * self.scale.1);
        Rect { x: self.x - w * 0.5, y: self.y + crate::world::GROUND_OFFSET - h * 0.5, w, h }
    }
}

/// One running move or rotation (`GroupCommandObject2`). A rotation eases its degrees like a
/// move eases one axis (`runRotateCommand`: mode 3, the same `updateAction`).
#[derive(Clone, Debug)]
pub struct MoveCommand {
    pub spec: MoveSpec,
    /// Rotations: (center group, lock the objects' own rotation).
    pub rotate: Option<(i32, bool)>,
    /// Scales: (center group, only move); the spec's dx/dy are the scale − 1.
    pub scale: Option<(i32, bool)>,
    /// Groups of the trigger that started it (stop triggers cancel by these).
    pub origin: Vec<i32>,
    /// Follow commands: (followed group, x mod, y mod).
    pub follow: Option<(i32, f32, f32)>,
    /// Follow Player Y commands: (speed, delay, offset, max speed).
    pub follow_y: Option<(f32, f32, i32, f32)>,
    /// +0x1ac: f32 elapsed, not advanced on the first tick.
    elapsed: f32,
    first_tick: bool,
    /// +0x20: f64 elapsed, advanced every tick (ends moves with nothing to ease).
    elapsed_total: f64,
    /// +0x30/+0x38: the offset reached so far on each axis.
    prev: [f64; 2],
    pub done: bool,
}

impl MoveCommand {
    pub fn new(spec: MoveSpec) -> MoveCommand {
        MoveCommand { spec, rotate: None, scale: None, origin: Vec::new(), follow: None, follow_y: None, elapsed: 0.0, first_tick: true, elapsed_total: 0.0, prev: [0.0; 2], done: false }
    }

    pub fn rotation(r: RotateSpec) -> MoveCommand {
        let spec = MoveSpec {
            group: r.group,
            dx: r.degrees,
            dy: 0.0,
            duration: r.duration,
            easing: r.easing,
            rate: r.rate,
            lock_x: false,
            lock_y: false,
            lock_mod: (1.0, 1.0),
            target: None,
        };
        MoveCommand { rotate: Some((r.center, r.lock)), ..MoveCommand::new(spec) }
    }

    pub fn scaling(s: ScaleSpec) -> MoveCommand {
        let spec = MoveSpec {
            group: s.group,
            dx: s.sx - 1.0,
            dy: s.sy - 1.0,
            duration: s.duration,
            easing: s.easing,
            rate: s.rate,
            lock_x: false,
            lock_y: false,
            lock_mod: (1.0, 1.0),
            target: None,
        };
        MoveCommand { scale: Some((s.center, s.only_move)), ..MoveCommand::new(spec) }
    }

    /// The offset reached so far on an axis (scales: the scale − 1).
    pub fn reached(&self, axis: usize) -> f64 {
        self.prev[axis]
    }

    fn duration(&self) -> f64 {
        self.spec.duration as f64
    }

    /// `updateAction`: the offset on one axis, returns its change since the last tick.
    fn update_axis(&mut self, axis: usize, amount: f32) -> f64 {
        let t = self.elapsed as f64 / self.duration().max(f32::EPSILON as f64);
        let t = if t >= 1.0 {
            1.0
        } else if t > 0.0 {
            t as f32
        } else {
            0.0
        };
        let e = easing::eased(t, self.spec.rate, self.spec.easing);
        let value = (-amount).mul_add(1.0 - e, amount) as f64;
        let delta = value - self.prev[axis];
        self.prev[axis] = value;
        delta
    }

    /// `GroupCommandObject2::step`: one physics step; returns the (x, y) movement.
    pub fn step(&mut self, dt: f32) -> (f64, f64) {
        if self.done {
            return (0.0, 0.0);
        }
        self.elapsed_total += dt as f64;
        if self.first_tick {
            self.first_tick = false;
        } else {
            self.elapsed += dt;
        }
        let (has_x, has_y) = (self.spec.dx != 0.0, self.spec.dy != 0.0);
        if !has_x && !has_y {
            // Nothing to ease (a pure lock to the player): runs for its duration.
            let d = self.duration();
            if d <= self.elapsed_total && d != -1.0 {
                self.done = true;
            }
            return (0.0, 0.0);
        }
        let dx = if has_x { self.update_axis(0, self.spec.dx) } else { 0.0 };
        let dy = if has_y { self.update_axis(1, self.spec.dy) } else { 0.0 };
        if self.elapsed > 0.0 && self.elapsed as f64 >= self.duration() {
            self.done = true;
        }
        (dx, dy)
    }
}

/// Everything the group triggers do while a level runs. Cloned with the world (restarts and
/// route search).
#[derive(Clone, Debug, Default)]
pub struct GroupTriggers {
    /// Triggers activated by the player's x, sorted by (x, order).
    by_x: Vec<Trigger>,
    next: usize,
    /// Which of `by_x` have fired (activation can also run along y or backwards, out of order).
    fired: Vec<bool>,
    /// Level order → index in `by_x`.
    by_order: HashMap<u32, usize>,
    /// Spawn-triggered triggers by each group they're in.
    by_group: HashMap<i32, Vec<Trigger>>,
    /// Touch-triggered triggers, with whether they fired.
    touch: Vec<(Trigger, bool)>,
    pub commands: Vec<MoveCommand>,
    /// Pending delayed spawns: (seconds left, group, the spawn trigger's groups, remap).
    delayed: Vec<(f64, i32, Vec<i32>, u16)>,
    /// Spawn remaps (index 0 is the empty one).
    remaps: Vec<Vec<(i32, i32)>>,
    /// Advanced random triggers' (group, weight) lists, and the random state.
    randoms: Vec<Vec<(i32, i32)>>,
    rng: crate::fastrand::FastRand,
    /// Triggers an ordered spawn queued: (seconds left, action, the trigger's groups).
    delayed_actions: Vec<(f64, Action, Vec<i32>)>,
    /// Item counters (`GJEffectManager::countForItem`) and armed count triggers.
    pub items: HashMap<i32, i32>,
    pub counts: Vec<(Action, Vec<i32>)>,
    /// Spawn-triggered triggers without "multi trigger" that already activated (by order).
    spawned_once: Vec<u32>,
}

impl GroupTriggers {
    /// Seed the random triggers' `fast_rand` (`fast_srand`).
    pub fn reseed(&mut self, seed: u64) {
        self.rng = crate::fastrand::FastRand::new(seed);
    }

    pub fn new(level: &LevelString) -> GroupTriggers {
        let mut out = GroupTriggers { remaps: vec![Vec::new()], rng: crate::fastrand::FastRand::new(0x9e37_79b9), ..GroupTriggers::default() };
        for (order, raw) in level.objects.iter().enumerate() {
            let t = match Trigger::parse(raw, order as u32) {
                Some(mut t) => {
                    if let Action::Random { table } = &mut t.action {
                        let v: Vec<i32> = raw.get(152).unwrap_or("").split('.').filter_map(|x| x.parse().ok()).collect();
                        out.randoms.push(v.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect());
                        *table = (out.randoms.len() - 1) as u16;
                    }
                    if let Action::Spawn { remap, .. } = &mut t.action {
                        if !t.remap.is_empty() {
                            out.remaps.push(t.remap.clone());
                            *remap = (out.remaps.len() - 1) as u16;
                        }
                    }
                    t
                }
                // Unhandled triggers still take their place in ordered spawns.
                None if raw.get(62) == Some("1") && crate::objects::info(raw.id()).is_some_and(|i| i.kind == crate::objects::ObjectType::MODIFIER) => Trigger {
                    x: raw.x(),
                    y: raw.y(),
                    order: order as u32,
                    action: Action::Noop,
                    groups: raw.groups(),
                    spawn_triggered: true,
                    touch_triggered: false,
                    multi: raw.get(87) == Some("1"),
                    scale: (1.0, 1.0),
                    remap: Vec::new(),
                },
                None => continue,
            };
            if t.spawn_triggered {
                for &g in &t.groups {
                    out.by_group.entry(g).or_default().push(t.clone());
                }
            }
            if t.touch_triggered {
                out.touch.push((t, false));
            } else if !t.spawn_triggered {
                out.by_x.push(t);
            }
        }
        out.by_x.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.order.cmp(&b.order)));
        out.fired = vec![false; out.by_x.len()];
        out.by_order = out.by_x.iter().enumerate().map(|(i, t)| (t.order, i)).collect();
        out
    }

    /// Every trigger action (for finding which groups move or toggle), including the remapped
    /// copies remapping spawns start.
    pub fn actions(&self) -> Vec<Action> {
        let all: Vec<&Trigger> = self.by_x.iter().chain(self.by_group.values().flatten()).chain(self.touch.iter().map(|(t, _)| t)).collect();
        let mut out: Vec<Action> = all.iter().map(|t| t.action).collect();
        for t in &all {
            if let Action::Spawn { group, remap, .. } = t.action {
                if remap != 0 {
                    for s in self.by_group.get(&group).into_iter().flatten() {
                        out.push(remap_action(s.action, &self.remaps[remap as usize]));
                    }
                }
            }
        }
        out
    }

    /// Which unused touch-triggered triggers `rect` touches, as a bit set (by position in the
    /// list, first 64). Two players with different sets would change the world differently.
    pub fn touch_key(&self, rect: &Rect) -> u64 {
        let mut key = 0;
        for (i, (t, used)) in self.touch.iter().enumerate().take(64) {
            if !*used && t.touch_rect().intersects(rect) {
                key |= 1 << i;
            }
        }
        key
    }

    /// Triggers that fire this step, after the player moved: passed by x, touched, or a
    /// delayed spawn coming due. Spawns are expanded into the triggers they start.
    /// A trigger whose own object is toggled off (`enabled`, by level order) doesn't fire: an
    /// x trigger is passed over, a touch trigger stays armed, a spawned one is skipped.
    /// `forward`: the player's x and whether position triggers are paused (sideways).
    /// `fired`: the position triggers the player's order-channel walk fired this step (level
    /// orders, in order; see `order.rs`).
    pub fn activate(
        &mut self,
        fired_orders: &[u32],
        player_rect: &Rect,
        _dt: f64,
        enabled: &[bool],
    ) -> Vec<(Action, Vec<i32>)> {
        let on = |t: &Trigger| enabled.get(t.order as usize).copied().unwrap_or(true);
        // (action, groups of the trigger that fired it)
        let mut fired: Vec<(Action, Vec<i32>)> = Vec::new();
        for &order in fired_orders {
            if let Some(&i) = self.by_order.get(&order) {
                if !self.fired[i] {
                    self.fired[i] = true;
                    let t = &self.by_x[i];
                    fired.push((t.action, t.groups.clone()));
                }
            }
        }
        for (t, used) in &mut self.touch {
            if !*used && on(t) && t.touch_rect().intersects(player_rect) {
                *used = true;
                fired.push((t.action, t.groups.clone()));
            }
        }
        self.expand(fired, &on)
    }

    /// `updateSpawnTriggers`, at the start of a step: delayed spawns and ordered-spawn
    /// triggers whose wait ran out fire now.
    pub fn due(&mut self, dt: f64, enabled: &[bool]) -> Vec<(Action, Vec<i32>)> {
        let on = |t: &Trigger| enabled.get(t.order as usize).copied().unwrap_or(true);
        let mut fired: Vec<(Action, Vec<i32>, f64)> = Vec::new();
        for d in &mut self.delayed {
            d.0 -= dt;
        }
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.delayed).into_iter().partition(|d| d.0 <= 0.0);
        self.delayed = waiting;
        // A wait that ran out mid-step passes its overshoot on: what it spawns waits that much
        // less (Tower 5001's bobbing chain: 139, 130, 138, 130 … steps, not a fixed 139/130).
        for (left, group, origin, remap) in due {
            fired.push((Action::Spawn { group, delay: 0.0, ordered: false, remap }, origin, left));
        }
        for d in &mut self.delayed_actions {
            d.0 -= dt;
        }
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.delayed_actions).into_iter().partition(|d| d.0 <= 0.0);
        self.delayed_actions = waiting;
        fired.extend(due.into_iter().map(|(left, a, g)| (a, g, left)));
        if fired.is_empty() {
            return Vec::new();
        }
        self.expand_timed(fired, &on)
    }

    /// `expand` for actions fired from outside the trigger list (custom orbs).
    pub fn expand_public(&mut self, fired: Vec<(Action, Vec<i32>)>, enabled: &[bool]) -> Vec<(Action, Vec<i32>)> {
        let on = |t: &Trigger| enabled.get(t.order as usize).copied().unwrap_or(true);
        self.expand(fired, &on)
    }

    /// Spawns expanded into the triggers they start (delayed ones queued), stops applied.
    fn expand(&mut self, fired: Vec<(Action, Vec<i32>)>, on: &dyn Fn(&Trigger) -> bool) -> Vec<(Action, Vec<i32>)> {
        self.expand_timed(fired.into_iter().map(|(a, o)| (a, o, 0.0)).collect(), on)
    }

    /// `expand` with each fired action's time offset (≤ 0: how far past its due time it fired),
    /// taken off the waits of what it spawns.
    fn expand_timed(&mut self, mut fired: Vec<(Action, Vec<i32>, f64)>, on: &dyn Fn(&Trigger) -> bool) -> Vec<(Action, Vec<i32>)> {
        let mut out = Vec::new();
        let mut depth = 0;
        // A trigger fires at most once in a chain of instant spawns; reached again (a spawn loop
        // with no delay) it waits for the next step, once, so such loops run once a step
        // instead of doubling through every level of the chain.
        let mut chain: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut deferred: std::collections::HashSet<u32> = std::collections::HashSet::new();
        while !fired.is_empty() && depth < 16 {
            let mut next = Vec::new();
            for (a, origin, offset) in fired {
                match a {
                    Action::Spawn { group, delay, remap, .. } if delay > 0.0 => self.delayed.push((delay as f64 + offset, group, origin, remap)),
                    Action::Spawn { group, ordered: true, remap, .. } => {
                        let map = &self.remaps[remap as usize];
                        // spawnObjectsInOrder: by x; later ones wait (x − first x) / 311.58 s.
                        let mut ts: Vec<&Trigger> = self
                            .by_group
                            .get(&group)
                            .map(|v| v.iter().filter(|t| on(t) && (t.multi || !self.spawned_once.contains(&t.order))).collect())
                            .unwrap_or_default();
                        ts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.order.cmp(&b.order)));
                        let x0 = ts.first().map_or(0.0, |t| t.x);
                        let mut queued = Vec::new();
                        for t in ts {
                            if !t.multi {
                                self.spawned_once.push(t.order);
                            }
                            let wait = ((t.x - x0) as f64 / 311.5801086425781) as f32;
                            let action = inherit_remap(remap_action(t.action, map), remap);
                            if wait <= 0.0 {
                                next.push((action, t.groups.clone(), offset));
                            } else {
                                queued.push((wait as f64 + offset, action, t.groups.clone()));
                            }
                        }
                        self.delayed_actions.extend(queued);
                    }
                    Action::Spawn { group, remap, .. } => {
                        let map = &self.remaps[remap as usize];
                        if let Some(ts) = self.by_group.get(&group) {
                            for t in ts.iter().filter(|t| on(t)) {
                                if !t.multi {
                                    if self.spawned_once.contains(&t.order) {
                                        continue;
                                    }
                                    self.spawned_once.push(t.order);
                                }
                                let action = inherit_remap(remap_action(t.action, map), remap);
                                if !chain.insert(t.order) {
                                    if deferred.insert(t.order) {
                                        self.delayed_actions.push((offset + 1e-9, action, t.groups.clone()));
                                    }
                                    continue;
                                }
                                next.push((action, t.groups.clone(), offset));
                            }
                        }
                    }
                    Action::Random { table } => {
                        // RandTriggerGameObject::getRandomGroupID (0x161e14): k = the
                        // weights' sum · fast_rand_0_1, rounded half away from zero; the first
                        // entry whose running sum reaches k (none when k is past them all)
                        let list = self.randoms.get(table as usize).cloned().unwrap_or_default();
                        if !list.is_empty() {
                            let total = list.iter().map(|c| c.1).sum::<i32>() as f32;
                            let k = (self.rng.unit() * total).round() as i32;
                            let mut sum = 0;
                            for (group, weight) in list {
                                sum += weight;
                                if sum >= k {
                                    if group > 0 {
                                        next.push((Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, origin.clone(), offset));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    Action::Chance { a, b, chance } => {
                        // fast_rand_0_1() · 100 ≤ chance (the game's generator, own seed)
                        let r = self.rng.unit();
                        let group = if r * 100.0 <= chance { a } else { b };
                        if group > 0 {
                            next.push((Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, origin.clone(), offset));
                        }
                    }
                    Action::Stop { group } => {
                        self.commands.retain(|c| !c.origin.contains(&group));
                        self.delayed.retain(|d| !d.2.contains(&group));
                        self.delayed_actions.retain(|d| !d.2.contains(&group));
                    }
                    other => out.push((other, origin)),
                }
            }
            fired = next;
            depth += 1;
        }
        out
    }

    /// How many position triggers not yet passed a player at `x` would pass now (the route
    /// planner keeps routes that would fire different triggers in different worlds).
    pub fn passed_count(&self, x: f32) -> usize {
        self.by_x.get(self.next..).map_or(0, |rest| rest.partition_point(|t| t.x <= x))
    }

    /// Position triggers the player's x has passed (each once).
    fn passed(&mut self, x: f32, on: &dyn Fn(&Trigger) -> bool) -> Vec<(Action, Vec<i32>)> {
        let mut fired = Vec::new();
        while let Some(t) = self.by_x.get(self.next).filter(|t| t.x <= x) {
            if !self.fired[self.next] {
                self.fired[self.next] = true;
                if on(t) {
                    fired.push((t.action, t.groups.clone()));
                }
            }
            self.next += 1;
        }
        fired
    }

    /// The catch-up after a rotation back from sideways: the position triggers passed by x,
    /// with spawns expanded.
    pub fn catch_up(&mut self, x: f32, enabled: &[bool]) -> Vec<(Action, Vec<i32>)> {
        let on = |t: &Trigger| enabled.get(t.order as usize).copied().unwrap_or(true);
        let fired = self.passed(x, &on);
        self.expand(fired, &on)
    }

    /// Item `item` changed by `delta`: the armed count triggers that now match fire.
    pub fn add_to_item(&mut self, item: i32, delta: i32) -> Vec<(Action, Vec<i32>)> {
        let count = self.items.entry(item).or_insert(0);
        *count += delta;
        let count = *count;
        let mut out = Vec::new();
        self.counts.retain(|(a, origin)| match *a {
            Action::Count { item: i, target, multi, .. } if i == item && target == count => {
                out.push((*a, origin.clone()));
                multi
            }
            _ => true,
        });
        out
    }
}

/// A spawn trigger started by a remapping spawn passes the remap on (its own key 442, if
/// any, wins): Tower 5001's bobbing chain (groups 45 → 44 remapped to 46) re-spawns itself
/// with the remap still applied (real traces: group 46 keeps moving every 1.117 s).
fn inherit_remap(a: Action, parent: u16) -> Action {
    match a {
        Action::Spawn { group, delay, ordered, remap: 0 } if parent != 0 => Action::Spawn { group, delay, ordered, remap: parent },
        other => other,
    }
}

/// A spawned trigger's action with its target groups put through a spawn remap.
fn remap_action(a: Action, map: &[(i32, i32)]) -> Action {
    if map.is_empty() {
        return a;
    }
    let r = |g: i32| map.iter().find(|m| m.0 == g).map_or(g, |m| m.1);
    match a {
        Action::Move(mut m) => {
            m.group = r(m.group);
            m.target = m.target.map(|(t, c, axis)| (r(t), r(c), axis));
            Action::Move(m)
        }
        Action::Rotate(mut s) => {
            s.group = r(s.group);
            s.center = r(s.center);
            Action::Rotate(s)
        }
        Action::Scale(mut s) => {
            s.group = r(s.group);
            s.center = r(s.center);
            Action::Scale(s)
        }
        Action::Follow { group, followed, mod_x, mod_y, duration } => Action::Follow { group: r(group), followed: r(followed), mod_x, mod_y, duration },
        Action::OnDeath { group, on } => Action::OnDeath { group: r(group), on },
        Action::Teleport { group, keep_x, keep_y, gravity } => Action::Teleport { group: r(group), keep_x, keep_y, gravity },
        Action::ItemCompare(mut spec) => {
            spec.target = r(spec.target);
            spec.other = r(spec.other);
            Action::ItemCompare(spec)
        }
        Action::Collision { a, b, group, on, exit } => Action::Collision { a, b, group: r(group), on, exit },
        Action::InstantCollision { a, b, yes, no } => Action::InstantCollision { a, b, yes: r(yes), no: r(no) },
        Action::FollowY { group, speed, delay, offset, max_speed, duration } => Action::FollowY { group: r(group), speed, delay, offset, max_speed, duration },
        Action::Toggle { group, on } => Action::Toggle { group: r(group), on },
        Action::Spawn { group, delay, ordered, remap } => Action::Spawn { group: r(group), delay, ordered, remap },
        Action::Count { item, target, group, on, multi } => Action::Count { item, target, group: r(group), on, multi },
        Action::InstantCount { item, target, mode, group, on } => Action::InstantCount { item, target, mode, group: r(group), on },
        Action::Stop { group } => Action::Stop { group: r(group) },
        Action::Chance { a, b, chance } => Action::Chance { a: r(a), b: r(b), chance },
        Action::Reset { group } => Action::Reset { group: r(group) },
        Action::Touch { group, hold, mode, only, dual } => Action::Touch { group: r(group), hold, mode, only, dual },
        Action::Area(mut a) => {
            a.target = r(a.target);
            a.center = r(a.center);
            Action::Area(a)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// volumeForProximityEffect's curve: near (424 x 3) keeps 421, then linear to 422 at
    /// near + medium and to 423 at near + medium + far; no objects: full volume.
    #[test]
    fn proximity_volume_curve() {
        let raw = crate::level::LevelString::parse("kA13,0;1,3603,2,0,3,0,51,5,138,1,421,1,422,0.5,423,0,424,10,425,10,426,20");
        let p = Proximity::from_raw(&raw.objects[0]).unwrap();
        let at = |x: f32| p.volume((0.0, 0.0), None, &[(x, 0.0)]);
        assert_eq!(at(20.0), 1.0);
        assert_eq!(at(30.0), 1.0);
        assert!((at(45.0) - 0.75).abs() < 1e-6);
        assert!((at(60.0) - 0.5).abs() < 1e-6);
        assert!((at(90.0) - 0.25).abs() < 1e-6);
        assert_eq!(at(200.0), 0.0);
        assert_eq!(p.volume((0.0, 0.0), None, &[]), 1.0);
        // mode 2 (obj - x): objects behind the listener count as near
        let mut q = p;
        q.mode = 2;
        assert_eq!(q.volume((100.0, 0.0), None, &[(0.0, 0.0)]), 1.0);
    }
}

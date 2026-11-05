//! A playable level: objects placed in game coordinates with their collision rectangles.

use crate::level::LevelString;
use crate::objects::{self, ObjectType};

/// Level-string y is relative to the ground; the ground sits at y = 90 in game coordinates.
pub const GROUND_OFFSET: f32 = 90.0;

/// Axis-aligned rectangle (origin + size), like cocos2d's `CCRect`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn max_x(&self) -> f32 {
        self.x + self.w
    }

    pub fn max_y(&self) -> f32 {
        self.y + self.h
    }

    /// cocos2d `CCRect::intersectsRect`: touching edges count as intersecting.
    pub fn intersects(&self, o: &Rect) -> bool {
        !(self.max_x() < o.x || o.max_x() < self.x || self.max_y() < o.y || o.max_y() < self.y)
    }
}

#[derive(Clone, Debug)]
pub struct Object {
    pub id: i32,
    pub kind: ObjectType,
    pub x: f32,
    pub y: f32,
    pub rect: Rect,
    /// Collision section (100-unit columns) and creation order (the game's unique id order).
    /// Solids are collided in descending (section, order): the game collects them section by
    /// section in ascending order, then walks the list backwards.
    pub section: i32,
    pub order: u32,
    /// Collision circle radius (sawblades and other round hazards; 0 = use the rect).
    pub radius: f32,
    /// The object's flips and rotation (degrees), for direction-dependent objects, and its
    /// scale (rotate triggers rebuild the hitbox from these).
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: f32,
    pub scale: (f32, f32),
    /// For rotations that aren't quarter turns: the hitbox's four corners (the game's
    /// `OBB2D`); `rect` is then their bounding box.
    pub obb: Option<[(f32, f32); 4]>,
    /// Slopes: `m_slopeDirection` (0–7), `m_slopeUphill`, `m_slopeIsHazard`.
    pub slope_dir: u8,
    pub slope_uphill: bool,
    pub slope_hazard: bool,
    /// The game's f64 position (`m_positionX/Y`); moved objects accumulate their moves here.
    pub pos: (f64, f64),
    /// How `rect` (and `obb`) follow from the position.
    pub shape: Shape,
    /// `GameObject::getLastPosition`: the position (f32) before the last step it moved in.
    pub last_pos: (f32, f32),
    /// The step `last_pos` was taken in.
    moved_step: u64,
    /// In at least one group (`m_groupCount` > 0): the player checks whether it moved away.
    pub grouped: bool,
    /// Teleport portals: how far above this portal its orange end sits (key 54).
    pub teleport_dy: f32,
    /// Teleport portals: key 354 gravity (1 normal, 2 flipped, 3 toggle) and keys 345/346/347
    /// the static force (on, strength, additive).
    pub teleport_gravity: i8,
    pub teleport_force: Option<(f32, bool)>,
    /// Teleports other than the 747 pair (2902 …): target group (key 51) and keep x / y (352,
    /// 353).
    pub teleport_target: Option<i32>,
    pub teleport_keep: (bool, bool),
    /// Teleports: key 55 smooth camera (+0x748): `teleportPlayer` starts the camera's 0.5 s
    /// teleport ease (y eased by 30).
    pub teleport_smooth: bool,
    /// Mode portals with key 112: the camera padding (114, clamped to [0, 1]) and easing (113,
    /// [1, 40]) they set (`playerWillSwitchMode`).
    pub portal_camera: Option<(f32, f32)>,
    /// "Passable" (key 134, `m_isPassable` +0x50d): `collidedWithObjectInternal` lets the
    /// player land on its feet side but never kills, pushes the head or meets its far side.
    pub passable: bool,
    /// Key 136 non-stick x (+0x50f: no platformer carry), 289 non-stick y (+0x510: its
    /// movement never counts as moving away from the player), 495 extra sticky (+0x514: the
    /// moving-platform snap reaches 20 per frame).
    pub non_stick: (bool, bool),
    pub extra_sticky: bool,
    /// Key 193 (`m_isGripSlope` +0x512): a slope that never forces a platformer slide.
    pub grip_slope: bool,
    /// Custom orbs (type 36): (group 51, key 56 "on", key 504 spawn instead of toggle).
    pub custom_ring: Option<(i32, bool, bool)>,
}

impl Object {
    /// `GameObject::isFacingDown`: flipped vertically XOR turned upside down (180°, or within
    /// 91..269° either way for angles that aren't multiples of 90).
    pub fn is_facing_down(&self) -> bool {
        let rot = self.rotation as i32;
        let turned = if rot % 90 != 0 {
            (91..=269).contains(&rot) || (-269..=-91).contains(&rot)
        } else {
            rot.abs() == 180
        };
        self.flip_y ^ turned
    }
}

/// A rotation / scale "center" meaning each object about itself (keyframe animations).
const IN_PLACE: i32 = i32::MIN;

/// The game's `m_sectionXFactor` (f32 1/100). Section = (int)(x * factor), computed in f64.
pub const SECTION_X_FACTOR: f32 = 0.01;

pub fn section_of(x: f32) -> i32 {
    if x <= 0.0 { 0 } else { (x as f64 * SECTION_X_FACTOR as f64) as i32 }
}

#[derive(Clone, Default)]
pub struct World {
    /// Collidable objects: the ones that never move sorted by `rect.x`, then (from
    /// `first_dynamic`) the ones in groups that move triggers move.
    pub objects: Vec<Object>,
    first_dynamic: usize,
    /// The never-moving colliders by class (`collider_class`), as indices into `objects` in its
    /// x order: each collision pass walks only its own class.
    by_class: [Vec<u32>; 3],
    /// Per level object (by order): shown and collidable (toggle triggers switch groups off).
    pub enabled: Vec<bool>,
    /// With `track_moves` (the drawing app's culling), every object moved since the log was last
    /// emptied (level order, repeats possible); `epoch` changes when the world is replaced, so
    /// a reader knows to start over.
    pub track_moves: bool,
    pub moved_log: Vec<u32>,
    pub epoch: u64,
    /// Per level object (by order): in a group that move, follow, rotate, scale or area
    /// triggers move, so its position can change (all the others stay where the level put them).
    pub can_move: Vec<bool>,
    /// Per level object (by order): how far move triggers have moved it (for drawing).
    pub moved: Vec<(f64, f64)>,
    /// Per level object: degrees rotate triggers turned it, and the factor scale triggers
    /// scaled it by (for drawing).
    pub turned: Vec<f32>,
    pub scaled: Vec<(f32, f32)>,
    /// Per level object: its rotation in the level (key 6).
    base_rotation: std::sync::Arc<Vec<f32>>,
    /// Per level object: its start position and its current f64 position (objects that
    /// triggers move, collidable or not — rotation centers are often decorations).
    start: Vec<(f32, f32)>,
    level_pos: Vec<(f64, f64)>,
    /// Per level object: `getLastPosition` and the step it was taken in.
    level_last: Vec<(f32, f32)>,
    level_stamp: Vec<u64>,
    /// Collectible items: (level order, item id, +1/-1, rect).
    collectibles: Vec<(u32, i32, i32, Rect)>,
    /// Level orders of each group that a trigger moves or toggles.
    members: std::collections::HashMap<i32, Vec<u32>>,
    /// Level order → index in `objects` (`u32::MAX` for objects that aren't collided).
    collider: Vec<u32>,
    pub triggers: crate::groups::GroupTriggers,
    /// The player's position at the previous step's start (lock-to-player moves follow it).
    last_player: Option<(f32, f32)>,
    /// Player 1's y at the start of each recent step, newest last (`getOldPosition`'s ring of
    /// 200 samples 0.01 s apart), with the time each was taken.
    player_history: std::collections::VecDeque<(f64, f32)>,
    /// A player control trigger fired after the last step (stop jump, move, rotation), and the
    /// one the coming player step applies.
    control: Option<(bool, bool, bool)>,
    pub control_now: Option<(bool, bool, bool)>,
    /// Armed On Death triggers: (group, on).
    death_triggers: Vec<(i32, bool)>,
    /// A teleport trigger fired after the last step (target x, y, keep x, keep y, gravity),
    /// and the one the coming player step applies.
    teleport: Option<(f64, f64, bool, bool, i8, bool)>,
    pub teleport_now: Option<(f64, f64, bool, bool, i8, bool)>,
    /// Player 1's rect after the last step (its collision block).
    player_rect: Option<Rect>,
    /// Collision blocks (1816): (level order, block id 80, dynamic 94, half size).
    collision_blocks: Vec<(usize, i32, bool, (f32, f32))>,
    /// Armed collision triggers: (a, b sorted low-high, group, on, exit).
    collision_triggers: Vec<(i32, i32, i32, bool, bool)>,
    /// The block-id pairs touching after the last step (low, high).
    colliding: std::collections::HashSet<(i32, i32)>,
    /// Time warp (GJBaseGameLayer+0x330): it splits each 1/240 s frame into steps of game time
    /// (see `start_frame`); the current step's game seconds (fVar52) and 60 fps units (fVar42).
    pub time_warp: f32,
    /// Gravity triggers' factor for the player's gravity (1: none).
    pub gravity_mod: f32,
    /// Hide-player triggers (1612/1613): the player isn't drawn.
    pub player_hidden: bool,
    /// Shake triggers (1520): (strength, interval, seconds left).
    pub shake: (f32, f32, f32),
    warp_acc: f64,
    frame_left: u32,
    /// Steps in the current frame, and its game seconds (`local`, what the frame's effects step by).
    frame_steps: u32,
    frame_secs: f64,
    /// Game updates started (`GJBaseGameLayer::update` calls) and how many make one rendered
    /// frame (the harness spec's `speed`).
    updates: u64,
    pub sched_every: u32,
    /// The first update's index within its rendered frame (harness `# sched` header).
    pub sched_phase: u32,
    pub step_secs: f64,
    pub step_dt60: f32,
    /// Physics steps run (stamps objects' `last_pos`).
    step_count: u64,
    /// Groups whose move finished last step: the game moves them by (0, 0) once more, which
    /// still refreshes their objects' last positions.
    finished_moves: Vec<i32>,
    /// The starting state, for restarts.
    initial: Option<Box<World>>,
    /// Active area effects and the per-object area offsets (area.rs).
    pub area: crate::area::AreaState,
    /// A spawned end trigger (3600) fired: the level is complete.
    pub level_end: bool,
    /// Color/pulse/alpha triggers (level orders) that spawns or touches fired since the app last
    /// took them (`Action::Effect`).
    pub fired_effects: Vec<u32>,
    /// Spawn-triggered camera triggers (level orders) fired this step; the player's camera
    /// step applies them.
    pub fired_camera: Vec<u32>,
    /// Per level object: when an animate trigger (1585) last started it (attempt seconds).
    pub animated_at: Vec<Option<f64>>,
    /// Sequence triggers (3607) by level order: their steps (group, count), the step reached
    /// (counting repeats) and when they last fired.
    sequences: std::collections::HashMap<u32, (Vec<(i32, i32)>, usize, f64)>,
    /// Event triggers (3604): each one's events by level order, and the armed ones.
    event_lists: std::collections::HashMap<u32, Vec<i32>>,
    event_links: Vec<(u32, i32)>,
    /// Collision State sensors (3640): (level order, half size, on group, off group, inside).
    sensors: Vec<(usize, (f32, f32), i32, i32, bool)>,
    /// Timers (3614) by id: (seconds, rate, running, stop time, target group).
    pub timers: std::collections::BTreeMap<i32, (f64, f32, bool, Option<f32>, i32)>,
    /// Armed time events (3615): (timer, time, target, multi).
    time_events: Vec<(i32, f32, i32, bool)>,
    /// SFX triggers fired since the app last looked.
    pub sfx: Vec<crate::groups::SfxPlay>,
    /// Song triggers fired since the app last looked: (song id, start ms, volume, looped).
    pub songs: Vec<(i32, i32, f32, bool, i32)>,
    /// kA41 (LevelSettings +0x1cb → layer +0x7bc): rotate triggers turn solids too.
    pub rotate_free_all: bool,
    /// Level setting kA40 (layer +0x7bb): move triggers' targets are `tryGetObject` (the main
    /// object, else a random one) instead of `tryGetMainObject` (`getMoveTargetDelta`).
    pub random_move_targets: bool,
    /// The game's `m_levelLength`: furthest object x + 340 (or more for very short levels).
    /// Gameplay ends once the player reaches `level_length - 300`.
    pub level_length: f32,
    /// Widest collidable rect, so window queries can start early enough.
    max_width: f32,
    max_radius: f32,
    max_slope_width: f32,
    /// Starting speed factor from the level settings (`kA4`: 0 → 0.9, 1 → 0.7, 2 → 1.1,
    /// 3 → 1.3, 4 → 1.6).
    pub start_speed: f32,
    /// Enter effects in force (enter.rs) and the enter triggers' values by level order.
    pub enter: crate::enter::EnterState,
    pub enter_specs: std::sync::Arc<std::collections::HashMap<u32, crate::enter::EnterSpec>>,
    /// BG effect off (1819; `PlayLayer` +0x399d): the glitter stays off. Kept across attempts,
    /// as the game never resets it.
    pub bg_effect_off: bool,
    /// BG / MG speed triggers (3606 / 3612): `m_bgSpeed` (+0x314, default 0.1, 0.1) and
    /// `m_mgSpeed` (+0x31c, default 0.3, 0.5).
    pub bg_speed: (f32, f32),
    pub mg_speed: (f32, f32),
    /// Edit song / SFX triggers fired since the app last looked.
    /// With each edit SFX trigger, the level orders of group 457 (the SFX triggers whose
    /// sounds it edits).
    pub audio_edits: Vec<(crate::groups::AudioEdit, Vec<u32>)>,
    /// Touch triggers registered (`GJEffectManager` +0x1c0): (group, hold, mode, only, dual).
    touch_triggers: Vec<(i32, bool, i32, i32, bool)>,
    /// A reverse trigger (1917) fired this step (the player's camera step applies it).
    reverse: bool,
    /// Spawn particle triggers (3608) fired since the app last looked: (particle group, position,
    /// rotation, scale).
    pub particle_spawns: Vec<(i32, (f32, f32), f32, f32)>,
    /// Spawn particle triggers' values by level order: (51 particle group, 71 position group,
    /// 547/548 offset, 549/550 its variance, 551 match rotation, 552/553 rotation, 554/555
    /// scale), and the trigger's own position.
    particle_specs: std::sync::Arc<std::collections::HashMap<u32, ParticleSpec>>,
    /// A platformer checkpoint (2063) activated this step (its level order), for the app
    /// (`PlayLayer::checkpointActivated` → `processCheckpoints`).
    pub checkpoint: Option<u32>,
    /// Persistent items and timers (3641): their values survive a restart.
    pub persistent_items: std::collections::BTreeSet<i32>,
    pub persistent_timers: std::collections::BTreeSet<i32>,
    /// The level's platformer progress (UI progress trigger 3663, `GJEffectManager` +0x340).
    pub platformer_progress: i32,
    /// Platformer mode (`m_isPlatformer`, kA22).
    pub platformer: bool,
    /// Breakable blocks a reset trigger (3618) restored (indices into `objects`), and a counter
    /// the player compares with to take them off its destroyed list.
    pub restored: Vec<usize>,
    pub restore_count: u64,
    /// Advanced follow (advfollow.rs), and what it follows besides groups: player 2 and the
    /// camera (the drivers tell the world, `observe`).
    pub adv: crate::advfollow::AdvFollow,
    /// Keyframe animations (keyframes.rs).
    pub keyframes: crate::keyframes::Keyframes,
    /// Group parents (key 274) and the clusters of linked objects / parent groups that
    /// advanced follow and area triggers move as one (parents.rs).
    pub links: std::sync::Arc<crate::parents::GroupLinks>,
    pub player2_pos: Option<(f32, f32)>,
    pub camera_pos: (f32, f32),
}

/// A spawn particle trigger's values (`SpawnParticleGameObject::customObjectSetup` 0x188b3c).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSpec {
    pub group: i32,
    pub position_group: i32,
    pub at: (f32, f32),
    pub offset: (f32, f32),
    pub offset_var: (f32, f32),
    pub match_rotation: bool,
    pub rotation: f32,
    pub rotation_var: f32,
    pub scale: f32,
    pub scale_var: f32,
}

impl World {
    /// What the game changes for its own (official) levels at setup: level 22 (Dash) plays with
    /// kA40 on (move targets by `tryGetObject`).
    pub fn official(&mut self, id: i32) {
        if id == 22 {
            self.random_move_targets = true;
        }
    }

    pub fn from_level(level: &LevelString) -> World {
        let mut objects = Vec::new();
        // UI triggers (3613) take their target groups off the level (`addUIObject` removes them
        // from their sections): never collided.
        let ui_groups: std::collections::HashSet<i32> = level.objects.iter().filter(|o| o.id() == 3613).filter_map(|o| o.get_i32(51)).collect();
        for (order, raw) in level.objects.iter().enumerate() {
            let Some(info) = objects::info(raw.canonical_id()) else { continue };
            if !ui_groups.is_empty() && raw.groups().iter().any(|g| ui_groups.contains(g)) {
                continue;
            }
            // Speed portals are modifiers that act on touch; other modifiers aren't collided.
            let kind = if speed_portal_factor(raw.canonical_id()).is_some() {
                ObjectType::SPEED_PORTAL
            } else if info.kind == ObjectType::ANIMATED_HAZARD {
                // Animated hazards (monsters, eyes): deadly like spikes, by rect or radius.
                ObjectType::HAZARD
            } else {
                info.kind
            };
            // "No touch" (key 121) objects are decoration to the game: never collided.
            if matches!(kind, ObjectType::DECORATION | ObjectType::MODIFIER) || raw.get(121) == Some("1") {
                continue;
            }
            let x = raw.x();
            let y = raw.y() + GROUND_OFFSET;
            let flip_x = raw.get(4) == Some("1");
            let flip_y = raw.get(5) == Some("1");
            let rotation = raw.get_f32(6).unwrap_or(0.0);
            let (slope_dir, slope_uphill) = slope_direction(flip_x, flip_y, rotation);
            // Scale: key 32, or 128/129 per axis (the editor's scale, flips are separate).
            let scale = raw.get_f32(32).unwrap_or(1.0);
            let (sx, sy) = (raw.get_f32(128).unwrap_or(scale), raw.get_f32(129).unwrap_or(scale));
            let shape = Shape::new(info, flip_x, flip_y, rotation, (sx, sy));
            let (rect, obb) = shape.at(x, y);
            // getObjectRadius: scaled by the larger scale unless both are 1.
            let radius = if sx == 1.0 && sy == 1.0 { info.radius } else { info.radius * sx.max(sy) };
            objects.push(Object {
                id: raw.canonical_id(),
                kind,
                x,
                y,
                rect,
                shape,
                section: section_of(x),
                order: order as u32,
                radius,
                flip_x,
                flip_y,
                rotation,
                scale: (sx, sy),
                obb,
                slope_dir,
                slope_uphill,
                // Spike slopes (`m_slopeIsHazard`): their surface sits 4 units further out and
                // any snap onto them kills. Ids from the game's own object dump.
                slope_hazard: matches!(raw.canonical_id(), 363 | 364 | 366 | 367 | 1717 | 1718 | 1723 | 1724),
                pos: (x as f64, y as f64),
                last_pos: (x, y),
                moved_step: 0,
                grouped: raw.get(57).is_some_and(|g| !g.is_empty()),
                teleport_dy: raw.get_f32(54).unwrap_or(100.0),
                teleport_gravity: raw.get_i32(354).unwrap_or(0) as i8,
                teleport_force: (raw.get(345) == Some("1")).then(|| (raw.get_f32(346).unwrap_or(0.0), raw.get(347) == Some("1"))),
                teleport_target: (kind == ObjectType::TELEPORT_PORTAL && raw.canonical_id() != 747).then(|| raw.get_i32(51)).flatten(),
                teleport_keep: (raw.get(352) == Some("1"), raw.get(353) == Some("1")),
                teleport_smooth: raw.get(55) == Some("1"),
                portal_camera: (raw.get(112) == Some("1")).then(|| {
                    let pad = raw.get_f32(114).unwrap_or(0.0);
                    let ease = raw.get_f32(113).unwrap_or(0.0);
                    (if pad < 0.0 { 0.0 } else if pad > 1.0 { 1.0 } else { pad }, if ease < 1.0 { 1.0 } else if ease > 40.0 { 40.0 } else { ease })
                }),
                passable: raw.get(134) == Some("1"),
                non_stick: (raw.get(136) == Some("1"), raw.get(289) == Some("1")),
                extra_sticky: raw.get(495) == Some("1"),
                grip_slope: raw.get(193).is_some_and(|v| v.parse::<i32>().unwrap_or(0) != 0),
                custom_ring: (kind == ObjectType::CUSTOM_RING).then(|| {
                    (raw.get_i32(51).unwrap_or(0), raw.get(56) == Some("1"), raw.get(504) == Some("1"))
                }),
            });
        }
        // Groups the triggers act on, and which objects move.
        let triggers = crate::groups::GroupTriggers::new(level);
        let mut moving_groups = std::collections::HashSet::new();
        let mut members: std::collections::HashMap<i32, Vec<u32>> = std::collections::HashMap::new();
        let mut adv_delayed: Vec<i32> = Vec::new();
        for action in triggers.actions() {
            match action {
                crate::groups::Action::Move(m) => {
                    moving_groups.insert(m.group);
                    members.entry(m.group).or_default();
                    if let Some((target, center, _)) = m.target {
                        members.entry(target).or_default();
                        members.entry(center).or_default();
                    }
                }
                crate::groups::Action::FollowY { group, .. } => {
                    moving_groups.insert(group);
                    members.entry(group).or_default();
                }
                crate::groups::Action::Follow { group, followed, .. } => {
                    moving_groups.insert(group);
                    members.entry(group).or_default();
                    members.entry(followed).or_default();
                }
                crate::groups::Action::Rotate(r) => {
                    moving_groups.insert(r.group);
                    members.entry(r.group).or_default();
                    members.entry(r.center).or_default();
                }
                crate::groups::Action::Scale(r) => {
                    moving_groups.insert(r.group);
                    members.entry(r.group).or_default();
                    members.entry(r.center).or_default();
                }
                crate::groups::Action::Toggle { group, .. }
                | crate::groups::Action::Reset { group }
                | crate::groups::Action::Touch { group, .. }
                | crate::groups::Action::OnDeath { group, .. }
                | crate::groups::Action::Animate { group, .. }
                | crate::groups::Action::Collision { group, .. }
                | crate::groups::Action::Count { group, .. }
                | crate::groups::Action::InstantCount { group, .. } => {
                    members.entry(group).or_default();
                }
                crate::groups::Action::Area(a) => {
                    // Area moves move their targets' positions (collidable ones too).
                    moving_groups.insert(a.target);
                    members.entry(a.target).or_default();
                    members.entry(a.center).or_default();
                }
                crate::groups::Action::Spawn { .. }
                | crate::groups::Action::InstantCollision { .. }
                | crate::groups::Action::ItemCompare(_)
                | crate::groups::Action::Teleport { .. }
                | crate::groups::Action::PlayerControl { .. }
                | crate::groups::Action::Song { .. }
                | crate::groups::Action::Timer { .. }
                | crate::groups::Action::Sequence { .. }
                | crate::groups::Action::EventLink { .. }
                | crate::groups::Action::TimeEvent { .. }
                | crate::groups::Action::TimeControl { .. }
                | crate::groups::Action::ItemEdit(_)
                | crate::groups::Action::Stop { .. }
                | crate::groups::Action::Noop
                | crate::groups::Action::TimeWarp { .. }
                | crate::groups::Action::End
                | crate::groups::Action::Gravity { .. }
                | crate::groups::Action::Pickup { .. }
                | crate::groups::Action::Random { .. }
                | crate::groups::Action::PlayerVisible { .. }
                | crate::groups::Action::Shake { .. }
                | crate::groups::Action::Chance { .. }
                | crate::groups::Action::AreaStop { .. }
                | crate::groups::Action::Persistent { .. }
                | crate::groups::Action::Progress { .. }
                | crate::groups::Action::BgEffect { .. }
                | crate::groups::Action::ArtSpeed { .. }
                | crate::groups::Action::Reverse
                | crate::groups::Action::Enter { .. }
                | crate::groups::Action::EnterStop { .. }
                | crate::groups::Action::Effect { .. }
                | crate::groups::Action::Camera { .. } => {}
                crate::groups::Action::Sfx(play) => {
                    // the proximity group and its listener group
                    if let Some(p) = play.proximity {
                        for g in [p.group, p.center].into_iter().filter(|&g| g > 0) {
                            members.entry(g).or_default();
                        }
                    }
                }
                crate::groups::Action::AudioEdit(edit) => {
                    // the SFX triggers it edits, and the proximity groups
                    if edit.triggers > 0 {
                        members.entry(edit.triggers).or_default();
                    }
                    if let Some(p) = edit.proximity {
                        for g in [p.group, p.center].into_iter().filter(|&g| g > 0) {
                            members.entry(g).or_default();
                        }
                    }
                }
                crate::groups::Action::KeyframeAnim { order } => {
                    // the animated group, and the keyframe objects' groups
                    if let Some(raw) = level.objects.get(order as usize) {
                        if let Some(g) = raw.get_i32(51).filter(|&g| g > 0) {
                            moving_groups.insert(g);
                            members.entry(g).or_default();
                        }
                        if let Some(g) = raw.get_i32(76).filter(|&g| g > 0) {
                            members.entry(g).or_default();
                        }
                    }
                    for o in level.objects.iter().filter(|o| o.id() == 3032) {
                        if let Some(g) = o.get_i32(51).filter(|&g| g > 0) {
                            moving_groups.insert(g);
                            members.entry(g).or_default();
                        }
                    }
                }
                crate::groups::Action::AdvFollow { order, group, follow } | crate::groups::Action::AdvFollowEdit { order, group, follow } => {
                    // the followed group and the moved group (as a spawn remap leaves them)
                    if let Some(raw) = level.objects.get(order as usize) {
                        if group > 0 && raw.id() == 3016 {
                            moving_groups.insert(group);
                            // a remapped follow target with a delay needs its history kept
                            let spec = crate::advfollow::AdvSpec::from_raw(raw, order);
                            if follow > 0 && spec.follow > 0 && (spec.delay.0 > 0.0 || spec.delay.1 != 0.0) {
                                adv_delayed.push(follow);
                            }
                        }
                        let keys = [raw.get_i32(560).unwrap_or(0), raw.get_i32(565).unwrap_or(0)];
                        for g in [group, follow].into_iter().chain(keys).filter(|&g| g > 0) {
                            members.entry(g).or_default();
                        }
                    }
                }
                crate::groups::Action::SpawnParticle { order } | crate::groups::Action::Checkpoint { order } => {
                    // their position groups (and the particle group, for the app)
                    if let Some(raw) = level.objects.get(order as usize) {
                        for k in [51, 71] {
                            if let Some(g) = raw.get_i32(k).filter(|&g| g > 0) {
                                members.entry(g).or_default();
                            }
                        }
                    }
                }
            }
        }
        // Teleport targets: their group's main object.
        for o in &objects {
            if let Some(g) = o.teleport_target {
                members.entry(g).or_default();
            }
        }
        // Group parents and linked objects (parents.rs): the clusters advanced follow and area
        // triggers move as one, and the groups whose main object is a parent.
        let links = crate::parents::GroupLinks::new(level);
        let mut moving_orders: std::collections::HashSet<u32> = links.cluster_objects().collect();
        for (order, raw) in level.objects.iter().enumerate() {
            for g in raw.groups() {
                if let Some(m) = members.get_mut(&g) {
                    m.push(order as u32);
                    if moving_groups.contains(&g) {
                        moving_orders.insert(order as u32);
                    }
                }
            }
        }
        let can_move: Vec<bool> = (0..level.objects.len()).map(|o| moving_orders.contains(&(o as u32))).collect();
        let (mut objects, dynamic): (Vec<Object>, Vec<Object>) =
            objects.into_iter().partition(|o| !moving_orders.contains(&o.order));
        objects.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        let first_dynamic = objects.len();
        objects.extend(dynamic);
        let mut collider = vec![u32::MAX; level.objects.len()];
        for (i, o) in objects.iter().enumerate() {
            collider[o.order as usize] = i as u32;
        }
        let max_width = objects.iter().map(|o| o.rect.w).fold(0.0, f32::max);
        let mut by_class: [Vec<u32>; 3] = Default::default();
        for (i, o) in objects[..first_dynamic].iter().enumerate() {
            if let Some(k) = collider_class(o.kind) {
                by_class[k].push(i as u32);
            }
        }
        if std::env::var_os("CADENCE_DEBUG_WORLD").is_some() {
            let mut w: Vec<f32> = objects.iter().map(|o| o.rect.w).collect();
            w.sort_by(|a, b| b.total_cmp(a));
            eprintln!("world: {} colliders, {} moving, widest {:?}", objects.len(), objects.len() - first_dynamic, &w[..w.len().min(8)]);
        }
        let max_x = level.objects.iter().map(|o| o.x()).fold(0.0, f32::max);
        let start_speed = match level.setting("kA4") {
            Some("1") => 0.7,
            Some("2") => 1.1,
            Some("3") => 1.3,
            Some("4") => 1.6,
            _ => 0.9,
        };
        let max_radius = objects.iter().map(|o| o.radius).fold(0.0, f32::max);
        let max_slope_width = objects.iter().filter(|o| o.kind == ObjectType::SLOPE).map(|o| o.rect.w).fold(0.0, f32::max);
        let collectibles: Vec<(u32, i32, i32, Rect)> = level
            .objects
            .iter()
            .enumerate()
            .filter_map(|(order, raw)| {
                let info = objects::info(raw.canonical_id())?;
                let item = raw.get_i32(80).filter(|_| info.kind.0 == 30)?;
                let scale = raw.get_f32(32).unwrap_or(1.0);
                let s = (raw.get_f32(128).unwrap_or(scale), raw.get_f32(129).unwrap_or(scale));
                let shape = Shape::new(info, raw.get(4) == Some("1"), raw.get(5) == Some("1"), raw.get_f32(6).unwrap_or(0.0), s);
                let (rect, _) = shape.at(raw.x(), raw.y() + GROUND_OFFSET);
                Some((order as u32, item, if raw.get(78) == Some("1") { -1 } else { 1 }, rect))
            })
            .collect();
        let n = level.objects.len();
        let start: Vec<(f32, f32)> = level.objects.iter().map(|o| (o.x(), o.y() + GROUND_OFFSET)).collect();
        let level_pos = start.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
        let mut world = World {
            objects,
            first_dynamic,
            can_move,
            track_moves: false,
            moved_log: Vec::new(),
            epoch: World::next_epoch(),
            by_class,
            enabled: vec![true; n],
            moved: vec![(0.0, 0.0); n],
            turned: vec![0.0; n],
            scaled: vec![(1.0, 1.0); n],
            level_last: start.clone(),
            level_stamp: vec![0; n],
            area: crate::area::AreaState::new(n),
            level_end: false,
            fired_effects: Vec::new(),
            fired_camera: Vec::new(),
            songs: Vec::new(),
            sfx: Vec::new(),
            timers: std::collections::BTreeMap::new(),
            sequences: level
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| o.id() == 3607)
                .map(|(order, o)| {
                    let parts: Vec<i32> = o.get(435).unwrap_or("").split('.').filter_map(|v| v.parse().ok()).collect();
                    let steps = parts.chunks(2).map(|c| (c[0], c.get(1).copied().unwrap_or(1).max(1))).collect();
                    (order as u32, (steps, 0, f64::NEG_INFINITY))
                })
                .collect(),
            event_lists: level
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| o.id() == 3604)
                .map(|(order, o)| (order as u32, o.get(430).unwrap_or("").split('.').filter_map(|v| v.parse().ok()).collect()))
                .collect(),
            event_links: Vec::new(),
            sensors: level
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| o.id() == 3640)
                .map(|(order, o)| {
                    let scale = o.get_f32(32).unwrap_or(1.0);
                    let (sx, sy) = (o.get_f32(128).unwrap_or(scale), o.get_f32(129).unwrap_or(scale));
                    (order, (15.0 * sx.abs(), 15.0 * sy.abs()), o.get_i32(51).unwrap_or(0), o.get_i32(71).unwrap_or(0), false)
                })
                .collect(),
            time_events: Vec::new(),
            animated_at: vec![None; n],
            rotate_free_all: level.setting("kA41") == Some("1") || std::env::var_os("CADENCE_ROTATE_ALL").is_some(),
            random_move_targets: level.setting("kA40") == Some("1"),
            start,
            level_pos,
            collectibles,
            members,
            collider,
            triggers,
            last_player: None,
            player_history: std::collections::VecDeque::new(),
            collision_blocks: level
                .objects
                .iter()
                .enumerate()
                .filter(|(_, raw)| raw.id() == 1816)
                .map(|(order, raw)| {
                    let scale = raw.get_f32(32).unwrap_or(1.0);
                    let (sx, sy) = (raw.get_f32(128).unwrap_or(scale), raw.get_f32(129).unwrap_or(scale));
                    (order, raw.get_i32(80).unwrap_or(0), raw.get(94) == Some("1"), (15.0 * sx.abs(), 15.0 * sy.abs()))
                })
                .collect(),
            collision_triggers: Vec::new(),
            player_rect: None,
            teleport: None,
            teleport_now: None,
            death_triggers: Vec::new(),
            control: None,
            control_now: None,
            colliding: std::collections::HashSet::new(),
            step_count: 0,
            time_warp: 1.0,
            gravity_mod: 1.0,
            player_hidden: false,
            shake: (0.0, 0.0, 0.0),
            warp_acc: 0.0,
            frame_left: 0,
            frame_steps: 4,
            frame_secs: 1.0 / 60.0,
            updates: 0,
            sched_every: 1,
            sched_phase: 0,
            step_secs: 1.0 / 240.0,
            step_dt60: 0.25,
            finished_moves: Vec::new(),
            initial: None,
            max_width,
            max_radius,
            max_slope_width,
            base_rotation: std::sync::Arc::new(level.objects.iter().map(|o| o.get_f32(6).unwrap_or(0.0)).collect()),
            level_length: max_x + 340.0,
            start_speed,
            enter: crate::enter::EnterState::default(),
            enter_specs: std::sync::Arc::new(
                level
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| crate::enter::is_enter_trigger(o.id()))
                    .map(|(order, o)| {
                        let mut spec = crate::enter::EnterSpec::from_raw(o);
                        spec.order = order as u32;
                        (order as u32, spec)
                    })
                    .collect(),
            ),
            bg_effect_off: false,
            bg_speed: (0.1, 0.1),
            mg_speed: (0.3, 0.5),
            audio_edits: Vec::new(),
            touch_triggers: Vec::new(),
            reverse: false,
            particle_spawns: Vec::new(),
            particle_specs: std::sync::Arc::new(
                level
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.id() == 3608)
                    .map(|(order, o)| {
                        let f = |k: u32| o.get_f32(k).unwrap_or(0.0);
                        let spec = ParticleSpec {
                            group: o.get_i32(51).unwrap_or(0),
                            position_group: o.get_i32(71).unwrap_or(0),
                            at: (o.x(), o.y() + GROUND_OFFSET),
                            offset: (f(547), f(548)),
                            offset_var: (f(549), f(550)),
                            match_rotation: o.get(551) == Some("1"),
                            rotation: f(552),
                            rotation_var: f(553),
                            scale: f(554),
                            scale_var: f(555),
                        };
                        (order as u32, spec)
                    })
                    .collect(),
            ),
            checkpoint: None,
            persistent_items: Default::default(),
            persistent_timers: Default::default(),
            platformer_progress: 0,
            platformer: level.setting("kA22") == Some("1"),
            restored: Vec::new(),
            restore_count: 0,
            adv: crate::advfollow::AdvFollow::new(level).with_delayed(&adv_delayed),
            keyframes: crate::keyframes::Keyframes::new(level),
            links: std::sync::Arc::new(links),
            player2_pos: None,
            camera_pos: (0.0, 0.0),
        };
        if first_dynamic < world.objects.len() || !world.members.is_empty() || !world.enter_specs.is_empty() || !world.triggers.actions().is_empty() {
            world.initial = Some(Box::new(world.clone()));
        }
        world
    }

    /// A level object's current position (f64; objects no trigger moves stay at their start).
    /// A group's main object (its first by level order), now.
    pub fn group_main_position(&self, group: i32) -> Option<(f64, f64)> {
        self.members.get(&group).and_then(|m| m.first()).map(|&o| self.level_pos[o as usize])
    }

    pub fn level_position(&self, order: usize) -> (f64, f64) {
        self.level_pos[order]
    }

    /// Back to the level's starting state (a new attempt).
    /// `fast_srand(seed)` as every attempt starts (the game seeds from the clock in
    /// `PlayLayer::resetLevel`; tests and harness comparisons fix it).
    pub fn set_fast_seed(&mut self, seed: u64) {
        self.triggers.reseed(seed);
        self.adv.reseed(seed);
        if let Some(initial) = self.initial.as_mut() {
            initial.triggers.reseed(seed);
            initial.adv.reseed(seed);
        }
    }

    pub fn reset(&mut self) {
        // kept across attempts: the BG effect switch, persistent items and timers
        let bg_off = self.bg_effect_off;
        let (pi, pt) = (std::mem::take(&mut self.persistent_items), std::mem::take(&mut self.persistent_timers));
        let items: Vec<(i32, i32)> = pi.iter().filter_map(|id| self.triggers.items.get(id).map(|&v| (*id, v))).collect();
        let timers: Vec<(i32, (f64, f32, bool, Option<f32>, i32))> = pt.iter().filter_map(|id| self.timers.get(id).map(|&v| (*id, v))).collect();
        if let Some(initial) = self.initial.take() {
            let (every, phase, track) = (self.sched_every, self.sched_phase, self.track_moves);
            *self = (*initial).clone();
            self.initial = Some(initial);
            self.sched_every = every;
            self.sched_phase = phase;
            self.track_moves = track;
        } else {
            self.enter = Default::default();
        }
        self.bg_effect_off = bg_off;
        self.persistent_items = pi;
        self.persistent_timers = pt;
        for (id, v) in items {
            self.triggers.items.insert(id, v);
        }
        for (id, v) in timers {
            self.timers.insert(id, v);
        }
        self.epoch = World::next_epoch();
    }

    /// A new value for `epoch` (the world was replaced: reset, or a checkpoint loaded).
    pub fn next_epoch() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// The scheduler's frame time when the coming step starts a rendered frame (the harness
    /// runs `sched_every` game updates per frame and gives the actions `sched_every` × 1/60 s;
    /// the game app one update per frame), else `None`.
    pub fn scheduler_tick_due(&self) -> Option<f32> {
        let every = self.sched_every.max(1) as u64;
        (self.frame_left == 0 && (self.updates + self.sched_phase as u64) % every == 0)
            .then(|| (1.0f32 / 60.0) * self.sched_every.max(1) as f32)
    }

    /// The next frame's split: (steps, game seconds run, seconds carried on).
    fn split_frame(&self) -> (u32, f64, f64) {
        let tw = self.time_warp as f64;
        let base = if tw < 1.0 { tw * 0.004166666666666667 } else { 0.004166666666666667 };
        let frame = (1.0f32 / 60.0) as f64;
        let total = (self.warp_acc + frame) as f32 as f64;
        let n = (total / base) as i64;
        let local = base * n as f64;
        let steps = ((((local * 60.0) / tw.min(1.0)) * 4.0) as i64 as f32).max(1.0) as u32;
        (steps.max(1), local, total - local)
    }

    /// Wall-clock seconds the next step stands for: each 1/60 s frame runs the steps the frame
    /// split gives it (4 at normal speed, 8 at time warp 0.5), so slowed sections play at the
    /// game's speed rather than at a fixed 240 steps a second.
    pub fn real_step_secs(&self) -> f64 {
        let steps = if self.frame_left > 0 { self.frame_steps } else { self.split_frame().0 };
        (1.0 / 60.0) / steps as f64
    }

    /// After the last step of a frame: the frame's game seconds, which `GJBaseGameLayer::update`
    /// then steps the effects by (`GJEffectManager::updateEffects((float)local)`); else `None`.
    pub fn frame_end(&self) -> Option<f32> {
        (self.frame_left == 0).then_some(self.frame_secs as f32)
    }

    /// `GJBaseGameLayer::update`'s frame split (0x10663c) for one 1/60 s frame (f32, as the
    /// harness drives it): the frame plus the carried time holds n steps of tw/240 s (1/240
    /// from tw ≥ 1), the rest carried on; steps = int(local·60/min(tw, 1)·4). A warp change
    /// ends the frame early (see `apply`).
    fn start_frame(&mut self) {
        self.updates += 1;
        let (steps, local, carried) = self.split_frame();
        self.warp_acc = carried;
        self.frame_steps = steps;
        self.frame_secs = local;
        self.frame_left = steps;
        self.step_secs = local / steps as f64;
        // fVar42 = (local·60)/steps (the player update and collisions).
        self.step_dt60 = ((local * 60.0) / steps as f64) as f32;
    }

    /// The start of a physics step, before the player moves: run the move commands
    /// (`prepareMoveActions` + `processMoveActions`). `player` is player 1's position now.
    pub fn pre_step(&mut self, player: (f32, f32)) {
        self.fired_camera.clear();
        let (pdx, pdy) = match self.last_player {
            Some((x, y)) => (player.0 - x, player.1 - y),
            None => (0.0, 0.0),
        };
        self.last_player = Some(player);
        self.teleport_now = self.teleport.take();
        self.control_now = self.control.take();
        self.reverse = false;
        self.step_count += 1;
        let now = self.step_count as f64 * self.step_secs;
        self.player_history.push_back((now, player.1));
        while self.player_history.len() > 2400 {
            self.player_history.pop_front();
        }
        if self.frame_left == 0 {
            self.start_frame();
        }
        self.frame_left = self.frame_left.saturating_sub(1);
        self.step_timers();
        // updateSpawnTriggers runs first in the step: delayed spawns come due here.
        let due = self.triggers.due(self.step_secs as f32 as f64, &self.enabled);
        if !due.is_empty() {
            self.apply(due);
        }
        // prepareMoveActions keeps a finished command one more step (+0x72) and
        // processMoveActions calls moveObjects even with a zero move: last positions refresh.
        for group in std::mem::take(&mut self.finished_moves) {
            for order in self.members.get(&group).cloned().unwrap_or_default() {
                let pos = self.level_pos[order as usize];
                self.set_level_pos(order as usize, pos, 0.0);
            }
        }
        if self.triggers.commands.is_empty() && self.keyframes.commands.is_empty() {
            self.process_adv_follow();
            self.process_area_moves();
            return;
        }
        let dt = self.step_secs as f32;
        if std::env::var_os("CADENCE_DEBUG_COMMANDS").is_some() && (self.step_count < 6 || self.step_count % 60 == 0) {
            let follows = self.triggers.commands.iter().filter(|c| c.follow.is_some() || c.follow_y.is_some()).count();
            eprintln!("step {}: {} move commands ({} follows)", self.step_count, self.triggers.commands.len(), follows);
        }
        // Per group, the sum of this step's moves (`getTempGroupCommand`), in first-use order;
        // per (group, center), the sum of this step's rotations.
        let mut moves: Vec<(i32, f64, f64)> = Vec::new();
        let mut turns: Vec<(i32, i32, f64, bool)> = Vec::new();
        let mut scales: Vec<(i32, i32, f32, f32, bool)> = Vec::new();
        let debug_group: Option<i32> = std::env::var("CADENCE_DEBUG_GROUP").ok().and_then(|v| v.parse().ok());
        let mut follow_ys: Vec<(i32, f32, f32, i32, f32)> = Vec::new();
        for cmd in &mut self.triggers.commands {
            let (mut dx, mut dy) = cmd.step(dt);
            if let Some((speed, delay, offset, max_speed)) = cmd.follow_y {
                follow_ys.push((cmd.spec.group, speed, delay, offset, max_speed));
                continue;
            }
            if debug_group == Some(cmd.spec.group) {
                eprintln!("group {} step {}: cmd dy {:.6} spec {:?}", cmd.spec.group, self.step_count, dy, cmd.spec);
            }
            if cmd.follow.is_some() {
                continue;
            }
            if let Some((center, only_move)) = cmd.scale {
                // this step's factor: (1 + reached) / (1 + reached before)
                let (rx, ry) = ((1.0 + cmd.reached(0)) / (1.0 + cmd.reached(0) - dx), (1.0 + cmd.reached(1)) / (1.0 + cmd.reached(1) - dy));
                if rx != 1.0 || ry != 1.0 {
                    scales.push((cmd.spec.group, center, rx as f32, ry as f32, only_move));
                }
                continue;
            }
            if let Some((center, lock)) = cmd.rotate {
                match turns.iter_mut().find(|t| t.0 == cmd.spec.group && t.1 == center) {
                    Some(t) => t.2 += dx,
                    None => turns.push((cmd.spec.group, center, dx, lock)),
                }
                continue;
            }
            if cmd.spec.lock_x {
                dx += pdx as f64 * cmd.spec.lock_mod.0 as f64;
            }
            if cmd.spec.lock_y {
                dy += pdy as f64 * cmd.spec.lock_mod.1 as f64;
            }
            match moves.iter_mut().find(|m| m.0 == cmd.spec.group) {
                Some(m) => {
                    m.1 += dx;
                    m.2 += dy;
                }
                None => moves.push((cmd.spec.group, dx, dy)),
            }
        }
        // keyframe commands (mode 5 in prepareMoveActions): a move into the group's sum, a turn
        // and a scale about the group's main object
        if !self.keyframes.commands.is_empty() {
            let mut spawns = Vec::new();
            for k in self.keyframe_step(dt as f64) {
                if k.mv != (0.0, 0.0) {
                    match moves.iter_mut().find(|m| m.0 == k.group) {
                        Some(m) => {
                            m.1 += k.mv.0 as f64;
                            m.2 += k.mv.1 as f64;
                        }
                        None => moves.push((k.group, k.mv.0 as f64, k.mv.1 as f64)),
                    }
                }
                if k.turn != 0.0 {
                    // (real traces: each object turns in place, nothing orbits)
                    match turns.iter_mut().find(|t| t.0 == k.group && t.1 == IN_PLACE) {
                        Some(t) => t.2 += k.turn,
                        None => turns.push((k.group, IN_PLACE, k.turn, false)),
                    }
                }
                if let Some((rx, ry)) = k.scale {
                    scales.push((k.group, IN_PLACE, rx as f32, ry as f32, false));
                }
                spawns.extend(k.spawns);
            }
            if !spawns.is_empty() {
                let actions: Vec<_> = spawns
                    .into_iter()
                    // queued: they come due at the next step's start (real traces: a spawned
                    // move starts a step later than an instant spawn would)
                    .map(|(group, delay)| (crate::groups::Action::Spawn { group, delay: delay.max(1e-9), ordered: false, remap: 0 }, Vec::new()))
                    .collect();
                let actions = self.triggers.expand_public(actions, &self.enabled);
                self.apply(actions);
            }
        }
        for (group, center, rx, ry, only_move) in scales {
            self.scale_group(group, center, rx, ry, only_move);
        }
        for c in self.triggers.commands.iter().filter(|c| c.done && c.rotate.is_none() && c.scale.is_none() && c.follow.is_none()) {
            if !self.finished_moves.contains(&c.spec.group) {
                self.finished_moves.push(c.spec.group);
            }
        }
        self.triggers.commands.retain(|c| !c.done || c.follow.is_some());
        // processRotationActions runs first and takes the group's move along (claimMoveAction).
        for (group, center, degrees, lock) in turns {
            let delta = degrees as f32;
            if delta == 0.0 {
                continue;
            }
            // no main object for the center (`tryGetMainObject`: no parent and not exactly one
            // object): every object turns in place (unless locked) and the move isn't claimed
            if center != IN_PLACE && self.main_object(center).is_none() {
                if !lock {
                    self.rotate_group(group, IN_PLACE, delta, (0.0, 0.0), false);
                }
                continue;
            }
            let claimed = match moves.iter().position(|m| m.0 == group) {
                Some(i) => {
                    let m = moves.remove(i);
                    (m.1 as f32, m.2 as f32)
                }
                None => (0.0, 0.0),
            };
            self.rotate_group(group, center, delta, claimed, lock);
        }
        for (group, dx, dy) in moves {
            let (fx, fy) = (dx as f32, dy as f32);
            if fx != 0.0 || fy != 0.0 {
                self.move_group(group, fx as f64, fy as f64);
            }
        }
        // processFollowActions: the followed main object's movement this step (stamped now if
        // it hasn't moved), times the mods.
        let follows: Vec<(i32, i32, f32, f32)> = self
            .triggers
            .commands
            .iter()
            .filter_map(|c| c.follow.map(|(f, mx, my)| (c.spec.group, f, mx, my)))
            .collect();
        for (group, followed, mx, my) in follows {
            let Some(main) = self.main_object(followed) else { continue };
            let main = main as usize;
            self.stamp(main);
            let dx = mx as f64 * (self.level_pos[main].0 - self.level_last[main].0 as f64);
            let dy = my as f64 * (self.level_pos[main].1 - self.level_last[main].1 as f64);
            if dx != 0.0 || dy != 0.0 {
                self.move_group(group, dx, dy);
            }
        }
        // processPlayerFollowActions
        let dt60 = dt as f64 * 60.0;
        for (group, speed, delay, offset, max_speed) in follow_ys {
            let share = (speed as f64 * dt60).clamp(0.0, 1.0) as f32;
            let target = self.old_player_y(delay);
            if target == 0.0 {
                continue;
            }
            let cap = (max_speed as f64 * dt60) as f32;
            for order in self.members.get(&group).cloned().unwrap_or_default() {
                let i = order as usize;
                self.stamp(i);
                let gap = share * ((target + offset as f32) - self.level_pos[i].1 as f32);
                if gap == 0.0 {
                    continue;
                }
                let dy = if cap > 0.0 { gap.clamp(-cap, cap) } else { gap };
                let pos = self.level_pos[i];
                self.set_level_pos(i, (pos.0, pos.1 + dy as f64), 0.0);
            }
        }
        self.triggers.commands.retain(|c| !c.done);
        // processAdvancedFollowActions (after the player follows; the game runs the follow
        // triggers after it)
        self.process_adv_follow();
        self.process_area_moves();
    }

    /// A reverse trigger fired this step (`Player::step_camera` turns the players around).
    pub fn reverse_pending(&self) -> bool {
        self.reverse
    }

    /// What the world can't see itself: player 2's position and the camera (advanced follow
    /// triggers can follow them). Drivers call it before `pre_step`.
    pub fn observe(&mut self, player2: Option<(f32, f32)>, camera: (f32, f32)) {
        self.player2_pos = player2;
        self.camera_pos = camera;
    }

    /// Player 1's position at this step's start.
    pub(crate) fn last_player_pos(&self) -> Option<(f32, f32)> {
        self.last_player
    }

    /// `getLastPosition` of a level object.
    pub(crate) fn level_last_of(&self, i: usize) -> (f32, f32) {
        self.level_last[i]
    }

    /// An object's rotation now (its own plus what triggers turned it).
    pub(crate) fn turned_of(&self, i: usize) -> f32 {
        self.base_rotation.get(i).copied().unwrap_or(0.0) + self.turned[i]
    }

    /// `m_canRotateFree`: solids, breakables and slopes don't turn unless the level allows it.
    pub(crate) fn can_rotate_free(&self, i: usize) -> bool {
        let c = self.collider[i];
        !(c != u32::MAX && matches!(self.objects[c as usize].kind.0, 0 | 21 | 25)) || self.rotate_free_all
    }

    /// Turn a level object in place.
    pub(crate) fn turn_object(&mut self, i: usize, degrees: f32) {
        let pos = self.level_pos[i];
        self.set_level_pos(i, pos, degrees);
    }

    /// `PlayerObject::getOldPosition`: player 1's y `delay` seconds ago (0.01 s buckets, at
    /// most 1.99 s back), or now.
    fn old_player_y(&self, delay: f32) -> f32 {
        let Some(&(now, y)) = self.player_history.back() else { return 0.0 };
        if delay <= 0.0 {
            return y;
        }
        let back = ((delay / 0.01) as i32).min(199) as f64 * 0.01;
        self.player_history.iter().rev().find(|(t, _)| now - t >= back).or(self.player_history.front()).map(|&(_, y)| y).unwrap_or(y)
    }

    /// The rotation part of `processRotationActions`: each object of the group orbits the
    /// center object (the first of its group) by `delta` degrees through cocos2d's node
    /// transform (`CCPointApplyAffineTransform`, doubles inside, f32 out), plus the claimed
    /// move, and turns itself by `delta` unless locked. Not yet checked against the game.
    fn rotate_group(&mut self, group: i32, center: i32, delta: f32, (mx, my): (f32, f32), lock: bool) {
        if center == IN_PLACE {
            // keyframe rotations: every object turns about itself (plus the claimed move)
            for order in self.members.get(&group).cloned().unwrap_or_default() {
                let i = order as usize;
                let (x, y) = self.level_pos[i];
                let turn = if self.can_rotate_free(i) { delta } else { 0.0 };
                self.set_level_pos(i, (x + mx as f64, y + my as f64), turn);
            }
            return;
        }
        // the center group's main object (`tryGetMainObject`: its parent, or its only object)
        let Some(center_order) = self.main_object(center) else { return };
        let Some(members) = self.members.get(&group).cloned() else { return };
        let (cx, cy) = (self.level_pos[center_order as usize].0 as f32, self.level_pos[center_order as usize].1 as f32);
        // CCNode::nodeToParentTransform with rotation r: radians = -r·(π/180) in f32.
        let rad = -(delta * 0.017_453_292_f32);
        let (cos, sin) = ((rad as f64).cos() as f32, (rad as f64).sin() as f32);
        let (a, b, c, d) = (cos as f64, sin as f64, -sin as f64, cos as f64);
        for order in members {
            let i = order as usize;
            let (x, y) = self.level_pos[i];
            let px = (x - cx as f64) as f32;
            let py = (y - cy as f64) as f32;
            let rx = (a * px as f64 + c * py as f64 + cx as f64) as f32;
            let ry = (b * px as f64 + d * py as f64 + cy as f64) as f32;
            let nx = x + ((rx + mx) as f64 - x);
            let ny = y + ((ry + my) as f64 - y);
            // `rotateObject` (0xfe7a4) turns only objects with m_canRotateFree (+0x364):
            // `GameObject::canRotateFree` (0x5afcf8) is false for solids, breakables and slopes
            // (types 0, 21, 25) that aren't no-touch, unless the level allows it
            // (settings +0x1cb → layer +0x7bc; off in the Towers). They orbit unturned.
            let fixed = {
                let c = self.collider[i];
                c != u32::MAX && matches!(self.objects[c as usize].kind.0, 0 | 21 | 25)
            };
            self.set_level_pos(i, (nx, ny), if lock || (fixed && !self.rotate_free_all) { 0.0 } else { delta });
        }
    }

    /// Scale trigger: the group's positions scale about the center group's main object (each
    /// object about itself without one), and the objects with them unless only moving.
    fn scale_group(&mut self, group: i32, center: i32, rx: f32, ry: f32, only_move: bool) {
        let Some(members) = self.members.get(&group).cloned() else { return };
        let c = self.members.get(&center).and_then(|m| m.first()).map(|&o| self.level_pos[o as usize]);
        for order in members {
            let i = order as usize;
            if let Some((cx, cy)) = c {
                let (x, y) = self.level_pos[i];
                let nx = cx + (x - cx) * rx as f64;
                let ny = cy + (y - cy) * ry as f64;
                self.set_level_pos(i, (nx, ny), 0.0);
            }
            if !only_move {
                self.scaled[i].0 *= rx;
                self.scaled[i].1 *= ry;
                let index = self.collider[i];
                if index != u32::MAX {
                    let o = &mut self.objects[index as usize];
                    o.scale.0 *= rx;
                    o.scale.1 *= ry;
                    if let Some(info) = crate::objects::info(o.id) {
                        o.shape = Shape::new(info, o.flip_x, o.flip_y, o.rotation, o.scale);
                    }
                    let (x, y) = (o.pos.0 as f32, o.pos.1 as f32);
                    (o.rect, o.obb) = o.shape.at(x, y);
                }
            }
        }
    }

    /// Moves level object `i` to `pos` (and turns it), keeping its collision shape in step.
    /// `getLastPosition` bookkeeping: the first move of an object in a step records where it was.
    /// The level object `i`'s f64 position, and moving it there (area.rs).
    pub(crate) fn level_pos_of(&self, i: usize) -> (f64, f64) {
        self.level_pos[i]
    }

    pub(crate) fn set_level_pos_pub(&mut self, i: usize, pos: (f64, f64)) {
        self.set_level_pos(i, pos, 0.0);
    }

    pub(crate) fn stamp_pub(&mut self, i: usize) {
        self.stamp(i);
    }

    pub(crate) fn group_members(&self, group: i32) -> Option<&Vec<u32>> {
        self.members.get(&group)
    }

    /// `GameObject::getUnmodifiedPosition`: the position without this step's area offset.
    pub(crate) fn unmodified_pos(&self, i: usize) -> (f32, f32) {
        let (ox, oy) = self.area.offset_of(i);
        let (x, y) = self.level_pos[i];
        ((x - ox as f64) as f32, (y - oy as f64) as f32)
    }

    /// `GJBaseGameLayer::rotateObjects` (0xfe820): each object's unmodified position orbits
    /// `center` by `angle` degrees through a pivot node's transform (cocos2d: radians =
    /// −angle·π/180, f32), and the object moves to the result plus `offset`; objects that can
    /// rotate freely turn by `angle` too.
    pub(crate) fn rotate_objects(&mut self, objs: &[u32], angle: f32, center: (f32, f32), offset: (f32, f32)) {
        let rad = -(angle * 0.017_453_292_f32);
        let (sin, cos) = (rad.sin(), rad.cos());
        for &o in objs {
            let i = o as usize;
            let p = self.unmodified_pos(i);
            let (dx, dy) = (p.0 - center.0, p.1 - center.1);
            // CCAffineTransformConcat (child at (dx, dy) under the pivot), contracted
            let tx = dx.mul_add(cos, dy * -sin) + center.0;
            let ty = dx.mul_add(sin, dy * cos) + center.1;
            let (x, y) = self.level_pos[i];
            let ny = y + ((ty + offset.1) - p.1) as f64;
            let nx = x + ((tx + offset.0) - p.0) as f64;
            let turn = if angle != 0.0 && self.can_rotate_free(i) { angle } else { 0.0 };
            self.set_level_pos(i, (nx, ny), turn);
        }
    }

    /// `tryGetMainObject`: the group's first parent object (key 274), else its only object,
    /// else none (a group of several objects without a parent has no main object).
    pub(crate) fn main_object(&self, group: i32) -> Option<u32> {
        if group <= 0 {
            return None;
        }
        if let Some(p) = self.links.parent_of(group) {
            return Some(p);
        }
        match self.members.get(&group).map(|m| m.as_slice()) {
            Some([only]) => Some(*only),
            _ => None,
        }
    }

    pub(crate) fn step_count_pub(&self) -> u64 {
        self.step_count
    }

    fn stamp(&mut self, i: usize) {
        if self.level_stamp[i] != self.step_count {
            self.level_stamp[i] = self.step_count;
            self.level_last[i] = (self.level_pos[i].0 as f32, self.level_pos[i].1 as f32);
        }
    }

    fn set_level_pos(&mut self, i: usize, pos: (f64, f64), turn: f32) {
        if self.track_moves {
            if self.moved_log.len() >= 1_000_000 {
                // not read for a long time (steps without drawing): start over instead
                self.moved_log.clear();
                self.epoch = World::next_epoch();
            }
            self.moved_log.push(i as u32);
        }
        self.stamp(i);
        self.turned[i] += turn;
        self.level_pos[i] = pos;
        self.moved[i] = (pos.0 - self.start[i].0 as f64, pos.1 - self.start[i].1 as f64);
        let index = self.collider[i];
        if index == u32::MAX {
            return;
        }
        let o = &mut self.objects[index as usize];
        if o.moved_step != self.step_count {
            o.moved_step = self.step_count;
            o.last_pos = (o.pos.0 as f32, o.pos.1 as f32);
        }
        o.pos = pos;
        if turn != 0.0 {
            o.rotation += turn;
            if let Some(info) = crate::objects::info(o.id) {
                o.shape = Shape::new(info, o.flip_x, o.flip_y, o.rotation, o.scale);
            }
        }
        let (x, y) = (pos.0 as f32, pos.1 as f32);
        (o.rect, o.obb) = o.shape.at(x, y);
        o.x = x;
        o.y = y;
        o.section = section_of(x);
    }

    /// The end of a physics step: activate the triggers the player reached.
    pub fn post_step(&mut self, player: &crate::physics::Player) {
        let dt = 1.0 / crate::physics::STEPS_PER_SECOND as f64;
        let player_rect = &player.rect();
        // Custom orbs the player fired this step (`activateCustomRing`): spawn or toggle their
        // group. (The game does it at once, inside the press or the collision pass.)
        for &i in &player.custom_fired {
            if let Some((group, on, spawn)) = self.objects[i].custom_ring {
                let action = if spawn {
                    crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }
                } else {
                    crate::groups::Action::Toggle { group, on }
                };
                let actions = self.triggers.expand_public(vec![(action, Vec::new())], &self.enabled);
                self.apply(actions);
            }
        }
        // Position triggers: the ones the player's order-channel walk fired.
        let mut actions = self.triggers.activate(&player.fired_triggers, player_rect, dt, &self.enabled);
        // Collectibles (type 30 with an item id, key 80) the player touches count once.
        for i in 0..self.collectibles.len() {
            let (order, item, delta, rect) = self.collectibles[i];
            if self.enabled[order as usize] && rect.intersects(player_rect) {
                self.enabled[order as usize] = false;
                actions.extend(self.triggers.add_to_item(item, delta));
            }
        }
        self.apply(actions);
        self.player_rect = Some(*player_rect);
        self.check_collision_blocks();
        self.check_sensors();
    }

    /// Player events this step (`GJGameEvent` ids): the armed event triggers listening for any
    /// of them spawn their groups.
    pub fn player_events(&mut self, ids: &[i32]) {
        if ids.is_empty() || self.event_links.is_empty() {
            return;
        }
        let spawns: Vec<i32> = self
            .event_links
            .iter()
            .filter(|(order, _)| self.event_lists.get(order).is_some_and(|l| l.iter().any(|e| ids.contains(e))))
            .map(|&(_, target)| target)
            .filter(|&g| g != 0)
            .collect();
        let actions: Vec<_> = spawns.into_iter().map(|group| (crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())).collect();
        if !actions.is_empty() {
            let actions = self.triggers.expand_public(actions, &self.enabled);
            self.apply(actions);
        }
    }

    /// The player died: the armed On Death triggers toggle their groups.
    pub fn player_died(&mut self) {
        for (group, on) in self.death_triggers.clone() {
            self.toggle(group, on);
        }
    }

    /// Running timers advance by the step × their rate; one reaching its stop time stops there
    /// and spawns its target; time events fire when their timer passes their time.
    fn step_timers(&mut self) {
        if self.timers.is_empty() {
            return;
        }
        let dt = self.step_secs;
        let mut spawns = Vec::new();
        for t in self.timers.values_mut() {
            if !t.2 {
                continue;
            }
            let before = t.0;
            t.0 += dt * t.1 as f64;
            if let Some(stop) = t.3 {
                let stop = stop as f64;
                let reached = if t.1 >= 0.0 { before < stop && t.0 >= stop } else { before > stop && t.0 <= stop };
                if reached {
                    t.0 = stop;
                    t.2 = false;
                    spawns.push(t.4);
                }
            }
        }
        let timers = &self.timers;
        let mut fired = Vec::new();
        self.time_events.retain(|&(id, time, target, multi)| {
            let Some(t) = timers.get(&id) else { return true };
            let (now, before) = (t.0, t.0 - dt * t.1 as f64);
            let hit = t.2 && ((before < time as f64 && now >= time as f64) || (before > time as f64 && now <= time as f64));
            if hit {
                fired.push(target);
            }
            !(hit && !multi)
        });
        spawns.extend(fired);
        let actions: Vec<_> = spawns.into_iter().filter(|&g| g != 0).map(|group| (crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())).collect();
        if !actions.is_empty() {
            let actions = self.triggers.expand_public(actions, &self.enabled);
            self.apply(actions);
        }
    }

    /// `getItemValue`: 1 an item's count, 2 a timer's seconds, 3 points (0), 4 the level
    /// time, 5 the attempt (1).
    fn item_value(&self, kind: i32, id: i32) -> f64 {
        match kind {
            1 => self.triggers.items.get(&id).copied().unwrap_or(0) as f64,
            2 => self.timers.get(&id).map_or(0.0, |t| t.0),
            4 => self.step_count as f64 * self.step_secs,
            5 => 1.0,
            _ => 0.0,
        }
    }

    /// Collision State sensors (3640): the player entering one spawns its "state on" group (51),
    /// leaving it its "state off" group (71).
    fn check_sensors(&mut self) {
        let Some(p) = self.player_rect else { return };
        let mut spawns = Vec::new();
        for k in 0..self.sensors.len() {
            let (order, half, on, off, was) = self.sensors[k];
            if !self.enabled[order] {
                continue;
            }
            let inside = self.block_rect(order, half).intersects(&p);
            if inside != was {
                self.sensors[k].4 = inside;
                spawns.push(if inside { on } else { off });
            }
        }
        let actions: Vec<_> = spawns.into_iter().filter(|&g| g != 0).map(|group| (crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())).collect();
        if !actions.is_empty() {
            let actions = self.triggers.expand_public(actions, &self.enabled);
            self.apply(actions);
        }
    }

    /// A collision block's rect now (its position ± half its scaled 30-unit size).
    fn block_rect(&self, order: usize, half: (f32, f32)) -> Rect {
        let (x, y) = self.level_pos[order];
        Rect { x: x as f32 - half.0, y: y as f32 - half.1, w: half.0 * 2.0, h: half.1 * 2.0 }
    }

    /// The block-id pairs touching now (`checkCollisionBlocks`: each dynamic block against every
    /// block with another id; the player's block counts as dynamic).
    fn touching_pairs(&self) -> std::collections::HashSet<(i32, i32)> {
        let mut out = std::collections::HashSet::new();
        let live: Vec<(i32, bool, Rect)> = self
            .collision_blocks
            .iter()
            .filter(|b| self.enabled[b.0])
            .map(|&(order, id, dynamic, half)| (id, dynamic, self.block_rect(order, half)))
            .chain(self.player_rect.map(|r| (-1, true, r)))
            .collect();
        for (i, a) in live.iter().enumerate() {
            if !a.1 {
                continue;
            }
            for (j, b) in live.iter().enumerate() {
                if i != j && a.0 != b.0 && a.2.intersects(&b.2) {
                    out.insert((a.0.min(b.0), a.0.max(b.0)));
                }
            }
        }
        out
    }

    /// Fire the armed collision triggers whose pair started (or, "on exit", stopped) touching
    /// (`handleObjectCollision` / `postCollisionCheck`).
    fn check_collision_blocks(&mut self) {
        if self.collision_triggers.is_empty() {
            return;
        }
        let now = self.touching_pairs();
        let mut actions = Vec::new();
        for &(a, b, group, on, exit) in &self.collision_triggers {
            let (was, is) = (self.colliding.contains(&(a, b)), now.contains(&(a, b)));
            if (!exit && is && !was) || (exit && was && !is) {
                actions.push((crate::groups::Action::Toggle { group, on }, Vec::new()));
            }
        }
        self.colliding = now;
        if !actions.is_empty() {
            let actions = self.triggers.expand_public(actions, &self.enabled);
            self.apply(actions);
        }
    }

    /// What `post_step` would do differently for different players (the route planner shares a
    /// world between routes with equal keys): touch triggers and collectibles touched, and the
    /// position triggers passed by x (none while sideways).
    pub fn plan_key(&self, player: &crate::physics::Player) -> (u64, u64, u64) {
        let rect = player.rect();
        let mut coll = 0u64;
        for (i, c) in self.collectibles.iter().enumerate().take(64) {
            if self.enabled[c.0 as usize] && c.3.intersects(&rect) {
                coll |= 1 << i;
            }
        }
        // The position triggers and custom orbs the player fired this step.
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&player.fired_triggers, &mut h);
        std::hash::Hash::hash(&player.custom_fired, &mut h);
        (self.triggers.touch_key(&rect), coll, std::hash::Hasher::finish(&h))
    }

    /// After the camera step (where rotate-gameplay triggers act): if the gameplay turned back
    /// from sideways, the position triggers passed meanwhile fire by x in the same step.
    /// (Kept for the drivers: the order-channel walk replaced the catch-up by x.)
    pub fn catch_up(&mut self, _player: &crate::physics::Player) {}

    fn apply(&mut self, actions: Vec<(crate::groups::Action, Vec<i32>)>) {
        for (action, origin) in actions {
            {
                match action {
                    crate::groups::Action::Move(mut spec) => {
                        // getMoveTargetDelta: target's position − center's, now (f32): their main
                        // objects (kA40: tryGetObject, a random object without one); none, or the
                        // same object: no movement
                        if let Some((target, center, axis)) = spec.target {
                            let (from, to) = if self.random_move_targets {
                                (self.try_get_object(center), self.try_get_object(target))
                            } else {
                                (self.main_object(center), self.main_object(target))
                            };
                            let (dx, dy) = match (to, from) {
                                (Some(t), Some(c)) if t != c => {
                                    let (t, c) = (self.level_pos[t as usize], self.level_pos[c as usize]);
                                    (t.0 as f32 - c.0 as f32, t.1 as f32 - c.1 as f32)
                                }
                                _ => (0.0, 0.0),
                            };
                            spec.dx = if axis == 2 { 0.0 } else { dx };
                            spec.dy = if axis == 1 { 0.0 } else { dy };
                        }
                        let mut c = crate::groups::MoveCommand::new(spec);
                        c.origin = origin;
                        self.triggers.commands.push(c);
                    }
                    crate::groups::Action::Rotate(spec) => {
                        let mut c = crate::groups::MoveCommand::rotation(spec);
                        c.origin = origin;
                        self.triggers.commands.push(c);
                    }
                    crate::groups::Action::Scale(spec) => {
                        let mut c = crate::groups::MoveCommand::scaling(spec);
                        c.origin = origin;
                        self.triggers.commands.push(c);
                    }
                    crate::groups::Action::Follow { group, followed, mod_x, mod_y, duration } => {
                        let spec = crate::groups::MoveSpec {
                            group,
                            dx: 0.0,
                            dy: 0.0,
                            duration,
                            easing: 0,
                            rate: 2.0,
                            lock_x: false,
                            lock_y: false,
                            lock_mod: (1.0, 1.0),
                            target: None,
                        };
                        let mut c = crate::groups::MoveCommand::new(spec);
                        c.follow = Some((followed, mod_x, mod_y));
                        c.origin = origin;
                        self.triggers.commands.push(c);
                    }
                    crate::groups::Action::FollowY { group, speed, delay, offset, max_speed, duration } => {
                        let spec = crate::groups::MoveSpec {
                            group,
                            dx: 0.0,
                            dy: 0.0,
                            duration,
                            easing: 0,
                            rate: 2.0,
                            lock_x: false,
                            lock_y: false,
                            lock_mod: (1.0, 1.0),
                            target: None,
                        };
                        let mut c = crate::groups::MoveCommand::new(spec);
                        c.follow_y = Some((speed, delay, offset, max_speed));
                        c.origin = origin;
                        self.triggers.commands.push(c);
                    }
                    crate::groups::Action::PlayerControl { stop_jump, stop_move, stop_rotation } => {
                        let was = self.control.unwrap_or((false, false, false));
                        self.control = Some((was.0 || stop_jump, was.1 || stop_move, was.2 || stop_rotation));
                    }
                    crate::groups::Action::Sfx(play) => self.sfx.push(play),
                    crate::groups::Action::EventLink { order, target } => {
                        if !self.event_links.contains(&(order, target)) {
                            self.event_links.push((order, target));
                        }
                    }
                    crate::groups::Action::Sequence { order, min_interval, mode } => {
                        let now = self.step_count as f64 * self.step_secs;
                        let Some((steps, at, last)) = self.sequences.get_mut(&order) else { continue };
                        if now - *last < min_interval as f64 || steps.is_empty() {
                            continue;
                        }
                        let total: usize = steps.iter().map(|s| s.1 as usize).sum();
                        if *at >= total {
                            match mode {
                                1 => *at = 0,
                                2 => *at = total - 1,
                                _ => continue,
                            }
                        }
                        // the step this activation is in
                        let mut k = *at;
                        let group = steps.iter().find(|s| {
                            if k < s.1 as usize {
                                true
                            } else {
                                k -= s.1 as usize;
                                false
                            }
                        }).map_or(0, |s| s.0);
                        *at += 1;
                        *last = now;
                        if group != 0 {
                            let more = self.triggers.expand_public(vec![(crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())], &self.enabled);
                            self.apply(more);
                        }
                    }
                    crate::groups::Action::Timer { id, start, stop, rate, target, paused, keep } => {
                        if !(keep && self.timers.get(&id).is_some_and(|t| t.2)) {
                            self.timers.insert(id, (start as f64, rate, !paused, stop, target));
                        }
                    }
                    crate::groups::Action::TimeEvent { id, time, target, multi } => {
                        if !self.time_events.contains(&(id, time, target, multi)) {
                            self.time_events.push((id, time, target, multi));
                        }
                    }
                    crate::groups::Action::TimeControl { id, stop } => {
                        if let Some(t) = self.timers.get_mut(&id) {
                            t.2 = !stop;
                        }
                    }
                    crate::groups::Action::Animate { group, .. } => {
                        let now = self.step_count as f64 * self.step_secs;
                        for &order in self.members.get(&group).map(Vec::as_slice).unwrap_or_default() {
                            self.animated_at[order as usize] = Some(now);
                        }
                    }
                    crate::groups::Action::Song { id, start_ms, volume, looped, channel, .. } => self.songs.push((id, start_ms, volume, looped, channel)),
                    crate::groups::Action::OnDeath { group, on } => {
                        if !self.death_triggers.contains(&(group, on)) {
                            self.death_triggers.push((group, on));
                        }
                    }
                    crate::groups::Action::Teleport { group, keep_x, keep_y, gravity, smooth } => {
                        if let Some((x, y)) = self.group_main_position(group) {
                            self.teleport = Some((x, y, keep_x, keep_y, gravity, smooth));
                        }
                    }
                    crate::groups::Action::ItemCompare(it) => {
                        // activateItemCompareTrigger: item 1 op mod 1 against item 2 op mod 2 (or
                        // mod 2 alone), each rounded and signed, within the tolerance
                        let a = crate::groups::item_shape(
                            crate::groups::item_op(if it.op1 > 0 { it.op1 } else { 3 }, self.item_value(it.kind1.max(1), it.id1), ((it.mod1 * 1000.0) as i32) as f64 / 1000.0),
                            it.round1,
                            it.sign1,
                        );
                        let use2 = it.kind2 != 0 && (it.id2 > 0 || matches!(it.kind2, 3 | 4));
                        let mod2 = ((it.mod2 * 1000.0) as i32) as f64 / 1000.0;
                        let b = if use2 {
                            crate::groups::item_shape(crate::groups::item_op(if it.op2 > 0 { it.op2 } else { 3 }, self.item_value(it.kind2, it.id2), mod2), it.round2, it.sign2)
                        } else {
                            crate::groups::item_shape(mod2, it.round2, it.sign2)
                        };
                        let tol = it.tolerance as f64;
                        let yes = match it.op3 {
                            0 => (a - b).abs() <= tol,
                            1 => a + tol > b,
                            2 => a + tol >= b,
                            3 => b > a - tol,
                            4 => b >= a - tol,
                            5 => (a - b).abs() > tol,
                            _ => false,
                        };
                        let group = if yes { it.target } else { it.other };
                        if group != 0 {
                            let more = self.triggers.expand_public(vec![(crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())], &self.enabled);
                            self.apply(more);
                        }
                    }
                    crate::groups::Action::ItemEdit(it) => {
                        // activateItemEditTrigger: (item 1 op item 2) op mod, assigned to the
                        // target item (only item counts are kept; timers and points read 0)
                        if it.target < 1 || it.target_kind.max(1) != 1 {
                            continue;
                        }
                        let use1 = it.kind1 != 0 && (it.id1 > 0 || matches!(it.kind1, 3 | 4));
                        let use2 = it.kind2 != 0 && (it.id2 > 0 || matches!(it.kind2, 3 | 4));
                        let (v1, v2) = (self.item_value(it.kind1, it.id1), self.item_value(it.kind2, it.id2));
                        let mod_ = ((it.mod1 * 1000.0) as i32) as f64 / 1000.0;
                        let x = match (use1, use2) {
                            (true, true) => crate::groups::item_op(it.op2.max(1), v1, v2),
                            (true, false) => v1,
                            (false, true) => v2,
                            (false, false) => 0.0,
                        };
                        let x = if use1 || use2 { crate::groups::item_op(if it.op3 >= 4 { 4 } else { 3 }, x, mod_) } else { mod_ };
                        let x = crate::groups::item_shape(x, it.round1, it.sign1);
                        let have = self.triggers.items.get(&it.target).copied().unwrap_or(0) as f64;
                        let new = match it.op1 {
                            0 => x,
                            op => crate::groups::item_op(op, have, x),
                        };
                        let new = crate::groups::item_shape(new, it.round2, it.sign2) as i32;
                        for (a, _) in self.triggers.add_to_item(it.target, new - have as i32) {
                            if let crate::groups::Action::Count { group, on, .. } = a {
                                self.toggle(group, on);
                            }
                        }
                    }
                    crate::groups::Action::Collision { a, b, group, on, exit } => {
                        let entry = (a.min(b), a.max(b), group, on, exit);
                        if !self.collision_triggers.contains(&entry) {
                            self.collision_triggers.push(entry);
                        }
                    }
                    crate::groups::Action::InstantCollision { a, b, yes, no } => {
                        let hit = self.touching_pairs().contains(&(a.min(b), a.max(b)));
                        let group = if hit { yes } else { no };
                        if group != 0 {
                            let more = self.triggers.expand_public(vec![(crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())], &self.enabled);
                            self.apply(more);
                        }
                    }
                    crate::groups::Action::Toggle { group, on } => self.toggle(group, on),
                    crate::groups::Action::Area(spec) => self.area.add(spec),
                    crate::groups::Action::End => self.level_end = true,
                    crate::groups::Action::Gravity { value } => self.gravity_mod = value,
                    crate::groups::Action::PlayerVisible { on } => self.player_hidden = !on,
                    crate::groups::Action::Shake { strength, interval, duration } => self.shake = (strength, interval, duration),
                    crate::groups::Action::Pickup { item, count, set } => {
                        let have = self.triggers.items.get(&item).copied().unwrap_or(0);
                        let delta = if set { count - have } else { count };
                        for (a, _) in self.triggers.add_to_item(item, delta) {
                            if let crate::groups::Action::Count { group, on, .. } = a {
                                self.toggle(group, on);
                            }
                        }
                    }
                    crate::groups::Action::Random { .. } => {}
                    crate::groups::Action::Effect { order } => self.fired_effects.push(order),
                    crate::groups::Action::Camera { order } => self.fired_camera.push(order),
                    crate::groups::Action::Count { item, target, group, on, .. } => {
                        // Armed; fires now if the count already matches.
                        if self.triggers.items.get(&item).copied().unwrap_or(0) == target {
                            self.toggle(group, on);
                        } else {
                            self.triggers.counts.push((action, origin));
                        }
                    }
                    crate::groups::Action::InstantCount { item, target, mode, group, on } => {
                        let count = self.triggers.items.get(&item).copied().unwrap_or(0);
                        let hit = match mode {
                            1 => count > target,
                            2 => count < target,
                            _ => count == target,
                        };
                        if hit {
                            self.toggle(group, on);
                        }
                    }
                    crate::groups::Action::TimeWarp { value } => {
                        // A changed warp ends the frame after this step: the time of its
                        // remaining steps goes back to the accumulator (f32).
                        if value != self.time_warp && self.frame_left > 0 {
                            self.warp_acc += (self.step_secs * self.frame_left as f64) as f32 as f64;
                            self.frame_left = 0;
                        }
                        self.time_warp = value;
                    }
                    crate::groups::Action::Spawn { .. }
                    | crate::groups::Action::Stop { .. }
                    | crate::groups::Action::Chance { .. }
                    | crate::groups::Action::Noop => {}
                    crate::groups::Action::AreaStop { id } => self.area.effects.retain(|e| e.effect_id != id),
                    crate::groups::Action::Reset { group } => self.reset_group(group),
                    crate::groups::Action::Persistent { item, on, all, reset, timer } => self.persistent(item, on, all, reset, timer),
                    crate::groups::Action::Progress { value, add, item, counter } => self.progress(value, add, item, counter),
                    crate::groups::Action::BgEffect { on } => self.bg_effect_off = !on,
                    crate::groups::Action::ArtSpeed { mg, x, y } => {
                        let pick = |v: f32, d: f32| if v.abs() > 999.0 { d } else { v };
                        if mg {
                            self.mg_speed = (pick(x, 0.3), pick(y, 0.5));
                        } else {
                            self.bg_speed = (pick(x, 0.1), pick(y, 0.1));
                        }
                    }
                    crate::groups::Action::AudioEdit(edit) => {
                        let triggers = if edit.triggers > 0 { self.members_of(edit.triggers).to_vec() } else { Vec::new() };
                        self.audio_edits.push((edit, triggers));
                    }
                    crate::groups::Action::Touch { group, hold, mode, only, dual } => {
                        let entry = (group, hold, mode, only, dual);
                        if !self.touch_triggers.contains(&entry) {
                            self.touch_triggers.push(entry);
                        }
                    }
                    crate::groups::Action::Reverse => self.reverse = !self.reverse,
                    crate::groups::Action::SpawnParticle { order } => self.spawn_particle(order),
                    crate::groups::Action::Checkpoint { order } => {
                        if self.platformer {
                            self.checkpoint = Some(order);
                        }
                    }
                    crate::groups::Action::Enter { order } => {
                        if let Some(spec) = self.enter_specs.get(&order).copied() {
                            self.enter.fire(&spec);
                        }
                    }
                    crate::groups::Action::EnterStop { channel, mode, effect_id } => self.enter.stop(channel, mode, effect_id),
                    crate::groups::Action::AdvFollow { order, group, follow } => self.adv_fire(order, group, follow),
                    crate::groups::Action::AdvFollowEdit { order, group, follow } => self.adv_edit(order, group, follow),
                    crate::groups::Action::KeyframeAnim { order } => self.keyframe_fire(order),
                }
            }
        }
    }

    /// Reset trigger (3618): the group's collectibles can be collected again, its breakable
    /// blocks are whole again (`removeTriggeredID` + `resetObject` on types 30 and 21; the
    /// checkpoints 2063 re-arm).
    fn reset_group(&mut self, group: i32) {
        let members = self.members.get(&group).cloned().unwrap_or_default();
        for order in members {
            if self.collectibles.iter().any(|c| c.0 == order) {
                self.enabled[order as usize] = true;
            }
            let index = self.collider[order as usize];
            if index != u32::MAX && self.objects[index as usize].kind == ObjectType::BREAKABLE {
                self.restored.push(index as usize);
                self.restore_count += 1;
            }
        }
    }

    /// UI progress trigger (3663, platformer only): 0..99, added to the old value or at least
    /// it; the counter item then holds it.
    fn progress(&mut self, value: i32, add: bool, item: Option<i32>, counter: i32) {
        if !self.platformer {
            return;
        }
        let v = match item {
            Some(id) => self.triggers.items.get(&id).copied().unwrap_or(0),
            None => value,
        }
        .clamp(0, 99);
        let old = self.platformer_progress;
        self.platformer_progress = if add { v + old } else { v.max(old) };
        if counter > 0 {
            let have = self.triggers.items.get(&counter).copied().unwrap_or(0);
            for (a, _) in self.triggers.add_to_item(counter, self.platformer_progress - have) {
                if let crate::groups::Action::Count { group, on, .. } = a {
                    self.toggle(group, on);
                }
            }
        }
    }

    /// Persistent item trigger (3641): items (or timers) whose values survive a restart.
    fn persistent(&mut self, item: i32, on: bool, all: bool, reset: bool, timer: bool) {
        if timer {
            if !all {
                if on {
                    self.persistent_timers.insert(item);
                } else {
                    self.persistent_timers.remove(&item);
                }
                if reset {
                    if let Some(t) = self.timers.get_mut(&item) {
                        t.0 = 0.0;
                    }
                }
            } else {
                if reset {
                    for t in self.timers.values_mut() {
                        t.0 = 0.0;
                    }
                }
                if !on {
                    self.persistent_timers.clear();
                }
            }
            return;
        }
        let ids: Vec<i32> = if all { self.triggers.items.keys().copied().collect() } else { vec![item] };
        if !all {
            if on {
                self.persistent_items.insert(item);
            } else {
                self.persistent_items.remove(&item);
            }
        }
        if reset {
            for id in ids {
                let have = self.triggers.items.get(&id).copied().unwrap_or(0);
                self.triggers.add_to_item(id, -have);
            }
        }
        if all && !on {
            self.persistent_items.clear();
        }
    }

    /// Spawn particle trigger (3608, `spawnParticleTrigger` 0x110b0c): at the trigger, or the
    /// position group's main object (with its rotation if 551), plus the offset; rotation and
    /// scale with their variances (`_rand()` in the game, a fixed sequence here).
    fn spawn_particle(&mut self, order: u32) {
        let Some(spec) = self.particle_specs.get(&order).copied() else { return };
        let mut state = (self.step_count as u32).wrapping_mul(0x9e37_79b9) ^ order ^ 0x5bd1_e995;
        let mut rand = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            ((state >> 8) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
        };
        let mut rotation = spec.rotation + spec.rotation_var * rand();
        let scale = spec.scale + spec.scale_var * rand();
        let mut at = spec.at;
        if spec.position_group > 0 {
            if let Some(&o) = self.members.get(&spec.position_group).and_then(|m| m.first()) {
                let p = self.level_pos[o as usize];
                at = (p.0 as f32, p.1 as f32);
                if spec.match_rotation {
                    rotation += self.turned[o as usize];
                }
            }
        }
        at.0 += spec.offset.0 + spec.offset_var.0 * rand();
        at.1 += spec.offset.1 + spec.offset_var.1 * rand();
        // spawnParticleTrigger: a scale of 0 is 1
        self.particle_spawns.push((spec.group, at, rotation, if scale == 0.0 { 1.0 } else { scale }));
    }

    /// Spawn a group now (as a spawn trigger without delay would), from outside the triggers.
    pub fn spawn_group(&mut self, group: i32) {
        let actions = self.triggers.expand_public(vec![(crate::groups::Action::Spawn { group, delay: 0.0, ordered: false, remap: 0 }, Vec::new())], &self.enabled);
        self.apply(actions);
    }

    /// A group's members by level order (the app's spawn particle copies).
    pub fn members_of(&self, group: i32) -> &[u32] {
        self.members.get(&group).map(Vec::as_slice).unwrap_or_default()
    }

    /// A player button event (`GJEffectManager::playerButton`, from `handleButton`): the
    /// registered touch triggers toggle their groups. `p2`: the second player's button (two
    /// player mode). Returns whether a dual-mode touch trigger takes player 1's presses away
    /// from the player (`hasActiveDualTouch`).
    pub fn player_button(&mut self, down: bool, p2: bool) -> bool {
        let mut dual = false;
        for (group, hold, mode, only, dual_mode) in self.touch_triggers.clone() {
            dual |= dual_mode;
            if dual_mode && p2 {
                continue;
            }
            let wanted = match only {
                1 => p2,
                2 => !p2,
                _ => true,
            };
            if !wanted || !(down || hold) {
                continue;
            }
            let enabled_now = self.members.get(&group).and_then(|m| m.first()).is_some_and(|&o| self.enabled[o as usize]);
            let on = match (hold, mode) {
                (_, 0) => !enabled_now,
                (false, m) => m == 1,
                (true, m) => (m != 1) ^ down,
            };
            self.toggle(group, on);
        }
        dual && !p2
    }

    /// Toggle a group's objects on or off.
    fn toggle(&mut self, group: i32, on: bool) {
        for &order in self.members.get(&group).map(Vec::as_slice).unwrap_or_default() {
            self.enabled[order as usize] = on;
        }
    }

    /// `GJBaseGameLayer::moveObjects`: every object of the group moves by (dx, dy) in f64.
    fn move_group(&mut self, group: i32, dx: f64, dy: f64) {
        let Some(members) = self.members.get(&group).cloned() else { return };
        for order in members {
            let (x, y) = self.level_pos[order as usize];
            self.set_level_pos(order as usize, (x + dx, y + dy), 0.0);
        }
    }

    /// The widest slope (slopes are gathered with a rect twice their size).
    pub fn max_slope_width(&self) -> f32 {
        self.max_slope_width
    }

    /// The largest collision radius of any object (how far a round hazard can reach past its
    /// rect).
    pub fn max_radius(&self) -> f32 {
        self.max_radius
    }

    /// Objects whose rect may overlap the x range `[min_x, max_x]`.
    pub fn near(&self, min_x: f32, max_x: f32) -> impl Iterator<Item = &Object> {
        self.near_indexed(min_x, max_x).map(|(_, o)| o)
    }

    /// [`World::near_indexed`] for one collider class (`collider_class`): the same objects of
    /// that class, without walking the others (dense stacks have thousands per pass).
    pub fn near_class(&self, class: usize, min_x: f32, max_x: f32) -> impl Iterator<Item = (usize, &Object)> {
        let lo = min_x - self.max_width;
        let list = &self.by_class[class];
        let start = list.partition_point(|&i| self.objects[i as usize].rect.x < lo);
        let window = list[start..].iter().map(move |&i| (i as usize, &self.objects[i as usize])).take_while(move |(_, o)| o.rect.x <= max_x);
        let moving = self.objects[self.first_dynamic..]
            .iter()
            .enumerate()
            .map(|(i, o)| (self.first_dynamic + i, o))
            .filter(move |(_, o)| collider_class(o.kind) == Some(class) && o.rect.x >= lo && o.rect.x <= max_x);
        window.chain(moving).filter(|(_, o)| self.enabled.get(o.order as usize).copied().unwrap_or(true))
    }

    /// Like [`World::near`], with each object's index in `objects`.
    pub fn near_indexed(&self, min_x: f32, max_x: f32) -> impl Iterator<Item = (usize, &Object)> {
        let lo = min_x - self.max_width;
        let fixed = &self.objects[..self.first_dynamic];
        let start = fixed.partition_point(|o| o.rect.x < lo);
        let window = fixed[start..]
            .iter()
            .enumerate()
            .map(move |(i, o)| (start + i, o))
            .take_while(move |(_, o)| o.rect.x <= max_x);
        let moving = self.objects[self.first_dynamic..]
            .iter()
            .enumerate()
            .map(|(i, o)| (self.first_dynamic + i, o))
            .filter(move |(_, o)| o.rect.x >= lo && o.rect.x <= max_x);
        window.chain(moving).filter(|(_, o)| self.enabled.get(o.order as usize).copied().unwrap_or(true))
    }
}

/// An object's hitbox relative to its position (`GameObject::getObjectRect`): a `w`×`h` box
/// (the object's hitbox scaled by its scale) around the position plus `center` (nonzero for a
/// few off-center hitboxes), turned by quarter turns, or by any angle as an oriented box
/// (`OBB2D`, corners computed in absolute f32 coordinates like the game) whose bounding box is
/// the rect. Round hazards keep an unturned box. Checked against the real game's rects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub center: (f32, f32),
    pub w: f32,
    pub h: f32,
    /// Degrees (clockwise) for oriented boxes; `None` for axis-aligned ones.
    pub turn: Option<f32>,
}

impl Shape {
    pub fn new(info: &objects::ObjectInfo, flip_x: bool, flip_y: bool, rotation: f32, (sx, sy): (f32, f32)) -> Shape {
        let (sx, sy) = (sx.abs(), sy.abs());
        let (w, h) = (info.rect_w * sx, info.rect_h * sy);
        // The table's offsets were measured from real rects; centers sit on half units.
        let snap = |c: f32| (c * 2.0).round() * 0.5;
        let (mut cx, mut cy) = (snap(info.rect_x + info.rect_w * 0.5) * sx, snap(info.rect_y + info.rect_h * 0.5) * sy);
        if flip_x {
            cx = -cx;
        }
        if flip_y {
            cy = -cy;
        }
        if info.radius > 0.0 || (rotation as i32) % 90 == 0 {
            // Round hazards ignore other angles, but quarter turns still swap their box.
            if info.radius > 0.0 && (rotation as i32) % 90 != 0 {
                return Shape { center: (cx, cy), w, h, turn: None };
            }
            let (cx, cy, w, h) = match (rotation.round() as i32).rem_euclid(360) {
                90 => (cy, -cx, h, w),
                180 => (-cx, -cy, w, h),
                270 => (-cy, cx, h, w),
                _ => (cx, cy, w, h),
            };
            return Shape { center: (cx, cy), w, h, turn: None };
        }
        let a = (-rotation as f64).to_radians();
        let (sin, cos) = a.sin_cos();
        let (cx, cy) = (cx as f64, cy as f64);
        let center = ((cx * cos - cy * sin) as f32, (cx * sin + cy * cos) as f32);
        Shape { center, w, h, turn: Some(rotation) }
    }

    /// The rect (and oriented box) with the object at (x, y).
    pub fn at(&self, x: f32, y: f32) -> (Rect, Option<[(f32, f32); 4]>) {
        let (cx, cy) = (x + self.center.0, y + self.center.1);
        let Some(rotation) = self.turn else {
            return (Rect { x: cx - self.w * 0.5, y: cy - self.h * 0.5, w: self.w, h: self.h }, None);
        };
        let obb = oriented_box(cx, cy, self.w, self.h, rotation);
        let (min_x, max_x) = obb.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
        let (min_y, max_y) = obb.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        (Rect { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y }, Some(obb))
    }
}

/// Flip, then rotate (multiples of 90°, clockwise positive like cocos2d) a rect given relative
/// to the object's position. Verified against the game's own rects for all 1,401 collidable
/// objects of Stereo Madness.
pub fn transform_rect(
    (mut dx, mut dy, mut w, mut h): (f32, f32, f32, f32),
    flip_x: bool,
    flip_y: bool,
    rotation: f32,
) -> (f32, f32, f32, f32) {
    if flip_x {
        dx = -(dx + w);
    }
    if flip_y {
        dy = -(dy + h);
    }
    match (rotation.round() as i32).rem_euclid(360) {
        90 => (dx, dy, w, h) = (dy, -(dx + w), h, w),
        180 => (dx, dy) = (-(dx + w), -(dy + h)),
        270 => (dx, dy, w, h) = (-(dy + h), dx, h, w),
        _ => {}
    }
    (dx, dy, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(world: &mut World, a: crate::groups::Action) {
        world.apply(vec![(a, Vec::new())]);
    }

    #[test]
    fn small_triggers() {
        use crate::groups::Action;
        let level = LevelString::parse("kA13,0,kA22,1;1,1,2,300,3,105,57,5;1,1614,2,400,3,105,57,6,80,3;1,3006,2,0,3,0,51,5,225,4;1,5,2,700,3,105,57,7;1,3618,2,0,3,0,51,6");
        let mut w = World::from_level(&level);
        // area stop: the area effects with that effect id go
        act(&mut w, Action::Area(crate::area::AreaSpec::from_raw(&level.objects[2], 2)));
        assert_eq!(w.area.effects.len(), 1);
        act(&mut w, Action::AreaStop { id: 3 });
        assert_eq!(w.area.effects.len(), 1);
        act(&mut w, Action::AreaStop { id: 4 });
        assert!(w.area.effects.is_empty());
        // BG effect off survives a restart; BG / MG speed (|v| > 999: the default)
        act(&mut w, Action::BgEffect { on: false });
        act(&mut w, Action::ArtSpeed { mg: false, x: 0.5, y: 1000.0 });
        assert_eq!(w.bg_speed, (0.5, 0.1));
        // persistent items keep their count across restarts
        act(&mut w, Action::Pickup { item: 3, count: 4, set: true });
        act(&mut w, Action::Persistent { item: 3, on: true, all: false, reset: false, timer: false });
        act(&mut w, Action::Pickup { item: 9, count: 2, set: true });
        w.reset();
        assert!(w.bg_effect_off);
        assert_eq!(w.bg_speed, (0.1, 0.1));
        assert_eq!(w.triggers.items.get(&3), Some(&4));
        assert_eq!(w.triggers.items.get(&9), None);
        // reset trigger: a taken collectible can be taken again
        w.enabled[1] = false;
        act(&mut w, Action::Reset { group: 6 });
        assert!(w.enabled[1]);
        // progress (platformer): at least the old value unless adding, 0..99
        act(&mut w, Action::Progress { value: 40, add: false, item: None, counter: 0 });
        act(&mut w, Action::Progress { value: 20, add: false, item: None, counter: 0 });
        assert_eq!(w.platformer_progress, 40);
        act(&mut w, Action::Progress { value: 150, add: true, item: None, counter: 0 });
        assert_eq!(w.platformer_progress, 139);
        // reverse: pending until the next step starts
        act(&mut w, Action::Reverse);
        assert!(w.reverse_pending());
        w.pre_step((0.0, 0.0));
        assert!(!w.reverse_pending());
    }

    #[test]
    fn touch_triggers_toggle_on_presses() {
        use crate::groups::Action;
        let level = LevelString::parse("kA13,0;1,5,2,300,3,105,57,5;1,1595,2,0,3,0,51,5;1,1049,2,0,3,0,51,5");
        let mut w = World::from_level(&level);
        // flip on each press
        act(&mut w, Action::Touch { group: 5, hold: false, mode: 0, only: 0, dual: false });
        assert!(!w.player_button(true, false));
        assert!(!w.enabled[0]);
        w.player_button(false, false);
        assert!(!w.enabled[0]);
        w.player_button(true, false);
        assert!(w.enabled[0]);
        // hold mode "on": on while held, off on release; player-2-only ones ignore player 1
        let mut w = World::from_level(&level);
        act(&mut w, Action::Touch { group: 5, hold: true, mode: 1, only: 1, dual: false });
        w.player_button(true, false);
        assert!(w.enabled[0]);
        w.player_button(true, true);
        assert!(w.enabled[0]);
        w.player_button(false, true);
        assert!(!w.enabled[0]);
        // dual mode keeps player 1's presses from the player
        let mut w = World::from_level(&level);
        act(&mut w, Action::Touch { group: 5, hold: false, mode: 2, only: 0, dual: true });
        assert!(w.player_button(true, false));
        assert!(!w.enabled[0]);
    }

    #[test]
    fn edit_sfx_names_the_sfx_triggers_of_its_group() {
        // SFX trigger (order 0) in group 9 with speed 5 and a proximity to group 4; an edit SFX
        // trigger (order 1) for group 9's SFX triggers
        let level = LevelString::parse("kA13,0;1,3602,2,0,3,0,57,9,392,7,404,5,51,4,138,1;1,3603,2,0,3,0,457,9,419,1,404,-3;1,5,2,300,3,105,57,4");
        let mut w = World::from_level(&level);
        let sfx = crate::groups::SfxPlay::from_raw(&level.objects[0], 0);
        assert_eq!((sfx.id, sfx.speed, sfx.proximity.map(|p| p.group)), (7, 5, Some(4)));
        let edit = match crate::groups::Trigger::parse(&level.objects[1], 1).unwrap().action {
            crate::groups::Action::AudioEdit(e) => e,
            a => panic!("{a:?}"),
        };
        act(&mut w, crate::groups::Action::Sfx(sfx));
        act(&mut w, crate::groups::Action::AudioEdit(edit));
        assert_eq!(w.sfx.len(), 1);
        assert_eq!(w.audio_edits, vec![(edit, vec![0])]);
        assert_eq!((edit.triggers, edit.speed), (9, Some(-3)));
        // the proximity group is known to the world
        assert_eq!(w.members_of(4), &[2]);
    }

    #[test]
    fn random_trigger_spawns_one_of_two() {
        let level = LevelString::parse("kA13,0;1,1912,2,0,3,0,51,5,71,6,10,100;1,1049,2,0,3,0,51,7,62,1,57,5;1,5,2,300,3,105,57,7");
        let mut w = World::from_level(&level);
        // chance 100: always group 5, whose spawn-triggered toggle turns group 7 off
        let fired = w.triggers.expand_public(vec![(crate::groups::Action::Chance { a: 5, b: 6, chance: 100.0 }, Vec::new())], &w.enabled.clone());
        w.apply(fired);
        assert!(!w.enabled[2]);
    }

    #[test]
    fn first_spike_of_stereo_madness() {
        let level = LevelString::parse("kA13,0;1,8,2,525,3,15;1,1,2,0,3,0,57,1;");
        let world = World::from_level(&level);
        let spike = world.objects.iter().find(|o| o.id == 8).unwrap();
        assert_eq!(spike.rect, Rect { x: 522.0, y: 99.0, w: 6.0, h: 12.0 });
    }

    #[test]
    fn follow_player_y_eases_toward_the_player() {
        // block 1 in group 5 at file y 105; follow player y with speed 0.5, max speed 0 (no cap)
        let level = LevelString::parse("kA13,0;1,1,2,300,3,105,57,5;1,1814,2,900,3,0,51,5,90,0.5,92,30,10,1;");
        let mut world = World::from_level(&level);
        let order = world.members[&5][0] as usize;
        let action = crate::groups::Action::FollowY { group: 5, speed: 0.5, delay: 0.0, offset: 30, max_speed: 0.0, duration: 1.0 };
        world.apply(vec![(action, Vec::new())]);
        world.pre_step((0.0, 305.0));
        // positions are in world space (the file's y + 90, like the player's): a 0.5·dt·60
        // share of the gap 305 + 30 − 195 = 140
        let share = (0.5 * world.step_secs as f32 as f64 * 60.0) as f32 as f64;
        assert!((world.level_pos[order].1 - (195.0 + share * 140.0)).abs() < 1e-3, "{:?}", world.level_pos[order]);
        for _ in 0..2000 {
            world.pre_step((0.0, 305.0));
        }
        // the command ended after its second, close to the target
        assert!(world.triggers.commands.is_empty());
        assert!((world.level_pos[order].1 - 335.0).abs() < 0.5, "{:?}", world.level_pos[order]);
    }

    #[test]
    fn collision_trigger_toggles_on_player_touch() {
        // collision block 5 at (300, 105 + 90); block in group 9; trigger: block 5 vs player 1
        // toggles group 9 off on enter
        let level = LevelString::parse("kA13,0;1,1816,2,300,3,105,80,5;1,1,2,600,3,105,57,9;1,1815,2,0,3,0,80,5,138,1,51,9;");
        let mut world = World::from_level(&level);
        let block = world.members[&9][0] as usize;
        let trigger = crate::groups::Action::Collision { a: 5, b: -1, group: 9, on: false, exit: false };
        world.apply(vec![(trigger, Vec::new())]);
        world.player_rect = Some(Rect { x: 200.0, y: 180.0, w: 30.0, h: 30.0 });
        world.check_collision_blocks();
        assert!(world.enabled[block]);
        world.player_rect = Some(Rect { x: 280.0, y: 180.0, w: 30.0, h: 30.0 });
        world.check_collision_blocks();
        assert!(!world.enabled[block]);
        // instant collision: touching now spawns the true group
        assert!(world.touching_pairs().contains(&(-1, 5)));
    }

    #[test]
    fn item_edit_and_compare() {
        let level = LevelString::parse("kA13,0;1,1,2,600,3,105,57,9;1,1049,2,0,3,0,51,9;");
        let mut world = World::from_level(&level);
        let spec = |s: &str| crate::groups::ItemSpec::from_raw(&LevelString::parse(&format!("kA13,0;{s};")).objects[0]);
        // no items: item 3 = mod (7)
        world.apply(vec![(crate::groups::Action::ItemEdit(spec("1,3619,51,3,479,7,480,0")), Vec::new())]);
        assert_eq!(world.triggers.items.get(&3), Some(&7));
        // item 4 = item 3 × 2 (kind 1, op with mod ×)
        world.apply(vec![(crate::groups::Action::ItemEdit(spec("1,3619,80,3,476,1,51,4,479,2,482,3")), Vec::new())]);
        assert_eq!(world.triggers.items.get(&4), Some(&14));
        // compare item 4 > 10: the fields land where activateItemCompareTrigger reads them
        let it = spec("1,3620,80,4,476,1,484,10,482,1,51,0,71,0");
        assert_eq!((it.id1, it.kind1, it.mod2, it.op3), (4, 1, 10.0, 1));
    }

    #[test]
    fn timers_stop_and_fire_events() {
        // timer 3 from 0, stop at 1 s spawning group 0; a time event at 0.5 s toggles nothing
        let level = LevelString::parse("kA13,0;1,1,2,600,3,105,57,9;");
        let mut world = World::from_level(&level);
        let timer = crate::groups::Action::Timer { id: 3, start: 0.0, stop: Some(1.0), rate: 1.0, target: 0, paused: false, keep: false };
        world.apply(vec![(timer, Vec::new())]);
        world.apply(vec![(crate::groups::Action::TimeEvent { id: 3, time: 0.5, target: 0, multi: false }, Vec::new())]);
        for _ in 0..130 {
            world.pre_step((0.0, 105.0));
        }
        let t = world.timers[&3];
        assert!(t.0 > 0.4 && t.2, "{t:?}");
        assert!(world.time_events.is_empty());
        for _ in 0..400 {
            world.pre_step((0.0, 105.0));
        }
        assert_eq!((world.timers[&3].0, world.timers[&3].2), (1.0, false));
        assert_eq!(world.item_value(2, 3), 1.0);
    }

    #[test]
    fn sequence_steps_through_its_groups() {
        // sequence: group 5 twice, then group 6 once; stop mode. Group 5/6 hold toggle
        // triggers that switch groups 7/8 off, so each spawn is visible.
        let level = LevelString::parse("kA13,0;1,3607,2,0,3,0,435,5.2.6.1,436,0;1,1,2,600,3,105,57,7;1,1,2,630,3,105,57,8;1,1049,2,0,3,0,51,7,57,5,62,1,87,1;1,1049,2,0,3,0,51,8,57,6,62,1,87,1;");
        let mut world = World::from_level(&level);
        let fire = |w: &mut World| {
            let a = crate::groups::Action::Sequence { order: 0, min_interval: 0.0, mode: 0 };
            w.apply(vec![(a, Vec::new())]);
            w.pre_step((0.0, 105.0));
        };
        fire(&mut world);
        assert_eq!(world.sequences[&0].1, 1);
        fire(&mut world);
        fire(&mut world);
        assert_eq!(world.sequences[&0].1, 3);
        // the end in stop mode: nothing more
        fire(&mut world);
        assert_eq!(world.sequences[&0].1, 3);
    }

    #[test]
    fn rotations_match_cocos() {
        // Block 2 rotated -90 at (3345,105) has rect origin (3330,90) in the game.
        assert_eq!(transform_rect((-15.0, -15.0, 30.0, 30.0), false, false, -90.0), (-15.0, -15.0, 30.0, 30.0));
        // An upside-down spike: the 6x12 box moves to the top half.
        assert_eq!(transform_rect((-3.0, -6.0, 6.0, 12.0), false, true, 0.0), (-3.0, -6.0, 6.0, 12.0));
        assert_eq!(transform_rect((-3.0, -10.0, 6.0, 12.0), false, true, 0.0), (-3.0, -2.0, 6.0, 12.0));
        assert_eq!(transform_rect((-3.0, -10.0, 6.0, 12.0), false, false, 90.0), (-10.0, -3.0, 12.0, 6.0));
    }

    #[test]
    fn time_warp_runs_more_steps_per_frame() {
        let mut world = World::from_level(&LevelString::parse("kA13,0;1,1,2,0,3,0;"));
        // A frame of 1/60 s runs 4 steps normally and 8 at half speed, so on screen each step
        // stands for a quarter or an eighth of the frame.
        let steps_in_frame = |world: &mut World| {
            let secs = world.real_step_secs();
            world.pre_step((0.0, 0.0));
            let mut n = 1;
            while world.frame_left > 0 {
                assert_eq!(world.real_step_secs(), secs);
                world.pre_step((0.0, 0.0));
                n += 1;
            }
            (n, secs)
        };
        assert_eq!(steps_in_frame(&mut world), (4, 1.0 / 240.0));
        world.time_warp = 0.5;
        // The slower frame can't start until the 4-step frame in progress ends (it has).
        let (n, secs) = steps_in_frame(&mut world);
        assert_eq!((n, (secs * 480.0).round()), (8, 1.0));
    }
}

/// The speed factor a speed portal sets (ids 200 slow, 201 normal, 202 fast, 203 faster,
/// 1334 fastest).
pub fn speed_portal_factor(id: i32) -> Option<f32> {
    Some(match id {
        200 => 0.7,
        201 => 0.9,
        202 => 1.1,
        203 => 1.3,
        1334 => 1.6,
        _ => return None,
    })
}

/// `GameObject::determineSlopeDirection`: a slope's direction (0–7) and its uphill flag from
/// its flips and rotation (whole degrees mod 360). Cases the game leaves unhandled keep the
/// defaults (0, not uphill). Directions 1, 3, 5, 6 have the floor on top (`slopeFloorTop`);
/// 2, 3, 4, 6 have the wall on the left (`slopeWallLeft`).
pub fn slope_direction(flip_x: bool, flip_y: bool, rotation: f32) -> (u8, bool) {
    let r = (rotation as i32) % 360;
    let abs180 = (r as f32).abs() == 180.0;
    let rotated_or_flip_y = flip_y || r != 0;
    if !flip_x && !rotated_or_flip_y {
        return (0, true);
    }
    if flip_x && flip_y && abs180 {
        return (0, true);
    }
    let flip_y_unrotated = r == 0 && flip_y;
    if (!flip_x && flip_y_unrotated) || (flip_x && !flip_y && abs180) {
        return (1, false);
    }
    if (flip_x && !rotated_or_flip_y) || (!flip_x && flip_y && abs180) {
        return (2, false);
    }
    if (flip_x && flip_y_unrotated) || (!flip_x && !flip_y && abs180) {
        return (3, true);
    }
    let either = flip_x || flip_y;
    let both = flip_x && flip_y;
    if (!either && (r == -270 || r == 90)) || (both && (r == -90 || r == 270)) {
        return (4, false);
    }
    if (!either && (r == -90 || r == 270)) || (both && (r == -270 || r == 90)) {
        return (5, false);
    }
    // One flip with a quarter turn.
    match (flip_x, flip_y, r) {
        (true, false, 90 | -270) | (false, true, -90 | 270) => (6, true),
        (true, false, 270 | -90) | (false, true, -270 | 90) => (7, true),
        _ => (0, false),
    }
}

/// `slopeFloorTop`: the floor is the slope's top surface.
pub fn slope_floor_top(direction: u8) -> bool {
    direction < 7 && (0x6a >> direction) & 1 == 1
}

/// `slopeWallLeft`.
pub fn slope_wall_left(direction: u8) -> bool {
    direction < 7 && (0x5c >> direction) & 1 == 1
}

/// `OBB2D::overlaps1Way` both ways: a separating-axis test between two oriented boxes (corners
/// in order around the box).
pub fn obb_overlaps(a: &[(f32, f32); 4], b: &[(f32, f32); 4]) -> bool {
    let axes = [
        (a[1].0 - a[0].0, a[1].1 - a[0].1),
        (a[3].0 - a[0].0, a[3].1 - a[0].1),
        (b[1].0 - b[0].0, b[1].1 - b[0].1),
        (b[3].0 - b[0].0, b[3].1 - b[0].1),
    ];
    axes.iter().all(|&(ax, ay)| {
        let proj = |p: &(f32, f32)| p.0 * ax + p.1 * ay;
        let (a_lo, a_hi) = a.iter().map(proj).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let (b_lo, b_hi) = b.iter().map(proj).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        a_lo <= b_hi && b_lo <= a_hi
    })
}

/// The corners of a `w`×`h` box centered at (cx, cy), turned `rotation` degrees clockwise
/// (cocos2d), like `OBB2D::create(w, h, -rotation in radians, center)`.
pub fn oriented_box(cx: f32, cy: f32, w: f32, h: f32, rotation: f32) -> [(f32, f32); 4] {
    // OBB2D::calculateWithCenter: half-size axes, then corners center ± X ± Y in f32.
    let a = (-rotation as f64).to_radians();
    let (sin, cos) = (a.sin() as f32, a.cos() as f32);
    let (hw, hh) = (w * 0.5, h * 0.5);
    let (xx, xy) = (cos * hw, sin * hw);
    let (yx, yy) = (-sin * hh, cos * hh);
    [
        (cx - xx - yx, cy - xy - yy),
        (cx + xx - yx, cy + xy - yy),
        (cx + xx + yx, cy + xy + yy),
        (cx - xx + yx, cy - xy + yy),
    ]
}

/// The collision passes' classes: 0 the special objects (orbs, pads, portals, slopes, special
/// blocks), 1 solids and breakable blocks, 2 hazards; others take no part.
pub fn collider_class(kind: ObjectType) -> Option<usize> {
    match kind {
        ObjectType::YELLOW_RING
        | ObjectType::PINK_RING
        | ObjectType::GRAVITY_RING
        | ObjectType::GREEN_RING
        | ObjectType::RED_RING
        | ObjectType::SHIP_PORTAL
        | ObjectType::CUBE_PORTAL
        | ObjectType::BALL_PORTAL
        | ObjectType::UFO_PORTAL
        | ObjectType::WAVE_PORTAL
        | ObjectType::ROBOT_PORTAL
        | ObjectType::SPIDER_PORTAL
        | ObjectType::SWING_PORTAL
        | ObjectType::RED_PAD
        | ObjectType::DASH_RING
        | ObjectType::PINK_DASH_RING
        | ObjectType::BLACK_RING
        | ObjectType::SPIDER_RING
        | ObjectType::CUSTOM_RING
        | ObjectType::SPECIAL_BLOCK
        | ObjectType::NORMAL_SIZE_PORTAL
        | ObjectType::MINI_PORTAL
        | ObjectType::SPEED_PORTAL
        | ObjectType::MIRROR_PORTAL
        | ObjectType::MIRROR_OFF_PORTAL
        | ObjectType::INVERSE_GRAVITY_PORTAL
        | ObjectType::NORMAL_GRAVITY_PORTAL
        | ObjectType::DUAL_PORTAL
        | ObjectType::SOLO_PORTAL
        | ObjectType::TELEPORT_PORTAL
        | ObjectType::YELLOW_PAD
        | ObjectType::PINK_PAD
        | ObjectType::GRAVITY_PAD
        | ObjectType::SLOPE => Some(0),
        ObjectType::SOLID | ObjectType::BREAKABLE => Some(1),
        ObjectType::HAZARD => Some(2),
        _ => None,
    }
}

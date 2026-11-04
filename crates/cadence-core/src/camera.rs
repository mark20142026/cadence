//! The gameplay camera (`GJBaseGameLayer::updateCamera`, basic modes), stepped once per physics
//! step after collisions and trigger activation. Verified against the camera position recorded
//! in real-game traces.
//!
//! Besides the picture, the camera feeds the physics: in a flying/ball/spider corridor whose
//! ground isn't the camera's own static view (a static camera trigger was active when the
//! corridor opened), the floor, the ceiling and the spider's jump limits are read from the
//! sliding ground layers' node positions through the camera (`updateCameraBGArt`,
//! `getMinPortalY`/`getMaxPortalY`, `checkCollisions`). With zoom the corridor edges scale too.

use std::sync::Arc;

use crate::level::LevelString;
use crate::physics::Player;

/// Height of the visible area in game units; the width follows the window's aspect ratio.
pub const VIEW_HEIGHT: f32 = 320.0;

/// A camera trigger fired this step, with its target resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraAction {
    /// Zoom (1913, key 371 zoom, 10 duration, 30 easing, 85 rate): `updateZoom`, clamped to
    /// [0.4, 3].
    Zoom { zoom: f32, duration: f32, easing: i32, rate: f32 },
    /// Static camera (1914, key 71 target group): the view's center moves to the target's
    /// position over the duration (`updateStaticCameraPos`); key 101: 1 = only x, 2 = only y.
    /// Key 212 follows the target group (its main object, by level order) with key 213's
    /// easing; key 453 "smooth velocity" moves along a bezier from the camera's velocity (key
    /// 454 the modifier).
    Static { target: (f32, f32), duration: f32, easing: i32, rate: f32, x: bool, y: bool, follow: Option<u32>, follow_easing: f32, smooth: bool, smooth_mod: f32 },
    /// Static camera with "exit" (1914 key 110): `exitStaticCamera` on x and/or y; the view
    /// returns to following over the duration (an offset that eases out, or a bezier with
    /// smooth velocity).
    ExitStatic { x: bool, y: bool, duration: f32, easing: i32, rate: f32, smooth: bool, smooth_mod: f32 },
    /// Camera offset (1916, keys 28/29, axis 101): `updateCameraOffsetX/Y` tween the offset.
    Offset { x: Option<f32>, y: Option<f32>, duration: f32, easing: i32, rate: f32 },
    /// Camera rotate (2015, key 68 degrees, 70 add to the current angle, 394 snap the current
    /// angle into [0, 360) first): tweens `m_cameraAngle`; the world is drawn turned about the
    /// view's center.
    Rotate { degrees: f32, add: bool, snap: bool, duration: f32, easing: i32, rate: f32 },
    /// Change BG / ground / MG (3029 / 3030 / 3031, key 533 the art id): the level's background,
    /// ground or middle ground art switches at once. `kind` 0 background, 1 ground, 2 MG.
    ChangeArt { kind: usize, id: i32 },
    /// Middle ground (2999, key 29 offset y, 10 duration, 30 easing, 85 rate):
    /// `updateMGOffsetY` tweens the middle ground's y offset (+0x2bc).
    MgOffset { y: f32, duration: f32, easing: i32, rate: f32 },
    /// Rotate gameplay (2900): applied to the player (`Player::rotate_gameplay`).
    RotateGameplay { dir: i32, dir2: i32, edit: bool, over: bool, mod_x: f32, mod_y: f32 },
}

/// A camera trigger as placed in the level.
#[derive(Clone, Debug, PartialEq)]
struct CameraTrigger {
    x: f32,
    /// y in level units (raw y + the ground offset), for the sideways forward axis.
    y: f32,
    order: u32,
    kind: TriggerKind,
}

#[derive(Clone, Debug, PartialEq)]
enum TriggerKind {
    Zoom { zoom: f32, duration: f32, easing: i32, rate: f32 },
    MgOffset { y: f32, duration: f32, easing: i32, rate: f32 },
    ChangeArt { kind: usize, id: i32 },
    Rotate { degrees: f32, add: bool, snap: bool, duration: f32, easing: i32, rate: f32 },
    /// The target group's main object (its first object by level order).
    Static { target_order: Option<u32>, duration: f32, easing: i32, rate: f32, x: bool, y: bool, follow: bool, follow_easing: f32, smooth: bool, smooth_mod: f32 },
    ExitStatic { x: bool, y: bool, duration: f32, easing: i32, rate: f32, smooth: bool, smooth_mod: f32 },
    Offset { x: Option<f32>, y: Option<f32>, duration: f32, easing: i32, rate: f32 },
    /// 2900: key 166 direction, 167 second direction; channels (inferred from Dash's chains):
    /// it fires only while the current channel is its key 170 (default 0) and then switches the
    /// channel to its key 173 when set; 582 edits the velocity (583 the new y velocity's factor).
    /// `keep` (key 169): without an edit, the velocity isn't carried over (it becomes 0; real
    /// s9-22 at x 6735).
    /// Key 171 (+0x744) switches the order channel to key 173 (+0x748, default 0); key 172
    /// (+0x745) only switches the channel.
    RotateGameplay { dir: i32, dir2: i32, change: bool, rotate: bool, target: i32, edit: bool, keep: bool, mod_x: f32, mod_y: f32 },
}

/// The level's camera and rotate-gameplay triggers, activated along the player's forward axis
/// like the world's triggers (x; y while sideways; crossing backward when going left), once
/// each. Touch-triggered ones (key 11) are kept apart in `touch` and fire when a player's
/// rect meets their scaled 30×30 square (as the world's touch triggers); spawn-triggered ones
/// aren't handled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraTriggers {
    list: Arc<Vec<CameraTrigger>>,
    /// Touch-triggered camera triggers and the square a player touches them in.
    touch: Arc<Vec<(crate::world::Rect, CameraTrigger)>>,
    /// Every position trigger of the level in its order channel (`orderSpawnObjects`).
    pub order: Arc<crate::order::OrderLists>,
}

impl CameraTriggers {
    pub fn from_level(level: &LevelString) -> CameraTriggers {
        let mut first_in_group: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
        for (order, raw) in level.objects.iter().enumerate() {
            for g in raw.groups() {
                first_in_group.entry(g).or_insert(order as u32);
            }
        }
        let mut list = Vec::new();
        let mut touch = Vec::new();
        for (order, raw) in level.objects.iter().enumerate() {
            if raw.get(62) == Some("1") {
                continue;
            }
            let duration = raw.get_f32(10).unwrap_or(0.0);
            let easing = raw.get_i32(30).unwrap_or(0);
            // customObjectSetup: a rate of 0 or less is 2.
            let rate = raw.get_f32(85).filter(|&r| r > 0.0).unwrap_or(2.0);
            let smooth = raw.get(453).is_some_and(|v| v.parse::<i32>().unwrap_or(0) != 0);
            let smooth_mod = raw.get_f32(454).unwrap_or(0.0);
            let kind = match raw.id() {
                2900 => TriggerKind::RotateGameplay {
                    dir: raw.get_i32(166).unwrap_or(0),
                    dir2: raw.get_i32(167).unwrap_or(0),
                    change: raw.get(171) == Some("1"),
                    rotate: raw.get(172) != Some("1"),
                    target: raw.get_i32(173).unwrap_or(0),
                    edit: raw.get(582) == Some("1"),
                    keep: raw.get(169) == Some("1"),
                    mod_x: raw.get_f32(584).unwrap_or(1.0),
                    mod_y: raw.get_f32(583).unwrap_or(1.0),
                },
                3029..=3031 => TriggerKind::ChangeArt { kind: (raw.id() - 3029) as usize, id: raw.get_i32(533).unwrap_or(0) },
                2999 => TriggerKind::MgOffset { y: raw.get_f32(29).unwrap_or(0.0), duration, easing, rate },
                1913 => TriggerKind::Zoom { zoom: raw.get_f32(371).unwrap_or(1.0), duration, easing, rate },
                2015 => TriggerKind::Rotate { degrees: raw.get_f32(68).unwrap_or(0.0), add: raw.get(70) == Some("1"), snap: raw.get(394) == Some("1"), duration, easing, rate },
                1914 if raw.get(110) == Some("1") => {
                    let axis = raw.get_i32(101).unwrap_or(0);
                    TriggerKind::ExitStatic { x: axis != 2, y: axis != 1, duration, easing, rate, smooth, smooth_mod }
                }
                1914 => {
                    let axis = raw.get_i32(101).unwrap_or(0);
                    let target_order = raw.get_i32(71).and_then(|g| first_in_group.get(&g).copied());
                    let follow = raw.get(212).is_some_and(|v| v.parse::<i32>().unwrap_or(0) != 0);
                    let follow_easing = raw.get_f32(213).unwrap_or(0.0);
                    TriggerKind::Static { target_order, duration, easing, rate, x: axis != 2, y: axis != 1, follow, follow_easing, smooth, smooth_mod }
                }
                1916 => {
                    let axis = raw.get_i32(101).unwrap_or(0);
                    let (ox, oy) = (raw.get_f32(28).unwrap_or(0.0), raw.get_f32(29).unwrap_or(0.0));
                    TriggerKind::Offset { x: (axis != 2).then_some(ox), y: (axis != 1).then_some(oy), duration, easing, rate }
                }
                _ => continue,
            };
            let t = CameraTrigger { x: raw.x(), y: raw.y() + crate::world::GROUND_OFFSET, order: order as u32, kind };
            if raw.get(11) == Some("1") {
                // The world's touch square (`Trigger::touch_rect`): 30×30 by the scale.
                let s = raw.get_f32(32).unwrap_or(1.0);
                let (w, h) = (30.0 * raw.get_f32(128).unwrap_or(s).abs(), 30.0 * raw.get_f32(129).unwrap_or(s).abs());
                touch.push((crate::world::Rect { x: t.x - w * 0.5, y: t.y - h * 0.5, w, h }, t));
            } else {
                list.push(t);
            }
        }
        list.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.order.cmp(&b.order)));
        let camera_orders: std::collections::HashMap<u32, usize> = list.iter().enumerate().map(|(i, t)| (t.order, i)).collect();
        let order = crate::order::OrderLists::from_level(level, &camera_orders);
        CameraTriggers { list: Arc::new(list), touch: Arc::new(touch), order: Arc::new(order) }
    }

    /// Camera trigger `index` firing now: its action, with a static camera's target position
    /// from the world.
    pub fn action(&self, index: usize, world: &crate::world::World) -> Option<CameraAction> {
        Self::action_of(&self.list[index], world)
    }

    /// Every (kind, id) a change BG / ground / MG trigger of the level switches to (to load
    /// their images before the level starts).
    pub fn art_ids(&self) -> Vec<(usize, i32)> {
        let all = self.list.iter().chain(self.touch.iter().map(|(_, t)| t));
        let mut out: Vec<(usize, i32)> = all.filter_map(|t| match t.kind { TriggerKind::ChangeArt { kind, id } => Some((kind, id)), _ => None }).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// How many touch-triggered camera triggers the level has.
    pub fn touch_count(&self) -> usize {
        self.touch.len()
    }

    /// Touch trigger `index`: whether `rect` touches it, and its level order (for `enabled`).
    pub fn touches(&self, index: usize, rect: &crate::world::Rect) -> Option<u32> {
        let (r, t) = &self.touch[index];
        r.intersects(rect).then_some(t.order)
    }

    /// Touch trigger `index` firing now (see `action`).
    pub fn touch_action(&self, index: usize, world: &crate::world::World) -> Option<CameraAction> {
        Self::action_of(&self.touch[index].1, world)
    }

    fn action_of(t: &CameraTrigger, world: &crate::world::World) -> Option<CameraAction> {
        Some(match t.kind {
            TriggerKind::Zoom { zoom, duration, easing, rate } => CameraAction::Zoom { zoom, duration, easing, rate },
            TriggerKind::MgOffset { y, duration, easing, rate } => CameraAction::MgOffset { y, duration, easing, rate },
            TriggerKind::ChangeArt { kind, id } => CameraAction::ChangeArt { kind, id },
            TriggerKind::Rotate { degrees, add, snap, duration, easing, rate } => CameraAction::Rotate { degrees, add, snap, duration, easing, rate },
            TriggerKind::Static { target_order, duration, easing, rate, x, y, follow, follow_easing, smooth, smooth_mod } => {
                let o = target_order?;
                let (px, py) = world.level_position(o as usize);
                let follow = follow.then_some(o);
                CameraAction::Static { target: (px as f32, py as f32), duration, easing, rate, x, y, follow, follow_easing, smooth, smooth_mod }
            }
            TriggerKind::ExitStatic { x, y, duration, easing, rate, smooth, smooth_mod } => {
                CameraAction::ExitStatic { x, y, duration, easing, rate, smooth, smooth_mod }
            }
            TriggerKind::Offset { x, y, duration, easing, rate } => CameraAction::Offset { x, y, duration, easing, rate },
            TriggerKind::RotateGameplay { dir, dir2, edit, keep, mod_x, mod_y, .. } => {
                if keep && !edit {
                    CameraAction::RotateGameplay { dir, dir2, edit: true, over: true, mod_x: 0.0, mod_y: 0.0 }
                } else {
                    CameraAction::RotateGameplay { dir, dir2, edit, over: false, mod_x, mod_y }
                }
            }
        })
    }

    /// A rotate-gameplay trigger's channel switch (`GJBaseGameLayer::rotateGameplay`):
    /// (switch to, the new channel reversed) and whether it turns the gameplay.
    pub fn rotation_channel(&self, index: usize) -> Option<(Option<(i32, bool)>, bool)> {
        match self.list[index].kind {
            TriggerKind::RotateGameplay { dir2, change, rotate, target, .. } => {
                Some((change.then_some((target, (dir2 & !1) == 2)), rotate))
            }
            _ => None,
        }
    }
}

/// Camera origin (bottom-left of the view) in level units. The fields follow the game's
/// `GJGameState` camera block (+0x1a8 on `GJBaseGameLayer`); `[x, y]` pairs are the game's
/// `CCPoint`s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The camera position after the step (+0x3e0, also kept as the last position +0x4b4):
    /// the followed / static position plus the static exit offset.
    pub x: f32,
    pub y: f32,
    /// The corridor seen at the last step, to notice mode portals.
    corridor: Option<(f32, f32)>,
    /// Mirror-portal flip, 0 (normal) to 1 (mirrored): the world is drawn scaled by 1 − 2·flip
    /// around the screen's center, and the background scrolls the other way.
    pub flip: f32,
    flip_tween: Option<Tween>,
    mirrored: bool,
    /// `m_cameraZoom` (+0x1a8) and its tween; the zoom a running tween ends at
    /// (`m_targetCameraZoom`, +0x1ac); last step's zoom (+0x2f0).
    pub zoom: f32,
    zoom_tween: Option<Tween>,
    /// `m_cameraAngle` (degrees, clockwise) and its tween.
    pub angle: f32,
    angle_tween: Option<Tween>,
    /// The background, ground and middle ground art the change triggers (3029–3031) set; 0 is
    /// the level's own (its settings kA6 / kA7 / kA25).
    pub art: [i32; 3],
    /// The middle ground's y offset (+0x2bc, middle-ground trigger 2999) and its tween.
    pub mg_offset: f32,
    mg_tween: Option<Tween>,
    pub target_zoom: f32,
    last_zoom: f32,
    /// `m_cameraOffset` (+0x1b0, camera offset triggers, tweens 15/16) and last step's
    /// (+0x1c8).
    pub offset: [f32; 2],
    offset_tween: [Option<Tween>; 2],
    last_offset: [f32; 2],
    /// `m_lastCameraTarget` (+0x1c0) and the camera's velocity (+0x1d0, per second).
    last_target: [f32; 2],
    delta: [f32; 2],
    /// Static camera: start (+0x1d8), position (+0x1e0, tweens 10/11), target (+0x1e8).
    static_start: [f32; 2],
    static_pos: [f32; 2],
    static_pos_tween: [Option<Tween>; 2],
    static_target: [f32; 2],
    /// The static exit offset (+0x1f0, tweens 12/13) and the part applied last step (+0x1f8).
    exit_offset: [f32; 2],
    exit_offset_tween: [Option<Tween>; 2],
    applied_exit: [f32; 2],
    /// Start velocity (+0x200), duration (+0x208), bezier progress (+0x210, tweens 21/22)
    /// and last progress (+0x218).
    start_velocity: [f32; 2],
    static_duration: [f32; 2],
    progress: [f32; 2],
    progress_tween: [Option<Tween>; 2],
    last_progress: [f32; 2],
    /// Follow easing (+0x220), the followed object (+0x228/+0x230, its level order).
    follow_easing: [f32; 2],
    follow: [Option<u32>; 2],
    /// Static on x / y (+0x238).
    pub static_active: [bool; 2],
    /// A static exit to apply this step (+0x240), with its duration (+0x248), rate (+0x250),
    /// easing (+0x258); smooth velocity (+0x260, shared with the static trigger's) and its
    /// modifier (+0x268); the exit's starting offset (+0x270).
    exit_pending: [bool; 2],
    exit_duration: [f32; 2],
    exit_rate: [f32; 2],
    exit_easing: [i32; 2],
    smooth: [bool; 2],
    smooth_mod: [f32; 2],
    exit_start: [f32; 2],
    /// The gameplay offset the follow eases toward (+0x280), its target (+0x288) and last
    /// step's target (+0x290).
    internal: [f32; 2],
    internal_target: [f32; 2],
    last_internal_target: [f32; 2],
    /// The static y belongs to the corridor itself (`m_dualStaticCamera`, +0x2a0).
    pub dual_static: bool,
    /// The player's sideways / going-left state at the last step (+0x4bc/+0x4bd).
    last_sideways: bool,
    last_going_left: bool,
    /// The corridor ground: slide-in progress (+0x3f4, tween 0x19), height (+0x2ec), center
    /// (+0x308), active (+0x310), and the two ground nodes' y (screen units).
    ground: f32,
    ground_tween: Option<Tween>,
    pub mode_height: f32,
    pub fly_center: f32,
    fly_active: bool,
    pub ground_bottom: f32,
    pub ground_top: f32,
}

/// A `GJGameState` tween (`updateTweenActions`): elapsed += dt; t = elapsed / duration (1 when
/// done), eased; value = fma(to − from, t, from). All f32.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tween {
    from: f32,
    to: f32,
    duration: f32,
    rate: f32,
    easing: i32,
    elapsed: f32,
    value: f32,
}

impl Tween {
    fn new(from: f32, to: f32, duration: f32, rate: f32) -> Tween {
        Tween { from, to, duration, rate, easing: 1, elapsed: 0.0, value: from }
    }

    fn with_easing(from: f32, to: f32, duration: f32, easing: i32, rate: f32) -> Tween {
        Tween { from, to, duration, rate, easing, elapsed: 0.0, value: from }
    }

    fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        let t = if self.elapsed < self.duration { self.elapsed / self.duration } else { 1.0 };
        let t = if self.easing == 1 { ease_in_out(t, self.rate) } else { crate::easing::eased(t, self.rate, self.easing) };
        self.value = (self.to - self.from).mul_add(t, self.from);
        self.duration <= self.elapsed
    }
}

/// `GameToolbox::getEasedValue` type 1 (ease in-out) with the given rate. The game calls the
/// system `powf` at run time, which can differ from an exact square by an ulp; `black_box`
/// keeps the compiler from folding `powf(x, 2.0)` into `x * x`, so native builds call the same
/// function. (wasm uses Rust's own libm, which may differ by an ulp — invisible.)
fn ease_in_out(t: f32, rate: f32) -> f32 {
    let rate = std::hint::black_box(rate);
    let x = t + t;
    if 1.0 <= x { (2.0 - x).powf(rate).mul_add(-0.5, 1.0) } else { x.powf(rate) * 0.5 }
}

const STEP: f32 = 1.0 / crate::physics::STEPS_PER_SECOND as f32;

/// `m_gameplayOffsetX/Y`: how far the followed player sits from the view's center (no
/// gameplay offset triggers yet).
const GAMEPLAY_OFFSET: [f32; 2] = [75.0, 75.0];

impl Camera {
    /// The camera starts here and holds until the player is far enough in.
    pub const START_X: f32 = 15.0;

    pub fn new() -> Camera {
        Camera {
            x: Self::START_X,
            y: 0.0,
            corridor: None,
            flip: 0.0,
            flip_tween: None,
            mirrored: false,
            zoom: 1.0,
            zoom_tween: None,
            angle: 0.0,
            angle_tween: None,
            art: [0; 3],
            mg_offset: 0.0,
            mg_tween: None,
            target_zoom: 1.0,
            last_zoom: 1.0,
            offset: [0.0; 2],
            offset_tween: [None; 2],
            last_offset: [0.0; 2],
            last_target: [0.0; 2],
            delta: [0.0; 2],
            static_start: [0.0; 2],
            static_pos: [0.0; 2],
            static_pos_tween: [None; 2],
            static_target: [0.0; 2],
            exit_offset: [0.0; 2],
            exit_offset_tween: [None; 2],
            applied_exit: [0.0; 2],
            start_velocity: [0.0; 2],
            static_duration: [0.0; 2],
            progress: [0.0; 2],
            progress_tween: [None; 2],
            last_progress: [0.0; 2],
            follow_easing: [0.0; 2],
            follow: [None; 2],
            static_active: [false; 2],
            exit_pending: [false; 2],
            exit_duration: [0.0; 2],
            exit_rate: [0.0; 2],
            exit_easing: [0; 2],
            smooth: [false; 2],
            smooth_mod: [0.0; 2],
            exit_start: [0.0; 2],
            internal: GAMEPLAY_OFFSET_START,
            internal_target: GAMEPLAY_OFFSET_START,
            last_internal_target: GAMEPLAY_OFFSET_START,
            dual_static: false,
            last_sideways: false,
            last_going_left: false,
            ground: 0.0,
            ground_tween: None,
            mode_height: 0.0,
            fly_center: 0.0,
            fly_active: false,
            ground_bottom: 0.0,
            ground_top: 0.0,
        }
    }

    /// Whether a corridor's ground is showing (+0x310).
    pub fn fly_active_pub(&self) -> bool {
        self.fly_active
    }

    /// Static camera on y (+0x23c).
    pub fn static_y_on(&self) -> bool {
        self.static_active[1]
    }

    /// One physics step without camera triggers.
    pub fn step(&mut self, player: &Player, view_w: f32) {
        self.step_with(player, view_w, &[], &|_| (0.0, 0.0));
    }

    /// One physics step, after the player's and the triggers': the corridor's ground and static
    /// view, this step's camera triggers, the tweens (`updateTweenActions`), then
    /// `updateCamera`. `position` gives an object's current position by level order (static
    /// cameras that follow their target).
    pub fn step_with(&mut self, player: &Player, view_w: f32, fired: &[CameraAction], position: &dyn Fn(u32) -> (f32, f32)) {
        let step = player.step_secs as f32;
        // updateDualGround: a free-mode portal (layer +0x311) animates the ground out instead.
        let corridor = if player.free_mode { None } else { player.corridor };
        if corridor != self.corridor {
            match corridor {
                Some((floor, ceiling)) => self.animate_in(floor, ceiling, view_w),
                None => self.animate_out(),
            }
            self.corridor = corridor;
        }
        for action in fired {
            self.apply(*action, view_w);
        }
        // updateTweenActions.
        if let Some(mut t) = self.angle_tween {
            let done = t.advance(step);
            self.angle = t.value;
            self.angle_tween = if done { None } else { Some(t) };
        }
        if let Some(mut t) = self.zoom_tween {
            let done = t.advance(step);
            self.zoom = t.value;
            self.zoom_tween = if done { None } else { Some(t) };
        }
        if let Some(mut t) = self.mg_tween {
            let done = t.advance(step);
            self.mg_offset = t.value;
            self.mg_tween = if done { None } else { Some(t) };
        }
        if let Some(mut t) = self.ground_tween {
            let done = t.advance(step);
            self.ground = t.value;
            self.ground_tween = if done { None } else { Some(t) };
        }
        for a in 0..2 {
            advance_into(&mut self.static_pos_tween[a], &mut self.static_pos[a], step);
            advance_into(&mut self.exit_offset_tween[a], &mut self.exit_offset[a], step);
            advance_into(&mut self.progress_tween[a], &mut self.progress[a], step);
            advance_into(&mut self.offset_tween[a], &mut self.offset[a], step);
        }
        self.step_flip(player.mirrored);
        self.update_camera(player, view_w, position);
        self.update_ground_nodes();
    }

    /// `GJBaseGameLayer::updateCamera` (0x1083f8), classic (not platformer) without camera
    /// edges, shake or level flip: x and y each either follow the player (y in a band around
    /// the center, x at the gameplay offset; the axes swap while sideways) or hold the static
    /// position; then the static exit offset is added.
    fn update_camera(&mut self, player: &Player, view_w: f32, position: &dyn Fn(u32) -> (f32, f32)) {
        let dt = (player.step_secs * 60.0) as f32;
        let (w, h) = (view_w, VIEW_HEIGHT);
        let z = self.zoom;
        let lz = self.last_zoom;
        let wz = w / z;
        let hz = h / z;
        let sideways = player.sideways;
        let left = player.going_left;
        let up = player.upside_down;
        let basic = matches!(player.mode, crate::physics::Mode::Cube | crate::physics::Mode::Robot);
        // local_100[0..1]: this step's static exit applies at once (instant easing).
        let instant = if dt == 0.0 { [true, true] } else { self.exit_pending };
        if 0.0 < dt && self.last_sideways != sideways {
            for a in 0..2 {
                if !self.static_active[a] && !instant[a] {
                    self.exit_pending[a] = true;
                    self.exit_duration[a] = 1.0;
                    self.exit_easing[a] = 1;
                    self.exit_rate[a] = 2.0;
                    self.smooth[a] = false;
                }
            }
        }
        let dir = if left { -1.0f32 } else { 1.0 };
        let goff = if !sideways { [(dir * GAMEPLAY_OFFSET[0]) / z, 0.0] } else { [0.0, (dir * GAMEPLAY_OFFSET[1]) / z] };
        let mut cam = [self.x - self.applied_exit[0], self.y - self.applied_exit[1]];
        for a in 0..2 {
            cam[a] += self.offset[a] - self.last_offset[a];
        }
        let ppos = [player.x, player.y];
        let pdelta = [player.x - player.last_x, player.y - player.last_y];
        // The follow band: 70 above and 40 below the center (swapped upside down); other
        // modes use the camera padding (`m_cameraPadding`, +0x2cc; 0 here) and easing.
        let (hi, lo) = if !up { (70.0f32, 40.0f32) } else { (40.0, 70.0) };
        let (padding, camera_easing) = player.camera_padding;
        let pad = [
            ((w * 0.5) - padding.mul_add((w * 0.5 + -2.0) + -30.0, 30.0)) * (1.0 / z),
            ((h * 0.5) - padding.mul_add((h * 0.5 + -2.0) + -30.0, 30.0)) * (1.0 / z),
        ];
        let mut ease = if sideways { [10.0f32, 1.0] } else { [1.0f32, 10.0] };
        // Zooming keeps the view's center (bVar13, the "last grounded on the floor" case, isn't
        // modelled).
        if z != lz {
            cam[0] = ((w / lz) * 0.5 - wz * 0.5) + cam[0];
            cam[1] = ((h / lz) * 0.5 - hz * 0.5) + cam[1];
        }
        let mut target = cam;
        for a in 0..2 {
            let half = if a == 0 { wz } else { hz } * 0.5;
            if !self.static_active[a] {
                let band_axis = if sideways { a == 0 } else { a == 1 };
                if band_axis {
                    let (hi, lo) = if basic { (hi, lo) } else {
                        ease[a] = camera_easing; // m_cameraEasing (+0x2d0)
                        (pad[a], pad[a])
                    };
                    let p = ppos[a];
                    let off = self.offset[a];
                    let center = half + cam[a];
                    if p + off <= hi + center {
                        if !(center - lo <= p + off) {
                            target[a] = off + (lo + (p - half));
                        }
                    } else {
                        target[a] = off + ((p - half) - hi);
                    }
                } else {
                    // The gameplay offset, eased toward its target by the player's movement.
                    self.internal_target[a] = goff[a];
                    let t = goff[a];
                    let cur = self.internal[a];
                    let v = if z != lz && cur == self.last_internal_target[a] && self.last_going_left == left {
                        t
                    } else {
                        let d = pdelta[a].abs();
                        if cur < t {
                            let n = cur + d;
                            if n > t { t } else { n }
                        } else {
                            let n = cur - d;
                            if n < t { t } else { n }
                        }
                    };
                    let v = if instant[a] { t } else { v };
                    self.internal[a] = v;
                    target[a] = ((ppos[a] - half) + v) + self.offset[a];
                    if a == 0 && !sideways {
                        target[a] = target[a].max(Self::START_X);
                    }
                }
            } else {
                let follow = self.follow[a];
                let mut v;
                if follow.is_none() && !self.smooth[a] {
                    ease[a] = 1.0;
                    v = self.static_pos[a];
                } else {
                    let tgt = match follow {
                        Some(o) => {
                            let p = position(o);
                            [p.0, p.1]
                        }
                        None => self.static_target,
                    };
                    let prog = self.progress[a];
                    if 1.0 <= prog {
                        self.static_pos[a] = tgt[a];
                    } else if !self.smooth[a] {
                        let last = self.last_progress[a];
                        if prog != last {
                            let pos = self.static_pos[a];
                            self.static_pos[a] = pos + (tgt[a] - pos) / ((1.0 - last) / (prog - last));
                            self.last_progress[a] = prog;
                        }
                    } else {
                        let p0 = self.static_start;
                        let p1 = [
                            p0[0] + (self.start_velocity[0] * self.static_duration[0]) / 3.0,
                            p0[1] + (self.start_velocity[1] * self.static_duration[1]) / 3.0,
                        ];
                        let m = self.smooth_mod[a];
                        let p2 = [(p1[0] - tgt[0]).mul_add(m, tgt[0]), (p1[1] - tgt[1]).mul_add(m, tgt[1])];
                        self.static_pos[a] = bezier(p0[a], p1[a], p2[a], tgt[a], prog);
                    }
                    ease[a] = match follow {
                        Some(_) => {
                            let e = self.follow_easing[a] * self.progress[a];
                            if e <= 1.0 { 1.0 } else { e }
                        }
                        None => 1.0,
                    };
                    v = self.static_pos[a];
                }
                if a == 1 && self.fly_active {
                    // Keep the corridor's ground in view.
                    let lim = (self.mode_height * 0.5) / z + 90.0;
                    let st = self.static_target[1];
                    if st < lim {
                        let f = if st == self.static_start[1] { 0.0 } else { (st - v) / (st - self.static_start[1]) };
                        v = (lim - st).mul_add(1.0 - f, v);
                    }
                }
                target[a] = (v - half) + self.offset[a];
            }
            // The bottom edge (y 0); no other camera edges.
            if a == 1 && target[1] < 0.0 {
                target[1] = 0.0;
            }
        }
        self.last_target = target;
        if debug_camera() {
            eprintln!("cam y: cam {:?} target {:?} ease {:?} z {} lz {} static {:?} basic {basic} side {sideways} up {up} p {:?}", cam, target, ease, z, lz, self.static_active, ppos);
        }
        if !instant[1] {
            // (The teleport ease timer isn't modelled.)
        } else {
            ease[1] = 1.0;
        }
        if instant[0] {
            ease[0] = 1.0;
        }
        if target[1] < 0.0 {
            target[1] = 0.0;
        }
        for a in 0..2 {
            cam[a] = if 1.0 < ease[a] {
                let e = if dt <= 0.0 { ease[a] } else { ease[a] / dt };
                cam[a] + (target[a] - cam[a]) / e
            } else {
                target[a]
            };
        }
        if cam[1] < 0.0 {
            cam[1] = 0.0;
        }
        // The static exit: the jump the camera just made becomes an offset that eases out.
        let diff = [self.x - cam[0], self.y - cam[1]];
        for a in 0..2 {
            if self.exit_pending[a] {
                self.exit_pending[a] = false;
                self.exit_offset[a] = diff[a];
                let dur = self.exit_duration[a];
                if dur <= 0.0 {
                    self.exit_offset_tween[a] = None;
                    self.exit_offset[a] = 0.0;
                } else if !self.smooth[a] {
                    self.exit_offset_tween[a] = Some(Tween::with_easing(diff[a], 0.0, dur, self.exit_easing[a], self.exit_rate[a]));
                } else {
                    self.exit_offset_tween[a] = None;
                    self.exit_start[a] = self.exit_offset[a];
                    self.progress[a] = 0.0;
                    self.progress_tween[a] = Some(Tween::with_easing(0.0, 1.0, dur, self.exit_easing[a], self.exit_rate[a]));
                }
            }
            if !self.static_active[a] && self.smooth[a] {
                let prog = self.progress[a];
                if 1.0 <= prog {
                    self.smooth[a] = false;
                    self.exit_offset[a] = 0.0;
                } else {
                    let p0 = self.exit_start;
                    let p1 = [
                        p0[0] + (self.start_velocity[0] * self.exit_duration[0]) / 3.0,
                        p0[1] + (self.start_velocity[1] * self.exit_duration[1]) / 3.0,
                    ];
                    let m = self.smooth_mod[a];
                    let p2 = [p1[0].mul_add(m, 0.0), p1[1].mul_add(m, 0.0)];
                    self.exit_offset[a] = bezier(p0[a], p1[a], p2[a], 0.0, prog);
                }
            }
        }
        let raw = cam;
        for a in 0..2 {
            cam[a] += self.exit_offset[a];
        }
        if cam[1] < 0.0 {
            cam[1] = 0.0;
        }
        self.applied_exit = [cam[0] - raw[0], cam[1] - raw[1]];
        self.delta = [(cam[0] - self.x) * 240.0, (cam[1] - self.y) * 240.0];
        self.x = cam[0];
        self.y = cam[1];
        self.last_zoom = z;
        self.last_sideways = sideways;
        self.last_going_left = left;
        self.last_offset = self.offset;
        self.last_internal_target = self.internal_target;
    }

    /// The view's center as `updateStaticCameraPos` / `exitStaticCamera` see it: the last
    /// camera position plus half the (zoomed) view, minus the camera offset.
    fn view_center(&self, view_w: f32) -> [f32; 2] {
        let z = self.zoom;
        [(self.x + (view_w * 0.5) / z) - self.offset[0], (self.y + (VIEW_HEIGHT * 0.5) / z) - self.offset[1]]
    }

    /// `updateStaticCameraPos(duration, rate, target, x, y, follow or smooth, easing)`.
    #[allow(clippy::too_many_arguments)]
    fn update_static_pos(&mut self, view_w: f32, duration: f32, rate: f32, target: [f32; 2], axes: [bool; 2], moving: bool, easing: i32) {
        self.dual_static = false;
        let center = self.view_center(view_w);
        for a in 0..2 {
            if !axes[a] {
                continue;
            }
            self.static_active[a] = true;
            self.static_pos[a] = center[a];
            self.exit_offset[a] = 0.0;
            self.exit_offset_tween[a] = None;
            self.start_velocity[a] = self.delta[a].clamp(-9999.0, 9999.0);
            self.static_duration[a] = duration;
            self.static_start[a] = center[a];
            self.static_target[a] = target[a];
        }
        if moving {
            return;
        }
        for a in 0..2 {
            if !axes[a] {
                continue;
            }
            if duration <= 0.0 {
                self.static_pos_tween[a] = None;
                self.static_pos[a] = self.static_target[a];
            } else {
                self.static_pos_tween[a] = Some(Tween::with_easing(self.static_pos[a], self.static_target[a], duration, easing, rate));
            }
        }
    }

    /// `exitStaticCamera` on the given axes.
    #[allow(clippy::too_many_arguments)]
    fn exit_static(&mut self, axes: [bool; 2], duration: f32, rate: f32, easing: i32, smooth: bool, smooth_mod: f32) {
        for a in 0..2 {
            if !axes[a] || !self.static_active[a] {
                continue;
            }
            self.exit_pending[a] = true;
            self.static_active[a] = false;
            self.static_pos_tween[a] = None;
            self.exit_duration[a] = duration;
            self.exit_easing[a] = easing;
            self.exit_rate[a] = rate;
            self.smooth[a] = smooth;
            self.smooth_mod[a] = smooth_mod;
            self.start_velocity[a] = self.delta[a].clamp(-9999.0, 9999.0);
            self.static_start[a] = self.static_pos[a];
        }
    }

    /// `animateInDualGroundNew`: the corridor's center and height are set, the ground slides in
    /// (0.5 s, 0.4 when it was already showing; ease in-out 2), and unless a static camera
    /// trigger holds the view, the camera's y is held on the corridor's center.
    /// The level starts in a flying mode (setupLevelStart): the corridor's ground and the
    /// camera are in place from the first frame, with no slide in.
    pub fn start_in_corridor(&mut self, corridor: Option<(f32, f32)>, view_w: f32) {
        let Some((floor, ceiling)) = corridor else { return };
        let height = ceiling - floor;
        self.ground = 1.0;
        self.ground_tween = None;
        self.update_static_pos(view_w, 0.0, 2.0, [0.0, height * 0.5 + floor], [false, true], false, 1);
        self.dual_static = true;
        self.fly_active = true;
        self.mode_height = height;
        self.fly_center = height * 0.5 + floor;
        self.corridor = corridor;
    }

    fn animate_in(&mut self, floor: f32, ceiling: f32, view_w: f32) {
        let half = VIEW_HEIGHT * 0.5;
        let height = ceiling - floor;
        let duration = if self.fly_active { 0.4 } else { 0.5 };
        // The ground tween restarts unless the same corridor is already showing.
        if !self.fly_active || self.mode_height != height {
            let mut from = self.ground;
            if from < 0.0 {
                from = 0.0;
            } else if 0.0 < from {
                from = (from * (half + (1.0 - self.mode_height * 0.5))) / ((1.0 - height * 0.5) + half);
            }
            self.ground_tween = Some(Tween::new(from, 1.0, duration, 2.0));
            self.ground = from;
        }
        if !self.static_active[1] || self.dual_static {
            self.update_static_pos(view_w, duration, 2.0, [0.0, height * 0.5 + floor], [false, true], false, 1);
            self.dual_static = true;
        }
        self.fly_active = true;
        self.mode_height = height;
        self.fly_center = height * 0.5 + floor;
    }

    /// `animateOutGroundNew`: the corridor's own static view ends at once; the ground slides
    /// out (0.4 s, ease in-out 1.5).
    fn animate_out(&mut self) {
        self.fly_active = false;
        if self.dual_static {
            self.static_pos_tween[1] = None;
            self.exit_offset_tween[1] = None;
            self.static_pos[1] = 0.0;
            self.exit_offset[1] = 0.0;
            self.static_active[1] = false;
            self.exit_pending[1] = false;
            self.smooth[1] = false;
        }
        self.ground_tween = Some(Tween::new(self.ground, 0.0, 0.4, 1.5));
    }

    /// A camera trigger's effect when it fires (before this step's tweens advance).
    fn apply(&mut self, action: CameraAction, view_w: f32) {
        match action {
            CameraAction::Zoom { zoom, duration, easing, rate } => {
                // updateZoom: clamped to [0.4, 3] (0 or less means 1).
                let z = if zoom <= 0.0 { 1.0 } else { zoom.min(3.0).max(0.4) };
                self.target_zoom = z;
                if duration <= 0.0 {
                    self.zoom_tween = None;
                    self.zoom = z;
                } else {
                    self.zoom_tween = Some(Tween::with_easing(self.zoom, z, duration, easing, rate));
                }
            }
            CameraAction::ChangeArt { kind, id } => {
                if let Some(a) = self.art.get_mut(kind) {
                    *a = id;
                }
            }
            CameraAction::MgOffset { y, duration, easing, rate } => {
                if duration <= 0.0 {
                    self.mg_tween = None;
                    self.mg_offset = y;
                } else {
                    self.mg_tween = Some(Tween::with_easing(self.mg_offset, y, duration, easing, rate));
                }
            }
            CameraAction::RotateGameplay { .. } => {}
            CameraAction::Rotate { degrees, add, snap, duration, easing, rate } => {
                if snap {
                    self.angle = self.angle.rem_euclid(360.0);
                }
                let to = if add { self.angle + degrees } else { degrees };
                if duration <= 0.0 {
                    self.angle_tween = None;
                    self.angle = to;
                } else {
                    self.angle_tween = Some(Tween::with_easing(self.angle, to, duration, easing, rate));
                }
            }
            CameraAction::ExitStatic { x, y, duration, easing, rate, smooth, smooth_mod } => {
                self.exit_static([x, y], duration, rate, easing, smooth, smooth_mod);
                // With a corridor showing, updateDualGround gives it its own static view again.
                if self.fly_active && !self.static_active[1] {
                    let (floor, height) = (self.fly_center - self.mode_height * 0.5, self.mode_height);
                    self.update_static_pos(view_w, 0.4, 2.0, [0.0, height * 0.5 + floor], [false, true], false, 1);
                    self.dual_static = true;
                }
            }
            CameraAction::Static { target, duration, easing, rate, x, y, follow, follow_easing, smooth, smooth_mod } => {
                // FUN_10011058c, then updateStaticCameraPos.
                let follow_easing = if follow_easing <= 1.0 { 1.0 } else { follow_easing };
                for (a, on) in [x, y].into_iter().enumerate() {
                    if !on {
                        continue;
                    }
                    self.follow[a] = follow;
                    self.smooth[a] = smooth;
                    self.smooth_mod[a] = smooth_mod;
                    self.follow_easing[a] = follow_easing;
                    self.progress_tween[a] = None;
                    if follow.is_some() || smooth {
                        if duration <= 0.0 {
                            self.progress[a] = 1.0;
                            self.last_progress[a] = 1.0;
                        } else {
                            self.progress[a] = 0.0;
                            self.last_progress[a] = 0.0;
                            self.progress_tween[a] = Some(Tween::with_easing(0.0, 1.0, duration, easing, rate));
                        }
                    }
                }
                self.update_static_pos(view_w, duration, rate, [target.0, target.1], [x, y], follow.is_some() || smooth, easing);
            }
            CameraAction::Offset { x, y, duration, easing, rate } => {
                // updateCameraOffsetX/Y: tweens 15/16 toward the new offset.
                for (a, v) in [x, y].into_iter().enumerate() {
                    let Some(v) = v else { continue };
                    if duration <= 0.0 {
                        self.offset_tween[a] = None;
                        self.offset[a] = v;
                    } else {
                        self.offset_tween[a] = Some(Tween::with_easing(self.offset[a], v, duration, easing, rate));
                    }
                }
            }
        }
    }

    /// `updateCameraBGArt`'s ground layers (screen units): the bottom one slides up from below the
    /// view to the corridor's floor (never below the level's ground at y 91), the top one down
    /// to its ceiling.
    fn update_ground_nodes(&mut self) {
        let zoom = self.zoom;
        let h = VIEW_HEIGHT;
        let p = self.ground;
        let half_h = self.mode_height * 0.5;
        let g = 0.0f32; // +0x3154 · zoom · 0.5 (0 in the traces seen so far)
        let slide = p.mul_add(g + h * 0.5 + (1.0 - half_h), -g);
        let layer_y = -self.y * zoom;
        let base = zoom.mul_add(91.0, layer_y);
        self.ground_bottom = if base < slide { slide } else { base };
        let top = h + g;
        self.ground_top = (-p).mul_add(top - ((h * 0.5 + half_h) + -1.0), top);
    }

    /// The corridor's floor and ceiling as `checkCollisions` reads them, given the corridor
    /// (floor, ceiling) the player is in: through the ground layers when a static camera trigger
    /// holds the view, else from the corridor's center and height scaled by the zoom.
    pub fn corridor_bounds(&self, corridor: (f32, f32)) -> (f32, f32) {
        if self.static_active[1] && !self.dual_static {
            let floor = self.y + (self.ground_bottom + -1.0) / self.zoom;
            let floor = if 90.0 < floor { floor } else { 90.0 };
            (floor, self.y + (self.ground_top + 1.0) / self.zoom)
        } else {
            let (lo, hi) = corridor;
            let height = hi - lo;
            let center = height * 0.5 + lo;
            let floor = self.offset[1] + (center + (height * -0.5) / self.zoom);
            let floor = if floor <= 90.0 { 90.0 } else { floor };
            (floor, height / self.zoom + floor)
        }
    }

    /// `getMinPortalY` / `getMaxPortalY`: the spider's jump limits.
    pub fn portal_limits(&self) -> (f32, f32) {
        if self.static_active[1] && !self.dual_static {
            let min = self.y + (self.ground_bottom + -1.0) / self.zoom;
            let min = if 90.0 < min { min } else { 90.0 };
            (min, self.y + (self.ground_top + 1.0) / self.zoom)
        } else {
            let min = self.offset[1] + (self.fly_center + (self.mode_height * -0.5) / self.zoom);
            let min = if min <= 90.0 { 90.0 } else { min };
            (min, self.mode_height / self.zoom + min)
        }
    }
}

/// The starting gameplay offset the follow eases from (`resetCamera`).
const GAMEPLAY_OFFSET_START: [f32; 2] = [75.0, 0.0];

/// `CADENCE_DEBUG_CAMSTEP`: log the camera update (read once).
fn debug_camera() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CADENCE_DEBUG_CAMSTEP").is_some())
}

/// Advances a camera tween and writes its value into its field (removed when done).
fn advance_into(tween: &mut Option<Tween>, field: &mut f32, dt: f32) {
    if let Some(mut t) = *tween {
        let done = t.advance(dt);
        *field = t.value;
        *tween = if done { None } else { Some(t) };
    }
}

/// The camera's cubic bezier as the game computes it (fused multiply-adds from the instructions):
/// u = 1 − t; fma(t·(t·p3), t, fma(t·(t·(p2·3)), u, fma(u·(u·p0), u, u·(u·(t·(p1·3)))))).
fn bezier(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    let a = u * (u * (t * (p1 * 3.0)));
    let s = (u * (u * p0)).mul_add(u, a);
    let s = (t * (t * (p2 * 3.0))).mul_add(u, s);
    (t * (t * p3)).mul_add(t, s)
}

impl Camera {
    /// After the level end: the camera eases toward showing the level's end at the right edge
    /// (`level_length − view width`). Approximate: about 1% of the remaining distance per step,
    /// fitted to real traces (the exact end-camera code isn't ported yet).
    pub fn step_end(&mut self, level_length: f32, view_w: f32) {
        let target = level_length - view_w;
        if target > self.x {
            self.x += (target - self.x) * 0.0095;
        }
    }

    /// `toggleFlipped`: a mirror portal tweens the flip to 1 (or back to 0) over 0.4 s, ease
    /// in-out at rate 1.2. A flip starting from exactly 0 or 1 starts a hair inside the range.
    fn step_flip(&mut self, mirrored: bool) {
        if mirrored != self.mirrored {
            self.mirrored = mirrored;
            let from = match (mirrored, self.flip) {
                (true, f) if f == 0.0 || f == 1.0 => f32::from_bits(0x3727c5ac),
                (false, f) if f == 0.0 || f == 1.0 => f32::from_bits(0x3f7fff58),
                (_, f) => f,
            };
            self.flip_tween = Some(Tween::new(from, if mirrored { 1.0 } else { 0.0 }, 0.4, 1.2));
        }
        if let Some(tween) = &mut self.flip_tween {
            let done = tween.advance(STEP);
            self.flip = tween.value;
            if done {
                self.flip_tween = None;
            }
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Camera::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_rotate_sets_adds_and_snaps() {
        let mut c = Camera::new();
        c.apply(CameraAction::Rotate { degrees: 90.0, add: false, snap: false, duration: 0.0, easing: 0, rate: 2.0 }, 569.0);
        assert_eq!(c.angle, 90.0);
        c.apply(CameraAction::Rotate { degrees: 300.0, add: true, snap: false, duration: 0.0, easing: 0, rate: 2.0 }, 569.0);
        assert_eq!(c.angle, 390.0);
        // snap: 390 → 30 first, then +45
        c.apply(CameraAction::Rotate { degrees: 45.0, add: true, snap: true, duration: 0.0, easing: 0, rate: 2.0 }, 569.0);
        assert_eq!(c.angle, 75.0);
        // a timed rotation tweens toward its end
        c.apply(CameraAction::Rotate { degrees: 0.0, add: false, snap: false, duration: 1.0, easing: 0, rate: 2.0 }, 569.0);
        assert_eq!(c.angle, 75.0);
        assert!(c.angle_tween.is_some());
    }

    #[test]
    fn touch_triggers_fire_by_their_scaled_square() {
        // Dash's 90° rotation: touch-triggered (11), rotated, scaled 3.278; and a position one.
        let level = LevelString::parse("kS38,1;1,2015,2,6703.2,3,345,6,90,128,3.278,11,1,68,90,10,1;1,2015,2,7995,3,1275,68,180,70,1,10,4;");
        let t = CameraTriggers::from_level(&level);
        assert_eq!(t.list.len(), 1, "touch triggers stay out of the position list");
        assert_eq!(t.touch_count(), 1);
        // 30 × 3.278 ≈ 98.3 wide, centered at (6703.2, 345 + ground offset)
        let y = 345.0 + crate::world::GROUND_OFFSET;
        let at = |x: f32, y: f32| crate::world::Rect { x: x - 15.0, y: y - 15.0, w: 30.0, h: 30.0 };
        assert_eq!(t.touches(0, &at(6703.2 - 49.0 - 14.0, y)), Some(0));
        assert_eq!(t.touches(0, &at(6703.2 - 49.2 - 16.0, y)), None);
        assert_eq!(t.touches(0, &at(6703.2, y + 70.0)), None);
        assert!(matches!(t.touch_action(0, &crate::world::World::default()), Some(CameraAction::Rotate { degrees, .. }) if degrees == 90.0));
    }

    #[test]
    fn art_changes_and_middle_ground_offset() {
        let level = LevelString::parse("kS38,1;1,3029,2,100,3,0,533,53;1,3029,2,200,3,0,533,25;1,3029,2,300,3,0,533,53;1,3031,2,400,3,0,533,2;1,2999,2,500,3,0,29,24,10,0.5;");
        let t = CameraTriggers::from_level(&level);
        assert_eq!(t.art_ids(), vec![(0, 25), (0, 53), (2, 2)]);
        let mut c = Camera::new();
        c.apply(CameraAction::ChangeArt { kind: 0, id: 53 }, 569.0);
        c.apply(CameraAction::ChangeArt { kind: 2, id: 2 }, 569.0);
        assert_eq!(c.art, [53, 0, 2]);
        c.apply(CameraAction::MgOffset { y: 24.0, duration: 0.0, easing: 0, rate: 2.0 }, 569.0);
        assert_eq!(c.mg_offset, 24.0);
        c.apply(CameraAction::MgOffset { y: 48.0, duration: 0.5, easing: 1, rate: 2.0 }, 569.0);
        assert_eq!(c.mg_offset, 24.0, "a timed change tweens from the current offset");
        assert!(c.mg_tween.is_some());
    }
}

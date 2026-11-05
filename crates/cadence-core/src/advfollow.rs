//! Advanced follow (3016) and its edit (3660) / re-target (3661) triggers, after
//! `GJBaseGameLayer::triggerAdvancedFollowCommand` (0xf1584), `processAdvancedFollowAction`
//! (0xff8e4), `getFollowSpeedVal` (0xfef2c), `triggerAdvancedFollowEditCommand` (0xff0b0) and
//! `modifyObjectPhysics` (0xff7fc).
//!
//! A fired trigger becomes an instance (the same trigger again only restarts it). Every step,
//! after the moves and the player follows (`GJBaseGameLayer::update`), the instances run in
//! priority order (key 365, highest first); each object of the target group keeps a velocity and
//! a direction, which the instance's mode updates toward the followed target (a group's main
//! object, a player or the camera, optionally as it was `delay` seconds ago):
//! - mode 0: velocity = distance / easing (361; with easing ≤ 1 the object snaps);
//! - mode 1: friction (558) and acceleration (334) toward the target, both blending toward
//!   their "near" values (357 / 561) inside the near distance (359);
//! - mode 2: steering: the direction turns toward the target by at most the steer force (316,
//!   the low / high ones by speed), smoothed, braking (326) on sharp turns.
//!
//! Then the speed is capped (298), the object moves by velocity · dt and, with "rotate
//! direction" (339), turns to face its direction.
//!
//! Group parents and linked objects (parents.rs; `m_targetGroups` under
//! `getSpecialKey(group, 280, 281)`): the objects moved alone go first, then each cluster leader
//! is followed like an object and its whole cluster moves by the leader's step; a turn then
//! orbits the cluster about the leader's unmodified position through `rotateObjects` with the
//! step as its offset, so a turning cluster moves by twice the step (the game's own code, seen
//! in real traces too). Not ported: the game's per-object random variances
//! (`m_varianceValues`; a fixed hash here).

use crate::level::RawObject;
use crate::world::World;

/// Who an instance follows (`m_followTarget` of the instance: group, −1 player 1, −2 player 2,
/// −3 the camera).
pub const FOLLOW_P1: i32 = -1;
pub const FOLLOW_P2: i32 = -2;
pub const FOLLOW_CAMERA: i32 = -3;

/// An advanced follow trigger's values (`AdvancedFollowTriggerObject::customObjectSetup`
/// 0x14d9e8), each with its variance.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AdvSpec {
    pub order: u32,
    pub group: i32,
    pub follow: i32,
    pub control_id: i32,
    pub priority: i32,
    pub delay: (f32, f32),
    pub start_speed: (f32, f32),
    pub start_dir: (f32, f32),
    /// 560: the start direction is relative to this group's movement; 565: away from this
    /// group's main object; 572: 0 set the first time, 1 always set, 2 add.
    pub start_ref_move: i32,
    pub start_ref_away: i32,
    pub start_mode: i32,
    pub max_speed: (f32, f32),
    pub max_range: (f32, f32),
    pub accel: (f32, f32),
    pub x_only: bool,
    pub y_only: bool,
    pub ignore_disabled: bool,
    /// 339 rotate to the direction, 340 its offset, 363 the direction's easing, 364 the distance
    /// under which the direction stops following the velocity.
    pub rotate: bool,
    pub rotate_offset: f32,
    pub rot_easing: f32,
    pub rot_deadzone: f32,
    pub mode: i32,
    pub easing: (f32, f32),
    pub near_dist: (f32, f32),
    pub near_accel: (f32, f32),
    pub friction: (f32, f32),
    pub near_friction: (f32, f32),
    pub steer: (f32, f32),
    pub steer_low: (f32, f32),
    pub steer_high: (f32, f32),
    pub low_on: bool,
    pub high_on: bool,
    pub speed_low: (f32, f32),
    pub speed_high: (f32, f32),
    pub break_force: (f32, f32),
    pub break_angle: (f32, f32),
    pub break_steer: (f32, f32),
    pub break_limit: (f32, f32),
    /// 305: accelerate toward the target rather than along the direction (mode 2).
    pub target_dir: bool,
    /// 280 / 281: ignore group parents / linked objects (move every object alone).
    pub ignore_parent: bool,
    pub ignore_linked: bool,
    /// 571: an object it moved isn't moved again this step by lower-priority instances.
    pub exclusive: bool,
    /// Edit (3660): 566 / 567 x speed factor, 568 / 569 y; 535 by control id; re-target (3661).
    pub edit: bool,
    pub retarget: bool,
    pub by_control_id: bool,
    pub mod_x: (f32, f32),
    pub mod_y: (f32, f32),
}

impl AdvSpec {
    pub fn from_raw(raw: &RawObject, order: u32) -> AdvSpec {
        let f = |k: u32| raw.get_f32(k).unwrap_or(0.0);
        let p = |a: u32, b: u32| (f(a), f(b));
        let on = |k: u32| raw.get(k) == Some("1");
        let i = |k: u32| raw.get_i32(k).unwrap_or(0);
        let follow = if on(138) {
            FOLLOW_P1
        } else if on(200) {
            FOLLOW_P2
        } else if on(201) {
            FOLLOW_CAMERA
        } else {
            i(71)
        };
        AdvSpec {
            order,
            group: i(51),
            follow,
            control_id: i(534),
            priority: i(365),
            delay: p(292, 293),
            start_speed: p(300, 301),
            start_dir: p(563, 564),
            start_ref_move: i(560),
            start_ref_away: i(565),
            start_mode: i(572),
            max_speed: p(298, 299),
            max_range: p(308, 309),
            accel: p(334, 335),
            x_only: on(306),
            y_only: on(307),
            ignore_disabled: on(336),
            rotate: on(339),
            rotate_offset: f(340),
            rot_easing: f(363),
            rot_deadzone: f(364),
            mode: i(367),
            easing: p(361, 362),
            near_dist: p(359, 360),
            near_accel: p(357, 358),
            friction: p(558, 559),
            near_friction: p(561, 562),
            steer: p(316, 317),
            steer_low: p(318, 319),
            steer_high: p(320, 321),
            low_on: on(337),
            high_on: on(338),
            speed_low: p(322, 323),
            speed_high: p(324, 325),
            break_force: p(326, 327),
            break_angle: p(328, 329),
            break_steer: p(330, 331),
            break_limit: p(332, 333),
            target_dir: on(305),
            ignore_parent: on(280),
            ignore_linked: on(281),
            exclusive: on(571),
            edit: raw.id() == 3660,
            retarget: raw.id() == 3661,
            by_control_id: on(535),
            mod_x: (raw.get_f32(566).unwrap_or(1.0), f(567)),
            mod_y: (raw.get_f32(568).unwrap_or(1.0), f(569)),
        }
    }
}

/// A running instance (`m_advanceFollowInstances`, 0x20 bytes: trigger, group, follow target,
/// control id, order counter, paused / done flags, started).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvInstance {
    pub spec: u32,
    pub group: i32,
    pub follow: i32,
    pub control_id: i32,
    pub counter: u32,
    pub started: bool,
}

/// An object's follow state (`+0x430` map by unique id): velocity, direction, the angle the
/// direction eases to, the smoothed steer force, and the steps it was last processed in.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AdvObject {
    pub vel: (f32, f32),
    pub dir: (f32, f32),
    pub angle: f32,
    pub steer: f32,
    pub processed: u64,
    pub exclusive: u64,
}

/// All advanced follow state of a level.
#[derive(Clone, Debug, Default)]
pub struct AdvFollow {
    pub specs: std::sync::Arc<std::collections::HashMap<u32, AdvSpec>>,
    pub instances: Vec<AdvInstance>,
    counter: u32,
    pub objects: std::collections::HashMap<u32, AdvObject>,
    /// Positions of the followed targets each step, newest last (for delays), by target.
    history: std::collections::HashMap<i32, std::collections::VecDeque<(f32, f32)>>,
    /// The targets that need a history (followed with a delay).
    pub delayed_targets: Vec<i32>,
    rng: crate::fastrand::FastRand,
}

/// A stable value in [−1, 1] for an object's `k`-th variance (`m_varianceValues`).
fn variance(order: u32, k: u32) -> f32 {
    let mut h = order.wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x85eb_ca6b) ^ 0x2545_f491;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// `ccpForAngle`.
fn for_angle(a: f32) -> (f32, f32) {
    (a.cos(), a.sin())
}

/// Wrap an angle into [−half, half] the game's way (0x10010084c..): n = ceil(floor(|a| / half)
/// · 0.5) turns added with one rounding.
fn wrap(a: f32, half: f32) -> f32 {
    if a < -half {
        let n = ((a.abs() / half).floor() * 0.5).ceil();
        n.mul_add(half * 2.0, a)
    } else if half < a {
        let n = ((a / half).floor() * 0.5).ceil();
        n.mul_add(-(half * 2.0), a)
    } else {
        a
    }
}

fn wrap_pi(a: f32) -> f32 {
    wrap(a, std::f32::consts::PI)
}

fn wrap_180(a: f32) -> f32 {
    wrap(a, 180.0)
}

/// `ccpLength` (x² fused onto y²).
fn length(v: (f32, f32)) -> f32 {
    v.0.mul_add(v.0, v.1 * v.1).sqrt()
}

/// `ccpNormalize` (0x1002d2fa8): a zero vector becomes (1, 0).
fn normalize(v: (f32, f32)) -> (f32, f32) {
    let l = length(v);
    if l == 0.0 { (1.0, 0.0) } else { (v.0 / l, v.1 / l) }
}

impl AdvFollow {
    pub fn new(level: &crate::level::LevelString) -> AdvFollow {
        let specs: std::collections::HashMap<u32, AdvSpec> = level
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| matches!(o.id(), 3016 | 3660 | 3661))
            .map(|(i, o)| (i as u32, AdvSpec::from_raw(o, i as u32)))
            .collect();
        let mut delayed: Vec<i32> = specs.values().filter(|s| !s.edit && !s.retarget && (s.delay.0 > 0.0 || s.delay.1 != 0.0)).map(|s| s.follow).collect();
        delayed.sort_unstable();
        delayed.dedup();
        AdvFollow { specs: std::sync::Arc::new(specs), delayed_targets: delayed, rng: crate::fastrand::FastRand::new(0x1234_5678), ..Default::default() }
    }

    /// Seed its `fast_rand` (`fast_srand`).
    pub fn reseed(&mut self, seed: u64) {
        self.rng = crate::fastrand::FastRand::new(seed);
    }

    /// `GameToolbox::fast_rand_0_1` (the game's generator; the game seeds it from the clock).
    fn rand01(&mut self) -> f32 {
        self.rng.unit()
    }

    /// `triggerAdvancedFollowCommand`: a new instance, or the same trigger's restarted.
    pub fn fire(&mut self, order: u32) {
        let Some(spec) = self.specs.get(&order).copied() else { return };
        self.counter += 1;
        let counter = self.counter;
        let mut found = false;
        for inst in &mut self.instances {
            if inst.spec == order && inst.group == spec.group && inst.follow == spec.follow && inst.control_id == spec.control_id {
                inst.counter = counter;
                found = true;
            }
        }
        if !found {
            self.instances.push(AdvInstance { spec: order, group: spec.group, follow: spec.follow, control_id: spec.control_id, counter, started: false });
        }
        // priority first (365, highest first), then the trigger's order
        let specs = std::sync::Arc::clone(&self.specs);
        self.instances.sort_by(|a, b| {
            let (pa, pb) = (specs.get(&a.spec).map_or(0, |s| s.priority), specs.get(&b.spec).map_or(0, |s| s.priority));
            pb.cmp(&pa).then(a.spec.cmp(&b.spec))
        });
    }

    /// Record this step's positions of the delayed targets (`position` gives a target's
    /// position now, `None` when it has none).
    pub fn record(&mut self, position: &dyn Fn(i32) -> Option<(f32, f32)>) {
        for &t in &self.delayed_targets {
            let h = self.history.entry(t).or_default();
            h.push_back(position(t).unwrap_or((0.0, 0.0)));
            if h.len() > 2400 {
                h.pop_front();
            }
        }
    }

    /// `FUN_1000e7310`: the target's position `delay` seconds ago (interpolated between
    /// steps), or now; `None` without enough history.
    fn target_at(&self, target: i32, delay: f32, now: Option<(f32, f32)>) -> Option<(f32, f32)> {
        if delay <= 0.0 {
            return now;
        }
        // delay / (1/240) in f64, rounded up (`fcvtps`): the sample that many steps back, moved
        // toward the next newer one by the rounding
        let f = ((delay as f64 / (1.0f64 / 240.0)).max(0.0)) as f32;
        let i = f.ceil() as usize;
        let h = self.history.get(&target)?;
        if i >= h.len() {
            return None;
        }
        let back = |k: usize| h[h.len() - 1 - k];
        let a = back(i);
        let b = back(i.saturating_sub(1));
        let t = i as f32 - f;
        Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
    }
}

impl World {
    /// An advanced follow trigger fired.
    pub(crate) fn adv_fire(&mut self, order: u32) {
        self.adv.fire(order);
    }

    /// Edit (3660) / re-target (3661) advanced follow triggers.
    pub(crate) fn adv_edit(&mut self, order: u32) {
        let Some(spec) = self.adv.specs.get(&order).copied() else { return };
        if spec.retarget {
            let target = spec.follow;
            if target == 0 {
                return;
            }
            if spec.by_control_id {
                for inst in &mut self.adv.instances {
                    if inst.control_id == spec.group {
                        inst.follow = target;
                    }
                }
            } else {
                // the advanced follow triggers in group 51
                let triggers: Vec<u32> = self.group_members(spec.group).cloned().unwrap_or_default();
                for inst in &mut self.adv.instances {
                    if triggers.contains(&inst.spec) {
                        inst.follow = target;
                    }
                }
            }
            return;
        }
        // edit: modifyObjectPhysics on the objects of group 51 (or of the instances with that
        // control id)
        let objects: Vec<u32> = if spec.by_control_id {
            let groups: Vec<i32> = self.adv.instances.iter().filter(|i| i.control_id == spec.group).map(|i| i.group).collect();
            groups.iter().flat_map(|g| self.group_members(*g).cloned().unwrap_or_default()).collect()
        } else {
            self.group_members(spec.group).cloned().unwrap_or_default()
        };
        for o in objects {
            if !self.adv.objects.contains_key(&o) {
                continue;
            }
            let (r1, r2) = (self.adv.rand01(), self.adv.rand01());
            let mx = spec.mod_x.0 + r1 * spec.mod_x.1;
            let my = spec.mod_y.0 + r2 * spec.mod_y.1;
            let speed = spec.start_speed.0 + self.adv.rand01() * spec.start_speed.1;
            let add = if speed != 0.0 {
                let dir = spec.start_dir.0 + self.adv.rand01() * spec.start_dir.1;
                Some(self.follow_speed_val(dir, speed, o, spec.start_ref_move, spec.start_ref_away))
            } else {
                None
            };
            let st = self.adv.objects.get_mut(&o).unwrap();
            st.vel.0 *= mx;
            st.vel.1 *= my;
            if let Some((ax, ay)) = add {
                if !spec.x_only {
                    st.vel.1 += ay;
                }
                if !spec.y_only {
                    st.vel.0 += ax;
                }
            }
        }
    }

    /// `getFollowSpeedVal`: a velocity of `speed` at `angle` degrees (0 up, clockwise), turned
    /// by the movement of group `by_move`'s main object (its last position − now) or away from
    /// group `away`'s.
    fn follow_speed_val(&mut self, angle: f32, speed: f32, obj: u32, by_move: i32, away: i32) -> (f32, f32) {
        let mut r = (0.0f32, 0.0f32);
        let by_move = if by_move > 0 { self.try_get_object(by_move) } else { None };
        let away = if by_move.is_none() && away > 0 { self.try_get_object(away) } else { None };
        if let Some(m) = by_move {
            let (lx, ly) = self.level_last_of(m as usize);
            let (x, y) = self.level_pos_of(m as usize);
            r = (lx - x as f32, ly - y as f32);
        } else if let Some(m) = away {
            let (x, y) = self.level_pos_of(obj as usize);
            let (ax, ay) = self.level_pos_of(m as usize);
            r = (x as f32 - ax as f32, y as f32 - ay as f32);
        }
        let mut a = (90.0 - angle) * 0.017453292;
        if r.0 != 0.0 || r.1 != 0.0 {
            a = a + r.1.atan2(r.0) + 1.5707964;
        }
        let d = for_angle(a);
        (d.0 * speed, d.1 * speed)
    }

    /// `tryGetObject`: the group's main object, else one of its objects at random
    /// (`fast_rand_0_1`).
    fn try_get_object(&mut self, group: i32) -> Option<u32> {
        if let Some(o) = self.main_object(group) {
            return Some(o);
        }
        let n = self.group_members(group).map_or(0, |m| m.len());
        if n == 0 {
            return None;
        }
        let r = self.adv.rand01();
        let r = if r != 1.0 { r } else { 0.0 };
        self.group_members(group).and_then(|m| m.get((r * n as f32) as usize)).copied()
    }

    /// A follow target's position now (`FUN_1000e7310` with no delay): a group's main object
    /// (`tryGetMainObject`: its parent, or its only object).
    fn adv_target_now(&self, target: i32) -> Option<(f32, f32)> {
        match target {
            FOLLOW_P1 => self.last_player_pos(),
            FOLLOW_P2 => self.player2_pos,
            FOLLOW_CAMERA => Some(self.camera_pos),
            g => {
                let o = self.main_object(g)?;
                let (x, y) = self.level_pos_of(o as usize);
                Some((x as f32, y as f32))
            }
        }
    }

    /// `processAdvancedFollowActions` for one step (dt in 240 Hz steps).
    pub(crate) fn process_adv_follow(&mut self) {
        if self.adv.instances.is_empty() && self.adv.delayed_targets.is_empty() {
            return;
        }
        let targets = self.adv.delayed_targets.clone();
        let now: Vec<(i32, Option<(f32, f32)>)> = targets.iter().map(|&t| (t, self.adv_target_now(t))).collect();
        self.adv.record(&|t| now.iter().find(|n| n.0 == t).and_then(|n| n.1));
        let dt = (self.step_secs * 240.0) as f32;
        let step = self.step_count_pub();
        let instances = self.adv.instances.clone();
        for inst in &instances {
            let Some(spec) = self.adv.specs.get(&inst.spec).copied() else { continue };
            match self.links.clusters(inst.group, spec.ignore_parent, spec.ignore_linked).cloned() {
                None => {
                    let members: Vec<u32> = self.group_members(inst.group).cloned().unwrap_or_default();
                    for o in members {
                        self.adv_object(&spec, inst, o, dt, step, None);
                    }
                }
                Some(c) => {
                    // pass 0: the objects moved alone; pass 1: the leaders with their clusters
                    for &o in &c.alone {
                        self.adv_object(&spec, inst, o, dt, step, None);
                    }
                    for &o in &c.leaders {
                        self.adv_object(&spec, inst, o, dt, step, c.members.get(&o).map(|m| m.as_slice()));
                    }
                }
            }
        }
    }

    /// One object (or a cluster leader with its cluster) of an instance.
    fn adv_object(&mut self, s: &AdvSpec, inst: &AdvInstance, o: u32, dt: f32, step: u64, cluster: Option<&[u32]>) {
        let v = |k: u32, x: (f32, f32)| if x.1 != 0.0 { x.1 * variance(o, k) } else { 0.0 };
        let i = o as usize;
        let st0 = self.adv.objects.get(&o).copied().unwrap_or_default();
        if st0.exclusive >= step && st0.exclusive != 0 {
            return;
        }
        // the target, delayed
        let delay = s.delay.0 + v(0, s.delay);
        let now = self.adv_target_now(inst.follow);
        let Some(target) = self.adv.target_at(inst.follow, delay, now) else { return };
        if (s.ignore_disabled && !self.enabled.get(i).copied().unwrap_or(true)) || target == (0.0, 0.0) {
            return;
        }
        let (px, py) = self.level_pos_of(i);
        let (px, py) = (px as f32, py as f32);
        let mut delta = (target.0 - px, target.1 - py);
        let max_speed = s.max_speed.0 + v(1, s.max_speed);
        let mut accel = s.accel.0 + v(2, s.accel);
        let range = s.max_range.0 + v(3, s.max_range);
        let dist = length(delta);
        if s.x_only {
            delta.1 = 0.0;
        } else if s.y_only {
            delta.0 = 0.0;
        }
        if range != 0.0 && range < dist {
            return;
        }
        let mut st = st0;
        // the start speed: the instance's first object only (the instance's +0x1f flag is set
        // after it), set the first time the object is followed, or per 572
        if !self.adv.instances.iter().any(|x| x.spec == inst.spec && x.started) {
            let speed = s.start_speed.0 + self.adv.rand01() * s.start_speed.1;
            if speed != 0.0 {
                let dir = s.start_dir.0 + self.adv.rand01() * s.start_dir.1;
                let (ax, ay) = self.follow_speed_val(dir, speed, o, s.start_ref_move, s.start_ref_away);
                let set = st.processed == 0 || s.start_mode == 1;
                if set || s.start_mode == 2 {
                    if !s.x_only {
                        st.vel.1 = if set { ay } else { st.vel.1 + ay };
                    }
                    if !s.y_only {
                        st.vel.0 = if set { ax } else { st.vel.0 + ax };
                    }
                }
            }
        }
        if let Some(x) = self.adv.instances.iter_mut().find(|x| x.spec == inst.spec) {
            x.started = true;
        }
        st.processed = step;
        if s.exclusive {
            st.exclusive = step;
        }
        accel *= 0.01;
        let mut move_dt = dt;
        let set_v = |st: &mut AdvObject, nv: (f32, f32)| {
            if !s.x_only {
                st.vel.1 = nv.1;
            }
            if !s.y_only {
                st.vel.0 = nv.0;
            }
        };
        if s.mode == 2 {
            self.adv_steer(s, &mut st, o, delta, accel, dt);
        } else {
            if s.mode == 1 {
                let near = s.near_dist.0 + v(6, s.near_dist);
                let mut friction = (s.friction.0 + v(7, s.friction)) * 0.01;
                let unit = normalize(delta);
                if 0.0 < near && dist < near {
                    let t = 1.0 - dist / near;
                    accel = t.mul_add((s.near_accel.0 + v(8, s.near_accel)) * 0.01 - accel, accel);
                    friction = t.mul_add((s.near_friction.0 + v(9, s.near_friction)) * 0.01 - friction, friction);
                }
                if friction != 0.0 {
                    let k = friction * dt;
                    let nv = (st.vel.0 - st.vel.0 * k, st.vel.1 - st.vel.1 * k);
                    set_v(&mut st, nv);
                }
                let k = accel * dt;
                let nv = (st.vel.0 + unit.0 * k, st.vel.1 + unit.1 * k);
                set_v(&mut st, nv);
            } else {
                // mode 0: distance over the easing
                let e = s.easing.0 + v(6, s.easing);
                let k = if e <= 1.0 {
                    move_dt = 1.0;
                    1.0
                } else {
                    1.0 / e
                };
                set_v(&mut st, (delta.0 * k, delta.1 * k));
            }
            // the direction eases toward the velocity's angle (frozen inside the deadzone)
            if s.rot_deadzone <= dist {
                st.angle = st.vel.1.atan2(st.vel.0);
            }
            let cur = st.dir.1.atan2(st.dir.0);
            let diff = wrap_pi(st.angle - cur);
            let e = if s.rot_easing <= 1.0 { 1.0 } else { s.rot_easing };
            st.dir = for_angle(cur + diff / e);
        }
        // turn a quarter of the way to face the direction each step
        let mut turn = 0.0f32;
        if s.rotate {
            let a = st.dir.1.atan2(st.dir.0);
            turn = wrap_180((a.mul_add(-57.29578, 90.0) + s.rotate_offset) - self.turned_of(i)) * 0.25;
        }
        // the speed cap
        let cap = max_speed;
        if cap != 0.0 && !(cap < 0.0) {
            if s.x_only {
                let a = st.vel.0.abs();
                if a != cap && cap <= a {
                    st.vel.0 = (cap / a) * st.vel.0;
                }
            } else if s.y_only {
                let a = st.vel.1.abs();
                if a != cap && cap <= a {
                    st.vel.1 = (cap / a) * st.vel.1;
                }
            } else {
                let a = length(st.vel);
                if a != cap && cap <= a {
                    let k = cap / a;
                    st.vel = (st.vel.0 * k, st.vel.1 * k);
                }
            }
        }
        self.adv.objects.insert(o, st);
        let mut m = (st.vel.0 * move_dt, st.vel.1 * move_dt);
        if s.x_only {
            m.1 = 0.0;
        } else if s.y_only {
            m.0 = 0.0;
        }
        let moved = m.0 != 0.0 || m.1 != 0.0;
        let Some(cluster) = cluster else {
            if moved {
                let (x, y) = self.level_pos_of(i);
                self.set_level_pos_pub(i, (x + m.0 as f64, y + m.1 as f64));
            }
            if turn != 0.0 && self.can_rotate_free(i) {
                self.turn_object(i, turn);
            }
            return;
        };
        // a leader: `moveObjects` on its cluster, then `rotateObjects` about the leader's
        // unmodified position with the step as the offset (moving the cluster a second time)
        if moved {
            for &k in cluster {
                let (x, y) = self.level_pos_of(k as usize);
                self.set_level_pos_pub(k as usize, (x + m.0 as f64, y + m.1 as f64));
            }
        }
        if turn != 0.0 {
            let center = self.unmodified_pos(i);
            let offset = if moved { m } else { (0.0, 0.0) };
            self.rotate_objects(cluster, turn, center, offset);
        }
    }

    /// Mode 2: steering (`processAdvancedFollowAction`, the `+0x818 == 2` branch).
    fn adv_steer(&mut self, s: &AdvSpec, st: &mut AdvObject, o: u32, delta: (f32, f32), accel: f32, dt: f32) {
        let v = |k: u32, x: (f32, f32)| if x.1 != 0.0 { x.1 * variance(o, k) } else { 0.0 };
        let to = normalize(delta);
        if st.dir == (0.0, 0.0) {
            st.dir = to;
        }
        let steer = s.steer.0 + v(9, s.steer);
        let speed = length(st.vel).abs();
        let low = s.speed_low.0 + v(12, s.speed_low);
        let force = if !s.low_on || low < speed {
            let high = s.speed_high.0 + v(13, s.speed_high);
            if s.high_on && 0.0 < high && high <= speed { s.steer_high.0 + v(11, s.steer_high) } else { steer }
        } else {
            s.steer_low.0 + v(10, s.steer_low)
        };
        let cur = st.dir.1.atan2(st.dir.0);
        let diff = wrap_pi(to.1.atan2(to.0) - cur);
        let brk = (s.break_force.0 + v(14, s.break_force)) * 0.01;
        let brk_angle = (s.break_angle.0 + v(15, s.break_angle)) * 0.017453292;
        let mut force = force;
        if 0.0 < brk {
            let limit = s.break_limit.0 + v(16, s.break_limit);
            if limit < speed && diff.abs() > brk_angle {
                force = s.break_steer.0 + v(16, s.break_steer);
            }
        }
        st.steer += (force * 0.01 - st.steer) / 10.0;
        let lim = st.steer;
        let turn = if 0.0 <= diff { diff.min(lim) } else { diff.max(-lim) };
        if turn != 0.0 {
            let a = turn * dt;
            let (sin, cos) = a.sin_cos();
            let (vx, vy) = st.vel;
            if !s.x_only {
                st.vel.1 = vy.mul_add(cos, vx * sin);
            }
            if !s.y_only {
                st.vel.0 = vx.mul_add(cos, -(vy * sin));
            }
            st.dir = for_angle(a + cur);
        }
        let mut accel = accel;
        if 0.0 < brk && brk_angle < (diff - turn).abs() {
            let bx = (brk * st.dir.0 * dt).abs();
            let by = (brk * st.dir.1 * dt).abs();
            let (vx, vy) = st.vel;
            if !s.x_only {
                st.vel.1 = vy - vy.clamp(-by, by);
            }
            if !s.y_only {
                st.vel.0 = vx - vx.clamp(-bx, bx);
            }
            accel = 0.0;
        }
        let along = if s.target_dir { to } else { st.dir };
        if !s.x_only {
            st.vel.1 = (accel * along.1).mul_add(dt, st.vel.1);
        }
        if !s.y_only {
            st.vel.0 = (accel * along.0).mul_add(dt, st.vel.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LevelString;

    fn world(objs: &str) -> World {
        World::from_level(&LevelString::parse(&format!("kA2,0;{objs}")))
    }

    #[test]
    fn mode0_eases_toward_the_target() {
        // object 1 (group 2) follows group 3's main object at (400, 300) with easing 10
        let mut w = world("1,1,2,100,3,100,57,2;1,1,2,400,3,300,57,3;1,3016,2,0,3,0,51,2,71,3,367,0,361,10");
        let start = w.level_position(0);
        w.adv_fire(2);
        w.pre_step((0.0, 0.0));
        let p = w.level_position(0);
        // a tenth of the way per step
        assert!(((p.0 - start.0) - 30.0).abs() < 1e-3, "{p:?} {start:?}");
        assert!(((p.1 - start.1) - 20.0).abs() < 1e-3);
        for _ in 0..400 {
            w.pre_step((0.0, 0.0));
        }
        let p = w.level_position(0);
        assert!((p.0 - 400.0).abs() < 0.01 && (p.1 - 390.0).abs() < 0.01, "{p:?}");
    }

    #[test]
    fn max_speed_caps_and_x_only() {
        let mut w = world("1,1,2,100,3,100,57,2;1,1,2,400,3,300,57,3;1,3016,2,0,3,0,51,2,71,3,367,0,361,1,298,2,306,1");
        let start = w.level_position(0);
        w.adv_fire(2);
        w.pre_step((0.0, 0.0));
        let p = w.level_position(0);
        assert!(((p.0 - start.0) - 2.0).abs() < 1e-4, "{p:?}");
        assert_eq!(p.1, start.1);
    }

    #[test]
    fn mode1_accelerates() {
        let mut w = world("1,1,2,100,3,100,57,2;1,1,2,400,3,100,57,3;1,3016,2,0,3,0,51,2,71,3,367,1,334,100");
        let start = w.level_position(0);
        w.adv_fire(2);
        w.pre_step((0.0, 0.0));
        let a = w.level_position(0).0 - start.0;
        w.pre_step((0.0, 0.0));
        let b = w.level_position(0).0 - start.0;
        // velocity 1, then 2 units per step
        assert!((a - 1.0).abs() < 1e-4 && (b - 3.0).abs() < 1e-4, "{a} {b}");
    }

    #[test]
    fn retarget_switches_the_followed_group() {
        let mut w = world("1,1,2,100,3,100,57,2;1,1,2,400,3,100,57,3;1,1,2,100,3,500,57,4;1,3016,2,0,3,0,51,2,71,3,57,9;1,3661,2,0,3,0,51,9,71,4");
        w.adv_fire(3);
        w.adv_edit(4);
        assert_eq!(w.adv.instances[0].follow, 4);
    }
}

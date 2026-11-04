//! `PlayerObject::collidedWithObjectInternal` (m1 0x371ff4), ported from the instructions for
//! the classic case: not platformer, not sideways, no swing/spider, no 2.2 player-state
//! triggers (hit head, flip gravity, no wall push, dart slide), no scaled objects.
//!
//! The block is tested against its top side, then its bottom side. Positions and the snap
//! distance are compared in f64 from f32 values exactly as the game does; the snap distance is
//! 10 (6 for ship/ufo/wave) signed by gravity, grown by the block's speed when it moved toward
//! the player's gravity side this step (its velocity is then `m_groundYVelocity`).

use crate::objects::ObjectType;
use crate::physics::{round_velocity, Mode, Player};
use crate::world::{Rect, Shape, World};

/// The time window of the gravity-flip grace (`0x3fb999999a000000`).
const FLIP_GRACE: f64 = 0.10000000149011612;

/// `rotateGameplayObject` (m1 0x370868)'s turn: cos(−90°) as the game's f32 constant.
const TURN_COS: f32 = -4.371139e-08;

/// An object as `rotateGameplayObject` leaves it for a sideways collision: turned −90° about
/// the player (f32), its f64 position moved by the same delta (`m_positionX/Y` += delta, then
/// the f32 position from it), its last position moved by the delta (the movement vector keeps
/// its unturned direction), its rotation −90° and its rect rebuilt from that.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Turned {
    pub pos: (f32, f32),
    pub last: (f32, f32),
    pub rect: Rect,
}

/// What the potential-slope scan found (`m_potentialSlopes`, plus the slope snapped to this
/// pass): `any` (bVar7) a slope that meets the block and the player; `under` (bVar13) one whose
/// surface is at or above the block's top (+2), `over` (bVar35) one whose surface is at or below
/// the block's bottom (−2).
#[derive(Default)]
struct SlopeScan {
    any: bool,
    under: bool,
    over: bool,
}

impl Player {
    /// `collidedWithObjectInternal(dt, obj, rect)`. `obj` is the object whose rect is used (the
    /// game's plVar1: snap jumps, breakable, collision ids); `owner` is the object passed in
    /// (param_3: its movement and the ground object). A slope's flat side passes the slope as
    /// `owner` with its own rect and no `obj`.
    pub(crate) fn collided_with_object_internal(
        &mut self,
        world: &World,
        obj: Option<usize>,
        owner: Option<usize>,
        b: Rect,
        dt: f32,
        turned: Option<&Turned>,
    ) {
        let start = self.rect();
        let destroyed = self.destroyed.len();
        self.collided_vertical(world, obj, owner, b, dt, turned);
        // Platformer: every contact that didn't break a block goes on to the side test
        // (joined_r0x000100372bd4), with the rect the player had on entry.
        if self.platformer.on && self.destroyed.len() == destroyed && !self.dead {
            self.platformer_side(world, obj, owner, b, dt, start);
        }
    }

    fn collided_vertical(
        &mut self,
        world: &World,
        obj: Option<usize>,
        owner: Option<usize>,
        b: Rect,
        dt: f32,
        turned: Option<&Turned>,
    ) {
        let up = self.upside_down;
        let mode = self.mode;
        let flyer = matches!(mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing);
        let p = self.rect();
        let h = p.h;
        let y = self.y;

        // Snap distance (d8): 10 (6 for ship/ufo/wave) signed by gravity, plus the slope center
        // offset just after a slope.
        let dir: i32 = if up { -1 } else { 1 };
        let mut snap = self.snap_distance() * dir as f64;
        if flyer && !self.platformer.on {
            snap = ((dir * 6) as f32) as f64;
        }
        if self.was_on_slope {
            snap = self.slope_center_offset(world).mul_add(dir as f64, snap);
        }

        // The object's movement since its last position (moved toward the gravity side: the
        // block pushes into the player).
        let mut fast = false; // bVar14: |v| > 5
        let mut still = true; // bVar10: didn't move away
        let mut block_v = 0.0f64; // local_140
        let mut static_ = true; // bVar9: didn't move toward the player
        if dt > 0.0 {
            if let Some(m) = owner {
                let o = &world.objects[m];
                let (last, cur) = match turned {
                    Some(t) => (t.last, t.pos),
                    None => (o.last_pos, (o.pos.0 as f32, o.pos.1 as f32)),
                };
                // Sideways the "up" axis is x (the turned frame keeps the unturned movement).
                let (s0, c) = if self.sideways { (last.0, cur.0) } else { (last.1, cur.1) };
                // + getScalePosDelta (0 without scale triggers)
                if !(cur.0 == last.0 && cur.1 == last.1) {
                    if (!up && s0 < c) || (up && s0 > c) {
                        let v = (c - s0) / dt;
                        let av = v.abs();
                        fast = av > 5.0;
                        snap = (av as f64).mul_add(if up { -1.0 } else { 1.0 }, snap);
                        block_v = v as f64;
                        static_ = false;
                        if !self.platformer.on {
                            self.ground_y_velocity = block_v;
                        }
                    } else {
                        still = !(if up { s0 < c } else { s0 > c });
                    }
                }
            }
        }

        // The player isn't turned: sideways its "up" movement is along x.
        let dy = if self.sideways { self.x - self.last_x } else { y - self.last_y };
        let bmax_y = b.max_y();
        let bmin_y = b.y;
        let (dmax_y, dmin_y) = (bmax_y as f64, bmin_y as f64);
        let breakable = obj.is_some_and(|i| world.objects[i].kind == ObjectType::BREAKABLE);
        // bVar32: a passable object (key 134) only takes contacts on the player's feet side.
        let passable = obj.is_some_and(|i| world.objects[i].passable);
        let id: i64 = obj.map_or(0, |i| i as i64 + 1);

        // The wave (without the dart-slide state) neither lands nor slides: only the kill test.
        if mode == Mode::Wave && self.state_dart_slide < 1 {
            self.inner_kill(world, obj, &b, false);
            return;
        }

        // Feet: the side gravity pulls toward (bottom, or top upside down).
        let feet = ((-0.5f32 * h).mul_add(if up { -1.0 } else { 1.0 }, y)) as f64;
        let scan = self.scan_potential_slopes(world, obj, &b);
        let d12 = dy as f64;
        let d13 = snap + feet;
        let d14 = d13 - d12;

        // Which sides the mode may meet: right side up every mode tests the block's top;
        // upside down only ship/ufo/wave/ball do (bVar26). The cube can snap under a block only
        // upside down (landing); ship/ufo/wave/ball also right side up (bVar29).
        let top_allowed = !up || (!passable && matches!(mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Ball | Mode::Swing));
        // bVar25: the hit-head or flip-gravity state (special blocks 1859/2866) lets every mode
        // meet both sides, stopping against the far one without hitGround.
        // In platformer every mode meets both sides.
        let head_state = (self.state_hit_head > 0 || self.state_flip_gravity > 0 || self.platformer.on) && !passable;
        let snap_jump = mode == Mode::Cube;
        // bVar34: the player is on the far side from its feet (head first into the block).
        let head = if up { !(d13 < dmax_y && d14 < dmax_y) } else { !(d13 > dmin_y && d14 > dmin_y) };
        // Platformer (0x100372844): only a block under the player's rect narrowed by 2.5 on
        // each side is met on top or bottom; anything else is a side contact.
        // The moving-platform snap's forced call (skip check) goes straight on.
        if self.platformer.on && !self.forced_contact {
            let narrow = Rect { x: 2.5 + p.x, y: p.y, w: p.w - 5.0, h: p.h };
            if !narrow.intersects(&b) {
                return;
            }
        }

        // Head side (top, or bottom upside down), minus the snap distance.
        let head_y = ((-0.5f32 * h).mul_add(if up { 1.0 } else { -1.0 }, y)) as f64;
        let d8 = head_y - snap;
        let d9 = d8 - d12;

        if top_allowed || head_state {
            let (d0, d1) = if up { (d9, d8) } else { (d14, d13) };
            if !(d1 < dmax_y && d0 < dmax_y) {
                // The block's top side.
                if up && mode == Mode::Ufo && breakable {
                    if b.intersects(&self.inner_rect()) {
                        self.destroyed.push(obj.unwrap());
                    }
                    return;
                }
                let v = self.y_velocity;
                let land = !(scan.any && scan.under) && (v <= 0.0 || self.was_on_slope || fast);
                if land {
                    self.y = bmax_y + h * 0.5;
                    if snap_jump {
                        if let Some(i) = obj {
                            self.snap_jump_to(world, i, turned.map(|t| t.pos));
                        }
                    }
                    let stop = !top_allowed && head_state;
                    self.contact(owner, id, head, breakable || passable, block_v, stop);
                    self.final_velocity(head, static_, snap_jump);
                } else if !(head || up || static_) {
                    // A block rising into a player moving up (faster than it): carried on top.
                    self.y = bmax_y + h * 0.5;
                    let v = self.y_velocity;
                    self.hit_ground_full();
                    self.landed_on(owner, id, block_v);
                    let v = round_velocity(v);
                    self.y_velocity = round_velocity(if v < block_v { block_v } else { v });
                } else if head && up && !still {
                    // Upside down, head on a block that moved away (up): pushed along at least as
                    // fast as the last ground velocity + 1.
                    self.y = bmax_y + h * 0.5;
                    let v = self.y_velocity as f32;
                    self.hit_ground_keep();
                    if !breakable && !passable && id != 0 {
                        self.last_collision_top = id;
                    }
                    let v = round_velocity(v as f64);
                    let floor = self.ground_y_velocity + 1.0;
                    self.y_velocity = round_velocity(if v < floor { floor } else { v });
                    self.head_flip();
                }
                return;
            }
        }

        // The block's bottom side.
        let bottom_allowed = up || (!passable && matches!(mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Ball | Mode::Swing));
        if !(bottom_allowed || head_state) {
            self.inner_kill(world, obj, &b, head);
            return;
        }
        let (d0, d1) = if up { (d14, d13) } else { (d9, d8) };
        if d1 > dmin_y && d0 > dmin_y {
            // Deeper than the snap distance: only the block met on this side last step counts.
            let prev = if up { self.prev_collision_bottom } else { self.prev_collision_top };
            if id != prev {
                self.inner_kill(world, obj, &b, head);
                return;
            }
        }
        if !up && mode == Mode::Ufo && breakable {
            if b.intersects(&self.inner_rect()) {
                self.destroyed.push(obj.unwrap());
            }
            return;
        }
        let v = self.y_velocity;
        let under = !(scan.any && scan.over) && (v >= 0.0 || self.was_on_slope || fast);
        if under {
            self.y = bmin_y + h * -0.5;
            if snap_jump {
                if let Some(i) = obj {
                    self.snap_jump_to(world, i, turned.map(|t| t.pos));
                }
            }
            let stop = !bottom_allowed && head_state;
            self.contact(owner, id, head, breakable || passable, block_v, stop);
            self.final_velocity(head, static_, snap_jump);
        } else if up && !static_ && !head {
            // Upside down, a block coming down onto a player moving away from it (slower than
            // it): carried along at min(v, block velocity).
            self.y = bmin_y + h * -0.5;
            self.hit_ground_full();
            self.landed_on(owner, id, block_v);
            let v = round_velocity(v);
            self.y_velocity = round_velocity(if v > block_v { block_v } else { v });
        } else if head && !still && !up {
            // Head under a block that moved away (down): pushed down at least as fast as the
            // last ground velocity − 1.
            self.y = bmin_y + h * -0.5;
            self.hit_ground_keep();
            if !breakable && !passable && id != 0 {
                self.last_collision_top = id;
            }
            let v = round_velocity(v);
            let ceil = self.ground_y_velocity - 1.0;
            self.y_velocity = round_velocity(if v > ceil { ceil } else { v });
            self.head_flip();
        }
    }

    /// `rotateGameplayObject` (0x370868) for object `index` (see `Turned`).
    pub(crate) fn turn_object(&self, world: &World, index: usize) -> Turned {
        let o = &world.objects[index];
        let (px, py) = (self.x, self.y);
        let (dx, dy) = (o.x - px, o.y - py);
        let nx = px + dx.mul_add(TURN_COS, -dy);
        let ny = dy.mul_add(TURN_COS, py + dx);
        let d = (nx - o.x, ny - o.y);
        let pos = ((o.pos.0 + d.0 as f64) as f32, (o.pos.1 + d.1 as f64) as f32);
        let last = (o.last_pos.0 + d.0, o.last_pos.1 + d.1);
        let rect = match crate::objects::info(o.id) {
            Some(info) => Shape::new(info, o.flip_x, o.flip_y, o.rotation - 90.0, o.scale).at(pos.0, pos.1).0,
            None => {
                let r = o.rect;
                let (cx, cy) = (r.x + r.w * 0.5 + d.0, r.y + r.h * 0.5 + d.1);
                Rect { x: cx - r.h * 0.5, y: cy - r.w * 0.5, w: r.h, h: r.w }
            }
        };
        Turned { pos, last, rect }
    }

    /// The snap distance (unsigned): 10, or 5 in platformer off slopes; 15 with the no-wall-push
    /// state (not modelled).
    pub(crate) fn snap_distance(&self) -> f64 {
        if self.platformer.on && !self.was_on_slope && !self.on_slope { 5.0 } else { 10.0 }
    }

    /// The platformer side test (0x1003732a0): a block level with the player's top or bottom
    /// (minus the snap distance) that the player's rect, shrunk 2.5 at top and bottom, touches
    /// (or the block last met on a side) stops the player beside it: on its right side when the
    /// player is right of its center (or it's `m_collidingWithLeft`), else on its left side.
    /// `start` is the player's rect on entry.
    fn platformer_side(&mut self, world: &World, obj: Option<usize>, owner: Option<usize>, b: Rect, dt: f32, start: Rect) {
        let (w, h) = (start.w, start.h);
        let y = self.y;
        // The block's x movement since its last position: (moved right or not, moved left or not).
        let (mut not_left, mut not_right) = (true, true);
        if dt > 0.0 {
            if let Some(m) = owner {
                let o = &world.objects[m];
                let d = o.pos.0 as f32 - o.last_pos.0;
                not_left = 0.0 <= d;
                not_right = d <= 0.0;
                let _ = not_left;
            }
        }
        let (top, bottom) = if self.on_slope || self.was_on_slope || self.total_time - self.slope_end_time_pub() < 0.20000000298023224 {
            let dir = if self.upside_down { -1.0f32 } else { 1.0 };
            ((h * 0.5).mul_add(dir, y) as f64, y as f64)
        } else {
            let snap = self.snap_distance();
            ((y + h * 0.5) as f64 - snap, snap + (y - h * 0.5) as f64)
        };
        let left_of = owner.is_some() && self.platformer.colliding_left == owner;
        let right_of = owner.is_some() && self.platformer.colliding_right == owner;
        if obj.is_some_and(|i| world.objects[i].passable) {
            return;
        }
        let shrunk = Rect { x: start.x, y: 2.5 + start.y, w: start.w, h: start.h - 5.0 };
        if !(shrunk.intersects(&b) || left_of || right_of) {
            return;
        }
        let (min_y, max_y) = (b.y as f64, b.max_y() as f64);
        if !((min_y < top && top < max_y) || (min_y < bottom && bottom < max_y)) {
            return;
        }
        let center = b.x + b.w * 0.5;
        if !left_of && (self.x <= center || right_of) {
            // updateCollideRight: the block is on the player's right.
            if obj.is_some() {
                self.platformer.colliding_right = obj;
                self.platformer.colliding_left = None;
            }
            self.x = b.x + w * -0.5;
            if self.platformer.x_velocity > 0.0 && not_right {
                self.platformer.x_velocity = 0.0;
            }
            if self.platformer.forced_slide_right {
                self.platformer.slope_jump_count = 0;
            }
        } else {
            if obj.is_some() {
                self.platformer.colliding_left = obj;
                self.platformer.colliding_right = None;
            }
            self.x = b.max_x() + w * 0.5;
            if self.platformer.x_velocity < 0.0 && not_left {
                self.platformer.x_velocity = 0.0;
            }
            if self.platformer.forced_slide_left {
                self.platformer.slope_jump_count = 0;
            }
        }
        // (A dash ends against the side unless its orb persists: not modelled for platformer.)
    }

    /// The snap onto a side (top or bottom) after the position is set: `hitGround` (keeping the
    /// ground flags when it's the head side), the collision ids and, landing, the ground object.
    /// `stop`: the hit-head state's far-side stop (velocity 0, no hitGround).
    fn contact(&mut self, owner: Option<usize>, id: i64, head: bool, breakable: bool, block_v: f64, stop: bool) {
        if stop {
            self.y_velocity = 0.0;
        }
        if head {
            if !stop {
                self.hit_ground_keep();
            }
            if !breakable && id != 0 {
                self.last_collision_top = id;
            }
        } else {
            if !stop {
                self.hit_ground_full();
            }
            if id != 0 {
                self.last_collision_bottom = id;
                self.landed_this_step = true;
            }
            self.last_ground = owner;
            self.ground_y_velocity = block_v;
        }
    }

    /// The moving-block landings (inline updateCollideBottom): collision id, ground object and
    /// ground velocity.
    fn landed_on(&mut self, owner: Option<usize>, id: i64, block_v: f64) {
        if id != 0 {
            self.last_collision_bottom = id;
            self.landed_this_step = true;
        }
        self.last_ground = owner;
        self.ground_y_velocity = block_v;
    }

    /// The end of a side contact: a flying player hitting its head on a block moving toward it
    /// takes the block's velocity.
    /// With the no-auto-jump state a cube that used an orb since its press loses the buffered
    /// jump; with the flip-gravity state a head contact flips gravity.
    fn final_velocity(&mut self, head: bool, static_: bool, snap_jump: bool) {
        if snap_jump && self.state_no_auto_jump >= 1 && self.ring_jumped_since_press {
            self.jump_buffered = false;
        }
        let flyer = matches!(self.mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing);
        if flyer && head && !static_ {
            self.y_velocity = round_velocity(self.ground_y_velocity);
        }
        if head {
            self.head_flip();
        }
    }

    /// The flip-gravity state's head contact (also `didHitHead`): flip gravity and move away
    /// from the new ground at 2, rising; with no-auto-jump the ground and buffered jump go too.
    pub(crate) fn head_flip(&mut self) {
        if self.state_flip_gravity < 1 {
            return;
        }
        self.flip_gravity_pub(!self.upside_down);
        self.y_velocity = round_velocity(if self.upside_down { 2.0 } else { -2.0 });
        self.rising = true;
        self.touching_ground = false;
        if self.state_no_auto_jump > 0 {
            self.on_ground = false;
            self.jump_buffered = false;
            self.ring_jump_ready = false;
        }
    }

    /// `hitGround(obj)`: velocity 0, touching the ground, on the ground unless moving away from
    /// it faster than 5, and the landing rotation.
    pub(crate) fn hit_ground_full(&mut self) {
        let along = if self.upside_down { -self.y_velocity } else { self.y_velocity };
        self.y_velocity = 0.0;
        self.platformer.on_ice = false;
        self.platformer.is_sliding = false;
        self.hit_ground();
        if along <= 5.0 {
            self.on_ground = true;
            self.platformer.last_land_time = self.total_time;
        }
    }

    /// `hitGround(nullptr)` with m_isOnGround, m_touchingGround and m_lastLandTime restored (a
    /// head hit): only the velocity and the rotation change.
    fn hit_ground_keep(&mut self) {
        let (on_ground, touching, landed) = (self.on_ground, self.touching_ground, self.platformer.last_land_time);
        self.hit_ground_full();
        self.on_ground = on_ground;
        self.touching_ground = touching;
        self.platformer.last_land_time = landed;
    }

    /// The inner-box test that ends the function when no side took the contact: the 0.3 box
    /// touching the block kills (breakable blocks break instead). Within 0.1 s of a gravity flip
    /// a head-first contact snaps to the block's far side instead.
    fn inner_kill(&mut self, world: &World, obj: Option<usize>, b: &Rect, head: bool) {
        // The kill test skips passable objects and platformer mode (`!(platformer | passable)`).
        if self.platformer.on || obj.is_some_and(|i| world.objects[i].passable) || !b.intersects(&self.inner_rect()) {
            return;
        }
        if head && self.last_flip_time != 0.0 && self.total_time - self.last_flip_time < FLIP_GRACE {
            let h = self.rect().h;
            self.y = if self.upside_down { b.max_y() + h * 0.5 } else { b.y + h * -0.5 };
            self.hit_ground_full();
            self.touching_ground = false;
            return;
        }
        if let Some(i) = obj.filter(|&i| world.objects[i].kind == ObjectType::BREAKABLE) {
            self.destroyed.push(i);
            return;
        }
        if std::env::var_os("CADENCE_DEBUG_DEATH").is_some() { eprintln!("inner kill by {:?} rect {:?} inner {:?}", obj.map(|i| (world.objects[i].order, world.objects[i].id)), b, self.inner_rect()); }
        self.die("collidedWithObjectInternal");
    }

    /// `m_slopeCenterOffset`: how much further than half the player's height the slope snap
    /// put its center ((h / cos angle) · 0.5 − h · 0.5, f32), set with the current slope.
    fn slope_center_offset(&self, world: &World) -> f64 {
        let Some(si) = self.colliding_slope else { return 0.0 };
        let sr = world.objects[si].rect;
        let angle = (sr.h / sr.w).atan();
        let h = self.rect().h;
        let half = (h / angle.cos()) * 0.5;
        (half + h * -0.5) as f64
    }

    /// `m_currentSlope`: the slope ridden, until the end of the pass that leaves it.
    fn current_slope(&self) -> Option<usize> {
        self.colliding_slope.filter(|_| self.on_slope || self.was_on_slope)
    }

    /// The potential-slope scan over `m_potentialSlopes`: the pass starts it with the current
    /// slope; slopes met this pass without touching their flat sides and slopes snapped to are
    /// added. Run while on a slope, just after one, or with any potential slope.
    fn scan_potential_slopes(&self, world: &World, obj: Option<usize>, b: &Rect) -> SlopeScan {
        let mut scan = SlopeScan::default();
        let current = self.current_slope();
        if !(self.on_slope || self.was_on_slope || !self.potential_slopes.is_empty() || current.is_some()) {
            return scan;
        }
        // Off slopes, a real object's center must lie within the slope's x range.
        let filter = !self.on_slope && obj.is_some() && !self.was_on_slope;
        let me = self.rect();
        let list = self.potential_slopes.iter().copied().chain(current.filter(|s| !self.potential_slopes.contains(s)));
        for si in list {
            let slope = &world.objects[si];
            let r = slope.rect;
            if filter && current != Some(si) {
                let px = world.objects[obj.unwrap()].x;
                if !(r.x < px) || !(px < r.max_x()) {
                    continue;
                }
            }
            let (w, h) = (r.w * 1.2, r.h * 1.1);
            let grown = Rect { x: r.x + (r.w - w) * 0.5, y: r.y + (r.h - h) * 0.5, w, h };
            if !b.intersects(&grown) || !me.intersects(&grown) {
                continue;
            }
            let ceiling = crate::world::slope_floor_top(slope.slope_dir);
            let x = if ceiling ^ slope.slope_uphill { b.x } else { b.max_x() };
            let surface = crate::slopes::slope_y_at(&r, slope.slope_uphill, slope.slope_hazard, ceiling, x);
            let is_current = current == Some(si);
            scan.any = true;
            if (b.max_y() as f64) <= surface + 2.0 && (!ceiling || is_current) {
                scan.under = true;
            }
            if surface - 2.0 <= b.y as f64 && (ceiling || is_current) {
                scan.over = true;
            }
            if scan.under && scan.over {
                break;
            }
        }
        scan
    }
}

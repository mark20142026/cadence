//! Slopes (`PlayerObject::collidedWithSlopeInternal` and its helpers), ported from the
//! decompiled game for static slopes, outside platformer mode, not sideways. Names follow the
//! game's fields; comments name the branches. See NOTES.md "Slopes".

use crate::objects::ObjectType;
use crate::physics::{Mode, Player, round_velocity};
use crate::world::{Rect, World, slope_floor_top, slope_wall_left};

/// `GameObject::slopeYPos(x)`: the surface height at x, in doubles. Uphill slopes rise to the
/// right along the rect's diagonal; past the left edge the formula from the other corner is
/// used (same line, different rounding).
pub fn slope_y_at(rect: &Rect, uphill: bool, hazard: bool, floor_top: bool, x: f32) -> f64 {
    let (min_x, max_x, min_y, max_y) = (rect.x, rect.max_x(), rect.y, rect.max_y());
    let k = (rect.h / rect.w) as f64;
    let from_right = x as f64 - max_x as f64;
    let mut y = if uphill { max_y as f64 + k * from_right } else { min_y as f64 - k * from_right };
    let from_left = min_x as f64 - x as f64;
    let y_left = if uphill { min_y as f64 - k * from_left } else { max_y as f64 + k * from_left };
    if x <= min_x {
        y = y_left;
    }
    if hazard {
        y += if floor_top { -4.0 } else { 4.0 };
    }
    y
}

impl Player {
    fn flying_or_dart(&self) -> bool {
        self.mode.is_flying()
    }

    /// The start of checkCollisions (FUN_10036e6dc): last step's slope contact becomes "was".
    pub(crate) fn begin_slope_pass(&mut self) {
        self.was_on_slope = self.on_slope;
        self.on_slope = false;
        self.potential_slopes.clear();
    }

    /// `collidedWithSlopeInternal` for one slope the player's rect overlaps.
    pub(crate) fn collide_slope(&mut self, world: &World, index: usize, dt: f32) {
        let obj = &world.objects[index];
        if self.pre_slope_collision(world, index, dt) {
            return;
        }
        let pr = match self.slope_rect_width {
            // The rect getObjectRect recomputes after a turning mode switch (centered).
            Some(w) if self.rect_cache.is_some_and(|(y, _)| y == self.y) => Rect { x: self.x - w * 0.5, y: self.y - w * 0.5, w, h: w },
            _ => self.rect(),
        };
        let sr = obj.rect;
        let ud = self.upside_down;
        let ceiling = slope_floor_top(obj.slope_dir);
        // "Downhill" (uVar19): moving right when the slope isn't uphill, going left when it is;
        // flipped by gravity.
        // In platformer mode "left" is the velocity's sign, not the facing (bVar6).
        let left = if self.platformer.on { self.platformer.x_velocity < 0.0 } else { self.going_left };
        let down = (obj.slope_uphill == left) ^ ud;
        let u27 = false;
        let xspeed = self.base_speed * self.speed as f64;
        // fVar42: the vertical speed of running along this slope.
        let k_speed: f32 = sr.h / (sr.w as f64 / xspeed) as f32;
        if self.was_on_slope {
            let sv = if ud { -self.slope_velocity } else { self.slope_velocity };
            if 0.0 < sv && down && self.current_slope_top == ceiling && self.current_slope_top == ud {
                return;
            }
            if 0.0 < sv && (k_speed as f64) < self.current_slope_yvel {
                return;
            }
        }
        let mut f43: f32 = if self.was_on_slope { if down { 4.0 } else { 0.0 } } else if down { 1.0 } else { 0.0 };
        // fVar41 and the probe point (the player's edge on the gravity side, along the slope).
        let half_tilted = (pr.h * 0.5) / (self.slope_rotation as f32).cos();
        let probe = if ud { f43 + half_tilted + self.y } else { (self.y - half_tilted) - f43 };
        if self.was_on_slope {
            if !ud {
                if sr.y + sr.h < probe {
                    return;
                }
            } else if probe < sr.y {
                return;
            }
        } else {
            let inner = Rect { x: sr.x, y: sr.y + 1.0, w: sr.w, h: sr.h - 2.0 };
            if !pr.intersects(&inner) {
                return;
            }
        }
        let (min_y, max_y) = (sr.y, sr.max_y());
        let angle = (sr.h / sr.w).atan();
        let cos_a = angle.cos();
        let (mut off, mut switching) = (0.0f32, false);
        if self.was_on_slope && self.colliding_slope != Some(index) && self.current_slope_top != ceiling {
            off = self.size * 20.0;
            switching = true;
        }
        let half_on_slope = (pr.h / cos_a) * 0.5;
        let surface = slope_y_at(&sr, obj.slope_uphill, obj.slope_hazard, ceiling, self.x) as f32;
        // far/near (bVar8/bVar7) only matter for moving slopes, not ported yet.
        let (snap, _far, _near);
        if !ceiling {
            let mut a = (max_y + pr.h * 0.5) - off;
            let b = (half_on_slope + surface) - off;
            _far = a <= b;
            if !_far {
                a = b;
            }
            _near = a <= min_y;
            snap = if min_y < a { a } else { min_y };
        } else {
            let mut a = off + (min_y - pr.h * 0.5);
            let b = off + (surface - half_on_slope);
            _far = b <= a;
            if a < b {
                a = b;
            }
            _near = max_y <= a;
            snap = if _near { max_y } else { a };
        }
        let y = self.y;
        let ship = self.mode == Mode::Ship;
        let bird = self.mode == Mode::Ufo;
        let ball = self.mode == Mode::Ball;
        let swing = self.mode == Mode::Swing;
        let jb = self.jump_buffered;
        let (b5, mut b20, b26, mut l114, mut take);
        let landing_side = ceiling == ud;
        if landing_side {
            // LAB_100370fd0: the surface gravity pulls toward.
            let mut u21 = false;
            if !switching && down && !self.on_slope {
                u21 = if ship { !jb } else { !self.rising || bird || swing || self.mode == Mode::Wave };
            }
            let mut b9;
            if !ceiling {
                b9 = y < snap;
                if !(b9 || !u21) {
                    if snap + f43 <= y {
                        b9 = false;
                    } else if bird {
                        b9 = self.y_velocity <= 0.0;
                    } else {
                        b9 = true;
                    }
                }
            } else {
                b9 = y != snap && y >= snap;
                if !(b9 || !u21) {
                    if y <= snap - f43 {
                        b9 = false;
                    } else if bird {
                        b9 = 0.0 <= self.y_velocity;
                    } else {
                        b9 = true;
                    }
                }
            }
            if jb {
                if self.flying_or_dart() && !down {
                    b20 = true;
                    b5 = true;
                } else {
                    b20 = false;
                    b5 = false;
                }
            } else {
                b5 = false;
                b20 = false;
            }
            b26 = false;
            l114 = false;
            take = false;
            if b9 {
                if self.slope_contact_kills(obj.slope_hazard, switching) {
                    self.die("slopes:168");
                    return;
                }
                take = true;
            }
        } else {
            // LAB_100370f48: the far side (a head hit, or a ship sliding under a slope).
            let mut f30 = 0.0f32;
            let mut b20h;
            let mut to_11c4 = false;
            if !down {
                if self.flying_or_dart() {
                    f30 = f43;
                    if jb {
                        f30 = if self.was_on_slope { 2.0 } else { 1.0 };
                    }
                    f43 = f30;
                    if switching { b20h = true } else { b20h = false; to_11c4 = true }
                } else if !switching {
                    b20h = false;
                    to_11c4 = true;
                } else {
                    b20h = true;
                }
            } else {
                f43 = f30;
                if switching { b20h = true } else { b20h = false; to_11c4 = true }
            }
            if to_11c4 {
                let u = if ship || bird || swing || self.mode == Mode::Wave { down } else { false };
                b20h = if u { true } else { self.on_slope };
            }
            let mut b9 = y < snap;
            if y >= snap && !b20h {
                b9 = y < snap + f43;
            }
            let mut b5h = snap < y;
            if snap >= y && !b20h {
                b5h = snap - f43 < y;
            }
            if ceiling {
                b9 = b5h;
            }
            if !jb {
                l114 = down;
                b20 = false;
                if ship || bird || swing {
                    if !down {
                        l114 = false;
                    } else {
                        b20 = self.was_on_slope;
                    }
                } else if ball && down {
                    b20 = self.was_on_slope;
                    l114 = false;
                } else {
                    l114 = false;
                }
            } else {
                b20 = false;
                l114 = false;
            }
            if b9 {
                // The hit-head state (H block 1859) lets every mode take the far side.
                if !self.mode.is_flying() && !ball && self.state_hit_head < 1 {
                    // Just after a mode change or a gravity flip the far side is a surface to
                    // snap to, moving away from it at 2 or more (the 0.1 s grace).
                    let recent = |t: f64| t != 0.0 && self.total_time - t < 0.10000000149011612;
                    if recent(self.mode_changed_time) || recent(self.last_flip_time) {
                        self.y = snap;
                        let v = self.y_velocity;
                        let v = if !ud { if v <= -2.0 { v } else { -2.0 } } else if 2.0 <= v { v } else { 2.0 };
                        self.y_velocity = round_velocity(v);
                        self.on_ground = false;
                        self.touching_ground = false;
                        return;
                    }
                    if !ceiling {
                        if snap <= y + 2.0 {
                            self.on_ground = false;
                            self.touching_ground = false;
                            return;
                        }
                    } else if y - 2.0 <= snap {
                        self.on_ground = false;
                        self.touching_ground = false;
                        return;
                    }
                    self.die("slopes:243");
                    return;
                }
                b5 = false;
                b26 = true;
                if self.slope_contact_kills(obj.slope_hazard, switching) {
                    self.die("slopes:249");
                    return;
                }
                take = true;
            } else {
                take = false;
                b5 = false;
                b26 = true;
            }
        }
        // Already resting at this snap height: nothing to do (LAB_1003714c8).
        if self.was_on_slope && self.last_snap_y == snap as f64 {
            take = false;
        }
        if !take {
            return;
        }
        // Snap onto the slope.
        if std::env::var_os("CADENCE_DEBUG_SLOPE").is_some() {
            eprintln!("slope snap t {:.4} slope {index} y {} -> {snap} ceiling {ceiling} ud {ud} down {down} b20 {b20} b26 {b26} l114 {l114} b5 {b5}", self.total_time, self.y);
        }
        self.colliding_slope = Some(index);
        self.current_slope_top = ceiling;
        self.last_snap_y = snap as f64;
        self.slope_downhill = down;
        let old_rotation = self.slope_rotation;
        let gsign: i32 = if ud { -1 } else { 1 };
        let mut sign = if down { gsign } else { -gsign };
        // Reversed going left (and again sideways).
        if left {
            sign = -sign;
        }
        if self.sideways {
            sign = -sign;
        }
        self.slope_rotation = (angle * 57.29578 * sign as f32 * 0.017453292) as f64;
        self.y = snap;
        self.on_slope = true;
        if !self.was_on_slope {
            self.slope_start_time = self.total_time;
        }
        if !b20 {
            if !b26 {
                let v = self.y_velocity as f32;
                self.hit_ground_slope();
                let fast = if ud { v < -5.0 } else { 5.0 < v };
                if fast {
                    self.y_velocity = round_velocity(v as f64);
                }
                // A hard landing (falling faster than 5) on a slope steeper than 40° halves the
                // platformer speed to the right (not to the left: the game tests v > 0 only).
                if self.platformer.on && (angle * 57.29578).abs() > 40.0 && v < -5.0 && 0.0 < self.platformer.x_velocity {
                    self.platformer.x_velocity *= 0.5;
                }
                // m_currentSlope2, m_slopeAngle, m_slopeSlidingMaybeRotated; an uphill slope of
                // 1 radian or more allows no slope jumps (count 3).
                let uphill = obj.slope_uphill != self.sideways;
                let pf = &mut self.platformer;
                pf.slope2 = Some(index);
                pf.slope_angle = angle;
                pf.slope2_uphill = uphill;
                if uphill && 1.0 <= angle {
                    pf.slope_jump_count = 3;
                }
            } else {
                let v = self.y_velocity;
                let kept = if !ud { if v <= 0.0 { v } else { 0.0 } } else if 0.0 <= v { v } else { 0.0 };
                self.y_velocity = round_velocity(kept);
                self.on_ground = false;
                self.touching_ground = false;
            }
        }
        if l114 {
            if !ud {
                if -2.0 < self.y_velocity {
                    self.y_velocity = -2.0;
                }
            } else if self.y_velocity < 2.0 {
                self.y_velocity = 2.0;
            }
        }
        if b5 {
            if !ud {
                if self.y_velocity < 2.0 {
                    self.y_velocity = 2.0;
                }
            } else if -2.0 < self.y_velocity {
                self.y_velocity = -2.0;
            }
        }
        self.current_slope_yvel = k_speed as f64;
        let boost = (0.8f32 / angle).min(1.1);
        let mut sv = k_speed * 1.4 * boost;
        if !u27 && down {
            sv = -sv;
        }
        let sv = if ud { -sv } else { sv };
        self.slope_velocity = if self.mode.is_flying() || ball { sv * 0.75 } else { sv };
        if !ball {
            self.update_slope_rotation(dt);
            return;
        }
        // The ball rolls faster on a slope: 1/cos of its angle (softened past 2), restarted
        // whenever the slope rotation changes.
        if self.slope_rotation == old_rotation as f32 as f64 {
            return;
        }
        self.spinning = false;
        self.ball_slope_scaled = false;
        self.rotation_speed = 0.0;
        let inv = 1.0 / (self.slope_rotation as f32).cos();
        let f = if inv <= 2.0 { inv } else { (inv - 1.0) * 0.7 + 1.0 };
        self.ball_slope_scaled = f != 1.0;
        self.ball_rotate_speed = f;
        let deg: i32 = if self.upside_down { -120 } else { 120 };
        let t = if self.size != 1.0 { 0.16000001f32 } else { 0.2 };
        self.rotation_speed = deg as f32 / (t * self.ball_speed_factor_pub());
        self.spinning = true;
    }

    /// `preSlopeCollision`: the flat sides of a slope (its wall and its base) act as one-unit
    /// solid strips (`collidedWithObjectInternal` with the strip's rect). Returns true when the
    /// slope itself should be skipped: a strip was hit, or the player isn't touching the slope's
    /// rect at all (except the slope being ridden).
    fn pre_slope_collision(&mut self, world: &World, index: usize, dt: f32) -> bool {
        if self.colliding_slope == Some(index) {
            return false;
        }
        let obj = &world.objects[index];
        let sr = obj.rect;
        let pr = self.rect();
        let wall_left = slope_wall_left(obj.slope_dir);
        let floor_top = slope_floor_top(obj.slope_dir);
        let pf = self.platformer.on;
        // The slope's own movement this step along the run axis against the player's.
        let run = (self.base_speed * self.speed as f64 * dt as f64) as f32;
        let moved = if self.sideways { obj.pos.1 as f32 - obj.last_pos.1 } else { obj.pos.0 as f32 - obj.last_pos.0 };
        // Platformer, on or just off a slope: the wall strips are 5 shorter at both ends.
        let pad = if pf && (self.on_slope || self.was_on_slope) { 5.0f32 } else { 0.0 };
        // The wall strip, only on the side the player comes from: a wall on the left when the
        // player is left of the slope (not going left in classic mode unless the slope runs
        // ahead of the player); a wall on the right going left, in platformer mode or for a
        // slope moving into the player, from beyond it.
        let mut hit = false;
        let mut inset = 0.0f32;
        let left_wall = wall_left && self.x < sr.x && !(self.going_left && !pf && run <= moved);
        let right_wall = !wall_left && (self.going_left || pf || run < moved) && sr.max_x() < self.x;
        if left_wall || right_wall {
            // Classic mode: inset by 1 more going left (left wall) / not going left (right).
            inset = if pf || self.going_left != left_wall { 0.0 } else { 1.0 };
            let x = if left_wall { sr.x } else { sr.x + sr.w + -1.0 };
            let wall = Rect { x, y: pad + inset + sr.y, w: 1.0, h: sr.h + (pad + inset) * -2.0 };
            if pr.intersects(&wall) {
                self.collide_solid_rect(world, index, wall, true);
            }
            hit = true;
        }
        // The base strip, the flat side opposite the slanted surface, met from beyond it.
        let base = if floor_top {
            Rect { x: inset + sr.x, y: sr.max_y() - 1.0, w: sr.w + inset * -2.0, h: 1.0 }
        } else {
            Rect { x: inset + sr.x, y: sr.y, w: sr.w + inset * -2.0, h: 1.0 }
        };
        let beyond = if floor_top { base.max_y() <= self.y } else { self.y <= base.max_y() };
        if beyond {
            if pr.intersects(&base) {
                self.collide_solid_rect(world, index, base, true);
                return true;
            }
        }
        // Not touching its flat sides: a potential slope for this pass (when not already on one).
        if !hit && !self.on_slope && !self.potential_slopes.contains(&index) {
            self.potential_slopes.push(index);
        }
        hit || !pr.intersects(&sr)
    }


    /// `updateSlopeRotation`: ease toward the slope's angle; the cube picks the 90° turn of it
    /// nearest its current rotation.
    pub(crate) fn update_slope_rotation(&mut self, dt: f32) {
        let mut target = (self.slope_rotation * 57.295780181884766) as f32;
        if self.mode == Mode::Cube {
            let current = ((self.rotation as i32) % 360) as f32;
            let mut a = target;
            let mut b = target;
            if target <= current {
                if current > target {
                    loop {
                        a = b;
                        b = a + 90.0;
                        if b >= current {
                            break;
                        }
                    }
                }
            } else {
                loop {
                    a = b;
                    b = a - 90.0;
                    if current >= b {
                        break;
                    }
                }
            }
            target = if (current - a).abs() <= (current - b).abs() { a } else { b };
        }
        // Sliding down a slope steeper than 80° the player stays upright.
        let pf = &self.platformer;
        if pf.on && 80.0 < pf.slide_slope_angle && (pf.forced_slide_left || pf.forced_slide_right) {
            target = 0.0;
        }
        self.ease_rotation(dt, target);
    }

    /// `PlayerObject::updateRotation(dt, target)`: the generic easing (×0.5 for the ship, ×3 for
    /// a grounded cube off slopes).
    pub(crate) fn ease_rotation(&mut self, dt: f32, target: f32) {
        let mut k = self.speed * 0.175;
        if self.mode == Mode::Ship {
            k *= 0.5;
        } else if !self.mode.is_flying() && self.mode != Mode::Ball && self.touching_ground && !self.on_slope && !self.was_on_slope {
            k *= 3.0;
        }
        let t = if dt <= k * dt { dt } else { k * dt };
        self.rotation = crate::physics::lerp_angle(self.rotation, target, t);
    }

    /// The end of checkCollisions (FUN_10036e834), slope part: leaving a slope.
    pub(crate) fn end_slope_pass(&mut self) {
        if self.on_slope == self.was_on_slope || self.on_slope {
            return;
        }
        let ud = self.upside_down;
        let sv = self.slope_velocity;
        let v = self.y_velocity;
        let top = self.current_slope_top;
        let mut launch = if !ud {
            (sv < 0.0 && (sv as f64) < v && top) || (0.0 < sv && v < sv as f64 && !top)
        } else if sv < 0.0 && v > sv as f64 && top {
            true
        } else if 0.0 < sv && (sv as f64) > v {
            !top
        } else {
            false
        };
        if self.rising && !self.mode.is_flying() {
            launch = self.mode == Mode::Ball && launch;
        }
        if !launch {
            let toward = if !ud { sv < 0.0 } else { 0.0 < sv };
            if toward && !self.rising && !self.jump_buffered && !self.ring_jump_ready && top == ud {
                self.touching_ground = false;
                let v = if ud { self.current_slope_yvel } else { -self.current_slope_yvel };
                self.y_velocity = round_velocity(v);
                self.on_ground = false;
                self.slope_spin(false);
            }
        } else {
            let t = self.total_time - self.slope_start_time;
            let mut m = t * 10.0;
            if m <= 0.4000000059604645 {
                m = 0.4000000059604645;
            }
            if 0.10000000149011612 <= t {
                m = 1.0;
            }
            let mut lv = (m * sv as f64) as f32;
            // Platformer: the launch is the speed along the slope, ±|vx·tan(m_slopeAngle)|·1.1
            // (the sign of the classic one), and only with a direction held fast enough:
            // |vx| ≥ 5·cos(angle) − 1.
            let mut allowed = true;
            if self.platformer.on {
                let a = self.platformer.slope_angle;
                let vx = self.platformer.x_velocity;
                let k = (vx * a.tan() as f64).abs() * 1.100000023841858;
                lv = (if 0.0 <= lv { k } else { -k }) as f32;
                let (l, r, _) = self.platformer_direction();
                allowed = (l || r) && ((a.cos() * 5.0) as f64 + -1.0) <= vx.abs();
            }
            if std::env::var_os("CADENCE_DEBUG_SLOPE").is_some() {
                eprintln!("slope exit t {:.4} start {:.4} m {m} sv {sv} lv {lv} v {v}", self.total_time, self.slope_start_time);
            }
            let skip = if !ud {
                if top && self.slope_downhill { v <= lv as f64 } else { lv as f64 <= v }
            } else if !top && self.slope_downhill {
                lv as f64 <= v
            } else {
                v <= lv as f64
            };
            if !skip && allowed {
                // FUN_10037013c: the slope launch.
                self.rising = true;
                self.touching_ground = false;
                self.on_ground = false;
                self.accelerating = true;
                self.y_velocity = round_velocity(lv as f64);
                self.slope_spin(true);
            }
        }
        // Platformer (LAB_10036eca4): leaving a slope while holding the way it slid ends the
        // slide's speed bonus, and the speed above the cap then fades slowly (m_isMoving).
        let (l, r, _) = self.platformer_direction();
        let pf = &mut self.platformer;
        if 0.0 < pf.slope_speed_bonus && ((l && pf.last_slide_left) || (r && !pf.last_slide_left)) {
            pf.is_sliding = false;
            pf.slope_speed_bonus = 0.0;
            pf.is_moving = true;
        }
        self.slope_velocity = 0.0;
        self.current_slope_yvel = 0.0;
        self.slope_rotation = 0.0;
    }

    /// Spin after leaving a slope: a launch spins the cube backward at 180°/0.8667 s (mini
    /// 0.6667); a plain exit starts the normal spin.
    fn slope_spin(&mut self, launched: bool) {
        if self.mode != Mode::Cube {
            return;
        }
        if launched {
            let t = if self.size != 1.0 { 0.6666667f32 } else { 0.8666667 };
            let deg = if self.upside_down { 180.0f32 } else { -180.0 };
            self.spinning = true;
            self.spin_dir = deg.signum();
            self.rotation_speed = deg / t;
        } else {
            self.start_spin();
        }
    }

    /// A slope contact that kills (joined_r0x0001003711b4): a spike slope, or switching
    /// slopes / the wave without the dart-slide state, unless the hit-head state is on.
    fn slope_contact_kills(&self, hazard: bool, switching: bool) -> bool {
        hazard || ((switching || (self.mode == Mode::Wave && self.state_dart_slide < 1)) && self.state_hit_head < 1)
    }

    /// `hitGround` from a slope surface.
    fn hit_ground_slope(&mut self) {
        let v = self.y_velocity * self.dir();
        // hitGround's platformer fields, as on blocks (`hit_ground_full`).
        self.platformer.on_ice = false;
        self.platformer.is_sliding = false;
        if v <= 5.0 {
            self.on_ground = true;
            self.platformer.last_land_time = self.total_time;
        }
        // hitGround always zeroes the velocity (a hard landing restores it afterwards).
        self.y_velocity = 0.0;
        if !self.touching_ground {
            self.events.push(crate::physics::Event::Land { slope_angle: Some((self.slope_rotation * 57.295780181884766) as f32) });
        }
        self.touching_ground = true;
        if self.mode == Mode::Cube {
            self.spinning = false;
        }
    }
}

/// True for slope objects.
pub fn is_slope(kind: ObjectType) -> bool {
    kind == ObjectType::SLOPE
}

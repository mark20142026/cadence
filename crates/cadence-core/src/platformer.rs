//! Platformer mode (`m_isPlatformer`, levels with kA22): left/right input and the horizontal
//! movement of `PlayerObject::updateMove` (m1 0x36a7c4), ported from the instructions (the f64
//! operations, fused multiply-adds and f32 constants as compiled). The x step is
//! `m_platformerXVelocity · dt` (PlayerObject::update, dt = 0.25) instead of the classic speed.

use crate::physics::{Mode, Player};

/// f32 constants the game loads as doubles.
const F_0_2: f64 = 0.20000000298023224;
const F_0_05: f64 = 0.05000000074505806;
const F_0_1: f64 = 0.10000000149011612;
const F_0_15: f64 = 0.15000000596046448;
const F_0_01: f64 = 0.009999999776482582;
const F_0_8: f64 = 0.800000011920929;
const F_0_95: f64 = 0.949999988079071;
const F_0_3: f64 = 0.30000001192092896;
const F_0_4: f64 = 0.4000000059604645;
const F_0_7: f64 = 0.699999988079071;
const F_1_35: f64 = 1.350000023841858;
const F_0_28: f64 = 0.2800000011920929;

impl Player {
    /// `pushButton` / `releaseButton` for buttons 2 (left) and 3 (right): `switchedDirTo` on a
    /// press. Ignored while controls are disabled. `GJBaseGameLayer::handleButton` drops them outside platformer levels.
    pub fn set_direction(&mut self, down: bool, button: u8) {
        if !self.platformer.on || !(button == 2 || button == 3) {
            return;
        }
        if let Some(p2) = &mut self.dual {
            p2.set_direction_one(down, button);
        }
        self.set_direction_one(down, button);
    }

    fn set_direction_one(&mut self, down: bool, button: u8) {
        // (Unlike the jump, a direction pressed on the attempt's first step counts: traces.)
        if self.controls_disabled_pub() {
            return;
        }
        let pf = &mut self.platformer;
        let vx = pf.x_velocity;
        if down {
            // switchedDirTo (0x37b9d4).
            if button == 3 {
                pf.holding_right = true;
                pf.left_last = false;
                if pf.is_sliding && pf.last_slide_left {
                    pf.is_sliding = false;
                    pf.slope_speed_bonus = 0.0;
                }
                if vx < 0.0 {
                    pf.is_moving = false;
                }
            } else {
                pf.holding_left = true;
                pf.left_last = true;
                if pf.is_sliding && !pf.last_slide_left {
                    pf.is_sliding = false;
                    pf.slope_speed_bonus = 0.0;
                }
                if vx > 0.0 {
                    pf.is_moving = false;
                }
            }
        } else if button == 3 {
            pf.holding_right = false;
            if vx > 0.0 {
                pf.is_moving = false;
            }
        } else {
            pf.holding_left = false;
            if vx < 0.0 {
                pf.is_moving = false;
            }
        }
    }

    /// The direction updateMove and updateJump read: (left, right, holding). Both held: the
    /// one pressed last. Nothing held: a forced slide counts as holding.
    pub(crate) fn platformer_direction(&self) -> (bool, bool, bool) {
        let pf = &self.platformer;
        if pf.holding_left {
            if pf.holding_right {
                (pf.left_last, !pf.left_last, true)
            } else {
                (true, false, true)
            }
        } else if pf.holding_right {
            (false, true, true)
        } else if pf.forced_slide_left {
            (false, false, true)
        } else {
            (false, false, pf.forced_slide_right)
        }
    }

    /// `updateMove(dt)` in platformer mode (dt = the step's dt · 0.9, as updateJump's).
    pub(crate) fn update_move(&mut self, dt: f32) {
        let mut max = self.base_speed * self.speed as f64; // d9
        if self.on_slope && !self.slope_downhill && !self.platformer.slope_resnapped {
            max *= self.slope_angle_f32().cos() as f64;
        }
        let (left, right, holding) = self.platformer_direction();
        let total_time = self.total_time;
        let touching = self.touching_ground;
        let slope_deg = match self.colliding_slope {
            Some(_) => ((self.slope_angle_f32() * 57.29578).abs()) as f64,
            None => 0.0,
        };
        let pf = &mut self.platformer;
        // d0: the acceleration factor (0.2 on ice or just after a slope jump the other way).
        let mut accel = 1.0f64;
        if pf.on_ice {
            accel = F_0_2;
        } else if !touching && pf.last_slide_left != left {
            if pf.slope_jump_count > 0 {
                accel = F_0_2;
            } else {
                let t = total_time - pf.slope_jump_reset_time;
                if t < 0.5 {
                    accel = ((t + t).mul_add(F_0_8, F_0_2) as f32) as f64;
                }
            }
        }
        let vx0 = pf.x_velocity; // d11
        let dtd = dt as f64; // d10
        let half = dtd * 0.5; // d13
        let slide_bonus = if ((pf.forced_slide_left && left) || (pf.forced_slide_right && right))
            && vx0.abs() >= max * F_0_95
            && self.on_slope
        {
            !pf.slope_resnapped
        } else {
            false
        };
        if pf.is_sliding || slide_bonus {
            let yb = pf.y_velocity_before_slope;
            let mut k = 1.0f64;
            if yb < 0.0 {
                k = (yb / -15.0).mul_add(F_0_3, 1.0);
            }
            let bonus = if slide_bonus {
                let b = ((dt * 0.25) as f64).mul_add(k, pf.slope_speed_bonus);
                pf.slope_speed_bonus = b;
                b
            } else {
                pf.slope_speed_bonus
            };
            max += bonus;
            pf.is_sliding = true;
        }
        let mut step = half * accel; // d12
        if (left && vx0 > 0.0) || (!(left && vx0 > 0.0) && right && vx0 < 0.0) {
            // Turning around: 1.5×.
            step *= 1.5;
        }
        // decel: d0 for the slow-down, slowing (w9: always slow down), vx (d1)
        let decel: f64;
        let mut slowing: bool;
        if left || right {
            pf.idle_time = 0.0;
            if (left && vx0 > -max) || (right && vx0 < max) {
                pf.x_velocity = step.mul_add(if left { -1.0 } else { 1.0 }, vx0);
            }
            slowing = false;
            decel = F_0_05;
        } else if pf.on_ice {
            slowing = true;
            decel = F_0_2;
        } else {
            let t = pf.idle_time;
            let d = (t + 0.2f32).min(1.0);
            let rate: f32 = if touching {
                0.2
            } else {
                match self.mode {
                    Mode::Ship | Mode::Ball | Mode::Ufo => 0.02,
                    Mode::Robot | Mode::Spider => 0.1,
                    _ => 0.05,
                }
            };
            decel = d as f64;
            pf.idle_time = dt.mul_add(rate, t);
            slowing = true;
        }
        let mut decel = decel;
        let mut vx = pf.x_velocity; // d1
        // af24 / af38 / af58 / af60.
        let mut momentum_path = false; // reached af58 (moving, forces or momentum)
        if vx.abs() < 0.5 {
            pf.affected_by_forces = false;
            if pf.is_moving {
                slowing = true;
                momentum_path = true;
            } else if pf.has_momentum {
                momentum_path = true;
            }
        } else if pf.affected_by_forces || pf.is_moving {
            slowing = true;
            momentum_path = true;
        } else if pf.has_momentum {
            momentum_path = true;
        }
        if momentum_path && !pf.on_ice {
            // afa8
            decel = if pf.decrease_boost_slide || holding { F_0_05 } else { F_0_2 };
        } else if pf.forced_slope_slide && !pf.on_ice {
            // af60
            decel = F_0_1;
        }
        let fsl = pf.forced_slide_left;
        // af88 / afe0: forced slides accelerate down the slope.
        let mut slide_accel = false;
        if fsl {
            pf.last_slide_left = true;
            if !left || !(right || !pf.forced_slide_right) {
                slide_accel = true;
            }
        } else if pf.forced_slide_right {
            pf.last_slide_left = false;
            if !right {
                slide_accel = true;
            }
        }
        if slide_accel {
            let angle = slope_deg;
            let a = if angle > 80.0 { 80.0 } else { angle };
            let a = a.max(40.0);
            let k = (a - 40.0) / 40.0;
            let mut push = half * k.mul_add(0.5, 0.5) * F_0_4;
            if self.mode == Mode::Ship {
                push *= F_0_7;
            }
            let fsr = pf.forced_slide_right;
            if (fsl && vx > 0.0) || (fsr && vx < 0.0) {
                let mut m = k.mul_add(F_0_7, F_1_35);
                m = match self.mode {
                    Mode::Ship => m + -0.25,
                    Mode::Ball => m + -F_0_28,
                    Mode::Spider => m + -F_0_2,
                    _ => m,
                };
                if pf.on_ice {
                    m *= 4.0;
                }
                if !(!fsl || right) || !(left || !fsr) {
                    m *= F_0_8;
                }
                let m = step * m;
                if m.abs() > push.abs() {
                    push = m;
                }
            } else if (fsl && right) || (fsr && left) {
                push += step;
            }
            if self.on_slope && !pf.was_touching_ground && pf.slope_jump_count >= 1 && !pf.slope_resnapped {
                vx *= F_0_8;
                pf.x_velocity = vx;
            }
            vx = push.mul_add(if fsl { -1.0 } else { 1.0 }, vx);
            pf.x_velocity = vx;
            slowing = false;
        }
        // b1f8
        if pf.forced_slope_slide && !(pf.on_ice || fsl) && !pf.forced_slide_right {
            if pf.last_slide_left {
                if vx > 0.0 {
                    pf.forced_slope_slide = false;
                }
            } else if vx < 0.0 {
                pf.forced_slope_slide = false;
            }
        }
        let friction = if pf.on_ice || pf.is_moving { F_0_15 } else { 1.0 };
        if slowing || vx.abs() > max {
            let d = -(decel * vx) * dtd;
            vx = d.mul_add(friction, vx);
            pf.x_velocity = vx;
            if vx.abs() < F_0_01 || (vx0 > 0.0 && vx < 0.0) || (vx0 < 0.0 && vx > 0.0) {
                pf.x_velocity = 0.0;
                pf.has_momentum = false;
                pf.affected_by_forces = false;
                pf.forced_slope_slide = false;
                pf.is_moving = false;
            }
        } else if vx.abs() < F_0_01 || (vx0 > 0.0 && vx < 0.0) || (vx0 < 0.0 && vx > 0.0) {
            pf.has_momentum = false;
            pf.affected_by_forces = false;
            pf.forced_slope_slide = false;
            pf.is_moving = false;
        }
        if pf.has_momentum {
            let m = pf.momentum as f64;
            let d = -(decel * m) * dtd;
            let friction = if pf.on_ice || pf.is_moving { F_0_15 } else { 1.0 };
            let m = d.mul_add(friction, m) as f32;
            pf.momentum = m;
            if m.abs() < 1.0 {
                pf.has_momentum = false;
            }
        }
        if self.dashing {
            self.platformer.x_velocity = max;
        } else {
            let was_left = self.going_left;
            if left && !was_left {
                self.do_reverse_player(true);
            } else if right && was_left {
                self.do_reverse_player(false);
            }
            if was_left != self.going_left {
                // exitPlatformerAnimateJump (visual), then the sprite turns with the player.
                match self.mode {
                    Mode::Ship | Mode::Ufo => self.rotation = -self.rotation,
                    Mode::Cube if !self.spinning => {
                        let mut r = self.rotation;
                        if self.on_slope {
                            r = (self.slope_rotation).mul_add(-57.295780181884766, r as f64) as f32;
                        }
                        let r = r.abs();
                        if (45.0 < r && r < 135.0) || (225.0 < r && r < 315.0) {
                            self.rotation += 180.0;
                        }
                    }
                    _ => {}
                }
            }
        }
        // b440: a cube moving sideways in the air starts its spin.
        if self.mode == Mode::Cube
            && self.platformer.x_velocity != 0.0
            && !self.spinning
            && holding
            && self.y_velocity.abs() > 1.0
        {
            self.run_rotate_action_pub();
        } else if self.mode == Mode::Cube
            && !self.on_slope
            && !holding
            && !self.was_on_slope
            && !self.touching_ground
            && !self.spinning
            && !self.dashing
            && (self.rotation as i32) % 90 != 0
        {
            // b47c: in the air with nothing held the cube eases to the nearest quarter turn
            // (of its truncated angle mod 360), as updateRotation(dt, target) (0x36ccac).
            let r = self.rotation as i32;
            let target = if r >= 0 {
                let m = r % 360;
                let s0 = m as f32;
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                if m != 0 {
                    loop {
                        lo = hi;
                        hi += 90.0;
                        if !(hi < s0) {
                            break;
                        }
                    }
                }
                if (s0 - hi).abs() < (s0 - lo).abs() { hi } else { lo }
            } else {
                let m = (-r) % 360;
                let s0 = (-m) as f32;
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                if m != 0 {
                    loop {
                        lo = hi;
                        hi += -90.0;
                        if !(hi > s0) {
                            break;
                        }
                    }
                }
                if (s0 - hi).abs() < (s0 - lo).abs() { hi } else { lo }
            };
            let k = self.speed * 0.175;
            let t = if dt <= k * dt { dt } else { k * dt };
            self.rotation = crate::physics::slerp_radians(self.rotation * 0.017453292, target * 0.017453292, t) * 57.29578;
        }
        // (The ball's roll speed is visual; not modelled yet.)
    }

    /// `doReversePlayer(left)` (0x36c7e0): facing; the ball restarts its roll.
    pub(crate) fn do_reverse_player(&mut self, left: bool) {
        self.going_left = left;
        if self.mode == Mode::Ball {
            self.run_rotate_action_pub();
        }
    }

    /// `m_slopeAngle` (+0x5f0) of the slope ridden: the slope's angle with its direction's sign,
    /// as landing on it set it (`slopes.rs`), 0 off slopes.
    pub(crate) fn slope_angle_f32(&self) -> f32 {
        if self.on_slope { self.slope_rotation as f32 } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use crate::level::LevelString;
    use crate::physics::Player;
    use crate::world::World;

    /// Holding right up a 45° slope (steeper than 40°) the player is forced to slide left: the
    /// speed falls below the slope's cap and keeps falling until the player slides back down
    /// (the real re/traces/pfslope run).
    #[test]
    fn steep_slopes_force_a_slide() {
        let level = LevelString::parse("kA13,0,kA22,1;1,289,2,315,3,15;1,1,2,345,3,15;1,1,2,375,3,15;1,1,2,345,3,45;1,1,2,375,3,45");
        let mut world = World::from_level(&level);
        world.reset();
        let mut p = Player::spawn(&world);
        p.apply_level_settings(&level);
        let mut on_slope_steps = 0;
        let mut min_vx = f64::MAX;
        for step in 0..900 {
            p.begin_step(&mut world);
            if step == 10 {
                p.set_direction(true, 3);
            }
            world.pre_step((p.x, p.y));
            p.step(&world);
            world.post_step(&p);
            if p.on_slope {
                on_slope_steps += 1;
                assert!(p.platformer.forced_slide_left && !p.platformer.forced_slide_right);
                min_vx = min_vx.min(p.platformer.x_velocity);
            }
        }
        assert!(on_slope_steps > 20);
        // The slide takes the speed below the cap (cos 45° of full speed) and then below 0.
        assert!(min_vx < 0.0, "min vx {min_vx}");
    }
}

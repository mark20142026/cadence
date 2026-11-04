//! Player physics, matched step for step against the real game (see NOTES.md, "Physics").
//!
//! The game runs 240 physics steps per second. Inside the player code time is measured in
//! 1/60 s units, so one step is `dt = 0.25`. Positions are f32; velocities are f64 and are
//! rounded every step by [`round_velocity`].

use crate::objects::ObjectType;
use crate::world::{Rect, World};

/// Physics steps per second.
pub const STEPS_PER_SECOND: u32 = 240;
/// Player time per step, in 1/60 s units.
pub const STEP_DT: f64 = 0.25;

/// Horizontal speed per 1/60 s at speed factor 1. Stored in the game as the f32 `5.770002`.
pub const BASE_SPEED: f64 = 5.770002_f32 as f64;
/// Speed factor of the default (1x) speed.
pub const SPEED_NORMAL: f32 = 0.9;

/// Cube jump velocity.
pub const CUBE_JUMP: f64 = 11.18;
/// `m_yStart` at normal speed: the orb launch speed, stored as the double of f32 11.18. The game
/// sets it per speed with the gravity (`PlayerObject::updateTimeMod`): 0.7 → 10.620032,
/// 0.9 → 11.180031776428223, 1.1 → 11.420032, 1.3/1.6 → 11.230032.
pub const Y_START: f64 = 11.180031776428223;
/// Base gravity per 1/60 s (f32 in the game), before mode and size factors.
pub const GRAVITY: f32 = 0.958199;
/// `m_gravity` at normal speed (a double holding f32 0.958199; per speed, see Y_START).
pub const M_GRAVITY: f64 = 0.9581990242004395;

/// Per-speed constants (`PlayerObject::updateTimeMod`): base x speed (`m_playerSpeedAC`),
/// `m_yStart` and `m_gravity`, as the doubles the game stores. Unknown factors only reset the
/// base speed.
pub fn speed_constants(speed: f32) -> (f64, Option<(f64, f64)>) {
    let d = |bits: u64| f64::from_bits(bits);
    if speed == 0.9 {
        (d(0x4017147b60000000), Some((d(0x40265c2d20000000), d(0x3feea99100000000))))
    } else if speed == 0.7 {
        (d(0x4017eb85a4f00ef1), Some((d(0x40253d74d594f26b), d(0x3fee161c36976bc2))))
    } else if speed == 1.1 {
        (d(0x40177ae1cde5d181), Some((d(0x4026d70e6f2e8c05), d(0x3feea15fcc1871e7))))
    } else if speed == 1.3 || speed == 1.6 {
        (d(0x401800008637bd06), Some((d(0x402675c6c11a1123), d(0x3feec22467be553b))))
    } else {
        (d(0x4017147b60000000), None)
    }
}
/// Gravity factor for the cube (0.9 for the robot; ball and spider 0.6).
pub const CUBE_GRAVITY_FACTOR: f32 = 1.0;
pub const BALL_GRAVITY_FACTOR: f32 = 0.6;

/// Terminal fall speed of the cube family (clamped after gravity every step).
pub const MAX_FALL_SPEED: f64 = 15.0;

/// The ground line and the player's center height when standing on it (half-size 15).
pub const GROUND: f32 = 90.0;
pub const GROUND_Y: f32 = GROUND + PLAYER_HALF;

/// Half the player's size (the cube is 30x30).
pub const PLAYER_HALF: f32 = 15.0;

/// Height of the floor-to-ceiling corridor each mode gets (`updateDualGround`): ship, UFO, wave
/// and swing 300; ball 240 (spider 270).
pub const SHIP_CORRIDOR: f32 = 300.0;
/// The robot's gravity factor in updateJump (the cube's is 1).
pub const ROBOT_GRAVITY_FACTOR: f32 = 0.9;
/// `GJBaseGameLayer::updateMaxGameplayY` for levels without their own settings: the height above
/// which the player is out of bounds.
pub const MAX_GAMEPLAY_Y: f32 = 2790.0;
pub const BALL_CORRIDOR: f32 = 240.0;

/// The game's velocity rounding: the fractional part is rounded to 3 decimals (half away from
/// zero) and added back to the integer part. Reproduces values like 7.9399999999999995 that a
/// plain `round(v * 1000) / 1000` does not.
pub fn round_velocity(v: f64) -> f64 {
    let whole = (v as i32) as f64;
    if whole == v {
        return v;
    }
    ((v - whole) * 1000.0).round() / 1000.0 + whole
}

/// The game's `convertToClosestRotation`: truncate to whole degrees mod 360, bracket that with
/// 90° steps from 0, and take the nearer bracket, ties going to the earlier one (so 315.4°
/// becomes 270°, not 360°).
pub fn closest_rotation(rotation: f32) -> f32 {
    let f = ((rotation as i32) % 360) as f32;
    let mut p = 0.0f32;
    let mut prev = p;
    if p <= f {
        if f > p {
            loop {
                prev = p;
                p += 90.0;
                if p >= f {
                    break;
                }
            }
        }
    } else {
        loop {
            prev = p;
            p -= 90.0;
            if f >= p {
                break;
            }
        }
    }
    if (f - prev).abs() <= (f - p).abs() { prev } else { p }
}

/// Move `from` toward `to` by fraction `t` along the shorter way around the circle.
pub fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    slerp_radians(from * 0.017453292, to * 0.017453292, t) * 57.29578
}

/// The game's angle easing (m1 0x216db8): a quaternion slerp between two z-rotations, built
/// from half angles, so the result is `2·atan2(..)` in (-2π, 2π]. All f32 with the same fused
/// multiply-adds as the original (`fmadd` in the dot product and the final blend).
pub fn slerp_radians(a: f32, b: f32, t: f32) -> f32 {
    let (sa, ca) = (a * 0.5).sin_cos();
    let (mut sb, mut cb) = (b * 0.5).sin_cos();
    let mut dot = ca.mul_add(cb, sa * sb);
    if dot < 0.0 {
        dot = -dot;
        sb = -sb;
        cb = -cb;
    }
    let (w_a, w_b) = if 1.0 - dot <= 0.0001 {
        (1.0 - t, t)
    } else {
        let theta = dot.acos();
        let st = theta.sin();
        (((1.0 - t) * theta).sin() / st, (theta * t).sin() / st)
    };
    let half = w_a.mul_add(sa, sb * w_b).atan2(w_a.mul_add(ca, cb * w_b));
    half + half
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Cube,
    Ship,
    Ball,
    Ufo,
    /// The wave (the game's "dart").
    Wave,
    Robot,
    /// The spider (`m_isSpider`): teleports to the surface on the other side (spider.rs).
    Spider,
    /// The swing (`m_isSwing`, 2.2): flies under 0.4 g; each press flips gravity.
    Swing,
}

impl Mode {
    /// The game's `isFlying`: modes that steer freely in a corridor.
    pub fn is_flying(self) -> bool {
        matches!(self, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing)
    }
}

/// Something this step did that the game shows an effect for (not part of the simulation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// An orb fired (`ringJump`): the orb's object index.
    Orb(usize),
    /// A pad launched the player (`propellPlayer` → `playBumpEffect`): the pad's index.
    Pad(usize),
    /// A portal changed the player (`spawnPortalCircle`): a circle shrinking from `radius` over
    /// the portal, in `color`.
    Portal { index: usize, radius: f32, color: [u8; 3] },
    /// The player touched down after being off the ground (`hitGround`): the landing burst,
    /// turned to the slope's angle (degrees) when landing on one.
    Land { slope_angle: Option<f32> },
    /// The spider teleported (`spiderTestJumpInternal` → `playSpiderDashEffect`): its center
    /// before and after.
    SpiderDash { from: (f32, f32), to: (f32, f32) },
}

/// The portal circle color for a mode (the toggle*Mode functions; switching to the cube shows
/// the color of the mode it leaves).
fn mode_circle_color(mode: Mode) -> [u8; 3] {
    match mode {
        Mode::Ship => [255, 0, 255],
        Mode::Ufo | Mode::Wave => [255, 200, 0],
        // toggleSwingMode: spawnPortalCircle(50, 0xc8ff → (255, 200, 0)).
        Mode::Swing => [255, 200, 0],
        Mode::Ball | Mode::Robot | Mode::Spider => [255, 50, 50],
        Mode::Cube => [255, 255, 255],
    }
}

/// Player state that the traces record.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// Dual mode: the second player (stepped right after this one, with the same input).
    pub dual: Option<Box<Player>>,
    /// Dual mode: the dual portal's y, which centers the shared corridor (`m_dualPortal`).
    pub dual_portal_y: Option<f32>,
    /// y of the last mode portal (`m_lastPortal`): the corridor's center outside dual mode.
    mode_portal_y: f32,
    /// The other player's (mode, upside down) during this step, in dual mode.
    partner: Option<(Mode, bool)>,
    /// This player is player 2.
    is_second: bool,
    /// The robot's hold boost timer (`m_accelerationOrSpeed`): up to 1.5, +dt·0.1 per held step.
    robot_boost: f64,
    /// Launched by a pad since the last jump (`m_touchedPad`): no robot hold boost.
    touched_pad: bool,
    /// The game's cached object rect during the collision pass: (y, width) it was computed at.
    /// It's only recomputed when the player moves (`setPosition`), so a size or mode portal
    /// doesn't resize it until then.
    pub(crate) rect_cache: Option<(f32, f32)>,
    /// The width slopes see after a turning mode switch this pass (see `switch_mode`).
    pub(crate) slope_rect_width: Option<f32>,
    /// Slopes met this pass without touching their flat sides (`m_potentialSlopes`): blocks they
    /// run into aren't landed on or pushed against.
    pub(crate) potential_slopes: Vec<usize>,
    /// Moving platforms: `m_lastGroundObject` (the block last landed on), whether anything was
    /// landed on this step (`m_collisionLogBottom` not empty), the block `testForMoving` found
    /// moving away under the player this pass (`m_collidedObject`), and +0x658 (the player was
    /// snapped back onto a moving block last step, so the pass looks for one).
    pub(crate) last_ground: Option<usize>,
    /// `m_wasTeleported`: set by a teleport, read and cleared by the next collision pass.
    was_teleported: bool,
    /// `m_isDashing` and `m_dashY` (tan of the dash angle): while dashing the y velocity is 0
    /// and y moves by the x step times it.
    pub dashing: bool,
    pub dash_y: f64,
    /// The icon sprite's spin while dashing (`updateDashArt`: CCRepeatForever(CCRotateBy(0.3,
    /// ±288))), stepped by the cocos scheduler once per rendered frame; a dash's end gives the
    /// player its rotation (`stopDashing`).
    pub icon_spin: Option<IconSpin>,
    /// A cube's dash ended in the air: its spin restarts once it starts falling.
    pub spin_after_dash: bool,
    dash_angle: f32,
    /// A speed portal restarted the ball's roll this step (applied after the rotation update).
    pending_ball_roll: bool,
    /// A speed portal's speed, applied at the start of the next step.
    pending_speed: Option<f32>,
    pub(crate) landed_this_step: bool,
    /// `m_groundYVelocity`: the velocity of the block last landed on (or that last pushed into
    /// the player).
    pub(crate) ground_y_velocity: f64,
    /// Player-state counters set to 2 by special blocks (`touchedObject`) and decremented at the
    /// end of every `PlayerObject::update`: active for the rest of the step they're touched in
    /// and the next one. `m_stateNoAutoJump` (1813), `m_stateDartSlide` (1755),
    /// `m_stateHitHead` (1859), `m_stateFlipGravity` (2866).
    pub(crate) state_no_auto_jump: i32,
    pub(crate) state_dart_slide: i32,
    pub(crate) state_hit_head: i32,
    pub(crate) state_flip_gravity: i32,
    /// Force blocks (2069, 3645): the state counter (+0xb28, 2 on touch), the force summed
    /// this step (+0xb2c, applied by the next update) and the force ids used this step.
    pub(crate) state_force: i32,
    pub(crate) force: (f32, f32),
    force_ids: Vec<i32>,
    /// The level's force blocks by creation order (`ForceBlockGameObject` keys).
    pub force_blocks: std::sync::Arc<std::collections::HashMap<u32, ForceBlock>>,
    /// `m_ringJumpedSincePress`: an orb (or gravity pad) fired since the last press.
    pub(crate) ring_jumped_since_press: bool,
    /// `m_lastSpiderFlipTime` (total time of the last spider jump; 0 = never).
    pub(crate) last_spider_flip_time: f64,
    /// The camera as the physics sees it (stepped by `step_camera` after the world's triggers):
    /// corridor floors and ceilings and the spider's limits depend on its zoom, static view and
    /// sliding ground. And the level's camera triggers it fires.
    pub camera: crate::camera::Camera,
    pub camera_triggers: crate::camera::CameraTriggers,
    /// The walk through the order channels (`order.rs`), the world triggers it fired this step
    /// (the world runs them in `post_step`) and the camera triggers for `step_camera`.
    pub order_state: crate::order::OrderState,
    /// `m_cameraPadding` / `m_cameraEasing` (+0x2cc/+0x2d0): resetCamera sets 0.5 and 10; mode
    /// portals with camera options change them.
    pub camera_padding: (f32, f32),
    pub fired_triggers: Vec<u32>,
    pending_camera: Vec<crate::camera::CameraAction>,
    /// The touch-triggered camera triggers already used (by index in `CameraTriggers`).
    camera_touched: Vec<bool>,
    /// The corridor ceiling `checkCollisions` saw last step (+0x6bc): while it moves (the
    /// ground sliding), a floor contact keeps the velocity.
    prev_ceiling: f32,
    /// The level's gravity-bug fix (kA32): updateJump's 2g tests mirrored upside down.
    pub fix_gravity_bug: bool,
    /// `m_maxGameplayY` (`updateMaxGameplayY` 0x10da98): 2790, or with the level's dynamic
    /// height (kA37, LevelSettingsObject+0x1c5) max(1200, highest object y) + 390.
    pub max_gameplay_y: f32,
    /// Free mode (the last mode portal's key 111, GJBaseGameLayer+0x311) and the level orders
    /// of the portals that set it.
    pub free_mode: bool,
    pub free_mode_portals: std::sync::Arc<Vec<u32>>,
    /// This step's game time in seconds and in 60 fps units (0.25 at 240 steps/s; time warp
    /// scales both).
    pub step_secs: f64,
    pub dt60: f64,
    /// kA39: round objects collide by center distance (see `hits_round`).
    pub fix_radius_collision: bool,
    /// Rotated gameplay (`rotateGameplay`, trigger 2900): `m_isSideways` (forward is ±y, the
    /// vertical axis is x) and `m_isGoingLeft` (forward is backward).
    pub sideways: bool,
    pub going_left: bool,
    /// Collision ids (object index + 1) of the blocks met on the feet side (`m_lastCollisionBottom`)
    /// and the head side (`m_lastCollisionTop`) this step, and last step's (`m_prev*`); −1 = none.
    pub(crate) last_collision_bottom: i64,
    pub(crate) last_collision_top: i64,
    pub(crate) prev_collision_bottom: i64,
    pub(crate) prev_collision_top: i64,
    collided_object: Option<usize>,
    on_moving_ground: bool,
    /// Out of bounds last step (`checkCollisions`' flag at +0x?: below the ground floor upside
    /// down without a fresh flip): out of bounds twice in a row is death.
    out_of_bounds: bool,
    /// Player 2's rotation when dual mode last ended: a new player 2 starts with it (the game
    /// reuses the same object and `copyAttributes` doesn't copy rotation).
    second_rotation: f32,
    /// `m_lastFlipTime` and `m_gameModeChangedTime` (total time when gravity last changed and
    /// when the mode last changed; 0 = never).
    pub(crate) last_flip_time: f64,
    pub(crate) mode_changed_time: f64,
    /// Set during a step: this player touched the dual portal (`Some(y)`) or the solo portal.
    dual_request: Option<Option<f32>>,
    /// Effects to show, from presses and steps since the caller last drained this (the game
    /// does after every step; headless tools never do, so only the newest are kept).
    pub events: Vec<Event>,
    pub mode: Mode,
    pub x: f32,
    pub y: f32,
    /// y at the start of the step (the game's `m_lastPosition.y`).
    pub last_y: f32,
    /// `m_lastPosition.x` (sideways collisions measure the "up" movement along x).
    pub last_x: f32,
    pub y_velocity: f64,
    pub on_ground: bool,
    pub speed: f32,
    pub holding: bool,
    /// Options trigger 165 (`disablePlayerControls`, +0x9e3): presses and releases are ignored;
    /// the button's physical state is still kept (+0xb68) for when controls come back.
    controls_disabled: bool,
    physical_held: bool,
    pub dead: bool,
    /// Reached the end of the level: physics stops (the game then plays the end animation).
    pub finished: bool,
    /// Jump snapping: the solid last landed on (index into the world) and the player's x
    /// offset from it at that moment (the game's `m_objectSnappedTo` / `m_snapDistance`).
    pub snapped_to: Option<usize>,
    pub snap_distance: f64,
    /// Floor and ceiling of the current flying corridor (`None` in ground modes).
    pub corridor: Option<(f32, f32)>,
    /// Portals already used (each activates once per attempt).
    pub used_portals: UsedList,
    /// Deaths don't happen (benchmarks and stress runs; see `die`).
    pub noclip: bool,
    /// Visual rotation in degrees (clockwise, like cocos2d) and whether the cube is spinning.
    pub rotation: f32,
    pub spinning: bool,
    /// Spin direction, fixed when the spin starts (`runRotateAction`): a later gravity flip
    /// doesn't reverse it.
    pub spin_dir: f32,
    /// Rotation speed in degrees per second for the ball (`m_rotationSpeed`): rolling after it
    /// lands, spinning after a flip, orb or pad (`ball_spinning`, `m_ballSpinning`).
    pub rotation_speed: f32,
    pub ball_spinning: bool,
    /// The button state the physics sees (`m_jumpBuffered`): set by a press, cleared by the
    /// release, and also by a ball flip (so holding doesn't flip again).
    pub jump_buffered: bool,
    /// `m_vehicleSize`: 1, or 0.6 after a mini portal.
    pub size: f32,
    /// The node's scale, which eases to the size after a size portal (0.5 s elastic). Drawing
    /// uses it, and so does the inner kill box.
    pub scale: f32,
    scale_anim: Option<ScaleAnim>,
    /// `m_touchingGround`: set by every landing, cleared by jumps, pads, orbs, mode changes and
    /// by falling faster than 4. The cube only eases its rotation while it's set.
    pub touching_ground: bool,
    /// `m_isRising`: set by jumps, pads and orbs; cleared once the velocity passes 2g toward
    /// gravity. While set, updateJump only applies gravity (no ground-flag, spin or clamp logic).
    pub rising: bool,
    /// Per-speed constants (see `speed_constants`).
    pub base_speed: f64,
    pub y_start: f64,
    pub m_gravity: f64,
    /// `m_gravityMod` (0xb24): gravity triggers' factor.
    pub gravity_mod: f32,
    /// `m_isAccelerating`: set by the red orb; while set, flying modes skip the speed limit.
    pub accelerating: bool,
    /// Breakable blocks destroyed this attempt (indices into the world).
    pub destroyed: Vec<usize>,
    /// Custom orbs fired this step (the world spawns / toggles their groups), and
    /// `m_touchedCustomRing` (one custom orb per step, apart from the other orbs).
    pub custom_fired: Vec<usize>,
    touched_custom_ring: bool,
    /// Mirror portals: the view is flipped (visual only; the camera animates it).
    pub mirrored: bool,
    /// Seconds since the attempt started (`m_totalTime`).
    pub total_time: f64,
    /// Slope state (see slopes.rs): on a slope this step / last step, the launch velocity, the
    /// vertical running speed, the rotation the slope imposes, whether it's a ceiling slope.
    pub on_slope: bool,
    pub was_on_slope: bool,
    pub slope_velocity: f32,
    pub current_slope_yvel: f64,
    pub slope_rotation: f64,
    pub current_slope_top: bool,
    pub colliding_slope: Option<usize>,
    pub slope_start_time: f64,
    pub last_snap_y: f64,
    pub slope_downhill: bool,
    /// The ball rolling on a slope turns faster (`m_isBallRotating`, `m_rotateSpeed`).
    pub ball_slope_scaled: bool,
    pub ball_rotate_speed: f32,
    /// Gravity flipped (the game's `m_isUpsideDown`).
    pub upside_down: bool,
    /// False until the first step has run (input is ignored before that).
    pub controls_ready: bool,
    /// Press-buffer flags (`pushButton`): set on every press. `ring_jump_ready` mirrors
    /// `m_stateRingJump` (cleared by an orb, a ground jump or a release); `state_pre_jump` and
    /// `state_jump_buffered` are the snapshots orbs check (`m_statePreJump`,
    /// `m_stateJumpBuffered`), refreshed from `ring_jump_ready` and the held state after every
    /// update.
    pub ring_jump_ready: bool,
    pub state_pre_jump: bool,
    pub state_jump_buffered: bool,
    /// An orb already fired during this step (`m_touchedRing`; one orb per step).
    pub touched_ring: bool,
    /// Orbs used since the last press (`m_usedRings`, cleared by each press).
    pub used_rings: UsedList,
    /// Orbs overlapped in the last collision pass (`m_touchingRings`): a press activates these.
    pub touching_rings: Vec<usize>,
    /// Orbs overlapped during the current pass (the id set `resetTouchedRings` keeps).
    touched_this_step: Vec<usize>,
    /// Platformer mode (`m_isPlatformer` +0xb10, from the level's kA22) and its state; see
    /// platformer.rs.
    pub platformer: PlatformerState,
    /// Set during the moving-platform snap's forced `collidedWithObjectInternal` (skip check).
    pub(crate) forced_contact: bool,
    /// `m_gameState.m_levelTime` (layer+0x3a0): += the step's time / time warp, just before
    /// the order walk. Platformer position triggers fire by it (`posForTime`).
    pub level_time: f64,
}

/// The platformer fields of `PlayerObject` (offsets m1 2.2091).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PlatformerState {
    pub on: bool,
    /// `m_holdingLeft` (+0xaa1), `m_holdingRight` (+0xaa0), `m_leftPressedFirst` (+0xaa2: left
    /// was pressed last; decides when both are held).
    pub holding_left: bool,
    pub holding_right: bool,
    pub left_last: bool,
    /// `m_platformerXVelocity` (+0xa98): the x step is this times dt (0.25).
    pub x_velocity: f64,
    /// +0x9d0 (f32): time without a direction held; the slow-down grows with it.
    pub idle_time: f32,
    /// `m_isMoving` (+0xaf8), `m_hasPlatformMomentum` (+0xab0), `m_platformMomentum` (+0xab4),
    /// `m_affectedByForces` (+0xb34), `m_forcedSlopeSlide` (+0xab8), `m_isSliding` (+0xab9),
    /// `m_slopeSpeedBonus` (+0xac0), `m_isOnIce` (+0xac8), `m_forcedSlideLeft/Right`
    /// (+0xaf9/+0xafa), `m_lastSlideLeft` (+0xafb), `m_slopeJumpCount` (+0xadc),
    /// `m_slopeJumpResetTime` (+0xae0), `m_decreaseBoostSlide` (+0x9e0), `m_wasTouchingGround`
    /// (+0xad8), `m_slopeResnapped` (+0x970), `m_yVelocityBeforeSlope` (+0x560).
    pub is_moving: bool,
    pub has_momentum: bool,
    pub momentum: f32,
    pub affected_by_forces: bool,
    pub forced_slope_slide: bool,
    pub is_sliding: bool,
    pub slope_speed_bonus: f64,
    pub on_ice: bool,
    pub forced_slide_left: bool,
    pub forced_slide_right: bool,
    pub last_slide_left: bool,
    pub slope_jump_count: i32,
    pub slope_jump_reset_time: f64,
    pub decrease_boost_slide: bool,
    pub was_touching_ground: bool,
    pub slope_resnapped: bool,
    pub y_velocity_before_slope: f64,
    /// `m_lastLandTime` (+0x9c8): set by hitGround when it grounds the player; updateJump
    /// keeps the ground flag for 0.05 s after it.
    pub last_land_time: f64,
    /// `m_collidingWithLeft/Right` (+0x608/+0x610): the block last met on that side.
    pub colliding_left: Option<usize>,
    pub colliding_right: Option<usize>,
}

impl Default for Player {
    fn default() -> Self {
        Player {
            mode: Mode::Cube,
            x: 0.0,
            y: GROUND_Y,
            last_y: GROUND_Y,
            last_x: 0.0,
            y_velocity: 0.0,
            on_ground: false,
            speed: SPEED_NORMAL,
            holding: false,
            controls_disabled: false,
            physical_held: false,
            dead: false,
            finished: false,
            snapped_to: None,
            snap_distance: 0.0,
            corridor: None,
            used_portals: UsedList::default(),
            noclip: false,
            events: Vec::new(),
            dual: None,
            dual_portal_y: None,
            mode_portal_y: 0.0,
            partner: None,
            is_second: false,
            robot_boost: 1.5,
            touched_pad: false,
            rect_cache: None,
            slope_rect_width: None,
            last_ground: None,
            was_teleported: false,
            dashing: false,
            dash_y: 0.0,
            icon_spin: None,
            spin_after_dash: false,
            dash_angle: 0.0,
            pending_ball_roll: false,
            pending_speed: None,
            landed_this_step: false,
            ground_y_velocity: 0.0,
            state_no_auto_jump: 0,
            state_dart_slide: 0,
            state_hit_head: 0,
            state_flip_gravity: 0,
            state_force: 0,
            force: (0.0, 0.0),
            force_ids: Vec::new(),
            force_blocks: Default::default(),
            ring_jumped_since_press: false,
            last_spider_flip_time: 0.0,
            camera: crate::camera::Camera::new(),
            camera_triggers: crate::camera::CameraTriggers::default(),
            order_state: crate::order::OrderState::default(),
            camera_padding: (0.5, 10.0),
            fired_triggers: Vec::new(),
            pending_camera: Vec::new(),
            camera_touched: Vec::new(),
            prev_ceiling: 0.0,
            fix_gravity_bug: false,
            max_gameplay_y: MAX_GAMEPLAY_Y,
            free_mode: false,
            free_mode_portals: Default::default(),
            step_secs: 1.0 / 240.0,
            dt60: STEP_DT,
            fix_radius_collision: false,
            sideways: false,
            going_left: false,
            last_collision_bottom: -1,
            last_collision_top: -1,
            prev_collision_bottom: -1,
            prev_collision_top: -1,
            collided_object: None,
            on_moving_ground: false,
            potential_slopes: Vec::new(),
            out_of_bounds: false,
            second_rotation: 0.0,
            last_flip_time: 0.0,
            mode_changed_time: 0.0,
            dual_request: None,
            rotation: 0.0,
            spinning: false,
            spin_dir: 1.0,
            rotation_speed: 0.0,
            ball_spinning: false,
            jump_buffered: false,
            size: 1.0,
            scale: 1.0,
            scale_anim: None,
            touching_ground: false,
            rising: false,
            base_speed: BASE_SPEED,
            y_start: Y_START,
            m_gravity: M_GRAVITY,
            gravity_mod: 1.0,
            accelerating: false,
            destroyed: Vec::new(),
            mirrored: false,
            total_time: 0.0,
            on_slope: false,
            was_on_slope: false,
            slope_velocity: 0.0,
            current_slope_yvel: 0.0,
            slope_rotation: 0.0,
            current_slope_top: false,
            colliding_slope: None,
            slope_start_time: 0.0,
            last_snap_y: 0.0,
            slope_downhill: false,
            ball_slope_scaled: false,
            ball_rotate_speed: 1.0,
            upside_down: false,
            controls_ready: false,
            ring_jump_ready: false,
            state_pre_jump: false,
            state_jump_buffered: false,
            touched_ring: false,
            custom_fired: Vec::new(),
            touched_custom_ring: false,
            used_rings: UsedList::default(),
            touching_rings: Vec::new(),
            touched_this_step: Vec::new(),
            platformer: PlatformerState::default(),
            level_time: 0.0,
            forced_contact: false,
        }
    }
}

impl Player {
    /// A player at the level start, with the level's starting speed.
    pub fn spawn(world: &World) -> Player {
        let mut p = Player::default();
        p.set_speed(world.start_speed);
        p
    }

    /// `updateTimeMod`: switch speed factor and the constants that come with it.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
        let (base, rest) = speed_constants(speed);
        self.base_speed = base;
        if let Some((y_start, gravity)) = rest {
            self.y_start = y_start;
            self.m_gravity = gravity;
        }
        if self.mode == Mode::Ball {
            // Speed portals act through trigger activation, after this step's rotation
            // update: the ball's new roll rate starts next step.
            self.pending_ball_roll = true;
        }
    }

    /// +1 with normal gravity, -1 upside down.
    pub fn dir(&self) -> f64 {
        if self.upside_down { -1.0 } else { 1.0 }
    }

    /// `m_width`/`m_height`: 30, or 10 for the wave (`toggleDartMode`).
    pub fn base_width(&self) -> f32 {
        match self.mode {
            Mode::Wave => 10.0,
            // toggleSpiderMode: m_width/m_height 27.
            Mode::Spider => 27.0,
            _ => 2.0 * PLAYER_HALF,
        }
    }

    /// The collision box: the base width scaled by the vehicle size (0.6 when mini), centered.
    pub fn rect(&self) -> Rect {
        let w = self.rect_width();
        Rect { x: self.x - w * 0.5, y: self.y - w * 0.5, w, h: w }
    }

    /// The width the object rect currently reflects (see `rect_cache`).
    fn rect_width(&self) -> f32 {
        match self.rect_cache {
            Some((y, w)) if y == self.y => w,
            _ => self.base_width() * self.size,
        }
    }

    /// Half the height the floor and ceiling use (`checkCollisions`: `m_unkAngle1`·size/2, where
    /// that field is 30, or 20 for the wave, 27 for the spider).
    pub fn bounds_half(&self) -> f32 {
        let g = match self.mode {
            Mode::Wave => 20.0,
            Mode::Spider => 27.0,
            _ => 2.0 * PLAYER_HALF,
        };
        g * self.size * 0.5
    }

    /// Half the collision box height.
    pub fn half(&self) -> f32 {
        self.rect_width() * 0.5
    }

    /// Launch and thrust scale for the size (`m_vehicleSize == 1 ? 1 : 0.8`).
    pub fn size_factor(&self) -> f32 {
        if self.size == 1.0 { 1.0 } else { 0.8 }
    }

    /// The flying modes' size factor (updateJump: 0.85 when mini).
    pub fn fly_size_factor(&self) -> f32 {
        if self.size == 1.0 { 1.0 } else { 0.85 }
    }

    /// The inner box: 9×9 (`getObjectRect(0.3, 0.3)` scales the base 30 units, so it stays 9
    /// when mini — confirmed by the game's own rects in traces). Touching a solid with it kills.
    pub fn inner_rect(&self) -> Rect {
        let w = self.base_width() * 0.3;
        Rect { x: self.x - w * 0.5, y: self.y - w * 0.5, w, h: w }
    }

    /// Press or release the button. For the cube, a press on the ground jumps immediately (the
    /// game's `pushButton`, run from `processCommands` before the player update), so that step
    /// still gets gravity. Flying modes just read the held state.
    /// A press or release (`GJBaseGameLayer::handleButton`): player 1, then in dual mode player 2.
    pub fn set_button(&mut self, down: bool, world: &World) {
        self.set_button_one(down, world);
        if let Some(p2) = &mut self.dual {
            p2.set_button_one(down, world);
        }
    }

    /// The step loop's `PlayerObject::updateTimeMod(pending)` (before processCommands): a
    /// speed portal touched last step takes effect now. The ball's roll restarts at the new
    /// rate right away (the same moment as the end of the portal step).
    fn apply_pending_speed(&mut self) {
        if let Some(speed) = self.pending_speed.take() {
            self.set_speed(speed);
            if std::mem::take(&mut self.pending_ball_roll) && self.mode == Mode::Ball {
                self.run_rotate_action();
            }
        }
    }

    fn set_button_one(&mut self, down: bool, world: &World) {
        self.apply_pending_speed();
        self.physical_held = down;
        if self.controls_disabled {
            return;
        }
        // Controls are still disabled on an attempt's first step (`m_controlsDisabled`): a press
        // there is dropped entirely, so it doesn't count as held either (`m_jumpBuffered`).
        if down && !self.controls_ready {
            return;
        }
        if !down {
            // releaseButton: m_jumpBuffered and m_stateRingJump both cleared, and m_touchedPad
            // set (so a robot's hold boost ends with the release, until its next jump).
            self.holding = false;
            self.jump_buffered = false;
            self.ring_jump_ready = false;
            self.touched_pad = true;
            // releaseButton: a dash ends with the release.
            self.stop_dash();
            return;
        }
        if self.holding {
            return;
        }
        self.holding = true;
        self.ring_jumped_since_press = false;
        self.jump_buffered = true;
        self.ring_jump_ready = true;
        self.used_rings.clear();
        self.state_jump_buffered = true;
        self.state_pre_jump = true;
        self.touched_ring = false;
        self.touched_custom_ring = false;
        if !self.touching_rings.is_empty() {
            // Still overlapping orbs from the last step: the press fires them instead of a jump,
            // custom orbs first.
            let rings = self.touching_rings.clone();
            let (custom, other): (Vec<usize>, Vec<usize>) =
                rings.into_iter().partition(|&i| world.objects[i].kind == ObjectType::CUSTOM_RING);
            for index in custom.into_iter().chain(other) {
                self.ring_jump(world, index);
            }
            return;
        }
        if self.mode == Mode::Cube && self.on_ground {
            self.y_velocity = self.jump_velocity_at(self.total_time + self.step_secs);
            self.on_ground = false;
            self.touching_ground = false;
            self.rising = true;
            self.ring_jump_ready = false;
            self.run_rotate_action();
        } else if self.mode == Mode::Ball && self.on_ground {
            self.ball_flip();
        } else if self.mode == Mode::Robot && self.on_ground {
            self.robot_jump(self.total_time + self.step_secs);
        } else if self.mode == Mode::Spider && self.on_ground && !self.dashing {
            // pushButton → updateJump(0): the spider jumps at once, before the player update
            // saves m_lastPosition (so collisions see only that step's movement after it).
            self.spider_jump(world, false);
        }
    }

    /// The robot's jump (updateJump): half the cube's launch (`m_yStart`·0.5, with the slope
    /// bonus), the hold boost restarting; it doesn't spin.
    fn robot_jump(&mut self, now: f64) {
        self.y_velocity = self.jump_velocity_scaled(now, 0.5);
        self.on_ground = false;
        self.touching_ground = false;
        self.rising = true;
        self.ring_jump_ready = false;
        self.touched_pad = false;
        self.robot_boost = 0.0;
    }

    /// Ground jump velocity (updateJump): round3(sizeFactor · m_yStart · dir), all f32.
    fn jump_velocity(&self) -> f64 {
        self.jump_velocity_at(self.total_time)
    }

    /// The jump velocity with the slope bonus timed at `now` (a press is handled after the
    /// game has already advanced `m_totalTime` for the coming step).
    fn jump_velocity_at(&self, now: f64) -> f64 {
        self.jump_velocity_scaled(now, 1.0)
    }

    fn jump_velocity_scaled(&self, now: f64, scale: f32) -> f64 {
        let dir: f32 = if self.upside_down { -1.0 } else { 1.0 };
        let v = round_velocity((self.size_factor() * ((self.y_start as f32) * scale) * dir) as f64);
        // Jumping on (or right off) a slope that's carrying the player along gravity's
        // opposite: add a quarter of the slope velocity (time-scaled like the launch, except
        // for the ball), capped at 1.4× the jump.
        let sv = self.slope_velocity;
        if (self.was_on_slope || self.on_slope) && 0.0 < dir * sv {
            let t = now - self.slope_start_time;
            let mut m = t * 10.0;
            if m <= 0.4000000059604645 {
                m = 0.4000000059604645;
            }
            if 0.10000000149011612 <= t {
                m = 1.0;
            }
            let sv = if self.mode == Mode::Ball { sv } else { (m * sv as f64) as f32 };
            let boosted = round_velocity(v + (sv * 0.25) as f64);
            let cap = (v * 1.399999976158142) as f32 as f64;
            let capped = if self.upside_down { cap.max(boosted) } else { cap.min(boosted) };
            return round_velocity(capped);
        }
        v
    }

    /// The ball's "jump" (updateJump on the ground): launch at m_yStart along the current
    /// gravity, flip gravity (which halves it), then scale by 0.6.
    fn ball_flip(&mut self) {
        self.y_velocity = self.jump_velocity();
        self.touching_ground = false;
        self.rising = true;
        self.ring_jump_ready = false;
        let flip = !self.upside_down;
        self.flip_gravity(flip);
        self.y_velocity *= 0.6000000238418579;
        self.jump_buffered = false;
    }

    /// Advance one physics step: move, then collide with the world, then rotate.
    /// One physics step: player 1, then in dual mode player 2 (`GJBaseGameLayer::update`).
    pub fn step(&mut self, world: &World) {
        self.gravity_mod = world.gravity_mod;
        // a player control trigger (1932) fired last step: releaseButton / stopRotation
        if let Some((stop_jump, stop_move, stop_rotation)) = world.control_now {
            if stop_jump {
                self.holding = false;
            }
            if stop_move {
                self.platformer.holding_left = false;
                self.platformer.holding_right = false;
            }
            if stop_rotation {
                self.spinning = false;
                self.rotation_speed = 0.0;
            }
        }
        // a teleport trigger (3022) fired last step: like a teleport portal with a target group
        if let Some((x, y, keep_x, keep_y, gravity)) = world.teleport_now {
            if !keep_x {
                self.x = x as f32;
            }
            if !keep_y {
                self.y = y as f32;
            }
            self.touching_ground = false;
            self.was_teleported = true;
            match gravity {
                1 => self.flip_gravity(false),
                2 => self.flip_gravity(true),
                3 => self.flip_gravity(!self.upside_down),
                _ => {}
            }
        }
        if self.events.len() > 64 {
            self.events.drain(..32);
        }
        let mut p2 = self.dual.take();
        self.partner = p2.as_ref().map(|p| (p.mode, p.upside_down));
        self.step_one(world);
        if self.spin_after_dash && (self.on_ground || self.dashing || self.mode != Mode::Cube) {
            self.spin_after_dash = false;
        } else if self.spin_after_dash && self.y_velocity != 0.0 {
            self.spin_after_dash = false;
            self.run_rotate_action();
        }
        match self.dual_request.take() {
            // toggleDualMode(on): player 2 copies player 1, with gravity flipped and the
            // velocity negated; it isn't stepped until the next step.
            Some(Some(y)) if p2.is_none() => {
                self.dual_portal_y = Some(y);
                let mut second = self.dual_copy();
                second.dual_portal_y = Some(y);
                self.corridor = Some(corridor_at(y, dual_height(self.mode, (self.mode, self.upside_down))));
                second.corridor = self.corridor;
                self.dual = Some(Box::new(second));
                return;
            }
            // toggleDualMode(off) by player 1: player 2 just goes.
            Some(None) if p2.is_some() => {
                self.second_rotation = p2.as_ref().map_or(0.0, |p| p.rotation);
                self.end_dual();
                return;
            }
            _ => {}
        }
        let Some(p2) = p2.as_mut() else { return };
        p2.partner = Some((self.mode, self.upside_down));
        p2.corridor = self.corridor;
        p2.step_one(world);
        self.corridor = p2.corridor;
        self.events.append(&mut p2.events);
        if p2.dead {
            // (Player 1's rotation on that last step: the game sometimes keeps the old one —
            // order of its rotation and collision passes in dual mode, not modelled.)
            self.dead = true;
        }
        if p2.dual_request.take() == Some(None) {
            // The solo portal touched by player 2: player 1 takes its place (copyAttributes).
            self.take_attributes(p2);
            self.second_rotation = p2.rotation;
            self.end_dual();
            return;
        }
        self.dual = Some(std::mem::replace(p2, Box::new(Player::default())));
    }

    /// Player 2 at the start of dual mode (`copyAttributes`, then flipped gravity and the
    /// negated velocity rounded to 3 decimals, `frinta`; not on the ground).
    /// The camera's step, after the world's triggers (`updateTweenActions`, `updateCamera`):
    /// fires the camera triggers the player reached and moves the camera. Drivers call it after
    /// `World::post_step`; without it the corridors keep their unzoomed static edges.
    pub fn step_camera(&mut self, world: &World, view_w: f32) {
        let fired = std::mem::take(&mut self.pending_camera);
        if !fired.is_empty() && std::env::var_os("CADENCE_DEBUG_CAMERA").is_some() {
            eprintln!("camera at x {} y {}: {:?}", self.x, self.y, fired);
        }
        let mut cam = self.camera;
        let position = |o: u32| {
            let (x, y) = world.level_position(o as usize);
            (x as f32, y as f32)
        };
        cam.step_with(self, view_w, &fired, &position);
        self.camera = cam;
    }

    fn dual_copy(&self) -> Player {
        let mut p = self.clone();
        p.dual = None;
        p.is_second = true;
        p.events.clear();
        p.used_rings.clear();
        p.used_portals.clear();
        p.touching_rings.clear();
        p.touched_this_step.clear();
        p.destroyed.clear();
        p.upside_down = !self.upside_down;
        p.last_flip_time = self.total_time;
        p.rotation = self.second_rotation;
        p.spinning = false;
        p.ball_spinning = false;
        p.rotation_speed = 0.0;
        p.accelerating = false;
        p.y_velocity = round_velocity(-self.y_velocity);
        p.on_ground = false;
        p.touching_ground = false;
        p
    }

    /// `copyAttributes(player 1, player 2)` when player 2 leaves dual mode: position, mode,
    /// gravity, speed, size, velocity, and in effect player 2's rotation.
    fn take_attributes(&mut self, from: &Player) {
        self.x = from.x;
        self.y = from.y;
        self.last_y = from.last_y;
        self.last_x = from.last_x;
        self.total_time = from.total_time;
        if self.mode != from.mode {
            self.switch_mode(from.mode, self.mode_portal_y);
        }
        // Through flipGravity (which leaves the ground); then player 2's rotation carries on.
        self.flip_gravity(from.upside_down);
        self.rotation = from.rotation;
        self.rotation_speed = from.rotation_speed;
        self.spinning = from.spinning;
        self.set_speed(from.speed);
        self.size = from.size;
        self.scale = from.scale;
        self.y_velocity = round_velocity(from.y_velocity);
        self.jump_buffered = from.jump_buffered;
        self.ring_jump_ready = from.ring_jump_ready;
    }

    /// Back to one player: the corridor follows player 1's mode around the last mode portal
    /// (`updateDualGround` with dual mode off).
    fn end_dual(&mut self) {
        self.dual = None;
        self.dual_portal_y = None;
        self.corridor = solo_corridor(self.mode, self.mode_portal_y);
    }

    fn step_one(&mut self, world: &World) {
        self.rect_cache = None;
        if self.finished {
            return;
        }
        let (x0, y0) = (self.x, self.y);
        self.controls_ready = true;
        self.apply_pending_speed();
        // This step's game time (time warp scales it).
        self.step_secs = world.step_secs;
        self.dt60 = world.step_dt60 as f64;
        self.total_time += self.step_secs;
        // updateInternalActions (before the player update): the size animation.
        if let Some(anim) = &mut self.scale_anim {
            if anim.is_done() {
                self.scale_anim = None;
            } else {
                self.scale = anim.step(self.step_secs as f32);
            }
        }
        // resetTouchedRings: keep only the orbs overlapped during the last pass.
        let touched = std::mem::take(&mut self.touched_this_step);
        self.touching_rings.retain(|i| touched.contains(i));
        self.update(world);
        // The end of PlayerObject::update copies both flags at once (one 2-byte store), so a
        // press already spent on a jump or orb can't fire an orb while held.
        self.state_jump_buffered = self.jump_buffered;
        self.state_pre_jump = self.ring_jump_ready;
        self.touched_ring = false;
        self.touched_custom_ring = false;
        self.check_collisions(world);
        // The order-channel walk (FUN_1000e7d4c) after the collision pass, before
        // updateRotation.
        if !self.is_second {
            let tw = if world.time_warp > 0.0 { world.time_warp } else { 1.0 };
            self.level_time += world.step_secs / tw as f64;
        }
        if !self.dead && !self.is_second {
            self.order_pass(world);
            self.camera_touch_pass(world);
        }
        // A step that kills the player (or ends the level) leaves the rotation as it was.
        if !self.dead && !self.finished {
            self.update_rotation(self.x - x0, self.y - y0);
        }
        if std::mem::take(&mut self.pending_ball_roll) && self.mode == Mode::Ball {
            self.run_rotate_action();
        }
        if world.level_length > 0.0 && self.x >= world.level_length - 300.0 && !self.platformer.on {
            self.finished = true;
        }
    }

    /// Visual rotation (`PlayerObject::updateRotation`). On the ground the cube eases toward the
    /// nearest 90° by t = min(dt, speed·0.175·3·dt) per step; in the air it spins 180° per
    /// 104 steps at 1x. The ship points along its movement, eased by 0.15·dt.
    fn update_rotation(&mut self, dx: f32, dy: f32) {
        let dt = self.dt60 as f32;
        if self.dashing {
            self.rotation = -self.dash_angle;
            return;
        }
        if self.on_slope && self.mode != Mode::Ball {
            // updateRotation's dispatcher: on a slope, follow the slope's angle.
            self.update_slope_rotation(dt);
            if self.mode == Mode::Cube && self.spinning {
                self.rotation += 1.0 * (dt / 60.0) * self.rotation_speed;
            }
            return;
        }
        match self.mode {
            Mode::Cube => {
                // Touching the ground and not moving away from it: ease to the nearest 90°
                // (which also ends a spin). Otherwise only a running spin turns the cube.
                if self.touching_ground {
                    let v = self.y_velocity;
                    let settled = if self.upside_down { v <= 0.25 } else { -0.25 <= v };
                    // Platformer: a spin running on the ground carries on (0x3753e8).
                    if settled && !(self.platformer.on && self.spinning) {
                        self.spinning = false;
                        // (convertToClosestRotation returns 0 while dashing)
                        let closest = if self.dashing { 0.0 } else { closest_rotation(self.rotation) };
                        let t = dt.min(self.speed * 0.175 * 3.0 * dt);
                        self.rotation = lerp_angle(self.rotation, closest, t);
                    }
                }
                if self.spinning {
                    // m_rotationSpeed (fixed when the spin started), times dt/60.
                    self.rotation += 1.0 * (dt / 60.0) * self.rotation_speed;
                }
            }
            Mode::Ball => {
                // No easing: m_rotationSpeed (deg/s) times dt/60, scaled by m_rotateSpeed while
                // rolling on a slope (fused multiply-add, as compiled).
                if self.rotation_speed != 0.0 {
                    let f = if self.ball_slope_scaled { self.ball_rotate_speed } else { 1.0 };
                    self.rotation = ((dt / 60.0) * self.rotation_speed).mul_add(f, self.rotation);
                }
            }
            // The robot and the spider (the dispatcher 0x3753e8): settled on the ground they ease
            // to the nearest 90° like the cube; otherwise (e.g. leaving a slope) back to 0°
            // with half the step time: t = min(speed·0.175·dt/2, dt/2) (0x36ccac).
            Mode::Robot | Mode::Spider => {
                let v = self.y_velocity;
                let settled = self.touching_ground && if self.upside_down { v <= 0.25 } else { -0.25 <= v };
                if settled {
                    // convertToClosestRotation(0) returns 0 for the robot and the spider: they
                    // always settle upright, never on their side (real s12-22 after a dash).
                    let closest = 0.0;
                    let t = dt.min(self.speed * 0.175 * 3.0 * dt);
                    self.rotation = lerp_angle(self.rotation, closest, t);
                } else if self.rotation != 0.0 {
                    let half = dt * 0.5;
                    let t = (self.speed * 0.175 * half).min(half);
                    self.rotation = lerp_angle(self.rotation, 0.0, t);
                }
            }
            // The swing tilts like the ship (updateShipRotation, 0.15·dt).
            Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing => {
                // updateShipRotation: only when the player moved enough this step, toward the
                // movement direction (radians, clockwise), eased by a fixed 0.15·dt (the wave
                // 0.25, mini 0.4). The UFO only tilts: −0.4 of that angle, nose-down at most
                // 0.1 rad (upside down the other way), eased by 0.07·dt.
                if dt * 1.2 <= dy * dy + dx * dx {
                    let angle = (-dy).atan2(dx);
                    let (target, k) = if self.mode == Mode::Ufo {
                        let tilt = angle * -0.4;
                        (if self.upside_down { tilt.min(0.1) } else { tilt.max(-0.1) }, 0.07f32)
                    } else if self.mode == Mode::Wave {
                        (angle, if self.size != 1.0 { 0.4 } else { 0.25 })
                    } else {
                        (angle, 0.15)
                    };
                    let t = (k * dt).min(dt);
                    self.rotation = slerp_radians(self.rotation * 0.017453292, target, t) * 57.29578;
                }
            }
        }
    }

    /// Movement for one step (`PlayerObject::update` → `updateJump`, `updateMove`).
    fn update(&mut self, world: &World) {
        let dt = self.dt60;
        self.last_y = self.y;
        self.last_x = self.x;
        // Ground modes drop the flag in updateJump's gravity branch (see `drop_ground_flag`);
        // flying modes keep it until a mode change.
        let was_on_ground = self.on_ground;
        // updateJump always receives dt·0.9 (f32), whatever the speed: vertical motion doesn't
        // scale with speed portals (only gravity and launch speeds change with them).
        let jump_dt = dt as f32 * 0.9;
        match self.mode {
            Mode::Cube => self.update_cube_velocity(jump_dt, was_on_ground),
            Mode::Ship | Mode::Ufo => self.update_flying_velocity(jump_dt),
            Mode::Swing => self.update_swing_velocity(jump_dt),
            Mode::Ball => self.update_ball_velocity(jump_dt, was_on_ground),
            Mode::Robot => self.update_robot_velocity(jump_dt, was_on_ground),
            Mode::Spider => self.update_spider_velocity(jump_dt, was_on_ground, world),
            // updateJump for the wave: the stored velocity is the x speed (round3), up while
            // held and flipped by gravity (not doubled when mini, unlike the movement); it
            // doesn't move the wave.
            Mode::Wave => {
                // The flying branch's limit test runs for the wave too: back within the limits,
                // no longer accelerating.
                self.clear_accelerating_within_limits();
                let mut v = round_velocity(self.base_speed * self.speed as f64);
                if self.upside_down {
                    v = -v;
                }
                if !self.jump_buffered {
                    v = -v;
                }
                self.y_velocity = v;
            }
        }
        self.apply_force(jump_dt);
        // The per-step movement is a float delta added to the float position: x by
        // base·speed·dt (doubles), y by velocity·(dt·0.9). The wave moves diagonally instead:
        // y by ±x step (up while held, flipped by gravity; doubled when mini).
        // Platformer: updateMove sets the x velocity (signed; facing doesn't flip it) and the
        // step is that velocity times dt.
        let x_step = if self.platformer.on {
            self.update_move(jump_dt);
            self.platformer.x_velocity * dt
        } else {
            self.base_speed * self.speed as f64 * dt
        };
        let forward = if self.going_left && !self.platformer.on { -x_step } else { x_step };
        if self.sideways {
            // Rotated gameplay: forward along y, the vertical movement along x.
            self.y += forward as f32;
            self.x += (self.y_velocity * jump_dt as f64) as f32;
        } else {
            self.x += forward as f32;
        }
        if self.sideways {
        } else if self.dashing {
            self.y_velocity = 0.0;
            self.y += (x_step * self.dash_y) as f32;
        } else if self.mode == Mode::Wave {
            let mut dy = if self.upside_down { -x_step.abs() } else { x_step.abs() };
            if !self.jump_buffered {
                dy = -dy;
            }
            if self.size != 1.0 {
                dy += dy;
            }
            self.y += dy as f32;
        } else {
            self.y += (self.y_velocity * jump_dt as f64) as f32;
        }
        // The end of PlayerObject::update: the special-block state counters count down.
        self.state_no_auto_jump -= 1;
        self.state_dart_slide -= 1;
        self.state_hit_head -= 1;
        self.state_flip_gravity -= 1;
        self.state_force -= 1;
        self.force = (0.0, 0.0);
        self.force_ids.clear();
    }

    /// The robot in updateJump: a fresh press on the ground jumps; while rising and held (not
    /// launched by a pad, boost under 1.5) it cancels gravity; gravity is the cube's × 0.9.
    fn update_robot_velocity(&mut self, dt: f32, was_on_ground: bool) {
        if self.jump_buffered && self.ring_jump_ready && was_on_ground {
            self.robot_jump(self.total_time);
        } else if self.rising {
            if self.jump_buffered && !self.touched_pad && self.robot_boost < 1.5 {
                self.robot_boost += (dt * 0.1) as f64;
                // gravity() is already signed for upside down.
                let push = self.gravity() * dt;
                self.y_velocity = round_velocity(self.y_velocity + (ROBOT_GRAVITY_FACTOR * push) as f64);
            }
            self.rising_gravity(ROBOT_GRAVITY_FACTOR, dt);
        } else {
            self.drop_ground_flag();
            self.apply_gravity(ROBOT_GRAVITY_FACTOR, dt);
            self.drop_touching_flag();
        }
    }

    fn update_cube_velocity(&mut self, dt: f32, was_on_ground: bool) {
        if self.jump_buffered && was_on_ground {
            // Held button on the ground: updateJump jumps and skips gravity for this step.
            self.y_velocity = self.jump_velocity();
            self.on_ground = false;
            self.touching_ground = false;
            self.rising = true;
            self.ring_jump_ready = false;
            self.run_rotate_action();
        } else if self.rising {
            self.rising_gravity(CUBE_GRAVITY_FACTOR, dt);
        } else {
            self.drop_ground_flag();
            self.apply_gravity(CUBE_GRAVITY_FACTOR, dt);
            // Falling faster than 0.25 starts the spin (runRotateAction); in platformer only
            // off the ground.
            if !self.spinning && self.y_velocity * self.dir() < -0.25 && !(self.platformer.on && self.touching_ground) {
                self.run_rotate_action();
            }
            self.drop_touching_flag();
        }
    }

    /// The gravity updateJump uses: the cube's per-speed m_gravity; every other mode the fixed
    /// 0.958199.
    fn gravity(&self) -> f32 {
        let g = if matches!(self.mode, Mode::Cube | Mode::Robot) { self.m_gravity as f32 } else { GRAVITY };
        let g = if self.gravity_mod != 1.0 { g * self.gravity_mod } else { g };
        if self.upside_down { -g } else { g }
    }

    /// Gravity step (f32 term added in f64, rounded), then the ±15 terminal clamp.
    fn apply_gravity(&mut self, factor: f32, dt: f32) {
        let g = self.gravity();
        let gravity_step = factor * -(g * dt);
        self.y_velocity = round_velocity(self.y_velocity + gravity_step as f64);
        self.y_velocity = if self.upside_down {
            round_velocity(self.y_velocity.min(MAX_FALL_SPEED))
        } else {
            round_velocity(self.y_velocity.max(-MAX_FALL_SPEED))
        };
    }

    /// updateJump while rising: gravity only (no clamp); once past 2g toward gravity (the same
    /// unmirrored test as `drop_ground_flag`) the rise ends and ground contact with it.
    fn rising_gravity(&mut self, factor: f32, dt: f32) {
        let g = self.gravity();
        self.y_velocity = round_velocity(self.y_velocity + (factor * -(g * dt)) as f64);
        let done = self.past_2g();
        if done {
            self.rising = false;
            self.touching_ground = false;
        }
    }

    /// Returns whether the player counts as falling (the spin-start check runs only then).
    fn drop_ground_flag(&mut self) -> bool {
        let clear = self.past_2g();
        // Platformer: the ground flag stays for 0.05 s after a landing (unless sliding into a
        // second slope jump).
        let coyote = self.platformer.on
            && self.total_time - self.platformer.last_land_time < 0.05000000447034836
            && self.platformer_jump_allowed();
        if clear && !coyote {
            self.on_ground = false;
        }
        clear
    }

    /// updateJump's bVar8 for the ground modes: a jump is allowed unless sliding against a
    /// forced slide with two slope jumps made (which also resets the count when the slide
    /// changed side).
    fn platformer_jump_allowed(&self) -> bool {
        let (left, right, _) = self.platformer_direction();
        let pf = &self.platformer;
        if (left || !pf.forced_slide_left) && (right || !pf.forced_slide_right) {
            return true;
        }
        let count = if pf.last_slide_left != pf.forced_slide_left { 0 } else { pf.slope_jump_count };
        count < 2
    }

    /// The 2g threshold updateJump tests against right side up: 2·(float)g, or −2g while in
    /// dual mode (`m_isDual`, +0xa39; upside down the test keeps +2g either way).
    /// updateJump's "past 2g toward gravity" test (the rise ends, the ground flag drops). Without
    /// the level's gravity-bug fix (kA32, `m_fixGravityBug`) it isn't mirrored upside down
    /// (`2g < v`); with it, upside down is `v > −2g` and right side up `v < 2g` (f32 2g, no dual
    /// sign).
    fn past_2g(&self) -> bool {
        if self.fix_gravity_bug || self.platformer.on {
            let g2 = (self.m_gravity + self.m_gravity) as f32;
            if self.upside_down { ((-g2) as f64) < self.y_velocity } else { self.y_velocity < (g2 as f64) }
        } else if self.upside_down {
            self.m_gravity + self.m_gravity < self.y_velocity
        } else {
            self.y_velocity < self.up_2g()
        }
    }

    /// The level settings the player reads (`loadLevelSettings`): kA32 → `m_fixGravityBug`.
    pub fn apply_level_settings(&mut self, level: &crate::level::LevelString) {
        self.fix_gravity_bug = level.setting("kA32") == Some("1");
        self.platformer.on = level.setting("kA22") == Some("1");
        // GJBaseGameLayer::resetPlayer (0xdef64): with a spawn group (kA36, settings+0x1bc)
        // the player starts at that group's first object (x only in platformer, else x 0).
        if let Some(group) = level.setting("kA36").and_then(|g| g.parse::<i32>().ok()).filter(|&g| g >= 1) {
            if let Some(o) = level.objects.iter().find(|o| o.groups().contains(&group)) {
                self.x = if self.platformer.on { o.x() } else { 0.0 };
                self.y = o.y() + crate::world::GROUND_OFFSET;
                self.last_x = self.x;
                self.last_y = self.y;
                // The player starts touching the ground (traces: a direction held from step 0
                // accelerates at the ground rate, not the air's slope-jump factor).
                self.touching_ground = true;
            }
        }
        // GJBaseGameLayer::setupLevelStart: the level's start mode (kA2: 0 cube, 1 ship, 2 ball,
        // 3 ufo, 4 wave, 5 robot, 6 spider, 7 swing), mini (kA3) and flipped gravity (kA11),
        // as if through a portal at the start (Bloodbath starts in the ship).
        let mode = match level.setting("kA2") {
            Some("1") => Mode::Ship,
            Some("2") => Mode::Ball,
            Some("3") => Mode::Ufo,
            Some("4") => Mode::Wave,
            Some("5") => Mode::Robot,
            Some("6") => Mode::Spider,
            Some("7") => Mode::Swing,
            _ => Mode::Cube,
        };
        if mode != Mode::Cube {
            self.switch_mode(mode, self.y);
            let corridor = self.corridor;
            self.camera.start_in_corridor(corridor, crate::camera::VIEW_HEIGHT * 16.0 / 9.0);
        }
        if level.setting("kA3") == Some("1") {
            self.size = 0.6;
            self.scale = 0.6;
        }
        if level.setting("kA11") == Some("1") {
            self.flip_gravity(true);
        }
        self.fix_radius_collision = level.setting("kA39") == Some("1");
        self.max_gameplay_y = if level.setting("kA37") == Some("1") {
            // Over every object (decorations and triggers too), at its start position.
            match level.objects.iter().map(|o| o.y() + crate::world::GROUND_OFFSET).reduce(f32::max) {
                Some(top) => (if 1200.0 < top { top } else { 1200.0 }) + 90.0 + 300.0,
                None => 1590.0,
            }
        } else {
            MAX_GAMEPLAY_Y
        };
        let mut blocks = std::collections::HashMap::new();
        for (order, raw) in level.objects.iter().enumerate() {
            if matches!(raw.id(), 2069 | 3645) {
                // customObjectSetup: values clamped to ±10000.
                let f = |k: u32| raw.get_f32(k).unwrap_or(0.0).clamp(-10000.0, 10000.0);
                blocks.insert(order as u32, ForceBlock {
                    force: f(149),
                    min: f(526),
                    max: f(527),
                    relative: raw.get_i32(528).unwrap_or(0) != 0,
                    range: raw.get_i32(529).unwrap_or(0) != 0,
                    id: raw.get_i32(530).unwrap_or(0),
                });
            }
        }
        self.force_blocks = std::sync::Arc::new(blocks);
        self.free_mode_portals = std::sync::Arc::new(
            level.objects.iter().enumerate().filter(|(_, o)| o.get(111) == Some("1")).map(|(i, _)| i as u32).collect(),
        );
    }

    /// A round object (radius > 0) against the player's rect `r`: objectIntersectsCircle (the
    /// circle's center in the rect or a corner within the radius), or with the level's "fix
    /// radius collision" (kA39, LevelSettingsObject+0x1c7) the center distance against the
    /// radius plus half the rect's width.
    fn hits_round(&self, r: &Rect, o: &crate::world::Object) -> bool {
        if self.fix_radius_collision {
            let (dx, dy) = (o.x - self.x, o.y - self.y);
            (dx * dx + dy * dy).sqrt() <= r.w.mul_add(0.5, o.radius)
        } else {
            rect_hits_circle(r, o.x, o.y, o.radius)
        }
    }

    /// `ForceBlockGameObject::calculateForceToTarget` (0x183efc) for force block `index`.
    fn force_to_player(&self, world: &World, index: usize, fb: &ForceBlock) -> (f32, f32) {
        let o = &world.objects[index];
        let r = o.rect;
        let mut rad = if o.radius > 0.0 { o.radius } else { (if r.h <= r.w { r.w } else { r.h }) * 0.5 };
        if rad <= 0.0 {
            rad = 1.0;
        }
        // (vtable 0x598, taken to be the vertical flip, turns the force around.)
        let flipped = o.flip_y;
        let d = (o.x - self.x, o.y - self.y);
        let angle = if fb.relative {
            d.1.atan2(d.0) + if flipped { 0.0 } else { std::f32::consts::PI }
        } else {
            ((90.0 - o.rotation) + if flipped { 180.0 } else { 0.0 }) * 0.017453292
        };
        let mut f = fb.force;
        if fb.range {
            let t = if fb.relative {
                let dist = (d.0 * d.0 + d.1 * d.1).sqrt();
                if dist < rad { 1.0 - dist / rad } else { 0.0 }
            } else {
                let (sin, cos) = angle.sin_cos();
                (cos.mul_add(d.0, d.1.mul_add(sin, rad)) / rad) * 0.5
            };
            let t = t.clamp(0.0, 1.0);
            f = t.mul_add(fb.max - fb.min, fb.min);
        }
        let (sin, cos) = angle.sin_cos();
        (cos * f, sin * f)
    }

    /// `touchedObject` for a force block: the state, then (once per force id per step) the
    /// force toward the player summed, its length capped at 9999.
    fn touch_force_block(&mut self, world: &World, index: usize) {
        self.state_force = 2;
        let Some(fb) = self.force_blocks.get(&world.objects[index].order).copied() else { return };
        if fb.id > 0 {
            if self.force_ids.contains(&fb.id) {
                return;
            }
            self.force_ids.push(fb.id);
        }
        let v = self.force_to_player(world, index, &fb);
        self.force = (self.force.0 + v.0, self.force.1 + v.1);
        let len = (self.force.1 * self.force.1 + self.force.0 * self.force.0).sqrt();
        if 9999.0 < len {
            let k = 9999.0 / len;
            self.force = (self.force.0 * k, self.force.1 * k);
        }
    }

    /// PlayerObject::update after updateJump: the force's y, scaled per mode, joins the
    /// velocity (round3), and any force makes the player accelerating.
    fn apply_force(&mut self, jump_dt: f32) {
        if self.state_force <= 0 {
            return;
        }
        let mini = self.size != 1.0;
        let factor: f32 = match self.mode {
            Mode::Ship => if mini { 0.5875 } else { 0.47 },
            Mode::Ufo => if mini { 0.72499996 } else { 0.58 },
            Mode::Ball | Mode::Spider => 0.6,
            Mode::Robot => 0.9,
            Mode::Swing => if mini { 0.61538464 } else { 0.4 },
            _ => 1.0,
        };
        let f = factor * jump_dt * self.force.1;
        self.y_velocity = round_velocity(self.y_velocity + f as f64);
        if f != 0.0 {
            self.accelerating = true;
        }
        // (A push against gravity also grows +0x800 by f/12.94·1.5: not modeled.)
    }

    fn up_2g(&self) -> f64 {
        let g = self.m_gravity as f32;
        let g = if self.dual_portal_y.is_some() { -g } else { g };
        (g + g) as f64
    }

    /// After gravity: falling faster than 4 ends ground contact (`m_touchingGround = 0`).
    fn drop_touching_flag(&mut self) {
        if self.y_velocity * self.dir() < -4.0 {
            self.touching_ground = false;
        }
    }

    /// Spider (updateJump): a buffered press on the ground (not dashing) is the spider jump,
    /// which ends updateJump; otherwise the ball's gravity (0.6 of the fixed 0.958199, ±15).
    fn update_spider_velocity(&mut self, dt: f32, was_on_ground: bool, world: &World) {
        if self.jump_buffered && was_on_ground && !self.dashing {
            self.spider_jump(world, false);
            return;
        }
        if self.rising {
            self.rising_gravity(BALL_GRAVITY_FACTOR, dt);
            return;
        }
        self.drop_ground_flag();
        self.apply_gravity(BALL_GRAVITY_FACTOR, dt);
        self.drop_touching_flag();
    }

    /// Ball: a buffered press flips on the ground (gravity skipped that step); otherwise gravity
    /// at 0.6 of the fixed 0.958199, clamped to ±15.
    fn update_ball_velocity(&mut self, dt: f32, was_on_ground: bool) {
        if self.jump_buffered && was_on_ground {
            self.ball_flip();
            return;
        }
        if self.rising {
            self.rising_gravity(BALL_GRAVITY_FACTOR, dt);
            return;
        }
        self.drop_ground_flag();
        self.apply_gravity(BALL_GRAVITY_FACTOR, dt);
        self.drop_touching_flag();
    }

    /// updateJump's flying branch (ship, UFO, wave): back within the speed limits [-6.4, 8]
    /// (÷ size factor, mirrored upside down), no longer accelerating.
    fn clear_accelerating_within_limits(&mut self) {
        let s = self.fly_size_factor();
        let v = self.y_velocity;
        let within = if self.upside_down {
            (v <= 0.0 && (-(8.0 / s) as f64) < v) || (0.0 <= v && v < (-(-6.4 / s)) as f64)
        } else {
            (0.0 <= v && v < (8.0 / s) as f64) || (v <= 0.0 && ((-6.4 / s) as f64) < v)
        };
        if within {
            self.accelerating = false;
        }
    }

    /// The flying branch of updateJump (ship and UFO): mode-specific thrust and gravity, the
    /// speed limit [-6.4, 8] (unless accelerating), and the end of a rise.
    fn update_flying_velocity(&mut self, dt: f32) {
        let s = self.fly_size_factor();
        self.clear_accelerating_within_limits();
        let g0 = if self.gravity_mod != 1.0 { GRAVITY * self.gravity_mod } else { GRAVITY };
        let g = if self.upside_down { -g0 } else { g0 };
        // The game's "below 2g" flag; upside down the test isn't mirrored (it reads "above +2g"),
        // so an upside-down ship normally gets the past-2g factors.
        // Sideways, swing or with the level's gravity-bug fix the test is mirrored (f32 2g).
        let mirrored = self.sideways || self.mode == Mode::Swing || self.fix_gravity_bug;
        let below_2g = if mirrored {
            let g2 = (self.m_gravity + self.m_gravity) as f32 as f64;
            if self.upside_down { -g2 < self.y_velocity } else { self.y_velocity < g2 }
        } else if self.upside_down {
            self.m_gravity + self.m_gravity < self.y_velocity
        } else {
            self.y_velocity < self.up_2g()
        };
        match self.mode {
            Mode::Ufo => {
                if self.ring_jump_ready && self.jump_buffered {
                    // A fresh press: boost to 7 (mini 8·0.85) unless already rising faster.
                    self.ring_jump_ready = false;
                    let t: f32 = if self.size == 1.0 { 7.0 } else { 8.0 };
                    let target = (s * if self.upside_down { -t } else { t }) as f64;
                    let boost = if self.upside_down { target < self.y_velocity } else { self.y_velocity < target };
                    if boost {
                        let v = round_velocity(target);
                        self.y_velocity = v;
                        // On a slope moving up: plus half the slope velocity, at most 1.4× the boost.
                        if (self.was_on_slope || self.on_slope) && 0.0 < self.slope_velocity {
                            let bonus = round_velocity(v + (self.slope_velocity * 0.5) as f64);
                            let cap = (v * 1.399999976158142) as f32 as f64;
                            self.y_velocity = round_velocity(bonus.min(cap));
                        }
                    }
                }
                // Gravity ×0.8, or ×1.2 once rising faster than 2g (upside down: unless falling
                // faster than 2g).
                let below = if self.upside_down {
                    self.y_velocity <= self.m_gravity + self.m_gravity
                } else {
                    !(self.up_2g() <= self.y_velocity)
                };
                let k: f32 = if self.upside_down { if below { 1.2 } else { 0.8 } } else if below { 0.8 } else { 1.2 };
                let delta = (k * g * dt * -0.5) / s;
                self.y_velocity = round_velocity(self.y_velocity + delta as f64);
                if self.jump_buffered {
                    self.touching_ground = false;
                }
            }
            _ => {
                // Ship: holding pushes against gravity by 0.5g (0.4g once past 2g); released it
                // falls by 0.4·0.8g (0.4·1.2g past 2g).
                // Accelerating (past the speed limits), the push is −1 (thrust) while moving
                // toward gravity and 0.8 (gravity-like) while moving away from it, held or not;
                // released past 2g it's 1.2.
                let v = self.y_velocity;
                let accel_dir: f32 = if !self.upside_down {
                    if v < 0.0 { -1.0 } else { 0.8 }
                } else if v <= 0.0 {
                    0.8
                } else {
                    -1.0
                };
                let (k, direction): (f32, f32) = if self.jump_buffered {
                    (if below_2g { 0.5 } else { 0.4 }, if self.accelerating { accel_dir } else { -1.0 })
                } else {
                    let d = if self.accelerating { accel_dir } else { 0.8 };
                    (0.4, if below_2g { d } else { 1.2 })
                };
                let delta = (k * direction * -(g * dt)) / s;
                self.y_velocity = round_velocity(self.y_velocity + delta as f64);
                if self.jump_buffered {
                    self.touching_ground = false;
                }
            }
        }
        if !self.accelerating {
            let (fall, rise) = (((0.8f32 * 8.0) / s) as f64, (8.0f32 / s) as f64);
            let (low, high) = if self.upside_down { (-rise, fall) } else { (-fall, rise) };
            self.y_velocity = round_velocity(self.y_velocity.max(low));
            self.y_velocity = round_velocity(self.y_velocity.min(high));
        }
        let rising = if self.upside_down {
            self.y_velocity <= self.m_gravity + self.m_gravity
        } else {
            self.up_2g() <= self.y_velocity
        };
        if !rising {
            self.rising = false;
        }
    }

    /// The swing in updateJump (0x368e30): a fresh press flips gravity and keeps 0.8 of the
    /// velocity (f32); gravity is 0.4 of the fixed 0.958199 (0.6 mini); the limit is ±8 (mini
    /// too) unless accelerating; the rise ends below 2g (f32, mirrored upside down).
    fn update_swing_velocity(&mut self, dt: f32) {
        self.clear_accelerating_within_limits();
        if self.ring_jump_ready && self.jump_buffered {
            self.ring_jump_ready = false;
            let v = self.y_velocity as f32;
            let up = !self.upside_down;
            self.flip_gravity(up);
            self.y_velocity = round_velocity((v * 0.8) as f64);
        }
        let factor: f32 = if self.size == 1.0 { 0.4 } else { 0.6 };
        let g0 = if self.gravity_mod != 1.0 { GRAVITY * self.gravity_mod } else { GRAVITY };
        let g = if self.upside_down { -g0 } else { g0 };
        self.y_velocity = round_velocity(self.y_velocity + (factor * -(g * dt)) as f64);
        if self.jump_buffered {
            self.touching_ground = false;
        }
        if !self.accelerating {
            self.y_velocity = round_velocity(self.y_velocity.max(-8.0));
            self.y_velocity = round_velocity(self.y_velocity.min(8.0));
        }
        let g2 = (self.m_gravity + self.m_gravity) as f32 as f64;
        let rising = if self.upside_down { self.y_velocity <= -g2 } else { g2 <= self.y_velocity };
        if !rising {
            self.rising = false;
        }
    }

    /// The game's `checkCollisions`: floor/ceiling, then portals and other special objects in
    /// section order, then solids (backwards, each may move the player), then hazards against
    /// the updated position.
    fn check_collisions(&mut self, world: &World) {
        // The step's collision-log reset (GJBaseGameLayer::update, 0x10036e5c8): this step's
        // collision ids become last step's.
        self.prev_collision_bottom = self.last_collision_bottom;
        self.prev_collision_top = self.last_collision_top;
        self.last_collision_bottom = -1;
        self.last_collision_top = -1;
        self.rect_cache = Some((self.y, self.base_width() * self.size));
        self.slope_rect_width = None;
        self.landed_this_step = false;
        self.begin_slope_pass();
        if self.collide_bounds() {
            return;
        }

        let me = self.rect();
        let (min_x, max_x) = (me.x, me.max_x());
        // Slopes are gathered with a rect twice their size, so look half a slope further.
        let reach = world.max_slope_width() * 0.5;
        let mut special: Vec<usize> = world
            .near_class(0, min_x - reach, max_x + reach)
            .map(|(i, _)| i)
            .collect();
        if crate::debug_counts() {
            crate::count("special candidates", special.len());
        }
        special.sort_by_cached_key(|&i| (world.objects[i].section, world.objects[i].order));

        let mut slope_calls = 0usize;
        let mut touched = 0usize;
        for index in special {
            let obj = &world.objects[index];
            if obj.kind == ObjectType::SLOPE {
                // Slopes are gathered with a rect twice their size (getObjectRect(2, 2)); the
                // slope code decides.
                let r = obj.rect;
                let wide = Rect { x: r.x - r.w * 0.5, y: r.y - r.h * 0.5, w: r.w * 2.0, h: r.h * 2.0 };
                if self.rect().intersects(&wide) {
                    slope_calls += 1;
                    self.collide_slope(world, index, self.dt60 as f32);
                    if self.dead {
                        return;
                    }
                }
                continue;
            }
            // The game's object rect is cached until the player is moved (`setPosition` marks it
            // dirty): a size portal earlier in this pass doesn't resize it yet.
            let r = self.rect();
            // Rotated objects (non-quarter turns), orbs included (`updateIsOriented` in
            // PlayLayer::addObject), are touched only through their oriented box against the
            // player's box turned by its rotation (real s13/s14-22: a −45° dash orb).
            // Round objects (the round force block, 3645) are touched through the circle test
            // (objectIntersectsCircle: center in the rect or a corner within the radius).
            let touching = if obj.radius > 0.0 {
                self.hits_round(&r, obj)
            } else {
                r.intersects(&obj.rect)
                && (obj.obb.as_ref().is_none_or(|obb| {
                    // The oriented box (`updateOrientedBox`) uses the current size, not the
                    // rect cached for the pass: a mode portal earlier in it already counts.
                    let w = self.base_width() * self.size;
                    let mine = crate::world::oriented_box(self.x, self.y, w, w, self.rotation);
                    crate::world::obb_overlaps(&mine, obb)
                }))
            };
            if touching {
                touched += 1;
                let mode = self.mode;
                self.touch_special(world, index);
                // A rotated portal is touched through the player's oriented box: after its mode
                // toggle (setScaleX marks the box dirty) the box is rebuilt with the new mode's
                // size, and the rect (the box's bounds) follows for the rest of the pass.
                if obj.obb.is_some() && self.mode != mode {
                    self.rect_cache = None;
                }
            }
        }

        if crate::debug_counts() {
            crate::count("slope calls", slope_calls);
            crate::count("specials touched", touched);
        }
        // The game gathers whole sections and tests each solid against the player's rect as it
        // is then: a snap (setPosition) can refresh the rect to a new mode's size mid-pass, so
        // gather a little wider than the pass-start rect.
        let mut solids: Vec<usize> = world
            .near_class(1, min_x - 30.0, max_x + 30.0)
            .filter(|(i, o)| {
                o.kind == ObjectType::SOLID || (o.kind == ObjectType::BREAKABLE && !self.destroyed.contains(i))
            })
            .map(|(i, _)| i)
            .collect();
        // Sections are 2-D (x then y, 100 units each; `checkCollisions` 0xe18e8), sorted by
        // unique id within; solids are then tested in reverse.
        // Interim rule (empirical): only objects an area move has offset sort by their y
        // section too (the Tower 5001 bridge, t3-5001); everything else by x section and
        // unique id (level 18's jump snaps and Fingerdash's moving blocks need that).
        // (cached keys: the key is computed once per solid, not per comparison; keys are unique,
        // so the order is the same)
        solids.sort_by_cached_key(|&i| {
            let o = &world.objects[i];
            let ysec = world.area_section_y(o.order, o.y);
            std::cmp::Reverse((o.section, ysec, o.order))
        });
        if crate::debug_counts() {
            crate::count("solid candidates", solids.len());
        }
        let mut probe = self.last_ground.is_some() && self.on_moving_ground;
        for index in solids {
            if probe && world.objects[index].grouped && self.test_for_moving(world, index) {
                probe = false;
            }
            if self.rect().intersects(&world.objects[index].rect) {
                self.collide_solid(world, index);
                if self.dead {
                    return;
                }
            }
        }
        let me = self.rect();
        // Round hazards reach past their rect: look a radius further (the game gathers every
        // hazard in the nearby sections and lets the exact test decide).
        let reach = world.max_radius();
        if crate::debug_counts() {
            crate::count("hazard candidates", world.near_class(2, min_x - reach, max_x + reach).count());
        }
        for (_, obj) in world.near_class(2, min_x - reach, max_x + reach) {
            let hit = if obj.radius > 0.0 {
                self.hits_round(&me, obj)
            } else if let Some(obb) = &obj.obb {
                // Against an oriented hitbox the player's box turns with its visual rotation
                // (PlayerObject::getObjectRotation is getRotation).
                let w = self.base_width() * self.size;
                let mine = crate::world::oriented_box(self.x, self.y, w, w, self.rotation);
                me.intersects(&obj.rect) && crate::world::obb_overlaps(&mine, obb)
            } else {
                me.intersects(&obj.rect)
            };
            if hit {
                self.die("check_collisions:978");
                return;
            }
        }
        self.end_slope_pass();
        self.snap_to_moving_ground(world);
    }

    /// `PlayerObject::testForMoving`: a grouped block that moved away from the player since its
    /// last position (down, or up when upside down) and that the player still touches with the
    /// block grown 5·dt toward the player becomes `m_collidedObject`. The game tests y even
    /// while the gameplay is sideways (0x3704f4 has no sideways case).
    fn test_for_moving(&mut self, world: &World, index: usize) -> bool {
        let o = &world.objects[index];
        let cur = (o.pos.0 as f32, o.pos.1 as f32);
        if cur == o.last_pos {
            return false;
        }
        let away = if self.upside_down { o.last_pos.1 < cur.1 } else { cur.1 < o.last_pos.1 };
        if !away {
            return false;
        }
        let ext = 5.0 * self.dt60 as f32;
        let mut r = o.rect;
        r.h += ext;
        if self.upside_down {
            r.y -= ext;
        }
        if self.rect().intersects(&r) {
            self.collided_object = Some(index);
            return true;
        }
        false
    }

    /// The moving-ground part of the game's post-collision step (0x36e834): if the block last
    /// landed on moved away under the player this step (or nothing was landed on), it's
    /// forgotten; if it moved away and the player is still within 5·dt of it, the player is
    /// snapped back onto it (`collidedWithObjectInternal` forced) — moving platforms carry the
    /// player down.
    fn snap_to_moving_ground(&mut self, world: &World) {
        // Platformer (FUN_10036e834, before the moving-away test): standing on a block
        // (m_lastGroundObject, touching the ground) carries the player by the block's x
        // movement this step, f32 (cur − last), unless the block is non-stick x (key 136,
        // +0x50f, not modelled). Slope riding (m_preLastGroundObject) isn't ported.
        if self.platformer.on {
            if let Some(g) = self.last_ground.filter(|&g| self.touching_ground && !world.objects[g].non_stick.0) {
                let o = &world.objects[g];
                let dx = o.pos.0 as f32 - o.last_pos.0;
                if self.sideways {
                    self.y += (o.pos.1 as f32 - o.last_pos.1) as f64 as f32;
                } else {
                    self.x += dx as f64 as f32;
                }
            }
            self.platformer.forced_slide_left = false;
            self.platformer.forced_slide_right = false;
        }
        let collided = self.collided_object.take();
        let mut moved_away = false;
        if let Some(g) = self.last_ground.filter(|&g| !self.on_slope && !world.objects[g].non_stick.1) {
            let o = &world.objects[g];
            let cur = (o.pos.0 as f32, o.pos.1 as f32);
            if cur != o.last_pos {
                // Sideways the gravity axis is x (the block isn't turned here).
                let (c, l) = if self.sideways { (cur.0, o.last_pos.0) } else { (cur.1, o.last_pos.1) };
                moved_away = if self.upside_down { l < c } else { c < l };
            }
        }
        if std::env::var_os("CADENCE_DEBUG_SNAP").is_some() {
            eprintln!("snap t {:.4} ground {:?} moved_away {} landed {} pos {:?}", self.total_time, self.last_ground.map(|g| (world.objects[g].order, world.objects[g].pos, world.objects[g].last_pos)), moved_away, self.landed_this_step, (self.x, self.y));
        }
        if !moved_away && self.landed_this_step {
            return;
        }
        self.on_moving_ground = false;
        let Some(g) = self.last_ground.take() else { return };
        if !moved_away {
            return;
        }
        // 5 units per frame (10 in platformer; 20 for key-? objects +0x514, not modelled).
        let ext = (if world.objects[g].extra_sticky { 20.0 } else if self.platformer.on { 10.0 } else { 5.0 }) * self.dt60 as f32;
        let r = world.objects[g].rect;
        let me = self.rect();
        let (grown, gap) = match (self.sideways, self.upside_down) {
            (false, true) => (Rect { y: r.y - ext, h: r.h + ext, ..r }, r.y - me.max_y()),
            (false, false) => (Rect { h: r.h + ext, ..r }, me.y - r.max_y()),
            (true, true) => (Rect { x: r.x - ext, w: r.w + ext, ..r }, r.x - me.max_x()),
            (true, false) => (Rect { w: r.w + ext, ..r }, me.x - r.max_x()),
        };
        let target = if me.intersects(&grown) { Some(g) } else { collided };
        if let Some(t) = target.filter(|_| gap < ext) {
            self.on_moving_ground = true;
            // collidedWithObjectInternal(dt, obj, rect, skipCheck = true).
            self.forced_contact = true;
            self.collide_solid_rect(world, t, world.objects[t].rect, false);
            self.forced_contact = false;
        }
    }

    /// Floor (and ceiling in a flying corridor). Touching either calls the game's `hitGround`,
    /// which marks the player on the ground only when the hit is gentle (velocity <= 5).
    /// Returns true when the collision pass ends here.
    fn collide_bounds(&mut self) -> bool {
        // Free mode (GJBaseGameLayer+0x311): checkCollisions leaves the corridor out (bVar31).
        let corridor = if self.free_mode { None } else { self.corridor };
        let (floor, ceiling) = match corridor {
            Some(c) => self.camera.corridor_bounds(c),
            None => (GROUND, f32::INFINITY),
        };
        // The ceiling moved since last step (the ground sliding or zooming): a floor/ceiling
        // contact keeps the velocity (canStickToGround: always, but ship and wave only without a
        // pending orb press), and the snap is 1 further while a zoom runs (can't stick).
        let moving = corridor.is_some() && ceiling - std::mem::replace(&mut self.prev_ceiling, ceiling) != 0.0;
        let stick = if matches!(self.mode, Mode::Ship | Mode::Wave) { !self.ring_jump_ready } else { true };
        let zooming = self.camera.zoom != self.camera.target_zoom;
        // hitGround tests the velocity along gravity, before it's zeroed.
        let gentle = self.y_velocity * self.dir() <= 5.0;
        let half = self.bounds_half();
        let was_out = std::mem::replace(&mut self.out_of_bounds, false);
        // checkCollisions reads and clears m_wasTeleported first: right after a teleport the
        // flying/ball corridor's floor and ceiling are skipped once.
        let teleported = std::mem::replace(&mut self.was_teleported, false);
        // Above the level's top (`m_maxGameplayY`: 2790 for the official levels) is out of
        // bounds too; twice in a row kills.
        // (Shrunk players get g·(1 − size)·0.5 more: +0xa50·(1 − m_vehicleSize)·0.5.)
        let shrink = if self.size == 1.0 {
            0.0
        } else {
            let g = match self.mode {
                Mode::Wave => 20.0,
                Mode::Spider => 27.0,
                _ => 2.0 * PLAYER_HALF,
            };
            g * (1.0 - self.size) * 0.5
        };
        if shrink + self.max_gameplay_y < self.y {
            self.out_of_bounds = true;
            if was_out {
                self.die("above the level");
                return true;
            }
        }
        if teleported && corridor.is_some() {
            return false;
        }
        if self.upside_down && corridor.is_none() && self.mode == Mode::Cube && self.y - half < floor {
            // Upside down on the ground floor: only just after a flip (isSafeFlip, 0.1 s) is
            // the player put back on the floor. Otherwise it's out of bounds: it carries on
            // below the floor, and still being out of bounds next step kills it.
            let safe = self.last_flip_time != 0.0 && self.total_time - self.last_flip_time < 0.10000000149011612;
            if !safe {
                self.out_of_bounds = true;
                if was_out {
                    self.die("out of bounds");
                    return true;
                }
                return false;
            }
            // (isSafeFlip branch): placed on the floor, grounded, spin stopped, but not
            // "touching" (no easing), and the rest of the collision pass is skipped.
            self.y = floor + half;
            if gentle {
                self.on_ground = true;
            }
            self.y_velocity = 0.0;
            self.spinning = false;
            self.touching_ground = false;
            return true;
        }
        // Strict on both sides (checkCollisions: y < floor + half, y > ceiling − half): exactly
        // touching isn't a contact.
        if self.y - half < floor {
            let saved = self.y_velocity;
            self.y = floor + half + if !stick && zooming { 1.0 } else { 0.0 };
            if gentle {
                self.on_ground = true;
            }
            if self.y_velocity < 0.0 || !self.mode.is_flying() {
                self.y_velocity = 0.0;
            }
            self.hit_ground();
            if moving && stick && !self.upside_down && saved < self.y_velocity {
                self.y_velocity = saved;
            }
        } else if self.y + half > ceiling {
            let saved = self.y_velocity;
            self.y = ceiling - half;
            // Upside down the ceiling is the ground (the gravity-side test). Otherwise it's a
            // head bump, grounded only when slow either way (a ball falling away from it at
            // 7.9 isn't; a wave pushing into it at 7.8 isn't).
            let counts = if self.upside_down { gentle } else { self.y_velocity.abs() <= 5.0 };
            if counts {
                self.on_ground = true;
            }
            if self.y_velocity > 0.0 {
                self.y_velocity = 0.0;
            }
            // A fast head bump isn't a ground hit at all (the ball keeps its roll).
            if counts || self.upside_down {
                self.hit_ground();
            }
            if moving && stick && self.upside_down && self.y_velocity < saved {
                self.y_velocity = saved;
            }
        }
        false
    }

    /// Portals (`collisionCheckObjects` switch on type).
    fn touch_special(&mut self, world: &World, index: usize) {
        if self.used_portals.contains(&index) {
            return;
        }
        let obj = &world.objects[index];
        match obj.kind {
            // `touchedObject` (0x383c28), every step the block is touched.
            ObjectType::SPECIAL_BLOCK => match obj.id {
                2069 | 3645 => self.touch_force_block(world, index),
                1755 => self.state_dart_slide = 2,
                1813 => self.state_no_auto_jump = 2,
                1859 => self.state_hit_head = 2,
                2866 => self.state_flip_gravity = 2,
                1829 => {
                    if self.dashing {
                        self.stop_dash();
                        self.jump_buffered = false;
                    }
                }
                _ => {}
            },
            ObjectType::SHIP_PORTAL | ObjectType::UFO_PORTAL | ObjectType::CUBE_PORTAL | ObjectType::BALL_PORTAL | ObjectType::WAVE_PORTAL | ObjectType::ROBOT_PORTAL | ObjectType::SPIDER_PORTAL | ObjectType::SWING_PORTAL => {
                let to = match obj.kind {
                    ObjectType::SHIP_PORTAL => Mode::Ship,
                    ObjectType::UFO_PORTAL => Mode::Ufo,
                    ObjectType::WAVE_PORTAL => Mode::Wave,
                    ObjectType::ROBOT_PORTAL => Mode::Robot,
                    ObjectType::BALL_PORTAL => Mode::Ball,
                    ObjectType::SPIDER_PORTAL => Mode::Spider,
                    ObjectType::SWING_PORTAL => Mode::Swing,
                    _ => Mode::Cube,
                };
                // playerWillSwitchMode: the portal's "free mode" (key 111, +0x6d4) becomes the
                // layer's (+0x311): no corridor floor/ceiling while it's on.
                self.free_mode = self.free_mode_portals.contains(&obj.order);
                if let Some(cam) = obj.portal_camera {
                    self.camera_padding = cam;
                }
                if self.mode != to {
                    self.used_portals.push(index);
                    let color = mode_circle_color(if to == Mode::Cube { self.mode } else { to });
                    self.events.push(Event::Portal { index, radius: 50.0, color });
                    // playerWillSwitchMode: in dual mode, a player entering the other's mode
                    // takes the opposite gravity.
                    if let Some((other, other_upside_down)) = self.partner {
                        if other == to {
                            self.flip_gravity(!other_upside_down);
                        }
                    }
                    self.switch_mode(to, obj.y);
                } else {
                    // Already in this mode: the portal still activates (once) and
                    // playerWillSwitchMode → updateDualGround re-centers the corridor on it.
                    self.used_portals.push(index);
                    self.mode_portal_y = obj.y;
                    self.corridor = match (self.dual_portal_y, self.partner) {
                        (Some(y), Some(other)) => Some(corridor_at(y, dual_height(to, other))),
                        _ => solo_corridor(to, obj.y),
                    };
                }
            }
            ObjectType::TELEPORT_PORTAL => {
                // GJBaseGameLayer::teleportPlayer: x stays, y goes to the orange end (the blue
                // portal's current y plus the start offset between the two), and the player
                // no longer touches the ground (`rotateGameplayOnly` clears m_touchingGround).
                self.used_portals.push(index);
                self.events.push(Event::Portal { index, radius: 50.0, color: [255, 200, 0] });
                if obj.id == 747 {
                    self.y = obj.pos.1 as f32 + obj.teleport_dy;
                } else if let Some((tx, ty)) = obj.teleport_target.and_then(|g| world.group_main_position(g)) {
                    // A target group: its main object's position (keys 352/353 keep x / y).
                    if !obj.teleport_keep.0 {
                        self.x = tx as f32;
                    }
                    if !obj.teleport_keep.1 {
                        self.y = ty as f32;
                    }
                } else {
                    self.y = obj.pos.1 as f32 + obj.teleport_dy;
                }
                self.touching_ground = false;
                self.was_teleported = true;
                // Key 354: the gravity after the teleport.
                match obj.teleport_gravity {
                    1 => self.flip_gravity(false),
                    2 => self.flip_gravity(true),
                    3 => self.flip_gravity(!self.upside_down),
                    _ => {}
                }
                // updateStaticForce: a velocity of the given strength out of the orange portal,
                // at (target type 749: 180) − its rotation (+ 180 when flipped; the orange
                // portal's flip isn't known, taken as unflipped: Dash's only one then pushes up).
                if let Some((force, additive)) = obj.teleport_force {
                    if force == 0.0 && !additive {
                        self.accelerating = false;
                        self.y_velocity = 0.0;
                    } else {
                        let angle = 180.0 - obj.rotation;
                        let a = angle * 0.017453292f32;
                        let (fx, fy) = (a.cos(), a.sin());
                        let len = (fy * fy + fx * fx).sqrt();
                        if 0.0 < len {
                            let k = force / len;
                            let (vx, vy) = (fx * k, fy * k);
                            let v = if self.sideways { vx } else { vy };
                            self.accelerating = true;
                            self.y_velocity = if additive { self.y_velocity + v as f64 } else { v as f64 };
                        }
                    }
                }
            }
            ObjectType::DUAL_PORTAL | ObjectType::SOLO_PORTAL => {
                self.used_portals.push(index);
                let on = obj.kind == ObjectType::DUAL_PORTAL;
                if on != self.dual_portal_y.is_some() {
                    self.dual_request = Some(if on { Some(obj.y) } else { None });
                    if on {
                        self.events.push(Event::Portal { index, radius: 50.0, color: [255, 150, 0] });
                    } else {
                        self.events.push(Event::Portal { index, radius: 50.0, color: [0, 255, 100] });
                    }
                }
            }
            ObjectType::MIRROR_PORTAL | ObjectType::MIRROR_OFF_PORTAL => {
                self.used_portals.push(index);
                let mirrored = obj.kind == ObjectType::MIRROR_PORTAL;
                if mirrored != self.mirrored {
                    // toggleFlipped: orange into the mirror, cyan out of it
                    let color = if mirrored { [255, 150, 0] } else { [0, 255, 255] };
                    self.events.push(Event::Portal { index, radius: 50.0, color });
                }
                self.mirrored = mirrored;
            }
            ObjectType::SPEED_PORTAL => {
                // Touch-triggered once (triggerObject → updateTimeMod).
                self.used_portals.push(index);
                // updateTimeMod(speed, 0): stored on the layer (+0x488) and handed to the
                // player at the start of the next step; the rest of this pass keeps the old
                // speed (a slope met after the portal still uses it).
                if let Some(speed) = crate::world::speed_portal_factor(obj.id) {
                    self.pending_speed = Some(speed);
                }
            }
            ObjectType::NORMAL_SIZE_PORTAL | ObjectType::MINI_PORTAL => {
                // togglePlayerScale: the hitbox changes at once (the sprite eases over 0.5 s);
                // a rolling ball restarts its roll at the new rate.
                self.used_portals.push(index);
                let size = if obj.kind == ObjectType::MINI_PORTAL { 0.6 } else { 1.0 };
                if self.size != size {
                    // togglePlayerScale: green to normal size, pink to mini
                    let color = if size == 1.0 { [0, 255, 150] } else { [255, 0, 150] };
                    self.events.push(Event::Portal { index, radius: 45.0, color });
                    self.size = size;
                    self.scale_anim = Some(ScaleAnim::new(self.scale, size));
                    if self.mode == Mode::Ball && self.spinning {
                        self.run_rotate_action();
                    }
                }
            }
            ObjectType::INVERSE_GRAVITY_PORTAL | ObjectType::NORMAL_GRAVITY_PORTAL => {
                self.used_portals.push(index);
                let flip = obj.kind == ObjectType::INVERSE_GRAVITY_PORTAL;
                if flip != self.upside_down {
                    // flipGravity: yellow when flipping upside down, blue back
                    let color = if flip { [255, 200, 0] } else { [0, 150, 255] };
                    self.events.push(Event::Portal { index, radius: 45.0, color });
                }
                self.flip_gravity(flip);
            }
            ObjectType::YELLOW_PAD | ObjectType::PINK_PAD | ObjectType::RED_PAD => {
                self.used_portals.push(index);
                // GJBaseGameLayer::bumpPlayer: the strength by pad and mode.
                let mini = self.size < 1.0;
                let power = match obj.kind {
                    ObjectType::PINK_PAD => match self.mode {
                        Mode::Ship => 0.35,
                        Mode::Ufo => 0.4,
                        Mode::Ball | Mode::Spider => 0.7,
                        _ => 0.65,
                    },
                    ObjectType::RED_PAD => match self.mode {
                        Mode::Ship => if mini { 0.95 } else { 0.63 },
                        Mode::Ufo => if mini { 0.98 } else { 0.6 },
                        _ => 1.25,
                    },
                    _ => 1.0,
                };
                self.propel(power);
                // bumpPlayer sets m_touchedPad (ending a robot's hold boost) only without
                // m_fixRobotJump, which loadLevelSettings turns on unless the level's settings
                // (+0x1c4) say otherwise — on for the official levels.
                self.events.push(Event::Pad(index));
                // PlayerObject::bumpPlayer: accelerating only after the red pad.
                self.accelerating = obj.kind == ObjectType::RED_PAD;
            }
            kind if is_ring(kind) => {
                // playerTouchedRing: remember the overlap; ground modes fire the orb on contact,
                // flying modes only through a press (pushButton → ringJump).
                if !self.touching_rings.contains(&index) {
                    self.touching_rings.push(index);
                }
                self.touched_this_step.push(index);
                if !self.mode.is_flying() {
                    self.ring_jump(world, index);
                }
                return;
            }
            ObjectType::GRAVITY_PAD => {
                // gravBumpPlayer: only a pad facing the current gravity works (upright pads for
                // normal gravity, flipped ones upside down); it launches, then flips gravity.
                if obj.is_facing_down() != self.upside_down {
                    return;
                }
                self.used_portals.push(index);
                self.ring_jumped_since_press = true;
                self.propel(0.8);
                self.events.push(Event::Pad(index));
                let flip = !obj.is_facing_down();
                self.flip_gravity(flip);
            }
            _ => {}
        }
    }

    /// Mode portals (`switchedToMode` and the toggle*Mode functions). Leaving or entering a
    /// flying mode halves the velocity (so UFO → ship quarters it), sets rotation 0 and leaves
    /// the ground; leaving the ball sets the upright rotation for the gravity. Every switch
    /// stops rotation; ship → cube starts the cube's spin at once. The corridor follows the mode.
    fn switch_mode(&mut self, to: Mode, portal_y: f32) {
        let from = self.mode;
        let rotation_before = self.rotation;
        let base_before = self.base_width();
        self.mode_changed_time = self.total_time;
        if matches!(to, Mode::Robot | Mode::Spider) {
            // toggleRobotMode / toggleSpiderMode: they never turn, the rotation is reset.
            self.robot_boost = 1.5;
            self.rotation = 0.0;
        }
        if from.is_flying() {
            self.y_velocity *= 0.5;
            self.rotation = 0.0;
            self.on_ground = false;
            self.touching_ground = false;
        }
        // Leaving the ball or the swing (toggleRollMode / toggleSwingMode) sets the upright
        // rotation for the gravity.
        if matches!(from, Mode::Ball | Mode::Swing) {
            self.rotation = if self.upside_down { 180.0 } else { 0.0 };
        }
        // toggleSpiderMode / toggleRobotMode turn the other modes off first and set rotation 0
        // last, so an upside-down swing or ball doesn't leave the spider at 180° (real s12-22).
        if matches!(to, Mode::Robot | Mode::Spider) {
            self.rotation = 0.0;
        }
        self.mode = to;
        if to.is_flying() {
            self.y_velocity *= 0.5;
            self.rotation = 0.0;
            self.on_ground = false;
            self.touching_ground = false;
        }
        self.spinning = false;
        self.ball_spinning = false;
        self.rotation_speed = 0.0;
        self.mode_portal_y = portal_y;
        self.corridor = match (self.dual_portal_y, self.partner) {
            (Some(y), Some(other)) => Some(corridor_at(y, dual_height(to, other))),
            _ => solo_corridor(to, portal_y),
        };
        if to == Mode::Cube && from.is_flying() {
            self.run_rotate_action();
        }
        // setRotation marks the cached object rect dirty: the rest of the pass sees the new
        // mode's size (real s19-22: wave → cube next to a slope).
        // The slope code reads the rect through getObjectRect, which recomputes it with the new
        // mode's m_width after setRotation marked it dirty (real s19-22: wave → cube next to a
        // ceiling slope); the solid pass keeps the rect it had (real w2/w3-18, r5-20).
        if self.rotation != rotation_before {
            if let Some((_, w)) = self.rect_cache {
                self.slope_rect_width = Some(w / base_before * self.base_width());
            }
        }
    }

    /// `PlayerObject::ringJump` for the jump orbs. Fires only for a press that hasn't fired an
    /// orb this step and while it's held (or still buffered from this step's press); each orb
    /// once per press. Launch speed is `m_yStart` (f32) times the orb's factor for the mode.
    fn ring_jump(&mut self, world: &World, index: usize) {
        if self.dead || self.used_rings.contains(&index) {
            return;
        }
        if world.objects[index].kind == ObjectType::CUSTOM_RING {
            // Its own touched flag; firing it spawns / toggles its group and (2.2 changes)
            // clears m_stateRingJump, so the hold fires no more orbs.
            if !(self.state_pre_jump && self.state_jump_buffered) || self.touched_custom_ring || self.dashing {
                return;
            }
            self.used_rings.push(index);
            self.used_portals.push(index);
            self.touched_custom_ring = true;
            self.touching_rings.retain(|&i| i != index);
            self.custom_fired.push(index);
            self.ring_jump_ready = false;
            return;
        }
        // No orb fires while dashing.
        if !(self.state_pre_jump && self.state_jump_buffered) || self.touched_ring || self.dashing {
            return;
        }
        let kind = world.objects[index].kind;
        self.used_rings.push(index);
        self.events.push(Event::Orb(index));
        // Marked activated by the player: collisionCheckObjects skips it for the rest of the
        // attempt (orbs without the 2.2 multi-activate option).
        self.used_portals.push(index);
        self.touched_ring = true;
        self.ring_jumped_since_press = true;
        self.touching_rings.retain(|&i| i != index);
        if kind == ObjectType::SPIDER_RING {
            // ringJump: an orb facing the other gravity flips first; then spiderTestJump, which
            // turns a cube or robot to 0/180 for the new gravity.
            if world.objects[index].is_facing_down() != self.upside_down {
                let flip = !self.upside_down;
                self.flip_gravity(flip);
            }
            self.spider_jump(world, false);
            if matches!(self.mode, Mode::Cube | Mode::Robot) {
                // spiderTestJump: 180/0, sideways 90/270.
                self.rotation = match (self.sideways, self.upside_down) {
                    (false, true) => 180.0,
                    (false, false) => 0.0,
                    (true, true) => 90.0,
                    (true, false) => 270.0,
                };
            }
            return;
        }
        if kind == ObjectType::BLACK_RING {
            // The black orb: toward gravity at 15 (ship, wave 14; UFO 11.2; spider 16.5),
            // 3-decimal rounding; it leaves the ground and rising flags alone.
            self.ring_jump_ready = false;
            let dir: f32 = if self.upside_down { -1.0 } else { 1.0 };
            let v = match self.mode {
                Mode::Ship | Mode::Wave | Mode::Swing => dir * -14.0,
                Mode::Ufo => (dir * -14.0) * 0.8,
                // The spider 1.1 times as hard (16.5).
                Mode::Spider => (dir * -15.0) * 1.1,
                _ => dir * -15.0,
            };
            self.y_velocity = round_velocity(v as f64);
            if self.mode == Mode::Ball {
                self.ball_spin(self.upside_down);
                self.jump_buffered = false;
            } else {
                self.run_rotate_action();
            }
            self.accelerating = true;
            return;
        }
        self.on_ground = false;
        self.touching_ground = false;
        self.rising = true;
        self.ring_jump_ready = false;
        if matches!(kind, ObjectType::DASH_RING | ObjectType::PINK_DASH_RING) {
            if kind == ObjectType::PINK_DASH_RING {
                let flip = !self.upside_down;
                self.flip_gravity(flip);
            }
            self.start_dash(&world.objects[index]);
            return;
        }
        let y_start = self.y_start as f32;
        let ship = self.mode == Mode::Ship;
        let ball = self.mode == Mode::Ball;
        let spider = self.mode == Mode::Spider;
        let ufo = self.mode == Mode::Ufo;
        let mini = self.size != 1.0;
        let v: f32 = match kind {
            ObjectType::GRAVITY_RING => y_start * 0.8,
            ObjectType::GREEN_RING => if ship { y_start * 0.7 } else { y_start },
            ObjectType::PINK_RING => y_start * if ship { 0.37 } else if ufo { 0.42 } else if ball { 0.77 } else { 0.72 },
            ObjectType::YELLOW_RING if self.mode == Mode::Robot => y_start * 0.9,
            ObjectType::RED_RING => {
                if ship {
                    if mini { y_start * 1.4 } else { y_start }
                } else if ufo {
                    y_start * if mini { 1.36 } else { 1.02 }
                } else {
                    y_start * if ball || spider { 1.34 } else if self.mode == Mode::Robot { 1.28 } else { 1.38 }
                }
            }
            _ => y_start,
        };
        if kind == ObjectType::RED_RING {
            self.accelerating = true;
        }
        if kind == ObjectType::GREEN_RING {
            // The green orb flips first, so it launches along the new gravity.
            let flip = !self.upside_down;
            self.flip_gravity(flip);
        }
        let size = self.size_factor();
        let v = if self.upside_down { -v } else { v };
        self.y_velocity = round_velocity((size * v) as f64);
        if ball {
            // runBallRotation2, then the ball keeps 70% of the launch and drops the press.
            self.ball_spin(self.upside_down);
            self.y_velocity *= 0.699999988079071;
            self.jump_buffered = false;
        } else if spider {
            // The spider also keeps 70% and drops the press (no spin).
            self.y_velocity *= 0.699999988079071;
            self.jump_buffered = false;
        } else if self.mode == Mode::Swing {
            // The swing keeps 60% and drops the press.
            self.run_rotate_action();
            self.y_velocity *= 0.6000000238418579;
            self.jump_buffered = false;
        } else {
            self.run_rotate_action();
        }
        if kind == ObjectType::GRAVITY_RING {
            // The blue orb launches along the old gravity, then flips (halving the velocity).
            let flip = !self.upside_down;
            self.flip_gravity(flip);
        }
    }

    /// The player dies (`destroyPlayer`). `CADENCE_DEBUG_DEATH` logs where and why. With
    /// `noclip` (tests and benchmarks) nothing happens, like a noclip hook on `destroyPlayer`:
    /// the rest of the step goes on as the game's code does after that call.
    pub(crate) fn die(&mut self, why: &str) {
        if self.noclip {
            return;
        }
        if std::env::var_os("CADENCE_DEBUG_DEATH").is_some() {
            eprintln!("death {why}: p{} x {} y {} vy {} mode {:?} upside {}", 1 + self.is_second as u8, self.x, self.y, self.y_velocity, self.mode, self.upside_down);
        }
        self.dead = true;
    }

    /// Start the cube's normal spin (runRotateAction).
    /// `m_slopeEndTime` (+0xaf0): not tracked yet (slopes in platformer).
    pub(crate) fn slope_end_time_pub(&self) -> f64 {
        -1.0e9
    }

    pub(crate) fn controls_disabled_pub(&self) -> bool {
        self.controls_disabled
    }

    pub(crate) fn run_rotate_action_pub(&mut self) {
        self.run_rotate_action();
    }

    pub(crate) fn start_spin(&mut self) {
        self.run_rotate_action();
    }

    /// `runRotateAction`: start (or restart) the cube's spin along the current gravity; the
    /// ball starts rolling at 120°/0.2 s.
    fn run_rotate_action(&mut self) {
        // Platformer (0x36cab4): the spin is stopped, and the cube only starts a new one while
        // a direction (or a forced slide) is held; the ball doesn't roll this way.
        if self.platformer.on {
            let pf = &self.platformer;
            let held = pf.holding_left || pf.holding_right || pf.forced_slide_left || pf.forced_slide_right;
            if self.mode == Mode::Ball || (self.mode == Mode::Cube && !held) {
                self.spinning = false;
                self.rotation_speed = 0.0;
                return;
            }
        }
        match self.mode {
            Mode::Cube => {
                // ±180° per 0.43333334 s (mini 0.33333334), set now and kept for the spin.
                self.spinning = true;
                // ±180 by gravity, flipped again going left and again sideways.
                let mut d = self.dir() as f32;
                if self.going_left {
                    d = -d;
                }
                if self.sideways {
                    d = -d;
                }
                self.spin_dir = d;
                let half_turn = if self.size != 1.0 { 0.33333334f32 } else { 0.43333334 };
                self.rotation_speed = (1.0 * (self.spin_dir * 180.0)) / half_turn;
            }
            Mode::Ball => {
                self.spinning = true;
                self.ball_spinning = false;
                self.ball_slope_scaled = false;
                self.ball_rotate_speed = 1.0;
                // runBallRotation: ±120 by gravity, reversed going left, the result negated
                // sideways.
                let mut deg: i32 = if self.upside_down { -120 } else { 120 };
                if self.going_left {
                    deg = -deg;
                }
                let t = if self.size != 1.0 { 0.16000001f32 } else { 0.2 };
                self.rotation_speed = deg as f32 / (t * self.ball_speed_factor());
                if self.sideways {
                    self.rotation_speed = -self.rotation_speed;
                }
            }
            Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Robot | Mode::Spider | Mode::Swing => {}
        }
    }

    /// `stopDashing` (classic mode). The cube (and ball, ufo... all but ship, ufo, wave, swing,
    /// robot, spider) takes the icon sprite's rotation, which spins (CCRotateBy 288° per 0.3 s,
    /// repeated) during the dash from 0 — driven by the cocos scheduler's wall-clock frame time,
    /// so not reproducible; taken as its start value 0.
    fn stop_dash(&mut self) {
        if !self.dashing {
            return;
        }
        self.dashing = false;
        // (Not ship, ufo, wave, swing, robot or spider:) the player takes the icon's rotation.
        if matches!(self.mode, Mode::Cube | Mode::Ball) {
            self.rotation = self.icon_spin.map_or(0.0, |s| s.rotation);
        }
        self.icon_spin = None;
        // The ball rolls again (runBallRotation's rate).
        if self.mode == Mode::Ball {
            self.run_rotate_action();
        }
        // An airborne cube spins on, at the jump rate, from the step after it starts falling
        // (real Fingerdash traces: the dash orbs' ends, both gravities).
        if self.mode == Mode::Cube && !self.on_ground && !self.platformer.on {
            self.spin_after_dash = true;
        }
    }

    /// A cocos scheduler tick (one rendered frame) before this step's update starts: the
    /// action manager steps the sprite actions by `dt` (the frame's time). See `IconSpin`.
    pub fn scheduler_tick(&mut self, dt: f32) {
        if let Some(spin) = &mut self.icon_spin {
            spin.step(dt);
        }
        if let Some(p2) = &mut self.dual {
            p2.scheduler_tick(dt);
        }
    }

    /// The order-channel walk (see `order.rs`): fire the current channel's triggers the player
    /// has passed, in order; world triggers go to `fired_triggers`, camera ones to the camera
    /// step, rotate-gameplay ones act now (channel switch, then the turn).
    fn order_pass(&mut self, world: &World) {
        let lists = std::sync::Arc::clone(&self.camera_triggers.order);
        for _ in 0..100_000 {
            let channel = self.order_state.channel;
            let Some(list) = lists.channels.get(&channel) else { break };
            let next = self.order_state.next.get(&channel).copied().unwrap_or(0);
            let Some(item) = list.get(next) else { break };
            let reversed = self.order_state.reversed.get(&channel).copied().unwrap_or(false);
            // Platformer (0x1000e7d4c): the walk follows time, not the player:
            // posForTime(levelTime) = 311.58011 · levelTime (f32).
            let px = if self.platformer.on { 311.5801f32 * self.level_time as f32 } else { self.x };
            let passed = match (self.sideways, reversed) {
                (false, false) => item.x <= px,
                (false, true) => px <= item.x,
                (true, false) => item.y <= self.y,
                (true, true) => self.y <= item.y,
            };
            if !passed {
                break;
            }
            self.order_state.next.insert(channel, next + 1);
            if std::env::var_os("CADENCE_DEBUG_ORDER").is_some() {
                eprintln!("order fire t {} x {} item {:?}", self.level_time, self.x, item);
            }
            // A trigger whose object is toggled off is passed over.
            if !world.enabled.get(item.order as usize).copied().unwrap_or(true) {
                continue;
            }
            match item.kind {
                crate::order::OrderKind::World(order) => self.fired_triggers.push(order),
                crate::order::OrderKind::Camera(i) => {
                    if let Some((switch, rotate)) = self.camera_triggers.rotation_channel(i) {
                        if let Some((target, reversed)) = switch {
                            self.order_state.channel = target;
                            self.order_state.reversed.insert(target, reversed);
                        }
                        if rotate {
                            if let Some(crate::camera::CameraAction::RotateGameplay { dir, dir2, edit, over, mod_x, mod_y }) =
                                self.camera_triggers.action(i, world)
                            {
                                self.rotate_gameplay(dir, dir2, edit, over, mod_x, mod_y);
                            }
                        }
                    } else if let Some(action) = self.camera_triggers.action(i, world) {
                        self.pending_camera.push(action);
                    }
                }
                crate::order::OrderKind::Options(p1) => {
                    if p1 == 1 {
                        // disablePlayerControls: release everything, then ignore the button.
                        let held = self.physical_held;
                        self.set_button_one(false, world);
                        self.physical_held = held;
                        self.controls_disabled = true;
                    } else if p1 == -1 && self.controls_disabled {
                        // enablePlayerControls: a button still held is pressed again.
                        self.controls_disabled = false;
                        if self.physical_held {
                            self.set_button_one(true, world);
                        }
                    }
                }
                crate::order::OrderKind::End => {
                    self.finished = true;
                    break;
                }
                crate::order::OrderKind::Other => {}
            }
        }
    }

    /// Touch-triggered camera triggers either player's rect meets fire once, after the
    /// order-channel walk (the world's touch triggers fire by the same rect test).
    fn camera_touch_pass(&mut self, world: &World) {
        let n = self.camera_triggers.touch_count();
        if n == 0 {
            return;
        }
        self.camera_touched.resize(n, false);
        let rects = [Some(self.rect()), self.dual.as_ref().filter(|p2| !p2.dead).map(|p2| p2.rect())];
        for i in 0..n {
            if self.camera_touched[i] {
                continue;
            }
            let Some(order) = rects.iter().flatten().find_map(|r| self.camera_triggers.touches(i, r)) else { continue };
            // A trigger whose object is toggled off stays armed.
            if !world.enabled.get(order as usize).copied().unwrap_or(true) {
                continue;
            }
            self.camera_touched[i] = true;
            match self.camera_triggers.touch_action(i, world) {
                Some(crate::camera::CameraAction::RotateGameplay { dir, dir2, edit, over, mod_x, mod_y }) => {
                    self.rotate_gameplay(dir, dir2, edit, over, mod_x, mod_y);
                }
                Some(action) => self.pending_camera.push(action),
                None => {}
            }
        }
    }

    /// Before a step's inputs: the scheduler tick if this step starts a rendered frame.
    pub fn begin_step(&mut self, world: &World) {
        self.custom_fired.clear();
        self.fired_triggers.clear();
        if let Some(p2) = &mut self.dual {
            p2.custom_fired.clear();
        }
        if let Some(dt) = world.scheduler_tick_due() {
            self.scheduler_tick(dt);
        }
    }

    /// `startDashing` (classic mode): the dash angle is the orb's rotation (+180 when flipped)
    /// negated mod 360, wrapped to ±180, mirrored past ±90 and limited to ±70°; the dash moves
    /// y by tan(angle) per unit of x.
    fn start_dash(&mut self, orb: &crate::world::Object) {
        if self.dashing {
            return;
        }
        self.dashing = true;
        let mut a = orb.rotation;
        if orb.flip_x {
            a += 180.0;
        }
        let mut ang = (-a) % 360.0;
        if ang > 180.0 {
            ang -= 360.0;
        } else if ang < -180.0 {
            ang += 360.0;
        }
        if ang.abs() > 90.0 {
            ang = if ang > 0.0 { 180.0 } else { -180.0 } - ang;
            if ang > 180.0 {
                ang -= 360.0;
            } else if ang < -180.0 {
                ang += 360.0;
            }
        }
        let ang = ang.clamp(-70.0, 70.0);
        let rad = ang * 0.017_453_292_f32;
        let (c, s) = ((rad as f64).cos() as f32, (rad as f64).sin() as f32);
        self.dash_y = (s / c.abs()) as f64;
        // The icon points along the dash (cocos rotation is clockwise) and stops spinning.
        self.dash_angle = ang;
        self.rotation = -ang;
        self.spinning = false;
        self.rotation_speed = 0.0;
        // updateDashArt: the icon spins (not ship, ufo, wave, swing, robot, spider).
        if matches!(self.mode, Mode::Cube | Mode::Ball) {
            let angle = if self.sideways != self.upside_down { -288.0 } else { 288.0 };
            self.icon_spin = Some(IconSpin::new(angle));
        }
    }

    /// The ball's fast spin after a flip or an orb (`flipGravity`, `runBallRotation2`):
    /// ∓340°/(0.8·size) per second, against the direction it rolls under that gravity.
    fn ball_spin(&mut self, upside_down: bool) {
        // Computed in doubles in the game (flipGravity, runBallRotation2), then stored as f32.
        // runBallRotation2: reversed going left (not sideways).
        let mut deg: i32 = if upside_down { 340 } else { -340 };
        if self.going_left {
            deg = -deg;
        }
        let size = self.size_factor() as f64;
        let rate = (1.0f32 * deg as f32) as f64 / (size * 0.8 * self.ball_speed_factor() as f64);
        self.rotation_speed = rate as f32;
        self.ball_spinning = true;
    }

    /// The ball's speed table, for other modules.
    pub(crate) fn ball_speed_factor_pub(&self) -> f32 {
        self.ball_speed_factor()
    }

    /// The ball's rotation scale for the speed (slower speeds spin faster per second).
    fn ball_speed_factor(&self) -> f32 {
        match self.speed {
            s if s == 0.7 => 1.2405638,
            s if s == 1.1 => 0.80424345,
            s if s == 1.3 => 0.6657693,
            s if s == 1.6 => 0.5409375,
            _ => 1.0,
        }
    }

    /// `hitGround`'s rotation part (any landing, floor or block): a ball that was spinning, or
    /// not rolling yet, starts rolling.
    pub(crate) fn hit_ground(&mut self) {
        if !self.touching_ground {
            self.events.push(Event::Land { slope_angle: None });
        }
        self.touching_ground = true;
        if self.mode == Mode::Ball && self.ball_slope_scaled && !self.on_slope {
            self.spinning = false;
            self.ball_slope_scaled = false;
            self.rotation_speed = 0.0;
        }
        match self.mode {
            Mode::Ball if self.ball_spinning || !self.spinning => self.run_rotate_action(),
            Mode::Cube => self.spinning = false,
            _ => {}
        }
    }

    /// `propellPlayer`: launch with `power * 16` along the current gravity (mini: 0.8x).
    fn propel(&mut self, power: f32) {
        let size = self.size_factor();
        let v = if self.upside_down { -(power * 16.0) } else { power * 16.0 };
        self.y_velocity = round_velocity((size * v) as f64);
        if matches!(self.mode, Mode::Ball | Mode::Spider) {
            self.y_velocity *= 0.6000000238418579;
        }
        self.on_ground = false;
        self.touching_ground = false;
        self.rising = true;
        self.run_rotate_action();
    }

    /// `PlayerObject::rotateGameplay` (0x37a3c0), classic mode: direction 3/4 turn the gameplay
    /// sideways, 1/4 flip gravity, the second direction 2/3 makes the player go left. When that
    /// changes anything, the velocity becomes the old forward speed (or the trigger's edited
    /// values), the player rises, leaves the ground and (cube) spins.
    pub fn rotate_gameplay(&mut self, dir: i32, dir2: i32, edit: bool, over: bool, mod_x: f32, mod_y: f32) {
        let speed_x = self.base_speed * self.speed as f64;
        let v = self.y_velocity;
        let was_sideways = self.sideways;
        let sideways = (dir - 3) as u32 <= 1;
        self.sideways = sideways;
        let was_up = self.upside_down;
        self.flip_gravity(dir == 1 || dir == 4);
        let was_left = self.going_left;
        self.going_left = (dir2 & !1) == 2;
        // doReversePlayer (always, before the no-change return): the ball restarts its roll for
        // the new direction.
        if self.mode == Mode::Ball {
            self.run_rotate_action();
        }
        // Without a change of axis nothing else happens (the velocity, the collision ids, the
        // spin only change when the gameplay turns sideways or back).
        let _ = (was_up, was_left);
        if was_sideways == sideways {
            return;
        }
        self.last_collision_bottom = -1;
        self.last_collision_top = -1;
        self.prev_collision_bottom = -1;
        self.prev_collision_top = -1;
        // (x speed, y velocity) swapped, optionally edited.
        let (mut a, mut b) = (v as f32, speed_x as f32);
        if edit {
            if over {
                a = mod_x;
            } else {
                a *= mod_x;
                b = (speed_x as f32) * mod_y;
            }
            b = if over { mod_y } else { b };
        }
        let _ = a;
        self.y_velocity = b as f64;
        // m_isAccelerating (+0x90a) on (the "no speed limit" flag; off with the trigger's 0x74d).
        self.accelerating = true;
        self.rising = true;
        self.touching_ground = false;
        // runRotateAction only when not already rotating (m_isRotating).
        if self.mode == Mode::Cube && !self.spinning {
            self.run_rotate_action();
        }
    }

    pub(crate) fn flip_gravity_pub(&mut self, upside_down: bool) {
        self.flip_gravity(upside_down);
    }

    /// `flipGravity`: switch gravity, halving the velocity; leaves the ground.
    fn flip_gravity(&mut self, upside_down: bool) {
        if self.upside_down == upside_down {
            return;
        }
        self.upside_down = upside_down;
        self.last_flip_time = self.total_time;
        // The collision logs and ids are cleared.
        self.last_collision_bottom = -1;
        self.last_collision_top = -1;
        self.prev_collision_bottom = -1;
        self.prev_collision_top = -1;
        // On a slope the downhill sense turns over with gravity (m_slopeIsDownhill ^= 1).
        if self.was_on_slope || self.on_slope {
            self.slope_downhill = !self.slope_downhill;
        }
        self.y_velocity *= 0.5;
        self.on_ground = false;
        if self.mode == Mode::Ball {
            self.spinning = false;
            self.ball_slope_scaled = false;
            self.ball_spin(upside_down);
        }
    }

    /// One solid (from `collidedWithObjectInternal`). Cube: land on top within 10 units, else
    /// the inner box kills. Ship: land on top or slide under within 6 units, else the inner box
    /// kills.
    fn collide_solid(&mut self, world: &World, index: usize) {
        self.collide_solid_rect(world, index, world.objects[index].rect, false);
    }

    /// A solid contact against `block` (a block's rect, or a slope's flat side): the game's
    /// `collidedWithObjectInternal` (collide.rs). `strip`: a slope's flat side, which the game
    /// passes with its own rect and no object (the slope is only the owner: no jump snap).
    /// Sideways (`handleRotatedCollisionInternal`, 0x370698) the object is turned −90° about
    /// the player first and the player's displacement is turned back after. A strip keeps the
    /// unturned rect it was passed (the game turns only the object).
    pub(crate) fn collide_solid_rect(&mut self, world: &World, index: usize, block: Rect, strip: bool) {
        let obj = if strip { None } else { Some(index) };
        if !self.sideways {
            self.collided_with_object_internal(world, obj, Some(index), block, self.dt60 as f32, None);
            return;
        }
        let (px, py) = (self.x, self.y);
        let turned = self.turn_object(world, index);
        let rect = if strip { block } else { turned.rect };
        self.collided_with_object_internal(world, obj, Some(index), rect, self.dt60 as f32, Some(&turned));
        let (x1, y1) = (self.x, self.y);
        self.x = px + (y1 - py);
        self.y = py - (x1 - px);
    }

    /// Jump snapping (`checkSnapJumpToObject`): landing on a solid that sits at a standard jump
    /// distance from the previous one nudges x (by at most the tolerance) so the landing offset
    /// matches the previous landing.
    /// `at`: the object's turned position in a sideways collision.
    pub(crate) fn snap_jump_to(&mut self, world: &World, index: usize, at: Option<(f32, f32)>) {
        let obj = &world.objects[index];
        let (obj_x, obj_y) = at.unwrap_or((obj.x, obj.y));
        if let Some(prev_index) = self.snapped_to.filter(|&p| p != index) {
            let prev = &world.objects[prev_index];
            if prev.kind == ObjectType::SOLID {
                let (dx, dy) = (obj_x - prev.x, obj_y - prev.y);
                let mini = self.size != 1.0;
                // (tolerance, 1 up, 1 down, 2 up) forward distances per speed
                let (tol, a, b, c): (f32, f32, f32, f32) = match self.speed {
                    s if s == 0.7 => (1.0, 90.0, 120.0, 60.0),
                    s if s == 1.1 => (2.0, if mini { 90.0 } else { 150.0 }, 195.0, 120.0),
                    s if s == 1.3 => (2.0, if mini { 90.0 } else { 180.0 }, 225.0, 135.0),
                    _ => (1.0, if mini { 90.0 } else { 120.0 }, 150.0, 90.0),
                };
                let up = if self.upside_down { -30.0 } else { 30.0 }; // gravity direction * 30
                let matches = ((dx - a).abs() <= tol && (dy - up).abs() <= tol)
                    || ((dx - b).abs() <= tol && (dy + up).abs() <= tol)
                    || ((dx - c).abs() <= tol && (dy - 2.0 * up).abs() <= tol);
                if matches {
                    let target = self.snap_distance + obj_x as f64;
                    let current = self.x as f64;
                    let tol = tol as f64;
                    let target = if (target - current).abs() > tol {
                        if target <= current { current - tol } else { current + tol }
                    } else {
                        target
                    };
                    self.x = target as f32;
                }
            }
        }
        self.snapped_to = Some(index);
        self.snap_distance = (self.x - obj_x) as f64;
    }
}

/// A force block's settings (keys 149 force, 526/527 min/max, 528 relative, 529 range,
/// 530 force id).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceBlock {
    pub force: f32,
    pub min: f32,
    pub max: f32,
    pub relative: bool,
    pub range: bool,
    pub id: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_keeps_the_games_neighbours() {
        assert_eq!(round_velocity(8.156 - 0.216), 7.939_999_999_999_999_5);
        assert_eq!(round_velocity(10.964 - 0.216), 10.747_999_999_999_999);
        assert_eq!(round_velocity(3.0), 3.0);
    }

    #[test]
    fn closest_rotation_breaks_ties_like_the_game() {
        assert_eq!(closest_rotation(315.374), 270.0);
        assert_eq!(closest_rotation(316.0), 360.0);
        assert_eq!(closest_rotation(28.5), 0.0);
        assert_eq!(closest_rotation(-122.6), -90.0);
    }

    #[test]
    fn one_x_speed_per_step() {
        let mut p = Player::default();
        // The step's length comes from the frame `pre_step` starts.
        let mut world = World::from_level(&crate::level::LevelString::parse("kA13,0;"));
        world.pre_step((p.x, p.y));
        p.step(&world);
        assert_eq!(p.x, 1.298_250_4);
        assert_eq!(p.y, GROUND_Y);
    }
}

fn is_ring(kind: ObjectType) -> bool {
    matches!(
        kind,
        ObjectType::YELLOW_RING
            | ObjectType::PINK_RING
            | ObjectType::GRAVITY_RING
            | ObjectType::GREEN_RING
            | ObjectType::RED_RING
            | ObjectType::DASH_RING
            | ObjectType::PINK_DASH_RING
            | ObjectType::BLACK_RING
            | ObjectType::SPIDER_RING
            | ObjectType::CUSTOM_RING
    )
}

/// The corridor a mode gets outside dual mode: flying modes 300, the ball 240, none for the cube.
fn solo_corridor(mode: Mode, portal_y: f32) -> Option<(f32, f32)> {
    match mode {
        Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing => Some(corridor_at(portal_y, SHIP_CORRIDOR)),
        Mode::Ball => Some(corridor_at(portal_y, BALL_CORRIDOR)),
        // The spider portal opens a 270 corridor (updateDualGround: type 33 animates it in).
        Mode::Spider => Some(corridor_at(portal_y, 270.0)),
        Mode::Cube | Mode::Robot => None,
    }
}

/// The shared corridor's height in dual mode (`updateDualGround`): the taller of this player's
/// new mode (ball counted as 270) and the other player's (flying 300, ball 240, else 270).
fn dual_height(to: Mode, (other, _): (Mode, bool)) -> f32 {
    let own = match to {
        Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing => SHIP_CORRIDOR,
        _ => 270.0,
    };
    let other = match other {
        Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing => SHIP_CORRIDOR,
        Mode::Ball => BALL_CORRIDOR,
        Mode::Cube | Mode::Robot | Mode::Spider => 270.0,
    };
    own.max(other)
}

/// Floor and ceiling of the corridor a portal at height `y` opens (`animateInDualGroundNew`):
/// the floor is snapped down to the 30-unit grid and kept at or above the ground.
fn corridor_at(y: f32, height: f32) -> (f32, f32) {
    let floor = (((y - height * 0.5) / 30.0) as i32 as f32 * 30.0).max(GROUND);
    (floor, floor + height)
}

/// The game's `objectIntersectsCircle`: the circle's center inside the rect, or one of the
/// rect's corners strictly closer than the radius. (A circle that only crosses the middle of an
/// edge doesn't count — kept as in the game.)
pub(crate) fn rect_hits_circle_pub(r: &Rect, cx: f32, cy: f32, radius: f32) -> bool {
    rect_hits_circle(r, cx, cy, radius)
}

fn rect_hits_circle(r: &Rect, cx: f32, cy: f32, radius: f32) -> bool {
    if cx >= r.x && cx <= r.max_x() && cy >= r.y && cy <= r.max_y() {
        return true;
    }
    let corners = [(r.x, r.y), (r.x, r.max_y()), (r.max_x(), r.max_y()), (r.max_x(), r.y)];
    corners.iter().any(|&(x, y)| {
        let (dx, dy) = (x - cx, y - cy);
        (dx * dx + dy * dy).sqrt() < radius
    })
}

/// The size portal's `CCEaseElasticOut(CCScaleTo(0.5, size))` (period 0.3), stepped like a
/// cocos2d action: the first tick runs at elapsed 0, later ones add dt; done once elapsed ≥ 0.5.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScaleAnim {
    start: f32,
    delta: f32,
    elapsed: f32,
    first_tick: bool,
}

impl ScaleAnim {
    const DURATION: f32 = 0.5;
    const PERIOD: f32 = 0.3;

    fn new(from: f32, to: f32) -> ScaleAnim {
        ScaleAnim { start: from, delta: to - from, elapsed: 0.0, first_tick: true }
    }

    fn is_done(&self) -> bool {
        self.elapsed >= Self::DURATION
    }

    fn step(&mut self, dt: f32) -> f32 {
        if self.first_tick {
            self.first_tick = false;
            self.elapsed = 0.0;
        } else {
            self.elapsed += dt;
        }
        let t = (self.elapsed / Self::DURATION.max(f32::EPSILON)).clamp(0.0, 1.0);
        let eased = if t == 0.0 || t == 1.0 {
            t
        } else {
            let p = (t * -10.0).exp2();
            let a = (t + Self::PERIOD * -0.25) * std::f32::consts::PI;
            let a = (a + a) / Self::PERIOD;
            p.mul_add(a.sin(), 1.0)
        };
        self.delta.mul_add(eased, self.start)
    }
}

/// `CCRepeatForever(CCRotateBy(0.3, angle))` on the icon sprite (cocos2d-x 2.2 semantics, f32):
/// the first step after starting only marks the start; each loop restarts from the current
/// rotation and carries the overshoot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconSpin {
    start: f32,
    angle: f32,
    elapsed: f32,
    first_tick: bool,
    pub rotation: f32,
}

impl IconSpin {
    const DURATION: f32 = 0.3;

    fn new(angle: f32) -> IconSpin {
        IconSpin { start: 0.0, angle, elapsed: 0.0, first_tick: true, rotation: 0.0 }
    }

    fn inner_step(&mut self, dt: f32) {
        if self.first_tick {
            self.first_tick = false;
            self.elapsed = 0.0;
        } else {
            self.elapsed += dt;
        }
        let t = (self.elapsed / Self::DURATION.max(f32::EPSILON)).min(1.0).max(0.0);
        self.rotation = self.angle.mul_add(t, self.start);
    }

    fn step(&mut self, dt: f32) {
        self.inner_step(dt);
        if self.elapsed >= Self::DURATION {
            let diff = self.elapsed - Self::DURATION;
            self.start = self.rotation;
            self.elapsed = 0.0;
            self.first_tick = true;
            self.inner_step(0.0);
            self.inner_step(diff);
        }
    }
}

/// The orbs, pads and portals an attempt has used, in order (`push`, `len`, `clear` as a list),
/// with a set beside it for `contains`: a linear search made touching big stacks of them
/// quadratic (pc killer touches up to 1,719 in one step).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsedList {
    list: Vec<usize>,
    set: std::collections::HashSet<usize>,
}

impl UsedList {
    pub fn push(&mut self, index: usize) {
        self.list.push(index);
        self.set.insert(index);
    }

    pub fn contains(&self, index: &usize) -> bool {
        self.set.contains(index)
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.set.clear();
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

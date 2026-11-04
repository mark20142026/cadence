//! Shader triggers (2.2, ids 2904–2924): the game's `ShaderLayer` post-process.
//!
//! `ShaderGameObject::customObjectSetup` (0x145f0c) reads the trigger keys (absent keys are 0),
//! `GJBaseGameLayer::triggerShaderCommand` (0xf2cfc) passes them to `ShaderLayer::trigger*`,
//! which tween values of a `GJShaderState` (`tweenValueAuto` 0x33d730: from the current value to
//! the trigger's over its duration with its easing; duration ≤ 0 sets it at once). Once a frame
//! `GJBaseGameLayer::updateShaderLayer` (0xf3448, after `GJEffectManager::updateEffects` with the
//! same frame seconds) runs `ShaderLayer::update` (0x33c00c: time += dt, `GJValueTween::step`
//! 0xcbdc0 on each tween) and `performCalculations` (0x340d4c, the `pre*Shader` functions), and
//! `ShaderLayer::visit` (0x340ff0, `FUN_10033c488`) sets the center uniforms. The fragment
//! shader itself is embedded in the binary (0x91ab52); Cadence's port is
//! `crates/cadence-game/src/postfx.wgsl`. `Uniforms` matches its uniform block.
//!
//! Positions here are in screen UV with y up (0..1 across the view), like the game's
//! `v_texCoord` on its render texture.

use std::collections::HashMap;

use crate::level::RawObject;

/// The trigger ids (`+0x404`).
pub mod id {
    pub const SETUP: i32 = 2904;
    pub const SHOCKWAVE: i32 = 2905;
    pub const SHOCKLINE: i32 = 2907;
    pub const GLITCH: i32 = 2909;
    pub const CHROMATIC: i32 = 2910;
    pub const CHROMATIC_GLITCH: i32 = 2911;
    pub const PIXELATE: i32 = 2912;
    pub const LENS_CIRCLE: i32 = 2913;
    pub const RADIAL_BLUR: i32 = 2914;
    pub const MOTION_BLUR: i32 = 2915;
    pub const BULGE: i32 = 2916;
    pub const PINCH: i32 = 2917;
    pub const GRAYSCALE: i32 = 2919;
    pub const SEPIA: i32 = 2920;
    pub const INVERT: i32 = 2921;
    pub const HUE: i32 = 2922;
    pub const EDIT_COLOR: i32 = 2923;
    pub const SPLIT_SCREEN: i32 = 2924;
}

pub fn is_shader_trigger(id: i32) -> bool {
    matches!(id, 2904 | 2905 | 2907 | 2909..=2917 | 2919..=2924)
}

/// `DAT_100ad0890`, set by a static initializer (0x1003416d4): 0x3f99d63c.
pub const SHADER_SCALE: f32 = 1.201_970_6;
/// The game's design width (`CCDirector::setupScreenScale`: winSize / design size).
pub const DESIGN_WIDTH: f32 = 480.0;

/// A shader trigger's fields (`ShaderGameObject`, offsets in the comments).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderSpec {
    pub id: i32,
    /// 10 duration (+0x5ac), 30 easing (+0x5d8), 85 rate (+0x5dc).
    pub duration: f32,
    pub easing: i32,
    pub rate: f32,
    /// The target (`triggerShaderCommand`): key 138 → −1 (player 1), 200 → −2 (player 2),
    /// 201 → −3, else key 51 (+0x5b8, 0..9999).
    pub target: i32,
    /// 51 as given (the grayscale tint channel), 71 (+0x5bc: center / tint / ref channel).
    pub group: i32,
    pub center: i32,
    /// Float keys: 175 +0x730, 176 +0x734, 177 +0x73c, 179 +0x740, 180 +0x744, 181 +0x74c,
    /// 182 +0x750, 183 +0x760, 189 +0x748, 191 +0x738, 290 +0x754, 291 +0x758, 512 +0x764.
    pub f: HashMap<u32, f32>,
    /// Bool keys: 184 +0x75c, 185 +0x768, 186 +0x769, 187 +0x76a, 188 +0x76b, 190 +0x76c,
    /// 192 +0x76f, 194 +0x76d, 513 +0x778, 514 +0x779, 515 +0x76e, 531 +0x77a.
    pub b: HashMap<u32, bool>,
    /// 196 (+0x770) / 197 (+0x774): the setup trigger's min/max layer.
    pub min_layer: i32,
    pub max_layer: i32,
}

/// `atoi`: the leading integer (garbage → 0).
fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for c in digits.bytes() {
        if !c.is_ascii_digit() {
            break;
        }
        v = (v * 10 + (c - b'0') as i64).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// `atof` → f32, clamped to ±1e9 (the setup's clamp); NaN → 0.
fn atof(s: &str) -> f32 {
    let v: f64 = s.trim().parse().unwrap_or(0.0);
    let v = v as f32;
    if v.is_nan() { 0.0 } else { v.clamp(-1e9, 1e9) }
}

impl ShaderSpec {
    const FLOATS: [u32; 13] = [175, 176, 177, 179, 180, 181, 182, 183, 189, 191, 290, 291, 512];
    const BOOLS: [u32; 12] = [184, 185, 186, 187, 188, 190, 192, 194, 513, 514, 515, 531];

    pub fn parse(o: &RawObject) -> Option<ShaderSpec> {
        let id = o.id();
        if !is_shader_trigger(id) {
            return None;
        }
        let fl = |k| o.get(k).map_or(0.0, atof);
        let bo = |k| o.get(k).is_some_and(|v| atoi(v) != 0);
        let group = o.get(51).map_or(0, atoi).clamp(0, 9999);
        let target = if bo(138) {
            -1
        } else if bo(200) {
            -2
        } else if bo(201) {
            -3
        } else {
            group
        };
        let mut spec = ShaderSpec {
            id,
            duration: fl(10),
            easing: o.get(30).map_or(0, atoi),
            rate: fl(85),
            target,
            group,
            center: o.get(71).map_or(0, atoi).clamp(0, 9999),
            min_layer: o.get(196).map_or(0, atoi),
            max_layer: o.get(197).map_or(0, atoi),
            ..Default::default()
        };
        // ShaderGameObject::customSetup (0x1423e8) zeroes some, but customObjectSetup
        // overwrites every key with its value or 0 anyway.
        for k in Self::FLOATS {
            spec.f.insert(k, fl(k));
        }
        for k in Self::BOOLS {
            spec.b.insert(k, bo(k));
        }
        Some(spec)
    }

    pub fn fv(&self, key: u32) -> f32 {
        self.f.get(&key).copied().unwrap_or(0.0)
    }

    pub fn bv(&self, key: u32) -> bool {
        self.b.get(&key).copied().unwrap_or(false)
    }
}

/// The tweened values (`FUN_10033a04c`'s keys; `GJShaderState` offsets from ShaderLayer).
pub mod key {
    pub const CHROMATIC_X: usize = 1; // 0x2d4
    pub const CHROMATIC_Y: usize = 2; // 0x2d8
    pub const CG_RGB: usize = 3; // 0x2ec
    pub const CG_STRENGTH: usize = 4; // 0x2f0
    pub const CG_LINE_THICK: usize = 5; // 0x2f8
    pub const CG_LINE_STRENGTH: usize = 6; // 0x2fc
    pub const CG_SEGMENT: usize = 7; // 0x2f4
    pub const CG_SPEED: usize = 8; // 0x2e8
    pub const PIXELATE_X: usize = 9; // 0x304
    pub const PIXELATE_Y: usize = 10; // 0x308
    pub const LENS_SIZE: usize = 11; // 0x310
    pub const LENS_FADE: usize = 12; // 0x314
    pub const LENS_STRENGTH: usize = 13; // 0x318
    pub const LENS_OFF_X: usize = 14; // 0x334
    pub const LENS_OFF_Y: usize = 15; // 0x338
    pub const RADIAL_SIZE: usize = 16; // 0x33c
    pub const RADIAL_OFF_X: usize = 17; // 0x354
    pub const RADIAL_OFF_Y: usize = 18; // 0x358
    pub const MOTION_X: usize = 19; // 0x35c
    pub const MOTION_Y: usize = 20; // 0x360
    pub const BULGE: usize = 21; // 0x380
    pub const BULGE_OFF_X: usize = 22; // 0x398
    pub const BULGE_OFF_Y: usize = 23; // 0x39c
    pub const BULGE_RADIUS: usize = 24; // 0x38c
    pub const PINCH_X: usize = 25; // 0x3a0
    pub const PINCH_Y: usize = 26; // 0x3a4
    pub const PINCH_OFF_X: usize = 27; // 0x3bc
    pub const PINCH_OFF_Y: usize = 28; // 0x3c0
    pub const PINCH_RADIUS: usize = 29; // 0x3c8
    pub const PINCH_MOD_X: usize = 30; // 0x3cc
    pub const PINCH_MOD_Y: usize = 31; // 0x3d0
    pub const GRAYSCALE: usize = 32; // 0x3d4
    pub const SEPIA: usize = 33; // 0x3e4
    pub const INVERT: usize = 34; // 0x3e8
    pub const INVERT_R: usize = 35; // 0x3ec
    pub const INVERT_G: usize = 36;
    pub const INVERT_B: usize = 37;
    pub const HUE: usize = 38; // 0x3fc
    pub const CC_R: usize = 39; // 0x400 (multiplier)
    pub const CC_G: usize = 40;
    pub const CC_B: usize = 41;
    pub const CB_R: usize = 42; // 0x40c (added)
    pub const CB_G: usize = 43;
    pub const CB_B: usize = 44;
    pub const SPLIT_ROWS: usize = 45; // 0x418 (f64 in the game)
    pub const SPLIT_COLS: usize = 46; // 0x420
    pub const BLUR_INTENSITY: usize = 47; // 0x20c
    pub const BLUR_FADE: usize = 48; // 0x340
    pub const GLITCH_STRENGTH: usize = 49; // 0x2b8
    pub const SW_SPEED: usize = 50; // 0x224
    pub const SW_THICK: usize = 51; // 0x228
    pub const SW_STRENGTH: usize = 52; // 0x22c
    pub const SW_WIDTH: usize = 53; // 0x230
    pub const SW_FADE_IN: usize = 54; // 0x234
    pub const SW_FADE_OUT: usize = 55; // 0x238
    pub const SW_INNER: usize = 56; // 0x240
    pub const SW_OFF_X: usize = 57; // 0x254
    pub const SW_OFF_Y: usize = 58; // 0x258
    pub const SW_OUTER: usize = 59; // 0x244
    pub const SW_MAX_SIZE: usize = 60; // 0x250
    pub const SW_TIME_OFFSET: usize = 61; // 0x220
    pub const SL_SPEED: usize = 62; // 0x278
    pub const SL_THICK: usize = 63; // 0x294
    pub const SL_STRENGTH: usize = 64; // 0x284
    pub const SL_WIDTH: usize = 65; // 0x288
    pub const SL_FADE_IN: usize = 66; // 0x28c
    pub const SL_FADE_OUT: usize = 67; // 0x290
    pub const SL_OFFSET: usize = 68; // 0x280
    pub const SL_MAX_SIZE: usize = 69; // 0x2a0
    pub const SL_TIME_OFFSET: usize = 70; // 0x274
    pub const COUNT: usize = 71;
}

/// `GJValueTween` (step 0xcbdc0).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tween {
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
    easing: i32,
    rate: f32,
    done: bool,
}

impl Tween {
    /// Advance by `dt` and return the value.
    fn step(&mut self, dt: f32) -> f32 {
        if !self.done {
            self.elapsed += dt;
        }
        let mut t = if self.duration <= self.elapsed { 1.0 } else { self.elapsed / self.duration };
        if self.easing > 0 {
            t = crate::easing::eased(t, self.rate, self.easing);
        }
        self.done = self.duration <= self.elapsed;
        self.from + t * (self.to - self.from)
    }
}

/// A target's position (world units, the object layer's space) for the visit-time centers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Target {
    pub id: i32,
    pub enabled: bool,
    /// Follow every frame (else the position is taken once, `m_*CenterDirty`).
    pub follow: bool,
    pub dirty: bool,
    pub pos: Option<(f32, f32)>,
}

/// `GJShaderState` plus the ShaderLayer fields the effects use.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderState {
    pub v: [f32; key::COUNT],
    tweens: HashMap<usize, Tween>,
    /// m_time (+0x1e8, f64, advanced by the frame seconds) and m_prevTime.
    pub time: f64,
    prev_time: f64,
    // Shockwave (+0x218 start time f32, +0x21c time, +0x23c invert, +0x24e relative).
    sw_start: f32,
    sw_time: f32,
    sw_invert: bool,
    sw_relative: bool,
    pub sw_target: Target,
    // Shockline (+0x268 start time f64, +0x270 time, +0x27c..+0x27f axis/direction/dual/invert).
    sl_start: f64,
    sl_time: f32,
    sl_axis: bool,
    sl_direction: bool,
    sl_dual: bool,
    sl_invert: bool,
    sl_relative: bool,
    pub sl_target: Target,
    // Glitch (+0x2bc speed, +0x2c0 slice height, +0x2c4 slice x, +0x2c8/+0x2cc color x/y).
    glitch: [f32; 5],
    glitch_relative: bool,
    chromatic_relative: bool,
    // Chromatic glitch (+0x2e0 start, +0x2e4 time, +0x300 enable, +0x302/+0x303 relative).
    cg_start: f32,
    cg_time: f32,
    cg_enable: bool,
    cg_relative: bool,
    cg_relative_pos: bool,
    // Pixelate (+0x30c snap, +0x30e relative, +0x30f hard edges).
    pixelate_relative: bool,
    pub pixelate_hard: bool,
    // Lens circle (+0x31c target, +0x320 tint channel, +0x324 relative).
    pub lens_target: Target,
    pub lens_tint_channel: i32,
    lens_relative: bool,
    // Radial blur (+0x344 target, +0x348 id), blur ref channel +0x210, only empty +0x214.
    pub radial_target: Target,
    pub blur_ref_channel: i32,
    blur_only_empty: bool,
    // Motion blur (+0x364.. speeds, +0x36c follow ease, +0x374 ids, +0x37c dual, +0x37d rel).
    motion_speed: [f32; 2],
    motion_follow: [f32; 2],
    motion_target: [i32; 2],
    motion_dual: bool,
    motion_relative: bool,
    // Bulge (+0x384 relative, +0x388 target).
    bulge_relative: bool,
    pub bulge_target: Target,
    // Pinch (+0x3a8/9 target flags, +0x3ac/+0x3b0 ids, +0x3c4 relative).
    pub pinch_target: [Target; 2],
    pinch_relative: bool,
    // Grayscale (+0x3d8 use luminance, +0x3dc tint channel), invert clamp (+0x3f8).
    gray_lum: bool,
    pub gray_tint_channel: i32,
    invert_clamp: bool,
    /// m_minBlendingLayer / m_maxBlendingLayer (+0x42c / +0x430, 1..15), no player particles.
    pub min_layer: i32,
    pub max_layer: i32,
    pub no_particles: bool,
}

impl Default for ShaderState {
    fn default() -> Self {
        let mut s = ShaderState {
            v: [0.0; key::COUNT],
            tweens: HashMap::new(),
            time: 0.0,
            prev_time: -1.0,
            sw_start: 0.0,
            sw_time: 0.0,
            sw_invert: false,
            sw_relative: false,
            sw_target: Target::default(),
            sl_start: 0.0,
            sl_time: 0.0,
            sl_axis: false,
            sl_direction: false,
            sl_dual: false,
            sl_invert: false,
            sl_relative: false,
            sl_target: Target::default(),
            glitch: [0.0; 5],
            glitch_relative: false,
            chromatic_relative: false,
            cg_start: 0.0,
            cg_time: 0.0,
            cg_enable: false,
            cg_relative: false,
            cg_relative_pos: false,
            pixelate_relative: false,
            pixelate_hard: false,
            lens_target: Target::default(),
            lens_tint_channel: 0,
            lens_relative: false,
            radial_target: Target::default(),
            blur_ref_channel: 0,
            blur_only_empty: false,
            motion_speed: [0.0; 2],
            motion_follow: [0.0; 2],
            motion_target: [0; 2],
            motion_dual: false,
            motion_relative: false,
            bulge_relative: false,
            bulge_target: Target::default(),
            pinch_target: [Target::default(); 2],
            pinch_relative: false,
            gray_lum: false,
            gray_tint_channel: 0,
            invert_clamp: false,
            min_layer: 1,
            max_layer: 15,
            no_particles: false,
        };
        s.reset_values();
        s
    }
}

/// What the uniforms need from the game at draw time.
pub struct Context<'a> {
    /// The view in world units: left, bottom, width, height (already divided by the zoom).
    pub view: (f32, f32, f32, f32),
    /// The view's size in points at zoom 1 (the window: `winSize`).
    pub win: (f32, f32),
    /// The object layer's scale (the camera zoom).
    pub zoom: f32,
    /// A target's world position: −1 player 1, −2 player 2, else a group's main object.
    pub position: &'a dyn Fn(i32) -> Option<(f32, f32)>,
    /// A color channel's active color (0..255) and whether it blends.
    pub channel: &'a dyn Fn(i32) -> ([u8; 3], bool),
    /// The background sprite's color (blur reference when no channel is set).
    pub background: [u8; 3],
}

impl Context<'_> {
    /// World position → screen UV (y up).
    fn uv(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (l, b, w, h) = self.view;
        ((x - l) / w, (y - b) / h)
    }

    /// `m_shockWaveTimeMult` (+0x47c): SHADER_SCALE / the screen scale factor.
    fn mult(&self) -> f32 {
        SHADER_SCALE / (self.win.0 / DESIGN_WIDTH)
    }

    /// h / w (`_screenAspect`).
    fn aspect(&self) -> f32 {
        self.win.1 / self.win.0
    }
}

/// The post shader's uniform block (`postfx.wgsl` `Fx`), 34 vec4s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Uniforms(pub [[f32; 4]; 34]);

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms([[0.0; 4]; 34])
    }
}

/// Uniform slots (vec4 index) — keep in sync with `postfx.wgsl`.
pub mod slot {
    pub const COMMON: usize = 0; // tex scale x, y, aspect (h/w), aspect inv
    pub const SW0: usize = 1; // time, t1, t2, t3
    pub const SW1: usize = 2; // t4, strength, waves, invert
    pub const SW2: usize = 3; // center x, y, min size, max size
    pub const SW3: usize = 4; // max dist val
    pub const SL0: usize = 5; // time, t1, t2, t3
    pub const SL1: usize = 6; // t4, axis, direction, dual
    pub const SL2: usize = 7; // waves, strength x, y, center
    pub const SL3: usize = 8; // max dist val
    pub const GLITCH0: usize = 9; // bot, top, x offset, rnd
    pub const GLITCH1: usize = 10; // col offset x, y, chromatic x, y
    pub const LENS0: usize = 11; // origin x, y, start, end
    pub const LENS1: usize = 12; // strength, additive
    pub const LENS2: usize = 13; // tint rgb
    pub const BULGE0: usize = 14; // origin x, y, value, value2
    pub const BULGE1: usize = 15; // radius
    pub const PINCH0: usize = 16; // center x, y, value x, y
    pub const PINCH1: usize = 17; // calc1 x, y, radius
    pub const BLUR0: usize = 18; // use ref, intensity, fade, only empty
    pub const BLUR1: usize = 19; // ref color rgb
    pub const RADIAL: usize = 20; // center x, y, value
    pub const MOTION: usize = 21; // value x, y, mult, dual
    pub const CG0: usize = 22; // time, rgb offset, y offset, strength
    pub const CG1: usize = 23; // height, line strength, line thick
    pub const COLOR0: usize = 24; // sepia, grayscale, use lum
    pub const GRAY_TINT: usize = 25; // rgb
    pub const INVERT: usize = 26; // rgba
    pub const HUE: usize = 27; // cos, sin
    pub const CC_C: usize = 28; // rgb
    pub const CC_B: usize = 29; // rgb
    pub const SPLIT0: usize = 30; // rowmod, colmod, rowmod calc, colmod calc
    pub const SPLIT1: usize = 31; // x start, x range, x range mult, y start
    pub const SPLIT2: usize = 32; // y range, y range mult
    pub const PIXELATE: usize = 33; // pixel size x, y (UV), hard edges
}

const TAU: f32 = 6.283_185_5;

impl ShaderState {
    pub fn new() -> ShaderState {
        ShaderState::default()
    }

    /// `GJShaderState::reset` (0x33a4d4) defaults: invert RGB and the edit color multipliers 1,
    /// lens size 1, chromatic glitch segment height 1, everything else 0.
    fn reset_values(&mut self) {
        self.v = [0.0; key::COUNT];
        for k in [key::INVERT_R, key::INVERT_G, key::INVERT_B, key::CC_R, key::CC_G, key::CC_B, key::LENS_SIZE, key::CG_SEGMENT] {
            self.v[k] = 1.0;
        }
    }

    /// `ShaderLayer::resetAllShaders` (0x341348): every effect off, layers BG..Max.
    pub fn reset_all(&mut self) {
        let time = self.time;
        *self = ShaderState::default();
        self.time = time;
    }

    /// `tweenValueAuto` (0x33d730).
    fn tween(&mut self, k: usize, to: f32, duration: f32, rate: f32, easing: i32) {
        if to == -909_190.0 {
            return;
        }
        self.tweens.remove(&k);
        if duration <= 0.0 {
            self.v[k] = to;
        } else {
            self.tweens.insert(k, Tween { from: self.v[k], to, duration, elapsed: 0.0, easing, rate, done: false });
        }
    }

    /// Whether any value is still tweening (the game also counts running actions as active).
    pub fn tweening(&self) -> bool {
        !self.tweens.is_empty()
    }

    /// `GJBaseGameLayer::triggerShaderCommand` (0xf2cfc) and the `ShaderLayer::trigger*` it calls.
    pub fn trigger(&mut self, s: &ShaderSpec) {
        let (d, e, r) = (s.duration, s.easing, s.rate);
        let f = |k| s.fv(k);
        let b = |k| s.bv(k);
        match s.id {
            id::SETUP => {
                // updateZLayer (0x33b128): min < 2 → BG; max ≤ 0 → Max.
                self.min_layer = s.min_layer.max(1);
                self.max_layer = if s.max_layer > 0 { s.max_layer } else { 15 };
                self.no_particles = b(188);
                if b(192) {
                    self.reset_all();
                }
            }
            id::SHOCKWAVE => {
                // triggerShockWave (0x33d4fc): speed 175, thickness 180, strength 176, wave width
                // 179, fade in 181, fade out 182, time offset 177, invert 184, inner 183, screen
                // offset 290/291, target 188, follow 190, relative 514, outer 191, max size 512,
                // animate 513.
                let animate = b(513);
                let mut dur = d;
                if !animate {
                    self.sw_start = self.time as f32;
                    self.sw_time = 0.0;
                    self.sw_invert = b(184);
                    self.sw_target = Target { id: s.target, enabled: b(188), follow: b(190), dirty: true, pos: None };
                    self.sw_relative = b(514);
                    dur = 0.0;
                } else {
                    self.sw_time += self.v[key::SW_SPEED] * (self.time - self.sw_start as f64) as f32;
                    self.sw_start = self.time as f32;
                }
                for (k, v) in [
                    (key::SW_SPEED, f(175)),
                    (key::SW_THICK, f(180)),
                    (key::SW_STRENGTH, f(176)),
                    (key::SW_WIDTH, f(179)),
                    (key::SW_FADE_IN, f(181)),
                    (key::SW_FADE_OUT, f(182)),
                    (key::SW_INNER, f(183)),
                    (key::SW_OFF_X, f(290)),
                    (key::SW_OFF_Y, f(291)),
                    (key::SW_OUTER, f(191)),
                    (key::SW_MAX_SIZE, f(512)),
                    (key::SW_TIME_OFFSET, f(177)),
                ] {
                    self.tween(k, v, dur, r, e);
                }
            }
            id::SHOCKLINE => {
                // triggerShockLine (0x33e1d0): speed 175, thickness 180, rotate 186, flip 185,
                // dual 187, invert 184, strength 176, width 179, fade in 181, fade out 182,
                // time offset 177, screen offset 290, target 188, follow 190, relative 514, max
                // size 512, animate 513.
                let animate = b(513);
                let mut dur = d;
                if !animate {
                    self.sl_start = self.time;
                    self.sl_time = 0.0;
                    self.sl_target = Target { id: s.target, enabled: b(188), follow: b(190), dirty: true, pos: None };
                    self.sl_relative = b(514);
                    self.sl_axis = b(186);
                    self.sl_direction = b(185);
                    self.sl_dual = b(187);
                    self.sl_invert = b(184);
                    dur = 0.0;
                } else {
                    self.sl_time += self.v[key::SL_SPEED] * (self.time - self.sl_start) as f32;
                    self.sl_start = self.time;
                }
                for (k, v) in [
                    (key::SL_SPEED, f(175)),
                    (key::SL_THICK, f(180)),
                    (key::SL_STRENGTH, f(176)),
                    (key::SL_WIDTH, f(179)),
                    (key::SL_FADE_IN, f(181)),
                    (key::SL_FADE_OUT, f(182)),
                    (key::SL_OFFSET, f(290)),
                    (key::SL_MAX_SIZE, f(512)),
                    (key::SL_TIME_OFFSET, f(177)),
                ] {
                    self.tween(k, v, dur, r, e);
                }
            }
            id::GLITCH => {
                // triggerGlitch (0x33e760): strength 176 (tweened, linear), speed 175, slice
                // height 179, max slice x 191, max color x/y 181/182, relative 514.
                self.tween(key::GLITCH_STRENGTH, f(176), d, 0.0, 0);
                self.glitch = [f(175), f(179), f(191), f(181), f(182)];
                self.glitch_relative = b(514);
            }
            id::CHROMATIC => {
                // triggerChromaticX/Y (0x33ec4c/0x33ec88): x 180 if 188, y 189 if 190.
                if b(188) {
                    self.tween(key::CHROMATIC_X, f(180), d, r, e);
                    self.chromatic_relative = b(514);
                }
                if b(190) {
                    self.tween(key::CHROMATIC_Y, f(189), d, r, e);
                    self.chromatic_relative = b(514);
                }
            }
            id::CHROMATIC_GLITCH => {
                // triggerChromaticGlitch (0x33edd0): enable = !192, speed 175, strength 176, rgb
                // offset 180, line thickness 179, line strength 191, segment height 189 (stored
                // as 1/max(0.1, h)), relative 514, relative position 194.
                let now = self.time;
                let (t, start) = if self.cg_enable { (self.cg_time, self.cg_start) } else { (now as f32, now as f32) };
                self.cg_enable = !b(192);
                self.cg_time = t + self.v[key::CG_SPEED] * (now - start as f64) as f32;
                self.cg_start = now as f32;
                self.tween(key::CG_SPEED, f(175), d, r, e);
                self.tween(key::CG_SEGMENT, 1.0 / f(189).max(0.1), d, r, e);
                self.tween(key::CG_STRENGTH, f(176), d, r, e);
                self.tween(key::CG_LINE_THICK, f(179), d, r, e);
                self.tween(key::CG_LINE_STRENGTH, f(191), d, r, e);
                self.tween(key::CG_RGB, f(180), d, r, e);
                self.cg_relative = b(514);
                self.cg_relative_pos = b(194);
            }
            id::PIXELATE => {
                // triggerPixelateX/Y (0x33f248/0x33f2b4): x 180 if 188, y 189 if 190; hard
                // edges 515, relative 514. Values start at 1 at least.
                for (on, k, v) in [(b(188), key::PIXELATE_X, f(180)), (b(190), key::PIXELATE_Y, f(189))] {
                    if on {
                        if self.v[k] < 1.0 {
                            self.v[k] = 1.0;
                        }
                        self.tween(k, v, d, r, e);
                        self.pixelate_relative = b(514);
                        self.pixelate_hard = b(515);
                    }
                }
            }
            id::LENS_CIRCLE => {
                // triggerLensCircle (0x33f824): size 179, fade 181, strength 176 (0..1), target,
                // tint channel 71, screen offset 290/291, relative 514.
                self.tween(key::LENS_STRENGTH, f(176).clamp(0.0, 1.0), d, r, e);
                self.tween(key::LENS_FADE, f(181), d, r, e);
                self.tween(key::LENS_SIZE, f(179), d, r, e);
                self.tween(key::LENS_OFF_X, f(290), d, r, e);
                self.tween(key::LENS_OFF_Y, f(291), d, r, e);
                self.lens_target = Target { id: s.target, enabled: s.target != 0, follow: true, dirty: true, pos: None };
                self.lens_tint_channel = s.center;
                self.lens_relative = b(514);
            }
            id::RADIAL_BLUR => {
                // triggerRadialBlur (0x33fac4): size 179, intensity 176, fade 181, ref channel
                // 71, screen offset 290/291, target 188, empty only 515.
                self.blur_ref_channel = s.center;
                self.tween(key::BLUR_FADE, f(181), d, r, e);
                self.tween(key::BLUR_INTENSITY, f(176), d, r, e);
                self.tween(key::RADIAL_SIZE, f(179), d, r, e);
                self.tween(key::RADIAL_OFF_X, f(290), d, r, e);
                self.tween(key::RADIAL_OFF_Y, f(291), d, r, e);
                self.radial_target = Target { id: s.target, enabled: b(188), follow: true, dirty: true, pos: None };
                self.blur_only_empty = b(515);
            }
            id::MOTION_BLUR => {
                // triggerMotionBlurX/Y (0x33fcd0/0x33fda4): x 180 if 188, y 189 if 190;
                // intensity 176, fade 181, follow ease 191, dual 194, ref channel 71, relative
                // 514, empty only 515.
                for (axis, on, k, v) in [(0, b(188), key::MOTION_X, f(180)), (1, b(190), key::MOTION_Y, f(189))] {
                    if on {
                        self.blur_ref_channel = s.center;
                        self.tween(key::BLUR_FADE, f(181), d, 0.0, 0);
                        self.tween(key::BLUR_INTENSITY, f(176), d, 0.0, 0);
                        self.tween(k, v, d, r, e);
                        self.motion_target[axis] = s.target;
                        self.motion_dual = b(194);
                        self.motion_follow[axis] = f(191);
                        self.motion_relative = b(514);
                        self.blur_only_empty = b(515);
                    }
                }
            }
            id::BULGE => {
                // triggerBulge (0x34014c): bulge 176, screen offset 290/291, radius 180, target
                // only with 188, relative 514.
                self.tween(key::BULGE, f(176), d, r, e);
                self.tween(key::BULGE_OFF_X, f(290), d, r, e);
                self.tween(key::BULGE_OFF_Y, f(291), d, r, e);
                self.tween(key::BULGE_RADIUS, f(180), d, r, e);
                self.bulge_relative = b(514);
                let t = if b(188) { s.target } else { 0 };
                self.bulge_target = Target { id: t, enabled: t != 0, follow: true, dirty: true, pos: None };
            }
            id::PINCH => {
                // triggerPinchX/Y (0x340288/0x340368): x 180 / offset 290 if 190, y 189 / 291 if
                // 194; radius 512, modifier 179, target 188, relative 514.
                for (axis, on, kv, ko, km, v, o) in [
                    (0, b(190), key::PINCH_X, key::PINCH_OFF_X, key::PINCH_MOD_X, f(180), f(290)),
                    (1, b(194), key::PINCH_Y, key::PINCH_OFF_Y, key::PINCH_MOD_Y, f(189), f(291)),
                ] {
                    if on {
                        self.tween(kv, v, d, r, e);
                        self.tween(ko, o, d, r, e);
                        self.tween(key::PINCH_RADIUS, f(512), d, r, e);
                        self.tween(km, f(179), d, r, e);
                        self.pinch_target[axis] = Target { id: s.target, enabled: b(188), follow: true, dirty: true, pos: None };
                        self.pinch_relative = b(514);
                    }
                }
            }
            id::GRAYSCALE => {
                // triggerGrayscale (0x3404b0): value 176, use luminance 188, tint channel 51 if
                // 190.
                if b(190) && s.group >= 0 {
                    self.gray_tint_channel = s.group;
                }
                self.gray_lum = b(188);
                self.tween(key::GRAYSCALE, f(176), d, r, e);
            }
            id::SEPIA => self.tween(key::SEPIA, f(176), d, r, e),
            id::INVERT => {
                // triggerInvertColor (0x340618): value 176; edit RGB 188 (r 179, g 180, b 189),
                // tween RGB 190, clamp 194.
                if b(188) {
                    self.invert_clamp = b(194);
                    if b(190) {
                        self.tween(key::INVERT_R, f(179), d, r, e);
                        self.tween(key::INVERT_G, f(180), d, r, e);
                        self.tween(key::INVERT_B, f(189), d, r, e);
                    } else {
                        self.v[key::INVERT_R] = f(179);
                        self.v[key::INVERT_G] = f(180);
                        self.v[key::INVERT_B] = f(189);
                    }
                }
                self.tween(key::INVERT, f(176), d, r, e);
            }
            id::HUE => self.tween(key::HUE, f(176), d, r, e),
            id::EDIT_COLOR => {
                // triggerColorChange (0x340864): multiply 176/191/175, add 179/180/189.
                for (k, v) in [
                    (key::CB_R, f(179)),
                    (key::CB_G, f(180)),
                    (key::CB_B, f(189)),
                    (key::CC_R, f(176)),
                    (key::CC_G, f(191)),
                    (key::CC_B, f(175)),
                ] {
                    self.tween(k, v, d, r, e);
                }
            }
            id::SPLIT_SCREEN => {
                // triggerSplitScreenCols/Rows: cols 180 if 188, rows 189 if 190.
                if b(188) {
                    self.tween(key::SPLIT_COLS, f(180), d, r, e);
                }
                if b(190) {
                    self.tween(key::SPLIT_ROWS, f(189), d, r, e);
                }
            }
            _ => {}
        }
    }

    /// The frame's part of `updateShaderLayer`: target positions (centers that don't follow
    /// are taken once), then `ShaderLayer::update` (time += dt, tweens).
    pub fn frame(&mut self, dt: f32, position: &dyn Fn(i32) -> Option<(f32, f32)>) {
        for t in [&mut self.sw_target, &mut self.sl_target] {
            if t.id != 0 && t.dirty {
                t.pos = position(t.id);
                if !t.follow {
                    t.dirty = false;
                }
            }
        }
        let [p0, p1] = &mut self.pinch_target;
        for t in [&mut self.lens_target, &mut self.radial_target, &mut self.bulge_target, p0, p1] {
            if t.enabled && t.id != 0 {
                t.pos = position(t.id);
            }
        }
        self.time += dt as f64;
        let mut done = Vec::new();
        for (&k, t) in self.tweens.iter_mut() {
            self.v[k] = t.step(dt);
            if t.done {
                done.push(k);
            }
        }
        for k in done {
            self.tweens.remove(&k);
        }
        // updateMotionBlurSpeedX/Y (0x3400e4): target speeds aren't tracked (no targets yet).
        let _ = (self.motion_speed, self.motion_follow, self.motion_target);
    }

    /// `performCalculations` + `visit`: the uniforms for this frame, or `None` when no effect is
    /// on (`m_usesShaders` +0x436 clear: the game then draws the layers normally).
    pub fn uniforms(&mut self, ctx: &Context) -> Option<Uniforms> {
        let mut u = Uniforms::default();
        let mut used = false;
        let v = self.v;
        let mult = ctx.mult();
        let aspect = ctx.aspect();
        let zoom = ctx.zoom.abs();
        let rel = |on: bool| if on { zoom } else { 1.0 };
        u.0[slot::COMMON] = [1.0, 1.0, aspect, 1.0 / aspect];
        // The screen-offset centers (`FUN_10033c488`): 0.5 + offset/2, or the target's UV.
        let center = |t: &Target, ox: f32, oy: f32| -> (f32, f32) {
            match t.pos {
                Some(p) if t.enabled => ctx.uv(p),
                _ => (0.5 + ox * 0.5, 0.5 + oy * 0.5),
            }
        };
        let advance = self.time != self.prev_time;
        self.prev_time = self.time;

        // Shockwave (`FUN_10033de48`).
        if self.sw_start > 0.0 {
            let t = (self.sw_time as f64 + v[key::SW_SPEED] as f64 * (self.time - self.sw_start as f64)) as f32;
            if advance {
                self.sw_time = t;
                self.sw_start = self.time as f32;
            }
            if v[key::SW_STRENGTH] > 0.0 {
                used = true;
                let thick = if v[key::SW_THICK] > 0.001 { v[key::SW_THICK] * 0.2 } else { 0.000_2 };
                let fade_in = v[key::SW_FADE_IN] * thick;
                let fade_out = v[key::SW_FADE_OUT] * thick;
                let t2 = v[key::SW_TIME_OFFSET].mul_add(0.1, t);
                let k = if self.sw_invert { 1.0 } else { t2 };
                let r = t2 * k - fade_in;
                let a = r.min(r - thick);
                let b = a.min(a - fade_out);
                let width = v[key::SW_WIDTH].max(0.001);
                let m = rel(self.sw_relative) * mult;
                let times = [(fade_in + r + 0.001) * m, r * m, (a + 0.001) * m, b * m];
                let max_dist = if v[key::SW_MAX_SIZE] != 0.0 { 1.0 / (v[key::SW_MAX_SIZE] * 0.5 * m) } else { 0.0 };
                let min_size = m * v[key::SW_INNER] * 0.5;
                let max_size = m * v[key::SW_OUTER];
                let mut time = (t2 * (22.0 / width)) % TAU;
                let c = center(&self.sw_target, v[key::SW_OFF_X], v[key::SW_OFF_Y]);
                // visit: off when the ring can't reach the screen.
                let mut d = ((c.0 - 0.5).powi(2) + (c.1 - 0.5).powi(2)).sqrt();
                if self.sw_invert {
                    d = max_size - d.max(min_size);
                }
                let lo = times.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = times.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                if (lo < d + std::f32::consts::SQRT_2) != (d - std::f32::consts::SQRT_2 <= hi) {
                    time = 0.0;
                }
                u.0[slot::SW0] = [time, times[0], times[1], times[2]];
                u.0[slot::SW1] = [times[3], m * v[key::SW_STRENGTH] * 0.1, (22.0 / width) / m, if self.sw_invert { 1.0 } else { 0.0 }];
                u.0[slot::SW2] = [c.0, c.1, min_size, max_size];
                u.0[slot::SW3] = [max_dist, 0.0, 0.0, 0.0];
            }
        }

        // Shockline (`FUN_10033e3c0`).
        if self.sl_start != 0.0 {
            let t = (self.sl_time as f64 + v[key::SL_SPEED] as f64 * (self.time - self.sl_start)) as f32;
            if advance {
                self.sl_time = t;
                self.sl_start = self.time;
            }
            if v[key::SL_STRENGTH] > 0.0 {
                used = true;
                let thick = if v[key::SL_THICK] > 0.001 { v[key::SL_THICK] * 0.2 } else { 0.000_2 };
                let fade_in = v[key::SL_FADE_IN] * thick;
                let fade_out = v[key::SL_FADE_OUT] * thick;
                let mut t2 = v[key::SL_TIME_OFFSET].mul_add(0.1, t);
                if self.sl_invert {
                    t2 = if self.sl_dual { 0.5 } else { 1.0 } * (fade_out + thick + fade_in) - t2;
                }
                let r = t2 - fade_in;
                let a = r.min(r - thick);
                let b = a.min(a - fade_out);
                let width = v[key::SL_WIDTH].max(0.001);
                let m = rel(self.sl_relative) * mult;
                let times = [(fade_in + r + 0.001) * m, r * m, (a + 0.001) * m, b * m];
                let max_dist = if v[key::SL_MAX_SIZE] != 0.0 { 1.0 / (v[key::SL_MAX_SIZE] * 0.5 * m) } else { 0.0 };
                let mut time = (t2 * (22.0 / width)) % TAU;
                let off = v[key::SL_OFFSET];
                let c = center(&self.sl_target, off, off);
                let mut d = if self.sl_axis { 0.5 - c.1 } else { 0.5 - c.0 };
                if self.sl_dual {
                    d = d.abs();
                }
                if self.sl_direction {
                    d = 1.0 - d;
                }
                let lo = times.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = times.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                if (lo < d + 1.0) != (d - 1.0 <= hi) {
                    time = 0.0;
                }
                let s = m * v[key::SL_STRENGTH] * 0.01;
                let flag = |b: bool| if b { 1.0 } else { 0.0 };
                u.0[slot::SL0] = [time, times[0], times[1], times[2]];
                u.0[slot::SL1] = [times[3], flag(self.sl_axis), flag(self.sl_direction), flag(self.sl_dual)];
                u.0[slot::SL2] = [(22.0 / width) / m, s, s, if self.sl_axis { c.1 } else { c.0 }];
                u.0[slot::SL3] = [max_dist, 0.0, 0.0, 0.0];
            }
        }

        // Glitch (`preGlitchShader` 0x33e7f0).
        if v[key::GLITCH_STRENGTH] > 0.0 {
            used = true;
            let m = rel(self.glitch_relative) * mult;
            let strength = v[key::GLITCH_STRENGTH];
            let [speed, slice, slice_x, col_x, col_y] = self.glitch;
            let seed = (self.time * speed as f64 * 20.0) as i64 as f32;
            let rnd = |k: f32| {
                let s = (seed * 12.9898 + k * 4.1414).sin() * 43758.547;
                s - s.trunc()
            };
            let bot = m * rnd(2345.0);
            let mut top = bot + m * strength * slice * rnd(9035.0);
            top += top.trunc();
            let x_range = m * strength * slice_x * 0.02;
            let cx = m * col_x * strength * 0.02;
            let cy = m * strength * col_y * 0.02;
            let col = (rnd(9545.0) * (cx + cx) - cx, rnd(7205.0) * (cy + cy) - cy);
            let aspect_wh = ctx.win.0 / ctx.win.1;
            u.0[slot::GLITCH0] = [bot * aspect_wh, top * aspect_wh, rnd(9625.0) * (x_range + x_range) - x_range, rnd(9545.0)];
            u.0[slot::GLITCH1][0] = col.0;
            u.0[slot::GLITCH1][1] = aspect_wh * col.1;
        }

        // Chromatic (`preChromaticShader` 0x33ecc4): offsets in points / SHADER_SCALE.
        if v[key::CHROMATIC_X] != 0.0 || v[key::CHROMATIC_Y] != 0.0 {
            used = true;
            let k = rel(self.chromatic_relative) / SHADER_SCALE;
            u.0[slot::GLITCH1][2] = k * v[key::CHROMATIC_X] / ctx.win.0;
            u.0[slot::GLITCH1][3] = k * v[key::CHROMATIC_Y] / ctx.win.1;
        }

        // Chromatic glitch (`preChromaticGlitchShader` 0x33ef50).
        if self.cg_enable && (v[key::CG_RGB] != 0.0 || v[key::CG_STRENGTH] != 0.0 || v[key::CG_LINE_STRENGTH] != 0.0) {
            used = true;
            let m = rel(self.cg_relative) * mult;
            let t = self.cg_time + v[key::CG_SPEED] * (self.time - self.cg_start as f64) as f32;
            let t = (t as i32 % 1000) as f32 + (t - t.trunc());
            if advance {
                self.cg_time = t;
                self.cg_start = self.time as f32;
            }
            let lt = m.mul_add(v[key::CG_LINE_THICK] * -0.2, 1.0).max(0.001);
            let line_thick = if v[key::CG_LINE_STRENGTH] != 0.0 { lt } else { 0.0 };
            let rgb = if v[key::CG_RGB] * 0.1 != 0.0 { v[key::CG_RGB] * 0.1 * 0.1 } else { 0.000_1 };
            let y_offset = if self.cg_relative_pos { -(ctx.uv((0.0, 0.0)).1) } else { 0.0 };
            u.0[slot::CG0] = [t, m * rgb, y_offset, m * v[key::CG_STRENGTH] * 0.02];
            u.0[slot::CG1] = [v[key::CG_SEGMENT] / (m * (ctx.win.0 / ctx.win.1)), v[key::CG_LINE_STRENGTH] * 0.05, line_thick, 0.0];
        }

        // Lens circle (`preLensCircleShader` 0x33f934 + visit).
        if v[key::LENS_STRENGTH] > 0.0 {
            used = true;
            let size = rel(self.lens_relative) * v[key::LENS_SIZE] * mult;
            let fade = v[key::LENS_FADE].max(0.0);
            let c = center(&self.lens_target, v[key::LENS_OFF_X], v[key::LENS_OFF_Y]);
            let (tint, additive) = if self.lens_tint_channel > 0 { (ctx.channel)(self.lens_tint_channel) } else { ([0; 3], false) };
            u.0[slot::LENS0] = [c.0, c.1 * aspect, (size - fade).max(0.0), size.max(0.0)];
            u.0[slot::LENS1] = [v[key::LENS_STRENGTH], if additive { 1.0 } else { 0.0 }, 0.0, 0.0];
            u.0[slot::LENS2] = [tint[0] as f32 / 255.0, tint[1] as f32 / 255.0, tint[2] as f32 / 255.0, 0.0];
        }

        // Blur common (`preCommonShader` 0x33c20c): the reference color is used when the
        // layer doesn't start at the background.
        let refc = if self.blur_ref_channel > 0 { (ctx.channel)(self.blur_ref_channel).0 } else { ctx.background };
        u.0[slot::BLUR1] = [refc[0] as f32 / 255.0, refc[1] as f32 / 255.0, refc[2] as f32 / 255.0, 0.0];
        let use_ref = self.min_layer > 1;
        u.0[slot::BLUR0] = [
            if use_ref { 1.0 } else { 0.0 },
            v[key::BLUR_INTENSITY] + 1.0,
            0.0,
            if use_ref && self.blur_only_empty { 1.0 } else { 0.0 },
        ];

        // Radial blur (`FUN_10033fbc8` + visit).
        if v[key::RADIAL_SIZE] != 0.0 {
            used = true;
            let c = center(&self.radial_target, v[key::RADIAL_OFF_X], v[key::RADIAL_OFF_Y]);
            u.0[slot::RADIAL] = [c.0, c.1, v[key::RADIAL_SIZE] * (1.0 / 9.0) * 0.2, 0.0];
            u.0[slot::BLUR0][2] = (v[key::BLUR_FADE] * 0.2).min(0.2);
        }

        // Motion blur (`preMotionBlurShader` 0x33fe78), without target speeds.
        if v[key::MOTION_X] != 0.0 || v[key::MOTION_Y] != 0.0 {
            used = true;
            let m = rel(self.motion_relative) * mult;
            let sx = if self.motion_target[0] != 0 { 0.001 } else { 1.0 };
            let sy = if self.motion_target[1] != 0 { 0.001 } else { 1.0 };
            u.0[slot::MOTION][0] = m * sx * v[key::MOTION_X] * 0.1;
            u.0[slot::MOTION][1] = (ctx.win.0 / ctx.win.1) * m * sy * v[key::MOTION_Y] * 0.1;
            if v[key::RADIAL_SIZE] != 0.0 {
                u.0[slot::MOTION][2] = 1.0;
            } else {
                let lim = if self.motion_dual { 0.2 } else { 0.1 };
                let fade = (v[key::BLUR_FADE] * lim).min(lim).max(-lim);
                u.0[slot::BLUR0][2] = fade;
                u.0[slot::MOTION][2] = 1.0 / fade.mul_add(if self.motion_dual { -20.0 } else { -36.0 }, 9.0);
                u.0[slot::MOTION][3] = if self.motion_dual { 1.0 } else { 0.0 };
            }
        }

        // Bulge (visit).
        if v[key::BULGE] > 0.0 {
            used = true;
            let c = center(&self.bulge_target, v[key::BULGE_OFF_X], v[key::BULGE_OFF_Y]);
            let m = rel(self.bulge_relative) * mult;
            let r = m * 0.5 * SHADER_SCALE * SHADER_SCALE;
            let len = (r * r + r * r).sqrt();
            let value = ((TAU / (len + len)) * 0.45 * v[key::BULGE]).max(0.001);
            u.0[slot::BULGE0] = [c.0, c.1, value, len / (len * value).tan()];
            u.0[slot::BULGE1] = [m * v[key::BULGE_RADIUS], 0.0, 0.0, 0.0];
        }

        // Pinch (visit).
        {
            let (px, py) = (v[key::PINCH_X], v[key::PINCH_Y]);
            let x = if px != 0.0 || py == 0.0 { px } else { 0.001 };
            let y = if py != 0.0 || x == 0.0 { py * -0.1 } else { -0.000_1 };
            u.0[slot::PINCH0][2] = x * -0.1;
            u.0[slot::PINCH0][3] = y;
            if px != 0.0 || py != 0.0 {
                used = true;
                let t = if self.pinch_target[0].enabled { self.pinch_target[0] } else { self.pinch_target[1] };
                let c = center(&t, v[key::PINCH_OFF_X], v[key::PINCH_OFF_Y]);
                let m = rel(self.pinch_relative) * mult;
                let (mx, my) = (v[key::PINCH_MOD_X].max(0.001), v[key::PINCH_MOD_Y].max(0.001));
                u.0[slot::PINCH0][0] = c.0;
                u.0[slot::PINCH0][1] = c.1;
                let (vx, vy) = (u.0[slot::PINCH0][2], u.0[slot::PINCH0][3]);
                u.0[slot::PINCH1] = [mx / (mx * vx * -20.0).atan(), my / (my * vy * -20.0).atan(), m * v[key::PINCH_RADIUS], 0.0];
            }
        }

        // Grayscale (`FUN_1003404d4`), sepia, invert, hue, edit color (`performCalculations`).
        let gray = v[key::GRAYSCALE];
        let sepia = v[key::SEPIA];
        u.0[slot::COLOR0] = [sepia, gray, if self.gray_lum { 1.0 } else { 0.0 }, 0.0];
        if gray > 0.0 || sepia > 0.0 {
            used = true;
        }
        let tint = if self.gray_tint_channel > 0 { (ctx.channel)(self.gray_tint_channel).0 } else { [255; 3] };
        u.0[slot::GRAY_TINT] = [tint[0] as f32 / 255.0, tint[1] as f32 / 255.0, tint[2] as f32 / 255.0, 0.0];
        let inv = v[key::INVERT];
        if inv > 0.0 {
            used = true;
        }
        let (mut ir, mut ig, mut ib) = (inv * v[key::INVERT_R], inv * v[key::INVERT_G], inv * v[key::INVERT_B]);
        if self.invert_clamp {
            ir = ir.min(1.0);
            ig = ig.min(1.0);
            ib = ib.min(1.0);
        }
        u.0[slot::INVERT] = [ir, ig, ib, inv];
        if v[key::HUE] != 0.0 {
            used = true;
        }
        let (sin, cos) = (v[key::HUE] * 0.017_453_292).sin_cos();
        u.0[slot::HUE] = [cos, sin, 0.0, 0.0];
        let cc = [v[key::CC_R], v[key::CC_G], v[key::CC_B]];
        let cb = [v[key::CB_R], v[key::CB_G], v[key::CB_B]];
        if cc != [1.0; 3] || cb != [0.0; 3] {
            used = true;
            u.0[slot::CC_C] = [cc[0].max(0.001), cc[1], cc[2], 0.0];
            u.0[slot::CC_B] = [cb[0], cb[1], cb[2], 0.0];
        }

        // Split screen (`FUN_100340af0`).
        let rows = v[key::SPLIT_ROWS] + 1.0;
        let cols = v[key::SPLIT_COLS] + 1.0;
        u.0[slot::SPLIT0] = [rows, cols, 1.0 - rows, 1.0 - cols];
        u.0[slot::SPLIT1] = [0.0, 1.0, 1.0, 0.0];
        u.0[slot::SPLIT2] = [1.0, 1.0, 0.0, 0.0];
        if v[key::SPLIT_ROWS] != 0.0 || v[key::SPLIT_COLS] != 0.0 {
            used = true;
        }

        // Pixelate (`prePixelateShader`): the game renders the layer into a smaller texture;
        // Cadence snaps the UVs to pixels of that size instead.
        let (pxx, pxy) = (v[key::PIXELATE_X], v[key::PIXELATE_Y]);
        if pxx > 1.0 || pxy > 1.0 {
            used = true;
            let k = rel(self.pixelate_relative);
            u.0[slot::PIXELATE] = [k * pxx.max(1.0) / ctx.win.0, k * pxy.max(1.0) / ctx.win.1, if self.pixelate_hard { 1.0 } else { 0.0 }, 0.0];
        }

        (used || self.tweening()).then_some(u)
    }
}

/// The level's shader triggers by level order, and the running state.
#[derive(Clone, Debug, Default)]
pub struct Shaders {
    specs: std::sync::Arc<HashMap<u32, ShaderSpec>>,
    pub state: ShaderState,
}

impl Shaders {
    pub fn from_level(level: &crate::level::LevelString) -> Shaders {
        let specs = level
            .objects
            .iter()
            .enumerate()
            .filter_map(|(order, o)| Some((order as u32, ShaderSpec::parse(o)?)))
            .collect();
        Shaders { specs: std::sync::Arc::new(specs), state: ShaderState::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// A trigger fired (by level order, `World::fired_effects`); others are ignored.
    pub fn fire(&mut self, order: u32) {
        if let Some(spec) = self.specs.get(&order) {
            self.state.trigger(spec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LevelString;

    fn obj(s: &str) -> RawObject {
        LevelString::parse(&format!("kS38,1;{s};")).objects.remove(0)
    }

    #[test]
    fn parses_dash_keys() {
        // Dash's shockwave at x 15765.5 (touch triggered, target group 304).
        let s = ShaderSpec::parse(&obj("1,2905,2,15765.5,3,2280,155,9,11,1,170,10,36,1,175,1.058,176,1,191,1,177,4.813,179,1.324,180,0.399,189,1,181,0.554,182,1.51,188,1,10,0.5,85,2,51,304,196,-842150451,197,-842150451,531,1")).unwrap();
        assert_eq!(s.id, 2905);
        assert_eq!(s.target, 304);
        assert_eq!(s.fv(175), 1.058);
        assert_eq!(s.fv(177), 4.813);
        assert!(s.bv(188) && !s.bv(184) && s.bv(531));
        assert_eq!(s.duration, 0.5);
        assert_eq!(s.min_layer, -842150451);
        // Player targets win over the group.
        let s = ShaderSpec::parse(&obj("1,2913,2,4517,3,285,51,7,138,1")).unwrap();
        assert_eq!(s.target, -1);
        assert!(ShaderSpec::parse(&obj("1,901,2,0,3,0")).is_none());
        assert_eq!(atoi("12abc"), 12);
        assert_eq!(atoi("-3.7"), -3);
        assert_eq!(atof("2e10"), 1e9);
    }

    #[test]
    fn setup_layers_default_to_everything() {
        let mut st = ShaderState::new();
        st.trigger(&ShaderSpec::parse(&obj("1,2904,2,255,3,-285,155,9,36,1,197,15")).unwrap());
        assert_eq!((st.min_layer, st.max_layer), (1, 15));
        st.trigger(&ShaderSpec::parse(&obj("1,2904,2,1500,3,1005,197,12")).unwrap());
        assert_eq!((st.min_layer, st.max_layer), (1, 12));
        st.trigger(&ShaderSpec::parse(&obj("1,2904,2,1500,3,1005,196,-842150451,197,-842150451")).unwrap());
        assert_eq!((st.min_layer, st.max_layer), (1, 15));
    }

    fn ctx_uniforms(st: &mut ShaderState) -> Option<Uniforms> {
        let pos = |_| Some((100.0, 100.0));
        let ch = |_| ([255, 0, 0], false);
        let ctx = Context { view: (0.0, 0.0, 480.0, 320.0), win: (480.0, 320.0), zoom: 1.0, position: &pos, channel: &ch, background: [10, 20, 30] };
        st.uniforms(&ctx)
    }

    #[test]
    fn tweens_like_gjvaluetween() {
        let mut st = ShaderState::new();
        // Chromatic x 5 at once, then back to 0 over 0.5 s with easing 1 (in-out, rate 2).
        st.trigger(&ShaderSpec::parse(&obj("1,2910,2,375,3,-225,175,1,176,1,180,5,188,1,85,2")).unwrap());
        assert_eq!(st.v[key::CHROMATIC_X], 5.0);
        st.trigger(&ShaderSpec::parse(&obj("1,2910,2,405,3,-225,188,1,10,0.5,30,1,85,2")).unwrap());
        assert_eq!(st.v[key::CHROMATIC_X], 5.0);
        st.frame(0.125, &|_| None);
        // t = 0.25 → in-out: (0.5)^2 / 2 = 0.125 → 5 − 0.625
        assert!((st.v[key::CHROMATIC_X] - 4.375).abs() < 1e-5);
        let u = ctx_uniforms(&mut st).unwrap();
        assert!((u.0[slot::GLITCH1][2] - 4.375 / SHADER_SCALE / 480.0).abs() < 1e-7);
        for _ in 0..3 {
            st.frame(0.125, &|_| None);
        }
        assert_eq!(st.v[key::CHROMATIC_X], 0.0);
        assert!(!st.tweening());
        assert!(ctx_uniforms(&mut st).is_none());
    }

    #[test]
    fn edit_color_and_invert() {
        let mut st = ShaderState::new();
        st.trigger(&ShaderSpec::parse(&obj("1,2923,2,2475,3,405,175,1.1,176,1.1,191,1.1,179,0.2,180,0.05")).unwrap());
        let u = ctx_uniforms(&mut st).unwrap();
        assert_eq!(u.0[slot::CC_C], [1.1, 1.1, 1.1, 0.0]);
        assert_eq!(u.0[slot::CC_B], [0.2, 0.05, 0.0, 0.0]);
        // Back to identity: the shader branch is off (C = 0).
        st.trigger(&ShaderSpec::parse(&obj("1,2923,2,2725,3,375,175,1,176,1,191,1")).unwrap());
        let u = ctx_uniforms(&mut st);
        assert!(u.is_none());
        st.trigger(&ShaderSpec::parse(&obj("1,2921,2,975,3,1215,176,1,179,2,180,1,189,1,188,1,194,1")).unwrap());
        let u = ctx_uniforms(&mut st).unwrap();
        assert_eq!(u.0[slot::INVERT], [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn shockwave_rings() {
        let mut st = ShaderState::new();
        st.time = 10.0;
        st.trigger(&ShaderSpec::parse(&obj("1,2905,2,15765.5,3,2280,175,1.058,176,1,191,1,177,4.813,179,1.324,180,0.399,181,0.554,182,1.51,188,1,51,304")).unwrap());
        st.frame(1.0 / 60.0, &|g| (g == 304).then_some((240.0, 160.0)));
        let u = ctx_uniforms(&mut st).unwrap();
        // centered on the target (the middle of the view)
        assert_eq!((u.0[slot::SW2][0], u.0[slot::SW2][1]), (0.5, 0.5));
        let mult = SHADER_SCALE;
        let t = 1.058f32 / 60.0;
        let t2 = 4.813f32.mul_add(0.1, t);
        let fade_in = 0.554 * 0.399 * 0.2f32;
        assert!((u.0[slot::SW0][2] - (t2 * t2 - fade_in) * mult).abs() < 1e-4);
        assert!((u.0[slot::SW1][1] - mult * 0.1).abs() < 1e-6);
        assert!(u.0[slot::SW0][0] > 0.0);
    }
}

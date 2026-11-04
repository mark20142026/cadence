//! The death effects (`PlayerObject::playDeathEffect` 0x1a176c), chosen in the icon kit.
//!
//! Effect 1 is the default: `explodeEffect` in the main color, one circle wave and the icon
//! shatter. Effects 2–20 each add an animated sprite from their own sheet
//! (`PlayerExplosion_NN`, frames `playerExplosion_NN_%03d.png`, NN = effect − 1) on top of the
//! object layer (z 10000), their own particle system (`explodeEffect`, `explodeEffectGrav` or
//! `explodeEffectVortex`, recoloured and rescaled) and two circle waves; every effect but 20
//! ends with the shatter. The numbers below are the function's own (its switch on the effect id,
//! one case per effect, read from the instructions); `R` is a fresh `rand() / 2^31` each time.

use crate::particles::{CircleWave, Emitter, ParticleDef, PositionType, Rng};

/// A random number the way the effect draws it.
#[derive(Clone, Copy, Debug)]
enum Num {
    Fixed(f32),
    /// base + var · (2R − 1)
    Centered(f32, f32),
    /// base + var · R
    Up(f32, f32),
}

impl Num {
    fn roll(self, rng: &mut Rng) -> f32 {
        match self {
            Num::Fixed(v) => v,
            Num::Centered(b, v) => b + v * (rng.unit() * 2.0 - 1.0),
            Num::Up(b, v) => b + v * rng.unit(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Rotation {
    None,
    /// a · (2R − 1)
    Centered(f32),
    /// −45 · R (effect 9: `-(float)rand() / 2^31 · 45`)
    Back45,
    /// 360 · R
    Full,
    /// 90, 180 or 270 at a quarter each in turn, else none (effect 14)
    Quarter,
}

/// The sprite's first animation frame: fixed, or `a` when R > 0.5 else `b`.
#[derive(Clone, Copy, Debug)]
enum Start {
    Fixed(u32),
    Coin(u32, u32),
}

/// The sprite's `CCDelayTime` + `CCFadeOut` (run beside the animation).
#[derive(Clone, Copy, Debug)]
enum Fade {
    None,
    /// delay · a, then a fade over delay · b
    Frames(f32, f32),
    /// fixed seconds (effect 7)
    Seconds(f32, f32),
}

#[derive(Clone, Copy, Debug)]
enum Color4 {
    C([f32; 4]),
    Main,
    Secondary,
}

/// The particle system's node scale: size · k, the sprite's scale · k, or (effect 10)
/// size · k · the sprite's scale.
#[derive(Clone, Copy, Debug)]
enum PScale {
    Size(f32),
    Sprite(f32),
    SizeSprite(f32),
}

#[derive(Clone, Copy, Debug)]
struct Particles {
    file: &'static str,
    z: i32,
    /// `setBlendAdditive`, when the effect calls it.
    additive: Option<bool>,
    start: Color4,
    end: Color4,
    start_var: [f32; 4],
    end_var: [f32; 4],
    speed: f32,
    scale: PScale,
    pos_var: f32,
    start_size: f32,
    end_size: f32,
    angle_var: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
enum WaveColor {
    C([u8; 3]),
    Main,
}

#[derive(Clone, Copy, Debug)]
struct Wave {
    /// end radius ÷ the wave size (the start is always 10 ×)
    r1: f32,
    /// seconds, or (negative) −k for the frame delay · k
    t: f32,
    color: WaveColor,
    opacity: f32,
    /// `ccDrawCircle` at glLineWidth 10 instead of a filled disc
    ring: bool,
    z: i32,
}

/// What the waves' sizes are measured in.
#[derive(Clone, Copy, Debug)]
enum WaveSize {
    Size,
    Sprite,
    Sprite08,
}

#[derive(Clone, Copy, Debug)]
struct Spec {
    /// The sprite is created with frame `first`, then animates `start..=last`.
    first: u32,
    start: Start,
    last: u32,
    scale: Num,
    rotation: Rotation,
    /// placed 6 above the player
    lift: bool,
    /// `setBlendFunc(GL_SRC_ALPHA, GL_ONE)`
    additive: bool,
    delay: Num,
    fade: Fade,
    /// effect 11: tints to the main color over 2 frame delays, then to the secondary over 8
    tint: bool,
    particles: Particles,
    wave_size: WaveSize,
    waves: [Wave; 2],
    shatter: bool,
}

const VAR0: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn c(r: f32, g: f32, b: f32) -> Color4 {
    Color4::C([r, g, b, 1.0])
}

/// `explodeEffect`-style particles with the usual zero variances.
fn parts(file: &'static str, z: i32, start: Color4, end: Color4, speed: f32, scale: PScale) -> Particles {
    Particles { file, z, additive: None, start, end, start_var: VAR0, end_var: VAR0, speed, scale, pos_var: 1.0, start_size: 1.0, end_size: 1.0, angle_var: None }
}

fn wave(r1: f32, t: f32, color: [u8; 3], opacity: f32, ring: bool, z: i32) -> Wave {
    Wave { r1, t, color: WaveColor::C(color), opacity, ring, z }
}

const E: &str = "explodeEffect.plist";
const GRAV: &str = "explodeEffectGrav.plist";
const VORTEX: &str = "explodeEffectVortex.plist";

/// The fire effects 17, 19 and 20 share `explodeEffectGrav` in orange with a 30° spread.
fn fire() -> Particles {
    Particles {
        additive: Some(false),
        start_var: [10.0 / 255.0, 10.0 / 255.0, 0.0, 1.0],
        angle_var: Some(30.0),
        ..parts(GRAV, 99, c(1.0, 200.0 / 255.0, 0.0), c(1.0, 50.0 / 255.0, 0.0), 1.0, PScale::Sprite(1.0))
    }
}

/// The second, 0.8 × smaller explodeEffect the effects 3, 6, 8 and 18 use (start size × 0.5, end
/// size × 0.1).
fn small(p: Particles) -> Particles {
    Particles { start_size: 0.5, end_size: 0.1, ..p }
}

/// The vortex settings 9 and 13–16 share: position spread × 1.2, sizes × 0.8 / × 0.4.
fn spread(p: Particles) -> Particles {
    Particles { additive: Some(true), pos_var: 1.2, start_size: 0.8, end_size: 0.4, ..p }
}

fn spec(effect: u32) -> Option<Spec> {
    let v = 1.0 / 255.0;
    let ring2 = |r1: f32, color: [u8; 3]| wave(r1, 0.4, color, 1.0, true, 99);
    let orange = [255, 150, 0];
    let cyan = [0, 255, 255];
    let base = Spec {
        first: 1,
        start: Start::Fixed(1),
        last: 1,
        scale: Num::Fixed(1.0),
        rotation: Rotation::Centered(45.0),
        lift: false,
        additive: false,
        delay: Num::Fixed(0.05),
        fade: Fade::Frames(6.0, 6.0),
        tint: false,
        particles: parts(E, 99, c(1.0, 1.0, 0.0), c(1.0, 0.0, 0.0), 0.8, PScale::Size(1.0)),
        wave_size: WaveSize::Size,
        waves: [wave(110.0, 0.5, [255, 255, 0], 1.0, false, 1000), ring2(115.0, orange)],
        shatter: true,
    };
    Some(match effect {
        2 => Spec {
            start: Start::Coin(1, 2),
            last: 11,
            scale: Num::Centered(1.5, 0.2),
            delay: Num::Up(0.05, 0.01),
            waves: [wave(110.0, 0.6, [255, 255, 0], 0.6, false, 1000), ring2(115.0, orange)],
            ..base
        },
        3 => Spec {
            start: Start::Fixed(2),
            last: 10,
            scale: Num::Centered(1.2, 0.2),
            delay: Num::Up(0.05, 0.01),
            particles: small(parts(E, 99, c(0.0, 200.0 * v, 1.0), c(0.0, 1.0, 1.0), 0.8, PScale::Size(0.8))),
            waves: [wave(110.0, 0.5, [125, 255, 255], 1.0, false, 1000), ring2(115.0, cyan)],
            ..base
        },
        4 => Spec {
            start: Start::Fixed(1),
            last: 10,
            scale: Num::Centered(1.4, 0.2),
            delay: Num::Up(0.04, 0.005),
            fade: Fade::Frames(5.0, 10.0),
            particles: Particles {
                additive: Some(false),
                start_var: [100.0 * v, 0.0, 0.0, 1.0],
                ..parts(GRAV, 99, c(0.0, 1.0, 0.0), c(0.0, 1.0, 0.0), 0.9, PScale::Size(1.0))
            },
            waves: [wave(100.0, 0.5, [150, 255, 0], 1.0, false, 1000), ring2(115.0, [0, 255, 0])],
            ..base
        },
        5 => Spec {
            start: Start::Coin(1, 2),
            last: 9,
            scale: Num::Centered(1.2, 0.1),
            delay: Num::Centered(0.05, 0.005),
            fade: Fade::Frames(8.0, 6.0),
            particles: parts(VORTEX, 101, c(1.0, 100.0 * v, 1.0), c(1.0, 0.0, 150.0 * v), 1.0, PScale::Size(1.0)),
            waves: [wave(110.0, 0.5, [255, 100, 255], 1.0, false, 1000), ring2(115.0, [255, 0, 255])],
            ..base
        },
        6 => Spec {
            start: Start::Coin(2, 3),
            last: 10,
            scale: Num::Centered(1.25, 0.15),
            delay: Num::Centered(0.05, 0.005),
            particles: small(parts(E, 99, c(1.0, 200.0 * v, 0.0), c(1.0, 150.0 * v, 0.0), 0.8, PScale::Size(0.8))),
            waves: [wave(110.0, 0.5, [255, 200, 0], 1.0, false, 1000), ring2(115.0, [255, 255, 0])],
            ..base
        },
        7 => Spec {
            start: Start::Fixed(1),
            last: 13,
            scale: Num::Centered(1.3, 0.15),
            rotation: Rotation::None,
            delay: Num::Fixed(0.055),
            fade: Fade::Seconds(0.44, 0.33),
            particles: fire(),
            wave_size: WaveSize::Sprite,
            waves: [wave(110.0, 0.6, [255, 200, 0], 1.0, false, 1000), ring2(115.0, orange)],
            ..base
        },
        8 => Spec {
            start: Start::Coin(1, 2),
            last: 11,
            scale: Num::Centered(1.3, 0.2),
            rotation: Rotation::Centered(90.0),
            delay: Num::Up(0.04, 0.005),
            particles: small(parts(E, 99, c(1.0, 0.0, 1.0), c(1.0, 0.0, 200.0 * v), 0.8, PScale::Sprite(0.8))),
            wave_size: WaveSize::Sprite,
            waves: [wave(85.0, 0.5, [255, 0, 255], 1.0, false, 1000), ring2(90.0, [255, 0, 200])],
            ..base
        },
        9 => Spec {
            start: Start::Fixed(2),
            last: 11,
            scale: Num::Up(0.8, 0.1),
            rotation: Rotation::Back45,
            lift: true,
            additive: true,
            delay: Num::Up(0.045, 0.01),
            fade: Fade::Frames(5.0, 8.0),
            particles: spread(parts(VORTEX, 99, c(0.0, 1.0, 1.0), c(0.0, 150.0 * v, 1.0), 0.9, PScale::Sprite(1.4))),
            wave_size: WaveSize::Sprite,
            waves: [wave(120.0, -10.0, [75, 255, 255], 0.8, false, 1000), ring2(115.0, cyan)],
            ..base
        },
        10 => Spec {
            start: Start::Fixed(2),
            last: 13,
            scale: Num::Centered(1.0, 0.1),
            delay: Num::Centered(0.045, 0.005),
            fade: Fade::Frames(10.0, 3.0),
            particles: Particles {
                additive: Some(true),
                start_size: 1.2,
                end_size: 0.5,
                ..parts(VORTEX, 101, c(200.0 * v, 1.0, 1.0), c(0.0, 100.0 * v, 1.0), 1.0, PScale::SizeSprite(1.2))
            },
            waves: [wave(120.0, 0.5, [0, 200, 255], 1.0, false, 1000), ring2(125.0, cyan)],
            ..base
        },
        11 => Spec {
            start: Start::Fixed(1),
            last: 12,
            scale: Num::Centered(1.5, 0.3),
            rotation: Rotation::None,
            additive: true,
            delay: Num::Centered(0.035, 0.005),
            fade: Fade::Frames(7.0, 6.0),
            tint: true,
            particles: Particles {
                additive: Some(true),
                start_size: 1.2,
                end_size: 0.5,
                ..parts(VORTEX, 101, Color4::Main, Color4::Secondary, 1.0, PScale::Sprite(1.1))
            },
            wave_size: WaveSize::Sprite,
            waves: [
                Wave { color: WaveColor::Main, ..wave(80.0, 0.5, [0; 3], 1.0, false, 1000) },
                Wave { color: WaveColor::Main, ..ring2(90.0, [0; 3]) },
            ],
            ..base
        },
        12 => Spec {
            start: Start::Fixed(1),
            last: 12,
            scale: Num::Centered(1.25, 0.1),
            delay: Num::Centered(0.04, 0.005),
            fade: Fade::None, // (the case builds a delay + fade-out but never runs it)
            particles: Particles { additive: Some(true), end_size: 0.5, ..parts(VORTEX, 101, c(1.0, 1.0, 0.0), c(1.0, 0.0, 0.0), 1.2, PScale::Sprite(1.2)) },
            wave_size: WaveSize::Sprite,
            waves: [wave(125.0, 0.5, [255, 100, 0], 0.8, false, 1000), ring2(135.0, [255, 175, 0])],
            ..base
        },
        13 => Spec {
            start: Start::Fixed(1),
            last: 12,
            scale: Num::Up(0.9, 0.2),
            lift: true,
            delay: Num::Up(0.04, 0.005),
            fade: Fade::None, // (the case builds a delay + fade-out but never runs it)
            particles: spread(parts(VORTEX, 1000, c(1.0, 1.0, 0.0), c(0.0, 1.0, 0.0), 0.9, PScale::Sprite(1.5))),
            wave_size: WaveSize::Sprite,
            waves: [wave(120.0, -10.0, [50, 255, 50], 1.0, false, 1000), ring2(115.0, [0, 255, 0])],
            ..base
        },
        14 => Spec {
            start: Start::Fixed(3),
            last: 12,
            scale: Num::Up(1.0, 0.4),
            rotation: Rotation::Quarter,
            lift: true,
            additive: true,
            delay: Num::Up(0.045, 0.005),
            fade: Fade::None, // (the case builds a delay + fade-out but never runs it)
            particles: spread(parts(E, 99, c(1.0, 0.0, 1.0), c(0.0, 0.0, 1.0), 0.9, PScale::Sprite(0.8 * 1.4))),
            wave_size: WaveSize::Sprite08,
            waves: [wave(120.0, -10.0, [0, 200, 255], 0.8, false, 1000), ring2(115.0, [0, 150, 255])],
            ..base
        },
        15 => Spec {
            first: 2,
            start: Start::Fixed(3),
            last: 15,
            scale: Num::Up(1.0, 0.2),
            lift: true,
            additive: true,
            delay: Num::Up(0.033, 0.004),
            fade: Fade::None, // (the case builds a delay + fade-out but never runs it)
            particles: spread(parts(VORTEX, 99, c(0.0, 1.0, 1.0), c(1.0, 0.0, 100.0 * v), 1.0, PScale::Sprite(0.8 * 1.4))),
            wave_size: WaveSize::Sprite08,
            waves: [wave(120.0, -10.0, [0, 200, 255], 0.8, false, 1000), ring2(115.0, [0, 150, 255])],
            ..base
        },
        16 => Spec {
            start: Start::Fixed(2),
            last: 16,
            scale: Num::Up(1.4, 0.4),
            rotation: Rotation::Full,
            lift: true,
            delay: Num::Up(0.04, 0.004),
            fade: Fade::None,
            particles: spread(parts(VORTEX, 99, c(150.0 * v, 1.0, 1.0), c(0.0, 0.0, 1.0), 1.0, PScale::Sprite(0.8 * 1.4))),
            wave_size: WaveSize::Sprite08,
            waves: [wave(90.0, -10.0, [0, 100, 255], 1.0, false, 1000), wave(80.0, -14.0, [0, 100, 255], 1.0, false, 99)],
            ..base
        },
        17 | 19 | 20 => Spec {
            first: 2,
            start: Start::Fixed(2),
            last: match effect {
                17 => 19,
                19 => 23,
                _ => 13,
            },
            scale: Num::Up(1.0, 0.3),
            rotation: match effect {
                17 => Rotation::None,
                19 => Rotation::Centered(180.0),
                _ => Rotation::Centered(45.0),
            },
            delay: if effect == 19 { Num::Up(0.035, 0.005) } else { Num::Up(0.04, 0.005) },
            fade: match effect {
                17 => Fade::Frames(12.0, 6.0),
                19 => Fade::Frames(18.0, 5.0),
                _ => Fade::None,
            },
            particles: fire(),
            wave_size: WaveSize::Sprite,
            waves: [wave(110.0, 0.6, [255, 200, 0], 1.0, false, 99), wave(115.0, 0.4, orange, 1.0, true, if effect == 20 { 101 } else { 99 })],
            shatter: effect != 20,
            ..base
        },
        18 => Spec {
            first: 2,
            start: Start::Fixed(2),
            last: 15,
            scale: Num::Up(1.0, 0.2),
            rotation: Rotation::Centered(180.0),
            additive: true,
            delay: Num::Up(0.035, 0.005),
            fade: Fade::Frames(12.0, 6.0),
            particles: small(parts(E, 99, Color4::C([1.0; 4]), c(0.0, 100.0 * v, 1.0), 0.8, PScale::Size(0.8))),
            waves: [wave(90.0, 0.5, cyan, 1.0, true, 1000), ring2(100.0, cyan)],
            ..base
        },
        _ => return None,
    })
}

/// The ranges of an effect's random parts (for checks against the real game).
#[derive(Clone, Debug)]
pub struct Ranges {
    pub scale: (f32, f32),
    pub rotation: (f32, f32),
    /// rotations are 0, 90, 180 or 270 only
    pub quarter: bool,
    pub starts: Vec<u32>,
    pub delay: (f32, f32),
    pub first: u32,
    pub last: u32,
}

pub fn ranges(effect: u32) -> Option<Ranges> {
    let s = spec(effect)?;
    let range = |n: Num| match n {
        Num::Fixed(v) => (v, v),
        Num::Centered(b, v) => (b - v, b + v),
        Num::Up(b, v) => (b, b + v),
    };
    let (rotation, quarter) = match s.rotation {
        Rotation::None => ((0.0, 0.0), false),
        Rotation::Centered(a) => ((-a, a), false),
        Rotation::Back45 => ((-45.0, 0.0), false),
        Rotation::Full => ((0.0, 360.0), false),
        Rotation::Quarter => ((0.0, 270.0), true),
    };
    let starts = match s.start {
        Start::Fixed(n) => vec![n],
        Start::Coin(a, b) => vec![a, b],
    };
    Some(Ranges { scale: range(s.scale), rotation, quarter, starts, delay: range(s.delay), first: s.first, last: s.last })
}

/// The particle files the death effects use.
pub const PARTICLE_FILES: [&str; 2] = [GRAV, VORTEX];

/// The sheet effect `effect` needs (`PlayerExplosion_NN`), if any.
pub fn sheet(effect: u32) -> Option<String> {
    spec(effect).map(|_| format!("PlayerExplosion_{:02}", effect - 1))
}

/// The frame a loaded sheet is known by.
pub fn probe_frame(effect: u32) -> Option<String> {
    spec(effect).map(|s| frame_name(effect, s.first))
}

fn frame_name(effect: u32, n: u32) -> String {
    format!("playerExplosion_{:02}_{n:03}.png", effect - 1)
}

/// The animated explosion sprite (in the object layer at z 10000, over everything there).
#[derive(Clone, Debug)]
pub struct DeathSprite {
    pub effect: u32,
    pub pos: (f32, f32),
    pub scale: f32,
    pub rotation: f32,
    pub additive: bool,
    first: u32,
    start: u32,
    last: u32,
    delay: f32,
    fade: Option<(f32, f32)>,
    tint: Option<([f32; 3], [f32; 3])>,
    /// Seconds into its actions; `None` until the first action tick (cocos2d's first step
    /// starts the actions at 0, so the sprite shows its created frame for one frame).
    elapsed: Option<f32>,
}

impl DeathSprite {
    /// Seconds the animation runs (frames · delay); the sprite is removed then.
    fn duration(&self) -> f32 {
        (self.last + 1 - self.start) as f32 * self.delay
    }

    pub fn update(&mut self, dt: f32) {
        self.elapsed = Some(self.elapsed.map_or(0.0, |t| t + dt));
    }

    pub fn finished(&self) -> bool {
        self.elapsed.is_some_and(|t| t >= self.duration())
    }

    /// The frame shown now (`CCAnimate`: frame i from i · delay).
    pub fn frame(&self) -> String {
        let n = match self.elapsed {
            None => self.first,
            Some(t) => (self.start + (t / self.delay) as u32).min(self.last),
        };
        frame_name(self.effect, n)
    }

    /// The sprite's color (white, or effect 11's tints) and opacity, 0..1 each.
    pub fn color(&self) -> [f32; 4] {
        let t = self.elapsed.unwrap_or(0.0);
        let opacity = match self.fade {
            Some((d, f)) if t > d => (255.0 * (1.0 - ((t - d) / f).min(1.0))) as u8 as f32 / 255.0,
            _ => 1.0,
        };
        let rgb = match self.tint {
            // CCTintTo over 2 delays to the main color, then over 8 to the secondary
            Some((main, second)) => {
                let byte = |from: f32, to: f32, k: f32| ((from + (to - from) * k.clamp(0.0, 1.0)) * 255.0) as u8 as f32 / 255.0;
                let (a, b) = (2.0 * self.delay, 8.0 * self.delay);
                if t < a {
                    std::array::from_fn(|i| byte(1.0, main[i], t / a))
                } else {
                    std::array::from_fn(|i| byte(main[i], second[i], (t - a) / b))
                }
            }
            None => [1.0; 3],
        };
        [rgb[0], rgb[1], rgb[2], opacity]
    }
}

/// Everything one death effect starts.
pub struct Death {
    pub sprite: Option<DeathSprite>,
    pub emitter: Emitter,
    pub waves: Vec<CircleWave>,
    /// The icon shatter (`ExplodeItemNode`) follows.
    pub shatter: bool,
}

/// The random parts of one death effect: the sprite's scale (÷ size), rotation, first animation
/// frame and frame delay.
#[derive(Clone, Copy, Debug)]
pub struct Rolls {
    pub scale: f32,
    pub rotation: f32,
    pub start: u32,
    pub delay: f32,
}

fn roll(s: &Spec, rng: &mut Rng) -> Rolls {
    // as the cases draw them: scale, rotation, the first frame's coin, the delay
    let scale = s.scale.roll(rng);
    let rotation = match s.rotation {
        Rotation::None => 0.0,
        Rotation::Centered(a) => a * (rng.unit() * 2.0 - 1.0),
        Rotation::Back45 => -45.0 * rng.unit(),
        Rotation::Full => 360.0 * rng.unit(),
        Rotation::Quarter => [90.0, 180.0, 270.0].into_iter().find(|_| rng.unit() < 0.25).unwrap_or(0.0),
    };
    let start = match s.start {
        Start::Fixed(n) => n,
        Start::Coin(a, b) => {
            if rng.unit() > 0.5 {
                a
            } else {
                b
            }
        }
    };
    let delay = s.delay.roll(rng);
    Rolls { scale, rotation, start, delay }
}

/// Start death effect `effect` (1 = the default) at `at`. `size` is the vehicle size (x 0.9
/// when at least 1), `colors` the player's main and secondary colors, `defs` the particle files.
pub fn play(effect: u32, at: (f32, f32), size: f32, colors: ([f32; 3], [f32; 3]), rng: &mut Rng, seed: u64, defs: &dyn Fn(&str) -> Option<ParticleDef>) -> Option<Death> {
    let rolls = spec(effect).map(|s| roll(&s, rng));
    build(effect, at, size, colors, rolls, seed, defs)
}

/// `play` with the random parts given.
pub fn build(effect: u32, at: (f32, f32), size: f32, colors: ([f32; 3], [f32; 3]), rolls: Option<Rolls>, seed: u64, defs: &dyn Fn(&str) -> Option<ParticleDef>) -> Option<Death> {
    let (Some(s), Some(Rolls { scale: rolled, rotation, start, delay })) = (spec(effect), rolls) else {
        // the default: explodeEffect grouped at z 100 in the main color, node scale = size; a
        // wave size·10 → size·90 over 0.5 s at z 99
        let mut def = defs(E)?;
        def.start_color = [colors.0[0], colors.0[1], colors.0[2], 1.0];
        let mut emitter = Emitter::new(def, at, seed);
        emitter.position_type = PositionType::Grouped;
        emitter.auto_remove = true;
        emitter.scale = (size, size);
        emitter.z = 100;
        let w = CircleWave { z: 99, first_tick: true, ..CircleWave::new(at, size * 10.0, size * 90.0, 0.5, colors.0) };
        return Some(Death { sprite: None, emitter, waves: vec![w], shatter: true });
    };
    let sprite_scale = size * rolled;
    let pos = if s.lift { (at.0, at.1 + 6.0) } else { at };
    let fade = match s.fade {
        Fade::None => None,
        Fade::Frames(a, b) => Some((delay * a, delay * b)),
        Fade::Seconds(a, b) => Some((a, b)),
    };
    let sprite = DeathSprite {
        effect,
        pos,
        scale: sprite_scale,
        rotation,
        additive: s.additive,
        first: s.first,
        start,
        last: s.last,
        delay,
        fade,
        tint: s.tint.then_some(colors),
        elapsed: None,
    };
    // the particles
    let p = s.particles;
    let mut def = defs(p.file)?;
    let color = |c: Color4| match c {
        Color4::C(v) => v,
        Color4::Main => [colors.0[0], colors.0[1], colors.0[2], 1.0],
        Color4::Secondary => [colors.1[0], colors.1[1], colors.1[2], 1.0],
    };
    def.start_color = color(p.start);
    def.end_color = color(p.end);
    def.start_color_var = p.start_var;
    def.end_color_var = p.end_var;
    if let Some(additive) = p.additive {
        def.additive = additive;
    }
    if let Some(a) = p.angle_var {
        def.angle_var = a;
    }
    def.speed *= p.speed;
    def.pos_var = (def.pos_var.0 * p.pos_var, def.pos_var.1 * p.pos_var);
    def.start_size *= p.start_size;
    def.end_size *= p.end_size;
    let node_scale = match p.scale {
        PScale::Size(k) => size * k,
        PScale::Sprite(k) => sprite_scale * k,
        PScale::SizeSprite(k) => size * k * sprite_scale,
    };
    let mut emitter = Emitter::new(def, at, seed);
    emitter.position_type = PositionType::Grouped;
    emitter.auto_remove = true;
    emitter.scale = (node_scale, node_scale);
    emitter.z = p.z;
    // the waves
    let wsize = match s.wave_size {
        WaveSize::Size => size,
        WaveSize::Sprite => sprite_scale,
        WaveSize::Sprite08 => sprite_scale * 0.8,
    };
    let waves = s
        .waves
        .iter()
        .map(|w| {
            let t = if w.t < 0.0 { delay * -w.t } else { w.t };
            let rgb = match w.color {
                WaveColor::C(c) => c.map(|v| v as f32 / 255.0),
                WaveColor::Main => colors.0,
            };
            CircleWave {
                z: w.z,
                opacity_mod: w.opacity,
                outline: w.ring,
                line_width: if w.ring { 10.0 } else { 2.0 },
                first_tick: true,
                ..CircleWave::new(at, wsize * 10.0, wsize * w.r1, t, rgb)
            }
        })
        .collect();
    Some(Death { sprite: Some(sprite), emitter, waves, shatter: s.shatter })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs(name: &str) -> Option<ParticleDef> {
        Some(ParticleDef { texture: name.into(), speed: 100.0, start_size: 10.0, end_size: 4.0, pos_var: (5.0, 5.0), additive: true, ..Default::default() })
    }

    #[test]
    fn every_effect_has_a_sheet_and_two_waves() {
        let mut rng = Rng::new(7);
        for effect in 2..=20 {
            let d = play(effect, (100.0, 200.0), 0.9, ([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]), &mut rng, 1, &defs).unwrap();
            assert_eq!(d.waves.len(), 2, "effect {effect}");
            assert_eq!(sheet(effect).unwrap(), format!("PlayerExplosion_{:02}", effect - 1));
            assert_eq!(d.shatter, effect != 20);
            let s = d.sprite.unwrap();
            assert!(s.frame().starts_with(&format!("playerExplosion_{:02}_", effect - 1)));
        }
        assert!(sheet(1).is_none());
        assert!(play(1, (0.0, 0.0), 1.0, ([1.0; 3], [1.0; 3]), &mut rng, 1, &defs).unwrap().sprite.is_none());
    }

    #[test]
    fn sprite_animates_after_its_first_tick_and_fades() {
        let mut rng = Rng::new(3);
        // effect 7: fixed delay 0.055, frames 1..=13, fade from 0.44 over 0.33
        let mut s = play(7, (0.0, 0.0), 0.9, ([1.0; 3], [1.0; 3]), &mut rng, 1, &defs).unwrap().sprite.unwrap();
        assert_eq!(s.frame(), "playerExplosion_06_001.png");
        s.update(1.0 / 60.0);
        assert_eq!(s.frame(), "playerExplosion_06_001.png");
        assert_eq!(s.color()[3], 1.0);
        for _ in 0..30 {
            s.update(1.0 / 60.0);
        }
        // 0.5 s in: frame 1 + floor(0.5 / 0.055) = 10, fading
        assert_eq!(s.frame(), "playerExplosion_06_010.png");
        assert!(s.color()[3] < 1.0 && s.color()[3] > 0.5);
        for _ in 0..20 {
            s.update(1.0 / 60.0);
        }
        assert!(s.finished());
    }

    #[test]
    fn particles_take_the_effect_settings() {
        let mut rng = Rng::new(1);
        let d = play(9, (0.0, 0.0), 0.9, ([1.0; 3], [1.0; 3]), &mut rng, 1, &defs).unwrap();
        let e = &d.emitter;
        assert!((e.def.speed - 90.0).abs() < 1e-3);
        assert!((e.def.pos_var.0 - 6.0).abs() < 1e-4);
        assert!((e.def.start_size - 8.0).abs() < 1e-4 && (e.def.end_size - 1.6).abs() < 1e-4);
        assert_eq!(e.def.start_color, [0.0, 1.0, 1.0, 1.0]);
        let s = d.sprite.unwrap();
        assert!((e.scale.0 - s.scale * 1.4).abs() < 1e-5);
        assert_eq!(s.pos, (0.0, 6.0));
        assert!(s.additive && s.rotation <= 0.0 && s.rotation > -45.0);
    }
}

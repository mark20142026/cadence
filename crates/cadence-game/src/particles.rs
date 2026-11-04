//! Particles: a port of cocos2d-x 2.2's `CCParticleSystem` (the version Geometry Dash uses),
//! driven by the game's own effect files (`explodeEffect.plist`, `dragEffect.plist`, ...) read
//! from the player's Resources. Particles are random, so they can't match the game frame for
//! frame, but rates, speeds, lifetimes, sizes, colors and blending are the game's.
//! Also `CCCircleWave`, the expanding circle the game shows on death and on orbs.

use std::collections::HashMap;

use crate::files::Files;

/// One effect file's settings (cocos2d particle plist keys).
#[derive(Clone, Debug, Default)]
pub struct ParticleDef {
    pub texture: String,
    pub max_particles: usize,
    pub duration: f32,
    pub life: f32,
    pub life_var: f32,
    pub angle: f32,
    pub angle_var: f32,
    pub start_color: [f32; 4],
    pub start_color_var: [f32; 4],
    pub end_color: [f32; 4],
    pub end_color_var: [f32; 4],
    pub start_size: f32,
    pub start_size_var: f32,
    pub end_size: f32,
    pub end_size_var: f32,
    pub pos_var: (f32, f32),
    pub rotation_start: f32,
    pub rotation_start_var: f32,
    pub rotation_end: f32,
    pub rotation_end_var: f32,
    /// 0 = gravity mode, 1 = radius mode.
    pub emitter_type: i32,
    pub gravity: (f32, f32),
    pub speed: f32,
    pub speed_var: f32,
    pub radial_accel: f32,
    pub radial_accel_var: f32,
    pub tangential_accel: f32,
    pub tangential_accel_var: f32,
    pub start_radius: f32,
    pub start_radius_var: f32,
    pub end_radius: f32,
    pub end_radius_var: f32,
    pub rotate_per_second: f32,
    pub rotate_per_second_var: f32,
    /// Additive when the destination blend factor is GL_ONE.
    pub additive: bool,
    /// Particles per second when set explicitly (GD's custom particles); else total / life.
    pub emission_rate: Option<f32>,
}

impl ParticleDef {
    /// A custom particle object's settings (object 2065, key 145): 'a'-separated values in
    /// `GameToolbox::particleStringToStruct` order (0x44fd80), applied like
    /// `FUN_100450ca4`. Many fields are read with atoi, so fractions are dropped as in the
    /// game. Returns the def (texture `particle_NN_001.png`) and whether particles move with the
    /// emitter (position type 2, grouped).
    pub fn from_gd_string(text: &str) -> Option<(ParticleDef, bool)> {
        let v: Vec<&str> = text.split('a').collect();
        if v.len() < 53 {
            return None;
        }
        let f = |i: usize| v.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) as f32;
        // atoi: leading integer part only.
        let n = |i: usize| {
            let s = v.get(i).copied().unwrap_or("");
            let end = s.char_indices().find(|&(k, c)| !(c.is_ascii_digit() || (k == 0 && (c == '-' || c == '+')))).map_or(s.len(), |(k, _)| k);
            s[..end].parse::<i64>().unwrap_or(0) as f32
        };
        let color = |i: usize| [f(i), f(i + 2), f(i + 4), f(i + 6)];
        let def = ParticleDef {
            texture: format!("particle_{:02}_001.png", n(57) as i32),
            max_particles: n(0).max(0.0) as usize,
            duration: f(1),
            life: f(2),
            life_var: f(3),
            emission_rate: Some(n(4)),
            angle: n(5),
            angle_var: n(6),
            speed: n(7),
            speed_var: n(8),
            pos_var: (n(9), n(10)),
            gravity: (n(11), n(12)),
            radial_accel: n(13),
            radial_accel_var: n(14),
            tangential_accel: n(15),
            tangential_accel_var: n(16),
            start_size: n(17),
            start_size_var: n(18),
            rotation_start: n(19),
            rotation_start_var: n(20),
            start_color: color(21),
            start_color_var: color(22),
            end_size: n(29),
            end_size_var: n(30),
            rotation_end: n(31),
            rotation_end_var: n(32),
            end_color: color(33),
            end_color_var: color(34),
            start_radius: n(45),
            start_radius_var: n(46),
            end_radius: n(47),
            end_radius_var: n(48),
            rotate_per_second: n(49),
            rotate_per_second_var: n(50),
            emitter_type: n(51) as i32,
            additive: n(53) != 0.0,
            ..Default::default()
        };
        Some((def, n(52) as i32 == 2))
    }

    pub fn parse(bytes: &[u8]) -> Option<ParticleDef> {
        let value: plist::Value = plist::from_bytes(bytes).ok()?;
        let dict = value.as_dictionary()?;
        let f = |k: &str| -> f32 {
            dict.get(k)
                .and_then(|v| v.as_real().or_else(|| v.as_signed_integer().map(|i| i as f64)))
                .unwrap_or(0.0) as f32
        };
        let color = |p: &str| [f(&format!("{p}Red")), f(&format!("{p}Green")), f(&format!("{p}Blue")), f(&format!("{p}Alpha"))];
        Some(ParticleDef {
            texture: dict.get("textureFileName").and_then(|v| v.as_string()).unwrap_or("square.png").to_string(),
            max_particles: f("maxParticles") as usize,
            duration: f("duration"),
            life: f("particleLifespan"),
            life_var: f("particleLifespanVariance"),
            angle: f("angle"),
            angle_var: f("angleVariance"),
            start_color: color("startColor"),
            start_color_var: color("startColorVariance"),
            end_color: color("finishColor"),
            end_color_var: color("finishColorVariance"),
            start_size: f("startParticleSize"),
            start_size_var: f("startParticleSizeVariance"),
            end_size: f("finishParticleSize"),
            end_size_var: f("finishParticleSizeVariance"),
            pos_var: (f("sourcePositionVariancex"), f("sourcePositionVariancey")),
            rotation_start: f("rotationStart"),
            rotation_start_var: f("rotationStartVariance"),
            rotation_end: f("rotationEnd"),
            rotation_end_var: f("rotationEndVariance"),
            emitter_type: f("emitterType") as i32,
            gravity: (f("gravityx"), f("gravityy")),
            speed: f("speed"),
            speed_var: f("speedVariance"),
            radial_accel: f("radialAcceleration"),
            radial_accel_var: f("radialAccelVariance"),
            tangential_accel: f("tangentialAcceleration"),
            tangential_accel_var: f("tangentialAccelVariance"),
            start_radius: f("maxRadius"),
            start_radius_var: f("maxRadiusVariance"),
            end_radius: f("minRadius"),
            end_radius_var: f("minRadiusVariance"),
            rotate_per_second: f("rotatePerSecond"),
            rotate_per_second_var: f("rotatePerSecondVariance"),
            additive: f("blendFuncDestination") as i32 == 1,
            emission_rate: None,
        })
    }
}

/// Where particles live once emitted (cocos2d `tCCPositionType`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionType {
    /// `kCCPositionTypeRelative`: particles stay where they were emitted in the level (the
    /// emitter can move on). (`Free` would pin them to the screen; the player's effects never use it.)
    Relative,
    /// `kCCPositionTypeGrouped`: particles move with the emitter.
    Grouped,
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    /// Position relative to where it was emitted (free) or to the emitter (grouped).
    pos: (f32, f32),
    start: (f32, f32),
    color: [f32; 4],
    delta_color: [f32; 4],
    size: f32,
    delta_size: f32,
    rotation: f32,
    delta_rotation: f32,
    ttl: f32,
    dir: (f32, f32),
    radial_accel: f32,
    tangential_accel: f32,
    angle: f32,
    degrees_per_second: f32,
    radius: f32,
    delta_radius: f32,
}

/// A small deterministic random source (cocos2d uses rand(); any uniform source will do).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Default for Rng {
    fn default() -> Rng {
        Rng::new(0x5eed)
    }
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1)
    }

    /// rand() / 2^31: uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// CCRANDOM_MINUS1_1: uniform in [-1, 1).
    pub fn minus1_1(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// One running particle system.
#[derive(Clone, Debug)]
pub struct Emitter {
    pub def: ParticleDef,
    pub position: (f32, f32),
    /// The node's rotation (degrees, clockwise) and scale; grouped particles are transformed by them.
    pub rotation: f32,
    pub scale: (f32, f32),
    pub position_type: PositionType,
    pub active: bool,
    /// Remove once stopped and empty (`setAutoRemoveOnFinish`).
    pub auto_remove: bool,
    elapsed: f32,
    emit_counter: f32,
    particles: Vec<Particle>,
    rng: Rng,
}

impl Emitter {
    pub fn new(def: ParticleDef, position: (f32, f32), seed: u64) -> Emitter {
        Emitter {
            def,
            position,
            rotation: 0.0,
            scale: (1.0, 1.0),
            position_type: PositionType::Relative,
            active: true,
            auto_remove: false,
            elapsed: 0.0,
            emit_counter: 0.0,
            particles: Vec::new(),
            rng: Rng::new(seed),
        }
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.elapsed = self.def.duration;
        self.emit_counter = 0.0;
    }

    /// `resumeSystem`: emit again (without clearing what's alive).
    pub fn resume(&mut self) {
        self.active = true;
    }

    /// `resetSystem`: start over (emitting again, the elapsed time from zero) and let the live
    /// particles expire at once.
    pub fn reset(&mut self) {
        self.active = true;
        self.elapsed = 0.0;
        for p in &mut self.particles {
            p.ttl = 0.0;
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    pub fn count(&self) -> usize {
        self.particles.len()
    }

    pub fn finished(&self) -> bool {
        !self.active && self.particles.is_empty()
    }

    fn add_particle(&mut self) {
        let d = &self.def;
        let r = &mut self.rng;
        let ttl = (d.life + d.life_var * r.minus1_1()).max(0.0);
        let pos = (d.pos_var.0 * r.minus1_1(), d.pos_var.1 * r.minus1_1());
        let mut start = [0.0f32; 4];
        let mut end = [0.0f32; 4];
        for i in 0..4 {
            start[i] = (d.start_color[i] + d.start_color_var[i] * r.minus1_1()).clamp(0.0, 1.0);
        }
        for i in 0..4 {
            end[i] = (d.end_color[i] + d.end_color_var[i] * r.minus1_1()).clamp(0.0, 1.0);
        }
        let inv = if ttl > 0.0 { 1.0 / ttl } else { 0.0 };
        let delta_color = std::array::from_fn(|i| (end[i] - start[i]) * inv);
        let start_size = (d.start_size + d.start_size_var * r.minus1_1()).max(0.0);
        let delta_size = if d.end_size == -1.0 {
            0.0
        } else {
            ((d.end_size + d.end_size_var * r.minus1_1()).max(0.0) - start_size) * inv
        };
        let rot_start = d.rotation_start + d.rotation_start_var * r.minus1_1();
        let rot_end = d.rotation_end + d.rotation_end_var * r.minus1_1();
        let a = (d.angle + d.angle_var * r.minus1_1()).to_radians();
        let mut p = Particle {
            pos,
            start: self.position,
            color: start,
            delta_color,
            size: start_size,
            delta_size,
            rotation: rot_start,
            delta_rotation: (rot_end - rot_start) * inv,
            ttl,
            dir: (0.0, 0.0),
            radial_accel: 0.0,
            tangential_accel: 0.0,
            angle: a,
            degrees_per_second: 0.0,
            radius: 0.0,
            delta_radius: 0.0,
        };
        if d.emitter_type == 0 {
            let s = d.speed + d.speed_var * r.minus1_1();
            p.dir = (a.cos() * s, a.sin() * s);
            p.radial_accel = d.radial_accel + d.radial_accel_var * r.minus1_1();
            p.tangential_accel = d.tangential_accel + d.tangential_accel_var * r.minus1_1();
        } else {
            let start_r = d.start_radius + d.start_radius_var * r.minus1_1();
            let end_r = d.end_radius + d.end_radius_var * r.minus1_1();
            p.radius = start_r;
            p.delta_radius = if d.end_radius == -1.0 { 0.0 } else { (end_r - start_r) * inv };
            p.degrees_per_second = (d.rotate_per_second + d.rotate_per_second_var * r.minus1_1()).to_radians();
        }
        self.particles.push(p);
    }

    /// `CCParticleSystem::update(dt)`.
    pub fn update(&mut self, dt: f32) {
        let rate = match self.def.emission_rate {
            Some(r) if r > 0.0 => 1.0 / r,
            Some(_) => f32::INFINITY,
            None if self.def.life > 0.0 => self.def.life / self.def.max_particles.max(1) as f32,
            None => 0.0,
        };
        if self.active {
            if self.particles.len() < self.def.max_particles {
                self.emit_counter += dt;
            }
            while self.particles.len() < self.def.max_particles && self.emit_counter > rate {
                self.add_particle();
                self.emit_counter -= rate;
                if rate == 0.0 && self.particles.len() >= self.def.max_particles {
                    break;
                }
            }
            self.elapsed += dt;
            if self.def.duration != -1.0 && self.def.duration < self.elapsed {
                self.stop();
            }
        }
        let gravity = self.def.gravity;
        let radius_mode = self.def.emitter_type == 1;
        self.particles.retain_mut(|p| {
            p.ttl -= dt;
            if p.ttl <= 0.0 {
                return false;
            }
            if !radius_mode {
                let len = (p.pos.0 * p.pos.0 + p.pos.1 * p.pos.1).sqrt();
                let radial = if len > 0.0 { (p.pos.0 / len, p.pos.1 / len) } else { (0.0, 0.0) };
                let tangential = (-radial.1 * p.tangential_accel, radial.0 * p.tangential_accel);
                let acc = (
                    radial.0 * p.radial_accel + tangential.0 + gravity.0,
                    radial.1 * p.radial_accel + tangential.1 + gravity.1,
                );
                p.dir.0 += acc.0 * dt;
                p.dir.1 += acc.1 * dt;
                p.pos.0 += p.dir.0 * dt;
                p.pos.1 += p.dir.1 * dt;
            } else {
                p.angle += p.degrees_per_second * dt;
                p.radius += p.delta_radius * dt;
                p.pos = (-p.angle.cos() * p.radius, -p.angle.sin() * p.radius);
            }
            for i in 0..4 {
                p.color[i] += p.delta_color[i] * dt;
            }
            p.size = (p.size + p.delta_size * dt).max(0.0);
            p.rotation += p.delta_rotation * dt;
            true
        });
    }

    /// How many particles are alive.
    pub fn live(&self) -> usize {
        self.particles.len()
    }

    /// Each live particle: world center, size, rotation (degrees), color (0..1 RGBA).
    pub fn particles(&self) -> impl Iterator<Item = ((f32, f32), f32, f32, [f32; 4])> + '_ {
        let (sin, cos) = (-self.rotation).to_radians().sin_cos();
        let (sx, sy) = self.scale;
        self.particles.iter().map(move |p| {
            let c = p.color.map(|v| v.clamp(0.0, 1.0));
            match self.position_type {
                PositionType::Relative => ((p.start.0 + p.pos.0, p.start.1 + p.pos.1), p.size, p.rotation, c),
                PositionType::Grouped => {
                    let (x, y) = (p.pos.0 * sx, p.pos.1 * sy);
                    let at = (self.position.0 + x * cos - y * sin, self.position.1 + x * sin + y * cos);
                    (at, p.size * sx.abs(), p.rotation + self.rotation, c)
                }
            }
        })
    }
}

/// `CCCircleWave`: a filled, additive disc whose radius grows from `r0` to `r1` over `duration`
/// while its opacity falls from 255 to 0. `CCCircleWave::create(r0, r1, t, fadeIn)` eases both
/// tweens with `CCEaseOut` rate 2 (√t); the drawn alpha is the opacity truncated to a byte.
#[derive(Clone, Debug)]
pub struct CircleWave {
    pub center: (f32, f32),
    pub r0: f32,
    pub r1: f32,
    pub duration: f32,
    pub color: [f32; 3],
    pub ease_out: bool,
    /// `fadeIn`: opacity rises 0 → 255 over the first half and falls over the second, and
    /// nothing is eased (the orb and portal circles).
    pub fade_in: bool,
    /// z in the object layer: the hit circles sit at 0 (under the player), the death circle at 99.
    pub z: i32,
    pub elapsed: f32,
    /// `m_lineWidth` set (+0x160): `ccDrawCircle` with a 2-point line instead of a filled disc.
    pub outline: bool,
    /// `m_opacityMod` (+0x168): the drawn alpha is opacity × this.
    pub opacity_mod: f32,
}

impl CircleWave {
    pub fn new(center: (f32, f32), r0: f32, r1: f32, duration: f32, color: [f32; 3]) -> CircleWave {
        CircleWave { center, r0, r1, duration, color, ease_out: true, fade_in: false, z: 0, elapsed: 0.0, outline: false, opacity_mod: 1.0 }
    }

    /// The `fadeIn` variant.
    pub fn fading_in(center: (f32, f32), r0: f32, r1: f32, duration: f32, color: [f32; 3]) -> CircleWave {
        CircleWave { fade_in: true, ease_out: false, ..CircleWave::new(center, r0, r1, duration, color) }
    }

    /// Current radius and alpha (0..1).
    pub fn state(&self) -> (f32, f32) {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let k = if self.ease_out { t.sqrt() } else { t };
        let opacity = if self.fade_in {
            if t < 0.5 { 255.0 * t * 2.0 } else { 255.0 - 255.0 * (t - 0.5) * 2.0 }
        } else {
            255.0 - 255.0 * k
        };
        (self.r0 + (self.r1 - self.r0) * k, ((opacity * self.opacity_mod).clamp(0.0, 255.0) as u8) as f32 / 255.0)
    }

    /// `CCCircleWave::draw`: segments for `ccDrawFilledCircle` by radius.
    pub fn segments(radius: f32) -> u32 {
        match radius {
            r if r < 10.0 => 10,
            r if r < 20.0 => 15,
            r if r < 40.0 => 20,
            r if r < 200.0 => 30,
            _ => 50,
        }
    }
}

/// One piece of a shattered icon (`ExplodeItemNode`'s sprites).
#[derive(Clone, Debug)]
pub struct Shard {
    /// Center, relative to the node (the death position).
    pub pos: (f32, f32),
    /// The piece of the captured icon: x, y (from the bottom), width, height, in points.
    pub rect: (f32, f32, f32, f32),
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub fade: f32,
    /// Degrees per 1/54 s, clockwise.
    pub spin: f32,
    pub rotation: f32,
}

/// `ExplodeItemNode`: the dead icon, captured to a `size` × `size` texture and broken into a grid
/// of pieces that fly off, fall, bounce on y = 90 and fade.
#[derive(Clone, Debug)]
pub struct Shatter {
    pub origin: (f32, f32),
    pub size: f32,
    /// The renderer sheet holding the capture (set once it's captured).
    pub sheet: Option<usize>,
    pub shards: Vec<Shard>,
}

impl Shatter {
    /// `ExplodeItemNode::createSprites(cols, rows, vx, vxVar, vy, vyVar, life, lifeVar)`: piece
    /// widths are round(size/cols · (0.55 + 0.9r)), the last taking the rest (heights alike).
    #[allow(clippy::too_many_arguments)]
    pub fn new(rng: &mut Rng, origin: (f32, f32), size: f32, cols: u32, rows: u32, vx: (f32, f32), vy: (f32, f32), life: (f32, f32)) -> Shatter {
        let cuts = |n: u32, rng: &mut Rng| {
            let mut sizes = Vec::new();
            let mut sum = 0.0;
            for _ in 1..n {
                let w = (size / n as f32 * (rng.unit() * 0.45 * 2.0 + 0.55)).round();
                sum += w;
                sizes.push(w);
            }
            sizes.push(size - sum);
            sizes
        };
        let widths = cuts(cols, rng);
        let heights = cuts(rows, rng);
        let mut shards = Vec::new();
        let mut x = 0.0;
        for &w in &widths {
            let mut y = 0.0;
            for &h in &heights {
                let vy = vy.0 + vy.1 * rng.minus1_1();
                let vx = vx.0 + vx.1 * rng.minus1_1();
                let life = life.0 + life.1 * rng.minus1_1();
                let spin = rng.minus1_1() * 360.0 / 60.0;
                shards.push(Shard {
                    pos: (x + w * 0.5 - size * 0.5, y + h * 0.5 - size * 0.5),
                    rect: (x, y, w, h),
                    vx,
                    vy,
                    life,
                    fade: life.min(0.5),
                    spin,
                    rotation: 0.0,
                });
                y += h;
            }
            x += w;
        }
        Shatter { origin, size, sheet: None, shards }
    }

    /// `ExplodeItemNode::update(dt)`.
    pub fn update(&mut self, dt: f32) {
        let f = dt * 54.0;
        let gravity = f * 0.5;
        let floor_y = 90.0 - self.origin.1;
        self.shards.retain_mut(|s| {
            s.life -= dt;
            if s.life <= 0.0 {
                return false;
            }
            s.vy -= gravity;
            s.vx *= (1.0 - f) * 0.02 + 0.98;
            s.pos.0 += f * s.vx;
            s.pos.1 += f * s.vy;
            let floor = floor_y + s.rect.2.min(s.rect.3) * 0.5;
            if s.pos.1 < floor && s.vy < 0.0 {
                s.vy = (s.vy * -0.8).max(1.5);
                s.pos.1 = floor;
            }
            s.rotation += f * s.spin;
            true
        });
    }

    /// A shard's opacity (it fades over its last `fade` seconds).
    pub fn opacity(s: &Shard) -> f32 {
        if s.life < s.fade { ((s.life / s.fade) * 255.0) as u8 as f32 / 255.0 } else { 1.0 }
    }
}

/// The particle system an object keeps while it's on screen (`GameObject::customSetup` picks
/// the effect file and colors; claimed from a pool when the object becomes visible). Values
/// recorded from the game's live emitters: `offset` is from the object's center (in its own
/// rotated space), `z` orders them in the object layer (0 under 4, both under the player).
#[derive(Clone, Copy, Debug)]
pub struct ObjectEffect {
    pub id: i32,
    pub file: &'static str,
    pub z: i32,
    pub offset: (f32, f32),
    pub start: [f32; 4],
    pub start_var: [f32; 4],
    pub end: [f32; 4],
}

#[rustfmt::skip]
pub const OBJECT_EFFECTS: &[ObjectEffect] = &[
    // the end portal (`EndPortalObject::init` 0x310cb4: endEffectPortal at (−15, 0), its colors
    // set to player color 1 by `updateColors`; the app sets them)
    ObjectEffect { id: END_PORTAL, file: "endEffectPortal.plist", z: 4, offset: (-15.0, 0.0), start: [0.4902, 1.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.4902, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 10, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.0000, 1.0000, 1.0000, 0.5000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 1.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 11, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 1.0000, 0.0000, 0.5000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 12, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.3000, 1.0000, 0.0000, 0.5000], start_var: [0.3000, 0.0000, 0.0000, 0.0000], end: [0.3000, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 13, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 0.0000, 1.0000, 0.5000], start_var: [0.0000, 0.3000, 0.0000, 0.0000], end: [1.0000, 0.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 35, file: "bumpEffect.plist", z: 0, offset: (0.0, 0.0), start: [1.0000, 1.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 36, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 0.7843, 0.1961, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [1.0000, 0.3922, 0.0980, 1.0000] },
    ObjectEffect { id: 45, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 0.5882, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.5882, 0.0000, 1.0000] },
    ObjectEffect { id: 46, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.0000, 1.0000, 1.0000, 0.5000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 1.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 47, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 0.3922, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.3922, 0.0000, 1.0000] },
    ObjectEffect { id: 67, file: "bumpEffect.plist", z: 0, offset: (0.0, 0.0), start: [0.0000, 1.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 1.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 84, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [0.0000, 1.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.0000, 0.4902, 1.0000, 1.0000] },
    ObjectEffect { id: 99, file: "portalEffect08.plist", z: 4, offset: (-3.0, 0.0), start: [0.0000, 1.0000, 0.0000, 1.0000], start_var: [0.2500, 0.0000, 0.2500, 0.5000], end: [0.0000, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 101, file: "portalEffect08.plist", z: 4, offset: (-3.0, 0.0), start: [1.0000, 0.0000, 1.0000, 1.0000], start_var: [0.0000, 0.5000, 0.0000, 0.5000], end: [1.0000, 0.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 111, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 0.7843, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.3922, 0.0000, 1.0000] },
    ObjectEffect { id: 140, file: "bumpEffect.plist", z: 0, offset: (0.0, 0.0), start: [1.0000, 0.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 141, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 0.3922, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [1.0000, 0.0000, 0.6863, 1.0000] },
    ObjectEffect { id: 286, file: "portalEffect08.plist", z: 4, offset: (-7.0, 0.0), start: [1.0000, 0.7843, 0.0000, 1.0000], start_var: [0.0000, 0.5000, 0.0000, 0.5000], end: [1.0000, 0.3922, 0.0000, 1.0000] },
    ObjectEffect { id: 287, file: "portalEffect08.plist", z: 4, offset: (-7.0, 0.0), start: [0.0000, 0.7843, 1.0000, 1.0000], start_var: [0.0000, 0.5000, 0.0000, 0.5000], end: [0.0000, 0.3922, 1.0000, 1.0000] },
    ObjectEffect { id: 660, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.0000, 0.7843, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 0.3922, 1.0000, 1.0000] },
    ObjectEffect { id: 745, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.5882, 0.5882, 0.5882, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.1961, 0.1961, 0.2941, 1.0000] },
    ObjectEffect { id: 747, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 0.7843, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.3922, 0.0000, 1.0000] },
    ObjectEffect { id: 1022, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [0.2941, 1.0000, 0.2941, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.0000, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1330, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 1.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1331, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.7843, 0.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.7843, 0.0000, 1.0000, 1.0000] },
    ObjectEffect { id: 1332, file: "bumpEffect.plist", z: 0, offset: (0.0, 0.0), start: [1.0000, 0.1961, 0.1961, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1333, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 0.3922, 0.0980, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [1.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1704, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [0.0000, 1.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1751, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 0.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.0000, 0.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 1933, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [1.0000, 1.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [1.0000, 0.7843, 0.0000, 1.0000] },
    ObjectEffect { id: 2926, file: "portalEffect01.plist", z: 4, offset: (-5.0, 0.0), start: [0.0000, 1.0000, 0.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.0000, 1.0000, 0.0000, 1.0000] },
    ObjectEffect { id: 3004, file: "ringEffect.plist", z: 4, offset: (0.0, 0.0), start: [1.0000, 0.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.6981], end: [0.3922, 0.0980, 1.0000, 1.0000] },
    ObjectEffect { id: 3005, file: "bumpEffect.plist", z: 0, offset: (0.0, 0.0), start: [1.0000, 0.0000, 1.0000, 1.0000], start_var: [0.0000, 0.0000, 0.0000, 0.0000], end: [0.3922, 0.0980, 1.0000, 1.0000] },
];

/// The id the end portal's effect goes by (not a real object id).
pub const END_PORTAL: i32 = -100;

/// A particle as the game pools it (`getParticleKey`): its file, layer and colors (the real
/// game's dumps show 40 systems for each portal color).
type ParticleKey = (&'static str, i32, [u32; 4]);

/// How many systems of one particle the game keeps (GJBaseGameLayer +0x2ff0, read from the
/// running game: 40 on every level).
pub const PARTICLE_LIMIT: usize = 40;

pub fn object_effect(id: i32) -> Option<&'static ObjectEffect> {
    OBJECT_EFFECTS.iter().find(|e| e.id == id)
}

/// A gravity-mode emitter from recorded values (`dragEffect`-style square particles).
#[allow(clippy::too_many_arguments)]
fn gravity_def(
    total: usize, duration: f32, life: (f32, f32), angle: (f32, f32), pos_var: (f32, f32), size: (f32, f32),
    start: [f32; 4], start_var: [f32; 4], end: [f32; 4], gravity: (f32, f32), speed: (f32, f32),
) -> ParticleDef {
    ParticleDef {
        texture: "square.png".into(),
        max_particles: total,
        duration,
        life: life.0,
        life_var: life.1,
        angle: angle.0,
        angle_var: angle.1,
        pos_var,
        start_size: size.0,
        start_size_var: size.1,
        start_color: start,
        start_color_var: start_var,
        end_color: end,
        gravity,
        speed: speed.0,
        speed_var: speed.1,
        additive: true,
        emission_rate: None,
        ..Default::default()
    }
}

/// The player's own particle systems (`PlayerObject::init`), with the settings the game gives
/// them (recorded from its live emitters): the ground drag, the ship's fire and click boost, the
/// ship's sparks along the ground, and the two alternating landing bursts.
pub struct PlayerFx {
    pub drag: Emitter,
    pub fire: Emitter,
    pub fire_boost: Emitter,
    pub sparks: Emitter,
    pub land: [Emitter; 2],
    next_land: usize,
    /// Seconds until the drag stops after leaving the ground (`CCDelayTime` 0.06, tag 3).
    drag_stop: Option<f32>,
}

impl PlayerFx {
    pub fn new(main: [f32; 3]) -> PlayerFx {
        let [r, g, b] = main;
        let fade = [0.1, 0.1, 0.1, 0.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let emitter = |def: ParticleDef, position_type, seed| {
            let mut e = Emitter::new(def, (0.0, 0.0), seed);
            e.position_type = position_type;
            e.stop();
            e
        };
        let drag = gravity_def(30, -1.0, (0.3, 0.15), (90.0, 45.0), (5.0, 1.0), (4.0, 3.0), [r, g, b, 1.0], fade, black, (0.0, -300.0), (75.0, 20.0));
        let fire = gravity_def(30, -1.0, (0.3, 0.15), (90.0, 45.0), (0.0, 2.0), (4.0, 3.0), [1.0, 0.3922, 0.0, 1.0], fade, [1.0, 0.0, 0.0, 1.0], (0.0, -300.0), (75.0, 20.0));
        let boost = ParticleDef { angle_var: 90.0, start_color: [1.0, 0.7451, 0.0, 1.0], ..fire.clone() };
        let sparks = gravity_def(40, -1.0, (0.3, 0.15), (110.0, 45.0), (9.0, 1.0), (3.0, 2.0), [1.0; 4], fade, black, (-350.0, -300.0), (94.624, 20.0));
        let land = gravity_def(10, 0.02, (0.0, 0.6), (90.0, 60.0), (10.0, 0.0), (5.0, 3.0), [r, g, b, 1.0], [0.1, 0.1, 0.1, 0.5], black, (0.0, -500.0), (150.0, 25.0));
        PlayerFx {
            drag: emitter(drag, PositionType::Relative, 11),
            fire: emitter(fire, PositionType::Relative, 12),
            fire_boost: emitter(boost, PositionType::Relative, 13),
            sparks: emitter(sparks, PositionType::Grouped, 14),
            land: [emitter(land.clone(), PositionType::Grouped, 15), emitter(land, PositionType::Grouped, 16)],
            next_land: 0,
            drag_stop: None,
        }
    }

    fn all_mut(&mut self) -> [&mut Emitter; 6] {
        let [l0, l1] = &mut self.land;
        [&mut self.drag, &mut self.fire, &mut self.fire_boost, &mut self.sparks, l0, l1]
    }

    /// Under the player (z 39): the drag and the ship's fire.
    pub fn below(&self) -> [&Emitter; 3] {
        [&self.drag, &self.fire, &self.fire_boost]
    }

    /// Over the player (z 61): the ship's sparks and the landing bursts.
    pub fn above(&self) -> [&Emitter; 3] {
        [&self.sparks, &self.land[0], &self.land[1]]
    }

    pub fn update(&mut self, dt: f32) {
        for e in self.all_mut() {
            e.update(dt);
        }
    }

    /// Stop everything and clear the particles (a new attempt).
    pub fn reset(&mut self) {
        for e in self.all_mut() {
            e.stop();
            e.clear();
        }
        self.drag_stop = None;
    }

    /// After a physics step: start and stop the systems (`PlayerObject::update`) and move them
    /// with the player (`setPosition`), and fire a landing burst (`hitGround`).
    pub fn step(&mut self, p: &cadence_core::physics::Player, dt: f32, landed: Option<Option<f32>>) {
        use cadence_core::physics::Mode;
        let flip = if p.upside_down { -1.0 } else { 1.0 };
        let size = p.size;
        let flying = p.mode.is_flying();
        // Ground drag: only cube and ball; it follows the player while touching the ground and
        // keeps going 0.06 s after leaving it.
        if !flying && p.touching_ground && !p.dead {
            self.drag.position = (p.x - 10.0 * size, p.y - 13.0 * size * flip);
            self.drag.def.angle = 90.0 * flip;
            self.drag.def.gravity.1 = -300.0 * flip;
            if !self.drag.active {
                self.drag.resume();
            }
            self.drag_stop = None;
        } else if self.drag.active {
            let left = self.drag_stop.get_or_insert(0.06);
            *left -= dt;
            if *left <= 0.0 || flying || p.dead {
                self.drag.stop();
                self.drag_stop = None;
            }
        }
        // Ship: the fire runs the whole time, the boost while the button is held; both come from
        // a point behind the ship, (−12·size, −9) in its rotated (and gravity-flipped) frame.
        let ship = p.mode == Mode::Ship && !p.dead;
        let (sin, cos) = (-p.rotation).to_radians().sin_cos();
        let (lx, ly) = (-12.0 * size, -9.0 * flip);
        let nozzle = (p.x + lx * cos - ly * sin, p.y + lx * sin + ly * cos);
        for (e, on) in [(&mut self.fire, ship), (&mut self.fire_boost, ship && p.holding)] {
            e.position = nozzle;
            e.def.angle = 90.0 * flip;
            e.def.gravity.1 = -300.0 * flip;
            if on && !e.active {
                e.resume();
            } else if !on && e.active {
                e.stop();
            }
        }
        // Ship sparks: sliding along the floor (or ceiling) without moving away from it.
        let toward = if p.upside_down { p.y_velocity < 1.0 } else { p.y_velocity > -1.0 };
        let sparks = ship && p.touching_ground && toward;
        self.sparks.position = (p.x + size, p.y - 15.0 * size * flip);
        self.sparks.def.angle = if p.upside_down { 290.0 } else { 110.0 };
        self.sparks.def.gravity.1 = -300.0 * flip;
        if sparks && !self.sparks.active {
            self.sparks.resume();
        } else if !sparks && self.sparks.active {
            self.sparks.stop();
        }
        // Landing: the two bursts alternate; one still showing 3 or more particles is skipped.
        if let Some(slope) = landed {
            let e = &mut self.land[self.next_land];
            self.next_land ^= 1;
            if e.count() < 3 {
                e.def.angle = 90.0 * flip;
                e.def.gravity.1 = -500.0 * flip;
                e.position = (p.x, p.y - 15.0 * size * flip);
                e.rotation = slope.unwrap_or(0.0);
                e.reset();
            }
        }
    }
}

/// Every running effect in a level.
#[derive(Default)]
pub struct Effects {
    pub emitters: Vec<Emitter>,
    /// Object effects by level object index, with their `ObjectEffect::z`.
    pub objects: HashMap<usize, (i32, Emitter)>,
    /// Which particle (file, layer) each object's emitter is, and the objects in view that found
    /// all of theirs taken (`sync_objects`).
    claims: HashMap<usize, ParticleKey>,
    denied: std::collections::HashSet<usize>,
    /// Custom particle objects' emitters (2065) by level index, with the object's z layer.
    pub custom: HashMap<usize, (i32, Emitter)>,
    /// Screenshots: seconds to run each new object emitter for when it appears (else they start
    /// empty, as in the game).
    pub prewarm: f32,
    pub waves: Vec<CircleWave>,
    pub shatters: Vec<Shatter>,
    pub rng: Rng,
    seed: u64,
    /// The glitter (`GJBaseGameLayer::m_glitterParticles`): squares over the whole view in player
    /// color 1 while flying (ship, UFO, wave, swing), see `glitter`.
    pub glitter: Option<Emitter>,
}

/// The glitter's settings, from a dump of the real system: 30 particles, life 1 ± 0.8, speed
/// 0 ± 20, size 6 ± 2 shrinking to nothing, spread over the view (width / 1.8, height / 2),
/// player color 1 (alpha ± 0.15) fading to black, additive, 30 a second.
pub fn glitter_def(color: [f32; 3], view: (f32, f32)) -> ParticleDef {
    ParticleDef {
        texture: "square.png".into(),
        max_particles: 30,
        duration: -1.0,
        life: 1.0,
        life_var: 0.8,
        angle: 270.0,
        speed_var: 20.0,
        pos_var: (view.0 / 1.8, view.1 * 0.5),
        start_size: 6.0,
        start_size_var: 2.0,
        start_color: [color[0], color[1], color[2], 1.0],
        start_color_var: [0.0, 0.0, 0.0, 0.15],
        end_color: [0.0, 0.0, 0.0, 1.0],
        additive: true,
        emission_rate: Some(30.0),
        ..Default::default()
    }
}

impl Effects {
    /// Start a one-shot effect that removes itself when done.
    pub fn spawn(&mut self, def: &ParticleDef, position: (f32, f32), position_type: PositionType, start_color: Option<[f32; 4]>) {
        self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
        let mut def = def.clone();
        if let Some(c) = start_color {
            def.start_color = c;
        }
        let mut e = Emitter::new(def, position, self.seed);
        e.position_type = position_type;
        e.auto_remove = true;
        self.emitters.push(e);
    }

    /// A one-shot system with the node's rotation and scale (spawn particle triggers' copies).
    pub fn spawn_node(&mut self, def: &ParticleDef, position: (f32, f32), position_type: PositionType, rotation: f32, scale: (f32, f32)) {
        self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
        let mut e = Emitter::new(def.clone(), position, self.seed);
        e.position_type = position_type;
        e.rotation = rotation;
        e.scale = scale;
        e.auto_remove = true;
        self.emitters.push(e);
    }

    /// Advance by one frame (cocos2d updates particles once per frame with the frame's delta).
    /// Keep an emitter for each visible object that has one: `visible` gives (level index,
    /// object id, center, rotation, scale). Objects that left the view lose theirs.
    pub fn sync_objects(&mut self, library: &ParticleLibrary, visible: &[(usize, i32, (f32, f32), f32, (f32, f32))]) {
        // (a set: a linear search per emitter was quadratic in the objects on screen)
        let shown: crate::fxhash::FxSet<usize> = visible.iter().filter(|v| object_effect(v.1).is_some()).map(|v| v.0).collect();
        self.objects.retain(|index, _| shown.contains(index));
        self.denied.retain(|index| shown.contains(index));
        // The game keeps at most PARTICLE_LIMIT systems per particle (file, layer and colors:
        // `createParticle` checks GJBaseGameLayer +0x2ff0, 40); an object claims one as it
        // comes into view and gets none while all are taken (until it leaves and comes back).
        let mut in_use: crate::fxhash::FxMap<ParticleKey, usize> = Default::default();
        for (index, _) in self.objects.iter() {
            if let Some(&key) = self.claims.get(index) {
                *in_use.entry(key).or_default() += 1;
            }
        }
        self.claims.retain(|index, _| shown.contains(index));
        // new objects claim in the order they come into view: left to right
        let mut order: Vec<&(usize, i32, (f32, f32), f32, (f32, f32))> = visible.iter().collect();
        order.sort_by(|a, b| a.2 .0.total_cmp(&b.2 .0).then(a.0.cmp(&b.0)));
        for &&(index, id, center, rotation, scale) in &order {
            let Some(fx) = object_effect(id) else { continue };
            if self.denied.contains(&index) {
                continue;
            }
            let (sin, cos) = (-rotation).to_radians().sin_cos();
            let (ox, oy) = (fx.offset.0 * scale.0, fx.offset.1 * scale.1);
            let position = (center.0 + ox * cos - oy * sin, center.1 + ox * sin + oy * cos);
            if let Some((_, e)) = self.objects.get_mut(&index) {
                e.position = position;
                e.rotation = rotation;
                e.scale = scale;
                continue;
            }
            let key: ParticleKey = (fx.file, fx.z, fx.start.map(f32::to_bits));
            let used = in_use.entry(key).or_default();
            if *used >= PARTICLE_LIMIT {
                self.denied.insert(index);
                continue;
            }
            let Some(base) = library.get(fx.file) else { continue };
            *used += 1;
            self.claims.insert(index, key);
            let mut def = base.clone();
            def.start_color = fx.start;
            def.start_color_var = fx.start_var;
            def.end_color = fx.end;
            def.end_color_var = [0.0; 4];
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut e = Emitter::new(def, position, self.seed ^ index as u64);
            e.position_type = PositionType::Grouped;
            e.rotation = rotation;
            e.scale = scale;
            for _ in 0..(self.prewarm * 60.0).round() as u32 {
                e.update(1.0 / 60.0);
            }
            self.objects.insert(index, (fx.z, e));
        }
    }

    /// Keep an emitter for each visible custom particle object (2065): `visible` gives (level
    /// index, center, rotation, scale, z layer, particle string).
    pub fn sync_custom(&mut self, visible: &[(usize, (f32, f32), f32, (f32, f32), i32, &str)]) {
        let shown: std::collections::HashSet<usize> = visible.iter().map(|v| v.0).collect();
        self.custom.retain(|index, _| shown.contains(index));
        for &(index, center, rotation, scale, z, text) in visible {
            if let Some((_, e)) = self.custom.get_mut(&index) {
                e.position = center;
                e.rotation = rotation;
                e.scale = scale;
                continue;
            }
            let Some((def, grouped)) = ParticleDef::from_gd_string(text) else { continue };
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut e = Emitter::new(def, center, self.seed ^ index as u64);
            e.position_type = if grouped { PositionType::Grouped } else { PositionType::Relative };
            e.rotation = rotation;
            e.scale = scale;
            for _ in 0..(self.prewarm * 60.0).round() as u32 {
                e.update(1.0 / 60.0);
            }
            self.custom.insert(index, (z, e));
        }
    }

    /// Keep the glitter centered on the view and running while `on` (`PlayLayer::toggleGlitter`
    /// 0x7d860 resumes or stops it; `updateEffectPositions` 0x7e6a8 centers it).
    pub fn sync_glitter(&mut self, on: bool, center: (f32, f32), color: [f32; 3], view: (f32, f32)) {
        if self.glitter.is_none() {
            if !on {
                return;
            }
            self.seed = self.seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut e = Emitter::new(glitter_def(color, view), center, self.seed);
            e.position_type = PositionType::Relative;
            for _ in 0..(self.prewarm * 60.0).round() as u32 {
                e.update(1.0 / 60.0);
            }
            self.glitter = Some(e);
        }
        if let Some(e) = &mut self.glitter {
            e.position = center;
            e.def.pos_var = (view.0 / 1.8, view.1 * 0.5);
            e.active = on;
        }
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(e) = &mut self.glitter {
            e.update(dt);
        }
        for (_, e) in self.custom.values_mut() {
            e.update(dt);
        }
        for e in &mut self.emitters {
            e.update(dt);
        }
        for (_, e) in self.objects.values_mut() {
            e.update(dt);
        }
        self.emitters.retain(|e| !(e.auto_remove && e.finished()));
        for w in &mut self.waves {
            w.elapsed += dt;
        }
        self.waves.retain(|w| w.elapsed < w.duration);
        for sh in &mut self.shatters {
            sh.update(dt);
        }
        self.shatters.retain(|sh| !sh.shards.is_empty());
    }

    pub fn clear(&mut self) {
        self.glitter = None;
        self.custom.clear();
        self.emitters.clear();
        self.objects.clear();
        self.waves.clear();
        self.shatters.clear();
    }
}

/// Effect files, parsed once.
#[derive(Default)]
pub struct ParticleLibrary {
    defs: HashMap<String, ParticleDef>,
}

impl ParticleLibrary {
    /// The effect files the game uses that Cadence draws.
    pub const FILES: &'static [&'static str] = &[
        "explodeEffect.plist",
        "dragEffect.plist",
        "landEffect.plist",
        "ringEffect.plist",
        "bumpEffect.plist",
        "shipDragEffect.plist",
        "burstEffect.plist",
        "portalEffect01.plist",
        "portalEffect02.plist",
        "portalEffect03.plist",
        "portalEffect04.plist",
        "portalEffect08.plist",
        "speedEffect_slow.plist",
        "speedEffect_normal.plist",
        "speedEffect_fast.plist",
        "speedEffect_vfast.plist",
        "speedEffect_vvfast.plist",
        "endEffectPortal.plist",
        "dashEffect.plist",
        "burstEffect2.plist",
        "swingBurstEffect.plist",
    ];

    pub fn load(files: &dyn Files) -> ParticleLibrary {
        let mut defs = HashMap::new();
        for name in Self::FILES {
            if let Some(def) = files.read(name).and_then(|b| ParticleDef::parse(&b)) {
                defs.insert(name.to_string(), def);
            }
        }
        ParticleLibrary { defs }
    }

    pub fn get(&self, name: &str) -> Option<&ParticleDef> {
        self.defs.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_emits_everything_at_once() {
        let def = ParticleDef { max_particles: 100, duration: 0.05, life: 0.0, life_var: 0.8, speed: 250.0, ..Default::default() };
        let mut e = Emitter::new(def, (0.0, 0.0), 1);
        e.update(1.0 / 60.0);
        assert!(!e.particles.is_empty() && e.particles.len() <= 100);
        for _ in 0..120 {
            e.update(1.0 / 60.0);
        }
        assert!(e.finished());
    }
}

#[cfg(test)]
mod gd_string_tests {
    use super::*;

    #[test]
    fn dash_custom_particle_string() {
        // A Dash (22) custom particle object's key 145.
        let s = "8a-1a1a0.3a6a89a24a79a10a11a0a0a-199a0a0a0a0a3a1a0a0a1a0a0.862745a0a0a0a1a0a2a1a0a0a1a0a0a0a0a0a1a0a0a0a0a0a0a0a0a0a0a0a0a2a1a0a0a0a0a0a0a0a0a0";
        let (def, grouped) = ParticleDef::from_gd_string(s).unwrap();
        let mut e = Emitter::new(def.clone(), (0.0, 0.0), 1);
        for _ in 0..60 {
            e.update(1.0 / 60.0);
        }
        // 8 particles at 6 a second, size 3 (atoi), falling (gravity −199), orange to red.
        assert_eq!((def.texture.as_str(), def.max_particles, def.emission_rate, def.start_size, def.gravity), ("particle_00_001.png", 8, Some(6.0), 3.0, (0.0, -199.0)));
        assert!(grouped);
        assert_eq!(e.particles().count(), 5);
    }
}

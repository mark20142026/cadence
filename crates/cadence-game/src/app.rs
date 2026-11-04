//! The game app shared by the native and web builds: plays an official level from the player's
//! own game files with the verified physics, drawn with wgpu.

use std::path::PathBuf;
use std::sync::Arc;

use web_time::Instant;

use cadence_core::camera::{Camera, VIEW_HEIGHT};
use cadence_core::codec::decode_level_bytes;
use cadence_core::level::{LevelString, RawObject};
use cadence_core::objects::{self, SpriteRole};
use cadence_core::objects::ObjectType;
use cadence_core::physics::{Event, Mode, Player, STEPS_PER_SECOND};
use cadence_core::triggers::{Colors, Hsv};
use cadence_core::world::{GROUND_OFFSET, World};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::assets::{Quality, Sheets};
use crate::audio::{Audio, NoAudio};
use crate::files::Files;
use crate::particles::{CircleWave, Effects, Emitter, ParticleLibrary, PlayerFx, PositionType, Shatter};
use crate::render::{Blend, Draw, Instance, Renderer, particle_instance, quad_instance, sprite_instance, triangle_instance, tint};

pub const SHEETS: &[&str] = &["GJ_GameSheet", "GJ_GameSheet02", "GJ_GameSheet03", "GJ_GameSheet04", "GJ_GameSheetGlow", "GJ_ParticleSheet", "PixelSheet_01", "FireSheet_01"];
pub const ICON_SHEETS: &[&str] = &["player_01", "ship_01", "player_ball_01", "bird_01", "dart_01", "robot_01", "spider_01", "swing_01"];
pub const SINGLE_IMAGES: &[&str] = &["game_bg_01_001", "groundSquare_01_001"];
/// The particle texture (white; the game has it in one resolution only).
pub const PARTICLE_TEXTURE: &str = "square.png";
/// The regular trail's texture (trail 1), shipped in one resolution.
pub const STREAK_TEXTURE: &str = "streak_01_001.png";
/// The default icon colors: main (green) and secondary (cyan).


/// 2D affine transform [a, b, c, d, tx, ty] (column-major 2x3).
type Affine = [f32; 6];

fn mul(p: Affine, q: Affine) -> Affine {
    [
        p[0] * q[0] + p[2] * q[1],
        p[1] * q[0] + p[3] * q[1],
        p[0] * q[2] + p[2] * q[3],
        p[1] * q[2] + p[3] * q[3],
        p[0] * q[4] + p[2] * q[5] + p[4],
        p[1] * q[4] + p[3] * q[5] + p[5],
    ]
}

/// cocos2d `nodeToParentTransform`: translate(pos) · rotate(-rotation°) · scale · translate(-anchor·size).
fn node_transform(x: f32, y: f32, rotation: f32, sx: f32, sy: f32, ax: f32, ay: f32, w: f32, h: f32) -> Affine {
    let r = -rotation.to_radians();
    let (sin, cos) = r.sin_cos();
    let (ox, oy) = (-ax * w, -ay * h);
    let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    [a, b, c, d, a * ox + c * oy + x, b * ox + d * oy + y]
}

/// The game's camera in basic modes (`GJBaseGameLayer::updateCamera`), updated after each
/// physics step from the player's new position. Verified against the real camera on a full
/// Stereo Madness route: x exact, y within 0.00002 units. (Traces record the camera before that
/// step's camera update, so they show it one step behind.)
/// - x: holds at 15, then trails the player by `view_width/2 - 75`.
/// - y: dead zone around the view center (player more than 70 above or 40 below moves the
///   target), constant easing 10 with dt 0.25 per step, stored as f32, never below 0.
/// - Flying corridor: the corridor's ground slide-in and static y (`cadence_core::camera`).
/// The player's icon sprites at `pos`, rotated `rotation` degrees: each sprite centered on its own
/// frame size (cocos2d content size). Mini scales the whole icon (eased like the game's scale
/// animation); upside down it's mirrored vertically.
fn player_draws(
    sheets: &Sheets,
    atlas: &dyn Fn(usize) -> (f32, f32),
    p: &Player,
    pos: (f32, f32),
    rotation: f32,
    colors: ([f32; 3], [f32; 3]),
    icons: &crate::ui::garage::IconSet,
    rig: Option<&[crate::anim::Part]>,
    alpha: f32,
    draws: &mut Vec<Draw>,
) {
    let frame_size = |name: &str| sheets.frames.get(name).map(|f| f.source_size);
    let mut push = |name: &str, transform: Affine, color: [f32; 3]| {
        if let Some(frame) = sheets.frames.get(name) {
            let instance = sprite_instance(frame, atlas(frame.sheet), transform, false, false, tint(color, alpha));
            draws.push(Draw { sheet: frame.sheet, blend: Blend::Normal, instance });
        }
    };
    let centered = |name: &str, parent: Affine, x: f32, y: f32, scale: f32| -> Affine {
        let (w, h) = frame_size(name).unwrap_or((30.0, 30.0));
        mul(parent, node_transform(x, y, 0.0, scale, scale, 0.5, 0.5, w, h))
    };
    let flip = if p.upside_down { -1.0 } else { 1.0 };
    let body = node_transform(pos.0, pos.1, rotation, p.scale, p.scale * flip, 0.0, 0.0, 0.0, 0.0);
    let (c1, c2) = colors;
    // The robot: its animation frame's parts, far limbs at 0.7 of the colors (truncated to bytes
    // like the game's ccColor3B).
    if let (Mode::Robot | Mode::Spider, Some(parts)) = (p.mode, rig) {
        let dark = |c: [f32; 3]| c.map(|v| ((v * 255.0).round() * 0.7).trunc() / 255.0);
        for part in parts {
            let (a, b) = if part.back { (dark(c1), dark(c2)) } else { (c1, c2) };
            let sx = part.scale.0 * if part.flip.0 { -1.0 } else { 1.0 };
            let sy = part.scale.1 * if part.flip.1 { -1.0 } else { 1.0 };
            // the rig names icon 1's parts; the chosen robot / spider has the same parts
            let kind = if p.mode == Mode::Robot { 5 } else { 6 };
            let texture = part.texture.replacen(&crate::ui::garage::icon_sheet(kind, 1), &icons.sheet(kind), 1);
            for (name, color) in [(format!("{texture}_2_001.png"), b), (format!("{texture}_001.png"), a)] {
                if let Some((w, h)) = frame_size(&name) {
                    let t = mul(body, node_transform(part.pos.0, part.pos.1, part.rotation, sx, sy, 0.5, 0.5, w, h));
                    push(&name, t, color);
                }
            }
        }
        return;
    }
    // (sprite, x, y, scale): the secondary-color layer first, then the main color.
    let n = |k: usize| icons.sheet(k);
    let layers: Vec<(String, f32, f32, f32)> = match p.mode {
        Mode::Cube => vec![(n(0), 0.0, 0.0, 1.0)],
        Mode::Ball => vec![(n(2), 0.0, 0.0, 1.0)],
        // the cube rides in the UFO/ship at 0.55 scale, (0, +5); the vehicle sits at (0, −5)
        Mode::Ufo => vec![(n(0), 0.0, 5.0, 0.55), (n(3), 0.0, -5.0, 1.0)],
        Mode::Ship => vec![(n(0), 0.0, 5.0, 0.55), (n(1), 0.0, -5.0, 1.0)],
        Mode::Wave => vec![(n(4), 0.0, 0.0, 1.0)],
        // (without its animation rig: the body part)
        Mode::Robot => vec![(format!("{}_01", n(5)), 0.0, 0.0, 1.0)],
        Mode::Spider => vec![(format!("{}_01", n(6)), 0.0, 0.0, 1.0)],
        // a single centered sprite, like the wave
        Mode::Swing => vec![(n(7), 0.0, 0.0, 1.0)],
    };
    for (base, x, y, scale) in &layers {
        let (x, y, scale) = (*x, *y, *scale);
        let second = format!("{base}_2_001.png");
        let dome = format!("{base}_3_001.png");
        let main = format!("{base}_001.png");
        let extra = format!("{base}_extra_001.png");
        push(&second, centered(&second, body, x, y, scale), c2);
        push(&dome, centered(&dome, body, x, y, scale), [1.0; 3]);
        push(&main, centered(&main, body, x, y, scale), c1);
        push(&extra, centered(&extra, body, x, y, scale), [1.0; 3]);
    }
}

/// `GJBaseGameLayer::playExitDualEffect` (0xe5200): a copy of the player that didn't die
/// (a `SimplePlayer` of its icon, colors and rotation) fades out over 0.4 s while it moves 124.63
/// screen units back, shrinks to 0.1 and turns 180° (all linear, starting a frame late like every
/// cocos2d action), with a 10 → 30 ring in its main color following it. The copy sits on the
/// unmirrored screen layer (the game flips its x scale and rotation when the view is mirrored);
/// drawn in level space it keeps the player's own.
/// The exit-dual ring's z (it's added to the layer at z 0 over the copy; any z under the T
/// layers that no death wave uses marks it).
const EXIT_DUAL_WAVE_Z: i32 = 100;

struct ExitDual {
    player: Player,
    colors: ([f32; 3], [f32; 3]),
    from: (f32, f32),
    dx: f32,
    turn: f32,
    rotation: f32,
    scale: f32,
    end_scale: f32,
    elapsed: Option<f32>,
}

impl ExitDual {
    const DURATION: f32 = 0.4;

    fn new(p: &Player, colors: ([f32; 3], [f32; 3]), zoom: f32, mirrored: bool) -> ExitDual {
        let mut player = p.clone();
        player.dual = None;
        ExitDual {
            colors,
            from: (p.x, p.y),
            dx: -124.632034 / zoom,
            turn: if mirrored { -180.0 } else { 180.0 },
            rotation: p.rotation,
            scale: p.scale,
            end_scale: 0.1 / zoom,
            elapsed: None,
            player,
        }
    }

    fn step(&mut self, dt: f32) {
        self.elapsed = Some(self.elapsed.map_or(0.0, |t| t + dt));
    }

    fn progress(&self) -> f32 {
        (self.elapsed.unwrap_or(0.0) / Self::DURATION).min(1.0)
    }

    /// The copy as drawn now (position, rotation, scale) and its opacity.
    fn state(&self) -> (Player, f32) {
        let k = self.progress();
        let mut p = self.player.clone();
        p.x = self.from.0 + self.dx * k;
        p.y = self.from.1;
        p.rotation = self.rotation + self.turn * k;
        p.scale = self.scale + (self.end_scale - self.scale) * k;
        (p, 1.0 - k)
    }
}

/// An emitter's particles as quads on the particle texture (`sheet`). The game's effects all
/// blend SRC_ALPHA, ONE (additive); others would be premultiplied for the normal blend.
fn push_particles(e: &Emitter, sheet: usize, draws: &mut Vec<Draw>) {
    for (pos, size, rot, c) in e.particles() {
        let (color, blend) = if e.def.additive { (c, Blend::Additive) } else { (tint([c[0], c[1], c[2]], c[3]), Blend::Normal) };
        draws.push(Draw { sheet, blend, instance: particle_instance(pos, size, rot, color) });
    }
}

/// A circle wave as an additive triangle fan (`ccDrawFilledCircle`), on the particle texture's
/// white center. `pixel` is one framebuffer pixel in level units (glLineWidth counts pixels).
fn push_wave(w: &CircleWave, sheet: usize, pixel: f32, draws: &mut Vec<Draw>) {
    let (radius, alpha) = w.state();
    let n = CircleWave::segments(radius);
    let color = [w.color[0], w.color[1], w.color[2], alpha];
    let at = |i: u32| {
        let a = i as f32 * std::f32::consts::TAU / n as f32;
        (w.center.0 + radius * a.cos(), w.center.1 + radius * a.sin())
    };
    if w.outline {
        // ccDrawCircle at glLineWidth w: a ring w/2 pixels either side of the radius (the real
        // game's 10-pixel death rings are ~4.4 units wide at 1280 x 720)
        let ring = |i: u32, r: f32| {
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            (w.center.0 + r * a.cos(), w.center.1 + r * a.sin())
        };
        let half = w.line_width * 0.5 * pixel;
        let (r0, r1) = ((radius - half).max(0.0), radius + half);
        for i in 0..n {
            for (a, b, c) in [(ring(i, r0), ring(i, r1), ring(i + 1, r1)), (ring(i, r0), ring(i + 1, r1), ring(i + 1, r0))] {
                let instance = triangle_instance(a, b, c, (0.5, 0.5), color);
                draws.push(Draw { sheet, blend: Blend::Additive, instance });
            }
        }
        return;
    }
    for i in 0..n {
        let instance = triangle_instance(w.center, at(i), at(i + 1), (0.5, 0.5), color);
        draws.push(Draw { sheet, blend: Blend::Additive, instance });
    }
}

/// The spider's teleport streak (`playSpiderDashEffect`): `spiderDash_001..008` at 0.04 s a
/// frame, additive, stretched between the two ends.
#[derive(Clone, Debug)]
struct SpiderDash {
    center: (f32, f32),
    rotation: f32,
    /// The streak's length (|Δ| + 30); the x scale is this over the frame's width.
    length: f32,
    color: [f32; 3],
    elapsed: f32,
}

/// `CCEaseElasticOut` (period `p`) at `t` in 0..1.
fn elastic_out(t: f32, p: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let s = p / 4.0;
    2f32.powf(-10.0 * t) * ((t - s) * std::f32::consts::TAU / p).sin() + 1.0
}

/// `GJBaseGameLayer::positionUIObjects`: each UI trigger (3613) pins its target group (51) to
/// the screen, relative to its reference group's main object (71). x: 385 2 center, 3 left,
/// 4 right (≤ 1: left or right by which side of the reference); y: 386 6 center, 7 bottom,
/// 8 top (0 or 5: by side). 387/388 scale the offset with the screen ((w/2)/240, (h/2)/160).
#[derive(Clone, Copy, Debug)]
struct UiPin {
    /// x anchor (2 center, 3 left, 4 right) and y anchor (6 center, 7 bottom, 8 top)
    x_ref: i32,
    y_ref: i32,
    /// the object's offset from the reference object, and whether it scales with the screen
    dx: f32,
    dy: f32,
    x_rel: bool,
    y_rel: bool,
}

impl UiPin {
    /// The screen point (points, from the bottom left) on a `w` × `h` screen.
    fn point(&self, w: f32, h: f32) -> (f32, f32) {
        let (fx, fy) = ((w * 0.5) / 240.0, (h * 0.5) / 160.0);
        let sx = |d: f32| if self.x_rel { fx * d } else { d };
        let sy = |d: f32| if self.y_rel { fy * d } else { d };
        let x = match self.x_ref {
            4 => w + sx(self.dx - 240.0),
            3 => sx(self.dx + 240.0),
            _ => w * 0.5 + sx(self.dx),
        };
        let y = match self.y_ref {
            8 => h + sy(self.dy - 160.0),
            7 => sy(self.dy + 160.0),
            _ => h * 0.5 + sy(self.dy),
        };
        (x, y)
    }
}

fn ui_objects(objects: &[RawObject]) -> std::collections::HashMap<usize, UiPin> {
    let mut out = std::collections::HashMap::new();
    let main_of = |g: i32| objects.iter().position(|o| o.groups().contains(&g));
    let pos = |o: &RawObject| (o.x(), o.y() + GROUND_OFFSET);
    for t in objects.iter().filter(|o| o.id() == 3613) {
        let (Some(target), Some(reference)) = (t.get_i32(51), t.get_i32(71)) else { continue };
        let Some(r) = main_of(reference) else { continue };
        let (rx, ry) = pos(&objects[r]);
        let (x_rel, y_rel) = (t.get(387) == Some("1"), t.get(388) == Some("1"));
        for (i, o) in objects.iter().enumerate().filter(|(_, o)| o.groups().contains(&target)) {
            if out.contains_key(&i) {
                continue;
            }
            let (ox, oy) = pos(o);
            let xr = match t.get_i32(385).unwrap_or(0) {
                r if r > 1 => r,
                _ if rx <= ox => 4,
                _ => 3,
            };
            let yr = match t.get_i32(386).unwrap_or(0) {
                0 | 5 if ry <= oy => 8,
                0 | 5 => 7,
                r => r,
            };
            out.insert(i, UiPin { x_ref: xr, y_ref: yr, dx: ox - rx, dy: oy - ry, x_rel, y_rel });
        }
    }
    out
}

struct LevelVisual {
    /// (z layer, z order, level index) for sorting, and the raw object.
    order: Vec<(i32, i32, usize)>,
    objects: Vec<RawObject>,
    /// Groups of the objects that pulse and alpha triggers target (by level index).
    pulse_groups: crate::fxhash::FxMap<usize, Vec<i32>>,
    /// Objects a UI trigger (3613) pins to the screen (`positionUIObjects`).
    ui_objects: std::collections::HashMap<usize, UiPin>,
    /// Each object's own HSV shift of its main and detail colors (keys 41/43, 42/44).
    hsv: Vec<(Option<Hsv>, Option<Hsv>)>,
    /// Custom particle objects (2065): level index and particle string (key 145).
    particles: Vec<(usize, String)>,
    /// Each object's spin in degrees per second (0: none), see `spin_rate`.
    spin: Vec<f32>,

    /// Each object's fields the frame needs, parsed once at load (not from text every frame).
    statics: Vec<ObjStatic>,
    /// Culling: positions in `order` by x column (COLUMN wide, from `column0`), and the
    /// positions of objects that can move (in a group, or pinned by a UI trigger), checked
    /// every frame.
    columns: Vec<Vec<u32>>,
    column0: i32,
    movable: Vec<u32>,
    /// Objects triggers have moved: their place in `order` by their current x column (they
    /// are skipped in `columns`), each object's flag and column, and the world epoch this is
    /// for (a reset or checkpoint replaces the world: start over).
    moved_columns: crate::fxhash::FxMap<i32, Vec<u32>>,
    displaced: Vec<bool>,
    moved_column: Vec<i32>,
    moved_epoch: u64,
    /// Each object's place in `order`.
    place: Vec<u32>,
}

impl LevelVisual {
    /// Bring the moved objects' columns up to date with the world's move log.
    fn track_moves(&mut self, world: &mut cadence_core::world::World) {
        let col = |x: f32| (x / COLUMN).floor() as i32;
        if world.epoch != self.moved_epoch {
            self.moved_epoch = world.epoch;
            self.moved_columns.clear();
            self.displaced.iter_mut().for_each(|d| *d = false);
            world.moved_log.clear();
            for i in 0..self.statics.len() {
                if world.moved[i] != (0.0, 0.0) {
                    self.place_moved(i, col(self.statics[i].x + world.moved[i].0 as f32));
                }
            }
            return;
        }
        let log = std::mem::take(&mut world.moved_log);
        for &i in &log {
            let i = i as usize;
            let c = col(self.statics[i].x + world.moved[i].0 as f32);
            if !self.displaced[i] {
                self.place_moved(i, c);
            } else if self.moved_column[i] != c {
                let k = self.place[i];
                if let Some(v) = self.moved_columns.get_mut(&self.moved_column[i]) {
                    if let Some(at) = v.iter().position(|&x| x == k) {
                        v.swap_remove(at);
                    }
                }
                self.moved_columns.entry(c).or_default().push(k);
                self.moved_column[i] = c;
            }
        }
        let mut log = log;
        log.clear();
        world.moved_log = log;
    }

    fn place_moved(&mut self, i: usize, c: i32) {
        self.displaced[i] = true;
        self.moved_column[i] = c;
        self.moved_columns.entry(c).or_default().push(self.place[i]);
    }
}

/// The culling columns' width.
const COLUMN: f32 = 64.0;

/// How far above and below the view object effects (particle emitters) still run: a section
/// (100 units) and a margin for the particles' spread.
const EFFECT_MARGIN_Y: f32 = 200.0;

/// An object type's sprite tree resolved for drawing: each sprite's frame, the order to draw
/// them in (children with negative z under the parent, the rest over, stable) and which are
/// portal backs.
/// Channel ids the cache tracks (custom 1–999 and the specials up to 1014).
const CACHE_CHANNELS: usize = 1100;

/// Static objects' finished sprites, kept from the frame they were built and replayed while
/// nothing they depend on has changed. Sprite instances are in world space (the camera is a
/// uniform), so an object that isn't moved, turned, scaled, animated, pulsing, spinning, faded or
/// entering produces the same instances every frame until one of its color channels changes.
/// Channels are tracked by a version each, bumped when a frame finds the channel's color, opacity
/// or blending different from the last frame's. Off with `nocache` (CADENCE_NOCACHE).
#[derive(Default)]
struct DrawCache {
    versions: Vec<u32>,
    /// Last frame's (color, opacity bits, blending) per channel id, `None` when not stored.
    states: Vec<Option<([u8; 3], u32, bool)>>,
    scratch: Vec<Option<([u8; 3], u32, bool)>>,
    /// By object index.
    entries: Vec<Option<Box<CachedObject>>>,
    /// The indices with an entry (to drop the ones not drawn for a while).
    live: Vec<usize>,
    frame: u32,
}

struct CachedObject {
    key: [u32; 4],
    /// The last frame it was drawn in.
    frame: u32,
    /// Its sprites, each with the list it went to: 0 the frame's draws (glows), 1 portal backs,
    /// 2 the layer's normal, 3 blending, 4 moved-up parts, 5 the B pass.
    draws: Vec<(u8, Draw)>,
}

impl DrawCache {
    /// Once a frame before objects are drawn: bump the versions of channels that changed, and
    /// every 64 frames drop the entries of objects not drawn in the last 64.
    fn sync(&mut self, colors: &cadence_core::triggers::Colors, objects: usize) {
        if self.versions.len() != CACHE_CHANNELS {
            self.versions = vec![0; CACHE_CHANNELS];
            self.states = vec![None; CACHE_CHANNELS];
        }
        if self.entries.len() != objects {
            self.entries = (0..objects).map(|_| None).collect();
            self.live.clear();
        }
        let mut now = std::mem::take(&mut self.scratch);
        now.clear();
        now.resize(CACHE_CHANNELS, None);
        for (id, color, opacity, blending) in colors.channel_states() {
            if let Some(slot) = usize::try_from(id).ok().and_then(|i| now.get_mut(i)) {
                *slot = Some((color, opacity.to_bits(), blending));
            }
        }
        for (i, v) in self.versions.iter_mut().enumerate() {
            if now[i] != self.states[i] {
                *v = v.wrapping_add(1);
            }
        }
        self.scratch = std::mem::replace(&mut self.states, now);
        self.frame = self.frame.wrapping_add(1);
        if self.frame % 64 == 0 {
            let (frame, entries) = (self.frame, &mut self.entries);
            self.live.retain(|&i| {
                let keep = entries[i].as_ref().is_some_and(|e| frame.wrapping_sub(e.frame) < 64);
                if !keep {
                    entries[i] = None;
                }
                keep
            });
        }
    }

    /// The key an object's sprites are valid for: its channels' versions (and the background
    /// and light background's, for glows colored by them). None for untracked channels.
    fn key(&self, base: i32, detail: i32, glow_bg: bool) -> Option<[u32; 4]> {
        let v = |c: i32| usize::try_from(c).ok().and_then(|i| self.versions.get(i).copied());
        let (b, d) = (v(base)?, v(detail)?);
        Some(if glow_bg { [b, d, v(1000)?, v(1007)?] } else { [b, d, 0, 0] })
    }

    fn get(&mut self, index: usize, key: [u32; 4]) -> Option<&[(u8, Draw)]> {
        let frame = self.frame;
        let e = self.entries.get_mut(index)?.as_mut()?;
        if e.key != key {
            return None;
        }
        e.frame = frame;
        Some(&e.draws)
    }

    fn store(&mut self, index: usize, key: [u32; 4], draws: Vec<(u8, Draw)>) {
        let Some(slot) = self.entries.get_mut(index) else { return };
        if slot.is_none() {
            self.live.push(index);
        }
        *slot = Some(Box::new(CachedObject { key, frame: self.frame, draws }));
    }
}

struct TypeSprites {
    frames: Vec<Option<crate::assets::Frame>>,
    order: Vec<usize>,
    back: Vec<bool>,
    /// Detail (colour) parts, and whether a visible one exists.
    detail: Vec<bool>,
    has_detail: bool,
}

impl TypeSprites {
    fn new(sprites: &[cadence_core::objects::SpriteDef], frame: &dyn Fn(&str) -> Option<crate::assets::Frame>) -> TypeSprites {
        let mut order: Vec<usize> = (0..sprites.len()).collect();
        order.sort_by_key(|&i| if sprites[i].parent < 0 { 0 } else { sprites[i].z as i32 });
        let detail: Vec<bool> = sprites.iter().map(|s| s.role == SpriteRole::Detail || s.frame.contains("_color_")).collect();
        TypeSprites {
            frames: sprites.iter().map(|s| frame(s.frame)).collect(),
            order,
            back: sprites.iter().map(|s| s.frame.starts_with("portal_") && s.frame.contains("_back_")).collect(),
            has_detail: sprites.iter().zip(&detail).any(|(s, &d)| s.visible && !s.frame.is_empty() && d),
            detail,
        }
    }
}

/// An object's fields as the level string gives them (`RawObject` parses text on every read).
#[derive(Clone, Copy, Debug)]
struct ObjStatic {
    x: f32,
    y: f32,
    id: i32,
    canonical: i32,
    /// key 32, and the separate x/y scales (128/129) when set
    scale: f32,
    scale_x: Option<f32>,
    scale_y: Option<f32>,
    flip_x: bool,
    flip_y: bool,
    rotation: f32,
    /// key 21 (or the 1.x color key 19), key 22
    main_channel: Option<i32>,
    detail_channel: Option<i32>,
    /// keys 64 (don't fade), 67 (don't enter), 96 (no glow); editor layers 20 and 61
    no_fade: bool,
    no_enter: bool,
    /// key 343: the enter channel (0..100) whose enter effect the object takes
    enter_channel: i16,
    no_glow: bool,
    layers: (i32, i32),
}

impl ObjStatic {
    fn of(o: &RawObject) -> ObjStatic {
        ObjStatic {
            x: o.x(),
            y: o.y(),
            id: o.id(),
            canonical: o.canonical_id(),
            scale: o.get_f32(32).unwrap_or(1.0),
            scale_x: o.get_f32(128),
            scale_y: o.get_f32(129),
            flip_x: o.get(4) == Some("1"),
            flip_y: o.get(5) == Some("1"),
            rotation: o.get_f32(6).unwrap_or(0.0),
            main_channel: o.get_i32(21).or_else(|| legacy_color(o)),
            detail_channel: o.get_i32(22),
            no_fade: o.get(64) == Some("1"),
            no_enter: o.get(67) == Some("1"),
            enter_channel: o.get_i32(343).unwrap_or(0).clamp(0, 100) as i16,
            no_glow: o.get(96) == Some("1"),
            layers: (o.get_i32(20).unwrap_or(0), o.get_i32(61).unwrap_or(0)),
        }
    }
}

/// The culling columns for `order`: each object's position in it, bucketed by its x; objects
/// that can move (in a group move-like triggers move, or pinned by a UI trigger) go in
/// `movable` instead (in 2.2 levels most objects are in some group; few groups move).
fn culling_columns(order: &[(i32, i32, usize)], statics: &[ObjStatic], can_move: &[bool], pinned: &std::collections::HashMap<usize, UiPin>) -> (Vec<Vec<u32>>, i32, Vec<u32>) {
    let col = |x: f32| (x / COLUMN).floor() as i32;
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for s in statics {
        lo = lo.min(col(s.x));
        hi = hi.max(col(s.x));
    }
    if lo > hi {
        return (Vec::new(), 0, Vec::new());
    }
    let mut columns = vec![Vec::new(); (hi - lo + 1) as usize];
    let mut movable = Vec::new();
    let all_movable = std::env::var_os("CADENCE_NO_COLUMNS").is_some();
    let _ = can_move; // (objects that move are tracked as they move: LevelVisual::track_moves)
    for (k, &(_, _, i)) in order.iter().enumerate() {
        if all_movable || pinned.contains_key(&i) {
            movable.push(k as u32);
        } else {
            columns[(col(statics[i].x) - lo) as usize].push(k as u32);
        }
    }
    (columns, lo, movable)
}


/// `FMODAudioEngine::pitchForIdx`: a speed index in semitones (−12..12) as a playback rate.
fn pitch_for_index(index: i32) -> f32 {
    2f32.powf(index.clamp(-12, 12) as f32 / 12.0)
}

/// An object's spin (`EnhancedGameObject::customSetup` 0x5bb8dc, turned each frame by
/// `updateRotateAction` 0x5be2c8): none with key 98; key 97 sets the rate; else the id's rate
/// from the dump, in a direction picked per object (the game uses rand()).
fn spin_rate(obj: &RawObject, index: usize) -> f32 {
    if obj.get(98) == Some("1") {
        return 0.0;
    }
    let Ok(i) = crate::spin::SPIN.binary_search_by_key(&(obj.canonical_id() as u16), |s| s.0) else { return 0.0 };
    if let Some(custom) = obj.get_f32(97).filter(|&v| v != 0.0) {
        return custom;
    }
    // A stable per-object coin flip.
    let h = (index as u64).wrapping_mul(0x9e3779b97f4a7c15) >> 63;
    if h == 0 { crate::spin::SPIN[i].1 } else { -crate::spin::SPIN[i].1 }
}

struct Game {
    /// Built for the level editor: never steps, draws no player, shows editor-only objects.
    editor: bool,
    /// Shake triggers: the last one seen, its time left, the next offset change, the offset.
    shake_seen: (f32, f32, f32),
    shake_left: f32,
    shake_next: f32,
    shake_offset: (f32, f32),
    world: World,
    /// The player as it starts the level (with its camera triggers and level settings).
    player_start: Player,
    /// The robot's and spider's part animations (`*_AnimDesc.plist`) and what they play.
    robot_desc: Option<crate::anim::AnimDesc>,
    spider_desc: Option<crate::anim::AnimDesc>,
    robot_anim: crate::anim::AnimState,
    /// An orb launched the spider: it shows its `jump` pose until it starts falling.
    spider_orb_jump: bool,
    /// Seconds since the last spider teleport (its 0.4 → 1 elastic scale pop) and the streaks.
    spider_pop: f32,
    spider_dashes: Vec<SpiderDash>,
    /// Animated decorations (frame and part animations, animobj.rs).
    anim_objects: crate::animobj::AnimObjects,
    level: LevelVisual,
    /// Color channels with color triggers; `colors_start` is the state at the level start.
    colors: Colors,
    colors_start: Colors,
    /// Shader and gradient triggers (fx.rs).
    fx: crate::fx::LevelFx,
    player: Player,
    camera: Camera,
    view_w: f32,
    /// Display refresh period; frame times close to it are snapped to it (like the game, whose
    /// frame delta comes from the display link), so every frame runs the same number of steps.
    refresh_period: f64,
    recent_frames: Vec<f64>,
    /// How many frames ran 0, 1, ... 7 physics steps (logged periodically).
    pacing: [u32; 8],
    /// Benchmark/autoplay: inputs keyed by physics step since the attempt started.
    autoplay: Option<std::collections::BTreeMap<u32, bool>>,
    step_count: u32,
    last_steps: usize,
    /// The last frame's simulation time (benchmarks).
    tick_ms: f64,
    /// Last frame's sprite count (the next frame's list starts this big).
    draw_capacity: usize,
    /// Static objects' finished sprites from earlier frames (see `DrawCache`).
    draw_cache: DrawCache,
    /// Each object's enter effect lock (`m_enterType` / `m_exitType`, cadence_core::enter), and
    /// the world epoch they belong to (a restart clears them).
    enter_locks: Vec<cadence_core::enter::EnterLock>,
    enter_epoch: u64,
    step_accumulator: f64,
    last_frame: Instant,
    holding: bool,
    /// The button state the touch triggers (1595) last saw, and whether a dual-mode one keeps
    /// the presses from the player.
    button_seen: bool,
    dual_touch: bool,
    /// Platformer left/right held (buttons 2 and 3), and what the player was last told.
    direction: [bool; 2],
    direction_sent: [bool; 2],
    death_timer: f32,
    /// playDeathEffect's `CCFadeTo(0.05, 0)` on the player that died (tag 4): None until the
    /// first frame after the death (cocos2d's first action step is at elapsed 0), then the
    /// action's elapsed time.
    death_fade: Option<f32>,
    /// `GJBaseGameLayer::playExitDualEffect` for the player that didn't die in dual mode.
    exit_dual: Option<ExitDual>,
    audio: Box<dyn Audio>,
    /// The level's song and where it starts (`kA13`, seconds).
    song: Option<(String, f32)>,
    /// The background's position in screen units (`m_background`, set to 0, 0 when created),
    /// moved as `updateCameraBGArt` does: by the view center's movement × zoom × 0.1 (x reversed
    /// while mirrored). The x wraps by the image's width when drawn.
    bg_pos: [f32; 2],
    /// Seconds since the level was completed (the end animation), if it was.
    end_time: Option<f32>,
    /// Where the player was when the level ended (the end animation starts there).
    end_from: (f32, f32),
    particle_defs: ParticleLibrary,
    effects: Effects,
    player_fx: PlayerFx,
    /// Secret coins (142, user coins 1329): level index and world rect, and the ones taken this
    /// attempt with seconds since. `clock` drives their spin.
    coins: Vec<(usize, cadence_core::world::Rect)>,
    coins_taken: Vec<(usize, f32)>,
    clock: f32,
    meter: Meter,
    /// Which `rod_ball_NN` the rods carry this session: PlayLayer::init picks
    /// round(rand·2) + 1 (+0x37ec, `GameObject::getBallFrame` 0x5b850c); CADENCE_BALL overrides.
    ball_frame: u32,
    /// The attempt number the label shows (1 for the first).
    attempt: u32,
    /// The regular trail (`m_regularTrail`) and where the player was at the last frame's end
    /// (the trail follows a frame behind).
    trail: crate::trail::MotionStreak,
    trail_pos: (f32, f32),
    wave_trail: crate::trail::WaveTrail,
    /// The level's background and ground art (kA6 / kA7, 0 = 1): `game_bg_NN_001`,
    /// `groundSquare_NN_001` and, for grounds 8+, its second layer `_2` (ground 2 color).
    bg_image: String,
    ground_image: String,
    ground2_image: Option<String>,
    /// The level's own background, ground and MG ids (kA6 / kA7 / kA25), the change triggers'
    /// art in force (`Camera::art`, 0 = the level's) when the names above were set, and every
    /// image the change triggers can switch to (loaded with the level's own).
    art_default: [i32; 3],
    art_shown: [i32; 3],
    art_preload: Vec<String>,
    /// The middle ground (kA25 1–3, `GJMGLayer`): `fg_NN_001` (channel 1013) and `fg_NN_2_001`
    /// (1014), its default y (`defaultYOffsetForBG2`: 25, 30, 30) and its scroll.
    mg: Option<(String, String, f32)>,
    mg_x: f32,
    /// Practice mode (`PauseLayer::onPracticeMode`): checkpoints placed (oldest first) and the
    /// auto checkpoint waiting to be committed (`PlayerObject::tryPlaceCheckpoint`).
    practice: bool,
    checkpoints: Vec<Checkpoint>,
    /// The last platformer checkpoint (2063) touched, with the group it spawns on a respawn
    /// (key 448): a death restarts from it (`PlayLayer::processCheckpoints`).
    platformer_checkpoint: Option<(Checkpoint, i32)>,
    /// The song channel the music plays on (song triggers' key 432; the level's own song is
    /// channel 0), for edit song triggers.
    music_channel: i32,
    pending_checkpoint: Option<Checkpoint>,
    /// The options that change play (game variables, from the save).
    opts: PlayOptions,
    /// The player's icons and colors (the icon kit).
    icons: crate::ui::garage::IconSet,
    pcol: ([f32; 3], [f32; 3]),
}

/// Game variables the level reads (`MoreOptionsLayer` toggles).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayOptions {
    /// 0052: respawn after 0.5 s instead of 1.
    pub faster_reset: bool,
    /// 0135 / 0134: no attempt label (normal / practice).
    pub hide_attempts: bool,
    pub hide_attempts_practice: bool,
    /// 0027: auto checkpoints; 0068: quick checkpoints (every 100 steps instead of 500).
    pub auto_checkpoints: bool,
    pub quick_checkpoints: bool,
    /// 0061: the spider's teleport effect in the main color instead of the secondary one.
    pub spider_main_color: bool,
    /// 0100: the chosen death effect in practice mode too (else the default there).
    pub practice_death_effect: bool,
}

impl PlayOptions {
    pub fn from_progress(p: &crate::ui::save::Progress) -> PlayOptions {
        PlayOptions {
            faster_reset: p.var("0052", false),
            hide_attempts: p.var("0135", false),
            hide_attempts_practice: p.var("0134", false),
            auto_checkpoints: p.var("0027", true),
            quick_checkpoints: p.var("0068", false),
            spider_main_color: p.var("0061", false),
            practice_death_effect: p.var("0100", false),
        }
    }
}

/// The game state a checkpoint restores (`PlayLayer::createCheckpoint` / `loadFromCheckpoint`).
#[derive(Clone)]
struct Checkpoint {
    /// The step counter when it was made (`+0xf20`; spacing is measured on it).
    step: u32,
    world: World,
    player: Player,
    colors: Colors,
    camera: Camera,
    bg_pos: [f32; 2],
    mg_x: f32,
}

/// Auto checkpoints: one `CHECKPOINT_GAP` steps after the last, committed once the player lives
/// another `CHECKPOINT_GAP` steps (`tryPlaceCheckpoint`: 500, or 100 in checkpoint mode +0x5f5).
const CHECKPOINT_GAP: u32 = 500;
/// `GameManager::getPracticeMusicFile`.
const PRACTICE_MUSIC: &str = "StayInsideMe.mp3";

/// The game's music meter (`FMODAudioEngine::updateMetering` 0x350390), stepped once a frame
/// from the music's peak: a beat (the level + 0.1 at least 10% over the current value, more
/// than 2 frames after the last) jumps to (level + 0.1)·1.1, else the value decays ×0.93.
/// Objects with an audio scale pulse with `value` (`PlayLayer::updateVisibility`).
#[derive(Clone, Copy, Default)]
struct Meter {
    value: f32,
    last_beat: f32,
    frames: i32,
}

impl Meter {
    fn step(&mut self, peak: f32) {
        let prev = self.value;
        let level = peak + 0.1;
        let beat = self.frames > 2
            && prev * 1.1 <= level
            && (self.last_beat * 0.95 <= level || prev <= self.last_beat * 0.2);
        if beat {
            self.last_beat = level;
            self.value = level * 1.1;
            self.frames = 1;
        } else {
            self.value = prev * 0.93;
            self.frames += 1;
        }
        if self.value <= 0.1 {
            self.last_beat = 0.0;
        }
    }
}

/// An object's audio-scale range (from the sprite dump: `m_usesAudioScale`,
/// `m_customAudioScale` min/max, set in `GameObject::customSetup`), and whether it's an orb
/// (`RingObject::setRScale` uses min(v + 0.3, 1.2)).
fn audio_scale(id: i32) -> Option<(Option<(f32, f32)>, bool)> {
    Some(match id {
        36 | 84 | 141 | 1022 | 1330 | 1333 | 1594 | 1704 | 1751 | 3004 | 3027 => (None, true),
        37 | 50 | 51 | 52 | 53 | 54 | 60 | 148 | 149 | 405 => (None, false),
        132 | 133 | 136 | 150 | 236 | 460 | 494 | 495 | 496 | 497 | 1055 | 1056 | 1057 => (Some((0.8, 1.2)), false),
        1582 => (Some((0.9, 1.2)), false),
        1583 => (Some((0.9, 1.1)), false),
        _ => return None,
    })
}

/// A pre-2.0 object's color (key 19, read when key 21 is absent; `GameObject::objectFromVector`
/// 0x5b1424): 1/2 player colors, 3/4/6/7 color 1–4, 5 light background, 8 3D line.
fn legacy_color(obj: &RawObject) -> Option<i32> {
    Some(match obj.get_i32(19)? {
        1 => 1005,
        2 => 1006,
        3 => 1,
        4 => 2,
        5 => 1007,
        6 => 3,
        7 => 4,
        8 => 1003,
        _ => return None,
    })
}

/// The level's ground art number (kA7, 0 = 1).
/// Background art `id` (kA6 / change BG trigger): game_bg_NN_001, 0 meaning 1.
fn bg_name(id: i32) -> String {
    format!("game_bg_{:02}_001", id.clamp(1, 59))
}

/// Ground art `id` (kA7 / change ground trigger): the ground and, from 8 on, its second layer.
fn ground_names(id: i32) -> (String, Option<String>) {
    let n = id.clamp(1, 22);
    (format!("groundSquare_{n:02}_001"), (n >= 8).then(|| format!("groundSquare_{n:02}_2_001")))
}

/// Middle ground art `id` (kA25 / change MG trigger): its two tiles and base y, none for 0.
fn mg_art(id: i32) -> Option<(String, String, f32)> {
    let n = usize::try_from(id).ok().filter(|v| (1..=3).contains(v))?;
    Some((format!("fg_{n:02}_001"), format!("fg_{n:02}_2_001"), [25.0, 30.0, 30.0][n - 1]))
}

fn level_ground(level: &LevelString) -> u32 {
    level.setting("kA7").and_then(|v| v.parse::<u32>().ok()).filter(|&v| v > 0).unwrap_or(1).min(22)
}

/// Secret coins: gold (main levels) and user coins.
fn is_coin(id: i32) -> bool {
    id == 142 || id == 1329
}

impl Game {
    /// One frame of the regular trail: on while flying (ship, UFO, swing; the wave has its own),
    /// at the player's position a frame ago (ship: 10.74 lower, fitted to one dump).
    fn step_trail(&mut self) {
        let flying = matches!(self.player.mode, Mode::Ship | Mode::Ufo | Mode::Swing);
        if flying {
            self.trail.active = true;
        }
        if self.player.mode == Mode::Wave || self.player.dead {
            self.trail.active = false;
        }
        self.trail.update(1.0 / 60.0, self.trail_pos);
        let down = if self.player.mode == Mode::Ship { -10.74 } else { 0.0 };
        self.trail_pos = (self.player.x, self.player.y + down);
    }

    /// The middle ground's node scale (`updateCameraBGArt` 0x10f978): (1 − (1 − zoom)·0.3)·1.2.
    fn mg_scale(&self) -> f32 {
        ((1.0 - (1.0 - self.camera.zoom).abs()) * -0.3 + 1.0) * 1.2
    }

    /// `updateCameraBGArt` for the background: the view's center moved from `before`'s (at
    /// its zoom) to the camera's now; the background moves the other way by that × the zoom
    /// × 0.1 (`updateBGArtSpeed`'s default), x reversed by the mirror flip.
    /// Shake triggers: a random offset of the view, changed every interval, for the duration,
    /// counted in game time (a step at a time), so a shake ends on time however many frames are
    /// drawn (screenshots and benchmarks draw few; slow machines drop some).
    fn step_shake(&mut self) {
        if self.world.shake != self.shake_seen {
            self.shake_seen = self.world.shake;
            self.shake_left = self.world.shake.2;
            self.shake_next = 0.0;
        }
        if self.shake_left > 0.0 && self.world.shake.0 > 0.0 {
            let dt = self.world.step_secs as f32;
            self.shake_left -= dt;
            self.shake_next -= dt;
            if self.shake_next <= 0.0 {
                self.shake_next = self.world.shake.1.max(0.0);
                let r = |k: u32| (((self.step_count as u64).wrapping_mul(2654435761).wrapping_add(k as u64 * 40503) % 2001) as f32 / 1000.0) - 1.0;
                self.shake_offset = (r(1) * self.world.shake.0, r(2) * self.world.shake.0);
            }
        } else {
            self.shake_offset = (0.0, 0.0);
        }
    }

    /// The art names for the change triggers' art in force (`Camera::art`).
    fn sync_art(&mut self) {
        if self.camera.art == self.art_shown {
            return;
        }
        self.art_shown = self.camera.art;
        let id = |k: usize| if self.camera.art[k] > 0 { self.camera.art[k] } else { self.art_default[k] };
        self.bg_image = bg_name(id(0));
        (self.ground_image, self.ground2_image) = ground_names(id(1));
        self.mg = mg_art(id(2));
    }

    fn scroll_bg(&mut self, before: Camera) {
        let z = |c: &Camera| if c.zoom > 0.0 { c.zoom } else { 1.0 };
        let center = |c: &Camera| (c.x + self.view_w / z(c) * 0.5, c.y + VIEW_HEIGHT / z(c) * 0.5);
        let ((x0, y0), (x1, y1)) = (center(&before), center(&self.camera));
        let zoom = z(&self.camera);
        // the speed factors (+0x314, BG speed triggers 3606; 0.1 by default)
        let (sx, sy) = self.world.bg_speed;
        self.bg_pos[0] += (1.0 - 2.0 * self.camera.flip) * (x0 - x1) * zoom * sx;
        self.bg_pos[1] += (y0 - y1) * zoom * sy;
    }

    /// The middle ground scrolls by the camera's movement over its scale (the game's speed
    /// factor +0x31c, fitted: 0.55), wrapping every 256.
    fn scroll_mg(&mut self, dx: f32) {
        if self.mg.is_some() {
            let s = self.mg_scale();
            // (scaled by MG speed triggers' x factor, +0x31c: 0.3 by default)
            let k = 0.55 * self.world.mg_speed.0 / 0.3;
            self.mg_x = (self.mg_x - dx * k * (1.0 - 2.0 * self.camera.flip) / s).rem_euclid(256.0) - 256.0;
        }
    }

    /// The wave trail follows the player while in the wave and is gone otherwise.
    fn step_wave_trail(&mut self) {
        if self.player.mode == Mode::Wave && !self.player.dead {
            self.wave_trail.step((self.player.x, self.player.y), self.camera.x - 60.0);
        } else {
            self.wave_trail.clear();
        }
    }

    /// Coins the player touches this step (the coin's 40×40 box against the player's).
    fn take_coins(&mut self) {
        let player = self.player.rect();
        for &(index, rect) in &self.coins {
            if rect.intersects(&player) && !self.coins_taken.iter().any(|c| c.0 == index) {
                // (The pickup sound isn't identified yet: gold01/gold02 belong to the reward screens.)
                self.coins_taken.push((index, 0.0));
            }
        }
    }

    /// Advance the robot's and spider's animations one physics step (`PlayerObject::update`'s
    /// animation calls, `playDynamicSpiderRun`, `ringJump`): on the ground the robot runs and
    /// the spider walks (up to speed 0.9) or runs; in the air the robot plays its jump loop
    /// while rising and the fall loop after, the spider its fall loop (or its single `jump` pose
    /// after an orb until it starts falling).
    fn update_robot_anim(&mut self) {
        let dt = 1.0 / STEPS_PER_SECOND as f32;
        self.spider_pop = (self.spider_pop + dt).min(1e6);
        for d in &mut self.spider_dashes {
            d.elapsed += dt;
        }
        self.spider_dashes.retain(|d| d.elapsed < 8.0 * 0.04);
        let p = &self.player;
        let up = if p.upside_down { -p.y_velocity } else { p.y_velocity };
        if p.mode != Mode::Spider || p.on_ground || up <= 0.0 {
            self.spider_orb_jump = false;
        }
        let anim = match p.mode {
            Mode::Robot if p.on_ground => "run",
            Mode::Robot if up > 0.0 => "jump_loop",
            Mode::Robot => "fall_loop",
            Mode::Spider if p.on_ground && p.speed <= 0.9 => "walk",
            Mode::Spider if p.on_ground => "run",
            Mode::Spider if self.spider_orb_jump => "jump",
            Mode::Spider => "fall_loop",
            _ => return,
        };
        self.robot_anim.play(anim, dt * self.robot_anim_speed());
    }

    /// `PlayerObject::updateRobotAnimationSpeed` 0x382aec: the animations run faster with the
    /// speed portals (0.7 → 0.9, 1.1 → 1.15, 1.3 → 1.3, 1.6 → 1.5, else 1) and when mini
    /// (÷0.85); the spider's run another ÷1.1 from speed 1.1 up.
    fn robot_anim_speed(&self) -> f32 {
        let s = self.player.speed;
        let k = match s {
            s if s == 0.7 => 0.9,
            s if s == 1.1 => 1.15,
            s if s == 1.3 => 1.3,
            s if s == 1.6 => 1.5,
            _ => 1.0,
        };
        let d = if self.player.size < 1.0 { 0.85 } else { 1.0 };
        let d = if self.player.mode == Mode::Spider && s >= 1.1 { d * 1.1 } else { d };
        k / d
    }

    /// The robot's or spider's parts for the current frame. Frame delays are the game's
    /// `objectDefinitions.plist` (robot run 0.0275, jump/fall loops 0.05; spider run 0.04, walk
    /// 0.03, fall loop 0.05); the animation time already runs at the speed factor.
    fn robot_rig(&self) -> Option<&[crate::anim::Part]> {
        let spider = self.player.mode == Mode::Spider;
        let desc = if spider { self.spider_desc.as_ref()? } else { self.robot_desc.as_ref()? };
        let anim = if self.robot_anim.anim.is_empty() { "run" } else { self.robot_anim.anim };
        let delay = match (spider, anim) {
            (false, "run") => 0.0275,
            (true, "run") => 0.04,
            (true, "walk") => 0.03,
            _ => 0.05,
        };
        desc.frame(anim, self.robot_anim.index(delay)).or_else(|| desc.frame("run", 0))
    }

    /// Start (or restart) the song for a new attempt.
    /// Song triggers (1934) fired this step: the custom song from the song cache (downloaded
    /// with the level), from its start time. Volume, speed, fades and channels aren't ported.
    fn play_trigger_songs(&mut self) {
        for (id, start_ms, _volume, _looped, channel) in std::mem::take(&mut self.world.songs) {
            if self.practice || id <= 0 {
                continue;
            }
            self.music_channel = channel;
            let name = crate::online::song_name(id);
            if crate::online::have(&name) {
                self.audio.play_music(&name, start_ms as f32 / 1000.0);
            } else {
                log(&format!("song trigger: song {id} isn't downloaded"));
            }
        }
    }

    /// SFX triggers (3602) fired this step: the library sound from the SFX cache (downloaded
    /// with the level).
    fn play_trigger_sfx(&mut self) {
        for (id, volume) in std::mem::take(&mut self.world.sfx) {
            let name = crate::online::sfx_name(id);
            if crate::online::have(&name) {
                self.audio.play_trigger_effect(&name, volume);
            }
        }
    }

    fn start_music(&mut self) {
        if self.practice {
            return;
        }
        if let Some((file, offset)) = &self.song {
            self.audio.play_music(file, *offset);
            self.music_channel = 0;
        }
    }

    fn reset(&mut self) {
        self.attempt += 1;
        self.start_music();
        self.world.reset();
        self.player = self.player_start.clone();
        self.player.set_button(self.holding, &self.world);
        self.coins_taken.clear();
        self.trail.reset();
        self.trail.active = false;
        self.wave_trail.clear();
        // A direction held into the new attempt is pressed again on its first step.
        self.direction_sent = [false; 2];
        self.camera = Camera::new();
        self.bg_pos = [0.0; 2];
        self.end_time = None;
        self.colors = self.colors_start.clone();
        self.fx.reset();
        self.death_timer = 0.0;
        self.death_fade = None;
        self.exit_dual = None;
        self.step_count = 0;
        self.effects.clear();
        self.player_fx.reset();
        self.pending_checkpoint = None;
        if self.practice {
            if let Some(cp) = self.checkpoints.last().cloned() {
                self.load_checkpoint(cp);
            }
        } else if let Some((cp, respawn)) = self.platformer_checkpoint.clone() {
            self.load_checkpoint(cp);
            if respawn > 0 {
                self.world.spawn_group(respawn);
            }
        }
    }

    /// A platformer checkpoint (2063) was touched (`PlayLayer::processCheckpoints` 0x7e75c):
    /// spawn 604 first, a checkpoint at the trigger (or the player with 138, or group 71's main
    /// object), spawn 51 after. A respawn from it spawns 448.
    fn checkpoint_trigger(&mut self) {
        let Some(order) = self.world.checkpoint.take() else { return };
        let Some(raw) = self.level.objects.get(order as usize) else { return };
        let (before, after, respawn) = (raw.get_i32(604).unwrap_or(0), raw.get_i32(51).unwrap_or(0), raw.get_i32(448).unwrap_or(0));
        let by_player = raw.get(138) == Some("1");
        let target = raw.get_i32(71).unwrap_or(0);
        if before > 0 {
            self.world.spawn_group(before);
        }
        let at = if by_player {
            (self.player.x, self.player.y)
        } else if let Some((x, y)) = (target > 0).then(|| self.world.group_main_position(target)).flatten() {
            (x as f32, y as f32)
        } else {
            let (x, y) = self.world.level_position(order as usize);
            (x as f32, y as f32)
        };
        let mut cp = self.snapshot();
        cp.player.x = at.0;
        cp.player.y = at.1;
        self.platformer_checkpoint = Some((cp, respawn));
        if after > 0 {
            self.world.spawn_group(after);
        }
    }

    /// Spawn particle triggers (3608) fired this step: a copy of each custom particle object
    /// (2065) of the particle group, placed by its offset from the group's main object (turned
    /// and scaled with the trigger's rotation and scale) around the spawn point
    /// (`GJBaseGameLayer::spawnParticleTrigger` 0xe4648). Systems that never end burst once.
    fn spawn_trigger_particles(&mut self) {
        for (group, at, rotation, scale) in std::mem::take(&mut self.world.particle_spawns) {
            let members: Vec<u32> = self.world.members_of(group).to_vec();
            let main = members.first().map(|&o| self.world.level_position(o as usize));
            for o in members {
                let index = o as usize;
                let Some(text) = self.level.particles.iter().find(|p| p.0 == index).map(|p| p.1.clone()) else { continue };
                let Some((mut def, grouped)) = crate::particles::ParticleDef::from_gd_string(&text) else { continue };
                if def.duration < 0.0 {
                    def.duration = 0.0;
                }
                let obj = &self.level.objects[index];
                let (px, py) = self.world.level_position(index);
                let position = match main {
                    Some((mx, my)) => {
                        let (dx, dy) = ((px - mx) as f32, (py - my) as f32);
                        // cocos rotations are clockwise
                        let (s, c) = (-rotation).to_radians().sin_cos();
                        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
                        (at.0 + rx * scale, at.1 + ry * scale)
                    }
                    None => at,
                };
                let s = obj.get_f32(32).unwrap_or(1.0);
                let node_scale = (obj.get_f32(128).unwrap_or(s) * scale, obj.get_f32(129).unwrap_or(s) * scale);
                let node_rotation = obj.get_f32(6).unwrap_or(0.0) + rotation;
                self.effects.spawn_node(&def, position, if grouped { PositionType::Grouped } else { PositionType::Relative }, node_rotation, node_scale);
            }
        }
    }

    /// Edit song / edit SFX triggers (3605 / 3603) fired this step: stop the channel, or change
    /// its volume / speed (`applySFXEditTrigger` 0x112928). Songs: the music when it plays on
    /// the trigger's channel; SFX: the sounds SFX triggers started.
    fn apply_audio_edits(&mut self) {
        for edit in std::mem::take(&mut self.world.audio_edits) {
            if edit.song {
                if edit.channel != self.music_channel {
                    continue;
                }
                if edit.stop {
                    self.audio.stop_music();
                    continue;
                }
                if let Some(v) = edit.volume {
                    self.audio.fade_music(v, edit.duration);
                }
                if let Some(speed) = edit.speed {
                    self.audio.set_music_speed(pitch_for_index(speed));
                }
            } else if edit.stop {
                self.audio.stop_effects();
            } else if let Some(v) = edit.volume {
                self.audio.set_effects_volume(v);
            }
        }
    }

    fn snapshot(&self) -> Checkpoint {
        Checkpoint {
            step: self.step_count,
            world: self.world.clone(),
            player: self.player.clone(),
            colors: self.colors.clone(),
            camera: self.camera,
            bg_pos: self.bg_pos,
            mg_x: self.mg_x,
        }
    }

    fn load_checkpoint(&mut self, cp: Checkpoint) {
        self.step_count = cp.step;
        self.world = cp.world;
        self.world.epoch = cadence_core::world::World::next_epoch();
        self.player = cp.player;
        self.player.dead = false;
        self.player.second_died = false;
        self.death_fade = None;
        self.exit_dual = None;
        self.player.set_button(self.holding, &self.world);
        self.colors = cp.colors;
        self.camera = cp.camera;
        self.bg_pos = cp.bg_pos;
        self.mg_x = cp.mg_x;
    }

    /// Enter or leave practice mode. Leaving restarts the level, as the game does.
    fn set_practice(&mut self, on: bool) {
        if self.practice == on {
            return;
        }
        self.practice = on;
        self.checkpoints.clear();
        self.pending_checkpoint = None;
        if on {
            self.audio.play_loop(PRACTICE_MUSIC);
        } else {
            self.reset();
        }
    }

    /// Z: a checkpoint here (`markCheckpoint`).
    fn place_checkpoint(&mut self) {
        if self.practice && !self.player.dead && self.end_time.is_none() {
            let cp = self.snapshot();
            self.checkpoints.push(cp);
        }
    }

    /// X: remove the last checkpoint (`removeCheckpoint`).
    fn remove_checkpoint(&mut self) {
        if self.practice {
            self.checkpoints.pop();
        }
    }

    /// `PlayerObject::tryPlaceCheckpoint`, once a step: on the ground (or flying), snapshot
    /// `CHECKPOINT_GAP` steps after the last checkpoint, commit it `CHECKPOINT_GAP` later.
    fn auto_checkpoint(&mut self) {
        if !self.practice || self.player.dead || !self.opts.auto_checkpoints {
            return;
        }
        let gap = if self.opts.quick_checkpoints { 100 } else { CHECKPOINT_GAP };
        let grounded = self.player.on_ground || !matches!(self.player.mode, cadence_core::physics::Mode::Cube | cadence_core::physics::Mode::Robot | cadence_core::physics::Mode::Ball | cadence_core::physics::Mode::Spider);
        if !grounded {
            return;
        }
        let last = self.checkpoints.last().map(|c| c.step);
        match &self.pending_checkpoint {
            None => {
                if last.is_none_or(|l| self.step_count.saturating_sub(l) > gap) {
                    self.pending_checkpoint = Some(self.snapshot());
                }
            }
            Some(p) => {
                if self.step_count.saturating_sub(p.step) > gap {
                    let cp = self.pending_checkpoint.take().unwrap();
                    self.checkpoints.push(cp);
                }
            }
        }
    }

    /// The circles for what the last physics step did: orbs (`ringJump`), pads (`playBumpEffect`)
    /// and portals (`spawnPortalCircle`), each over its object.
    fn on_events(&mut self) {
        let rgb = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
        let mut landed = None;
        // the same events as `GJGameEvent` ids, for event triggers (3604)
        let gj: Vec<i32> = self
            .player
            .events
            .iter()
            .flat_map(|e| {
                let id = |i: usize| self.world.objects.get(i).map_or(0, |o| o.id);
                match *e {
                    Event::Orb(i) => {
                        let kind = match id(i) {
                            36 => 34,
                            141 => 35,
                            1333 => 36,
                            84 => 37,
                            1022 => 38,
                            1330 => 39,
                            1594 => 40,
                            1704 => 41,
                            1751 => 42,
                            3004 => 43,
                            3027 => 44,
                            _ => 0,
                        };
                        vec![8, kind]
                    }
                    Event::Pad(i) => {
                        let kind = match id(i) {
                            35 => 45,
                            140 => 46,
                            1332 => 47,
                            67 => 48,
                            3005 => 49,
                            _ => 0,
                        };
                        vec![9, kind]
                    }
                    Event::Portal { index, .. } => vec![match id(index) {
                        12 => 26,
                        13 => 27,
                        47 => 28,
                        111 => 29,
                        660 => 30,
                        745 => 31,
                        1331 => 32,
                        1933 => 33,
                        2926 => 50,
                        10 => 51,
                        11 => 52,
                        45 => 53,
                        46 => 54,
                        99 => 55,
                        101 => 56,
                        286 => 57,
                        287 => 58,
                        747 => 59,
                        _ => 0,
                    }],
                    Event::Land { .. } => vec![4],
                    Event::SpiderDash { .. } => vec![18],
                }
            })
            .filter(|&v| v != 0)
            .collect();
        if !gj.is_empty() {
            self.world.player_events(&gj);
        }
        for event in std::mem::take(&mut self.player.events) {
            let wave = match event {
                Event::Land { slope_angle } => {
                    // hitGround stops the trail (flying modes keep it).
                    if !matches!(self.player.mode, Mode::Ship | Mode::Ufo | Mode::Swing) {
                        self.trail.active = false;
                    }
                    landed = Some(slope_angle);
                    continue;
                }
                Event::Orb(i) => {
                    self.trail.active = true;
                    let o = &self.world.objects[i];
                    let color = match o.kind {
                        ObjectType::PINK_RING => [255, 0, 255],
                        ObjectType::GRAVITY_RING => [0, 255, 255],
                        ObjectType::GREEN_RING => [0, 255, 0],
                        ObjectType::RED_RING => [255, 100, 0],
                        ObjectType(37) => [0, 255, 0],
                        ObjectType(38) => [255, 0, 255],
                        ObjectType(43) => [255, 50, 255],
                        _ => [255, 200, 0],
                    };
                    let r0 = if o.kind == ObjectType::RED_RING { 42.0 } else { 35.0 };
                    CircleWave::fading_in((o.x, o.y), r0, 5.0, 0.35, rgb(color))
                }
                Event::Pad(i) => {
                    self.trail.active = true;
                    let o = &self.world.objects[i];
                    let color = match o.kind {
                        ObjectType::PINK_PAD => [255, 0, 255],
                        ObjectType::GRAVITY_PAD => [0, 255, 255],
                        ObjectType(44) => [255, 50, 255],
                        _ => [255, 200, 0],
                    };
                    let r0 = if o.kind == ObjectType::RED_PAD && self.player.size >= 1.0 { 12.0 } else { 10.0 };
                    CircleWave::new((o.x, o.y), r0, 40.0, 0.25, rgb(color))
                }
                Event::Portal { index, radius, color } => {
                    let o = &self.world.objects[index];
                    CircleWave::fading_in((o.x, o.y), radius, 5.0, 0.3, rgb(color))
                }
                Event::SpiderDash { from, to } => {
                    self.spider_dash_effect(from, to);
                    continue;
                }
            };
            if matches!(event, Event::Orb(_)) && self.player.mode == Mode::Spider {
                // ringJump: tweenToAnimation(0.06, "jump")
                self.spider_orb_jump = true;
            }
            self.effects.waves.push(wave);
        }
        self.player_fx.step(&self.player, 1.0 / STEPS_PER_SECOND as f32, landed);
    }

    /// `PlayerObject::playSpiderDashEffect` 0x376a20: both ends move 7.5 forward; a filled
    /// circle shrinks at the start (size·13 → 1 in 0.15 s), and at the end (x + 6) a filled one
    /// at 0.42 opacity (size·26 → 2, 0.25 s) and a 2-point ring (size·10 → size·45, 0.25 s), all
    /// linear; the streak sprite goes between them; the spider pops from scale 0.4 back to 1
    /// (`CCEaseElasticOut` 0.35 over 0.45 s). All in the secondary color, or the main one with
    /// option 0061 ("Switch Spider Teleport Color").
    fn spider_dash_effect(&mut self, from: (f32, f32), to: (f32, f32)) {
        let p = &self.player;
        let dir = if p.going_left { -1.0 } else { 1.0 };
        let (mut from, mut to) = (from, to);
        from.0 += dir * 7.5;
        to.0 += dir * 7.5;
        let color = if self.opts.spider_main_color { self.pcol.0 } else { self.pcol.1 };
        let size = p.size;
        to.0 += 6.0;
        let linear = |c: CircleWave| CircleWave { ease_out: false, ..c };
        self.effects.waves.push(linear(CircleWave::new(from, size * 10.0 * 1.3, 1.0, 0.15, color)));
        self.effects.waves.push(CircleWave { opacity_mod: 0.42, ..linear(CircleWave::new(to, size * 20.0 * 1.3, 2.0, 0.25, color)) });
        self.effects.waves.push(CircleWave { outline: true, ..linear(CircleWave::new(to, size * 10.0, size * 30.0 * 1.5, 0.25, color)) });
        let sideways = p.sideways;
        let rotation = if sideways { if p.upside_down { 180.0 } else { 0.0 } } else if p.upside_down { 90.0 } else { -90.0 };
        let (center, length) = if sideways {
            ((from.0 + (to.0 - from.0) * 0.5, from.1), (from.0 - to.0).abs() + 30.0)
        } else {
            ((from.0, from.1 + (to.1 - from.1) * 0.5), (from.1 - to.1).abs() + 30.0)
        };
        self.spider_dashes.push(SpiderDash { center, rotation, length, color, elapsed: 0.0 });
        self.spider_pop = 0.0;
    }

    /// The spider's teleport pop scale now (1 when settled).
    fn spider_scale(&self) -> f32 {
        if self.spider_pop >= 0.45 {
            return 1.0;
        }
        0.4 + 0.6 * elastic_out(self.spider_pop / 0.45, 0.35)
    }

    /// `PlayerObject::playDeathEffect` (0x1a176c): the death effect chosen in the icon kit (the
    /// default in practice mode unless option 0100 shows them there), see `crate::death`. All of
    /// it sits at the player's position, sized by the vehicle size (× 0.9 when it's at least 1).
    fn on_death(&mut self) {
        // `PlayLayer::destroyPlayer`: the player that died explodes (player 2 in dual mode when
        // it was its death, with the colors swapped); the other one leaves with the exit-dual
        // effect and is hidden (`playerDestroyed(other, true)`).
        let second = self.player.second_died && self.player.dual.is_some();
        let (p, pcol) = match (second, self.player.dual.as_deref()) {
            (true, Some(p2)) => (p2.clone(), (self.pcol.1, self.pcol.0)),
            _ => (self.player.clone(), self.pcol),
        };
        if let Some(other) = if second { Some(&self.player) } else { self.player.dual.as_deref() } {
            let colors = if second { self.pcol } else { (self.pcol.1, self.pcol.0) };
            self.exit_dual = Some(ExitDual::new(other, colors, self.camera.zoom, self.player.mirrored));
            // The ring: CCCircleWave(10 → 30, 0.4 s) in the main color, outlined, following
            // the copy (`followObject`).
            let wave = CircleWave { outline: true, z: EXIT_DUAL_WAVE_Z, first_tick: true, ..CircleWave::new((other.x, other.y), 10.0, 30.0, 0.4, colors.0) };
            self.effects.waves.push(wave);
        }
        let p = &p;
        let at = (p.x, p.y);
        let size = if p.size >= 1.0 { p.size * 0.9 } else { p.size };
        let mut effect = self.icons.ids[10];
        if self.practice && !self.opts.practice_death_effect {
            effect = 1;
        }
        let seed = self.effects.next_seed();
        let defs = |name: &str| self.particle_defs.get(name).cloned();
        let Some(death) = crate::death::play(effect, at, size, pcol, &mut self.effects.rng, seed, &defs) else { return };
        self.effects.emitters.push(death.emitter);
        self.effects.waves.extend(death.waves);
        self.effects.deaths.extend(death.sprite);
        if !death.shatter {
            return;
        }
        // The icon shatter (unless the "no explode shatter" option, game variable 0153, is on):
        // a (size·40)² capture cut into 2–3 × 2–3 pieces (5% of the time one column, 5% one
        // row), flying at the player's speed · 0.72 (vx 0.5v ± v, vy 6 ± 3, 1.4 s).
        let rng = &mut self.effects.rng;
        let (r1, r2, r3) = (rng.unit(), rng.unit(), rng.unit());
        let mut rows = (r2 * 2.0) as u32 + 2;
        let cols = if r3 <= 0.95 {
            if r3 > 0.9 {
                rows = 1;
            }
            (r1 * 2.0) as u32 + 2
        } else {
            1
        };
        let v = (p.base_speed * p.speed as f64 * 0.8999999761581421 * 0.800000011920929) as f32;
        let side = (size * 40.0) as i32 as f32;
        let shatter = Shatter::new(rng, at, side, cols, rows, (v * 0.5, v), (6.0, 3.0), (1.4, 0.0));
        self.effects.shatters.push(shatter);
    }

    fn tick(&mut self, now: Instant) {
        let mut dt = now.duration_since(self.last_frame).as_secs_f64().min(0.1);
        self.last_frame = now;
        // Where the display rate is unknown (the web), estimate it from the median of recent
        // frame times, so frames still run a steady number of steps.
        if self.refresh_period <= 0.0 && (0.004..0.05).contains(&dt) {
            self.recent_frames.push(dt);
            if self.recent_frames.len() > 31 {
                self.recent_frames.remove(0);
            }
        }
        let period = if self.refresh_period > 0.0 {
            self.refresh_period
        } else if self.recent_frames.len() >= 8 {
            let mut sorted = self.recent_frames.clone();
            sorted.sort_by(f64::total_cmp);
            let median = sorted[sorted.len() / 2];
            // Browser timers are coarse (16.70 ms measured for 60 Hz); snap to the standard rate it
            // is, or the error adds an extra step every few seconds.
            [60.0, 75.0, 90.0, 100.0, 120.0, 144.0, 165.0, 240.0]
                .iter()
                .map(|hz: &f64| 1.0 / hz)
                .find(|p| (median - p).abs() < p * 0.015)
                .unwrap_or(median)
        } else {
            0.0
        };
        // Frames are presented on the display's clock, so each frame lasts a whole number of
        // display periods (the game's frame delta comes from the display link). CPU-side timing
        // jitters around that, so round each frame on its own: jitter gives exactly one period
        // (4 steps at 60 Hz), a missed frame two, frames arriving too fast none. (Carrying the
        // remainder between frames is unstable: near half a period it alternates 0 and 2.)
        if period > 0.0 {
            dt = (dt / period).round() * period;
        }
        self.effects.update(dt as f32);
        self.clock += dt as f32;
        for c in &mut self.coins_taken {
            c.1 += dt as f32;
        }
        self.player_fx.update(dt as f32);
        if self.player.dead {
            self.death_fade = Some(self.death_fade.map_or(0.0, |t| t + dt as f32));
            if let Some(e) = &mut self.exit_dual {
                e.step(dt as f32);
                let (copy, _) = e.state();
                for w in self.effects.waves.iter_mut().filter(|w| w.z == EXIT_DUAL_WAVE_Z) {
                    w.center = (copy.x, copy.y);
                }
            }
            self.death_timer += dt as f32;
            if self.death_timer > if self.opts.faster_reset { 0.5 } else { 1.0 } {
                self.reset();
            }
            return;
        }
        if let Some(t) = &mut self.end_time {
            // The end animation (PlayLayer::playEndAnimationToPos): the player flies a 1 s curve
            // into the end portal; then the level restarts (there's no results screen yet).
            *t += dt as f32;
            for _ in 0..((dt * STEPS_PER_SECOND as f64).round() as usize) {
                self.camera.step_end(self.world.level_length, self.view_w);
            }
            if *t > 3.0 {
                self.reset();
            }
            return;
        }
        self.step_accumulator += dt;
        // Tolerance so float error (a 60 Hz period is 3.999999 steps after rounding) never turns a
        // 4-step frame into 3 + 5.
        const EPS: f64 = 1e-9;
        let mut steps_this_frame = 0;
        loop {
            // Each step stands for 1/60 s over its frame's step count: 1/240 s normally, less
            // while a time warp slows the level (the game runs more, shorter steps per frame).
            let step = self.world.real_step_secs();
            if self.step_accumulator + EPS < step {
                break;
            }
            self.step_accumulator -= step;
            steps_this_frame += 1;
            if let Some(&down) = self.autoplay.as_ref().and_then(|a| a.get(&self.step_count)) {
                self.holding = down;
            }
            self.step_count += 1;
            self.player.begin_step(&self.world);
            if self.holding != self.button_seen {
                // touch triggers (`GJEffectManager::playerButton` in `handleButton`)
                self.button_seen = self.holding;
                self.dual_touch = self.world.player_button(self.holding, false);
            }
            if self.holding != self.player.holding && !(self.dual_touch && self.holding) {
                self.player.set_button(self.holding, &self.world);
                // JumpPush / JumpRelease for event triggers
                self.world.player_events(&[if self.holding { 69 } else { 70 }]);
            }
            for (i, button) in [2u8, 3].into_iter().enumerate() {
                if self.direction[i] != self.direction_sent[i] {
                    self.direction_sent[i] = self.direction[i];
                    self.player.set_direction(self.direction[i], button);
                }
            }
            self.world.observe(self.player.dual.as_ref().map(|p| (p.x, p.y)), (self.player.camera.x, self.player.camera.y));
            self.world.pre_step((self.player.x, self.player.y));
            self.player.step(&self.world);
            self.world.post_step(&self.player);
            self.on_events();
            self.update_robot_anim();
            let cam_before = self.camera.x;
            let cam_prev = self.camera;
            self.player.step_camera(&self.world, self.view_w);
            self.world.catch_up(&self.player);
            self.camera = self.player.camera;
            self.scroll_bg(cam_prev);
            self.sync_art();
            self.step_shake();
            self.scroll_mg(self.camera.x - cam_before);
            self.colors.step(self.player.x);
            for order in self.world.fired_effects.drain(..) {
                self.colors.fire(order);
                self.fx.fire(order);
            }
            self.play_trigger_songs();
            self.play_trigger_sfx();
            self.apply_audio_edits();
            self.spawn_trigger_particles();
            self.checkpoint_trigger();
            self.take_coins();
            self.auto_checkpoint();
            self.step_wave_trail();
            if let Some(dt) = self.world.frame_end() {
                self.colors.frame(dt);
                self.fx.frame(dt, &self.world, &self.player);
                self.meter.step(self.audio.peak());
                self.audio.update(dt);
                self.step_trail();
            }
            // CADENCE_NOCLIP (stress runs): deaths don't count, the run goes on
            if self.player.dead && dev_flag("noclip") {
                self.player.dead = false;
            }
            if self.player.dead {
                // the practice music keeps playing through deaths
                if !self.practice {
                    self.audio.stop_music();
                }
                self.pending_checkpoint = None;
                self.audio.play_effect(crate::audio::DEATH_EFFECT);
                self.world.player_died();
                self.on_death();
                break;
            }
            // A spawned end trigger (platformer levels) ends the level like reaching the end.
            if self.player.finished || self.world.level_end {
                self.end_time = Some(0.0);
                self.end_from = (self.player.x, self.player.y);
                self.audio.play_effect(crate::audio::END_EFFECT);
                break;
            }
        }
        self.last_steps = steps_this_frame;
        if steps_this_frame != 4 && std::env::var_os("CADENCE_DEBUG_PACING").is_some() {
            log(&format!("pacing: {steps_this_frame} steps, dt {dt:.5}, period {period:.5}, acc {:.6}, step {:.6}, warp {}, x {}", self.step_accumulator, self.world.real_step_secs(), self.world.time_warp, self.player.x));
        }
        self.pacing[steps_this_frame.min(self.pacing.len() - 1)] += 1;
        if self.pacing.iter().sum::<u32>() >= 300 {
            log(&format!("pacing (frames by steps run 0..7): {:?}, period {:.2} ms", self.pacing, period * 1000.0));
            self.pacing = [0; 8];
        }
    }
}

/// Events delivered to the app from outside the event loop.
pub enum AppEvent {
    /// The renderer finished its asynchronous setup (on the web it can't be created blocking).
    RendererReady(Renderer),
}

pub struct App {
    files: Box<dyn Files>,
    /// Handed to the game once it's loaded.
    audio: Option<Box<dyn Audio>>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    proxy: EventLoopProxy<AppEvent>,
    level_id: i32,
    /// An online level being played (instead of `levels/{id}.txt`).
    online: Option<crate::ui::online::OnlineLevel>,
    /// The editor's level (decoded level string) while the editor is open: the game is built
    /// from it in editor mode (no physics, no player, editor-only objects shown).
    editor_level: Option<String>,
    /// CADENCE_UI_SCRIPT: scripted input for headless UI tests (one command a line: click X Y
    /// in points, key Name, wheel D, wait FRAMES, shot PATH), and the shot due this frame.
    script: Option<std::collections::VecDeque<String>>,
    script_shot: Option<PathBuf>,
    script_wait: u32,
    /// The "in a level without a game" fallback was logged.
    warned_no_game: bool,
    /// Held modifier keys, for the editor's wheel (shift scrolls sideways, ctrl/cmd zooms).
    mods: winit::keyboard::ModifiersState,
    quality: Quality,
    sheets: Option<Sheets>,
    renderer: Option<Renderer>,
    game: Option<Game>,
    window_created: bool,
    /// Benchmark: per-frame (interval ms, cpu ms, steps), and when to stop.
    bench: Option<(Vec<(f64, f64, usize, usize, usize)>, Instant, f64)>,
    last_draw: Option<Instant>,
    frames: u32,
    fps_time: Instant,
    /// The menus (crate::ui): which screen is up, the pause menu and end screen, the save.
    ui: crate::ui::Ui,
    ui_clock: Instant,
    /// Headless shots still to take (CADENCE_SHOTS), the decoded song and its time so far.
    shot_queue: Option<std::collections::VecDeque<u32>>,
    shot_song: Option<Option<(crate::audio::DecodedSong, f32)>>,
    shot_song_time: f32,
}

impl App {
    fn load_game(&mut self) -> Result<Game, String> {
        let text = if let Some(t) = &self.editor_level {
            t.clone()
        } else {
            // CADENCE_LEVEL_FILE=<path> (tests, benchmarks): the level's data from a file, so
            // repeated runs of an online level don't download it from the servers each time
            let from_file = std::env::var_os("CADENCE_LEVEL_FILE").and_then(|p| std::fs::read(p).ok());
            let bytes = match (from_file, self.online.as_ref().filter(|o| o.info.id == self.level_id)) {
                (Some(b), _) => b,
                (None, Some(o)) => o.data.clone(),
                (None, None) => {
                    let path = format!("levels/{}.txt", self.level_id);
                    self.files.read(&path).ok_or_else(|| format!("{path}: missing"))?
                }
            };
            decode_level_bytes(&bytes).map_err(|e| e.to_string())?
        };
        let in_editor = self.editor_level.is_some();
        let level = LevelString::parse(&text);
        // CADENCE_DUMP_HEADER (debug): the level's start settings and the objects near its start
        if std::env::var_os("CADENCE_DUMP_HEADER").is_some() {
            let header = text.split(';').next().unwrap_or_default();
            let fields: Vec<&str> = header.split(',').collect();
            let settings: Vec<String> = fields.chunks(2).filter(|c| c.len() == 2 && c[0].starts_with("kA") && c[0] != "kA14").map(|c| format!("{}={}", c[0], c[1])).collect();
            log(&format!("header: {}", settings.join(" ")));
            let mut near: Vec<(f32, i32, f32)> = level.objects.iter().filter(|o| o.x() < 400.0).map(|o| (o.x(), o.canonical_id(), o.y())).collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            log(&format!("objects near the start (x, id, y): {:?}", near.iter().take(60).collect::<Vec<_>>()));
            // CADENCE_DUMP_IDS=1520,1913: those objects in the first 3000 units, with their keys
            if let Ok(ids) = std::env::var("CADENCE_DUMP_IDS") {
                let ids: Vec<i32> = ids.split(',').filter_map(|v| v.parse().ok()).collect();
                for o in level.objects.iter().filter(|o| ids.contains(&o.canonical_id()) && o.x() < 3000.0) {
                    log(&format!("object {} at ({}, {}): {}", o.canonical_id(), o.x(), o.y(), o.props().map(|(k, v)| format!("{k:?}={v}")).collect::<Vec<_>>().join(" ")));
                }
            }
        }
        let mut world = World::from_level(&level);
        world.track_moves = true;
        // The level's camera triggers and settings (kA32 fixes the gravity bug) ride with the
        // player; restarts clone this starting player.
        let mut player = Player::spawn(&world);
        player.noclip = dev_flag("noclip");
        player.camera_triggers = cadence_core::camera::CameraTriggers::from_level(&level);
        let setting = |k: &str| level.setting(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        let art_default = [setting("kA6"), level_ground(&level) as i32, setting("kA25")];
        let art_preload: Vec<String> = player
            .camera_triggers
            .art_ids()
            .into_iter()
            .flat_map(|(kind, id)| match kind {
                0 => vec![bg_name(id)],
                1 => {
                    let (a, b) = ground_names(id);
                    std::iter::once(a).chain(b).collect()
                }
                _ => mg_art(id).map(|(a, b, _)| vec![a, b]).unwrap_or_default(),
            })
            .collect();
        player.apply_level_settings(&level);
        let mut icons = self.ui.icons();
        // CADENCE_DEATH_EFFECT=<1..20>: screenshots of a death effect (headless runs read no save)
        if let Some(e) = std::env::var("CADENCE_DEATH_EFFECT").ok().and_then(|v| v.parse::<u32>().ok()) {
            icons.ids[10] = e.clamp(1, 20);
        }
        let icon_bytes = { let (a, b) = icons.colors(); [a, b] };
        let colors = Colors::new(&level, icon_bytes);
        let mut order = Vec::new();
        for (i, obj) in level.objects.iter().enumerate() {
            // Triggers are editor-only markers; the game doesn't draw them while playing.
            if !in_editor && objects::info(obj.canonical_id()).is_some_and(|o| o.kind == objects::ObjectType::MODIFIER) {
                continue;
            }
            if let Some(v) = objects::visual(obj.canonical_id()) {
                // Editor-only objects (collision/force/damage blocks, area triggers, particle
                // emitters, text, counters...) use an editor button as their sprite, which the
                // game never shows in play. Text, counters and particle emitters draw something
                // else instead (not ported yet).
                // Keyframe objects (3032) are the editor's markers of an animation path.
                if !in_editor && ((v.sprites.first().is_some_and(|s| s.frame.starts_with("edit_")) && obj.id() != 914) || obj.id() == 3032) {
                    continue;
                }
                // Hidden objects (key 135, m_isHide) collide but are never drawn.
                if obj.get(135) == Some("1") {
                    continue;
                }
                let z_layer = obj.get_i32(24).unwrap_or(v.z_layer as i32);
                let z_order = obj.get_i32(25).unwrap_or(v.z_order as i32);
                order.push((z_layer, z_order, i));
            }
        }
        order.sort();
        let offset = level.setting("kA13").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let song = match self.online.as_ref().filter(|o| o.info.id == self.level_id) {
            Some(o) => o.song.clone(),
            None => crate::audio::level_song(self.level_id),
        }
        .map(|file| (file, offset));
        let audio = self.audio.take().unwrap_or_else(|| Box::new(NoAudio));
        let statics: Vec<ObjStatic> = level.objects.iter().map(ObjStatic::of).collect();
        let (columns, column0, movable) = culling_columns(&order, &statics, &world.can_move, &ui_objects(&level.objects));
        if dev_flag("debug_slow") {
            log(&format!("culling: {} objects, {} movable, {} columns", order.len(), movable.len(), columns.len()));
        }
        let mut game = Game {
            player_start: player.clone(),
            world,
            level: LevelVisual {
                statics: statics.clone(),
                columns,
                column0,
                movable,
                moved_columns: Default::default(),
                displaced: vec![false; level.objects.len()],
                moved_column: vec![0; level.objects.len()],
                moved_epoch: 0,
                place: {
                    let mut p = vec![0u32; level.objects.len()];
                    for (k, &(_, _, i)) in order.iter().enumerate() {
                        p[i] = k as u32;
                    }
                    p
                },
                order,
                // (moved in below, once nothing else reads the level: one copy, not two)
                objects: Vec::new(),
                spin: level.objects.iter().enumerate().map(|(i, o)| spin_rate(o, i)).collect(),

                particles: level
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.id() == 2065)
                    .filter_map(|(i, o)| Some((i, o.get(145)?.to_string())))
                    .collect(),
                hsv: level
                    .objects
                    .iter()
                    .map(|o| {
                        let hsv = |on, key| (o.get(on) == Some("1")).then(|| o.get(key).map_or(Hsv::IDENTITY, Hsv::parse));
                        (hsv(41, 43), hsv(42, 44))
                    })
                    .collect(),
                ui_objects: ui_objects(&level.objects),
                pulse_groups: {
                    let targets: std::collections::HashSet<i32> = colors.effect_groups().collect();
                    level
                        .objects
                        .iter()
                        .enumerate()
                        .filter_map(|(i, o)| {
                            let g = o.groups();
                            g.iter().any(|g| targets.contains(g)).then_some((i, g))
                        })
                        .collect()
                },
            },
            robot_desc: self.files.read("Robot_AnimDesc.plist").and_then(|b| crate::anim::AnimDesc::parse(&b).ok()),
            spider_desc: self.files.read("Spider_AnimDesc.plist").and_then(|b| crate::anim::AnimDesc::parse(&b).ok()),
            robot_anim: crate::anim::AnimState::default(),
            spider_orb_jump: false,
            spider_pop: f32::MAX,
            spider_dashes: Vec::new(),
            anim_objects: crate::animobj::AnimObjects::new(self.files.as_ref(), &level.objects),
            editor: self.editor_level.is_some(),
            shake_seen: (0.0, 0.0, 0.0),
            shake_left: 0.0,
            shake_next: 0.0,
            shake_offset: (0.0, 0.0),
            colors_start: colors.clone(),
            fx: crate::fx::LevelFx::new(&level),
            colors,
            player,
            camera: Camera::new(),
            view_w: VIEW_HEIGHT * 16.0 / 9.0,
            refresh_period: 0.0,
            recent_frames: Vec::new(),
            pacing: [0; 8],
            autoplay: None,
            step_count: 0,
            last_steps: 0,
            tick_ms: 0.0,
            draw_capacity: 0,
            draw_cache: DrawCache::default(),
            enter_locks: Vec::new(),
            enter_epoch: 0,
            step_accumulator: 0.0,
            last_frame: Instant::now(),
            holding: false,
            button_seen: false,
            dual_touch: false,
            direction: [false; 2],
            direction_sent: [false; 2],
            death_timer: 0.0,
            death_fade: None,
            exit_dual: None,
            audio,
            song,
            bg_pos: [0.0; 2],
            end_time: None,
            end_from: (0.0, 0.0),
            particle_defs: ParticleLibrary::load(self.files.as_ref()),
            effects: Effects::default(),
            player_fx: PlayerFx::new(icon_bytes[0].map(|v| v as f32 / 255.0)),
            coins: level
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| is_coin(o.id()))
                .map(|(i, o)| {
                    let half = 20.0 * o.get_f32(32).unwrap_or(1.0);
                    let (x, y) = (o.x(), o.y() + GROUND_OFFSET);
                    (i, cadence_core::world::Rect { x: x - half, y: y - half, w: 2.0 * half, h: 2.0 * half })
                })
                .collect(),
            coins_taken: Vec::new(),
            clock: 0.0,
            meter: Meter::default(),
            attempt: 1,
            trail: crate::trail::MotionStreak::new(0.3, 5.0, 10.0),
            trail_pos: (0.0, 0.0),
            wave_trail: crate::trail::WaveTrail::default(),
            bg_image: bg_name(art_default[0]),
            ground_image: ground_names(art_default[1]).0,
            ground2_image: ground_names(art_default[1]).1,
            mg: mg_art(art_default[2]),
            art_default,
            art_shown: [0; 3],
            art_preload,
            mg_x: 0.0,
            practice: false,
            checkpoints: Vec::new(),
            platformer_checkpoint: None,
            music_channel: 0,
            pending_checkpoint: None,
            opts: PlayOptions::default(),
            icons,
            pcol: (icon_bytes[0].map(|v| v as f32 / 255.0), icon_bytes[1].map(|v| v as f32 / 255.0)),
            ball_frame: std::env::var("CADENCE_BALL").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                let nanos = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
                1 + nanos % 3
            }),
        };
        game.level.objects = level.objects;
        Ok(game)
    }

    fn draw(&mut self) {
        // CADENCE_EDITOR_LEVEL=<file>: screenshots of the editor on these objects (a level
        // string, header optional), nothing selected, the BPM markers on, the view's bottom left
        // at CADENCE_EDITOR_CAM=x,y (zoom 1); applied once, when the editor opens
        static EDITOR_LEVEL: std::sync::Once = std::sync::Once::new();
        if let Some(e) = self.ui.editor.as_mut().filter(|_| !EDITOR_LEVEL.is_completed()) {
            EDITOR_LEVEL.call_once(|| {
                let Some(text) = std::env::var("CADENCE_EDITOR_LEVEL").ok().and_then(|p| std::fs::read_to_string(p).ok()) else { return };
                let text = if text.contains("kA") { text } else { format!(";{}", text.trim()) };
                e.objects = crate::ui::editor::Editor::from_level_string(&e.name, &text).objects;
                e.selected.clear();
                e.show_bpm_markers = true;
                e.dirty = true;
                if let Some((x, y)) = std::env::var("CADENCE_EDITOR_CAM").ok().and_then(|v| v.split_once(',').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))) {
                    e.cam = (x, y);
                    e.zoom = 1.0;
                }
            });
        }
        if self.ui.editor_dirty() {
            self.rebuild_editor();
        }
        if let Some(g) = self.game.as_mut() {
            g.opts = PlayOptions::from_progress(&self.ui.progress);
        }
        // The level's background and ground art, loaded the first time it's drawn.
        if let (Some(renderer), Some(sheets), Some(game)) = (self.renderer.as_mut(), self.sheets.as_mut(), self.game.as_ref()) {
            for name in game.icons.sheets() {
                let loaded = sheets.frames.contains_key(&format!("{name}_001.png")) || sheets.frames.contains_key(&format!("{name}_01_001.png"));
                if !loaded && !sheets.failed.contains(&name) {
                    let before = sheets.images.len();
                    match sheets.load(self.files.as_ref(), "icons/", &name) {
                        Ok(()) => {
                            upload(renderer, &mut sheets.images[before..]);
                        }
                        Err(_) => {
                            sheets.failed.insert(name);
                        }
                    }
                }
            }
            // the chosen death effect's sheet (`GameManager::loadDeathEffect`), only that one
            let effect = game.icons.ids[10];
            if let (Some(name), Some(probe)) = (crate::death::sheet(effect), crate::death::probe_frame(effect)) {
                if !sheets.frames.contains_key(&probe) && !sheets.failed.contains(&name) {
                    let before = sheets.images.len();
                    match sheets.load(self.files.as_ref(), "", &name) {
                        Ok(()) => upload(renderer, &mut sheets.images[before..]),
                        Err(e) => {
                            log(&format!("{name}: {e}"));
                            sheets.failed.insert(name);
                        }
                    }
                }
            }
            let mg = game.mg.as_ref().map(|m| [&m.0, &m.1]).into_iter().flatten();
            let art = [Some(&game.bg_image), Some(&game.ground_image), game.ground2_image.as_ref()].into_iter().flatten().chain(mg);
            for name in art.chain(&game.art_preload) {
                if !sheets.frames.contains_key(&format!("{name}.png")) {
                    let before = sheets.images.len();
                    if dev_flag("debug_slow") {
                        log(&format!("loading {name} mid-level"));
                    }
                    match sheets.load_single(self.files.as_ref(), name) {
                        Ok(()) => {
                            upload(renderer, &mut sheets.images[before..]);
                        }
                        Err(e) => log(&format!("{name}: {e}")),
                    }
                }
            }
        }
        self.upload_pictures();
        let frame_start = Instant::now();
        let interval = self.last_draw.map(|t| frame_start.duration_since(t).as_secs_f64() * 1000.0);
        self.last_draw = Some(frame_start);
        let (Some(renderer), Some(sheets), Some(game)) = (self.renderer.as_mut(), self.sheets.as_ref(), self.game.as_mut()) else {
            return;
        };
        if self.bench.is_some() && game.autoplay.is_none() {
            game.autoplay = Some(
                std::env::var_os("CADENCE_SPEC")
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .map(|t| cadence_core::trace::Spec::parse(&t).attempts.swap_remove(0))
                    .unwrap_or_default(),
            );
        }
        // CADENCE_SHOTS=a,b,c: one run saves a frame at each of these steps (CADENCE_SCREENSHOT
        // with %d for the step); otherwise one frame at CADENCE_STEPS.
        if self.shot_queue.is_none() {
            self.shot_queue = Some(match std::env::var("CADENCE_SHOTS") {
                Ok(list) => list.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
                Err(_) => std::env::var("CADENCE_STEPS").ok().and_then(|s| s.parse().ok()).into_iter().collect(),
            });
        }
        let shot_step = self.shot_queue.as_mut().and_then(|q| q.pop_front());
        let screenshot = if self.script.is_some() {
            self.script_shot.take()
        } else {
            std::env::var_os("CADENCE_SCREENSHOT").map(|p| PathBuf::from(p.to_string_lossy().replace("%d", &shot_step.unwrap_or(0).to_string())))
        };
        let scripted = self.script.is_some();
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
        let more_shots = self.shot_queue.as_ref().is_some_and(|q| !q.is_empty());
        let (w, h) = renderer.size();
        let view_w = VIEW_HEIGHT * w as f32 / h as f32;
        game.view_w = view_w;
        if screenshot.is_some() && !game.editor && !scripted {
            let steps: u32 = shot_step.unwrap_or(0);
            // The song, decoded once so music-pulsing objects pulse as in a real run.
            if self.shot_song.is_none() {
                self.shot_song = Some(game.song.as_ref().and_then(|(file, offset)| Some((crate::audio::DecodedSong::decode(self.files.as_ref(), file)?, *offset))));
            }
            let song = self.shot_song.as_ref().and_then(|s| s.as_ref());
            let mut song_time = self.shot_song_time;
            // The harness runs `speed` game updates per rendered frame while the music plays in
            // real time, so match its song position when comparing with its screenshots.
            let song_speed = std::env::var_os("CADENCE_SPEC")
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map_or(1, |t| cadence_core::trace::Spec::parse(&t).speed.max(1)) as f32;
            // The harness runs `speed` updates per rendered frame, and the cocos scheduler (timed
            // triggers) ticks once per rendered frame: match it, as `cadence verify` does.
            if std::env::var_os("CADENCE_SPEC").is_some() {
                game.world.sched_every = song_speed as u32;
            }
            // Optional inputs: the first attempt of a harness spec.
            let inputs = std::env::var_os("CADENCE_SPEC")
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| cadence_core::trace::Spec::parse(&t).buttons.swap_remove(0))
                .unwrap_or_default();
            for step in game.step_count..steps {
                game.player.begin_step(&game.world);
                // Every button event of the step in spec order: 1 jumps, 2/3 are left/right.
                for &(down, button) in inputs.get(&step).into_iter().flatten() {
                    if button == 1 {
                        if !game.world.player_button(down, false) || !down {
                            game.player.set_button(down, &game.world);
                        }
                    } else {
                        game.player.set_direction(down, button);
                    }
                }
                game.world.observe(game.player.dual.as_ref().map(|p| (p.x, p.y)), (game.player.camera.x, game.player.camera.y));
                game.world.pre_step((game.player.x, game.player.y));
                game.player.step(&game.world);
                game.world.post_step(&game.player);
                game.on_events();
                game.update_robot_anim();
                game.effects.update(1.0 / STEPS_PER_SECOND as f32);
                game.player_fx.update(1.0 / STEPS_PER_SECOND as f32);
                let cam_before = game.camera.x;
                let cam_prev = game.camera;
                game.player.step_camera(&game.world, game.view_w);
                game.world.catch_up(&game.player);
                game.camera = game.player.camera;
                game.scroll_bg(cam_prev);
                game.sync_art();
                game.step_shake();
                game.scroll_mg(game.camera.x - cam_before);
                game.colors.step(game.player.x);
                for order in game.world.fired_effects.drain(..) {
                    game.colors.fire(order);
                    game.fx.fire(order);
                }
                game.step_count += 1;
                game.auto_checkpoint();
                game.step_wave_trail();
                if let Some(dt) = game.world.frame_end() {
                    game.colors.frame(dt);
                    game.fx.frame(dt, &game.world, &game.player);
                    // Music runs in real time: a frame is 1/60 s whatever the time warp.
                    song_time += 1.0 / 60.0 / song_speed;
                    game.step_trail();
                    if let Some((song, offset)) = &song {
                        game.meter.step(song.peak(offset + song_time));
                    }
                }
                if let Some(at) = std::env::var("CADENCE_DEBUG_STATE").ok().and_then(|v| v.parse::<u32>().ok()) {
                    if step + 1 >= at && step < at + 40 {
                        let p = &game.player;
                        log(&format!("state {step} x {} y {} yvel {} mode {:?} grav {}", p.x, p.y, p.y_velocity, p.mode, p.upside_down));
                    }
                }
                if game.player.dead && dev_flag("noclip") {
                    game.player.dead = false;
                }
                if game.player.dead {
                    log(&format!("screenshot run died at step {step}: x {} y {}", game.player.x, game.player.y));
                    game.on_death();
                    self.shot_queue = Some(Default::default());
                    break;
                }
            }
            self.shot_song_time = song_time;
            if let Ok(ids) = std::env::var("CADENCE_DEBUG_COLORS") {
                for id in ids.split(',').filter_map(|v| v.parse::<i32>().ok()) {
                    log(&format!("channel {id}: {:?} alpha {}", game.colors.active(id), game.colors.object_opacity(id, &[])));
                }
            }
            log(&format!("checkpoints {:?}", game.checkpoints.iter().map(|c| c.player.x as i32).collect::<Vec<_>>()));
            log(&format!("screenshot meter {:.4} song time {:.3} level length {} camera {:?} player cam {:?} player {:?}", game.meter.value, song_time, game.world.level_length, (game.camera.x, game.camera.y, game.camera.zoom), (game.player.camera.x, game.player.camera.y), (game.player.x, game.player.y)));
            // Optional: advance effects this many seconds (60 Hz frames) after the last step.
            let fx: f32 = std::env::var("CADENCE_FX").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            for _ in 0..(fx * 60.0).round() as u32 {
                game.effects.update(1.0 / 60.0);
                game.player_fx.update(1.0 / 60.0);
            }
            // object effects come into view already running
            game.effects.prewarm = 1.0;

        } else if game.editor {
            // the editor: no steps; its camera (the view's bottom left and zoom)
            game.last_frame = Instant::now();
            if let Some((x, y, zoom)) = self.ui.editor_camera(game.view_w) {
                game.camera.x = x;
                game.camera.y = y;
                game.camera.zoom = zoom;
                // the floor at world y 90 (screen units from the camera), no ceiling
                game.camera.ground_bottom = (90.0 - y) * zoom;
                game.camera.ground_top = VIEW_HEIGHT + 100.0;
            }
        } else if self.ui.level_running() {
            // The end screen holds the completed level at the end of its animation.
            if self.ui.hold_end()
                && let Some(t) = game.end_time.as_mut()
            {
                *t = t.min(2.9);
            }
            let tick_start = Instant::now();
            game.tick(Instant::now());
            game.tick_ms = tick_start.elapsed().as_secs_f64() * 1000.0;
        } else {
            game.last_frame = Instant::now(); // paused: no catch-up on resume
        }
        let ui_dt = self.ui_clock.elapsed().as_secs_f32().min(0.1);
        self.ui_clock = Instant::now();
        self.ui.level_frame(level_status(game), ui_dt);
        if game.refresh_period == 0.0 {
            game.refresh_period = renderer
                .window
                .current_monitor()
                .and_then(|m| m.refresh_rate_millihertz())
                .map_or(-1.0, |mhz| 1000.0 / mhz as f64);
        }
        // A new shatter captures the dead icon (CCRenderTexture) before this frame is drawn.
        let points_to_pixels = h as f32 / VIEW_HEIGHT;
        let rig_parts: Option<Vec<crate::anim::Part>> = game.robot_rig().map(|p| p.to_vec());
        if let Some(sh) = game.effects.shatters.iter_mut().find(|s| s.sheet.is_none()) {
            let mut icon = Vec::new();
            let half = sh.size * 0.5;
            player_draws(sheets, &|s| renderer.atlas_size(s), &game.player, (half, half), game.player.rotation, game.pcol, &game.icons, rig_parts.as_deref(), 1.0, &mut icon);
            let px = (sh.size * points_to_pixels).round() as u32;
            sh.sheet = Some(renderer.capture(&icon, (0.0, 0.0, sh.size, sh.size), px));
        }
        let p = &game.player;
        // shake triggers (`Game::step_shake`)
        let (cam_x, cam_y) = (game.camera.x + game.shake_offset.0, game.camera.y + game.shake_offset.1);
        // Zoom (camera trigger 1913): the view covers the window's size divided by the zoom.
        let zoom = if game.camera.zoom > 0.0 { game.camera.zoom } else { 1.0 };
        let view_w = view_w / zoom;
        let view_h = VIEW_HEIGHT / zoom;
        // one framebuffer pixel in level units (line widths)
        let pixel = view_h / renderer.size().1.max(1) as f32;
        let fonts = &self.ui.fonts;
        // sized like last frame's, so a big level's list doesn't regrow (and copy) mid-frame
        let mut draws: Vec<Draw> = Vec::with_capacity(game.draw_capacity.max(4096));
        // CADENCE_DUMP_DRAWS=<file>: every sprite drawn (frame, center, alpha), for tools/sprite_diff.py
        let dumped: std::cell::RefCell<Option<Vec<(String, f32, f32, f32, f32)>>> =
            std::cell::RefCell::new(std::env::var_os("CADENCE_DUMP_DRAWS").map(|_| Vec::new()));
        let push = |name: &str, transform: Affine, flip_x: bool, flip_y: bool, color: [f32; 4], blend: Blend, draws: &mut Vec<Draw>| {
            if let Some(frame) = sheets.frames.get(name) {
                let atlas = renderer.atlas_size(frame.sheet);
                draws.push(Draw { sheet: frame.sheet, blend, instance: sprite_instance(frame, atlas, transform, flip_x, flip_y, color) });
                if let Some(d) = dumped.borrow_mut().as_mut() {
                    let (w, h) = frame.source_size;
                    let t = transform;
                    d.push((format!("{name}"), t[0] * w * 0.5 + t[2] * h * 0.5 + t[4], t[1] * w * 0.5 + t[3] * h * 0.5 + t[5], color[3], t[1].atan2(t[0]).to_degrees()));
                }
            }
        };
        let frame_size = |name: &str| sheets.frames.get(name).map(|f| f.source_size);
        // `push` with the frame already looked up (the object loop resolves each object type's
        // frames once per frame, not by name for every sprite)
        let push_frame = |frame: &crate::assets::Frame, name: &str, transform: Affine, flip_x: bool, flip_y: bool, color: [f32; 4], blend: Blend, draws: &mut Vec<Draw>| {
            let atlas = renderer.atlas_size(frame.sheet);
            draws.push(Draw { sheet: frame.sheet, blend, instance: sprite_instance(frame, atlas, transform, flip_x, flip_y, color) });
            if let Some(d) = dumped.borrow_mut().as_mut() {
                let (w, h) = frame.source_size;
                let t = transform;
                d.push((name.to_string(), t[0] * w * 0.5 + t[2] * h * 0.5 + t[4], t[1] * w * 0.5 + t[3] * h * 0.5 + t[5], color[3], t[1].atan2(t[0]).to_degrees()));
            }
        };

        // Background (screen space, opaque): scaled to view_width/480, at `bg_pos` (screen
        // units, so ÷ zoom here), x wrapped into (−width, 0] (the game's m_background).
        let bg = game.colors.display(1000);
        let bg_frame = format!("{}.png", game.bg_image);
        if let Some((bw, _)) = frame_size(&bg_frame) {
            let scale = view_w / 480.0;
            let width = bw * scale;
            let mut x = (game.bg_pos[0] / zoom).rem_euclid(width);
            if x > 0.0 {
                x -= width;
            }
            let y = game.bg_pos[1] / zoom;
            while x < view_w {
                let t = [scale, 0.0, 0.0, scale, cam_x + x, cam_y + y];
                push(&bg_frame, t, false, false, tint(bg, 1.0), Blend::Opaque, &mut draws);
                x += width;
            }
        }
        // The editor shows nothing under its ground layer (one tile tall): black from the tile's
        // bottom down.
        if game.editor {
            let gf = format!("{}.png", game.ground_image);
            if let (Some((_, th)), Some(frame)) = (frame_size(&gf), sheets.frames.get(PARTICLE_TEXTURE)) {
                let top = 90.0 - th;
                if top > cam_y {
                    let atlas = renderer.atlas_size(frame.sheet);
                    let mut inst = sprite_instance(frame, atlas, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], false, false, tint([0.0; 3], 1.0));
                    let (u, v) = ((inst.uv[0] + inst.uv[6]) * 0.5, (inst.uv[1] + inst.uv[7]) * 0.5);
                    inst.uv = [u, v, u, v, u, v, u, v];
                    inst.m = [view_w + 2.0, 0.0, 0.0, (top - cam_y).min(view_h), cam_x - 1.0, cam_y];
                    draws.push(Draw { sheet: frame.sheet, blend: Blend::Opaque, instance: inst });
                }
            }
        }
        let bg_end = draws.len();

        // The middle ground (z −39, over the background): tiles of fg_NN / fg_NN_2 in screen
        // units, node at y = (mgY + offset)·((1 − f) + f·zoom) − f·camera y (`updateCameraBGArt`;
        // offset from middle-ground triggers, f = +0x320: 0.5, which fits every real Dash frame
        // to 0.001) and scaled by `mg_scale`.
        if let Some((a, b, base_y)) = &game.mg {
            let s = game.mg_scale();
            // f: the MG speed's y factor (MG speed triggers 3612; 0.5 by default)
            let f = game.world.mg_speed.1;
            let node_y = (base_y + game.camera.mg_offset) * ((1.0 - f) + f * zoom) - f * game.camera.y;
            let (fa, fb) = (format!("{a}.png"), format!("{b}.png"));
            let (c1, c2) = (game.colors.display(1013), game.colors.display(1014));
            for (frame, color) in [(&fa, c1), (&fb, c2)] {
                if let Some((w, h)) = frame_size(frame) {
                    let mut x = game.mg_x;
                    while x * s < VIEW_HEIGHT * 4.0 && (x * s) / zoom < view_w + w * s / zoom {
                        // screen units → world: camera + screen / zoom.
                        let (wx, wy) = (cam_x + x * s / zoom, cam_y + node_y / zoom);
                        let t = [s / zoom, 0.0, 0.0, s / zoom, wx, wy];
                        let t = mul(t, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                        let _ = h;
                        push(frame, t, false, false, tint(color, 1.0), Blend::Normal, &mut draws);
                        x += w;
                    }
                }
            }
        }
        let world_start = draws.len();
        let t_objects = Instant::now();
        // Where the shader/gradient z layers start in `draws` (fx.rs).
        let mut marks = crate::fx::LayerMarks::default();

        // Level objects, sorted by z, culled to the view. Glows go first, additive, under objects.
        // Breakable blocks the player broke this attempt are gone.
        let broken: crate::fxhash::FxSet<u32> = game.player.destroyed.iter().map(|&w| game.world.objects[w].order).collect();
        // the editor's shown layer (objects off it are dimmed)
        let edit_layer = if game.editor { self.ui.editor_layer() } else { None };
        // Culling: the x columns in view (static objects) and every object that can move, by
        // their place in the draw order (sorted back into it), 120 units past each edge.
        let (left, right) = (cam_x - 120.0, cam_x + view_w + 120.0);
        let t_cull = Instant::now();
        let mut ks: Vec<u32> = Vec::new();
        game.level.track_moves(&mut game.world);
        {
            let lv = &game.level;
            let first = ((left / COLUMN).floor() as i32 - lv.column0).max(0) as usize;
            let last = ((right / COLUMN).floor() as i32 - lv.column0 + 1).max(0) as usize;
            for column in lv.columns.iter().take(last.min(lv.columns.len())).skip(first) {
                ks.extend(column.iter().copied().filter(|&k| {
                    let i = lv.order[k as usize].2;
                    let x = lv.statics[i].x;
                    !lv.displaced[i] && x >= left && x <= right
                }));
            }
            // moved objects, by the column they are in now
            for c in (left / COLUMN).floor() as i32..=(right / COLUMN).floor() as i32 {
                if let Some(column) = lv.moved_columns.get(&c) {
                    ks.extend(column.iter().copied().filter(|&k| {
                        let i = lv.order[k as usize].2;
                        let x = lv.statics[i].x + game.world.moved[i].0 as f32;
                        x >= left && x <= right
                    }));
                }
            }
            ks.extend(lv.movable.iter().copied().filter(|&k| {
                let i = lv.order[k as usize].2;
                let x = lv.statics[i].x + game.world.moved[i].0 as f32;
                (x >= left && x <= right) || lv.ui_objects.contains_key(&i)
            }));
        }
        ks.sort_unstable();
        // (a UI-pinned object that triggers move is in both `movable` and the moved columns)
        ks.dedup();
        if dev_flag("debug_slow") && game.step_count % 240 < 4 {
            log(&format!("visit {} objects ({} candidates in view)", ks.len(), ks.len()));
        }
        // CADENCE_CHECK_CULL (tests): the same set the slow way, every object checked
        if dev_flag("check_cull") {
            let lv = &game.level;
            let all: Vec<u32> = (0..lv.order.len() as u32)
                .filter(|&k| {
                    let i = lv.order[k as usize].2;
                    let x = lv.statics[i].x + game.world.moved[i].0 as f32;
                    (x >= left && x <= right) || lv.ui_objects.contains_key(&i)
                })
                .collect();
            if all != ks {
                log(&format!("CULL MISMATCH at step {}: {} vs {} objects", game.step_count, ks.len(), all.len()));
            }
        }
        let visible: Vec<(i32, usize)> = ks
            .into_iter()
            .map(|k| {
                let (z_layer, _, index) = game.level.order[k as usize];
                (z_layer, index)
            })
            .filter(|&(_, i)| game.world.enabled[i] && !broken.contains(&(i as u32)))
            .collect();
        let cull_ms = t_cull.elapsed().as_secs_f64() * 1000.0;
        // Two passes, like the game's object layer: the B layers (z layer ≤ 3, plus portal backs,
        // which the game puts in B1) under the objects' particle effects and the player; the T
        // layers go over the player (drawn below).
        let mut below: Vec<Draw> = Vec::with_capacity(game.draw_capacity / 2);
        // Portal backs: the game adds each portal's back half as its own object, in the last B
        // batch (z −5), so they sit over every other B object and under the particles.
        let mut backs: Vec<Draw> = Vec::new();
        let mut object_draws: Vec<Draw> = Vec::with_capacity(game.draw_capacity / 2);
        let mut effect_objects = Vec::new();
        // Within a z layer the game draws its blending batch node (additive) under the normal
        // one (`GJBaseGameLayer::setupLayers`: blend node z = normal z − 20). When only one of an
        // object's main and detail sprites blends, that one moves a layer up
        // (`addMainSpriteToParent` 0x5af6b8 / `addColorSpriteToParent` 0x5a8714, picked by
        // m_colorZLayerRelated, `color_z::COLOR_Z_RELATED`): the one that moves goes over the
        // layer (`up`, like the next layer's blend node), the other stays in the layer's blend pass.
        let (mut layer_blend, mut layer_normal, mut layer_up) = (Vec::new(), Vec::new(), Vec::new());
        let mut layer: Option<bool> = None;
        let mut last_z = i32::MIN;
        // A z layer is a set of batch nodes, one per texture (`setupLayers` 0xce858): the
        // GJ_GameSheetGlow node lowest (about 230 below the layer), then FireSheet_01, then
        // GJ_GameSheet (blending 20 under normal). So within a layer, the sprites from the glow
        // sheet draw first and the fire sheet's next, each in object order; then the rest.
        // (Drawn per object they would also split the layer into a draw call per object.)
        let regroup = std::env::var_os("CADENCE_NO_REGROUP").is_none();
        let glow_sheet = sheets.frames.get("blackCogwheel_01_glow_001.png").map(|f| f.sheet).filter(|_| regroup);
        let fire_sheet = sheets.frames.get("Fire_01_2_looped_001.png").map(|f| f.sheet).filter(|_| regroup);
        let flush = |blend: &mut Vec<Draw>, normal: &mut Vec<Draw>, up: &mut Vec<Draw>, out: &mut Vec<Draw>| {
            for sheet in [glow_sheet, fire_sheet].into_iter().flatten() {
                for list in [&*blend, &*normal] {
                    out.extend(list.iter().filter(|d| d.sheet == sheet).cloned());
                }
            }
            let other = |d: &Draw| Some(d.sheet) != glow_sheet && Some(d.sheet) != fire_sheet;
            for list in [blend, normal] {
                out.extend(list.drain(..).filter(|d| other(d)));
            }
            // (the parts that go a layer up are the next layer's: as they were)
            out.append(up);
        };
        let debug_obj = std::env::var("CADENCE_DEBUG_OBJ").ok().and_then(|v| v.parse::<i32>().ok());
        let prof = dev_flag("debug_slow");
        // Enter effects (cadence_core::enter): each object's lock, cleared on a restart; how far
        // in from the view's edges an effect can reach (custom effects have their own lengths).
        if game.enter_epoch != game.world.epoch || game.enter_locks.len() != game.level.objects.len() {
            game.enter_epoch = game.world.epoch;
            game.enter_locks = vec![Default::default(); game.level.objects.len()];
        }
        let enter_margin = game.world.enter.custom.iter().flatten().flatten().fold(70.0f32, |m, e| {
            m.max(e.length.abs() + e.length_var.abs() + e.offset.abs() + e.offset_var.abs() + 1.0)
        });
        let enter_view = cadence_core::enter::EnterView { left: cam_x, width: view_w, cam_y, win_h: VIEW_HEIGHT };
        // per object type (static sprite trees only): each sprite's frame, the draw order (by z
        // under/over the parent) and whether it is a portal back
        let mut types: crate::fxhash::FxMap<i32, TypeSprites> = Default::default();
        let mut portal_backs: crate::fxhash::FxMap<&str, Option<(String, crate::assets::Frame)>> = Default::default();
        let mut transforms: Vec<Affine> = Vec::with_capacity(16);
        // Static objects' sprites from earlier frames (DrawCache); not in the editor (objects
        // change, layers dim) or while sprites are being dumped or debugged.
        let cache_on = !game.editor && debug_obj.is_none() && dumped.borrow().is_none() && !dev_flag("nocache");
        if cache_on {
            game.draw_cache.sync(&game.colors, game.level.objects.len());
        }
        for &(z_layer, index) in &visible {
            let obj = &game.level.objects[index];
            let st = game.level.statics[index];
            // Move triggers move objects in f64; the game draws the f32 of that.
            let (mx, my) = game.world.moved[index];
            let (ox, oy) = match game.level.ui_objects.get(&index) {
                // pinned to the screen by a UI trigger (moves still apply)
                Some(pin) => {
                    let (sx, sy) = pin.point(view_w * zoom, VIEW_HEIGHT);
                    (cam_x + sx / zoom + mx as f32, cam_y + sy / zoom + my as f32)
                }
                None => ((st.x as f64 + mx) as f32, ((st.y + GROUND_OFFSET) as f64 + my) as f32),
            };
            if st.id == 1615 && !game.editor {
                // A counter label (`LabelGameObject`): item 80's count, or with 466 its timer's
                // seconds (two decimals; 389 whole seconds), in bigFont centered on the object.
                if let Some(font) = fonts.first() {
                    let id = obj.get_i32(80).unwrap_or(0);
                    let text = if obj.get(466) == Some("1") {
                        let secs = game.world.timers.get(&id).map_or(0.0, |t| t.0);
                        if obj.get(389) == Some("1") { format!("{}", secs.floor() as i64) } else { format!("{secs:.2}") }
                    } else {
                        game.world.triggers.items.get(&id).copied().unwrap_or(0).to_string()
                    };
                    let (glyphs, (w, h)) = font.layout(&text);
                    let scale = obj.get_f32(32).unwrap_or(1.0);
                    let node = node_transform(ox, oy, obj.get_f32(6).unwrap_or(0.0), scale, scale, 0.5, 0.5, w, h);
                    let channel = obj.get_i32(21).unwrap_or(1);
                    let groups: &[i32] = game.level.pulse_groups.get(&index).map_or(&[], Vec::as_slice);
                    let color = game.colors.object_color(channel, game.colors.active(channel), groups, None);
                    let alpha = game.colors.object_opacity(channel, groups);
                    for g in glyphs {
                        if let Some((gw, gh)) = frame_size(&g.frame) {
                            let t = mul(node, node_transform(g.x, g.y, 0.0, 1.0, 1.0, 0.5, 0.5, gw, gh));
                            push(&g.frame, t, false, false, tint(color, alpha), Blend::Normal, &mut layer_normal);
                        }
                    }
                }
                continue;
            }
            if st.id == 914 {
                // A text object (`TextGameObject`): its text (key 31, base64) as a bitmap-font
                // label centered on the object, in its color, scale, rotation and flips. (The
                // level's font kA18 > 0 picks gjFontNN; only bigFont is loaded so far.)
                if let (Some(font), Some(text)) = (fonts.first(), obj.get(31)) {
                    let text = cadence_core::codec::base64_decode(text).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                    let (glyphs, (w, h)) = font.layout(&text);
                    let scale = obj.get_f32(32).unwrap_or(1.0);
                    let sx = obj.get_f32(128).unwrap_or(scale) * if obj.get(4) == Some("1") { -1.0 } else { 1.0 };
                    let sy = obj.get_f32(129).unwrap_or(scale) * if obj.get(5) == Some("1") { -1.0 } else { 1.0 };
                    let node = node_transform(ox, oy, obj.get_f32(6).unwrap_or(0.0), sx, sy, 0.5, 0.5, w, h);
                    let channel = obj.get_i32(21).unwrap_or(1004);
                    let groups: &[i32] = game.level.pulse_groups.get(&index).map_or(&[], Vec::as_slice);
                    let color = game.colors.object_color(channel, game.colors.active(channel), groups, None);
                    let alpha = game.colors.object_opacity(channel, groups);
                    for g in glyphs {
                        if let Some((gw, gh)) = frame_size(&g.frame) {
                            let t = mul(node, node_transform(g.x, g.y, 0.0, 1.0, 1.0, 0.5, 0.5, gw, gh));
                            push(&g.frame, t, false, false, tint(color, alpha), Blend::Normal, &mut layer_normal);
                        }
                    }
                }
                continue;
            }
            if is_coin(st.id) {
                // A spinning coin: 4 frames, 0.05 s each, looped (`GameObject::createWithKey`
                // 0x1f26dc). Taken coins disappear (the pickup animation isn't ported yet).
                if !game.coins_taken.iter().any(|c| c.0 == index) {
                    let prefix = if st.id == 1329 { "secretCoin_2_01" } else { "secretCoin_01" };
                    let frame = format!("{prefix}_{:03}.png", (game.clock / 0.05) as usize % 4 + 1);
                    if let Some((fw, fh)) = frame_size(&frame) {
                        let scale = obj.get_f32(32).unwrap_or(1.0);
                        let t = node_transform(ox, oy, obj.get_f32(6).unwrap_or(0.0), scale, scale, 0.5, 0.5, fw, fh);
                        push(&frame, t, false, false, tint([1.0; 3], 1.0), Blend::Normal, &mut layer_normal);
                    }
                }
                continue;
            }
            let Some(visual) = objects::visual(st.canonical) else { continue };
            // Animated objects swap in this frame's sprite tree (animobj.rs).
            let tb0 = if prof { Some(Instant::now()) } else { None };
            let anim_sprites = game.anim_objects.sprites(index, st.canonical, visual, game.player.total_time, game.world.animated_at[index], &frame_size);
            let color_child = if anim_sprites.is_none() { crate::animobj::with_color_child(visual, &frame_size) } else { None };
            let visual = &match color_child {
                Some(list) => crate::animobj::View::with_sprites(visual, list),
                None => crate::animobj::View::of(visual, anim_sprites.as_deref(), game.colors.active(1000)),
            };
            let tb1 = tb0.map(|_| Instant::now());
            let scale = st.scale;
            // Music pulse: a relative scale on top of the object's own (`setRScale`).
            let pulse = match audio_scale(st.canonical) {
                Some((_, true)) => (game.meter.value + 0.3).min(1.2),
                Some((Some((min, max)), false)) => min + (max - min) * (game.meter.value - 0.1),
                Some((None, false)) => game.meter.value,
                None => 1.0,
            };
            // scale triggers' factor on top (World::scaled)
            let (gx, gy) = game.world.scaled[index];
            let (ax, ay) = game.world.area.scale[index];
            let (gx, gy) = (gx * ax, gy * ay);
            let sx = gx * pulse * st.scale_x.unwrap_or(scale) * if st.flip_x { -1.0 } else { 1.0 };
            let sy = gy * pulse * st.scale_y.unwrap_or(scale) * if st.flip_y { -1.0 } else { 1.0 };
            // The green and black orbs (1022, 1330) are a still ring under a spinning copy of
            // itself: only the child and the glow turn (real dumps: RingObjects never rotate).
            let spin = game.level.spin[index] * game.clock;
            let child_spin = matches!(st.canonical, 1022 | 1330);
            // rotate triggers turn the object too (World::turned)
            let rotation = st.rotation + game.world.turned[index] + game.world.area.turn[index] + if child_spin { 0.0 } else { spin };
            // Object effects run only for objects near the view in y too: the game activates
            // objects by 2-D sections (100 units square) around the camera, so a tall stack's
            // far-off emitters don't run (pc killer had 20,000 running, most off screen).
            let near_y = oy >= cam_y - EFFECT_MARGIN_Y && oy <= cam_y + view_h + EFFECT_MARGIN_Y;
            if near_y && crate::particles::object_effect(st.canonical).is_some() {
                effect_objects.push((index, st.canonical, (ox, oy), rotation, (sx, sy)));
            }
            let in_b = z_layer <= 3;
            if z_layer != last_z {
                if let Some(was_b) = layer {
                    marks.note(last_z, was_b, if was_b { below.len() } else { object_draws.len() });
                    flush(&mut layer_blend, &mut layer_normal, &mut layer_up, if was_b { &mut below } else { &mut object_draws });
                }
                layer = Some(in_b);
                last_z = z_layer;
            }
            let tb2 = tb0.map(|_| Instant::now());
            // Cached: an object nothing moves, turns, scales, tints, animates, pulses or spins,
            // 70 units or more inside both view edges (no edge fade, no enter effect), whose
            // channels haven't changed since its sprites were built.
            let cache_key = if cache_on
                && anim_sprites.is_none()
                && pulse == 1.0
                // rods' balls pulse with the music (drawn below)
                && !matches!(st.canonical, 15..=17)
                && game.level.spin[index] == 0.0
                && game.world.moved[index] == (0.0, 0.0)
                && game.world.turned[index] == 0.0
                && game.world.scaled[index] == (1.0, 1.0)
                && game.world.area.scale[index] == (1.0, 1.0)
                && game.world.area.turn[index] == 0.0
                && game.world.area.opacity[index] == 1.0
                && game.world.area.tint[index].0 == 0
                && game.world.area.tint_hsv[index].is_none()
                && !game.level.ui_objects.contains_key(&index)
                && !game.level.pulse_groups.contains_key(&index)
                && (ox - cam_x).min(cam_x + view_w - ox) >= enter_margin
            {
                let base = st.main_channel.unwrap_or(visual.base_color as i32);
                let detail = st.detail_channel.unwrap_or(if visual.detail_color < 0 { 1010 } else { visual.detail_color as i32 });
                let glow_bg = crate::black::GLOW_COLOR.binary_search_by_key(&(st.canonical as u16), |g| g.0).is_ok();
                game.draw_cache.key(base, detail, glow_bg)
            } else {
                None
            };
            if let Some(cached) = cache_key.and_then(|key| game.draw_cache.get(index, key)) {
                for &(list, d) in cached {
                    match list {
                        0 => draws.push(d),
                        1 => backs.push(d),
                        2 => layer_normal.push(d),
                        3 => layer_blend.push(d),
                        4 => layer_up.push(d),
                        _ => below.push(d),
                    }
                }
                continue;
            }
            // where each list stood before this object, to keep what it adds (CachedObject::draws)
            let marks_before = cache_key.map(|_| [draws.len(), backs.len(), layer_normal.len(), layer_blend.len(), layer_up.len(), below.len()]);
            let groups: &[i32] = game.level.pulse_groups.get(&index).map_or(&[], Vec::as_slice);
            let (hsv_main, hsv_detail) = game.level.hsv[index];
            let base_channel = st.main_channel.unwrap_or(visual.base_color as i32);
            // The detail ("_color_") layer defaults to black when the object has no detail channel.
            let detail_channel = st.detail_channel.unwrap_or(if visual.detail_color < 0 { 1010 } else { visual.detail_color as i32 });
            let main_active = game.colors.active(base_channel);
            // Objects flagged m_isObjectBlack keep a black main sprite whatever their channel
            // (but their glow still takes the channel's color, below).
            let object_black = crate::black::OBJECT_BLACK.binary_search(&(st.canonical as u16)).is_ok();
            let unblack = game.colors.object_color(base_channel, main_active, groups, hsv_main);
            let base = if object_black { [0.0; 3] } else { unblack };
            let detail = game.colors.object_color(detail_channel, main_active, groups, hsv_detail);
            // area tint triggers (3010): toward the channel's color, or an HSV shift (278)
            let (base, detail) = match game.world.area.tint[index] {
                (0, ..) => (base, detail),
                (channel, share, main, det) => {
                    let to = game.colors.active(channel).map(|v| v as f32 / 255.0);
                    let mix = |c: [f32; 3]| [0, 1, 2].map(|k| c[k] + (to[k] - c[k]) * share);
                    (if main { mix(base) } else { base }, if det { mix(detail) } else { detail })
                }
            };
            let (base, detail) = match game.world.area.tint_hsv[index] {
                Some((h, main, det)) => {
                    let shift = |c: [f32; 3]| {
                        let u = c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
                        cadence_core::triggers::transform_color(u, h).map(|v| v as f32 / 255.0)
                    };
                    (if main { shift(base) } else { base }, if det { shift(detail) } else { detail })
                }
                None => (base, detail),
            };
            if debug_obj == Some(st.id) {
                log(&format!("obj {} at ({ox},{oy}) z {z_layer} base {base:?} detail {detail:?} channels {base_channel}/{detail_channel} sprites {:?}",
                    st.id, visual.sprites.iter().map(|s| (s.frame, s.role == SpriteRole::Detail, s.visible)).collect::<Vec<_>>()));
            }
            // Edge fade and enter effects (`PlayLayer::updateVisibility` 0x7ae88, enter.rs):
            // objects within 70 units of the view's left or right edge fade out, opacity =
            // int(clamp(distance / 70, 0, 1)·255) measured from the object's far edge ("don't
            // fade" objects, key 64, don't), and move / scale / turn / tint by the enter effect.
            let enter = {
                let eo = cadence_core::enter::EnterObject {
                    x: ox,
                    y: oy,
                    margin: objects::info(st.canonical).map_or(0.0, |i| cadence_core::enter::fade_margin(i.width, i.height, (sx, sy), rotation)),
                    channel: st.enter_channel as i32,
                    ignore_enter: st.no_enter,
                    ignore_fade: st.no_fade,
                    ui: game.level.ui_objects.contains_key(&index),
                    no_scale: audio_scale(st.canonical).is_some(),
                    seed: index as u32,
                };
                game.world.enter.apply(&eo, &enter_view, &mut game.enter_locks[index])
            };
            let fade = enter.opacity;
            // enter tints (3021): toward the channel's color, or an HSV shift
            let (base, detail) = match enter.tint {
                Some((channel, share, main, det)) => {
                    let to = game.colors.active(channel).map(|v| v as f32 / 255.0);
                    let mix = |c: [f32; 3]| [0, 1, 2].map(|k| c[k] + (to[k] - c[k]) * share);
                    (if main { mix(base) } else { base }, if det { mix(detail) } else { detail })
                }
                None => (base, detail),
            };
            let (base, detail) = match enter.hsv {
                Some((h, main, det)) => {
                    let shift = |c: [f32; 3]| {
                        let u = c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
                        cadence_core::triggers::transform_color(u, h).map(|v| v as f32 / 255.0)
                    };
                    (if main { shift(base) } else { base }, if det { shift(detail) } else { detail })
                }
                None => (base, detail),
            };
            // area fade triggers (3009) on top
            let fade = fade * game.world.area.opacity[index];
            // the editor: objects off the shown layer are dimmed
            let fade = match edit_layer {
                Some(l) if l >= 0 => {
                    let (l1, l2) = st.layers;
                    if l1 == l || (l2 != 0 && l2 == l) { fade } else { fade * 0.3 }
                }
                _ => fade,
            };
            let (base_alpha, detail_alpha) =
                (fade * game.colors.object_opacity(base_channel, groups), fade * game.colors.object_opacity(detail_channel, groups));
            // the enter effect's move, scale and turn
            let (ox, oy, sx, sy) = (ox + enter.dx, oy + enter.dy, sx * enter.scale.0, sy * enter.scale.1);
            let rotation = rotation + enter.rotation;
            let (base_blends, detail_blends) = (game.colors.blending(base_channel), game.colors.blending(detail_channel));
            // this type's sprite tree, resolved once per frame (fixed trees) or for this object
            let fixed = anim_sprites.is_none() && color_child.is_none();
            let order_of = |sprites: &[_]| -> TypeSprites { TypeSprites::new(sprites, &|n: &str| sheets.frames.get(n).copied()) };
            let built;
            let ty: &TypeSprites = if fixed {
                types.entry(st.canonical).or_insert_with(|| order_of(visual.sprites))
            } else {
                built = order_of(visual.sprites);
                &built
            };
            let has_detail = ty.has_detail;
            let child_black = crate::black::CHILD_BLACK
                .binary_search_by_key(&(st.canonical as u16), |c| c.0)
                .ok()
                .map(|i| crate::black::CHILD_BLACK[i].1);
            let detail_moves = crate::color_z::COLOR_Z_RELATED.binary_search(&(st.canonical as u16)).is_ok();
            let tb3 = tb0.map(|_| Instant::now());
            if visual.glow && !st.no_glow {
                // the glow sprite the game gives the object (dumped with it), else the main
                // frame's `_glow` version
                let main = visual.sprites[0].frame;
                let glow = visual
                    .sprites
                    .iter()
                    .find(|s| s.role == SpriteRole::Glow && !s.frame.is_empty())
                    .map_or_else(|| main.replace("_001.png", "_glow_001.png"), |s| s.frame.to_string());
                if let Some((gw, gh)) = frame_size(&glow) {
                    let t = node_transform(ox, oy, rotation + if child_spin { spin } else { 0.0 }, sx, sy, 0.5, 0.5, gw, gh);
                    // `GameObject::setObjectColor` (0x5b8708) gives the glow the object's color
                    // (`setGlowColor`, also for black objects, whose main sprite stays black) —
                    // white for the default channels, the player's color for P1/P2 objects, the
                    // shifted color under an area or enter tint. Ids with m_customGlowColor
                    // (`updateVisibility`) take the light background, or the background shifted
                    // s −0.2 v +0.2.
                    let glow_color = match crate::black::GLOW_COLOR.binary_search_by_key(&(st.canonical as u16), |g| g.0) {
                        Ok(i) if crate::black::GLOW_COLOR[i].1 => game.colors.display(1007),
                        Ok(_) => {
                            let bg = game.colors.active(1000);
                            let h = Hsv { h: 0.0, s: -0.2, v: 0.2, abs_s: true, abs_v: true };
                            cadence_core::triggers::transform_color(bg, h).map(|v| v as f32 / 255.0)
                        }
                        Err(_) if object_black => unblack,
                        Err(_) => base,
                    };
                    // the glow follows the object's opacity (`GameObject::setGlowOpacity`): hidden
                    // or faded objects (channel or group alpha) take their glow with them
                    // the glow batch blends SRC_ALPHA, ONE (real scene dumps: 770,1)
                    push(&glow, t, false, false, [glow_color[0], glow_color[1], glow_color[2], base_alpha], Blend::Additive, &mut draws);
                }
            }
            let main = visual.sprites[0].frame;
            // a portal's back half: its frame looked up once per frame name (not searched and
            // built as a string for every portal every frame)
            let back = portal_backs
                .entry(main)
                .or_insert_with(|| {
                    if main.starts_with("portal_") && main.contains("_front_") {
                        let name = main.replace("_front_", "_back_");
                        sheets.frames.get(&name).copied().map(|f| (name, f))
                    } else {
                        None
                    }
                })
                .clone();
            if let Some((name, frame)) = back {
                let (bw, bh) = frame.source_size;
                let t = node_transform(ox, oy, rotation, sx, sy, 0.5, 0.5, bw, bh);
                push_frame(&frame, &name, t, false, false, tint(base, base_alpha), Blend::Normal, &mut backs);
            }
            let tb4 = tb0.map(|_| Instant::now());
            transforms.clear();
            for s in visual.sprites {
                let t = if s.parent < 0 {
                    node_transform(ox, oy, rotation, sx, sy, s.anchor_x, s.anchor_y, s.w, s.h)
                } else {
                    let parent = transforms[s.parent as usize];
                    let r = s.rotation + if child_spin && s.parent == 0 { spin } else { 0.0 };
                    mul(parent, node_transform(s.x, s.y, r, s.scale_x, s.scale_y, s.anchor_x, s.anchor_y, s.w, s.h))
                };
                transforms.push(t);
            }
            // children with negative z draw below their parent, the rest above
            for &i in &ty.order {
                let s = &visual.sprites[i];
                if !s.visible || s.frame.is_empty() || s.role == SpriteRole::Glow {
                    continue;
                }
                let detail_part = ty.detail[i];
                // `GameObject::setOpacity` (0x5adee8): the color sprite (a child of the main one)
                // gets the main sprite's opacity times its own channel's, so a faded base channel
                // fades the whole object.
                let (c, alpha) = if detail_part && s.parent >= 0 {
                    (detail, base_alpha * game.colors.object_opacity(detail_channel, &[]))
                } else if detail_part {
                    (detail, detail_alpha)
                } else {
                    (base, base_alpha)
                };
                let c = visual.tint(s).unwrap_or(c);
                // m_isColorSpriteBlack: the object's child sprites (e.g. a block's fill) stay black.
                let (c, alpha) = match child_black {
                    Some(op) if s.parent >= 0 && !detail_part => ([0.0; 3], if op > 0.0 { alpha * op } else { alpha }),
                    _ => (c, alpha),
                };
                let blends = if detail_part { detail_blends } else { base_blends };
                let back = ty.back[i];
                // Portal backs are in the last B batch (z −5) even for T-layer portals.
                let target = if back {
                    if in_b { &mut layer_normal } else { &mut below }
                } else if !blends {
                    &mut layer_normal
                } else if base_blends != detail_blends && has_detail && detail_part == detail_moves {
                    &mut layer_up
                } else {
                    &mut layer_blend
                };
                let blend = if blends || s.additive { Blend::Additive } else { Blend::Normal };
                // Ground spikes (9) take one of pit_01..03 at random as the level loads (real dumps
                // differ from run to run): a stable choice per object.
                if st.canonical == 9 && s.frame.starts_with("pit_0") {
                    let v = ((index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 32) % 3 + 1;
                    push(&format!("pit_0{v}_001.png"), transforms[i], s.flip_x, s.flip_y, tint(c, alpha), blend, target);
                } else if let Some(frame) = &ty.frames[i] {
                    push_frame(frame, s.frame, transforms[i], s.flip_x, s.flip_y, tint(c, alpha), blend, target);
                }
            }
            if let (Some(key), Some(at)) = (cache_key, marks_before) {
                let mut out = Vec::new();
                for (list, (items, from)) in [&draws, &backs, &layer_normal, &layer_blend, &layer_up, &below].into_iter().zip(at).enumerate() {
                    out.extend(items[from..].iter().map(|&d| (list as u8, d)));
                }
                game.draw_cache.store(index, key, out);
            }
            if let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (tb0, tb1, tb2, tb3, tb4) {
                let f = Instant::now();
                BLOCKS.with(|x| {
                    let mut x = x.borrow_mut();
                    x[0] += (b - a).as_secs_f64();
                    x[1] += (c - b).as_secs_f64();
                    x[2] += (d - c).as_secs_f64();
                    x[3] += (e - d).as_secs_f64();
                    x[4] += (f - e).as_secs_f64();
                });
            }
            // Rods (15–17) carry a ball the game adds as its own object (37): half the rod's
            // height + 10 above its center along the rod, player color 1 (always additive),
            // pulsing with the music.
            if matches!(st.canonical, 15..=17) {
                if let Some((_, rod_h)) = frame_size(visual.sprites[0].frame) {
                    let ball = format!("rod_ball_{:02}_001.png", game.ball_frame);
                    if let Some((bw, bh)) = frame_size(&ball) {
                        let d = (rod_h * 0.5 + 10.0) * sy;
                        let (sin, cos) = rotation.to_radians().sin_cos();
                        let (bx, by) = (ox + d * sin, oy + d * cos);
                        let k = game.meter.value * sx.abs();
                        let t = node_transform(bx, by, rotation, k, k, 0.5, 0.5, bw, bh);
                        let color = game.colors.object_color(1005, game.colors.active(1005), groups, None);
                        push(&ball, t, false, false, tint(color, base_alpha), Blend::Additive, &mut layer_blend);
                    }
                }
            }
        }
        if let Some(was_b) = layer {
            marks.note(last_z, was_b, if was_b { below.len() } else { object_draws.len() });
            flush(&mut layer_blend, &mut layer_normal, &mut layer_up, if was_b { &mut below } else { &mut object_draws });
        }
        // The attempt label (`PlayLayer::resetLevel`'s m_attemptLabel, z 1400 in the object
        // layer): "Attempt N" in bigFont, white, half a view plus 15 ahead of the start and 140
        // above it (real dumps: (261.5, 245) for a 493-wide view from (0, 105)).
        let hide_attempts = game.editor || if game.practice { game.opts.hide_attempts_practice } else { game.opts.hide_attempts };
        if let Some(font) = fonts.first().filter(|_| !hide_attempts) {
            let (glyphs, (w, h)) = font.layout(&format!("Attempt {}", game.attempt));
            let start = (game.player_start.x, game.player_start.y);
            let node = node_transform(start.0 + view_w * zoom * 0.5 + 15.0, start.1 + 140.0, 0.0, 1.0, 1.0, 0.5, 0.5, w, h);
            for g in glyphs {
                if let Some((gw, gh)) = frame_size(&g.frame) {
                    let t = mul(node, node_transform(g.x, g.y, 0.0, 1.0, 1.0, 0.5, 0.5, gw, gh));
                    push(&g.frame, t, false, false, tint([1.0; 3], 1.0), Blend::Normal, &mut object_draws);
                }
            }
        }
        // The end portal (`EndPortalObject`, 0x310cb4) at the level's end: a column of square_02
        // (rotated −90°, 30 apart, 1.5 views tall) and, 15/zoom + 14 to its left, the additive
        // gradientBar in player color 1, 1.5 views tall. `updateEndPos` (0x311280): at the view's
        // middle, never below 225 except while flying.
        {
            let end_x = game.world.level_length;
            if !game.editor && end_x > cam_x - 60.0 && end_x - 60.0 < cam_x + view_w {
                let flying = matches!(game.player.mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing);
                let mid = cam_y + view_h * 0.5;
                let end_y = if flying { mid } else { mid.max(225.0) };
                let n = (VIEW_HEIGHT * 1.5 / 30.0) as i32;
                let obj_color = game.colors.display(1004);
                if let Some((bw, bh)) = frame_size("square_02_001.png") {
                    for i in 0..n {
                        let y = end_y + (n / 2) as f32 * 30.0 - i as f32 * 30.0;
                        let t = node_transform(end_x, y, -90.0, 1.0, 1.0, 0.5, 0.5, bw, bh);
                        push("square_02_001.png", t, false, false, tint(obj_color, 1.0), Blend::Normal, &mut object_draws);
                    }
                }
                effect_objects.push((usize::MAX, crate::particles::END_PORTAL, (end_x, end_y), 0.0, (1.0, 1.0)));
                if let Some((gw, gh)) = frame_size("gradientBar.png") {
                    let x = end_x - 15.0 / zoom - 14.0;
                    let t = node_transform(x, end_y, 0.0, 1.0, VIEW_HEIGHT * 1.5 / gh, 0.5, 0.5, gw, gh);
                    push("gradientBar.png", t, false, false, tint(game.pcol.0, 1.0), Blend::Additive, &mut object_draws);
                }
            }
        }
        // Practice checkpoints (`CheckpointObject`, checkpoint_01_001) where they were placed.
        for cp in &game.checkpoints {
            let name = "checkpoint_01_001.png";
            if let Some((cw, ch)) = frame_size(name) {
                let t = node_transform(cp.player.x, cp.player.y, 0.0, 1.0, 1.0, 0.5, 0.5, cw, ch);
                push(name, t, false, false, tint([1.0; 3], 1.0), Blend::Normal, &mut object_draws);
            }
        }
        let (below_base, below_len) = (draws.len(), below.len());
        draws.extend(below);
        draws.extend(backs);
        marks.start[8] = draws.len();
        let objects_ms = t_objects.elapsed().as_secs_f64() * 1000.0;
        let t_after = Instant::now();
        game.effects.sync_objects(&game.particle_defs, &effect_objects);
        if dev_flag("debug_slow") && std::env::var_os("CADENCE_SCREENSHOT").is_some() {
            log(&format!("emitters at step {}: {}", game.step_count, game.effects.objects.len()));
            let mut by_id: std::collections::BTreeMap<i32, usize> = Default::default();
            for (i, _) in game.effects.objects.iter() {
                if let Some(st) = game.level.statics.get(*i) {
                    *by_id.entry(st.canonical).or_default() += 1;
                }
            }
            log(&format!("emitters by id: {by_id:?}"));
        }
        if let Some((_, e)) = game.effects.objects.get_mut(&usize::MAX) {
            let [r, g, b] = game.pcol.0;
            e.def.start_color = [r, g, b, 1.0];
            e.def.end_color = [r, g, b, 1.0];
        }
        // Glitter while flying, centered on the view (z 0 in the object layer: over the B
        // layers, under the player).
        let flying = matches!(game.player.mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Swing) && !game.player.dead;
        // (BG effect off triggers, 1819, keep it off: `toggleBGEffectVisibility`)
        let glitter = flying && !game.world.bg_effect_off;
        game.effects.sync_glitter(glitter, (cam_x + view_w * 0.5, cam_y + view_h * 0.5), game.pcol.0, (view_w, view_h));
        if let (Some(e), Some(square)) = (&game.effects.glitter, sheets.frames.get(PARTICLE_TEXTURE)) {
            push_particles(e, square.sheet, &mut draws);
        }
        // The wave trail (z −3): player color 1, then a white core a third as wide.
        if game.player.mode == Mode::Wave {
            if let Some(square) = sheets.frames.get(PARTICLE_TEXTURE) {
                let atlas = renderer.atlas_size(square.sheet);
                let white = ((square.tex_x + square.tex_w * 0.5) / atlas.0, (square.tex_y + square.tex_h * 0.5) / atlas.1);
                let pulse = (game.meter.value - 0.1).mul_add(2.1, 0.4);
                let size = game.player.size;
                let width = 6.0 * size * pulse;
                let head = (game.player.x, game.player.y);
                let [r, g, b] = game.pcol.0;
                for (w, color) in [(width, [r, g, b, 1.0]), (width / 3.0, [1.0, 1.0, 1.0, 165.0 / 255.0])] {
                    for t in game.wave_trail.band(head, w) {
                        draws.push(Draw { sheet: square.sheet, blend: Blend::Additive, instance: triangle_instance(t[0], t[1], t[2], white, color) });
                    }
                }
            }
        }
        // The regular trail (z −2, under the player): player color 2, additive, streak_01.
        if let Some(frame) = sheets.frames.get(STREAK_TEXTURE) {
            let atlas = renderer.atlas_size(frame.sheet);
            let (aw, ah) = atlas;
            // (u across, v along) → atlas coordinates (the frame may be stored rotated).
            let uv = |u: f32, v: f32| {
                if frame.rotated {
                    ((frame.tex_x + v * frame.tex_h) / aw, (frame.tex_y + u * frame.tex_w) / ah)
                } else {
                    ((frame.tex_x + u * frame.tex_w) / aw, (frame.tex_y + (1.0 - v) * frame.tex_h) / ah)
                }
            };
            let [r, g, b] = game.pcol.1;
            for (p, t, alpha) in game.trail.triangles() {
                let corners = t.map(|(u, v)| uv(u, v));
                let m = [p[1].0 - p[0].0, p[1].1 - p[0].1, p[2].0 - p[0].0, p[2].1 - p[0].1, p[0].0, p[0].1];
                let uvs = [corners[0].0, corners[0].1, corners[1].0, corners[1].1, corners[2].0, corners[2].1, corners[2].0, corners[2].1];
                draws.push(Draw { sheet: frame.sheet, blend: Blend::Additive, instance: Instance { m, _pad: [1.0, 0.0], uv: uvs, color: [r, g, b, alpha] } });
            }
        }
        // Custom particle objects in view (their editor icon isn't drawn in play).
        let custom: Vec<(usize, (f32, f32), f32, (f32, f32), i32, &str)> = game
            .level
            .particles
            .iter()
            .filter_map(|(index, text)| {
                let obj = &game.level.objects[*index];
                let (mx, my) = game.world.moved[*index];
                let x = (obj.x() as f64 + mx) as f32;
                if !(x >= cam_x - 200.0 && x <= cam_x + view_w + 200.0) || !game.world.enabled[*index] {
                    return None;
                }
                let y = ((obj.y() + GROUND_OFFSET) as f64 + my) as f32;
                let scale = obj.get_f32(32).unwrap_or(1.0);
                let s = (obj.get_f32(128).unwrap_or(scale), obj.get_f32(129).unwrap_or(scale));
                Some((*index, (x, y), obj.get_f32(6).unwrap_or(0.0), s, obj.get_i32(24).unwrap_or(5), text.as_str()))
            })
            .collect();
        game.effects.sync_custom(&custom);
        {
            // by z layer, then object (a HashMap's order changes from run to run)
            let mut fx: Vec<(&usize, &(i32, Emitter))> = game.effects.custom.iter().collect();
            fx.sort_by_key(|(i, (z, _))| (*z, **i));
            for (_, (_, e)) in fx {
                let Some(frame) = sheets.frames.get(e.def.texture.as_str()) else { continue };
                let atlas = renderer.atlas_size(frame.sheet);
                let (sw, sh) = frame.source_size;
                for (pos, size, rot, c) in e.particles() {
                    let (color, blend) = if e.def.additive { (c, Blend::Additive) } else { (tint([c[0], c[1], c[2]], c[3]), Blend::Normal) };
                    let t = node_transform(pos.0, pos.1, rot, size / sw, size / sh, 0.5, 0.5, sw, sh);
                    draws.push(Draw { sheet: frame.sheet, blend, instance: sprite_instance(frame, atlas, t, false, false, color) });
                }
            }
        }
        if let Some(square) = sheets.frames.get(PARTICLE_TEXTURE) {
            for w in game.effects.waves.iter().filter(|w| w.z < 4) {
                push_wave(w, square.sheet, pixel, &mut draws);
            }
            let mut fx: Vec<(&usize, &(i32, Emitter))> = game.effects.objects.iter().collect();
            fx.sort_by_key(|(i, (z, _))| (*z, **i));
            for (_, (_, e)) in fx {
                push_particles(e, square.sheet, &mut draws);
            }
        }

        // The player's drag and ship fire (z 39) go under it, its sparks and landings (z 61) over.
        let square_sheet = sheets.frames.get(PARTICLE_TEXTURE).map(|f| f.sheet);
        if let Some(sheet) = square_sheet {
            for e in game.player_fx.below() {
                push_particles(e, sheet, &mut draws);
            }
        }
        // Player: each sprite centered on its own frame size (cocos2d content size). After a
        // death the player that died fades out over 0.05 s (playDeathEffect's CCFadeTo, a frame
        // late); in dual mode the other one is hidden and its exit copy drawn instead.
        let death_alpha = if p.dead { 1.0 - (game.death_fade.unwrap_or(0.0) / 0.05).min(1.0) } else { 1.0 };
        let second_died = p.dead && p.second_died && p.dual.is_some();
        if let Some((copy, a)) = game.exit_dual.as_ref().filter(|_| p.dead && !game.editor).map(|e| e.state()) {
            if 0.0 < a {
                let colors = game.exit_dual.as_ref().map(|e| e.colors).unwrap_or(game.pcol);
                player_draws(sheets, &|s| renderer.atlas_size(s), &copy, (copy.x, copy.y), copy.rotation, colors, &game.icons, None, a, &mut draws);
            }
        }
        if !p.dead || 0.0 < death_alpha {
            let (mut px, mut py, mut rot) = (p.x, p.y, p.rotation);
            if let Some(t) = game.end_time {
                // playEndAnimationToPos: a 1 s cubic Bézier (ease-in 1.2) through
                // (x, y), (x + 40, end.y + 150) to (end.x + 50, end.y − 20), spinning 360°
                // (ease-in 1.5). The end point is the end portal, near the level's end.
                let (sx, sy) = game.end_from;
                let (ex, ey) = (game.world.level_length - 100.0, cam_y + view_h * 0.5);
                let k = t.min(1.0);
                let b = k.powf(1.2);
                let u = 1.0 - b;
                let pts = [(sx, sy), (sx, sy), (sx + 40.0, ey + 150.0), (ex + 50.0, ey - 20.0)];
                let w = [u * u * u, 3.0 * u * u * b, 3.0 * u * b * b, b * b * b];
                px = pts.iter().zip(w).map(|(q, w)| q.0 * w).sum();
                py = pts.iter().zip(w).map(|(q, w)| q.1 * w).sum();
                rot += 360.0 * k.powf(1.5);
            }
            // the teleport streaks (PlayLayer +0xf50, z 40: under the player)
            for d in &game.spider_dashes {
                let frame = format!("spiderDash_{:03}.png", ((d.elapsed / 0.04) as usize).min(7) + 1);
                if let Some((w, h)) = frame_size(&frame) {
                    let t = node_transform(d.center.0, d.center.1, d.rotation, d.length / w, 1.0, 0.5, 0.5, w, h);
                    push(&frame, t, false, false, tint(d.color, 1.0), Blend::Additive, &mut draws);
                }
            }
            let rig = game.robot_rig();
            let popped;
            let p = if p.mode == Mode::Spider && game.spider_scale() != 1.0 {
                let mut q = p.clone();
                q.scale *= game.spider_scale();
                popped = q;
                &popped
            } else {
                p
            };
            let a1 = if p.dead { if second_died { 0.0 } else { death_alpha } } else { 1.0 };
            if !game.editor && !game.world.player_hidden && 0.0 < a1 {
                player_draws(sheets, &|s| renderer.atlas_size(s), p, (px, py), rot, game.pcol, &game.icons, rig, a1, &mut draws);
            }
            // Dual mode: player 2, with the two colors swapped (the game's default).
            let a2 = if p.dead { if second_died { death_alpha } else { 0.0 } } else { 1.0 };
            if let Some(p2) = p.dual.as_ref().filter(|_| !game.editor && 0.0 < a2) {
                player_draws(sheets, &|s| renderer.atlas_size(s), p2, (p2.x, p2.y), p2.rotation, (game.pcol.1, game.pcol.0), &game.icons, rig, a2, &mut draws);
            }
        }
        if let Some(sheet) = square_sheet {
            for e in game.player_fx.above() {
                push_particles(e, sheet, &mut draws);
            }
        }

        // Effects: children of the object layer, so under the ground. All of the game's particle
        // effects blend additively (SRC_ALPHA, ONE) with the white square texture; circle waves are
        // additive filled polygons.
        // (z 120 and over: after the T layers)
        if let Some(square) = sheets.frames.get(PARTICLE_TEXTURE) {
            for e in game.effects.emitters.iter().filter(|e| e.z < 120) {
                push_particles(e, square.sheet, &mut draws);
            }
            for w in game.effects.waves.iter().filter(|w| (4..120).contains(&w.z)) {
                push_wave(w, square.sheet, pixel, &mut draws);
            }
        }
        for sh in &game.effects.shatters {
            let Some(sheet) = sh.sheet else { continue };
            for s in &sh.shards {
                let (x, y, w, h) = s.rect;
                let (u0, u1) = (x / sh.size, (x + w) / sh.size);
                // the capture's first row is its top
                let (vt, vb) = (1.0 - (y + h) / sh.size, 1.0 - y / sh.size);
                let center = (sh.origin.0 + s.pos.0, sh.origin.1 + s.pos.1);
                let color = tint([1.0, 1.0, 1.0], Shatter::opacity(s));
                let instance = quad_instance(center, (w, h), s.rotation, [u0, vb, u1, vb, u0, vt, u1, vt], color);
                draws.push(Draw { sheet, blend: Blend::Normal, instance });
            }
        }

        // The T layers (z layer ≥ 5, batches at z ≥ 120 in the object layer) cover the player (59)
        // and the death effects (99–101): portal fronts, blocks, decoration.
        let (t_base, t_len) = (draws.len(), object_draws.len());
        draws.extend(object_draws);
        marks.place_objects(below_base, below_len, t_base, t_len);
        // The death effects' parts above the T layers: the first circle wave (z 1000), effect
        // 13's particles (1000) and the explosion sprite (z 10000 in the object layer).
        if let Some(square) = sheets.frames.get(PARTICLE_TEXTURE) {
            for w in game.effects.waves.iter().filter(|w| w.z >= 120) {
                push_wave(w, square.sheet, pixel, &mut draws);
            }
            for e in game.effects.emitters.iter().filter(|e| e.z >= 120) {
                push_particles(e, square.sheet, &mut draws);
            }
        }
        for d in &game.effects.deaths {
            let frame = d.frame();
            if let Some((w, h)) = frame_size(&frame) {
                let [r, g, b, a] = d.color();
                let t = node_transform(d.pos.0, d.pos.1, d.rotation, d.scale, d.scale, 0.5, 0.5, w, h);
                let blend = if d.additive { Blend::Additive } else { Blend::Normal };
                push(&frame, t, false, false, tint([r, g, b], a), blend, &mut draws);
            }
        }
        marks.start[13] = draws.len();

        // Ground layers (the game's GJGroundLayer): top edge 1 above the physics floor; 128-wide
        // tiles anchored to the camera origin; the 444-wide additive line centered on screen; and
        // the two edge shadows (DST_COLOR blend, 0.7 wide, opacity 100). In a flying corridor a
        // second, flipped layer forms the ceiling.
        let g = game.colors.display(1001);
        let line = game.colors.display(1002);
        // The two ground layers where `updateCameraBGArt` puts them (screen units from the
        // camera): the floor slides up to the corridor (or sits on the level's ground), the
        // ceiling slides down; off-screen ones (free mode slides both out) aren't drawn.
        let mut grounds = Vec::new();
        if game.camera.ground_bottom > 0.5 {
            grounds.push((cam_y + game.camera.ground_bottom / zoom, 1.0f32));
        }
        if game.camera.ground_top < VIEW_HEIGHT - 0.5 {
            grounds.push((cam_y + game.camera.ground_top / zoom, -1.0));
        }
        for (ground_y, dir) in grounds {
            let ground_frame = format!("{}.png", game.ground_image);
            let ground2_frame = game.ground2_image.as_ref().map(|n| format!("{n}.png"));
            let g2 = game.colors.display(cadence_core::triggers::channel::GROUND_2);
            if let Some((tw, th)) = frame_size(&ground_frame) {
                let mut x = -((cam_x - Camera::START_X).rem_euclid(tw));
                while x < view_w {
                    let t = node_transform(cam_x + x, ground_y, 0.0, 1.0, dir, 0.0, 1.0, tw, th);
                    push(&ground_frame, t, false, false, tint(g, 1.0), Blend::Normal, &mut draws);
                    // Grounds 8+ have a second layer in the ground 2 color.
                    if let Some(f2) = &ground2_frame {
                        push(f2, t, false, false, tint(g2, 1.0), Blend::Normal, &mut draws);
                    }
                    x += tw;
                }
            }
            if let Some((lw, lh)) = frame_size("floorLine_01_001.png") {
                let t = node_transform(cam_x + view_w * 0.5, ground_y + 0.5 * dir, 0.0, 1.0, dir, 0.5, 1.0, lw, lh);
                push("floorLine_01_001.png", t, false, false, tint(line, 1.0), Blend::Additive, &mut draws);
            }
            if let Some((sw, sh)) = frame_size("groundSquareShadow_001.png") {
                let shade = tint([1.0, 1.0, 1.0], 100.0 / 255.0);
                let left = node_transform(cam_x - 1.0, ground_y, 0.0, 0.7, dir, 0.0, 1.0, sw, sh);
                let right = node_transform(cam_x + view_w + 1.0, ground_y, 0.0, 0.7, dir, 1.0, 1.0, sw, sh);
                push("groundSquareShadow_001.png", left, false, false, shade, Blend::Shade, &mut draws);
                push("groundSquareShadow_001.png", right, true, false, shade, Blend::Shade, &mut draws);
            }
        }

        // Gradient triggers at their layers, and the layer marks for the shader layer.
        marks.start[1] = 0;
        marks.start[2] = bg_end;
        marks.start[3] = world_start;
        marks.start[14] = draws.len();
        marks.start[15] = draws.len();
        marks.start[16] = draws.len();
        let grads = crate::fx::gradient_draws(&game.fx, &game.colors, &game.world, (cam_x, cam_y, view_w, view_h), zoom);
        let layer_marks = crate::fx::splice_gradients(&mut draws, &marks, grads);
        let world_start = layer_marks[3];

        // Mirror portals: everything but the background is scaled by 1 − 2·flip around the
        // screen's center (the flip animates through zero width, like the game's).
        let flip = game.camera.flip;
        if flip != 0.0 {
            let s = 1.0 - 2.0 * flip;
            let cx = cam_x + view_w * 0.5;
            for d in &mut draws[world_start..] {
                let m = &mut d.instance.m;
                m[0] *= s;
                m[2] *= s;
                m[4] = m[4] * s + cx * (1.0 - s);
            }
        }

        // Camera rotate triggers (2015): the world (not the background) turns clockwise about
        // the view's center.
        let angle = game.camera.angle;
        if angle != 0.0 {
            let (sin, cos) = (-angle).to_radians().sin_cos();
            let (cx, cy) = (cam_x + view_w * 0.5, cam_y + view_h * 0.5);
            for d in &mut draws[world_start..] {
                let m = &mut d.instance.m;
                let (a, b, c, e) = (m[0], m[1], m[2], m[3]);
                m[0] = cos * a - sin * b;
                m[1] = sin * a + cos * b;
                m[2] = cos * c - sin * e;
                m[3] = sin * c + cos * e;
                let (tx, ty) = (m[4] - cx, m[5] - cy);
                m[4] = cos * tx - sin * ty + cx;
                m[5] = sin * tx + cos * ty + cy;
            }
        }

        // The level's UI on top (pause button, pause menu, end screen; crate::ui).
        self.ui.set_window(w as f32, h as f32);
        let name = self.ui.level_name(self.level_id);
        if let (Some(d), Some(path)) = (
            dumped.borrow().as_ref(),
            std::env::var("CADENCE_DUMP_DRAWS").ok().map(|p| p.replace("%d", &game.step_count.to_string())),
        ) {
            let mut text = format!("camera {cam_x} {cam_y} {view_w} {view_h}\n");
            let pl = &game.player;
            text.push_str(&format!("# player {:?} rot {:.1} ground {} anim {} t {:.3} part0 {:?}\n", pl.mode, pl.rotation, pl.on_ground, game.robot_anim.anim, game.robot_anim.time, game.robot_rig().and_then(|r| r.first()).map(|q| (q.texture.clone(), q.pos, q.rotation))));
            for (name, x, y, a, rot) in d {
                text.push_str(&format!("{name} {x:.2} {y:.2} {a:.3} {rot:.1}\n"));
            }
            let _ = std::fs::write(path, text);
        }
        self.ui.overlay(sheets, &|s| renderer.atlas_size(s), &name, (cam_x, cam_y, view_w, view_h), &mut draws, ui_dt);
        for sound in self.ui.take_sounds() {
            game.audio.play_effect(sound);
        }

        let compose_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
        PHASES.with(|p| {
            let mut p = p.borrow_mut();
            p.0 += objects_ms;
            p.1 += t_after.elapsed().as_secs_f64() * 1000.0;
            p.2 += compose_ms;
            p.3 += 1;
            if dev_flag("debug_slow") && p.3 % 300 == 0 {
                let n = p.3 as f64;
                BLOCKS.with(|x| { let x = x.borrow(); log(&format!("object blocks ms/frame: visual {:.2} scale {:.2} colours {:.2} glow {:.2} sprites {:.2}", x[0]*1000.0/n, x[1]*1000.0/n, x[2]*1000.0/n, x[3]*1000.0/n, x[4]*1000.0/n)); });
                log(&format!("phases ms: before objects {:.2}, objects {:.2} (culling {:.2}), after {:.2} (sim {:.2})", p.2 / n - p.0 / n - p.1 / n - game.tick_ms, p.0 / n, cull_ms, p.1 / n, game.tick_ms));
            }
        });
        let post = crate::fx::post_fx(&mut game.fx, &game.colors, &game.world, &game.player, (cam_x, cam_y, view_w, view_h), (view_w * zoom, VIEW_HEIGHT), zoom, &layer_marks);
        game.draw_capacity = draws.len() + draws.len() / 8;
        renderer.render_frame([bg[0] as f64, bg[1] as f64, bg[2] as f64], (cam_x, cam_y, view_w, view_h), &draws, post.as_ref(), screenshot.as_deref());
        #[cfg(not(target_arch = "wasm32"))]
        if screenshot.is_some() && !scripted && !(more_shots && !game.player.dead) {
            std::process::exit(0);
        }
        renderer.window.request_redraw();

        if let (Some((samples, started, secs)), Some(interval)) = (self.bench.as_mut(), interval) {
            // draw calls: runs of the same sheet and blend
            let runs = draws.windows(2).filter(|w| w[0].sheet != w[1].sheet || w[0].blend != w[1].blend).count() + 1;
            if std::env::var_os("CADENCE_DUMP_RUNS").is_some() && draws.len() > 100_000 && samples.len() % 10 == 0 {
                let mut changes: std::collections::BTreeMap<String, usize> = Default::default();
                for w in draws.windows(2).filter(|w| w[0].sheet != w[1].sheet || w[0].blend != w[1].blend) {
                    *changes.entry(format!("{}/{:?} -> {}/{:?}", w[0].sheet, w[0].blend, w[1].sheet, w[1].blend)).or_default() += 1;
                }
                let mut v: Vec<_> = changes.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1));
                log(&format!("runs {runs}, sheets {:?}", v.iter().take(12).collect::<Vec<_>>()));
                // exact duplicates drawn back to back (same sheet, blend and instance)
                let same = |a: &Draw, b: &Draw| a.sheet == b.sheet && a.blend == b.blend && bytemuck::bytes_of(&a.instance) == bytemuck::bytes_of(&b.instance);
                let adjacent = draws.windows(2).filter(|w| same(&w[0], &w[1])).count();
                let mut set = std::collections::HashSet::new();
                for d in draws.iter() {
                    set.insert((d.sheet, d.blend as u8, bytemuck::bytes_of(&d.instance).to_vec()));
                }
                log(&format!("{} draws, {} distinct, {adjacent} repeat the one before", draws.len(), set.len()));
            }
            if dev_flag("debug_slow") && samples.len() % 60 == 0 {
                let live: usize = game.effects.objects.values().map(|(_, e)| e.live()).sum();
                log(&format!("emitters {} particles {} draws {}", game.effects.objects.len(), live, draws.len()));
            }
            if compose_ms > 40.0 && dev_flag("debug_slow") {
                log(&format!("slow frame: {compose_ms:.1} ms compose, {} steps, step {}, x {:.0}", game.last_steps, game.step_count, game.player.x));
            }
            samples.push((interval, compose_ms, game.last_steps, draws.len(), runs));
            TICK_TOTAL.with(|t| { let mut t = t.borrow_mut(); t.0 += game.tick_ms; t.1 += 1; });
            if started.elapsed().as_secs_f64() > *secs {
                report_bench(samples);
                // CADENCE_BENCH_OUT=<file>: every frame interval (ms), one a line, like the
                // harness's vanilla benchmarks (the first frames, while loading, left out)
                if let Some(path) = std::env::var_os("CADENCE_BENCH_OUT") {
                    let text: String = samples.iter().skip(30).map(|s| format!("{:.3}\n", s.0)).collect();
                    let _ = std::fs::write(path, text);
                }
                std::process::exit(0);
            }
        }
        self.frames += 1;
        let elapsed = self.fps_time.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            // (development only, CADENCE_FPS_TITLE: changing the title is a synchronous call to
            // the window server that can make the frame miss its slot)
            if dev_flag("fps_title") {
                let title = format!("Cadence — level {} — {:.0} FPS — {} sprites — {}", self.level_id, self.frames as f32 / elapsed, draws.len(), renderer.backend);
                renderer.window.set_title(&title);
            }
            self.frames = 0;
            self.fps_time = Instant::now();
        }
    }
}

impl App {
    /// Load sheets, upload them and the level once the renderer exists.
    fn finish_setup(&mut self, mut renderer: Renderer, event_loop: &ActiveEventLoop) {
        let files = self.files.as_ref();
        // Texture quality as the game's "auto" picks it (`CCDirector::updateContentScale` with
        // quality 0, design size 480 × 320): UHD from 3× the design size in height or width,
        // low under 1.5× in both, else HD. CADENCE_QUALITY (native) forces one.
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("CADENCE_QUALITY").is_none() {
            let (w, h) = renderer.size();
            let (w, h) = (w as f32, h as f32);
            self.quality = if 320.0 * 3.0 <= h || 480.0 * 3.0 <= w {
                Quality::High
            } else if h < 320.0 * 1.5 && w < 480.0 * 1.5 {
                Quality::Low
            } else {
                Quality::Medium
            };
        }
        let mut sheets = Sheets::new(self.quality);
        for name in SHEETS {
            if let Err(e) = sheets.load(files, "", name) {
                log(&format!("sheet {name}: {e}"));
            }
        }
        for name in ICON_SHEETS {
            if let Err(e) = sheets.load(files, "icons/", name) {
                log(&format!("icons {name}: {e}"));
            }
        }
        for name in SINGLE_IMAGES {
            if let Err(e) = sheets.load_single(files, name) {
                log(&format!("{name}: {e}"));
            }
        }
        for single in [PARTICLE_TEXTURE, STREAK_TEXTURE] {
            if let Err(e) = sheets.load_unscaled(files, single) {
                log(&format!("{single}: {e}"));
            }
        }
        self.ui.fonts = crate::ui::load_assets(files, &mut sheets).fonts;
        let rig = |f: &str| files.read(f).and_then(|b| crate::anim::AnimDesc::parse(&b).ok());
        if let (Some(r), Some(sp)) = (rig("Robot_AnimDesc.plist"), rig("Spider_AnimDesc.plist")) {
            self.ui.rigs = Some([r, sp]);
        }
        upload(&mut renderer, &mut sheets.images);
        log(&format!("loaded {} sheets, {} frames, backend {}", sheets.images.len(), sheets.frames.len(), renderer.backend));
        if self.ui.in_level() {
            match self.load_game() {
                Ok(mut game) => {
                    game.start_music();
                    // CADENCE_PRACTICE=1: start in practice mode (screenshots)
                    game.practice = std::env::var_os("CADENCE_PRACTICE").is_some();
                    self.game = Some(game);
                    self.ui.entered_level(self.level_id);
                    if let Ok(which) = std::env::var("CADENCE_SCREEN") {
                        let page = std::env::var("CADENCE_PAGE").ok().and_then(|p| p.parse().ok());
                        self.ui.open_for_screenshot(&which, page);
                    }
                }
                Err(e) => {
                    log(&format!("level: {e}"));
                    event_loop.exit();
                    return;
                }
            }
        }
        renderer.window.request_redraw();
        self.sheets = Some(sheets);
        self.renderer = Some(renderer);
    }
}

/// The menu screens `CADENCE_SCREEN` can open for a headless screenshot (`Ui::open_for_screenshot`).
const MENU_SCREENS: &[&str] = &[
    "levels", "locked", "online", "settings", "options", "garage", "colors", "stats", "creator", "achievements", "account", "login", "support", "songs", "howto", "video", "parental", "osearch", "oinfo", "odaily", "opacks", "ogauntlets", "oscores", "olists", "oquests", "opaths", "rewards", "news", "editor", "tower", "shop", "shopbuy", "secret", "treasure", "keys", "shards", "register", "accounthelp", "myprofile", "friends", "requests", "acctsettings", "share", "oldmessages",
];

/// The menus at startup: a level id of 0 opens the main menu, any other starts that level
/// (the menus are reached from its pause menu). Screenshots pick the screen with
/// `CADENCE_SCREEN` = menu | levels (`CADENCE_PAGE` = page) | pause | end.
fn new_ui(level_id: i32) -> crate::ui::Ui {
    use crate::ui::{Screen, Ui, save::Progress};
    let headless = std::env::var_os("CADENCE_SCREENSHOT").is_some() || std::env::var_os("CADENCE_BENCH").is_some() || std::env::var_os("CADENCE_UI_SCRIPT").is_some();
    // Headless runs never touch the save.
    // (a test save can be read, never written, with CADENCE_SAVE_DIR)
    let progress = if !headless {
        Progress::load()
    } else if std::env::var_os("CADENCE_SAVE_DIR").is_some() {
        Progress { persist: false, ..Progress::load() }
    } else {
        Progress::default()
    };
    let which = std::env::var("CADENCE_SCREEN").unwrap_or_default();
    let screen = match which.as_str() {
        "menu" => Screen::Menu,
        w if MENU_SCREENS.contains(&w) => Screen::Menu,
        w if w.starts_with("trig") => Screen::Menu,
        _ if level_id == 0 => Screen::Menu,
        _ => Screen::Level,
    };
    let mut ui = Ui::new(screen, Vec::new(), progress);
    ui.transitions = !headless;
    if MENU_SCREENS.contains(&which.as_str()) || which.starts_with("trig") {
        let page = std::env::var("CADENCE_PAGE").ok().and_then(|p| p.parse().ok());
        ui.open_for_screenshot(&which, page);
    }
    if let Some(secs) = std::env::var("CADENCE_UI_TIME").ok().and_then(|s| s.parse::<f32>().ok()) {
        ui.advance(secs);
        ui.skip_open_animations(secs);
    }
    ui
}

/// What the UI needs to know about the level after a frame.
fn level_status(game: &Game) -> crate::ui::LevelStatus {
    let length = game.world.level_length;
    let percent = if length > 0.0 { (game.player.x / length * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    // Secret coins are numbered left to right.
    let mut order: Vec<(f32, usize)> = game.coins.iter().map(|(i, r)| (r.x, *i)).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut coins = [false; 3];
    for (n, (_, index)) in order.iter().take(3).enumerate() {
        coins[n] = game.coins_taken.iter().any(|c| c.0 == *index);
    }
    crate::ui::LevelStatus { dead: game.player.dead, end_time: game.end_time, percent, coins, coin_count: order.len().min(3), steps: game.step_count, practice: game.practice }
}

impl App {
    /// Draw the main menu or the level select (no level loaded).
    fn draw_menu(&mut self) {
        // icon sheets the menu shows (the icon kit's page), loaded the first time
        if let (Some(renderer), Some(sheets)) = (self.renderer.as_mut(), self.sheets.as_mut()) {
            for name in self.ui.wanted_sheets() {
                if !sheets.frames.contains_key(&format!("{name}_001.png")) && !sheets.frames.contains_key(&format!("{name}_01_001.png")) && !sheets.failed.contains(&name) {
                    let before = sheets.images.len();
                    match sheets.load(self.files.as_ref(), "icons/", &name) {
                        Ok(()) => {
                            upload(renderer, &mut sheets.images[before..]);
                        }
                        Err(_) => {
                            sheets.failed.insert(name);
                        }
                    }
                }
            }
        }
        self.upload_pictures();
        let (Some(renderer), Some(sheets)) = (self.renderer.as_mut(), self.sheets.as_ref()) else { return };
        let dt = self.ui_clock.elapsed().as_secs_f32().min(0.1);
        self.ui_clock = Instant::now();
        if let Some(audio) = self.audio.as_deref_mut() {
            self.ui.play_audio(audio);
        }
        let screenshot = if self.script.is_some() { self.script_shot.take() } else { std::env::var_os("CADENCE_SCREENSHOT").map(PathBuf::from) };
        self.ui.draw_screen(renderer, sheets, dt, screenshot.as_deref());
        #[cfg(not(target_arch = "wasm32"))]
        if screenshot.is_some() && self.script.is_none() {
            std::process::exit(0);
        }
        renderer.window.request_redraw();
        self.frames += 1;
        let elapsed = self.fps_time.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            renderer.window.set_title(&format!("Cadence — {:.0} FPS — {}", self.frames as f32 / elapsed, renderer.backend));
            self.frames = 0;
            self.fps_time = Instant::now();
        }
    }

    /// Carry out what the UI asked for.
    fn apply_ui(&mut self, event_loop: &ActiveEventLoop) {
        use crate::ui::Action;
        if let Some((music, sfx)) = self.ui.take_volumes() {
            if let Some(a) = self.audio.as_deref_mut() {
                a.set_volumes(music, sfx);
            }
            if let Some(g) = self.game.as_mut() {
                g.audio.set_volumes(music, sfx);
            }
        }
        for action in self.ui.take_actions() {
            match action {
                // (the web page can't close itself)
                Action::Quit => {
                    // a UI script keeps going (its stress runs click Quit's Yes too)
                    if self.script.is_some() {
                        log("script: quit ignored");
                    } else if cfg!(not(target_arch = "wasm32")) {
                        event_loop.exit();
                    }
                }
                Action::Play(id) => {
                    self.online = None;
                    self.start_level(id)
                }
                Action::MenuMusic => {}
                // the editor's music button
                Action::EditorMusic(start) => {
                    if let Some(a) = self.audio.as_deref_mut() {
                        match (start, self.ui.editor_song.clone()) {
                            (Some(offset), Some(song)) => a.play_music(&song, offset),
                            _ => a.stop_music(),
                        }
                    }
                }
                Action::OpenLink(i) => {
                    if let Some(url) = crate::ui::LINKS.get(i as usize) {
                        open_link(url);
                    }
                }
                Action::Unlink => crate::gjapi::unlink(),
                // started by the UI itself (its confirm), never sent here
                Action::CloudSave | Action::CloudLoad | Action::AcceptRequest(_) | Action::ReportLevel(_) | Action::DeleteSaved(_) | Action::ToggleLdm(_) | Action::DeleteMine | Action::DeleteMyLevel(_) => {}
                Action::Practice(on) => {
                    if let Some(g) = self.game.as_mut() {
                        g.audio.set_paused(false);
                        g.holding = false;
                        g.last_frame = Instant::now();
                        g.set_practice(on);
                    }
                }
                Action::Video { fullscreen, width, height } => {
                    if let Some(r) = self.renderer.as_ref() {
                        if fullscreen {
                            r.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        } else {
                            r.window.set_fullscreen(None);
                            let _ = r.window.request_inner_size(winit::dpi::LogicalSize::new(width as f64, height as f64));
                        }
                    }
                }
                Action::PlayOnline => {
                    self.editor_level = None;
                    if let Some(l) = self.ui.take_online() {
                        let id = l.info.id;
                        self.online = Some(l);
                        self.start_level(id);
                    }
                }
                Action::Pause => {
                    if let Some(g) = self.game.as_mut() {
                        g.audio.set_paused(true);
                        g.holding = false;
                        g.direction = [false; 2];
                    }
                }
                Action::Resume => {
                    if let Some(g) = self.game.as_mut() {
                        g.audio.set_paused(false);
                        g.holding = false;
                        g.last_frame = Instant::now();
                    }
                }
                Action::Restart => {
                    if let Some(g) = self.game.as_mut() {
                        g.audio.set_paused(false);
                        g.holding = false;
                        g.last_frame = Instant::now();
                        g.reset();
                    }
                }
                Action::LeaveLevel | Action::LeaveEditor => {
                    if let Some(mut g) = self.game.take() {
                        g.audio.stop_music();
                        g.audio.set_paused(false);
                        self.audio = Some(std::mem::replace(&mut g.audio, Box::new(NoAudio)));
                    }
                    if action == Action::LeaveEditor {
                        self.editor_level = None;
                    }
                }
                Action::EnterEditor => {
                    if let Some(a) = self.audio.as_deref_mut() {
                        a.stop_music();
                    }
                    self.rebuild_editor();
                }
            }
        }
    }

    /// Run the UI script's commands up to the next frame (a wait or a shot).
    /// The news popup's pictures, as they finish downloading: one-frame sheets "news:<file>".
    fn upload_pictures(&mut self) {
        let (Some(renderer), Some(sheets)) = (self.renderer.as_mut(), self.sheets.as_mut()) else { return };
        for (file, image) in std::mem::take(&mut self.ui.online.news_pictures) {
            sheets.add_image(&format!("news:{file}"), image, 1.0);
            let last = sheets.images.len().saturating_sub(1);
            upload(renderer, &mut sheets.images[last..]);
        }
    }

    fn run_script(&mut self, event_loop: &ActiveEventLoop) {
        if self.script_wait > 0 {
            self.script_wait -= 1;
            return;
        }
        let px_per_pt = self.renderer.as_ref().map_or(2.25, |r| r.size().1 as f32 / 320.0);
        while let Some(line) = self.script.as_mut().and_then(|q| q.pop_front()) {
            let w: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| w.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            match w.first().copied() {
                Some("click") | Some("press") | Some("release") | Some("move") => {
                    let h = self.renderer.as_ref().map_or(720.0, |r| r.size().1 as f32);
                    self.ui.mouse_moved(num(1) * px_per_pt, h - num(2) * px_per_pt);
                    if w[0] == "click" || w[0] == "press" {
                        self.ui.mouse_button(true);
                    }
                    if w[0] == "click" || w[0] == "release" {
                        self.ui.mouse_button(false);
                    }
                    self.apply_ui(event_loop);
                }
                Some("wheel") => self.ui.scroll(num(1)),
                Some("key") => {
                    let code = match w.get(1).copied().unwrap_or("") {
                        "Escape" => Some(KeyCode::Escape),
                        "Space" => Some(KeyCode::Space),
                        "Enter" => Some(KeyCode::Enter),
                        "Left" => Some(KeyCode::ArrowLeft),
                        "Right" => Some(KeyCode::ArrowRight),
                        "Up" => Some(KeyCode::ArrowUp),
                        "Down" => Some(KeyCode::ArrowDown),
                        "Delete" => Some(KeyCode::Delete),
                        "Backspace" => Some(KeyCode::Backspace),
                        "A" => Some(KeyCode::KeyA),
                        "D" => Some(KeyCode::KeyD),
                        "Z" => Some(KeyCode::KeyZ),
                        "X" => Some(KeyCode::KeyX),
                        "1" => Some(KeyCode::Digit1),
                        "2" => Some(KeyCode::Digit2),
                        "3" => Some(KeyCode::Digit3),
                        "4" => Some(KeyCode::Digit4),
                        _ => None,
                    };
                    if let Some(k) = code {
                        self.ui.key(k, true);
                        self.ui.key(k, false);
                        self.apply_ui(event_loop);
                    }
                }
                Some("text") => {
                    let _ = self.ui.text(&w[1..].join(" "));
                }
                Some("wait") => {
                    self.script_wait = num(1) as u32;
                    return;
                }
                Some("shot") => {
                    self.script_shot = w.get(1).map(PathBuf::from);
                    return;
                }
                _ => log(&format!("script: unknown line {line:?}")),
            }
        }
        if self.script.as_ref().is_some_and(|q| q.is_empty()) && self.script_shot.is_none() {
            log("script: done");
            std::process::exit(0);
        }
    }

    /// Build the level the editor shows from its current objects.
    fn rebuild_editor(&mut self) {
        let Some(text) = self.ui.take_editor_level().or_else(|| self.editor_level.clone()) else { return };
        if let Some(mut g) = self.game.take() {
            self.audio = Some(std::mem::replace(&mut g.audio, Box::new(NoAudio)));
        }
        self.editor_level = Some(text);
        match self.load_game() {
            Ok(mut game) => {
                game.last_frame = Instant::now();
                self.game = Some(game);
            }
            Err(e) => log(&format!("editor: {e}")),
        }
    }

    /// Load and start a level chosen in the level select.
    fn start_level(&mut self, id: i32) {
        if let Some(mut g) = self.game.take() {
            self.audio = Some(std::mem::replace(&mut g.audio, Box::new(NoAudio)));
        }
        if let Some(a) = self.audio.as_deref_mut() {
            a.stop_music();
        }
        self.level_id = id;
        match self.load_game() {
            Ok(mut game) => {
                game.start_music();
                game.last_frame = Instant::now();
                self.game = Some(game);
                self.ui.entered_level(id);
            }
            Err(e) => log(&format!("level {id}: {e}")),
        }
    }
}

/// Open `url` in the browser (the menu's links).
fn open_link(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32"), target_os = "windows"))]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32"), not(target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
}

thread_local! {
    /// Debug: summed per-object block times (visual, scale/pulse, colours, glow, sprites).
    static BLOCKS: std::cell::RefCell<[f64; 5]> = const { std::cell::RefCell::new([0.0; 5]) };
    /// Debug: summed draw phase times (objects, after the objects, whole compose) and frames.
    static PHASES: std::cell::RefCell<(f64, f64, f64, usize)> = const { std::cell::RefCell::new((0.0, 0.0, 0.0, 0)) };
    /// Benchmarks: total simulation time and frames.
    static TICK_TOTAL: std::cell::RefCell<(f64, usize)> = const { std::cell::RefCell::new((0.0, 0)) };
}

/// Print frame-time statistics for a benchmark run.
fn report_bench(samples: &[(f64, f64, usize, usize, usize)]) {
    let skip = samples.len().min(30); // loading
    let s = &samples[skip..];
    let stats = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        let p = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
        format!("mean {mean:.2} sd {sd:.2} p50 {:.2} p95 {:.2} p99 {:.2} max {:.2}", p(0.5), p(0.95), p(0.99), v[v.len() - 1])
    };
    let intervals: Vec<f64> = s.iter().map(|x| x.0).collect();
    let median = {
        let mut v = intervals.clone();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let long = intervals.iter().filter(|&&x| x > median * 1.5).count();
    let mut hist = [0usize; 9];
    for x in s {
        hist[x.2.min(8)] += 1;
    }
    log(&format!("bench: {} frames", s.len()));
    log(&format!("  interval ms: {}", stats(intervals)));
    log(&format!("  frames longer than 1.5x median ({median:.2} ms): {long}"));
    log(&format!("  simulate + compose ms per frame: {}", stats(s.iter().map(|x| x.1).collect())));
    log(&format!("  steps per frame 0..8: {hist:?}"));
    log(&format!("  sprites per frame: {}", stats(s.iter().map(|x| x.3 as f64).collect())));
    TICK_TOTAL.with(|t| { let t = t.borrow(); log(&format!("  simulation ms per frame (of the above): mean {:.2}", t.0 / t.1.max(1) as f64)); });
    log(&format!("  draw calls per frame: {}", stats(s.iter().map(|x| x.4 as f64).collect())));
}

/// A development flag: natively `CADENCE_<NAME>` in the environment, on the web `?<name>` in the
/// page's address (e.g. noclip: deaths don't count and losing focus doesn't pause, for tests).
/// Upload sheet images to the GPU, then free their pixels: only the GPU copy is used after
/// that (the CPU copies of every sheet were about half of Cadence's memory).
fn upload(renderer: &mut Renderer, images: &mut [crate::assets::Image]) {
    for image in images {
        renderer.add_sheet(image);
        image.rgba = Vec::new();
    }
}

pub fn dev_flag(name: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var_os(format!("CADENCE_{}", name.to_ascii_uppercase())).is_some()
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| q.trim_start_matches('?').split('&').any(|kv| kv.split('=').next() == Some(name)))
    }
}

pub fn log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&message.into());
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
}

impl ApplicationHandler<AppEvent> for App {
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // level files dropped on the web page
        #[cfg(target_arch = "wasm32")]
        for bytes in std::mem::take(&mut *crate::web::IMPORTS.lock().unwrap_or_else(|e| e.into_inner())) {
            self.ui.import_level(&bytes);
        }
        if let Some(r) = self.renderer.as_ref() {
            r.window.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() || self.window_created {
            return;
        }
        self.window_created = true;
        // CADENCE_WINDOW=WxH (benchmarks): another window size, in points
        let (w, h) = std::env::var("CADENCE_WINDOW")
            .ok()
            .and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?))
            })
            .unwrap_or((1280.0, 720.0));
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("Cadence").with_inner_size(winit::dpi::LogicalSize::new(w, h));
        // CADENCE_CORNER (benchmarks while the machine is in use): the window never takes focus
        // and is kept on top, parked past the main screen's bottom-right corner (`park_in_corner`).
        if dev_flag("corner") {
            attrs = attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            if let Some(m) = event_loop.primary_monitor() {
                let (pos, size) = (m.position().to_logical::<f64>(m.scale_factor()), m.size().to_logical::<f64>(m.scale_factor()));
                attrs = attrs.with_position(winit::dpi::LogicalPosition::new(pos.x + size.width - 320.0, pos.y + size.height - 300.0));
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_append(true);
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        #[cfg(target_os = "macos")]
        if dev_flag("corner") {
            park_in_corner(&window);
        }
        #[cfg(not(target_arch = "wasm32"))]
        match pollster::block_on(Renderer::new(window)) {
            Ok(renderer) => self.finish_setup(renderer, event_loop),
            Err(e) => {
                log(&format!("renderer: {e}"));
                event_loop.exit();
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Renderer::new(window).await {
                    Ok(renderer) => {
                        let _ = proxy.send_event(AppEvent::RendererReady(renderer));
                    }
                    Err(e) => log(&format!("renderer: {e}")),
                }
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::RendererReady(renderer) => self.finish_setup(renderer, event_loop),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // a level file dropped on the window: into your levels (`.gmd`)
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::DroppedFile(path) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    if !self.ui.import_level(&bytes) {
                        log(&format!("import: {} isn't a level file", path.display()));
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.run_script(event_loop);
                // in a level (or the editor) without a built game (a failed load or rebuild),
                // the menus draw instead of a frozen frame
                if self.ui.in_level() && self.game.is_none() && !self.warned_no_game {
                    self.warned_no_game = true;
                    log(&format!("in a level ({:?}) without a game: drawing the menus", self.ui.screen));
                }
                if self.ui.in_level() && self.game.is_some() { self.draw() } else { self.draw_menu() }
                self.apply_ui(event_loop);
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.ui.held_mods = (self.mods.shift_key(), self.mods.control_key() || self.mods.super_key());
                self.ui.held_alt = self.mods.alt_key();
            }
            WindowEvent::CursorMoved { position, .. } => self.ui.mouse_moved(position.x as f32, position.y as f32),
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, d) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32 * 0.5, p.y as f32 * 0.5),
                };
                let (shift, ctrl) = (self.mods.shift_key(), self.mods.control_key() || self.mods.super_key());
                if !self.ui.editor_wheel(dx, d, shift, ctrl) {
                    self.ui.scroll(d);
                }
            }
            // Losing focus pauses a level, but not in headless runs (benchmarks), whose hidden
            // window never has focus.
            WindowEvent::Focused(false) if self.bench.is_none() && std::env::var_os("CADENCE_SCREENSHOT").is_none() && self.script.is_none() && !dev_flag("noclip") => {
                self.ui.focus_lost();
                self.apply_ui(event_loop);
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.text.as_ref().is_some_and(|t| !t.chars().any(char::is_control))
                    && self.ui.text(event.text.as_deref().unwrap_or("")) => {}
            WindowEvent::KeyboardInput { event, .. } if !event.repeat && matches!(event.physical_key, PhysicalKey::Code(k) if self.ui.key(k, event.state == ElementState::Pressed)) => {
                self.apply_ui(event_loop);
            }
            WindowEvent::KeyboardInput { .. } if !self.ui.in_level() || !self.ui.level_running() || self.ui.hold_end() => {}
            WindowEvent::KeyboardInput { event, .. } => {
                // the keybinds' gameplay actions (Jump, Move, the checkpoints, Restart …)
                let down = event.state == ElementState::Pressed;
                let PhysicalKey::Code(key) = event.physical_key else { return };
                use crate::ui::keybinds::act;
                for a in self.ui.play_actions(key, down) {
                    let Some(g) = self.game.as_mut() else { break };
                    match a {
                        act::JUMP_P1 | act::JUMP_P2 => {
                            if down && !event.repeat && !g.holding {
                                self.ui.jumped();
                            }
                            g.holding = down;
                        }
                        act::LEFT_P1 | act::LEFT_P2 => g.direction[0] = down,
                        act::RIGHT_P1 | act::RIGHT_P2 => g.direction[1] = down,
                        act::PLACE_CHECKPOINT if down && !event.repeat => g.place_checkpoint(),
                        act::DELETE_CHECKPOINT if down && !event.repeat => g.remove_checkpoint(),
                        act::RESTART | act::FULL_RESET if down => g.reset(),
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let down = state == ElementState::Pressed;
                // The UI takes clicks on its buttons and while a menu is up; the rest jump.
                let used = self.ui.mouse_button(down);
                if let Some(g) = self.game.as_mut()
                    && (!used || !down)
                {
                    if down && !g.holding {
                        self.ui.jumped();
                    }
                    g.holding = down && !used;
                }
                self.apply_ui(event_loop);
            }
            _ => {}
        }
    }
}

/// CADENCE_CORNER: only the window's top-left 320 × 200 points show, in the main screen's
/// bottom-right corner just above the Dock; the rest is past the screen's edges, and the mouse
/// passes through. Still visible, so frames keep the display's pace (a hidden or fully covered
/// window isn't paced by vsync), while the drawn size stays what CADENCE_WINDOW says. The harness
/// mod's `corner` parks the real game the same way.
#[cfg(target_os = "macos")]
fn park_in_corner(window: &Window) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSPoint, NSRect};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    unsafe {
        let view = handle.ns_view.as_ptr() as *mut AnyObject;
        let win: *mut AnyObject = msg_send![view, window];
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let screen: *mut AnyObject = msg_send![screens, firstObject];
        if win.is_null() || screen.is_null() {
            return;
        }
        // (AppKit's coordinates: y up, the visible frame stops at the Dock and the menu bar)
        let shown: NSRect = msg_send![screen, visibleFrame];
        let frame: NSRect = msg_send![win, frame];
        let to = NSPoint::new(shown.origin.x + shown.size.width - 320.0, shown.origin.y + 200.0 - frame.size.height);
        let _: () = msg_send![win, setFrameOrigin: to];
        let _: () = msg_send![win, setIgnoresMouseEvents: true];
        log(&format!("corner: window {}x{} at {},{}", frame.size.width, frame.size.height, to.x, to.y));
    }
}

/// Run the game on the given files. Blocks on native; on the web it returns and the browser
/// drives the event loop.
pub fn run(files: Box<dyn Files>, audio: Box<dyn Audio>, level_id: i32, quality: Quality) {
    #[allow(unused_mut)]
    let mut builder = EventLoop::<AppEvent>::with_user_event();
    // a CADENCE_CORNER window doesn't take focus from the app in front at launch
    #[cfg(target_os = "macos")]
    if dev_flag("corner") {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_activate_ignoring_other_apps(false);
    }
    let event_loop = builder.build().expect("event loop");
    // Wait, with a redraw requested after each round of events (about_to_wait): frames come at
    // the display's pace. (Poll made the web loop schedule a browser task per turn, keeping the
    // one CPU core busy between frames.)
    event_loop.set_control_flow(if dev_flag("poll") { ControlFlow::Poll } else { ControlFlow::Wait });
    let app = App {
        files,
        audio: Some(audio),
        proxy: event_loop.create_proxy(),
        level_id,
        // CADENCE_ONLINE=<level id>: play an online level straight away.
        online: {
            #[cfg(not(target_arch = "wasm32"))]
            let o = std::env::var("CADENCE_ONLINE").ok().and_then(|v| v.parse().ok()).and_then(|id| match crate::ui::online::Browser::fetch_now(id) {
                Ok(l) => Some(l),
                Err(e) => {
                    log(&format!("online {id}: {e}"));
                    None
                }
            });
            #[cfg(target_arch = "wasm32")]
            let o = None;
            o
        },
        editor_level: None,
        script: std::env::var("CADENCE_UI_SCRIPT").ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().map(str::to_string).filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect()),
        script_shot: None,
        script_wait: 0,
        warned_no_game: false,
        mods: Default::default(),
        quality,
        sheets: None,
        renderer: None,
        game: None,
        window_created: false,
        bench: std::env::var("CADENCE_BENCH").ok().and_then(|v| v.parse::<f64>().ok()).map(|secs| (Vec::new(), Instant::now(), secs)),
        last_draw: None,
        frames: 0,
        fps_time: Instant::now(),
        ui: new_ui(level_id),
        shot_queue: None,
        shot_song: None,
        shot_song_time: 0.0,
        ui_clock: Instant::now(),
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        event_loop.run_app(&mut app).expect("run");
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real game's exit-dual copy (re/traces/dualdeath, player 2 died, zoom 1): six frames
    /// after the death the copy is 25.965 back, at scale 0.8125, turned 37.5 degrees, opacity 201.
    #[test]
    fn exit_dual_copy_matches_the_capture() {
        let mut p = Player::default();
        p.x = 682.883;
        p.y = 105.0;
        let mut e = ExitDual::new(&p, ([0.5; 3], [1.0; 3]), 1.0, false);
        for _ in 0..6 {
            e.step(1.0 / 60.0);
        }
        let (copy, a) = e.state();
        assert!((copy.x - (682.883 - 25.965)).abs() < 0.01, "{}", copy.x);
        assert!((copy.scale - 0.8125).abs() < 1e-4);
        assert!((copy.rotation - 37.5).abs() < 1e-3);
        assert_eq!((a * 255.0) as i32, 201);
    }
}

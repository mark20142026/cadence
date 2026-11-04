//! Animated level objects: what each frame of an animated decoration shows.
//!
//! Two kinds, both from the GD 2.2091 decompile (NOTES "Animated objects"):
//! - **Frame animations** (`EnhancedGameObject`, 125 ids: fire, waves, gj22_anim monsters, lava,
//!   drops, pixel art...). `GameManager`'s table (FUN_1001f2b60 → `addGameAnimation` 0x1f93ec)
//!   gives each id its frames and frame time; `EnhancedGameObject::updateSyncedAnimation`
//!   0x1efde4 (called by `PlayLayer::updateVisibility` with the layer time +0x398) picks the frame
//!   from the time, so these stay in step with the level: frame = ⌊(t + offset) / (frameTime /
//!   speed)⌋ mod frames. `customSetup` 0x5bb8dc gives most ids a random speed 0.8–1.2; the start
//!   offset is random only with "randomize start" (key 106).
//! - **Part animations** (`AnimatedGameObject` 0x156c04: 918/1327/1328/1584/2012 beasts, 919 black
//!   sludge): a `CCAnimatedSprite` playing a `*_AnimDesc.plist` (`CCPartAnimSprite`), each frame
//!   placing the parts; started at a random point (`updateObjectAnimation` 0x157898 →
//!   `offsetCurrentAnimation(rand)`).
//!
//! The app asks [`AnimObjects::sprites`] for an object's sprite tree this frame and draws it
//! like the dumped static tree (same parents, z, colors: parts take the main color, color sprites
//! the detail color), so edge fades, enter effects and opacity apply unchanged.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cadence_core::level::RawObject;
use cadence_core::objects::{ObjectVisual, SpriteDef, SpriteRole};

use crate::anim::AnimDesc;
use crate::files::Files;

#[path = "animobj_table.rs"]
mod table;

/// How far the layer time (+0x398) the game animates by is ahead of the attempt's step time.
/// Fitted: d_animWave frames in 7 real scene dumps (levels 19–21) all fit 0.067–0.083 s; one
/// level 22 sample wants ≥ 0.086 (how +0x398 runs ahead isn't traced yet).
const LAYER_TIME_LEAD: f64 = 0.075;

/// How `updateSyncedAnimation` shows an id's frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// The main sprite takes the frames; the color sprite (a child of the main sprite, z −100)
    /// the color frames (label LAB_1001f0668).
    Main,
    /// The main sprite takes the frames; the color sprite stays on its first frame, undrawn, and
    /// its child (scale 2: the color art is half resolution) takes the color frames
    /// (LAB_1001f047c: fire 920–924, fire_b, most gj22_anim).
    ColorChild,
    /// The main sprite stays (undrawn); its child (tag 1) takes the frames and the color sprite
    /// the color frames (1516 waterfall, 1518 splash; LAB_1001f06b0).
    ChildFrames,
    /// d_animWave: frames 1–10 played as a ping-pong of 6 frames, the way back flipped in x, on
    /// the main sprite's child (tag 1); 3000–3002 also on the color sprite's child. `first`:
    /// 1050/3000's own mapping (frames 4, 5, 9, 10 → 3, 2, 8, 7 flipped).
    Wave { color: bool, first: bool },
    /// Lava tops: 1591 plays a random lava animation (2058 top 70%·70%, 2060 bubble02 70%·30%,
    /// 2059 bubble 30%) each cycle, starting with 2058; 1593 always 2058.
    Lava { random: bool },
    /// 2892/2893: 16 steps of `step` seconds; the child (tag 1) and the color sprite's child
    /// cycle `frames` frames and turn −22.5° per step (2892) or −90° per 4 steps (2893).
    Spin { step: f32, frames: u32, quarter: bool },
}

/// Where a randomized start ("randomize start", key 106) puts the animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Start {
    /// `saveActiveColors` 0x5bb414: rand() seconds in [0, 1).
    Unit,
    /// customSetup: rand·0.5 s.
    Half,
    /// customSetup: 70% rand·0.5 s, else 0.
    Half70,
}

pub struct FrameAnim {
    pub id: u16,
    /// Seconds per frame (`frameTimeForAnimation`).
    pub delay: f32,
    pub kind: Kind,
    /// `usesFreezeAnimation`: after the last frame the object hides for rand·0.5 + 0.2 s.
    pub freeze: bool,
    /// customSetup's random speed 0.8–1.2 (+0x55c).
    pub random_speed: bool,
    pub start: Start,
    pub frames: &'static [&'static str],
    pub colors: &'static [&'static str],
}

/// One part animation of an `objectDefinitions.plist` type.
pub struct PartAnimDef {
    pub kind: &'static str,
    pub anim: &'static str,
    pub delay: f32,
    pub frames: u32,
    pub looped: bool,
}

/// The AnimatedGameObjects: id, type, default animation, content size (`setupAnimatedSize`
/// 0x156da0), the part that is the color sprite (`setupChildSprites` 0x156f30), the glows that
/// follow parts (part, frame), and the chance the next idle is idle01 (`animationFinished`
/// 0x157c1c; else idle02).
struct PartObject {
    id: u16,
    kind: &'static str,
    default: &'static str,
    size: (f32, f32),
    color_part: Option<u32>,
    glows: &'static [(u32, &'static str)],
    idle01: f32,
}

const PART_OBJECTS: &[PartObject] = &[
    // GJBeast01: every part has a glow; its color sprite is a custom child of part 1.
    PartObject { id: 918, kind: "GJBeast01", default: "bite", size: (48.0, 48.0), color_part: None, glows: &[(0, "GJBeast01_02_glow_001.png"), (1, "GJBeast01_01_glow_001.png")], idle01: 1.0 },
    PartObject { id: 919, kind: "BlackSludge", default: "loop", size: (25.0, 6.0), color_part: None, glows: &[], idle01: 1.0 },
    PartObject { id: 1327, kind: "GJBeast02", default: "idle01", size: (8.0, 8.0), color_part: Some(0), glows: &[(1, "GJBeast02_01_glow_001.png")], idle01: 0.75 },
    PartObject { id: 1328, kind: "GJBeast03", default: "idle01", size: (8.0, 15.0), color_part: Some(0), glows: &[(1, "GJBeast03_01_glow_001.png")], idle01: 0.9 },
    PartObject { id: 1584, kind: "GJBeast04", default: "idle01", size: (8.0, 8.0), color_part: Some(0), glows: &[(1, "GJBeast04_04_glow_001.png"), (2, "GJBeast04_02_glow_001.png"), (3, "GJBeast04_01_glow_001.png")], idle01: 0.75 },
    PartObject { id: 2012, kind: "GJBeast05", default: "idle01", size: (8.0, 8.0), color_part: Some(1), glows: &[(0, "GJBeast05_02_glow_001.png")], idle01: 0.75 },
];

/// GJBeast01's color sprite: `GJBeast01_03_001.png` added to part 1 at its center, z 1.
const BEAST01_COLOR: &str = "GJBeast01_03_001.png";

/// A part animation frame with interned names.
#[derive(Clone, Copy)]
struct RigPart {
    frame: &'static str,
    pos: (f32, f32),
    scale: (f32, f32),
    rotation: f32,
    z: i32,
    tag: u32,
}

/// One AnimDesc type: animation name → frames → parts.
#[derive(Default)]
struct Rig {
    anims: HashMap<String, Vec<Vec<RigPart>>>,
}

/// Interns frame names (part textures come from plists at run time; sprite defs need 'static).
fn intern(s: &str) -> &'static str {
    static SET: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let mut set = SET.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&v) = set.get(s) {
        return v;
    }
    let v: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(v);
    v
}

impl Rig {
    fn from_desc(desc: &AnimDesc, kind: &str) -> Rig {
        let mut rig = Rig::default();
        for def in table::PART_ANIMS.iter().filter(|d| d.kind == kind) {
            let n = desc.len(def.anim);
            let frames = (0..n)
                .filter_map(|i| desc.frame(def.anim, i))
                .map(|parts| {
                    parts
                        .iter()
                        .map(|p| {
                            let name = if p.texture.ends_with(".png") { p.texture.clone() } else { format!("{}_001.png", p.texture) };
                            // Flips are negative scales (`transformSprite` 0x2ec9dc).
                            let sx = if p.flip.0 { -p.scale.0 } else { p.scale.0 };
                            let sy = if p.flip.1 { -p.scale.1 } else { p.scale.1 };
                            RigPart { frame: intern(&name), pos: p.pos, scale: (sx, sy), rotation: p.rotation, z: p.z, tag: p.tag }
                        })
                        .collect()
                })
                .collect();
            rig.anims.insert(def.anim.to_string(), frames);
        }
        rig
    }
}

/// Per-object settings from the level string and customSetup's randomness.
#[derive(Clone, Copy, Debug, Default)]
struct ObjAnim {
    /// Seconds added to the layer time (+0x548).
    offset: f32,
    /// Animation speed (+0x55c, or |key 107| with key 122).
    speed: f32,
    /// A negative custom speed plays backwards.
    reverse: bool,
    /// Key 462 single frame (+0x58c) and key 592 "offset anim" (+0x590).
    single: i32,
    single_offset: bool,
    /// Key 123: waits invisible for an animate trigger (+0x589; `waitForAnimationTrigger`).
    on_trigger: bool,
    /// Key 126 disables the freeze (delayed loop).
    no_freeze: bool,
    seed: u64,
}

/// A per-object random number in [0, 1), stable for (seed, n).
fn rand01(seed: u64, n: u64) -> f32 {
    let mut z = seed ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(0x632b_e59b_d9b4_e019);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn frame_anim(id: i32) -> Option<&'static FrameAnim> {
    let id = u16::try_from(id).ok()?;
    table::FRAME_ANIMS.binary_search_by_key(&id, |a| a.id).ok().map(|i| &table::FRAME_ANIMS[i])
}

fn part_object(id: i32) -> Option<&'static PartObject> {
    PART_OBJECTS.iter().find(|p| p.id as i32 == id)
}

fn part_def(kind: &str, anim: &str) -> Option<&'static PartAnimDef> {
    table::PART_ANIMS.iter().find(|d| d.kind == kind && d.anim == anim)
}

/// The files the part animations need (for the web build's file list).
pub const FILES: &[&str] = &[
    "BlackSludge_AnimDesc.plist",
    "GJBeast01_AnimDesc.plist",
    "GJBeast02_AnimDesc.plist",
    "GJBeast03_AnimDesc.plist",
    "GJBeast04_AnimDesc.plist",
    "GJBeast05_AnimDesc.plist",
];

/// The level's animated objects and their playback state.
#[derive(Default)]
pub struct AnimObjects {
    /// By level index: settings for animated objects.
    objects: Vec<Option<ObjAnim>>,
    rigs: HashMap<&'static str, Rig>,
    /// Cycle cache for objects whose cycles have random lengths (freeze delays, lava, idles):
    /// (start of the current cycle in effective seconds, its number, its choice).
    cycles: Vec<(f32, u32, u32)>,
}

impl AnimObjects {
    pub fn new(files: &dyn Files, objects: &[RawObject]) -> AnimObjects {
        let mut out = AnimObjects { objects: vec![None; objects.len()], cycles: vec![(0.0, 0, 0); objects.len()], ..Default::default() };
        for (i, obj) in objects.iter().enumerate() {
            let id = obj.canonical_id();
            let seed = (i as u64).wrapping_mul(0x2545_f491_4f6c_dd1d) ^ (id as u64) << 32;
            if let Some(p) = part_object(id) {
                if !out.rigs.contains_key(p.kind) {
                    let rig = files
                        .read(&format!("{}_AnimDesc.plist", p.kind))
                        .and_then(|b| AnimDesc::parse(&b).ok())
                        .map(|d| Rig::from_desc(&d, p.kind))
                        .unwrap_or_default();
                    out.rigs.insert(p.kind, rig);
                }
                out.objects[i] = Some(ObjAnim { speed: 1.0, seed, ..Default::default() });
                continue;
            }
            if cogwheel_copies(id).is_some() {
                out.objects[i] = Some(ObjAnim { speed: 1.0, seed, ..Default::default() });
                continue;
            }
            let Some(anim) = frame_anim(id).or(if matches!(id, 1591 | 1593) { frame_anim(2058) } else { None }) else { continue };
            let custom = obj.get(122) == Some("1") && obj.get_f32(107).is_some_and(|v| v != 0.0);
            let speed = if custom {
                obj.get_f32(107).unwrap_or(1.0).abs()
            } else if anim.random_speed {
                rand01(seed, 1) * 0.4 + 0.8
            } else {
                1.0
            };
            let offset = if obj.get(106) == Some("1") {
                match anim.start {
                    Start::Unit => rand01(seed, 2),
                    Start::Half => rand01(seed, 2) * 0.5,
                    Start::Half70 if rand01(seed, 3) <= 0.7 => rand01(seed, 2) * 0.5,
                    Start::Half70 => 0.0,
                }
            } else {
                0.0
            };
            out.objects[i] = Some(ObjAnim {
                offset,
                speed,
                reverse: custom && obj.get_f32(107).unwrap_or(1.0) < 0.0,
                single: obj.get_i32(462).unwrap_or(0),
                single_offset: obj.get(592) == Some("1"),
                on_trigger: obj.get(123) == Some("1"),
                no_freeze: obj.get(126) == Some("1"),
                seed,
            });
        }
        out
    }

    /// The object's sprite tree at layer time `time` (seconds since the attempt started), or
    /// None if it isn't animated. `frame_size` gives a frame's content size.
    /// Whether object `index` is animated (`sprites` gives it a sprite tree every frame).
    pub fn is_animated(&self, index: usize) -> bool {
        self.objects.get(index).is_some_and(Option::is_some)
    }

    pub fn sprites(&mut self, index: usize, id: i32, visual: &ObjectVisual, time: f64, started: Option<f64>, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> Option<Vec<SpriteDef>> {
        let mut o = (*self.objects.get(index)?)?;
        // started by an animate trigger: plays from then like any animated object
        let time = match started {
            Some(at) if o.on_trigger => {
                o.on_trigger = false;
                (time - at).max(0.0)
            }
            _ => time,
        };
        if let Some(p) = part_object(id) {
            return self.part_sprites(index, p, o, (time + LAYER_TIME_LEAD) as f32, visual, frame_size);
        }
        if let Some(copies) = cogwheel_copies(id) {
            return Some(cogwheel(visual, copies, frame_size));
        }
        let t = (time + LAYER_TIME_LEAD) as f32 + o.offset;
        let lava = matches!(id, 1591 | 1593);
        let anim = if lava { frame_anim(2058)? } else { frame_anim(id)? };
        let mut out: Vec<SpriteDef> = visual.sprites.to_vec();
        if o.on_trigger {
            // Invisible until an animate trigger (1585) plays it.
            out.iter_mut().for_each(|s| s.visible = false);
            return Some(out);
        }
        // Frame (1-based) and which animation's frames.
        let mut anim = anim;
        let k: u32;
        match anim.kind {
            _ if lava => {
                let (a, kk) = self.lava(index, id == 1591, t, o.seed);
                anim = a;
                k = kk;
            }
            Kind::Spin { step, frames, quarter } => {
                let u = (t / (step / o.speed)) as i64;
                let v = (u & 15) as u32;
                let idx = if o.reverse { 16 - v } else { v + 1 };
                let f = idx % frames + 1;
                let rot = (if quarter { ((idx as f32 / 4.0) as i32 as f32 * -90.0) as i32 } else { (idx as f32 * -22.5) as i32 }) as f32;
                let (main, color) = (anim.frames.get(f as usize - 1).copied(), anim.colors.get(f as usize - 1).copied());
                hide_root(&mut out);
                if let Some(c) = first_child(&out) {
                    set_frame(&mut out[c], main, frame_size);
                    out[c].rotation = rot;
                }
                if let Some(color) = color {
                    let (parent, child) = add_color_child(&mut out, anim.colors[0], 1.0, frame_size);
                    out[parent].visible = false;
                    set_frame(&mut out[child], Some(color), frame_size);
                    out[child].rotation = rot;
                }
                return Some(out);
            }
            _ => {
                let fd = anim.delay / o.speed;
                let n = anim.frames.len() as u32;
                if anim.freeze && !o.no_freeze {
                    match self.freeze(index, t, fd, n, o.seed) {
                        Some(kk) => k = kk,
                        // Hidden between plays (+0x306 → opacity 0); 1855 holds its last frame.
                        None if id == 1855 => k = n,
                        None => {
                            out.iter_mut().for_each(|s| s.visible = false);
                            return Some(out);
                        }
                    }
                } else {
                    let mut i = ((t / fd) as i32 as f32).max(0.0) as i32;
                    if o.single > 0 {
                        if !o.single_offset {
                            i = o.single - 1;
                        } else {
                            i += o.single;
                        }
                    }
                    let i = i.rem_euclid(n as i32) as u32;
                    k = if o.reverse { n - i } else { i + 1 };
                }
            }
        }
        let main = anim.frames.get(k as usize - 1).copied();
        let color = anim.colors.get(k as usize - 1).copied();
        match anim.kind {
            Kind::Main | Kind::Lava { .. } => {
                set_frame(&mut out[0], main, frame_size);
                if let Some(color) = color {
                    match out.iter().position(|s| s.role == SpriteRole::Detail) {
                        Some(d) => set_frame(&mut out[d], Some(color), frame_size),
                        None => {
                            // The color sprite: under the main sprite (z −100), over it for lava (z 100).
                            let z = if lava { 100 } else { -100 };
                            let c = add_child(&mut out, 0, color, z, 1.0, frame_size);
                            out[c].role = SpriteRole::Detail;
                        }
                    }
                }
            }
            Kind::ColorChild => {
                set_frame(&mut out[0], main, frame_size);
                if let Some(color) = color {
                    let (parent, child) = add_color_child(&mut out, anim.colors[0], 2.0, frame_size);
                    out[parent].visible = false;
                    set_frame(&mut out[child], Some(color), frame_size);
                }
            }
            Kind::ChildFrames => {
                hide_root(&mut out);
                if let Some(c) = first_child(&out) {
                    set_frame(&mut out[c], main, frame_size);
                }
                if let (Some(color), Some(d)) = (color, out.iter().position(|s| s.role == SpriteRole::Detail)) {
                    set_frame(&mut out[d], Some(color), frame_size);
                }
            }
            Kind::Wave { color: has_color, first } => {
                // updateSyncedAnimation's ping-pong (ids 0x41a–0x41c, 3000–3002).
                let (f, flip) = match (first, k) {
                    (true, 4) => (3, true),
                    (true, 5) => (2, true),
                    (true, 9) => (8, true),
                    (true, 10) => (7, true),
                    (false, 7) => (5, true),
                    (false, 8) => (4, true),
                    (false, 9) => (3, true),
                    (false, 10) => (2, true),
                    _ => (k, false),
                };
                let main = anim.frames.get(f as usize - 1).copied();
                if has_color {
                    hide_root(&mut out);
                }
                let c = match first_child(&out) {
                    Some(c) => c,
                    None => add_child(&mut out, 0, anim.frames[0], -1, 1.0, frame_size),
                };
                set_frame(&mut out[c], main, frame_size);
                out[c].flip_x = flip;
                if has_color {
                    if let Some(color) = anim.colors.get(f as usize - 1).copied() {
                        let (parent, child) = add_color_child(&mut out, anim.colors[0], 1.0, frame_size);
                        out[parent].visible = false;
                        set_frame(&mut out[child], Some(color), frame_size);
                        out[child].flip_x = flip;
                    }
                }
            }
            Kind::Spin { .. } => {}
        }
        Some(out)
    }

    /// A freeze-animation frame: cycles of frames + 1 frame times plus a random 0.2–0.7 s,
    /// hidden after the last frame (`updateSyncedAnimation`'s +0x54c/+0x550/+0x554 delay).
    fn freeze(&mut self, index: usize, t: f32, fd: f32, n: u32, seed: u64) -> Option<u32> {
        let cache = &mut self.cycles[index];
        if t < cache.0 {
            *cache = (0.0, 0, 0);
        }
        let cycle = (n + 1) as f32 * fd;
        loop {
            let len = cycle + 0.2 + 0.5 * rand01(seed, 100 + cache.1 as u64);
            if t < cache.0 + len || cache.1 > 1_000_000 {
                break;
            }
            cache.0 += len;
            cache.1 += 1;
        }
        let i = ((t - cache.0) / fd).max(0.0) as u32;
        (i < n).then_some(i + 1)
    }

    /// Lava (0x637/0x639 in updateSyncedAnimation): the animation for this cycle and its frame.
    /// Frame times are fixed (0.13 s for 2058, 0.12 s for the bubbles), not scaled by speed.
    fn lava(&mut self, index: usize, random: bool, t: f32, seed: u64) -> (&'static FrameAnim, u32) {
        let pick = |kind: u32| -> (&'static FrameAnim, f32) {
            let id = match kind {
                2 => 2059,
                3 => 2060,
                _ => 2058,
            };
            let a = frame_anim(id).unwrap_or(&table::FRAME_ANIMS[0]);
            (a, if kind == 1 { 0.13 } else { 0.12 })
        };
        if !random {
            let (a, fd) = pick(1);
            let n = a.frames.len() as u32;
            return (a, ((t / fd).max(0.0) as u32) % n + 1);
        }
        let cache = &mut self.cycles[index];
        if t < cache.0 {
            *cache = (0.0, 0, 1);
        }
        if cache.2 == 0 {
            cache.2 = 1;
        }
        loop {
            let (a, fd) = pick(cache.2);
            let len = a.frames.len() as f32 * fd;
            if t < cache.0 + len || cache.1 > 1_000_000 {
                break;
            }
            cache.0 += len;
            cache.1 += 1;
            // 70%: (70%: the top, else bubble02); 30%: bubble.
            let (r1, r2) = (rand01(seed, 200 + cache.1 as u64), rand01(seed, 300 + cache.1 as u64));
            cache.2 = if r1 <= 0.7 { if r2 <= 0.7 { 1 } else { 3 } } else { 2 };
        }
        let (a, fd) = pick(cache.2);
        let n = a.frames.len() as u32;
        (a, (((t - cache.0) / fd).max(0.0) as u32).min(n - 1) + 1)
    }

    fn part_sprites(&mut self, index: usize, p: &PartObject, o: ObjAnim, t: f32, visual: &ObjectVisual, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> Option<Vec<SpriteDef>> {
        let rig = self.rigs.get(p.kind)?;
        // The default animation, entered at a random point (`offsetCurrentAnimation(rand)`).
        let def = part_def(p.kind, p.default)?;
        let length = def.delay * def.frames as f32;
        let phase = rand01(o.seed, 5) * length;
        let (anim, k) = if def.looped {
            (p.default, ((t + phase) / def.delay).max(0.0) as u32 % def.frames.max(1))
        } else {
            // Non-looping idles: when one ends, `animationFinished` plays idle01 or idle02.
            let cache = &mut self.cycles[index];
            let e = t + phase;
            if e < cache.0 {
                *cache = (0.0, 0, 0);
            }
            let name = |c: u32| if c == 0 { "idle01" } else { "idle02" };
            loop {
                let d = part_def(p.kind, name(cache.2)).unwrap_or(def);
                let len = d.delay * d.frames as f32;
                if e < cache.0 + len || cache.1 > 1_000_000 {
                    break;
                }
                cache.0 += len;
                cache.1 += 1;
                cache.2 = u32::from(rand01(o.seed, 400 + cache.1 as u64) > p.idle01);
            }
            let d = part_def(p.kind, name(cache.2)).unwrap_or(def);
            (d.anim, (((e - cache.0) / d.delay).max(0.0) as u32).min(d.frames.max(1) - 1))
        };
        let frames = rig.anims.get(anim)?;
        let parts = frames.get(k as usize % frames.len().max(1))?;
        let (w, h) = p.size;
        let mut out = vec![SpriteDef { frame: "", w, h, ..visual.sprites[0] }];
        let base = |frame: &'static str, x: f32, y: f32, z: i16, role: SpriteRole| SpriteDef {
            parent: 0,
            role,
            frame,
            x,
            y,
            anchor_x: 0.5,
            anchor_y: 0.5,
            w: 0.0,
            h: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            z,
            visible: true,
            flip_x: false,
            flip_y: false,
            additive: false,
        };
        // Parts sit on the CCAnimatedSprite at the object's center. The app sorts all sprites
        // by z, so z is laid out globally: glows (the follower node, z −1) under the parts, a
        // part's custom child just over its part.
        for (n, part) in parts.iter().enumerate() {
            for &(tag, glow) in p.glows {
                if tag == part.tag {
                    let mut g = base(glow, part.pos.0 + w * 0.5, part.pos.1 + h * 0.5, -1000 + n as i16, SpriteRole::Child);
                    (g.w, g.h) = frame_size(glow).unwrap_or((0.0, 0.0));
                    g.rotation = part.rotation;
                    out.push(g);
                }
            }
        }
        for part in parts {
            let role = if p.color_part == Some(part.tag) { SpriteRole::Detail } else { SpriteRole::Child };
            let (fw, fh) = frame_size(part.frame).unwrap_or((0.0, 0.0));
            let mut s = base(part.frame, part.pos.0 + w * 0.5, part.pos.1 + h * 0.5, (part.z * 10) as i16, role);
            (s.w, s.h, s.scale_x, s.scale_y, s.rotation) = (fw, fh, part.scale.0, part.scale.1, part.rotation);
            out.push(s);
            if p.id == 918 && part.tag == 1 {
                let parent = (out.len() - 1) as i8;
                let mut c = base(BEAST01_COLOR, fw * 0.5, fh * 0.5, (part.z * 10 + 1) as i16, SpriteRole::Detail);
                c.parent = parent;
                (c.w, c.h) = frame_size(BEAST01_COLOR).unwrap_or((0.0, 0.0));
                out.push(c);
            }
        }
        Some(out)
    }
}

/// Black cogwheels (1734–1736): the color sprite's child copies (real scene dumps, level 20).
fn cogwheel_copies(id: i32) -> Option<usize> {
    match id {
        1734 => Some(4),
        1735 | 1736 => Some(1),
        _ => None,
    }
}

/// The black cogwheels as they are in play (the sprite dump caught them before in-level setup):
/// the main sprite and the color sprite (`_color_001`, detail channel, z −100) only hold
/// children, which spin and draw: the dumped copies of the main frame and `copies` copies of the
/// color frame (z −1, then 0), the four of 1734 being quarters. The parents' own quads aren't
/// drawn.
fn cogwheel(visual: &ObjectVisual, copies: usize, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> Vec<SpriteDef> {
    let mut out = visual.sprites.to_vec();
    hide_root(&mut out);
    let color = intern(&visual.sprites[0].frame.replace("_001.png", "_color_001.png"));
    let parent = add_child(&mut out, 0, color, -100, 1.0, frame_size);
    out[parent].role = SpriteRole::Detail;
    out[parent].visible = false;
    // The copies are quarters, mirrored like the main frame's copies (flips (0,0) (1,0) (1,1) (0,1)).
    let flips: Vec<(bool, bool)> = visual.sprites.iter().filter(|s| s.parent == 0).map(|s| (s.flip_x, s.flip_y)).collect();
    for n in 0..copies {
        let c = add_child(&mut out, parent, color, if n == 0 { -1 } else { 0 }, 1.0, frame_size);
        out[c].role = SpriteRole::Detail;
        (out[c].flip_x, out[c].flip_y) = flips.get(n).copied().unwrap_or_default();
    }
    out
}

fn hide_root(out: &mut [SpriteDef]) {
    if let Some(r) = out.first_mut() {
        r.visible = false;
    }
}

/// The root's first non-color child (the dump's tag-1 child).
fn first_child(out: &[SpriteDef]) -> Option<usize> {
    out.iter().position(|s| s.parent == 0 && s.role != SpriteRole::Detail && !s.frame.contains("_color_"))
}

fn set_frame(s: &mut SpriteDef, frame: Option<&'static str>, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) {
    if let Some(f) = frame {
        s.frame = f;
        if let Some((w, h)) = frame_size(f) {
            (s.w, s.h) = (w, h);
        }
    }
}

/// Adds a child at the center of `parent` and returns its index.
fn add_child(out: &mut Vec<SpriteDef>, parent: usize, frame: &'static str, z: i16, scale: f32, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> usize {
    let (pw, ph) = (out[parent].w, out[parent].h);
    let (w, h) = frame_size(frame).unwrap_or((0.0, 0.0));
    out.push(SpriteDef {
        parent: parent as i8,
        role: SpriteRole::Child,
        frame,
        x: pw * 0.5,
        y: ph * 0.5,
        anchor_x: 0.5,
        anchor_y: 0.5,
        w,
        h,
        scale_x: scale,
        scale_y: scale,
        rotation: 0.0,
        z,
        visible: true,
        flip_x: false,
        flip_y: false,
        additive: false,
    });
    out.len() - 1
}

/// The color sprite as these objects build it: a child of the main sprite (z −100) on its first
/// color frame with a child (z −1, `scale`) that shows the animation. Returns (parent, child).
fn add_color_child(out: &mut Vec<SpriteDef>, first: &'static str, scale: f32, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> (usize, usize) {
    let parent = add_child(out, 0, first, -100, 1.0, frame_size);
    out[parent].role = SpriteRole::Detail;
    let child = add_child(out, parent, first, -1, scale, frame_size);
    out[child].role = SpriteRole::Detail;
    (parent, child)
}

/// An object's visual with its sprite tree swapped for this frame's (the draw loop reads the
/// same fields as from `ObjectVisual`).
pub struct View<'a> {
    pub id: u16,
    pub base_color: i16,
    pub detail_color: i16,
    pub z_layer: i8,
    pub z_order: i16,
    pub glow: bool,
    pub sprites: &'a [SpriteDef],
    /// The beasts' glow sprites' color (see [`glow_color`]); None for unanimated objects.
    glow_tint: Option<[f32; 3]>,
}

/// The color `PlayLayer::updateVisibility` 0x7ae88 gives animated objects' glow sprites
/// (`AnimatedGameObject::updateChildSpriteColor` 0x157818): the background color (channel 1000)
/// through `GameToolbox::transformColor` with s −0.3, v +0.4 (absolute). Matches the real
/// (238, 76, 255) on Fingerdash's purple background.
pub fn glow_color(background: [u8; 3]) -> [f32; 3] {
    let hsv = cadence_core::triggers::Hsv { h: 0.0, s: -0.3, v: 0.4, abs_s: true, abs_v: true };
    cadence_core::triggers::transform_color(background, hsv).map(|v| v as f32 / 255.0)
}

impl<'a> View<'a> {
    /// `background`: channel 1000's color, for the glow sprites.
    pub fn of(v: &'a ObjectVisual, sprites: Option<&'a [SpriteDef]>, background: [u8; 3]) -> View<'a> {
        let s = sprites.unwrap_or(v.sprites);
        // A hidden object (between plays, waiting for a trigger) has no glow either.
        let shown = s.iter().any(|d| d.visible && !d.frame.is_empty());
        let glow_tint = sprites.map(|_| glow_color(background));
        View { id: v.id, base_color: v.base_color, detail_color: v.detail_color, z_layer: v.z_layer, z_order: v.z_order, glow: v.glow && shown, sprites: s, glow_tint }
    }

    /// The object with another sprite list (no beast glow tint): the color children some
    /// objects only get inside real levels (`with_color_child`).
    pub fn with_sprites(v: &'a ObjectVisual, sprites: &'a [SpriteDef]) -> View<'a> {
        View { id: v.id, base_color: v.base_color, detail_color: v.detail_color, z_layer: v.z_layer, z_order: v.z_order, glow: v.glow, sprites, glow_tint: None }
    }

    /// The color a sprite takes instead of its main/detail color: the glows that follow beast parts.
    pub fn tint(&self, s: &SpriteDef) -> Option<[f32; 3]> {
        self.glow_tint.filter(|_| s.frame.contains("_glow_"))
    }
}

thread_local! {
    static COLOR_CHILDREN: std::cell::RefCell<crate::fxhash::FxMap<u16, Option<&'static [SpriteDef]>>> = Default::default();
}

/// Objects the game gives a color sprite only inside levels (`GameObject::addColorSprite`; the
/// synthetic level's dump, and so visuals.rs, missed it), as the real scene dumps show it
/// (`color_children.rs`, mined by tools/gen_color_children.py): a z 100 child in the detail
/// color, either the frame's `_color` version or, for objects of several parts, a copy of the
/// main frame holding the parts' `_color` versions (z −1, at the parts' positions). Keyed by the
/// object's frame and its children's frames, which both sides see.
pub fn with_color_child(v: &ObjectVisual, frame_size: &dyn Fn(&str) -> Option<(f32, f32)>) -> Option<&'static [SpriteDef]> {
    COLOR_CHILDREN.with(|c| {
        *c.borrow_mut().entry(v.id).or_insert_with(|| {
            let main = v.sprites.first()?;
            if v.sprites.iter().any(|s| s.frame.contains("_color_") || s.role == SpriteRole::Detail) {
                return None;
            }
            let kids: Vec<usize> = (1..v.sprites.len())
                .filter(|&i| v.sprites[i].parent == 0 && v.sprites[i].role != SpriteRole::Glow && !v.sprites[i].frame.contains("_glow_"))
                .collect();
            let mut names: Vec<&str> = kids.iter().map(|&i| v.sprites[i].frame).collect();
            names.sort_unstable();
            let sig = format!("{}|{}", main.frame, names.join(","));
            let table = crate::color_children::COLOR_CHILDREN;
            let k = table.binary_search_by(|e| e.0.cmp(sig.as_str())).ok()?;
            let (_, color, x, y, z) = table[k];
            let (w, h) = frame_size(color)?;
            let sprite = |parent: usize, frame: &'static str, x: f32, y: f32, w: f32, h: f32, z: i16, role: SpriteRole| SpriteDef {
                parent: parent as i8,
                role,
                frame,
                x,
                y,
                anchor_x: 0.5,
                anchor_y: 0.5,
                w,
                h,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                z,
                visible: true,
                flip_x: false,
                flip_y: false,
                additive: false,
            };
            let mut list = v.sprites.to_vec();
            if color != main.frame {
                list.push(sprite(0, color, x, y, w, h, z, SpriteRole::Child));
            } else {
                // a detail-colored copy of the main frame, holding the parts' color versions
                let copy = list.len();
                list.push(sprite(0, color, x, y, w, h, 100, SpriteRole::Detail));
                for &i in &kids {
                    let part = v.sprites[i];
                    let Some(stem) = part.frame.strip_suffix("_001.png") else { continue };
                    let name: &'static str = Box::leak(format!("{stem}_color_001.png").into_boxed_str());
                    let Some((pw, ph)) = frame_size(name) else { continue };
                    let mut s = sprite(copy, name, part.x, part.y, pw, ph, -1, SpriteRole::Child);
                    (s.rotation, s.scale_x, s.scale_y, s.flip_x, s.flip_y) = (part.rotation, part.scale_x, part.scale_y, part.flip_x, part.flip_y);
                    list.push(s);
                }
            }
            Some(&*Box::leak(list.into_boxed_slice()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_complete() {
        assert!(table::FRAME_ANIMS.windows(2).all(|w| w[0].id < w[1].id));
        let fire = frame_anim(923).unwrap();
        assert_eq!((fire.frames.len(), fire.delay, fire.kind), (9, 0.08, Kind::ColorChild));
        assert_eq!(fire.colors[0], "Fire_01_2_looped_001.png");
        assert_eq!(frame_anim(1583).unwrap().frames[3], "fireball_02_002.png");
        assert!(frame_anim(1050).is_some() && frame_anim(2032).is_some());
        assert!(part_def("BlackSludge", "loop").is_some_and(|d| d.frames == 11 && d.looped));
    }

    #[test]
    fn rand_is_uniform_enough() {
        let mean: f32 = (0..1000).map(|n| rand01(7, n)).sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05);
        assert!((0..1000).all(|n| (0.0..1.0).contains(&rand01(3, n))));
    }
}

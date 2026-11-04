//! Area triggers (2.2 `EnterEffectObject`, ids 3006–3010), the move kind (3006): every physics
//! step, after the move and follow actions (`GJBaseGameLayer` 0x1000fb3a4 → `processAreaEffects`
//! 0xf8d7c → `processAreaMoveGroupAction` 0xfa334), each object of the target group near the
//! center group's main object is offset by `moveDistance` along `moveAngle`, scaled by one minus
//! the eased distance value (`getAreaObjectValue` 0xfb7fc, `getEasedAreaValue` 0xfba2c). The
//! offset is taken off again at the next step's reset (`resetAreaObjectValues` 0xf992c), so it
//! follows the center. Ported for what the Tower levels use: no variances, no "relative" or
//! XY mode, no ease-out, no edit-area transitions (3011–3015), centers that are groups.

use crate::level::RawObject;
use crate::world::World;

/// The trigger's values (`EnterEffectObject::customObjectSetup` 0x13c89c; an absent key reads
/// as 0, as `atoi`/`atof` of an empty string).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaSpec {
    /// The trigger's level order (an effect from the same trigger is reloaded, not added).
    pub order: u32,
    /// 51 target group, 71 center group, 225 effect id (a new effect replaces an older one with
    /// the same id).
    pub target: i32,
    pub center: i32,
    pub effect_id: i32,
    /// 222 length, 220 offset, 252 offset y (ints), 263/264 front/back modifiers, 282 deadzone.
    pub length: f32,
    pub offset: f32,
    pub offset_y: f32,
    pub mod_front: f32,
    pub mod_back: f32,
    pub deadzone: f32,
    /// 218 move distance, 231 move angle (ints).
    pub move_distance: f32,
    pub move_angle: f32,
    /// 262 direction: 0 distance, 1 x, 2 y.
    pub dir_type: i32,
    /// 242 / 243 easing in (type, rate rounded to 0.01).
    pub ease_in: (i32, f32),
    /// 276 inbound (the value inverted), 283 both directions.
    pub inbound: bool,
    pub two_dir: bool,
    /// Features not ported (variances 223/221/253/219/232, 241 XY mode, 287 relative, 261 ease
    /// out): set when the trigger uses any of them.
    pub unsupported: bool,
    /// 3006 move, 3007 rotate (270 degrees), 3008 scale (233/234), 3009 fade (286 opacity),
    /// 3010 tint (260 channel, 265 amount, 65 main only, 66 detail only; 278 HSV mode: the
    /// HSV 49 scaled by the area's weight instead of a channel).
    pub kind: i32,
    pub tint_channel: i32,
    pub tint: f32,
    pub tint_main_only: bool,
    pub tint_detail_only: bool,
    pub tint_hsv_mode: bool,
    pub tint_hsv: crate::triggers::Hsv,
    pub rotation: f32,
    pub scale: (f32, f32),
    pub opacity: f32,
}

impl AreaSpec {
    pub fn from_raw(raw: &RawObject, order: u32) -> AreaSpec {
        let i = |k: u32| raw.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) as f32;
        let f = |k: u32| raw.get_f32(k).filter(|v| !v.is_nan()).unwrap_or(0.0).clamp(-1.0e9, 1.0e9);
        let rate = ((f(243) * 100.0) as i32) as f32 / 100.0;
        let on = |k: u32| raw.get(k) == Some("1");
        let unsupported = [223, 221, 253, 219, 232].iter().any(|&k| i(k) != 0.0) || on(241) || on(287) || on(261);
        AreaSpec {
            order,
            target: raw.get_i32(51).unwrap_or(0),
            center: raw.get_i32(71).unwrap_or(0),
            effect_id: raw.get_i32(225).unwrap_or(0),
            length: i(222),
            offset: i(220),
            offset_y: i(252),
            mod_front: f(263),
            mod_back: f(264),
            deadzone: f(282),
            move_distance: i(218),
            move_angle: i(231),
            dir_type: raw.get_i32(262).unwrap_or(0),
            ease_in: (raw.get_i32(242).unwrap_or(0), rate),
            inbound: on(276),
            two_dir: on(283),
            unsupported,
            kind: raw.id(),
            tint_channel: raw.get_i32(260).unwrap_or(0),
            tint: f(265),
            tint_main_only: on(65),
            tint_detail_only: on(66),
            tint_hsv_mode: on(278),
            tint_hsv: raw.get(49).map(crate::triggers::Hsv::parse).unwrap_or(crate::triggers::Hsv::IDENTITY),
            rotation: f(270),
            scale: (raw.get_f32(233).unwrap_or(1.0), raw.get_f32(234).unwrap_or(1.0)),
            opacity: f(286),
        }
    }
}

/// Active area moves and the per-object offsets they applied.
#[derive(Clone, Debug, Default)]
pub struct AreaState {
    pub effects: Vec<AreaSpec>,
    /// Per level object: the area offset applied (f32 x, y; obj +0x29c/+0x2a0) and the step it
    /// was last reset in (+0x4d8 against the layer's counter +0x3b0).
    off: Vec<(f32, f32)>,
    reset_step: Vec<u64>,
    /// Objects an area move has offset at least once (their collision section follows y).
    pub(crate) ever: Vec<bool>,
    /// Easing tables (`GJBaseGameLayer` +0x2f68): 101 eased values per (type, rate).
    tables: Vec<((i32, u32), Vec<f32>)>,
    /// Per level object, this step's visual area effects: opacity, extra degrees, scale
    /// (`GameObject::setAreaOpacity` and the transform actions; drawing only).
    pub opacity: Vec<f32>,
    pub turn: Vec<f32>,
    pub scale: Vec<(f32, f32)>,
    /// Area tints (`processAreaTintGroupAction`): (channel, share of its color, tint main,
    /// tint detail); the newest tint of the step wins. Channel 0 is none.
    pub tint: Vec<(i32, f32, bool, bool)>,
    /// Area tints in HSV mode (278, `processAreaTintGroupAction` 0xfaafc): the trigger's HSV
    /// times the weight (`getMultipliedHSV(1 − v)`) applied to the object's color, (main,
    /// detail) as for `tint`.
    pub tint_hsv: Vec<Option<(crate::triggers::Hsv, bool, bool)>>,
    /// Objects with a visual effect last step (cleared before the next).
    touched: Vec<usize>,
}

impl AreaState {
    pub fn new(n: usize) -> AreaState {
        AreaState {
            off: vec![(0.0, 0.0); n],
            reset_step: vec![0; n],
            ever: vec![false; n],
            opacity: vec![1.0; n],
            turn: vec![0.0; n],
            scale: vec![(1.0, 1.0); n],
            tint: vec![(0, 0.0, false, false); n],
            tint_hsv: vec![None; n],
            ..Default::default()
        }
    }

    /// `addAreaEffect` (0xf84a0): the same trigger again only reloads its values; an effect id
    /// replaces the older effect with that id.
    pub fn add(&mut self, spec: AreaSpec) {
        if self.effects.iter().any(|e| e.order == spec.order && e.target == spec.target && e.center == spec.center) {
            return;
        }
        if spec.effect_id > 0 {
            if let Some(i) = self.effects.iter().position(|e| e.effect_id == spec.effect_id) {
                self.effects.remove(i);
            }
        }
        if spec.unsupported && std::env::var_os("CADENCE_DEBUG_AREA").is_some() {
            eprintln!("area trigger {} uses features not ported", spec.order);
        }
        self.effects.push(spec);
    }

    /// The eased value through the easing table (`getEasedAreaValue` / `getEnterEasingValue`):
    /// type 0 linear; types 3, 7, 8, 9 computed directly; else linear between the table's
    /// values at the hundredths (eased(i / 100)).
    fn eased(&mut self, t: f32, (kind, rate): (i32, f32)) -> f32 {
        if kind == 0 {
            return t;
        }
        if matches!(kind, 3 | 7 | 8 | 9) {
            return crate::easing::eased(t, rate, kind);
        }
        let key = (kind, rate.to_bits());
        let table = match self.tables.iter().position(|e| e.0 == key) {
            Some(i) => &self.tables[i].1,
            None => {
                let v = (0..=100).map(|i| crate::easing::eased(i as f32 / 100.0, rate, kind)).collect();
                self.tables.push((key, v));
                &self.tables.last().unwrap().1
            }
        };
        let s2 = t * 100.0;
        let idx = s2 as i32 as usize;
        let (a, b) = (table[idx.min(100)], table[(idx + 1).min(100)]);
        let frac = ((s2 as i32) as f32).mul_add(-0.01, t);
        (frac * (b - a)).mul_add(100.0, a)
    }
}

/// `getEasedAreaValue` / `getEnterEasingValue` without the cache: type 0 linear; types 3, 7, 8, 9
/// computed directly; else linear between the game's table values at the hundredths
/// (`m_enterEasingValues` +0x2f68: eased(i / 100)).
pub fn table_eased(t: f32, kind: i32, rate: f32) -> f32 {
    if kind == 0 {
        return t;
    }
    if matches!(kind, 3 | 7 | 8 | 9) {
        return crate::easing::eased(t, rate, kind);
    }
    let s2 = t * 100.0;
    let idx = (s2 as i32).clamp(0, 100);
    let at = |i: i32| crate::easing::eased(i.min(100) as f32 / 100.0, rate, kind);
    let (a, b) = (at(idx), at(idx + 1));
    let frac = ((s2 as i32) as f32).mul_add(-0.01, t);
    (frac * (b - a)).mul_add(100.0, a)
}

/// Section index along one axis (`reorderObjectSection`: pos · 0.01, clamped, 0 below 0).
fn section(v: f32) -> i32 {
    if 0.0 < v {
        if 1.0e7 <= v { (0.01f32 * 1.0e7) as i32 } else { (v * 0.01) as i32 }
    } else {
        0
    }
}

impl World {
    /// The y section the solid pass sorts level object `order` by (see physics.rs): its
    /// current one once an area move has offset it, else 0.
    pub fn area_section_y(&self, order: u32, y: f32) -> i32 {
        if self.area.ever.get(order as usize).copied().unwrap_or(false) { crate::world::section_of(y) } else { 0 }
    }

    /// `resetAreaObjectValues(obj, true)`: once per step, stamp the last position and take the
    /// area offset back off the f64 position.
    fn area_reset(&mut self, i: usize) {
        let step = self.step_count_pub();
        if self.area.reset_step[i] >= step {
            return;
        }
        self.stamp_pub(i);
        self.area.reset_step[i] = step;
        let (ox, oy) = self.area.off[i];
        if ox != 0.0 || oy != 0.0 {
            let (mut x, mut y) = self.level_pos_of(i);
            if ox != 0.0 {
                x -= ox as f64;
            }
            if oy != 0.0 {
                y -= oy as f64;
            }
            self.area.off[i] = (0.0, 0.0);
            self.set_level_pos_pub(i, (x, y));
        }
    }

    /// The area moves of this step (after moves and follows).
    pub(crate) fn process_area_moves(&mut self) {
        // the visual effects are recomputed each step
        for i in std::mem::take(&mut self.area.touched) {
            self.area.opacity[i] = 1.0;
            self.area.turn[i] = 0.0;
            self.area.scale[i] = (1.0, 1.0);
            self.area.tint[i].0 = 0;
            self.area.tint_hsv[i] = None;
        }
        if self.area.effects.is_empty() {
            return;
        }
        for e in 0..self.area.effects.len() {
            let mut a = self.area.effects[e];
            let Some(&center) = self.group_members(a.center).and_then(|m| m.first()) else { continue };
            let center = center as usize;
            self.area_reset(center);
            let (cx, cy) = self.level_pos_of(center);
            let (cx, cy) = (cx as f32 + a.offset, cy as f32 + a.offset_y);
            if a.two_dir {
                a.mod_front = -a.mod_front.abs();
                self.area.effects[e].mod_front = a.mod_front;
            }
            // The section window: radius (int)(length + offset) / min(|front|, |back|).
            let r = (1.0 / a.mod_front.abs().min(a.mod_back.abs())) * ((a.length + a.offset) as i32) as f32;
            let (x0, x1) = (section(cx - r), section(cx + r));
            let (y0, y1) = (section(cy - r), section(cy + r));
            let (xr, yr) = match a.dir_type {
                1 if !a.inbound => (if a.two_dir { (x0, x1) } else { (0, x1) }, (0, i32::MAX)),
                2 if !a.inbound => ((0, i32::MAX), if a.two_dir { (y0, y1) } else { (0, y1) }),
                _ if a.inbound => ((0, i32::MAX), (0, i32::MAX)),
                _ => ((x0, x1), (y0, y1)),
            };
            let angle = (a.move_angle + -90.0) * 0.017453292;
            let dir = (angle.cos(), angle.sin());
            let Some(targets) = self.group_members(a.target).cloned() else { continue };
            for i in targets {
                let i = i as usize;
                let (px, py) = self.level_pos_of(i);
                let (px, py) = (px as f32, py as f32);
                let (sx, sy) = (section(px), section(py));
                if !(xr.0 <= sx && sx <= xr.1 && yr.0 <= sy && sy <= yr.1) {
                    continue;
                }
                self.area_reset(i);
                let (px, py) = self.level_pos_of(i);
                let (px, py) = (px as f32, py as f32);
                let d = match a.dir_type {
                    1 | 2 => {
                        let d = if a.dir_type == 1 { px - cx } else { py - cy };
                        d * if d < 0.0 { a.mod_front } else { a.mod_back }
                    }
                    _ => ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt(),
                };
                let ratio = d / ((a.length as i32) as f32);
                let mut v = if a.deadzone == 0.0 {
                    if ratio >= 1.0 { 1.0 } else if ratio <= 0.0 { 0.0 } else { ratio }
                } else {
                    let t = (ratio - a.deadzone) / (1.0 - a.deadzone);
                    if t >= 1.0 { 1.0 } else if t <= 0.0 { 0.0 } else { t }
                };
                if a.inbound {
                    v = 1.0 - v;
                }
                if a.kind != 3006 {
                    // fade / rotate / scale: from the effect's value at the center (v = 0) to
                    // none far away (v = 1)
                    if v >= 1.0 {
                        continue;
                    }
                    let eased = self.area.eased(v, a.ease_in);
                    let k = 1.0 - eased;
                    match a.kind {
                        3009 => self.area.opacity[i] *= a.opacity + eased * (1.0 - a.opacity),
                        3007 => self.area.turn[i] += a.rotation * k,
                        3008 => {
                            let s = &mut self.area.scale[i];
                            s.0 *= 1.0 + (a.scale.0 - 1.0) * k;
                            s.1 *= 1.0 + (a.scale.1 - 1.0) * k;
                        }
                        // multipliedColorValue((1 − tint) + tint·v, channel, own): the channel's
                        // share is tint·(1 − v)
                        3010 if a.tint_hsv_mode => {
                            self.area.tint[i].0 = 0;
                            self.area.tint_hsv[i] = Some((a.tint_hsv.multiplied(k), !a.tint_detail_only, !a.tint_main_only));
                        }
                        3010 => {
                            self.area.tint[i] = (a.tint_channel, (a.tint * k).clamp(0.0, 1.0), !a.tint_detail_only, !a.tint_main_only);
                            self.area.tint_hsv[i] = None;
                        }
                        _ => continue,
                    }
                    self.area.touched.push(i);
                    continue;
                }
                if 1.0 <= v || a.move_distance == 0.0 {
                    continue;
                }
                let eased = self.area.eased(v, a.ease_in);
                let f = (1.0 - eased) * a.move_distance;
                let (ox, oy) = (dir.0 * f, f * dir.1);
                if ox == 0.0 && oy == 0.0 {
                    continue;
                }
                let (mut x, mut y) = self.level_pos_of(i);
                self.area.ever[i] = true;
                let off = &mut self.area.off[i];
                if oy != 0.0 {
                    y += oy as f64;
                    off.1 += oy;
                }
                if ox != 0.0 {
                    off.0 += ox;
                    x += ox as f64;
                }
                self.set_level_pos_pub(i, (x, y));
            }
        }
    }
}

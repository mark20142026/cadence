//! Enter effects: how objects come in at the right edge of the view and leave at the left one.
//!
//! - The triggers (`GJBaseGameLayer::updateActiveEnterEffect` 0xd918c): the old ones (22–28,
//!   55–59) and "no enter effect" (1915) set a code for enter channel 344 (0..100); the custom
//!   ones (3017 move, 3018 rotate, 3019 scale, 3020 fade, 3021 tint) set code −15 and add an
//!   effect to the channel's list (`addCustomEnterEffect` 0xd9544; the same effect id 225
//!   replaces an older one, the same trigger reloads it). Key 217 picks the side: 0 both, 1
//!   entering (right), 2 exiting (left). Enter stop (3023, `stopCustomEnterEffect` 0xd9c10)
//!   removes the channel's effects with its effect id (all without one); an emptied channel goes
//!   back to −2. Every attempt starts at −2 everywhere (`resetActiveEnterEffects`).
//! - Each frame (`PlayLayer::updateVisibility` 0x7ae88), per visible object: the side is the
//!   one its effect is locked on, else right of the view's middle is "entering"; its code is the
//!   side's table entry for the object's enter channel (key 343), −14 for objects pinned by UI
//!   triggers. Except for −14 and −15 the edge fade applies (opacity = int(clamp(distance from
//!   the edge to the object's far side / 70)·255), not for "don't fade" objects).
//! - `applyEnterEffect` (0x7c570) runs the old codes over t = clamp(distance from the edge to
//!   the object's center / 70): the code is locked on the object when it starts entering (so a
//!   trigger passed meanwhile doesn't change an object already coming in) and released at t = 0
//!   or t = 1. `applyCustomEnterEffect` (0x7bdd0) runs the channel's custom effects, each over
//!   its own length 222 / offset 220 / deadzone 282 and easing 242 / 243.

use crate::level::RawObject;
use crate::triggers::Hsv;

/// The enter effect triggers (`EffectGameObject::triggerObject` → `updateActiveEnterEffect`).
pub fn is_enter_trigger(id: i32) -> bool {
    matches!(id, 22..=28 | 55..=59 | 1915 | 3017..=3021)
}

/// The code an old enter effect trigger sets (`updateActiveEnterEffect`), −15 for the custom ones.
pub fn legacy_code(id: i32) -> i32 {
    match id {
        23 => -6,
        24 => -5,
        25 => -7,
        26 => -8,
        27 => -3,
        28 => -4,
        55 => -11,
        56 => -10,
        57 => -9,
        58 => -12,
        59 => -13,
        1915 => -14,
        3017..=3021 => -15,
        _ => -2,
    }
}

/// An enter effect trigger's values (`EnterEffectObject::customObjectSetup` 0x13c89c; the
/// variances 223 / 221 / 219 / 232 / 238 / 240 / 234 / 236 / 271 scale per-object random values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnterSpec {
    pub id: i32,
    pub code: i32,
    /// 344 enter channel (0..100), 217 side (0 both, 1 enter, 2 exit), 225 effect id.
    pub channel: i32,
    pub mode: i32,
    pub effect_id: i32,
    pub target: i32,
    pub center: i32,
    pub order: u32,
    pub length: f32,
    pub length_var: f32,
    pub offset: f32,
    pub offset_var: f32,
    pub deadzone: f32,
    pub distance: f32,
    pub distance_var: f32,
    pub angle: f32,
    pub angle_var: f32,
    /// 241: move by (237, 239) (variances 238, 240) instead of distance along angle.
    pub xy: bool,
    pub move_x: f32,
    pub move_x_var: f32,
    pub move_y: f32,
    pub move_y_var: f32,
    pub scale: (f32, f32),
    pub scale_var: (f32, f32),
    pub rotation: f32,
    pub rotation_var: f32,
    /// 3021: 260 channel, 265 amount, 278 HSV mode (49 the HSV), 65 main only, 66 detail only.
    pub tint_channel: i32,
    pub tint: f32,
    pub hsv_mode: bool,
    pub hsv: Hsv,
    pub main_only: bool,
    pub detail_only: bool,
    /// 242 / 243 easing (type, rate rounded to 0.01).
    pub ease: (i32, f32),
}

impl EnterSpec {
    pub fn from_raw(raw: &RawObject) -> EnterSpec {
        let i = |k: u32| raw.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) as f32;
        let f = |k: u32| raw.get_f32(k).filter(|v| !v.is_nan()).unwrap_or(0.0).clamp(-1.0e9, 1.0e9);
        let on = |k: u32| raw.get(k) == Some("1");
        let id = raw.id();
        EnterSpec {
            id,
            code: legacy_code(id),
            channel: raw.get_i32(344).unwrap_or(0).clamp(0, 100),
            mode: raw.get_i32(217).unwrap_or(0),
            effect_id: raw.get_i32(225).unwrap_or(0),
            target: raw.get_i32(51).unwrap_or(0),
            center: raw.get_i32(71).unwrap_or(0),
            order: 0,
            length: i(222),
            length_var: i(223),
            offset: i(220),
            offset_var: i(221),
            deadzone: f(282),
            distance: i(218),
            distance_var: i(219),
            angle: i(231),
            angle_var: i(232),
            xy: on(241),
            move_x: i(237),
            move_x_var: i(238),
            move_y: i(239),
            move_y_var: i(240),
            scale: (raw.get_f32(233).unwrap_or(1.0), raw.get_f32(235).unwrap_or(1.0)),
            scale_var: (f(234), f(236)),
            rotation: f(270),
            rotation_var: f(271),
            tint_channel: raw.get_i32(260).unwrap_or(0),
            tint: f(265),
            hsv_mode: on(278),
            hsv: raw.get(49).map(Hsv::parse).unwrap_or(Hsv::IDENTITY),
            main_only: on(65),
            detail_only: on(66),
            ease: (raw.get_i32(242).unwrap_or(0), ((f(243) * 100.0) as i32) as f32 / 100.0),
        }
    }
}

/// The enter / exit code of each channel and the custom effects (GJBaseGameLayer +0x598 /
/// +0x5b0, +0x548 / +0x570). Index 0 of the pairs is the exit (left) side, 1 the enter side.
#[derive(Clone, Debug, PartialEq)]
pub struct EnterState {
    pub codes: [Vec<i32>; 2],
    pub custom: [Vec<Vec<EnterSpec>>; 2],
}

impl Default for EnterState {
    fn default() -> EnterState {
        EnterState { codes: [vec![-2; 101], vec![-2; 101]], custom: [vec![Vec::new(); 101], vec![Vec::new(); 101]] }
    }
}

impl EnterState {
    /// Nothing changed from the start (every channel −2): only the edge fade.
    pub fn is_default(&self) -> bool {
        self.codes.iter().all(|c| c.iter().all(|&v| v == -2))
    }

    /// `updateActiveEnterEffect`: the trigger's code for its sides; custom effects join the
    /// channel's list, old ones empty it.
    pub fn fire(&mut self, spec: &EnterSpec) {
        let ch = spec.channel.clamp(0, 100) as usize;
        let sides: &[usize] = match spec.mode {
            0 => &[1, 0],
            1 => &[1],
            2 => &[0],
            _ => &[],
        };
        for &side in sides {
            self.codes[side][ch] = spec.code;
            let list = &mut self.custom[side][ch];
            if spec.code != -15 {
                list.clear();
                continue;
            }
            if spec.effect_id > 0 {
                if let Some(i) = list.iter().position(|e| e.effect_id == spec.effect_id) {
                    if list[i].order == spec.order && list[i].target == spec.target && list[i].center == spec.center {
                        list[i] = *spec;
                        continue;
                    }
                    list.remove(i);
                }
            }
            list.push(*spec);
        }
    }

    /// Enter stop (3023): the channel's effects with the effect id go (all of them for id 0); a
    /// channel left without any is back to −2.
    pub fn stop(&mut self, channel: i32, mode: i32, effect_id: i32) {
        let ch = channel.clamp(0, 100) as usize;
        let sides: &[usize] = match mode {
            0 => &[1, 0],
            1 => &[1],
            2 => &[0],
            _ => &[],
        };
        for &side in sides {
            let list = &mut self.custom[side][ch];
            if list.is_empty() {
                continue;
            }
            if effect_id == 0 {
                list.clear();
            } else if let Some(i) = list.iter().position(|e| e.effect_id == effect_id) {
                list.remove(i);
            }
            if list.is_empty() {
                self.codes[side][ch] = -2;
            }
        }
    }
}

/// What the renderer knows about an object for its enter effect.
#[derive(Clone, Copy, Debug)]
pub struct EnterObject {
    /// The object's position (its real position, before the effect).
    pub x: f32,
    pub y: f32,
    /// `m_fadeMargin`: half its width (the fade is measured from its far side).
    pub margin: f32,
    pub channel: i32,
    /// Keys 67 "don't enter" / 64 "don't fade"; pinned by a UI trigger; an audio-scaled object
    /// (the scale effects leave it alone).
    pub ignore_enter: bool,
    pub ignore_fade: bool,
    pub ui: bool,
    pub no_scale: bool,
    /// A per-object random number source for the variances (`m_varianceValues`), in [−1, 1].
    pub seed: u32,
}

/// The view the effects measure against (camera x, its width and its vertical middle).
#[derive(Clone, Copy, Debug)]
pub struct EnterView {
    pub left: f32,
    pub width: f32,
    pub cam_y: f32,
    /// The window's height in points (`getWinSize`, not divided by the zoom).
    pub win_h: f32,
}

/// The per-object state the game keeps on the object (`m_exitType` +0x3f2 / `m_enterType`
/// +0x3f0: the code locked on each side, −1 none; `+0x3e8` the angle of the angled codes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnterLock {
    pub code: [i16; 2],
    pub angle: f32,
}

impl Default for EnterLock {
    fn default() -> EnterLock {
        EnterLock { code: [-1, -1], angle: 0.0 }
    }
}

/// The effect on one object this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnterResult {
    pub dx: f32,
    pub dy: f32,
    pub scale: (f32, f32),
    /// Extra rotation (degrees; the custom rotate effect's).
    pub rotation: f32,
    /// 0..1 (an int / 255, as the game's opacity).
    pub opacity: f32,
    /// A custom tint: (channel, the channel's share, main, detail), or an HSV shift.
    pub tint: Option<(i32, f32, bool, bool)>,
    pub hsv: Option<(Hsv, bool, bool)>,
}

impl EnterResult {
    pub const NONE: EnterResult = EnterResult { dx: 0.0, dy: 0.0, scale: (1.0, 1.0), rotation: 0.0, opacity: 1.0, tint: None, hsv: None };
}

/// A stable value in [−1, 1] for the object's `k`-th variance.
fn variance(seed: u32, k: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// `getEnterEasingValue`: linear for type 0, computed for 3/7/8/9, else interpolated in the
/// game's 101-entry table (shared with the area effects).
pub fn eased(t: f32, (kind, rate): (i32, f32)) -> f32 {
    crate::area::table_eased(t, kind, rate)
}

impl EnterState {
    /// `PlayLayer::updateVisibility`'s enter and fade part for one object.
    pub fn apply(&self, obj: &EnterObject, view: &EnterView, lock: &mut EnterLock) -> EnterResult {
        let right = view.left + view.width;
        let side = if lock.code[1] != -1 {
            1
        } else if lock.code[0] == -1 && view.left + view.width * 0.5 < obj.x {
            1
        } else {
            0
        };
        let code = if obj.ui { -14 } else { self.codes[side][obj.channel.clamp(0, 100) as usize] };
        let mut out = EnterResult::NONE;
        // the edge fade
        if !(obj.ignore_fade || code == -14 || code == -15) {
            let d = if side == 0 { (obj.x + obj.margin) - view.left } else { right - (obj.x - obj.margin) };
            out.opacity = ((d / 70.0).clamp(0.0, 1.0) * 255.0) as i32 as f32 / 255.0;
        }
        if code == -15 {
            if !obj.ignore_enter {
                self.apply_custom(obj, view, side, lock, &mut out);
            }
            return out;
        }
        self.apply_legacy(obj, view, side, code, lock, &mut out);
        out
    }

    /// `applyEnterEffect` for the old codes.
    fn apply_legacy(&self, obj: &EnterObject, view: &EnterView, side: usize, code: i32, lock: &mut EnterLock, out: &mut EnterResult) {
        let t = if side == 0 { obj.x - view.left } else { (view.left + view.width) - obj.x };
        let t = (t / 70.0).clamp(0.0, 1.0);
        let ignore = obj.ignore_enter || t == 1.0;
        let locked = lock.code[side];
        if locked == -1 || (code == -14 && locked != -14) {
            if ignore {
                return;
            }
            lock.code[side] = code as i16;
            let lower = obj.y <= view.cam_y + view.win_h * 0.5;
            match code {
                -13 | -12 => lock.angle = if lower { 0.0 } else { 180.0 } + if code == -13 { 180.0 } else { 0.0 },
                -10 => lock.angle = if lower { -45.0 } else { -135.0 },
                -9 => lock.angle = if lower { 45.0 } else { 135.0 },
                // rand() in the game
                -11 => lock.angle = variance(obj.seed, 99) * 180.0,
                _ => {}
            }
        } else if ignore {
            lock.code[side] = -1;
            return;
        }
        let away = (1.0 - t) * 100.0;
        match lock.code[side] {
            -13..=-9 => {
                let a = (lock.angle - 90.0) * 0.017453292;
                out.dx = a.cos() * away;
                out.dy = a.sin() * away;
            }
            -8 => out.dx = away,
            -7 => out.dx = -away,
            -6 => out.dy = -away,
            -5 => out.dy = away,
            -4 if !obj.no_scale => {
                let s = (1.0 - t) * 0.75 + 1.0;
                out.scale = (s, s);
            }
            -3 if !obj.no_scale => out.scale = (t, t),
            _ => {}
        }
        if t == 0.0 {
            lock.code[side] = -1;
        }
    }

    /// `applyCustomEnterEffect`: every effect of the object's channel on this side.
    fn apply_custom(&self, obj: &EnterObject, view: &EnterView, side: usize, lock: &mut EnterLock, out: &mut EnterResult) {
        let dir = if side == 0 { 1.0f32 } else { -1.0 };
        let edge = if side == 0 { view.left } else { view.left + view.width };
        let list = &self.custom[side][obj.channel.clamp(0, 100) as usize];
        let v = |k: u32| variance(obj.seed, k);
        let mut none = true;
        let (mut opacity, mut rotation, mut rotated) = (1.0f32, 1.0f32, false);
        let (mut sx, mut sy, mut scaled) = (1.0f32, 1.0f32, false);
        for e in list {
            let off = if e.offset_var != 0.0 { e.offset_var * v(1) } else { 0.0 };
            let len = if e.length_var != 0.0 { e.length_var * v(0) } else { 0.0 };
            let mut t = (dir * ((obj.x - (edge + e.offset)) + off)) / ((e.length + len) as i32) as f32;
            if e.deadzone != 0.0 {
                t = (t - e.deadzone) / (1.0 - e.deadzone);
            }
            if !(t < 1.0) {
                continue;
            }
            let t = if t > 0.0 { t } else { 0.0 };
            none = false;
            match e.id {
                3017 if e.xy => {
                    let mx = dir * (e.move_x + v(8) * e.move_x_var);
                    let my = dir * (e.move_y + v(9) * e.move_y_var);
                    if mx != 0.0 || my != 0.0 {
                        let k = 1.0 - eased(t, e.ease);
                        out.dx += k * mx;
                        out.dy += k * my;
                    }
                }
                3017 => {
                    let d = e.distance + v(10) * e.distance_var;
                    if d != 0.0 {
                        let d = d * (1.0 - eased(t, e.ease));
                        let a = (e.angle + v(11) * e.angle_var + -90.0) * 0.017453292;
                        out.dx += a.cos() * d;
                        out.dy += a.sin() * d;
                    }
                }
                3018 => {
                    let r = e.rotation + v(18) * e.rotation_var;
                    if r != 0.0 {
                        // the game's accumulator starts at 1 (0x10007bf24 stores 1.0 in both
                        // the opacity and the rotation slot)
                        rotation += r * (1.0 - eased(t, e.ease));
                        rotated = true;
                    }
                }
                3019 => {
                    let a = e.scale.0 + v(12) * e.scale_var.0;
                    let b = e.scale.1 + v(13) * e.scale_var.1;
                    if a != 0.0 || b != 0.0 {
                        let k = 1.0 - eased(t, e.ease);
                        sx *= k * (a - 1.0) + 1.0;
                        sy *= k * (b - 1.0) + 1.0;
                        scaled = true;
                    }
                }
                3020 => {
                    if !obj.ignore_fade {
                        opacity *= eased(t, e.ease);
                    }
                }
                3021 => {
                    if e.hsv_mode {
                        let k = 1.0 - t;
                        let h = Hsv {
                            h: e.hsv.h * k,
                            s: if e.hsv.abs_s { e.hsv.s * k } else { 1.0 + (e.hsv.s - 1.0) * k },
                            v: if e.hsv.abs_v { e.hsv.v * k } else { 1.0 + (e.hsv.v - 1.0) * k },
                            ..e.hsv
                        };
                        out.hsv = Some((h, !e.detail_only, !e.main_only));
                    } else {
                        // multipliedColorValue(channel, own, (1 − tint) + tint·t): the channel's
                        // share is tint·(1 − t) (not eased)
                        out.tint = Some((e.tint_channel, (e.tint * (1.0 - t)).clamp(0.0, 1.0), !e.detail_only, !e.main_only));
                    }
                }
                _ => {}
            }
        }
        out.opacity = (opacity * 255.0) as i32 as f32 / 255.0;
        if rotated {
            out.rotation = rotation;
        }
        if scaled {
            out.scale = (sx, sy);
        }
        lock.code[side] = if none { -1 } else { -15 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(s: &str) -> RawObject {
        RawObject::parse(s)
    }

    fn obj(x: f32) -> EnterObject {
        EnterObject { x, y: 200.0, margin: 15.0, channel: 0, ignore_enter: false, ignore_fade: false, ui: false, no_scale: false, seed: 7 }
    }

    const VIEW: EnterView = EnterView { left: 0.0, width: 480.0, cam_y: 0.0, win_h: 320.0 };

    #[test]
    fn edge_fade_only_by_default() {
        let s = EnterState::default();
        let mut lock = EnterLock::default();
        // 35 units in from the right edge, far side 50 in: opacity int(50/70·255)/255
        let r = s.apply(&obj(445.0), &VIEW, &mut lock);
        assert_eq!(r.opacity, ((50.0f32 / 70.0) * 255.0) as i32 as f32 / 255.0);
        assert_eq!((r.dx, r.dy), (0.0, 0.0));
    }

    #[test]
    fn no_enter_effect_disables_the_fade() {
        let mut s = EnterState::default();
        s.fire(&EnterSpec::from_raw(&raw("1,1915,2,0,3,0")));
        let r = s.apply(&obj(470.0), &VIEW, &mut EnterLock::default());
        assert_eq!(r.opacity, 1.0);
    }

    #[test]
    fn legacy_code_locks_while_entering() {
        let mut s = EnterState::default();
        s.fire(&EnterSpec::from_raw(&raw("1,23,2,0,3,0")));
        let mut lock = EnterLock::default();
        // from below: 35 in → t = 0.5, 50 units down
        let r = s.apply(&obj(445.0), &VIEW, &mut lock);
        assert_eq!(r.dy, -50.0);
        assert_eq!(lock.code[1], -6);
        // another trigger meanwhile: the object keeps its effect until it's in
        s.fire(&EnterSpec::from_raw(&raw("1,24,2,0,3,0")));
        let r = s.apply(&obj(450.0), &VIEW, &mut lock);
        assert!(r.dy < 0.0);
        let r = s.apply(&obj(400.0), &VIEW, &mut lock);
        assert_eq!(r.dy, 0.0);
        assert_eq!(lock.code[1], -1);
    }

    #[test]
    fn custom_move_and_fade() {
        let mut s = EnterState::default();
        // enter move: distance 100 at angle 0 (from below: the direction is angle − 90°), length 100
        let mut m = EnterSpec::from_raw(&raw("1,3017,2,0,3,0,218,100,231,0,222,100"));
        m.order = 1;
        s.fire(&m);
        let mut f = EnterSpec::from_raw(&raw("1,3020,2,0,3,0,222,100"));
        f.order = 2;
        s.fire(&f);
        let mut lock = EnterLock::default();
        // 25 in from the right edge: t = 0.25
        let r = s.apply(&obj(455.0), &VIEW, &mut lock);
        assert!((r.dy + 75.0).abs() < 1e-3 && r.dx.abs() < 1e-3, "{r:?}");
        assert_eq!(r.opacity, (0.25f32 * 255.0) as i32 as f32 / 255.0);
        assert_eq!(lock.code[1], -15);
        // fully in: untouched, no edge fade
        let r = s.apply(&obj(300.0), &VIEW, &mut lock);
        assert_eq!(r, EnterResult::NONE);
        assert_eq!(lock.code[1], -1);
        // enter stop without an id clears the channel
        s.stop(0, 0, 0);
        assert!(s.is_default());
    }

    #[test]
    fn custom_rotate_starts_from_one_degree() {
        let mut s = EnterState::default();
        s.fire(&EnterSpec::from_raw(&raw("1,3018,2,0,3,0,270,90,222,100")));
        let r = s.apply(&obj(480.0), &VIEW, &mut EnterLock::default());
        assert_eq!(r.rotation, 91.0);
    }

    #[test]
    fn same_effect_id_replaces() {
        let mut s = EnterState::default();
        let mut a = EnterSpec::from_raw(&raw("1,3017,2,0,3,0,225,5,218,10"));
        a.order = 1;
        let mut b = EnterSpec::from_raw(&raw("1,3019,2,0,3,0,225,5,233,2"));
        b.order = 2;
        s.fire(&a);
        s.fire(&b);
        assert_eq!(s.custom[1][0].len(), 1);
        assert_eq!(s.custom[1][0][0].id, 3019);
        // exit-only effects leave the enter side alone
        let mut c = EnterSpec::from_raw(&raw("1,3020,2,0,3,0,217,2"));
        c.order = 3;
        s.fire(&c);
        assert_eq!(s.custom[0][0].len(), 2);
        assert_eq!(s.codes[1][0], -15);
    }
}

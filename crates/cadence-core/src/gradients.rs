//! Gradient triggers (2903, `GradientTriggerObject`): colored quads between groups.
//!
//! `GradientTriggerObject::customObjectSetup` (0x141f44) reads 174 blending (+0x734), 202
//! layer (+0x730), 203/204/205/206 the U/D/L/R groups (+0x73c..+0x748), 207 vertex mode
//! (+0x74c), 208 disable (+0x74d), 508 disable all (+0x74e), 209 gradient id (+0x738).
//! `GJBaseGameLayer::triggerGradientCommand` (0xf1794) gives each id a `GJGradientLayer` (a
//! `CCLayerGradient`) in its layer's parent; `updateGradientLayers` (0xf1e28) sets it up every
//! frame: the trigger's main/detail channels (keys 21/22, with their HSV) at their opacity times
//! the trigger's own opacity, the direction (cos, sin) of −rotation (key 6), and the quad — the
//! view (±20) with the U/D/L/R groups' main objects moving the top/bottom/left/right edges, or,
//! in vertex mode, the four corners (U bottom-left, D bottom-right, L top-left, R top-right).
//! `CCLayerGradient::updateColor` (0x412a38, compressed interpolation) gives the corner colors.

use std::collections::{BTreeMap, HashMap};

use crate::level::RawObject;
use crate::triggers::Hsv;

pub const GRADIENT: i32 = 2903;

#[derive(Clone, Debug, PartialEq)]
pub struct GradientSpec {
    pub order: u32,
    /// 0 normal (SRC_ALPHA, 1−SRC_ALPHA), 1 additive (SRC_ALPHA, ONE), 2 multiply (DST_COLOR,
    /// 1−SRC_ALPHA, premultiplied), 3 (1−DST_COLOR, 1−SRC_COLOR, premultiplied).
    pub blending: i32,
    /// The z layer (1 BG, 2 MG, 3–7 B5–B1, 8 P, 9–12 T1–T4, 13 G, 14 UI, 15 Max).
    pub layer: i32,
    pub u: i32,
    pub d: i32,
    pub l: i32,
    pub r: i32,
    pub vertex_mode: bool,
    pub disable: bool,
    pub disable_all: bool,
    pub id: i32,
    pub main: i32,
    pub detail: i32,
    pub main_hsv: Option<Hsv>,
    pub detail_hsv: Option<Hsv>,
    pub rotation: f32,
    /// `getObjectZOrder` clamped to −5..5 (added to the layer's z).
    pub z: i32,
    pub groups: Vec<i32>,
}

fn atoi(s: &str) -> i32 {
    let t = s.trim();
    let end = t.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))).map_or(t.len(), |(i, _)| i);
    t[..end].parse().unwrap_or(0)
}

impl GradientSpec {
    pub fn parse(o: &RawObject, order: u32) -> Option<GradientSpec> {
        if o.id() != GRADIENT {
            return None;
        }
        let i = |k| o.get(k).map_or(0, atoi);
        let b = |k| o.get(k).is_some_and(|v| atoi(v) != 0);
        let hsv = |on, k| (o.get(on) == Some("1")).then(|| o.get(k).map_or(Hsv::IDENTITY, Hsv::parse));
        Some(GradientSpec {
            order,
            blending: o.get_f32(174).map_or(0, |v| v.clamp(-1e9, 1e9) as i32),
            layer: i(202),
            u: i(203),
            d: i(204),
            l: i(205),
            r: i(206),
            vertex_mode: b(207),
            disable: b(208),
            disable_all: b(508),
            id: i(209).min(999),
            main: o.get_i32(21).unwrap_or(0),
            detail: o.get_i32(22).unwrap_or(0),
            main_hsv: hsv(41, 43),
            detail_hsv: hsv(42, 44),
            rotation: o.get_f32(6).unwrap_or(0.0),
            z: o.get_i32(25).unwrap_or(0).clamp(-5, 5),
            groups: o.groups(),
        })
    }

    /// Layers drawn in screen space (`triggerGradientCommand`: mask 0xe006 → +0x250 = 0).
    pub fn screen_space(&self) -> bool {
        matches!(self.layer, 1 | 2 | 13 | 14 | 15)
    }
}

/// One frame's gradient: corners bottom-left, bottom-right, top-left, top-right (world
/// units) and their colors (0..1 RGBA; premultiplied for blending 2 and 3, like the game's
/// +0x231 flag).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientQuad {
    pub layer: i32,
    pub z: i32,
    pub blending: i32,
    pub corners: [(f32, f32); 4],
    pub colors: [[f32; 4]; 4],
}

/// The level's gradient triggers and which trigger each gradient id shows.
#[derive(Clone, Debug, Default)]
pub struct Gradients {
    specs: std::sync::Arc<HashMap<u32, GradientSpec>>,
    /// Gradient id → the trigger it shows (`plVar14[0x49]`), in id order.
    pub active: BTreeMap<i32, u32>,
}

/// What the quads need from the game.
pub struct GradientContext<'a> {
    /// The view (world units): left, bottom, width, height; and the zoom.
    pub view: (f32, f32, f32, f32),
    pub zoom: f32,
    /// A group's main object position (world units).
    pub group_position: &'a dyn Fn(i32) -> Option<(f32, f32)>,
    /// A channel's active color with the object's HSV.
    pub color: &'a dyn Fn(i32, Option<Hsv>) -> [u8; 3],
    /// A channel's opacity (0..1) for an object in these groups.
    pub opacity: &'a dyn Fn(i32, &[i32]) -> f32,
}

impl Gradients {
    pub fn from_level(level: &crate::level::LevelString) -> Gradients {
        let specs = level
            .objects
            .iter()
            .enumerate()
            .filter_map(|(order, o)| Some((order as u32, GradientSpec::parse(o, order as u32)?)))
            .collect();
        Gradients { specs: std::sync::Arc::new(specs), active: BTreeMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// A gradient trigger fired (by level order).
    pub fn fire(&mut self, order: u32) {
        let Some(s) = self.specs.get(&order) else { return };
        if s.disable_all {
            self.active.clear();
        } else if s.disable {
            self.active.remove(&s.id);
        } else {
            self.active.insert(s.id, order);
        }
    }

    /// This frame's quads, in draw order within each layer (z, then id).
    pub fn quads(&self, ctx: &GradientContext) -> Vec<GradientQuad> {
        let mut out = Vec::new();
        for &order in self.active.values() {
            let s = &self.specs[&order];
            let main = (ctx.color)(s.main, s.main_hsv);
            let detail = (ctx.color)(s.detail, s.detail_hsv);
            // Opacities go through bytes (setValues takes GLubytes).
            let o_main = ((ctx.opacity)(s.main, &s.groups) * 255.0) as i32 as f32 / 255.0;
            let o_detail = ((ctx.opacity)(s.detail, &s.groups) * 255.0) as i32 as f32 / 255.0;
            if (o_main == 0.0 && o_detail == 0.0) || (s.blending == 1 && main == [0; 3] && detail == [0; 3]) {
                continue;
            }
            let (l, b, w, h) = ctx.view;
            let m = if s.screen_space() { 20.0 / ctx.zoom.max(1e-3) } else { 20.0 };
            let (min, max) = ((l - m, b - m), (l + w + m, b + h + m));
            let pos = |g: i32| if g > 0 { (ctx.group_position)(g) } else { None };
            let corners = if s.vertex_mode {
                let bl = pos(s.u).unwrap_or(min);
                let br = pos(s.d).unwrap_or((max.0, min.1));
                let tl = pos(s.l).unwrap_or((min.0, max.1));
                let tr = pos(s.r).unwrap_or(max);
                [bl, br, tl, tr]
            } else {
                let bottom = pos(s.d).map_or(min.1, |p| p.1);
                let left = pos(s.l).map_or(min.0, |p| p.0);
                let top = pos(s.u).map_or(max.1, |p| p.1);
                let right = pos(s.r).map_or(max.0, |p| p.0);
                [(left, bottom), (right, bottom), (left, top), (right, top)]
            };
            let (sin, cos) = (s.rotation * -0.017_453_292).sin_cos();
            let colors = corner_colors((cos, sin), (main, o_main), (detail, o_detail), matches!(s.blending, 2 | 3));
            out.push(GradientQuad { layer: s.layer, z: s.z, blending: s.blending, corners, colors });
        }
        out.sort_by_key(|q| (q.layer, q.z));
        out
    }
}

/// `CCLayerGradient::updateColor` with compressed interpolation: start color S at the side the
/// vector points away from; corners bottom-left, bottom-right, top-left, top-right.
pub fn corner_colors(vector: (f32, f32), start: ([u8; 3], f32), end: ([u8; 3], f32), premultiply: bool) -> [[f32; 4]; 4] {
    let len = (vector.0 * vector.0 + vector.1 * vector.1).sqrt();
    if len == 0.0 {
        return [[0.0; 4]; 4];
    }
    let (mut ux, mut uy) = (vector.0 / len, vector.1 / len);
    let k = (1.0 / (ux.abs() + uy.abs())) * std::f32::consts::SQRT_2;
    ux *= k;
    uy *= k;
    let c = |rgb: [u8; 3], a: f32| {
        let f = |v: u8| v as f32 / 255.0 * if premultiply { a } else { 1.0 };
        [f(rgb[0]), f(rgb[1]), f(rgb[2]), a]
    };
    let s = c(start.0, start.1);
    let e = c(end.0, end.1);
    let c2 = std::f32::consts::SQRT_2;
    let w = [(ux + c2 + uy), (c2 - ux + uy), (ux + c2 - uy), (c2 - ux - uy)].map(|v| v / 2.828_427);
    w.map(|t| [0, 1, 2, 3].map(|i| e[i] + t * (s[i] - e[i])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LevelString;

    #[test]
    fn parses_and_switches() {
        let level = LevelString::parse("kS38,1;1,2903,2,375,3,-165,6,-90,21,78,22,79,174,1,202,14;1,2903,2,825,3,-195,6,-90,21,78,22,79,174,1,202,5,203,103,204,102;1,2903,2,900,3,0,208,1;");
        let mut g = Gradients::from_level(&level);
        let s = &g.specs[&0];
        assert_eq!((s.blending, s.layer, s.id, s.main, s.detail), (1, 14, 0, 78, 79));
        assert!(s.screen_space());
        g.fire(0);
        assert_eq!(g.active.get(&0), Some(&0));
        g.fire(1); // same id 0: replaces
        assert_eq!(g.active.get(&0), Some(&1));
        assert_eq!(g.specs[&1].u, 103);
        g.fire(2); // disable id 0
        assert!(g.active.is_empty());
    }

    #[test]
    fn corner_colors_follow_the_vector() {
        // Rotation −90: vector (cos 90°, sin 90°) = (0, 1): the start color at the bottom.
        let (sin, cos) = (90f32.to_radians()).sin_cos();
        let c = corner_colors((cos, sin), ([255, 0, 0], 1.0), ([0, 0, 255], 1.0), false);
        assert!((c[0][0] - 1.0).abs() < 1e-5 && c[0][2].abs() < 1e-5); // bottom-left red
        assert!((c[2][2] - 1.0).abs() < 1e-5 && c[2][0].abs() < 1e-5); // top-left blue
        // Premultiplied halves the color with alpha 0.5.
        let c = corner_colors((1.0, 0.0), ([255, 255, 255], 0.5), ([255, 255, 255], 0.5), true);
        assert!((c[1][0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn rect_edges_from_groups() {
        let level = LevelString::parse("kS38,1;1,2903,2,0,3,0,21,1,22,2,202,6,204,7;");
        let mut g = Gradients::from_level(&level);
        g.fire(0);
        let gp = |grp| (grp == 7).then_some((50.0, 300.0));
        let color = |_, _| [255, 255, 255];
        let opacity = |_: i32, _: &[i32]| 1.0;
        let ctx = GradientContext { view: (0.0, 0.0, 480.0, 320.0), zoom: 1.0, group_position: &gp, color: &color, opacity: &opacity };
        let q = g.quads(&ctx);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].corners[0], (-20.0, 300.0));
        assert_eq!(q[0].corners[3], (500.0, 340.0));
    }
}

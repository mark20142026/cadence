//! Shader and gradient triggers in the draw: the glue between `cadence_core::shaders` /
//! `cadence_core::gradients` and the renderer (`render::PostFx`, the gradient pipelines).
//!
//! The draw list is built in the game's z order; `LayerMarks` records where each of the
//! game's shader/gradient z layers starts in it (1 BG, 2 MG, 3–7 B5–B1, 8 P, 9–12 T1–T4,
//! 13 G, 14 UI, 15 Max; `SetupShaderEffectPopup::zLayerToString` 0x25d2ec). Gradients are
//! spliced in at their layer, and the shader layer covers `marks[min]..end(max)`
//! (`GJBaseGameLayer::updateShaderLayer` 0xf3448 reparents those layers into the
//! `ShaderLayer`).

use cadence_core::gradients::{GradientContext, Gradients};
use cadence_core::level::LevelString;
use cadence_core::physics::Player;
use cadence_core::shaders::{Context, Shaders};
use cadence_core::triggers::{Colors, Hsv, transform_color};
use cadence_core::world::World;

use crate::render::{Blend, Draw, PostFx, gradient_instances};

/// The level's shader and gradient triggers and their state.
#[derive(Clone, Default)]
pub struct LevelFx {
    pub shaders: Shaders,
    pub gradients: Gradients,
    /// Each group's main object (its first by level order; `tryGetMainObject`).
    main_objects: std::sync::Arc<std::collections::HashMap<i32, u32>>,
}

/// A group's main object, where it is now (move triggers update `World::level_position`).
fn group_position(world: &World, mains: &std::collections::HashMap<i32, u32>, group: i32) -> Option<(f32, f32)> {
    let order = *mains.get(&group)?;
    let (x, y) = world.level_position(order as usize);
    Some((x as f32, y as f32))
}

/// A target's world position (`GJBaseGameLayer::positionForShaderTarget` 0xf458c): −1 player
/// 1, −2 player 2 (player 1 when not dual), else the group's main object.
fn target_position(world: &World, mains: &std::collections::HashMap<i32, u32>, player: &Player, id: i32) -> Option<(f32, f32)> {
    match id {
        -1 => Some((player.x, player.y)),
        -2 => Some(player.dual.as_ref().map_or((player.x, player.y), |p| (p.x, p.y))),
        id if id > 0 => group_position(world, mains, id),
        _ => None,
    }
}

impl LevelFx {
    pub fn new(level: &LevelString) -> LevelFx {
        let mut mains = std::collections::HashMap::new();
        for (order, o) in level.objects.iter().enumerate() {
            for g in o.groups() {
                mains.entry(g).or_insert(order as u32);
            }
        }
        LevelFx { shaders: Shaders::from_level(level), gradients: Gradients::from_level(level), main_objects: std::sync::Arc::new(mains) }
    }

    /// A new attempt (`resetAllShaders`, gradients off).
    pub fn reset(&mut self) {
        self.shaders.state = cadence_core::shaders::ShaderState::new();
        self.gradients.active.clear();
    }

    /// A spawned, touched or passed trigger fired (`World::fired_effects`).
    pub fn fire(&mut self, order: u32) {
        self.shaders.fire(order);
        self.gradients.fire(order);
    }

    /// The end of a frame of `dt` game seconds (after `GJEffectManager::updateEffects`).
    pub fn frame(&mut self, dt: f32, world: &World, player: &Player) {
        if !self.shaders.is_empty() {
            let mains = &self.main_objects;
            self.shaders.state.frame(dt, &|id| target_position(world, mains, player, id));
        }
    }
}

/// Where each z layer starts in the draw list; `start[16]` is the end of the world (before the
/// menus drawn over it).
#[derive(Clone, Copy, Debug, Default)]
pub struct LayerMarks {
    pub start: [usize; 17],
    /// Positions in the object vectors where each flushed z layer begins (game z layer, index).
    below: [Option<usize>; 17],
    above: [Option<usize>; 17],
}

/// The game z layer (key 24: B5 −5 … T4 11) → the shader/gradient layer index.
fn layer_of_z(z: i32) -> usize {
    match z {
        ..=-4 => 3,
        -3..=-2 => 4,
        -1..=0 => 5,
        1..=2 => 6,
        3 => 7,
        4..=5 => 9,
        6..=7 => 10,
        8..=9 => 11,
        _ => 12,
    }
}

impl LayerMarks {
    /// An object z layer starts at `len` in the B (`below`) or T (`object_draws`) list.
    pub fn note(&mut self, z: i32, below: bool, len: usize) {
        let l = layer_of_z(z);
        let slot = if below { &mut self.below[l] } else { &mut self.above[l] };
        if slot.is_none() {
            *slot = Some(len);
        }
    }

    /// The B layers were appended at `base` (`len` draws), the T layers at `t_base`.
    pub fn place_objects(&mut self, base: usize, len: usize, t_base: usize, t_len: usize) {
        for (range, list, b, n) in [(3..=7usize, self.below, base, len), (9..=12, self.above, t_base, t_len)] {
            for l in range {
                // the first flushed layer at or above l
                let first = (l..=12).filter_map(|k| list[k]).min().unwrap_or(n);
                self.start[l] = b + first;
            }
        }
    }
}

/// Insert the gradients at their layers; returns the new marks.
pub fn splice_gradients(draws: &mut Vec<Draw>, marks: &LayerMarks, grads: Vec<(usize, Vec<Draw>)>) -> [usize; 17] {
    let m = marks.start;
    let at = |layer: usize| match layer {
        0 | 1 => m[2],
        2 => m[3],
        8 => m[9],
        13 => m[14],
        14 | 15 => m[15],
        l => m[l],
    };
    let mut grads: Vec<(usize, usize, Vec<Draw>)> = grads.into_iter().map(|(l, d)| (at(l), l, d)).collect();
    grads.sort_by_key(|g| (g.0, g.1));
    let mut new_m = m;
    for (k, nm) in new_m.iter_mut().enumerate().skip(1) {
        let before: usize = grads.iter().filter(|(ip, l, _)| *ip < m[k] || (*ip == m[k] && *l < k)).map(|g| g.2.len()).sum();
        *nm = m[k] + before;
    }
    if grads.is_empty() {
        return new_m;
    }
    let old = std::mem::take(draws);
    let mut gi = grads.into_iter().peekable();
    for (i, d) in old.into_iter().enumerate() {
        while gi.peek().is_some_and(|g| g.0 == i) {
            draws.extend(gi.next().unwrap().2);
        }
        draws.push(d);
    }
    for g in gi {
        draws.extend(g.2);
    }
    new_m
}

/// The frame's gradient draws by layer.
pub fn gradient_draws(fx: &LevelFx, colors: &Colors, world: &World, view: (f32, f32, f32, f32), zoom: f32) -> Vec<(usize, Vec<Draw>)> {
    if fx.gradients.active.is_empty() {
        return Vec::new();
    }
    let gp = |g: i32| group_position(world, &fx.main_objects, g);
    let color = |c: i32, hsv: Option<Hsv>| {
        let a = colors.active(c);
        hsv.map_or(a, |h| transform_color(a, h))
    };
    let opacity = |c: i32, groups: &[i32]| colors.object_opacity(c, groups);
    let ctx = GradientContext { view, zoom, group_position: &gp, color: &color, opacity: &opacity };
    let quads = fx.gradients.quads(&ctx);
    if std::env::var_os("CADENCE_DEBUG_FX").is_some() {
        crate::app::log(&format!("gradients {:?} quads {quads:?}", fx.gradients.active));
    }
    quads
        .into_iter()
        .map(|q| {
            let blend = Blend::gradient(q.blending);
            let draws = gradient_instances(q.corners, q.colors).map(|instance| Draw { sheet: 0, blend, instance }).to_vec();
            (q.layer.clamp(0, 15) as usize, draws)
        })
        .collect()
}

/// The frame's shader layer, if any effect is on.
#[allow(clippy::too_many_arguments)]
pub fn post_fx(
    fx: &mut LevelFx,
    colors: &Colors,
    world: &World,
    player: &Player,
    view: (f32, f32, f32, f32),
    win: (f32, f32),
    zoom: f32,
    marks: &[usize; 17],
) -> Option<PostFx> {
    if fx.shaders.is_empty() || std::env::var_os("CADENCE_NO_POSTFX").is_some() {
        return None;
    }
    let mains = fx.main_objects.clone();
    let pos = |id: i32| target_position(world, &mains, player, id);
    let channel = |c: i32| (colors.active(c), colors.blending(c));
    let ctx = Context { view, win, zoom, position: &pos, channel: &channel, background: colors.active(1000) };
    let state = &mut fx.shaders.state;
    let u = state.uniforms(&ctx)?;
    if std::env::var_os("CADENCE_DEBUG_FX").is_some() {
        crate::app::log(&format!("shader layers {}..{} uniforms {:?}", state.min_layer, state.max_layer, u.0));
    }
    let min = state.min_layer.clamp(1, 15) as usize;
    let max = state.max_layer.clamp(1, 15) as usize;
    let start = marks[min];
    let end = marks[max + 1].max(start);
    Some(PostFx { range: (start, end), uniforms: u.0 })
}

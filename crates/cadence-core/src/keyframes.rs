//! Keyframe animations: keyframe objects (3032) and the keyframe animation trigger (3033), after
//! `FUN_1000e6070` (the trigger), `GJEffectManager::createKeyframeCommand` (0x26a6fc) and the
//! mode-5 branch of `GJEffectManager::prepareMoveActions` (0x26dd8c).
//!
//! Keyframe objects with the same keyframe group (373) form an animation, ordered by their index
//! (374, `updateKeyframeOrder`). The trigger finds the keyframe objects in its animation group
//! (76) and starts one command per keyframe group on its target group (51, else the first
//! keyframe's 51). Relative to the first keyframe, each keyframe gives a position, scale and
//! rotation (scaled by the trigger's mods 521 / 545, 523 / 546, 522); a command keeps each
//! keyframe's change from the previous one, its duration (10 × the time mod 520), easing (30 /
//! 85) and the group it spawns when reached (71, after 557 seconds; at 95% with 377). Each step
//! the elapsed time walks the keyframes: finished segments count fully, the current one by its
//! eased progress; the group moves by the change of the summed offset, turns about its main
//! object by the change of the summed rotation (+ 360 per revolution 537, the shorter way unless
//! direction 536 forces one) and scales by the change of 1 + the summed scale changes. Not
//! ported: curves (378, the game's splines), reference-only keyframes' time modes (375 / 379),
//! closing the loop (376) and the center group's own rotation turning the moves.

use crate::level::LevelString;
use crate::world::World;

/// A keyframe object's values (`KeyframeGameObject::customObjectSetup` 0x16530c).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframeObj {
    pub order: u32,
    pub group: i32,
    pub index: i32,
    pub target: i32,
    pub duration: f32,
    pub easing: i32,
    pub rate: f32,
    pub spawn: i32,
    pub spawn_delay: f32,
    pub proximity: bool,
    pub direction: i32,
    pub revolutions: i32,
    pub scale: (f32, f32),
    pub rotation: f32,
}

/// The trigger's values (`KeyframeAnimTriggerObject::customObjectSetup` 0x15c158).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframeAnimSpec {
    pub anim_group: i32,
    pub target: i32,
    pub center: i32,
    pub time_mod: f32,
    pub pos_mod: (f32, f32),
    pub rot_mod: f32,
    pub scale_mod: (f32, f32),
}

/// One keyframe of a running command (`KeyframeObject`, 0x1c0 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    duration: f64,
    easing: i32,
    rate: f32,
    spawn: i32,
    spawn_delay: f32,
    proximity: bool,
    spawned: bool,
    delta: (f32, f32),
    scale: (f64, f64),
    rotation: f64,
}

/// A running keyframe command (a `GroupCommandObject2` in mode 5).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeCommand {
    pub group: i32,
    keys: Vec<Key>,
    elapsed: f64,
    total: f64,
    /// What was applied so far (+0x1d0 offset, +0xd8 / +0xe0 scale, +0xe8 rotation).
    offset: (f32, f32),
    scale: (f64, f64),
    rotation: f64,
    pub done: bool,
}

/// The level's keyframes and running keyframe commands.
#[derive(Clone, Debug, Default)]
pub struct Keyframes {
    /// Keyframe objects by keyframe group, ordered by index.
    pub groups: std::sync::Arc<std::collections::HashMap<i32, Vec<KeyframeObj>>>,
    pub specs: std::sync::Arc<std::collections::HashMap<u32, KeyframeAnimSpec>>,
    pub commands: Vec<KeyframeCommand>,
}

impl Keyframes {
    pub fn new(level: &LevelString) -> Keyframes {
        let mut groups: std::collections::HashMap<i32, Vec<KeyframeObj>> = Default::default();
        let mut specs = std::collections::HashMap::new();
        for (order, o) in level.objects.iter().enumerate() {
            let f = |k: u32, d: f32| o.get_f32(k).unwrap_or(d);
            let i = |k: u32| o.get_i32(k).unwrap_or(0);
            match o.id() {
                3032 => {
                    let s = f(32, 1.0);
                    let flip = |k: u32| if o.get(k) == Some("1") { -1.0 } else { 1.0 };
                    let k = KeyframeObj {
                        order: order as u32,
                        group: i(373),
                        index: i(374),
                        target: i(51),
                        duration: f(10, 0.0),
                        easing: i(30),
                        rate: o.get_f32(85).filter(|&r| r > 0.0).unwrap_or(2.0),
                        spawn: i(71),
                        spawn_delay: o.get_f32(557).or_else(|| o.get_f32(63)).unwrap_or(0.0),
                        proximity: o.get(377) == Some("1"),
                        direction: i(536),
                        revolutions: i(537),
                        scale: (f(128, s) * flip(4), f(129, s) * flip(5)),
                        rotation: f(6, 0.0),
                    };
                    groups.entry(k.group).or_default().push(k);
                }
                3033 => {
                    // absent mods are 0 (`customObjectSetup` 0x15c158); a y mod of 0 takes the
                    // x one (545 → 521, 546 → 523)
                    let (px, sx) = (f(521, 0.0), f(523, 0.0));
                    let (py, sy) = (f(545, 0.0), f(546, 0.0));
                    specs.insert(
                        order as u32,
                        KeyframeAnimSpec {
                            anim_group: i(76),
                            target: i(51),
                            center: i(71),
                            time_mod: f(520, 0.0),
                            pos_mod: (px, if py == 0.0 { px } else { py }),
                            rot_mod: f(522, 0.0),
                            scale_mod: (sx, if sy == 0.0 { sx } else { sy }),
                        },
                    );
                }
                _ => {}
            }
        }
        for list in groups.values_mut() {
            list.sort_by(|a, b| a.index.cmp(&b.index).then(a.order.cmp(&b.order)));
        }
        Keyframes { groups: std::sync::Arc::new(groups), specs: std::sync::Arc::new(specs), commands: Vec::new() }
    }
}

impl World {
    /// The keyframe animation trigger (3033) fired: one command per keyframe group among the
    /// keyframe objects in its animation group.
    pub(crate) fn keyframe_fire(&mut self, order: u32) {
        let Some(spec) = self.keyframes.specs.get(&order).copied() else { return };
        let members: Vec<u32> = self.group_members(spec.anim_group).cloned().unwrap_or_default();
        let groups = std::sync::Arc::clone(&self.keyframes.groups);
        if std::env::var_os("CADENCE_DEBUG_KEYFRAMES").is_some() {
            eprintln!("keyframe trigger {order}: {spec:?}, animation group members {members:?}, keyframe groups {:?}", groups.keys().collect::<Vec<_>>());
        }
        let mut seen = Vec::new();
        for o in members {
            let Some((kg, first)) = groups.iter().find_map(|(g, list)| list.iter().find(|k| k.order == o).map(|k| (*g, *k))) else { continue };
            if seen.contains(&kg) {
                continue;
            }
            seen.push(kg);
            let list = &groups[&kg];
            let target = if spec.target != 0 { spec.target } else { first.target };
            if let Some(cmd) = self.keyframe_command(&spec, list, target) {
                self.keyframes.commands.push(cmd);
            }
        }
    }

    /// `createKeyframeCommand`: each keyframe's change from the previous one.
    fn keyframe_command(&self, spec: &KeyframeAnimSpec, list: &[KeyframeObj], target: i32) -> Option<KeyframeCommand> {
        if list.is_empty() {
            return None;
        }
        let time_mod = spec.time_mod.max(0.0001);
        let mut keys = Vec::with_capacity(list.len());
        let mut base = ((0.0f32, 0.0f32), 0.0f64, (1.0f64, 1.0f64));
        let mut prev = ((0.0f32, 0.0f32), 0.0f64, (1.0f64, 1.0f64));
        for (k, o) in list.iter().enumerate() {
            let (px, py) = self.level_pos_of(o.order as usize);
            let mut pos = (px as f32, py as f32);
            let (mut sx, mut sy) = (o.scale.0 as f64, o.scale.1 as f64);
            let mut rot = o.rotation as f64 + self.turned[o.order as usize] as f64;
            if sx < 0.0 && sy < 0.0 {
                rot += if rot <= 0.0 { 180.0 } else { -180.0 };
                sx = -sx;
                sy = -sy;
            }
            if k == 0 {
                base = (pos, rot, (sx, sy));
                sx = 1.0;
                sy = 1.0;
            } else {
                if spec.pos_mod.0 != 1.0 {
                    pos.0 = base.0 .0 + spec.pos_mod.0 * (pos.0 - base.0 .0);
                }
                if spec.pos_mod.1 != 1.0 {
                    pos.1 = base.0 .1 + spec.pos_mod.1 * (pos.1 - base.0 .1);
                }
                if spec.scale_mod.0 != 1.0 {
                    sx = base.2 .0 + spec.scale_mod.0 as f64 * (sx - base.2 .0);
                }
                if spec.scale_mod.1 != 1.0 {
                    sy = base.2 .1 + spec.scale_mod.1 as f64 * (sy - base.2 .1);
                }
                if spec.rot_mod != 1.0 {
                    rot = base.1 + spec.rot_mod as f64 * (rot - base.1);
                }
                sx /= base.2 .0;
                sy /= base.2 .1;
            }
            let full = rot + (o.revolutions * 360) as f64;
            let mut key = Key {
                duration: (time_mod * o.duration) as f64,
                easing: o.easing,
                rate: o.rate,
                spawn: 0,
                spawn_delay: 0.0,
                proximity: false,
                spawned: false,
                delta: (0.0, 0.0),
                scale: (sx, sy),
                rotation: full,
            };
            if k > 0 {
                key.delta = (pos.0 - prev.0 .0, pos.1 - prev.0 .1);
                key.scale = (sx - prev.2 .0, sy - prev.2 .1);
                let mut d = full - prev.1;
                let turn = d - (o.revolutions * 360) as f64;
                if 180.0 < turn || (0.0 < turn && o.direction == 2) {
                    d -= 360.0;
                } else if turn < -180.0 || (turn < 0.0 && o.direction == 1) {
                    d += 360.0;
                }
                key.rotation = d;
                // the previous keyframe spawns this one's group when it's reached
                let p: &mut Key = keys.last_mut().unwrap();
                p.spawn = o.spawn;
                p.spawn_delay = o.spawn_delay;
                p.proximity = o.proximity;
            }
            prev = (pos, rot, (sx, sy));
            keys.push(key);
        }
        let total = keys[..keys.len() - 1].iter().map(|k| k.duration).sum();
        Some(KeyframeCommand { group: target, keys, elapsed: 0.0, total, offset: (0.0, 0.0), scale: (1.0, 1.0), rotation: 0.0, done: false })
    }

    /// The keyframe commands' step (`prepareMoveActions`, mode 5): this step's move, turn and
    /// scale per command, and the groups to spawn.
    pub(crate) fn keyframe_step(&mut self, dt: f64) -> Vec<KeyframeStep> {
        let mut out = Vec::new();
        for cmd in &mut self.keyframes.commands {
            if cmd.done {
                continue;
            }
            cmd.elapsed += dt;
            let n = cmd.keys.len();
            let mut t = cmd.elapsed;
            let mut offset = (0.0f32, 0.0f32);
            let (mut sx, mut sy, mut rot) = (1.0f64, 1.0f64, 0.0f64);
            let mut spawns = Vec::new();
            for k in 0..n.saturating_sub(1) {
                let d = cmd.keys[k].duration;
                let complete = d <= t && (cmd.total <= cmd.elapsed || k + 1 != n - 1);
                let p = if complete {
                    1.0
                } else {
                    let mut p = t / d;
                    if cmd.keys[k].easing > 0 {
                        p = crate::easing::eased(p as f32, cmd.keys[k].rate, cmd.keys[k].easing) as f64;
                    }
                    p
                };
                let near = 0.95 < p && cmd.keys[k].proximity;
                if (complete || near) && !cmd.keys[k].spawned {
                    cmd.keys[k].spawned = true;
                    if cmd.keys[k].spawn > 0 {
                        spawns.push((cmd.keys[k].spawn, cmd.keys[k].spawn_delay));
                    }
                }
                let next = cmd.keys[k + 1];
                offset.0 = (offset.0 as f64 + next.delta.0 as f64 * p) as f32;
                offset.1 = (offset.1 as f64 + next.delta.1 as f64 * p) as f32;
                sx += next.scale.0 * p;
                sy += next.scale.1 * p;
                rot += next.rotation * p;
                let more = d <= t;
                t -= d;
                if !more {
                    break;
                }
            }
            let mv = (offset.0 - cmd.offset.0, offset.1 - cmd.offset.1);
            cmd.offset = offset;
            let turn = rot - cmd.rotation;
            cmd.rotation = rot;
            let scale = if 0.01 <= sx.abs() && 0.01 <= sy.abs() && (sx != cmd.scale.0 || sy != cmd.scale.1) {
                let f = (sx / cmd.scale.0, sy / cmd.scale.1);
                cmd.scale = (sx, sy);
                Some(f)
            } else {
                None
            };
            if cmd.total <= cmd.elapsed {
                cmd.done = true;
            }
            if std::env::var_os("CADENCE_DEBUG_KEYFRAMES").is_some() {
                eprintln!("keyframe command on {}: t {:.4} move {mv:?} turn {turn} scale {scale:?}", cmd.group, cmd.elapsed);
            }
            out.push(KeyframeStep { group: cmd.group, mv, turn, scale, spawns });
        }
        self.keyframes.commands.retain(|c| !c.done);
        out
    }
}

/// One command's step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeyframeStep {
    pub group: i32,
    pub mv: (f32, f32),
    pub turn: f64,
    pub scale: Option<(f64, f64)>,
    pub spawns: Vec<(i32, f32)>,
}

#[cfg(test)]
mod tests {
    use crate::level::LevelString;
    use crate::world::World;

    #[test]
    fn keyframes_move_and_turn_the_group() {
        // two keyframes 100 apart and turned 90°, 1 s; the trigger animates group 5
        let level = LevelString::parse(
            "kA2,0;1,5,2,0,3,300,57,5;1,3032,2,100,3,100,373,1,374,0,10,1,57,7;1,3032,2,200,3,100,373,1,374,1,6,90,10,1,57,7;1,3033,2,0,3,0,76,7,51,5,520,1,521,1,522,1,523,1",
        );
        let mut w = World::from_level(&level);
        let start = w.level_position(0);
        w.keyframe_fire(3);
        for _ in 0..120 {
            w.pre_step((0.0, 0.0));
        }
        let half = w.level_position(0);
        assert!(((half.0 - start.0) - 50.0).abs() < 0.5, "{half:?}");
        for _ in 0..200 {
            w.pre_step((0.0, 0.0));
        }
        let end = w.level_position(0);
        assert!(((end.0 - start.0) - 100.0).abs() < 1e-3, "{end:?}");
        assert!((w.turned[0] - 90.0).abs() < 1e-3, "{}", w.turned[0]);
        assert!(w.keyframes.commands.is_empty());
    }
}

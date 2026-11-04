//! Keyframe animations: keyframe objects (3032) and the keyframe animation trigger (3033), after
//! `FUN_1000e6070` (the trigger), `GJEffectManager::createKeyframeCommand` (0x26a6fc),
//! `KeyframeObject::setupSpline` (0xcb680) and the mode-5 branch of
//! `GJEffectManager::prepareMoveActions` (0x26dd8c).
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
//! direction 536 forces one) and scales by the change of 1 + the summed scale changes.
//!
//! Close loop (376 on the last keyframe): the first keyframe is appended once more, so the
//! animation returns to it. Reference-only keyframes (375; never the first, nor the last unless
//! the loop closes) shape the path inside the previous real keyframe's time: its time mode (379)
//! keeps the durations (0), splits its own duration evenly over the run (1) or by distance (2);
//! with easing the whole run is eased as one (its "ref run duration"), and reference-only
//! keyframes are never eased themselves. Curves (378): each run of curve keyframes up to the next
//! plain one is a natural cubic spline of the positions over time (`spline.rs`), started at the
//! run's first keyframe; a keyframe at the same place as the next one can't curve. Not ported:
//! the center group's own rotation turning the moves (the +0x628 list) and the scale's axis
//! angle (+0xe8 of the scale command).

use crate::level::LevelString;
use crate::spline::Spline;
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
    /// 375 reference only (+0x748), 378 curve (+0x74a), 376 close loop (+0x74b), 379 time mode
    /// (+0x74c).
    pub ref_only: bool,
    pub curve: bool,
    pub close_loop: bool,
    pub time_mode: i32,
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
#[derive(Clone, Debug, PartialEq)]
struct Key {
    duration: f64,
    easing: i32,
    rate: f32,
    ref_only: bool,
    spawn: i32,
    spawn_delay: f32,
    proximity: bool,
    spawned: bool,
    curve: bool,
    /// The spline pair (x(t), y(t)) of the curve run starting here and its time length
    /// (m_spline1 / m_spline2 / m_curveLength).
    spline: Option<std::sync::Arc<(Spline, Spline)>>,
    curve_len: f64,
    /// m_refRunDuration: the length the reference-only run after this keyframe is eased over.
    ref_run: f64,
    time_mode: i32,
    delta: (f32, f32),
    position: (f32, f32),
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
            let b = |k: u32| o.get_i32(k).is_some_and(|v| v != 0);
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
                        ref_only: b(375),
                        curve: b(378),
                        close_loop: b(376),
                        time_mode: i(379),
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

    /// `createKeyframeCommand`: each keyframe's change from the previous one, then the
    /// reference-only runs' times and the curve runs' splines.
    fn keyframe_command(&self, spec: &KeyframeAnimSpec, list: &[KeyframeObj], target: i32) -> Option<KeyframeCommand> {
        if list.is_empty() {
            return None;
        }
        let time_mod = spec.time_mod.max(0.0001);
        let count = list.len();
        let mut keys: Vec<Key> = Vec::with_capacity(count + 1);
        let mut base = ((0.0f32, 0.0f32), 0.0f64, (1.0f64, 1.0f64));
        // The previous keyframe's position, rotation (without its revolutions) and scale, the
        // last two kept as f32 (s8 / d11 in the compiled loop).
        let mut prev = ((0.0f32, 0.0f32), 0.0f64, (1.0f64, 1.0f64));
        let mut prev_raw: Option<(f32, f32)> = None;
        let mut k = 0usize;
        loop {
            let o = &list[k];
            let (px, py) = self.level_pos_of(o.order as usize);
            let raw = (px as f32, py as f32);
            let mut pos = raw;
            let (mut sx, mut sy) = (o.scale.0 as f64, o.scale.1 as f64);
            let mut rot = o.rotation as f64 + self.turned[o.order as usize] as f64;
            if sx < 0.0 && sy < 0.0 {
                rot += if rot <= 0.0 { 180.0 } else { -180.0 };
                sx = -sx;
                sy = -sy;
            }
            if k == 0 {
                // (Also the closing keyframe: the first one again, unmodded.)
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
            // Reference only: never the first keyframe, nor the last unless the loop closes.
            let ref_ok = 0 < k && (o.close_loop || k < count - 1);
            let mut key = Key {
                duration: (time_mod * o.duration) as f64,
                easing: o.easing,
                rate: o.rate,
                ref_only: o.ref_only && ref_ok,
                spawn: 0,
                spawn_delay: 0.0,
                proximity: false,
                spawned: false,
                curve: o.curve,
                spline: None,
                curve_len: 0.0,
                ref_run: 0.0,
                time_mode: o.time_mode,
                delta: (0.0, 0.0),
                position: pos,
                scale: (sx, sy),
                rotation: full,
            };
            if !keys.is_empty() {
                // Two keyframes at the same place: the first one can't curve.
                if prev_raw == Some(raw) {
                    keys.last_mut().unwrap().curve = false;
                }
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
            prev = (pos, rot as f32 as f64, (sx as f32 as f64, sy as f32 as f64));
            prev_raw = Some(raw);
            keys.push(key);
            // Close loop (the last keyframe's 376): the first keyframe once more, then stop.
            if keys.len() > count {
                break;
            }
            if k == count - 1 {
                if !o.close_loop {
                    break;
                }
                k = 0;
            } else {
                k += 1;
            }
        }
        // Reference-only runs: [the real keyframe before, the refs..., the next real one].
        let mut run: Vec<usize> = Vec::new();
        let mut start = 0usize;
        let mut ref_sum = 0.0f64;
        for k in 0..keys.len() {
            if keys[k].ref_only {
                if run.is_empty() {
                    run.push(start);
                }
                run.push(k);
                ref_sum += keys[k].duration;
                continue;
            }
            if !run.is_empty() {
                run.push(k);
                let own = keys[start].duration;
                let segs = run.len() - 1;
                let mut extra = ref_sum;
                match keys[start].time_mode {
                    // by distance (ccpDistance: sqrtf(fma(dx, dx, dy·dy)))
                    2 => {
                        extra = 0.0;
                        let mut total = 0.0f64;
                        for i in 0..segs {
                            let (a, b) = (keys[run[i]].position, keys[run[i + 1]].position);
                            let (dx, dy) = (a.0 - b.0, a.1 - b.1);
                            let dist = dx.mul_add(dx, dy * dy).sqrt() as f64;
                            total += dist;
                            keys[run[i]].duration = dist;
                        }
                        for i in 0..segs {
                            let k = run[i];
                            keys[k].duration = if total == 0.0 { own / segs as f64 } else { own * (keys[k].duration / total) };
                        }
                    }
                    // evenly
                    1 => {
                        extra = 0.0;
                        for i in 0..segs {
                            keys[run[i]].duration = own / segs as f64;
                        }
                    }
                    _ => {}
                }
                if 0 < keys[start].easing {
                    keys[start].ref_run = own + extra;
                }
            }
            run.clear();
            ref_sum = 0.0;
            start = k;
        }
        let n = keys.len();
        let total = keys[..n - 1].iter().map(|k| k.duration).sum();
        // Curve runs: the curve keyframes up to the next plain one (or the last); a spline needs
        // two curve keyframes and the end, otherwise they don't curve.
        let mut curve_run: Vec<usize> = Vec::new();
        for k in 0..n {
            if k < n - 1 && keys[k].curve {
                curve_run.push(k);
                continue;
            }
            if curve_run.is_empty() {
                continue;
            }
            if curve_run.len() < 2 {
                for &c in &curve_run {
                    keys[c].curve = false;
                }
            } else {
                curve_run.push(k);
                let first = curve_run[0];
                let m = curve_run.len();
                let (mut t, mut xs, mut ys) = (vec![0.0f64; m], vec![0.0f64; m], vec![0.0f64; m]);
                for i in 1..m {
                    let key = &keys[curve_run[i]];
                    xs[i] = xs[i - 1] + key.delta.0 as f64;
                    ys[i] = ys[i - 1] + key.delta.1 as f64;
                    t[i] = t[i - 1] + keys[curve_run[i - 1]].duration.max(1e-5);
                }
                keys[first].curve_len = t[m - 1];
                keys[first].spline = Some(std::sync::Arc::new((Spline::new(&t, &xs), Spline::new(&t, &ys))));
            }
            curve_run.clear();
        }
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
            // The time along the keyframes with reference-only runs eased as one: `adj` is the
            // eased elapsed time, `rem` the time into the current keyframe `cur`.
            let mut rem = cmd.elapsed;
            let mut adj = 0.0f64;
            let mut cur: Option<usize> = None;
            if n >= 2 {
                let mut k = 0;
                loop {
                    let key = &cmd.keys[k];
                    let mut t = rem;
                    if 0.0 < key.ref_run && rem < key.ref_run {
                        t = key.ref_run * crate::easing::eased((rem / key.ref_run) as f32, key.rate, key.easing) as f64;
                    }
                    let d = key.duration;
                    let into = k == n - 2 || t < d;
                    if into {
                        rem = t;
                        adj += t;
                    } else {
                        rem = t - d;
                        adj += d;
                    }
                    if into || k + 1 >= n - 1 {
                        cur = Some(k);
                        break;
                    }
                    k += 1;
                }
            }
            let mut t = adj;
            let mut offset = (0.0f32, 0.0f32);
            let (mut sx, mut sy, mut rot) = (1.0f64, 1.0f64, 0.0f64);
            let mut on_curve = false;
            let mut spawns = Vec::new();
            for k in 0..n.saturating_sub(1) {
                let mut near_cur = false;
                let key = &cmd.keys[k];
                if let Some(sp) = key.spline.as_ref().filter(|_| 0.0 < key.curve_len && t < key.curve_len) {
                    // The spline's time, eased within the current keyframe when that one eases.
                    let mut s = t;
                    if let Some(c) = cur.map(|c| &cmd.keys[c]).filter(|c| rem < c.duration && 1 <= c.easing && !c.ref_only && c.ref_run <= 0.0) {
                        let e = crate::easing::eased((rem / c.duration) as f32, c.rate, c.easing);
                        s = (t - rem) + c.duration * e as f64;
                        near_cur = 0.95 < e && c.proximity;
                    }
                    offset.0 = (sp.0.at(s) + offset.0 as f64) as f32;
                    offset.1 = (sp.1.at(s) + offset.1 as f64) as f32;
                    on_curve = true;
                }
                let d = key.duration;
                let complete = d <= t && (cmd.total <= cmd.elapsed || k + 1 != n - 1);
                let p = if complete {
                    1.0
                } else {
                    let mut p = t / d;
                    if 0 < key.easing && !key.ref_only && key.ref_run <= 0.0 {
                        p = crate::easing::eased(p as f32, key.rate, key.easing) as f64;
                    }
                    p
                };
                let near = !complete && ((0.95 < p && key.proximity) || near_cur);
                let key = &mut cmd.keys[k];
                if (complete || near) && !key.spawned {
                    key.spawned = true;
                    if key.spawn > 0 {
                        spawns.push((key.spawn, key.spawn_delay));
                    }
                }
                let next = &cmd.keys[k + 1];
                if !on_curve {
                    offset.0 = (offset.0 as f64 + next.delta.0 as f64 * p) as f32;
                    offset.1 = (offset.1 as f64 + next.delta.1 as f64 * p) as f32;
                }
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

    /// A closed loop of three curved keyframes returns the group to where it started, along a
    /// curve (off the straight segments); a reference-only middle keyframe with time mode 1
    /// splits the first keyframe's second evenly.
    #[test]
    fn curves_close_loops_and_reference_keyframes() {
        let level = LevelString::parse(
            "kA2,0;1,5,2,0,3,300,57,5;1,3032,2,100,3,100,373,1,374,0,10,1,378,1,57,7;1,3032,2,200,3,100,373,1,374,1,10,1,378,1,57,7;1,3032,2,200,3,200,373,1,374,2,10,1,378,1,376,1,57,7;1,3033,2,0,3,0,76,7,51,5,520,1,521,1,522,1,523,1",
        );
        let mut w = World::from_level(&level);
        let start = w.level_position(0);
        w.keyframe_fire(4);
        // 0.5 s in: on the curve from (100,100) towards (200,100), bent away from the line y = 100.
        for _ in 0..120 {
            w.pre_step((0.0, 0.0));
        }
        let mid = w.level_position(0);
        assert!((mid.1 - start.1).abs() > 0.5, "{mid:?}");
        for _ in 0..800 {
            w.pre_step((0.0, 0.0));
        }
        let end = w.level_position(0);
        assert!((end.0 - start.0).abs() < 1e-3 && (end.1 - start.1).abs() < 1e-3, "{end:?} vs {start:?}");
        // Reference only, time mode 1: (0,0) → ref (100,0) → (100,100) in the first keyframe's 2 s.
        let level = LevelString::parse(
            "kA2,0;1,5,2,0,3,300,57,5;1,3032,2,100,3,100,373,1,374,0,10,2,379,1,57,7;1,3032,2,200,3,100,373,1,374,1,10,9,375,1,57,7;1,3032,2,200,3,200,373,1,374,2,10,1,57,7;1,3033,2,0,3,0,76,7,51,5,520,1,521,1,522,1,523,1",
        );
        let mut w = World::from_level(&level);
        let start = w.level_position(0);
        w.keyframe_fire(4);
        for _ in 0..240 {
            w.pre_step((0.0, 0.0));
        }
        let corner = w.level_position(0);
        assert!(((corner.0 - start.0) - 100.0).abs() < 1e-3 && (corner.1 - start.1).abs() < 1e-3, "{corner:?}");
        for _ in 0..240 {
            w.pre_step((0.0, 0.0));
        }
        let end = w.level_position(0);
        assert!(((end.0 - start.0) - 100.0).abs() < 1e-3 && ((end.1 - start.1) - 100.0).abs() < 1e-3, "{end:?}");
    }
}

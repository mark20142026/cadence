//! Checks the death effects against the real game's captures (the harness's `deathshot`
//! frames: `.scene`, `.waves` and `.particles` dumps of each frame after death).
//! `CADENCE_CHECK_DEATH=<dir>`: the dir holds `e<attempt>-f<frame>.png.*` with attempt =
//! effect − 1. The real random rolls (scale, rotation, first frame, delay) are read back from the
//! capture, the effect is built with them and stepped frame by frame at 1/60 s, and everything
//! else must agree: sprite frame, opacity, color, scale, rotation, position, blend; every wave's
//! z, color, outline, line width, opacity mod, radius and opacity; the particle system's z,
//! scale, colors, speed, sizes, spread and blend.

use std::path::Path;

use crate::death::{build, ranges, Rolls};
use crate::files::Files;
use crate::particles::ParticleLibrary;

const FRAMES: [u32; 8] = [0, 1, 2, 4, 8, 14, 20, 30];

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|w| w.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
}

fn pair(v: &str) -> (f32, f32) {
    let mut it = v.split(',').map(|s| s.parse().unwrap_or(f32::NAN));
    (it.next().unwrap_or(f32::NAN), it.next().unwrap_or(f32::NAN))
}

#[derive(Debug, Clone)]
struct RealSprite {
    frame: u32,
    pos: (f32, f32),
    scale: f32,
    rotation: f32,
    opacity: f32,
    color: [f32; 3],
    additive: bool,
}

fn real_sprite(scene: &str) -> Option<RealSprite> {
    let line = scene.lines().find(|l| l.contains("frame=playerExplosion_"))?;
    let frame = field(line, "frame")?;
    let n = frame.trim_end_matches(".png").rsplit('_').next()?.parse().ok()?;
    let col: Vec<f32> = field(line, "col")?.split(',').map(|v| v.parse::<f32>().unwrap_or(0.0) / 255.0).collect();
    Some(RealSprite {
        frame: n,
        pos: pair(field(line, "pos")?),
        scale: pair(field(line, "scale")?).0,
        rotation: field(line, "rot")?.parse().ok()?,
        opacity: field(line, "op")?.parse().ok()?,
        color: [col[0], col[1], col[2]],
        additive: field(line, "blend")? == "770,1",
    })
}

#[derive(Debug, Clone)]
struct RealWave {
    z: i32,
    radius: f32,
    opacity: f32,
    color: [u8; 3],
    outline: bool,
    width: i32,
    opmod: f32,
    center: (f32, f32),
}

fn real_waves(text: &str) -> Vec<RealWave> {
    text.lines()
        .filter_map(|l| {
            let c: Vec<u8> = field(l, "color")?.split(',').filter_map(|v| v.parse().ok()).collect();
            Some(RealWave {
                z: field(l, "z")?.parse().ok()?,
                radius: field(l, "radius")?.parse().ok()?,
                opacity: field(l, "opacity")?.parse().ok()?,
                color: [c[0], c[1], c[2]],
                outline: field(l, "outline")? != "0",
                width: field(l, "width")?.parse().ok()?,
                opmod: field(l, "opmod")?.parse().ok()?,
                center: pair(field(l, "center")?),
            })
        })
        .collect()
}

/// One `particles` dump row's settings.
#[derive(Debug, Clone)]
struct RealParticles {
    at: (f32, f32),
    z: i32,
    angle_var: f32,
    pos_var: (f32, f32),
    start_size: f32,
    end_size: f32,
    colors: [[f32; 4]; 4],
    scale: f32,
    additive: bool,
    speed: f32,
}

fn real_particles(text: &str, at: (f32, f32)) -> Vec<RealParticles> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            if f.len() < 29 {
                return None;
            }
            let p = |i: usize| f[i].parse::<f32>().unwrap_or(f32::NAN);
            let slash = |i: usize| pair(&f[i].replace('/', ","));
            let color = |i: usize| -> [f32; 4] {
                let v: Vec<f32> = f[i].split('/').map(|s| s.parse().unwrap_or(f32::NAN)).collect();
                [v[0], v[1], v[2], v[3]]
            };
            let speed = f[28..].join(",").split_whitespace().find_map(|w| w.strip_prefix("speed=").map(|v| pair(&v.replace('/', ",")).0)).unwrap_or(f32::NAN);
            let r = RealParticles {
                at: (p(1), p(2)),
                z: f[4].parse().ok()?,
                angle_var: p(13),
                pos_var: slash(14),
                start_size: slash(15).0,
                end_size: slash(16).0,
                colors: [color(17), color(18), color(19), color(20)],
                scale: p(22),
                additive: f[26] == "770" && f[27] == "1",
                speed,
            };
            ((r.at.0 - at.0).abs() < 0.5 && (r.at.1 - at.1).abs() < 0.5 && f[6] == "2").then_some(r)
        })
        .collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// Check every captured effect; returns a report (one line per effect, problems indented).
pub fn check_real(files: &dyn Files, dir: &Path) -> String {
    let lib = ParticleLibrary::load(files);
    let defs = |name: &str| lib.get(name).cloned();
    let read = |a: u32, f: u32, ext: &str| std::fs::read_to_string(dir.join(format!("e{a}-f{f}.png.{ext}"))).ok();
    let size = 0.9f32;
    // the harness's icon colors (`playericon 1 0 3`): 0 = 125,255,0 and 3 = 0,255,255
    let colors = ([125.0 / 255.0, 1.0, 0.0], [0.0, 1.0, 1.0]);
    let mut report = String::new();
    let mut all_ok = 0;
    for effect in 1..=20u32 {
        let a = effect - 1;
        let Some(waves1) = read(a, 1, "waves") else {
            report += &format!("effect {effect}: no capture\n");
            continue;
        };
        let w1 = real_waves(&waves1);
        let Some(at) = w1.first().map(|w| w.center) else {
            report += &format!("effect {effect}: no waves at frame 1\n");
            continue;
        };
        let mut problems: Vec<String> = Vec::new();
        let sprites: Vec<(u32, Option<RealSprite>)> = FRAMES.iter().map(|&f| (f, read(a, f, "scene").and_then(|s| real_sprite(&s)))).collect();
        // the rolls the real effect drew
        let rolls = match ranges(effect) {
            None => None,
            Some(r) => {
                let Some(s0) = sprites[0].1.clone() else {
                    report += &format!("effect {effect}: no sprite at frame 0\n");
                    continue;
                };
                let k = s0.scale / size;
                if !(r.scale.0 - 1e-3..=r.scale.1 + 1e-3).contains(&k) {
                    problems.push(format!("sprite scale ÷ size {k} outside {:?}", r.scale));
                }
                if !(r.rotation.0 - 1e-3..=r.rotation.1 + 1e-3).contains(&s0.rotation) || (r.quarter && s0.rotation % 90.0 != 0.0) {
                    problems.push(format!("rotation {} outside {:?}", s0.rotation, r.rotation));
                }
                // first frame and delay: every value consistent with all the captured frames
                let mut found: Vec<(u32, f32)> = Vec::new();
                for &start in &r.starts {
                    for i in 0..=4000 {
                        let d = r.delay.0 + (r.delay.1 - r.delay.0) * i as f32 / 4000.0;
                        let ok = sprites.iter().all(|(f, s)| {
                            if *f == 0 {
                                return s.as_ref().is_some_and(|s| s.frame == r.first);
                            }
                            let t = (*f - 1) as f32 * (1.0 / 60.0);
                            let alive = t < (r.last + 1 - start) as f32 * d;
                            match s {
                                Some(s) => alive && s.frame == (start + (t / d) as u32).min(r.last),
                                None => !alive,
                            }
                        });
                        if ok {
                            found.push((start, d));
                        }
                    }
                }
                if found.is_empty() {
                    let seen: Vec<String> = sprites.iter().map(|(f, s)| format!("f{f}:{}", s.as_ref().map_or("-".into(), |s| s.frame.to_string()))).collect();
                    problems.push(format!("no first frame / delay in {:?} {:?} fits the frames {}", r.starts, r.delay, seen.join(" ")));
                }
                // of those, the delay the fade and the waves fit best
                let (start, delay) = found
                    .iter()
                    .copied()
                    .min_by(|x, y| {
                        let err = |&(start, d): &(u32, f32)| {
                            let rolls = Rolls { scale: k, rotation: s0.rotation, start, delay: d };
                            let death = build(effect, at, size, colors, Some(rolls), 1, &defs).unwrap();
                            let mut sp = death.sprite.unwrap();
                            let mut e = 0.0f32;
                            let mut frame = 0;
                            for (f, s) in &sprites {
                                while frame < *f {
                                    sp.update(1.0 / 60.0);
                                    frame += 1;
                                }
                                if let Some(s) = s {
                                    e += (sp.color()[3] * 255.0 - s.opacity).abs();
                                }
                            }
                            // and the waves whose durations follow the delay
                            let mut waves = death.waves.clone();
                            let mut frame = 0;
                            for &f in &FRAMES {
                                while frame < f {
                                    for w in &mut waves {
                                        if w.first_tick {
                                            w.first_tick = false;
                                        } else {
                                            w.elapsed += 1.0 / 60.0;
                                        }
                                    }
                                    frame += 1;
                                }
                                for rw in read(a, f, "waves").map(|t| real_waves(&t)).unwrap_or_default() {
                                    let color = rw.color.map(|v| v as f32 / 255.0);
                                    if let Some(w) = waves.iter().find(|w| w.elapsed < w.duration && w.z == rw.z && w.color.iter().zip(color).all(|(a, b)| close(*a, b, 0.003))) {
                                        e += (w.state().0 - rw.radius).abs();
                                    }
                                }
                            }
                            e
                        };
                        err(x).total_cmp(&err(y))
                    })
                    .unwrap_or((r.starts[0], (r.delay.0 + r.delay.1) * 0.5));
                Some(Rolls { scale: k, rotation: s0.rotation, start, delay })
            }
        };
        let Some(death) = build(effect, at, size, colors, rolls, 1, &defs) else {
            report += &format!("effect {effect}: can't build\n");
            continue;
        };
        // the sprite, frame by frame
        if let Some(mut sp) = death.sprite.clone() {
            let mut frame = 0;
            for (f, real) in &sprites {
                while frame < *f {
                    sp.update(1.0 / 60.0);
                    frame += 1;
                }
                let Some(real) = real else { continue };
                let ours = sp.frame();
                let [r, g, b, op] = sp.color();
                if !ours.ends_with(&format!("_{:03}.png", real.frame)) {
                    problems.push(format!("f{f}: frame {ours}, real {}", real.frame));
                }
                if !close(op * 255.0, real.opacity, 2.0) {
                    problems.push(format!("f{f}: opacity {}, real {}", op * 255.0, real.opacity));
                }
                if !(close(r, real.color[0], 0.01) && close(g, real.color[1], 0.01) && close(b, real.color[2], 0.01)) {
                    problems.push(format!("f{f}: color {:?}, real {:?}", [r, g, b], real.color));
                }
                if !close(sp.pos.0, real.pos.0, 0.01) || !close(sp.pos.1, real.pos.1, 0.01) {
                    problems.push(format!("f{f}: position {:?}, real {:?}", sp.pos, real.pos));
                }
                if sp.additive != real.additive {
                    problems.push(format!("f{f}: additive {}, real {}", sp.additive, real.additive));
                }
            }
        } else if sprites.iter().any(|(_, s)| s.is_some()) {
            problems.push("the real effect has a sprite".into());
        }
        // the waves, frame by frame
        let mut waves = death.waves.clone();
        let mut frame = 0;
        for &f in &FRAMES {
            while frame < f {
                for w in &mut waves {
                    if w.first_tick {
                        w.first_tick = false;
                    } else {
                        w.elapsed += 1.0 / 60.0;
                    }
                }
                frame += 1;
            }
            let Some(text) = read(a, f, "waves") else { continue };
            let real = real_waves(&text);
            let live: Vec<_> = waves.iter().filter(|w| w.elapsed < w.duration).collect();
            if real.len() != live.len() {
                problems.push(format!("f{f}: {} waves, real {}", live.len(), real.len()));
                continue;
            }
            for rw in &real {
                let color = rw.color.map(|v| v as f32 / 255.0);
                let Some(w) = live.iter().find(|w| w.z == rw.z && w.color.iter().zip(color).all(|(a, b)| close(*a, b, 0.003))) else {
                    let ours: Vec<String> = live.iter().map(|w| format!("z{} {:?}", w.z, w.color.map(|v| (v * 255.0).round()))).collect();
                    problems.push(format!("f{f}: no wave z {} color {:?} (ours: {})", rw.z, rw.color, ours.join(", ")));
                    continue;
                };
                let (radius, alpha) = w.state();
                if w.outline != rw.outline || (w.outline && w.line_width as i32 != rw.width) || !close(w.opacity_mod, rw.opmod, 0.002) {
                    problems.push(format!("f{f}: wave z {}: outline {} width {} opmod {}, real {} {} {}", rw.z, w.outline, w.line_width, w.opacity_mod, rw.outline, rw.width, rw.opmod));
                }
                if !close(radius, rw.radius, 0.05 + rw.radius * 0.002) {
                    problems.push(format!("f{f}: wave z {} radius {radius}, real {}", rw.z, rw.radius));
                }
                let real_alpha = ((rw.opacity * rw.opmod).clamp(0.0, 255.0) as u8) as f32;
                if !close(alpha * 255.0, real_alpha, 2.5) {
                    problems.push(format!("f{f}: wave z {} alpha {}, real {}", rw.z, alpha * 255.0, real_alpha));
                }
            }
        }
        // the particle system's settings (frame 1)
        let e = &death.emitter;
        match read(a, 1, "particles").map(|t| real_particles(&t, at)).unwrap_or_default().into_iter().find(|p| p.z == e.z) {
            None => problems.push(format!("no real particle system at z {}", e.z)),
            Some(p) => {
                let d = &e.def;
                let ours = [d.start_color, d.start_color_var, d.end_color, d.end_color_var];
                for (i, name) in ["start color", "start color var", "end color", "end color var"].iter().enumerate() {
                    if !ours[i].iter().zip(p.colors[i]).all(|(a, b)| close(*a, b, 0.001)) {
                        problems.push(format!("particles {name} {:?}, real {:?}", ours[i], p.colors[i]));
                    }
                }
                let checks = [
                    ("scale", e.scale.0, p.scale, 0.001),
                    ("speed", d.speed, p.speed, 0.01),
                    ("start size", d.start_size, p.start_size, 0.01),
                    ("end size", d.end_size, p.end_size, 0.01),
                    ("pos var x", d.pos_var.0, p.pos_var.0, 0.01),
                    ("pos var y", d.pos_var.1, p.pos_var.1, 0.01),
                    ("angle var", d.angle_var, p.angle_var, 0.01),
                ];
                for (name, ours, real, tol) in checks {
                    if !close(ours, real, tol) {
                        problems.push(format!("particles {name} {ours}, real {real}"));
                    }
                }
                if d.additive != p.additive {
                    problems.push(format!("particles additive {}, real {}", d.additive, p.additive));
                }
            }
        }
        if problems.is_empty() {
            all_ok += 1;
            let r = rolls.map_or(String::new(), |r| format!(" (rolls: scale {:.3}, rotation {:.1}, frame {}, delay {:.4})", r.scale, r.rotation, r.start, r.delay));
            report += &format!("effect {effect}: ok{r}\n");
        } else {
            report += &format!("effect {effect}: {} differences\n", problems.len());
            for p in problems {
                report += &format!("    {p}\n");
            }
        }
    }
    report += &format!("{all_ok}/20 effects match\n");
    report
}

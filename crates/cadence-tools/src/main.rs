use std::collections::BTreeMap;
use std::process::ExitCode;

use cadence_core::codec::decode_level_bytes;
use cadence_core::level::LevelString;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["level", path] => level_summary(path),
        ["level-raw", path] => level_raw(path),
        ["save", path] => save_summary(path),
        // the game's save as XML, and back (for the harness's own copy only)
        ["save-xml", path] => (|| -> Result<(), Box<dyn std::error::Error>> {
            print!("{}", cadence_core::save::decrypt_mac(&std::fs::read(path)?)?);
            Ok(())
        })(),
        ["save-write", xml, path] => (|| -> Result<(), Box<dyn std::error::Error>> {
            std::fs::write(path, cadence_core::save::encrypt_mac(&std::fs::read_to_string(xml)?))?;
            Ok(())
        })(),
        ["replay", spec, out] => replay(spec, out),
        ["watch", spec] => watch(spec),
        ["verify", spec] => verify(spec),
        ["route", level, count, out_pattern] => route(level, count, out_pattern),
        ["proute", level, waypoints, out_pattern] => route_platformer(level, waypoints, out_pattern),
        ["features", level] => features(level),
        ["rects", level] => rects(level),
        ["diff", expected, actual] => diff(expected, actual),
        _ => {
            eprintln!("usage: cadence level <file> | level-raw <file> | save <file> | replay <spec> <out.csv> | diff <real.csv> <ours.csv> | verify <spec> | route <level> <count> <out%d.csv> | proute <level> <waypoints> <out%d.csv> | features <level> | rects <level>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_level(path: &str) -> Result<(String, LevelString), Box<dyn std::error::Error>> {
    let text = decode_level_bytes(&std::fs::read(path)?)?;
    let level = LevelString::parse(&text);
    Ok((text, level))
}

fn level_summary(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (text, level) = load_level(path)?;
    let max_x = level.objects.iter().map(|o| o.x()).fold(0.0f32, f32::max);
    let mut ids: BTreeMap<i32, usize> = BTreeMap::new();
    for obj in &level.objects {
        *ids.entry(obj.id()).or_default() += 1;
    }
    let mut top: Vec<_> = ids.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));

    println!("{path}");
    println!("  decoded bytes: {}", text.len());
    println!("  objects:       {} ({} distinct ids)", level.objects.len(), ids.len());
    println!("  last x:        {max_x}");
    println!("  round-trip:    {}", if level.to_string() == text { "exact" } else { "DIFFERS" });
    let keys: Vec<&str> = level.settings.iter().map(|(k, _)| k.as_str()).collect();
    println!("  settings:      {}", keys.join(" "));
    let top: Vec<String> = top.iter().take(12).map(|(id, n)| format!("{id}x{n}")).collect();
    println!("  top ids:       {}", top.join(" "));
    Ok(())
}

fn level_raw(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    print!("{}", load_level(path)?.0);
    Ok(())
}

fn save_summary(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::save;
    let xml = save::decode_any(&std::fs::read(path)?)?;
    let dict = save::parse_plist(&xml)?;
    let rewritten = save::write_plist(&dict);
    println!("{path}");
    println!("  xml bytes:  {}", xml.len());
    println!("  top keys:   {}", dict.len());
    println!("  round-trip: {}", if rewritten == xml { "exact" } else { "DIFFERS" });
    let keys: Vec<&str> = dict.iter().map(|(k, _)| k.as_str()).take(20).collect();
    println!("  keys:       {}", keys.join(" "));
    Ok(())
}

/// Run one attempt through Cadence's physics; returns the trace in the harness CSV format.
/// One trace line (the harness's columns, the ones Cadence models).
fn trace_line(step: u32, p: &cadence_core::physics::Player, cam: (f32, f32)) -> String {
    use cadence_core::physics::{Mode, STEPS_PER_SECOND};
    use cadence_core::trace::{self, FLAG_DEAD, FLAG_ON_GROUND};
    let dt = 1.0 / STEPS_PER_SECOND as f32;
    let flags = (p.on_ground as u32 * FLAG_ON_GROUND)
        | (p.upside_down as u32 * trace::FLAG_UPSIDE_DOWN)
        | (p.dead as u32 * FLAG_DEAD)
        | (p.mode == Mode::Ship) as u32 * trace::FLAG_SHIP
        | (p.mode == Mode::Ball) as u32 * trace::FLAG_BALL
        | (p.mode == Mode::Ufo) as u32 * trace::FLAG_UFO
        | (p.mode == Mode::Wave) as u32 * trace::FLAG_WAVE
        | (p.mode == Mode::Robot) as u32 * trace::FLAG_ROBOT
        | (p.mode == Mode::Swing) as u32 * trace::FLAG_SWING
        | (p.mode == Mode::Spider) as u32 * trace::FLAG_SPIDER
        | (p.sideways as u32) * trace::FLAG_SIDEWAYS
        | (p.going_left as u32) * trace::FLAG_GOING_LEFT
        | (p.dashing as u32) * trace::FLAG_DASHING
        | (p.platformer.on as u32) * trace::FLAG_PLATFORMER
        | (p.on_slope as u32) * trace::FLAG_ON_SLOPE
        | (p.accelerating as u32) * trace::FLAG_ACCELERATING;
    format!("{step},{},{},{},{},{},{flags},{},{},{},0,0,{},{}\n", trace::fmt_f32(dt), trace::fmt_f32(p.x),
        trace::fmt_f32(p.y), trace::fmt_f64(p.y_velocity), trace::fmt_f32(p.rotation),
        trace::fmt_f32(p.speed), trace::fmt_f32(p.size), trace::fmt_f64(p.m_gravity),
        trace::fmt_f32(cam.0), trace::fmt_f32(cam.1))
}

/// Simulate an attempt: player 1's trace, and player 2's for the steps in dual mode.
/// `dash_ends`: for traces recorded before the harness made the cocos scheduler deterministic,
/// the real rotation at each step a dash ended (the icon spin ran on wall-clock time there);
/// it's given to player 1 after that step.
fn simulate_both(
    world: &cadence_core::world::World,
    inputs: &std::collections::BTreeMap<u32, Vec<(bool, u8)>>,
    steps: u32,
    view_w: f32,
    dash_ends: &BTreeMap<u32, f32>,
) -> (String, String) {
    let header = "step,dt,x,y,yvel,rot,flags,speed,size,gravity,fx,fy,cam_x,cam_y\n";
    let mut world = world.clone();
    let world = &mut world;
    world.reset();
    let mut player = spawn_player(world);
    let mut out = format!("# cadence replay\n{header}");
    let mut out2 = format!("# cadence replay player 2\n{header}");
    let debug_ticks = std::env::var_os("CADENCE_DEBUG_TICKS").is_some();
    for step in 0..steps {
        if debug_ticks && world.scheduler_tick_due().is_some() {
            eprintln!("tick {step}");
        }
        let was_dashing = player.dashing;
        player.begin_step(world);
        for &(down, button) in inputs.get(&step).map(|v| v.as_slice()).unwrap_or(&[]) {
            press(&mut player, world, down, button);
        }
        let cam = player.camera;
        world.observe(player.dual.as_ref().map(|p| (p.x, p.y)), (cam.x, cam.y));
        world.pre_step((player.x, player.y));
        player.step(world);
        if was_dashing && !player.dashing {
            if let Some(&r) = dash_ends.get(&step) {
                player.rotation = r;
            }
        }
        world.post_step(&player);
        player.step_camera(world, view_w);
        world.catch_up(&player);
        if std::env::var_os("CADENCE_DEBUG_CORRIDOR").is_some() {
            eprintln!("{step} corridor {:?} y {}", player.corridor, player.y);
        }
        if let Some(at) = std::env::var("CADENCE_DEBUG_NEAR").ok().and_then(|v| v.parse::<u32>().ok()) {
            // Every collidable object within 80 units of the player at that step.
            if step == at {
                for o in &world.objects {
                    let id_match = std::env::var("CADENCE_DEBUG_NEAR_ID").ok().and_then(|v| v.parse::<i32>().ok()) == Some(o.id);
                    if ((o.x - player.x).abs() < 80.0 && (o.y - player.y).abs() < 80.0) || (id_match && (o.x - player.x).abs() < 200.0) {
                        eprintln!("near {step}: order {} id {} kind {:?} pos {},{} rect {:?} rot {} radius {}", o.order, o.id, o.kind, o.x, o.y, o.rect, o.rotation, o.radius);
                    }
                }
                eprintln!("player {} {} rect {:?} rot {}", player.x, player.y, player.rect(), player.rotation);
            }
        }
        if let Ok(ids) = std::env::var("CADENCE_DEBUG_OBJECTS") {
            // Comma-separated level orders: print their f64 positions each step.
            let mut line = format!("{step}");
            for order in ids.split(',').filter_map(|v| v.parse::<u32>().ok()) {
                let (x, y) = world.level_position(order as usize);
                let rot = world.objects.iter().find(|o| o.order == order).map(|o| o.rotation);
                line.push_str(&format!(" {order}:{:.9},{:.9}", x, y));
                if !world.enabled.get(order as usize).copied().unwrap_or(true) {
                    line.push_str(",off");
                }
                if let Some(r) = rot {
                    line.push_str(&format!(",r{r}"));
                }
            }
            eprintln!("{line}");
        }
        out.push_str(&trace_line(step, &player, (cam.x, cam.y)));
        if let Some(p2) = &player.dual {
            out2.push_str(&trace_line(step, p2, (cam.x, cam.y)));
        }
        if player.dead || player.finished || world.level_end {
            break;
        }
    }
    (out, out2)
}

/// One button event (`GJBaseGameLayer::handleButton`): 1 jumps, 2/3 are left/right.
/// Touch triggers (1595) see the jump button first (`GJEffectManager::playerButton`); a
/// dual-mode one keeps presses from the player.
fn press(player: &mut cadence_core::physics::Player, world: &mut cadence_core::world::World, down: bool, button: u8) {
    if button == 1 {
        let dual = world.player_button(down, false);
        if !(dual && down) {
            player.set_button(down, world);
        }
    } else {
        player.set_direction(down, button);
    }
}

/// The world of a spec: its `levelstring` file (a raw or encoded level string) or main level.
fn load_spec_world(spec: &cadence_core::trace::Spec) -> Result<cadence_core::world::World, Box<dyn std::error::Error>> {
    let Some(path) = &spec.level_string else { return load_world(spec.level) };
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if text.trim_start().starts_with("H4sI") || !text.contains(';') { decode_level_bytes(&bytes)? } else { text.into_owned() };
    Ok(world_of(LevelString::parse(&text)))
}

fn load_world(level: i32) -> Result<cadence_core::world::World, Box<dyn std::error::Error>> {
    let level_path = format!("{}/levels/{}.txt", gd_resources(), level);
    let level = LevelString::parse(&decode_level_bytes(&std::fs::read(&level_path)?)?);
    Ok(world_of(level))
}

/// A world for a level, with its camera triggers and settings kept for `spawn_player`.
fn world_of(level: LevelString) -> cadence_core::world::World {
    CAMERA_TRIGGERS.with(|c| *c.borrow_mut() = cadence_core::camera::CameraTriggers::from_level(&level));
    LEVEL_SETTINGS.with(|c| *c.borrow_mut() = Some(level.clone()));
    cadence_core::world::World::from_level(&level)
}

/// `cadence watch <spec>`: simulate the spec's first attempt and write each `watchgroup`'s
/// objects every step next to the harness's file (`<file>.ours`): `step,id:x:y:rot:real=x:y`
/// per object (the level object's f64 position as the game's +0x3b0/+0x3b8, its turned
/// rotation), plus `:op=` opacity-related state is left to screenshots.
fn watch(spec_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let spec = cadence_core::trace::Spec::parse(&std::fs::read_to_string(spec_path)?);
    let mut world = load_spec_world(&spec)?;
    world.sched_every = spec.speed;
    if let Ok(text) = std::fs::read_to_string(spec.out_path(0)) {
        world.sched_phase = cadence_core::trace::sched_index(&text).unwrap_or(0);
    }
    let level = LEVEL_SETTINGS.with(|c| c.borrow().clone()).unwrap_or_default();
    let view_w = std::env::var("CADENCE_VIEW_W").ok().and_then(|v| v.parse().ok()).unwrap_or(569.0);
    let inputs = spec.buttons.first().cloned().unwrap_or_default();
    world.reset();
    let mut player = spawn_player(&world);
    // each watched group's first objects by level order (the game's group array order)
    let groups: Vec<(Vec<usize>, String)> = spec
        .watch
        .iter()
        .map(|(g, path)| {
            let members: Vec<usize> = level.objects.iter().enumerate().filter(|(_, o)| o.groups().contains(g)).map(|(i, _)| i).take(spec.watch_max).collect();
            (members, format!("{path}.ours"))
        })
        .collect();
    let mut outs: Vec<String> = vec![String::new(); groups.len()];
    // enter effects as `PlayLayer::updateVisibility` applies them (per step here): the drawn
    // position, opacity and scale
    let mut locks = vec![cadence_core::enter::EnterLock::default(); level.objects.len()];
    for step in 0..spec.steps {
        player.begin_step(&world);
        for &(down, button) in inputs.get(&step).map(|v| v.as_slice()).unwrap_or(&[]) {
            press(&mut player, &mut world, down, button);
        }
        world.observe(player.dual.as_ref().map(|p| (p.x, p.y)), (player.camera.x, player.camera.y));
        world.pre_step((player.x, player.y));
        player.step(&world);
        world.post_step(&player);
        player.step_camera(&world, view_w);
        for (k, (members, _)) in groups.iter().enumerate() {
            let mut line = format!("{step}");
            for &i in members {
                let (x, y) = world.level_position(i);
                let o = &level.objects[i];
                let rot = o.get_f32(6).unwrap_or(0.0) + world.turned[i];
                let scale = o.get_f32(32).unwrap_or(1.0);
                let sx = o.get_f32(128).unwrap_or(scale) * world.scaled[i].0;
                let sy = o.get_f32(129).unwrap_or(scale) * world.scaled[i].1;
                let eo = cadence_core::enter::EnterObject {
                    x: x as f32,
                    y: y as f32,
                    margin: cadence_core::objects::info(o.canonical_id()).map_or(15.0, |inf| inf.width * 0.5) * sx.abs(),
                    channel: o.get_i32(343).unwrap_or(0).clamp(0, 100),
                    ignore_enter: o.get(67) == Some("1"),
                    ignore_fade: o.get(64) == Some("1"),
                    ui: false,
                    no_scale: false,
                    seed: i as u32,
                };
                let z = if player.camera.zoom > 0.0 { player.camera.zoom } else { 1.0 };
                let view = cadence_core::enter::EnterView { left: player.camera.x, width: view_w / z, cam_y: player.camera.y, win_h: 320.0 };
                let e = world.enter.apply(&eo, &view, &mut locks[i]);
                line.push_str(&format!(
                    ",{}:{}:{}:{}:real={}:{}:op={}:sc={}:{}",
                    o.id(),
                    x as f32 + e.dx,
                    y as f32 + e.dy,
                    rot + e.rotation,
                    x,
                    y,
                    (e.opacity * 255.0).round() as i32,
                    sx * e.scale.0,
                    sy * e.scale.1
                ));
            }
            outs[k].push_str(&line);
            outs[k].push('\n');
        }
        if player.dead || player.finished || world.level_end {
            break;
        }
    }
    for (k, (_, path)) in groups.iter().enumerate() {
        std::fs::write(path, &outs[k])?;
        eprintln!("wrote {path}");
    }
    Ok(())
}

thread_local! {
    /// The camera triggers of the level `load_world` last loaded (given to every new player).
    static CAMERA_TRIGGERS: std::cell::RefCell<cadence_core::camera::CameraTriggers> = Default::default();
    /// The level `load_world` last loaded (for its settings).
    static LEVEL_SETTINGS: std::cell::RefCell<Option<LevelString>> = Default::default();
}

/// A player at the level start, with the level's camera triggers.
fn spawn_player(world: &cadence_core::world::World) -> cadence_core::physics::Player {
    let mut p = cadence_core::physics::Player::spawn(world);
    p.camera_triggers = CAMERA_TRIGGERS.with(|c| c.borrow().clone());
    LEVEL_SETTINGS.with(|c| {
        if let Some(level) = c.borrow().as_ref() {
            p.apply_level_settings(level);
        }
    });
    p
}

/// The window width the route planner assumes for the camera (the harness's default window).
const PLAN_VIEW_W: f32 = 493.0;

/// Replay the first attempt of a spec and write the trace.
fn replay(spec_path: &str, out_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let spec = cadence_core::trace::Spec::parse(&std::fs::read_to_string(spec_path)?);
    let mut world = load_spec_world(&spec)?;
    world.sched_every = spec.speed;
    // The scheduler phase from the real trace when there is one.
    if let Ok(text) = std::fs::read_to_string(spec.out_path(0)) {
        world.sched_phase = cadence_core::trace::sched_index(&text).unwrap_or(0);
    }
    let view_w = std::env::var("CADENCE_VIEW_W").ok().and_then(|v| v.parse().ok()).unwrap_or(569.0);
    // With %d in the path every attempt is written; otherwise just the first.
    for (i, inputs) in spec.buttons.iter().enumerate() {
        let dash_ends = std::fs::read_to_string(spec.out_path(i))
            .map(|t| legacy_dash_ends(&t, &cadence_core::trace::parse_csv(&t)))
            .unwrap_or_default();
        let text = simulate_both(&world, inputs, spec.steps, view_w, &dash_ends).0;
        if !out_path.contains("%d") {
            std::fs::write(out_path, text)?;
            break;
        }
        std::fs::write(out_path.replace("%d", &i.to_string()), text)?;
    }
    Ok(())
}

/// For traces without the harness's `# sched` header (wall-clock cocos actions): the real
/// rotation at each step a dash ended.
fn legacy_dash_ends(text: &str, rows: &[cadence_core::trace::Row]) -> BTreeMap<u32, f32> {
    let mut out = BTreeMap::new();
    if cadence_core::trace::sched_index(text).is_some() {
        return out;
    }
    for w in rows.windows(2) {
        let dash = cadence_core::trace::FLAG_DASHING;
        if w[0].flags & dash != 0 && w[1].flags & dash == 0 {
            out.insert(w[1].step, w[1].rotation);
        }
    }
    out
}

/// Replay every attempt of a spec and compare each with the real game's trace.
fn verify(spec_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::trace::parse_csv;
    let spec = cadence_core::trace::Spec::parse(&std::fs::read_to_string(spec_path)?);
    let mut world = load_spec_world(&spec)?;
    world.sched_every = spec.speed;
    let (mut matched, mut total, mut steps) = (0, 0, 0usize);
    let (mut rot_max, mut rot_sum, mut rot_n) = (0.0f32, 0.0f64, 0usize);
    let mut rot_exact = 0;
    let (mut cam_exact, mut cam_max, mut first_cam) = (0, 0.0f32, None::<String>);
    for (i, inputs) in spec.buttons.iter().enumerate() {
        let Ok(real_text) = std::fs::read_to_string(spec.out_path(i)) else { continue };
        total += 1;
        let real = parse_csv(&real_text);
        let view_w = cadence_core::trace::window_size(&real_text).map_or(569.0, |(w, _)| w);
        world.sched_phase = cadence_core::trace::sched_index(&real_text).unwrap_or(0);
        let mut p2_bad = false;
        let dash_ends = legacy_dash_ends(&real_text, &real);
        let (ours_text, ours2_text) = simulate_both(&world, inputs, spec.steps, view_w, &dash_ends);
        let ours = parse_csv(&ours_text);
        // Player 2 (dual mode), compared step by step where the real game recorded it.
        if let Ok(real2_text) = std::fs::read_to_string(format!("{}.p2", spec.out_path(i))) {
            let real2 = parse_csv(&real2_text);
            let ours2_rows = parse_csv(&ours2_text);
            let ours2: std::collections::HashMap<u32, &cadence_core::trace::Row> = ours2_rows.iter().map(|r| (r.step, r)).collect();
            let mut msg = None;
            // The real game keeps writing a dead player 2 until it restarts; stop at its death.
            let end = real2.iter().position(|w| w.flags & cadence_core::trace::FLAG_DEAD != 0).map_or(real2.len(), |i| i + 1);
            for w in &real2[..end] {
                match ours2.get(&w.step) {
                    None => {
                        msg = Some(format!("step {}: real has player 2, ours doesn't", w.step));
                        break;
                    }
                    Some(g) => {
                        if let Some(m) = first_difference(std::slice::from_ref(w), std::slice::from_ref(*g)) {
                            msg = Some(m);
                            break;
                        }
                    }
                }
            }
            if let Some(m) = msg {
                println!("attempt {i} player 2: {m}");
                p2_bad = true;
            }
        }
        let mut cam_ok = true;
        for (w, g) in real.iter().zip(&ours) {
            if let (Some(a), Some(b)) = (w.cam, g.cam) {
                let d = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                cam_max = cam_max.max(d);
                if a.0.to_bits() != b.0.to_bits() || a.1.to_bits() != b.1.to_bits() {
                    if cam_ok && first_cam.is_none() {
                        first_cam = Some(format!("attempt {i} step {}: real {:?} ours {:?}", w.step, a, b));
                    }
                    cam_ok = false;
                }
            }
        }
        if cam_ok {
            cam_exact += 1;
        }
        steps += real.len();
        if real.iter().zip(&ours).all(|(w, g)| w.rotation.to_bits() == g.rotation.to_bits()) {
            rot_exact += 1;
        }
        let mut first_rot = true;
        for (w, g) in real.iter().zip(&ours) {
            let d = (w.rotation - g.rotation + 180.0).rem_euclid(360.0) - 180.0;
            if std::env::var_os("CADENCE_ROT_LOG").is_some() && d.abs() > 5.0 {
                println!("rot step {} real {} ours {} flags {:#x}", w.step, w.rotation, g.rotation, w.flags);
            }
            if first_rot && d.abs() > 0.5 {
                first_rot = false;
                println!("attempt {i} rotation first off at step {}: real {} ours {} (x {} flags {:#x})", w.step, w.rotation, g.rotation, w.x, w.flags);
            }
            rot_max = rot_max.max(d.abs());
            rot_sum += d.abs() as f64;
            rot_n += 1;
        }
        match first_difference(&real, &ours) {
            None if !p2_bad => matched += 1,
            None => {}
            Some(msg) => println!("attempt {i}: {msg}"),
        }
    }
    println!("{matched}/{total} attempts match exactly ({steps} real steps checked)");
    println!(
        "rotation (visual): {rot_exact}/{total} bit-exact, mean error {:.4}°, max {:.3}°",
        rot_sum / rot_n.max(1) as f64,
        rot_max
    );
    println!("camera: {cam_exact}/{total} bit-exact, max error {cam_max:.4}{}", first_cam.map_or(String::new(), |f| format!(" (first: {f})")));
    if matched == total { Ok(()) } else { Err("some attempts differ".into()) }
}

fn first_difference(want: &[cadence_core::trace::Row], got: &[cadence_core::trace::Row]) -> Option<String> {
    for (w, g) in want.iter().zip(got) {
        let mut diffs = Vec::new();
        if w.x != g.x { diffs.push(format!("x {} vs {}", w.x, g.x)); }
        if w.y != g.y { diffs.push(format!("y {} vs {}", w.y, g.y)); }
        if w.y_velocity != g.y_velocity { diffs.push(format!("yvel {} vs {}", w.y_velocity, g.y_velocity)); }
        if w.flags != g.flags { diffs.push(format!("flags {} vs {}", w.flags, g.flags)); }
        if !diffs.is_empty() {
            return Some(format!("step {} (real x {}): {}", w.step, w.x, diffs.join("; ")));
        }
    }
    // Ours stops at death or the level end (the game then animates the player into the end
    // portal, which isn't physics); a real trace that continues past our end is fine.
    (want.len() < got.len()).then(|| format!("length real {} vs ours {}", want.len(), got.len()))
}

/// Compare two traces; report the first step where position, velocity or flags differ.
fn diff(expected: &str, actual: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::trace::parse_csv;
    let want = parse_csv(&std::fs::read_to_string(expected)?);
    let got = parse_csv(&std::fs::read_to_string(actual)?);
    match first_difference(&want, &got) {
        None => {
            println!("MATCH: all {} steps identical", want.len());
            Ok(())
        }
        Some(msg) => {
            println!("FIRST DIFFERENCE at {msg}");
            Err("traces differ".into())
        }
    }
}

/// The game's Resources folder: `$CADENCE_GD_RESOURCES`, else the default Steam path on macOS.
fn gd_resources() -> String {
    std::env::var("CADENCE_GD_RESOURCES").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Library/Application Support/Steam/steamapps/common/Geometry Dash/Geometry Dash.app/Contents/Resources")
    })
}

/// Splits world `wi` among its routes by key: the first key keeps the world, every other key
/// gets a copy (at most 32 worlds; routes that find no free copy die rather than run in the
/// wrong world). Returns (world, route indices) per key.
fn split_world<C, K: PartialEq + Copy>(
    worlds: &mut Vec<cadence_core::world::World>,
    cands: &mut [C],
    wi: usize,
    keyed: Vec<(usize, K)>,
    world_of: impl Fn(&mut C) -> &mut usize,
    alive_of: impl Fn(&mut C) -> &mut bool,
) -> Vec<(usize, Vec<usize>)> {
    let mut keys: Vec<K> = Vec::new();
    for &(_, k) in &keyed {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let mut out = Vec::new();
    for (ki, &key) in keys.iter().enumerate() {
        let group: Vec<usize> = keyed.iter().filter(|m| m.1 == key).map(|m| m.0).collect();
        let target = if ki == 0 {
            wi
        } else if worlds.len() < 32 {
            worlds.push(worlds[wi].clone());
            worlds.len() - 1
        } else {
            for &i in &group {
                *alive_of(&mut cands[i]) = false;
            }
            continue;
        };
        for &i in &group {
            *world_of(&mut cands[i]) = target;
        }
        out.push((target, group));
    }
    out
}

/// Find input routes that survive as far as possible in Cadence's simulation, and print a harness
/// spec with one attempt per route. Beam search over hold/release every 4 steps, with a seeded
/// shuffle so different routes explore different choices.
fn route(level: &str, count: &str, out_pattern: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::physics::Player;
    let level: i32 = level.parse()?;
    let count: usize = count.parse()?;
    let mut base_world = load_world(level)?;
    // The planned spec runs at speed 8 (8 game updates per rendered frame).
    base_world.sched_every = 8;
    base_world.sched_phase = 2; // the harness's first recorded update is the 3rd of its frame
    const DECISION: u32 = 4;
    let beam_width: usize = std::env::var("CADENCE_BEAM").ok().and_then(|v| v.parse().ok()).unwrap_or(512);
    // Height buckets per unit when deduplicating the beam (coarser keeps more varied heights).
    let bucket: f32 = std::env::var("CADENCE_BUCKET").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
    const MAX_STEPS: u32 = 40_000;
    // CADENCE_MAX_Y: prune routes that climb above this y (outside rotated gameplay), to steer
    // the search along the intended path.
    let max_y: f32 = std::env::var("CADENCE_MAX_Y").ok().and_then(|v| v.parse().ok()).unwrap_or(f32::MAX);
    // CADENCE_MAX_Y_FROM=<x>: apply CADENCE_MAX_Y only from that x on.
    let max_y_from: f32 = std::env::var("CADENCE_MAX_Y_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(f32::MIN);
    // CADENCE_REQUIRE=<mode>:<x>: from that x on (not sideways), only routes in that mode
    // (cube, ship, ball, ufo, wave, robot, spider, swing) live — to steer through a portal.
    let require: Option<(String, f32)> = std::env::var("CADENCE_REQUIRE").ok().and_then(|v| {
        let (m, x) = v.split_once(':')?;
        Some((m.to_lowercase(), x.parse().ok()?))
    });

    println!("level {level}\nsteps {MAX_STEPS}\nspeed 8\nout {out_pattern}");
    for seed in 0..count as u64 {
        // Triggers act on the world by the player's x, the same for every route (barring
        // different speed portals), so routes share a world; touch-triggered triggers depend on
        // where each player is, so routes that touch different ones get their own copy.
        let mut worlds = vec![base_world.clone()];
        // state: (world, player, inputs, holding)
        // (world, player, inputs, holding, stall anchor: where and when it last moved 60 units)
        let mut beam: Vec<(usize, Player, Vec<(u32, bool)>, bool, (f32, f32, u32))> = vec![(0, spawn_player(&worlds[0]), Vec::new(), false, (0.0, 0.0, 0))];
        // Progress: steps survived (the forward axis turns with the gameplay), then forward
        // position.
        let mut best: ((u32, f32), Vec<(u32, bool)>, (f32, f32, bool)) = ((0, 0.0), Vec::new(), (0.0, 0.0, false));
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut step = 0;
        // CADENCE_ROUTE_FROM=<spec>:<step>: start from the first attempt of an existing route,
        // replayed up to that step (a decision boundary), to search a hard part only.
        if let Some((path, until)) = std::env::var("CADENCE_ROUTE_FROM").ok().as_deref().and_then(|v| v.rsplit_once(':')) {
            let spec = cadence_core::trace::Spec::parse(&std::fs::read_to_string(path)?);
            let until: u32 = until.parse::<u32>()? / DECISION * DECISION;
            let inputs = spec.attempts.into_iter().next().unwrap_or_default();
            let (w, p, ins, holding, anchor) = &mut beam[0];
            let world = &mut worlds[*w];
            world.sched_every = spec.speed;
            for s in 0..until {
                p.begin_step(world);
                if let Some(&down) = inputs.get(&s) {
                    p.set_button(down, world);
                    ins.push((s, down));
                    *holding = down;
                }
                world.pre_step((p.x, p.y));
                p.step(world);
                world.post_step(&p);
                p.step_camera(world, PLAN_VIEW_W);
                world.catch_up(&p);
                if p.dead {
                    return Err(format!("the route to start from dies at step {s} (x {})", p.x).into());
                }
            }
            step = until;
            *anchor = (p.x, p.y, step);
        }
        while step < MAX_STEPS && !beam.is_empty() {
            // Every candidate (each route held or released) steps in lockstep with its world.
            struct Cand {
                world: usize,
                player: Player,
                inputs: Vec<(u32, bool)>,
                hold: bool,
                /// The input to apply at this decision's first step (after the scheduler tick).
                press: Option<bool>,
                alive: bool,
                lived: u32,
                anchor: (f32, f32, u32),
            }
            let mut cands: Vec<Cand> = Vec::with_capacity(beam.len() * 2);
            for (w, player, inputs, holding, anchor) in &beam {
                for hold in [false, true] {
                    let p = player.clone();
                    let mut ins = inputs.clone();
                    let press = (hold != *holding).then_some(hold);
                    if press.is_some() {
                        ins.push((step, hold));
                    }
                    cands.push(Cand { world: *w, player: p, inputs: ins, hold, press, alive: true, lived: step, anchor: *anchor });
                }
            }
            for _ in 0..DECISION {
                for c in cands.iter_mut().filter(|c| c.alive) {
                    c.player.begin_step(&worlds[c.world]);
                    if let Some(h) = c.press.take() {
                        c.player.set_button(h, &worlds[c.world]);
                    }
                }
                for (wi, world) in worlds.iter_mut().enumerate() {
                    if let Some(c) = cands.iter().find(|c| c.alive && c.world == wi) {
                        world.pre_step((c.player.x, c.player.y));
                    }
                }
                for c in cands.iter_mut().filter(|c| c.alive) {
                    c.player.step(&worlds[c.world]);
                    // A route that stays within 60 units for 10 s is stuck: it counts as dead
                    // from when it stopped moving.
                    let (ax, ay, at) = c.anchor;
                    if (c.player.x - ax).abs() > 60.0 || (c.player.y - ay).abs() > 60.0 {
                        c.anchor = (c.player.x, c.player.y, c.lived);
                    }
                    if c.lived - c.anchor.2 > 2400 {
                        c.alive = false;
                        c.lived = at;
                    } else if c.player.dead
                        || (!c.player.sideways && c.player.x >= max_y_from && c.player.y > max_y)
                        || require.as_ref().is_some_and(|(m, x)| {
                            !c.player.sideways && c.player.x >= *x && format!("{:?}", c.player.mode).to_lowercase() != *m
                        })
                    {
                        c.alive = false;
                    } else {
                        c.lived += 1;
                    }
                }
                // Routes share a world while it would act the same for them; a route that
                // would fire different triggers (touched, collected, fired by its order walk)
                // gets a copy.
                let n = worlds.len();
                for wi in 0..n {
                    let keyed: Vec<(usize, (u64, u64, u64))> = cands
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.alive && c.world == wi)
                        .map(|(i, c)| (i, worlds[wi].plan_key(&c.player)))
                        .collect();
                    for (target, group) in split_world(&mut worlds, &mut cands, wi, keyed, |c| &mut c.world, |c| &mut c.alive) {
                        let p = cands[group[0]].player.clone();
                        worlds[target].post_step(&p);
                    }
                }
                for c in cands.iter_mut().filter(|c| c.alive) {
                    c.player.step_camera(&worlds[c.world], PLAN_VIEW_W);
                }
            }
            let mut next = Vec::with_capacity(cands.len());
            for c in cands {
                let fwd = if c.player.sideways { c.player.y } else { c.player.x };
                let fwd = if c.player.going_left { -fwd } else { fwd };
                if (c.lived, fwd) > best.0 {
                    best = ((c.lived, fwd), c.inputs.clone(), (c.player.x, c.player.y, c.player.dead));
                }
                if c.alive {
                    next.push((c.world, c.player, c.inputs, c.hold, c.anchor));
                }
            }
            // keep a diverse beam: shuffle, then dedupe by rounded position
            for i in (1..next.len()).rev() {
                rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
                next.swap(i, (rng % (i as u64 + 1)) as usize);
            }
            let mut seen = std::collections::HashSet::new();
            // Hidden state counts too: two players at the same height can differ in which orbs
            // they used, whether the press can still fire an orb, gravity, size or speed.
            next.retain(|(w, p, _, h, _)| {
                let flags = (p.on_ground as u32)
                    | (p.upside_down as u32) << 1
                    | (p.ring_jump_ready as u32) << 2
                    | (p.state_pre_jump as u32) << 3
                    | (p.rising as u32) << 4
                    | (p.touching_ground as u32) << 5
                    | ((p.size != 1.0) as u32) << 6
                    | (p.sideways as u32) << 7
                    | (p.going_left as u32) << 8;
                // The height is along x while sideways.
                let height = if p.sideways { p.x } else { p.y };
                seen.insert((
                    *w,
                    (height * bucket) as i32,
                    (p.y_velocity * 5.0 * bucket as f64) as i32,
                    *h,
                    flags,
                    p.mode as u8,
                    p.used_portals.len() as u32,
                    p.speed.to_bits(),
                ))
            });
            next.truncate(beam_width);
            // Drop worlds no route uses any more.
            let mut used: Vec<usize> = next.iter().map(|b| b.0).collect();
            used.sort_unstable();
            used.dedup();
            if used.len() < worlds.len() {
                let old = std::mem::take(&mut worlds);
                let mut remap = vec![usize::MAX; old.len()];
                for (new_index, (i, w)) in old.into_iter().enumerate().filter(|(i, _)| used.binary_search(i).is_ok()).enumerate() {
                    remap[i] = new_index;
                    worlds.push(w);
                }
                for b in &mut next {
                    b.0 = remap[b.0];
                }
            }
            beam = next;
            step += DECISION;
        }
        println!("attempt");
        for (s, down) in best.1 {
            println!("{} {s} 1", if down { "press" } else { "release" });
        }
        eprintln!("route {seed}: survived {} steps (forward {:.0}; at x {} y {}, dead {})", best.0 .0, best.0 .1, best.2 .0, best.2 .1, best.2 .2);
    }
    Ok(())
}

/// The platformer route planner: a beam search over (left / right / neither) × (jump held or
/// not) every `CADENCE_DECISION` steps (default 4), steered through waypoints. The waypoint file
/// has one `x y [radius]` per line (radius default 30); a route reaches a waypoint when its
/// center comes within the radius; candidates are ranked by waypoints reached, then distance to
/// the next one (deduplicated by position, velocities and state). Ends at the last waypoint or
/// when the level is finished. Prints a harness spec with the best route as one attempt.
/// `CADENCE_ROUTE_FROM=<spec>:<step>` starts from an existing route's first attempt.
fn route_platformer(level: &str, waypoints: &str, out_pattern: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::physics::Player;
    let level: i32 = level.parse()?;
    let wps: Vec<(f32, f32, f32)> = std::fs::read_to_string(waypoints)?
        .lines()
        .filter_map(|l| {
            let l = l.split('#').next()?.trim();
            let f: Vec<f32> = l.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            (f.len() >= 2).then(|| (f[0], f[1], f.get(2).copied().unwrap_or(30.0)))
        })
        .collect();
    let env = |k: &str, d: u32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let decision = env("CADENCE_DECISION", 4);
    let beam_width = env("CADENCE_BEAM", 512) as usize;
    let max_steps = env("CADENCE_MAX_STEPS", 60_000);
    let mut base_world = load_world(level)?;
    base_world.sched_every = 8;
    base_world.sched_phase = 2;

    #[derive(Clone)]
    struct Route {
        world: usize,
        player: Player,
        events: Vec<(u32, bool, u8)>,
        dir: i8,
        jump: bool,
        wp: usize,
        dead: bool,
        trail: Vec<(f32, f32)>,
        /// Where and when it last moved 60 units (a route stuck for 10 s dies).
        anchor: (f32, f32, u32),
    }
    let apply = |p: &mut Player, w: &cadence_core::world::World, ev: &[(u32, bool, u8)]| {
        // (the planner shares worlds between routes: touch triggers aren't run here)
        for &(_, down, b) in ev {
            if b == 1 {
                p.set_button(down, w);
            } else {
                p.set_direction(down, b);
            }
        }
    };
    let mut worlds = vec![base_world.clone()];
    let mut start = Route { world: 0, player: spawn_player(&worlds[0]), events: Vec::new(), dir: 0, jump: false, wp: 0, dead: false, trail: Vec::new(), anchor: (0.0, 0.0, 0) };
    let mut step = 0u32;
    if let Some((path, until)) = std::env::var("CADENCE_ROUTE_FROM").ok().as_deref().and_then(|v| v.rsplit_once(':')) {
        let spec = cadence_core::trace::Spec::parse(&std::fs::read_to_string(path)?);
        let until: u32 = until.parse::<u32>()? / decision * decision;
        let inputs = spec.buttons.into_iter().next().unwrap_or_default();
        let world = &mut worlds[0];
        let p = &mut start.player;
        for s in 0..until {
            p.begin_step(world);
            for &(down, b) in inputs.get(&s).map(|v| v.as_slice()).unwrap_or(&[]) {
                press(p, world, down, b);
                start.events.push((s, down, b));
                match b {
                    1 => start.jump = down,
                    2 => start.dir = if down { -1 } else if start.dir == -1 { 0 } else { start.dir },
                    _ => start.dir = if down { 1 } else if start.dir == 1 { 0 } else { start.dir },
                }
            }
            world.pre_step((p.x, p.y));
            p.step(world);
            world.post_step(p);
            p.step_camera(world, PLAN_VIEW_W);
            if p.dead {
                return Err(format!("the route to start from dies at step {s}").into());
            }
            while start.wp < wps.len() && {
                let (x, y, r) = wps[start.wp];
                (p.x - x).hypot(p.y - y) <= r
            } {
                start.wp += 1;
            }
        }
        step = until;
    }
    let pdebug = std::env::var_os("CADENCE_PDEBUG").is_some();
    let stall = env("CADENCE_STALL", 2400);
    let mut beam = vec![start];
    let mut best = beam[0].clone();
    let score = |r: &Route| -> (usize, f32) {
        let d = wps.get(r.wp).map_or(0.0, |&(x, y, _)| (r.player.x - x).hypot(r.player.y - y));
        (r.wp, -d)
    };
    while step < max_steps && !beam.is_empty() {
        let mut cands: Vec<Route> = Vec::with_capacity(beam.len() * 6);
        for r in &beam {
            for dir in [-1i8, 0, 1] {
                for jump in [false, true] {
                    let mut c = r.clone();
                    let mut ev = Vec::new();
                    if dir != r.dir {
                        if r.dir == -1 {
                            ev.push((step, false, 2));
                        } else if r.dir == 1 {
                            ev.push((step, false, 3));
                        }
                        if dir == -1 {
                            ev.push((step, true, 2));
                        } else if dir == 1 {
                            ev.push((step, true, 3));
                        }
                    }
                    if jump != r.jump {
                        ev.push((step, jump, 1));
                    }
                    c.dir = dir;
                    c.jump = jump;
                    c.events.extend(ev);
                    cands.push(c);
                }
            }
        }
        for k in 0..decision {
            let s = step + k;
            for c in cands.iter_mut().filter(|c| !c.dead) {
                c.player.begin_step(&worlds[c.world]);
                if k == 0 {
                    let ev: Vec<_> = c.events.iter().rev().take_while(|e| e.0 == s).copied().collect::<Vec<_>>().into_iter().rev().collect();
                    apply(&mut c.player, &worlds[c.world], &ev);
                }
            }
            for (wi, world) in worlds.iter_mut().enumerate() {
                if let Some(c) = cands.iter().find(|c| !c.dead && c.world == wi) {
                    world.pre_step((c.player.x, c.player.y));
                }
            }
            for c in cands.iter_mut().filter(|c| !c.dead) {
                c.player.step(&worlds[c.world]);
                if pdebug {
                    c.trail.push((c.player.x, c.player.y));
                }
                if c.player.dead {
                    c.dead = true;
                }
                if (c.player.x - c.anchor.0).abs() > 60.0 || (c.player.y - c.anchor.1).abs() > 60.0 {
                    c.anchor = (c.player.x, c.player.y, s);
                } else if s - c.anchor.2 > stall {
                    c.dead = true;
                }
                while c.wp < wps.len() && {
                    let (x, y, r) = wps[c.wp];
                    (c.player.x - x).hypot(c.player.y - y) <= r
                } {
                    c.wp += 1;
                }
            }
            let n = worlds.len();
            for wi in 0..n {
                let keyed: Vec<(usize, (u64, u64, u64))> = cands
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| !c.dead && c.world == wi)
                    .map(|(i, c)| (i, worlds[wi].plan_key(&c.player)))
                    .collect();
                let mut alive: Vec<bool> = cands.iter().map(|c| !c.dead).collect();
                let mut idx: Vec<usize> = (0..cands.len()).collect();
                let groups = {
                    // split_world works on parallel slices: pair each cand with its alive flag.
                    let mut pairs: Vec<(usize, bool)> = cands.iter().map(|c| (c.world, !c.dead)).collect();
                    let g = split_world(&mut worlds, &mut pairs, wi, keyed, |c| &mut c.0, |c| &mut c.1);
                    for (i, (w, a)) in pairs.into_iter().enumerate() {
                        cands[i].world = w;
                        alive[i] = a;
                    }
                    g
                };
                idx.clear();
                for (i, a) in alive.iter().enumerate() {
                    if !a && !cands[i].dead {
                        cands[i].dead = true;
                    }
                }
                for (target, group) in groups {
                    let p = cands[group[0]].player.clone();
                    worlds[target].post_step(&p);
                }
            }
            for c in cands.iter_mut().filter(|c| !c.dead) {
                c.player.step_camera(&worlds[c.world], PLAN_VIEW_W);
            }
        }
        step += decision;
        let mut next: Vec<Route> = Vec::with_capacity(cands.len());
        let mut done = false;
        for c in cands {
            if c.dead {
                continue;
            }
            if score(&c) > score(&best) || (c.wp == best.wp && best.dead) {
                best = c.clone();
            }
            if c.wp >= wps.len() || c.player.finished || worlds[c.world].level_end {
                best = c.clone();
                done = true;
                break;
            }
            next.push(c);
        }
        if done {
            break;
        }
        next.sort_by(|a, b| score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal));
        let mut seen = std::collections::HashSet::new();
        next.retain(|r| {
            let p = &r.player;
            seen.insert((
                r.world,
                (p.x / 3.0) as i32,
                (p.y / 3.0) as i32,
                (p.platformer.x_velocity * 2.0) as i32,
                (p.y_velocity * 2.0) as i32,
                p.on_ground,
                p.upside_down,
                r.dir,
                r.jump,
                p.mode as u8,
            ))
        });
        // Spread the beam: at most CADENCE_CELL routes per 30×30 cell (best first).
        let cap = env("CADENCE_CELL", 24) as usize;
        let mut cells: std::collections::HashMap<(i32, i32, usize), usize> = std::collections::HashMap::new();
        next.retain(|r| {
            let n = cells.entry(((r.player.x / 15.0).floor() as i32, (r.player.y / 15.0).floor() as i32, r.wp)).or_default();
            *n += 1;
            *n <= cap
        });
        next.truncate(beam_width);
        let mut used: Vec<usize> = next.iter().map(|b| b.world).collect();
        used.sort_unstable();
        used.dedup();
        if used.len() < worlds.len() {
            let old = std::mem::take(&mut worlds);
            let mut remap = vec![usize::MAX; old.len()];
            for (new_index, (i, w)) in old.into_iter().enumerate().filter(|(i, _)| used.binary_search(i).is_ok()).enumerate() {
                remap[i] = new_index;
                worlds.push(w);
            }
            for b in &mut next {
                b.world = remap[b.world];
            }
        }
        if step % 240 == 0 {
            let b = &next.first().unwrap_or(&best);
            eprintln!("step {step}: beam {} worlds {} lead wp {}/{} at ({:.0}, {:.0})", next.len(), worlds.len(), b.wp, wps.len(), b.player.x, b.player.y);
        }
        beam = next;
    }
    if std::env::var_os("CADENCE_PDEBUG").is_some() {
        // Re-simulate the chosen route alone (as replay does) to catch planning/replay drift.
        let mut inputs: BTreeMap<u32, Vec<(bool, u8)>> = BTreeMap::new();
        for &(s, d, b) in &best.events {
            inputs.entry(s).or_default().push((d, b));
        }
        let (text, _) = simulate_both(&base_world, &inputs, step, PLAN_VIEW_W, &BTreeMap::new());
        let last = text.lines().last().unwrap_or("");
        for row in cadence_core::trace::parse_csv(&text) {
            if let Some(&(x, y)) = best.trail.get(row.step as usize) {
                if row.x != cadence_core::trace::fmt_f32(x) || row.y != cadence_core::trace::fmt_f32(y) {
                    eprintln!("drift at step {}: planned {x},{y} replay {},{}", row.step, row.x, row.y);
                    break;
                }
            }
        }
        eprintln!("re-simulated: {} | planned at x {} y {} dead {}", last, best.player.x, best.player.y, best.player.dead);
    }
    let last = best.events.last().map_or(0, |e| e.0);
    println!("level {level}\nsteps {}\nspeed 8\nout {out_pattern}\nattempt", (step + 240).max(last + 240));
    for (s, down, b) in &best.events {
        println!("{} {s} {b}", if *down { "press" } else { "release" });
    }
    eprintln!("route: waypoint {}/{} at x {} y {} step {step} finished {}", best.wp, wps.len(), best.player.x, best.player.y, best.player.finished);
    Ok(())
}

/// Print every collidable object's collision shape (level order, id, kind, position, rect,
/// radius), to compare with the harness's `dumplevel`.
fn rects(level: &str) -> Result<(), Box<dyn std::error::Error>> {
    let world = load_world(level.parse()?)?;
    let mut objects: Vec<_> = world.objects.iter().collect();
    objects.sort_by_key(|o| o.order);
    println!("order,id,type,x,y,rect_x,rect_y,rect_w,rect_h,radius");
    for o in objects {
        println!("{},{},{},{},{},{},{},{},{},{}", o.order, o.id, o.kind.0, o.x, o.y, o.rect.x, o.rect.y, o.rect.w, o.rect.h, o.radius);
    }
    Ok(())
}

/// List what a level uses that the physics doesn't handle yet: object types (with example ids),
/// trigger ids, and objects whose hitbox needs scaling.
fn features(level: &str) -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::objects::{self, ObjectType};
    use std::collections::BTreeMap;
    // a main level's id, or a level file (a downloaded level or a raw level string)
    let level_path = if std::path::Path::new(level).is_file() { level.to_string() } else { format!("{}/levels/{}.txt", gd_resources(), level) };
    let bytes = std::fs::read(&level_path)?;
    let text = String::from_utf8_lossy(&bytes);
    let level = if text.contains(";1,") && !text.trim_start().starts_with("H4sI") { LevelString::parse(&text) } else { LevelString::parse(&decode_level_bytes(&bytes)?) };
    let handled_kinds = [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 32, 34, 35, 37, 38, 47];
    // every trigger the world, the camera or the order walk runs (groups.rs, camera.rs,
    // order.rs), and the editor-only / inert ones (1931 end and 3655 do nothing in play, 2016
    // camera guide and 3642 BPM are editor guides)
    let mut handled_triggers = vec![
        29, 30, 31, 104, 105, 200, 201, 202, 203, 221, 717, 718, 743, 744, 899, 900, 901, 915, 1049, 1268, 1334,
        1346, 1347, 1611, 1616, 1913, 1914, 1916, 1935, 2899, 2900, 2902, 3600,
        1595, 1612, 1613, 1811, 1812, 1814, 1815, 1816, 1817, 1818, 1819, 1912, 1917, 1931, 1932, 1934, 2015, 2016,
        2062, 2063, 2066, 2067, 2068, 2901, 2925, 3016, 3022, 3023, 3024, 3032, 3033, 3602, 3603, 3604, 3605, 3606,
        3607, 3608, 3609, 3612, 3613, 3614, 3615, 3617, 3618, 3619, 3620, 3640, 3641, 3642, 3655, 3660, 3661, 3662,
        3663,
    ];
    handled_triggers.extend(3006..=3010);
    handled_triggers.extend(3017..=3021);
    handled_triggers.extend(3029..=3031);
    // Triggers that only change how things look or sound (enter effects 22–28 and 55–59, trail
    // 32/33, pulse 1006, alpha 1007, gradient 2903, shaders 2904–2924, middle ground 2999,
    // background/ground/middle-ground swaps 3029–3031, sound effects 3602/3603): listed apart,
    // they don't affect the physics.
    let mut visual_triggers = vec![22, 23, 24, 25, 26, 27, 28, 32, 33, 55, 56, 57, 58, 59, 1006, 1007, 1520, 1585, 1915];
    visual_triggers.extend(2903..=2924);
    visual_triggers.extend([2999, 3029, 3030, 3031, 3602, 3603]);
    let mut kinds: BTreeMap<i32, (usize, BTreeMap<i32, usize>)> = BTreeMap::new();
    let mut triggers: BTreeMap<i32, usize> = BTreeMap::new();
    let mut visual: BTreeMap<i32, usize> = BTreeMap::new();
    let (mut scaled, mut unknown) = (0, BTreeMap::<i32, usize>::new());
    for raw in &level.objects {
        let Some(info) = objects::info(raw.canonical_id()) else {
            *unknown.entry(raw.canonical_id()).or_default() += 1;
            continue;
        };
        if info.kind == ObjectType::MODIFIER {
            if visual_triggers.contains(&raw.id()) {
                *visual.entry(raw.id()).or_default() += 1;
            } else if !handled_triggers.contains(&raw.id()) {
                *triggers.entry(raw.id()).or_default() += 1;
            }
            continue;
        }
        if raw.get_f32(32).is_some_and(|s| s != 1.0) && info.kind != ObjectType::DECORATION {
            scaled += 1;
        }
        if !handled_kinds.contains(&(info.kind.0 as i32)) {
            let e = kinds.entry(info.kind.0 as i32).or_default();
            e.0 += 1;
            *e.1.entry(raw.id()).or_default() += 1;
        }
    }
    println!("{} objects", level.objects.len());
    for (kind, (n, ids)) in &kinds {
        let ids: Vec<String> = ids.iter().map(|(id, c)| format!("{id}×{c}")).collect();
        println!("unhandled type {kind}: {n} ({})", ids.join(", "));
    }
    for (id, n) in &triggers {
        println!("unhandled trigger {id}: {n}");
    }
    for (id, n) in &visual {
        println!("visual-only trigger {id}: {n}");
    }
    let _ = scaled;
    if !unknown.is_empty() {
        println!("ids missing from the object table: {unknown:?}");
    }
    Ok(())
}

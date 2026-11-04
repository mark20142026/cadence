//! Platform-free game logic for Cadence: data formats, objects, physics.
//! No file, window or network access here, so it builds the same for native and wasm32.

pub mod area;
pub mod fxhash;
pub mod camera;
pub mod codec;
pub mod collide;
pub mod easing;
pub mod enter;
pub mod gradients;
pub mod groups;
pub mod level;
pub mod objects;
pub mod order;
pub mod physics;
pub mod platformer;
pub mod save;
pub mod shaders;
pub mod slopes;
pub mod spider;
pub mod trace;
pub mod triggers;
pub mod world;

/// Debug counters (CADENCE_DEBUG_COUNTS): totals and maxima per name, printed every 600 calls of
/// the first counter.
pub fn debug_counts() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CADENCE_DEBUG_COUNTS").is_some())
}

pub fn count(name: &'static str, n: usize) {
    use std::sync::Mutex;
    static C: Mutex<Vec<(&'static str, usize, usize, usize)>> = Mutex::new(Vec::new());
    let mut c = C.lock().unwrap_or_else(|e| e.into_inner());
    match c.iter_mut().find(|e| e.0 == name) {
        Some(e) => {
            e.1 += 1;
            e.2 += n;
            e.3 = e.3.max(n);
        }
        None => c.push((name, 1, n, n)),
    }
    if c[0].1 % 600 == 0 && c[0].0 == name {
        let line: Vec<String> = c.iter().map(|e| format!("{}: mean {} max {}", e.0, e.2 / e.1.max(1), e.3)).collect();
        eprintln!("counts: {}", line.join(", "));
    }
}

//! The game's f32 libm calls (`sinf`, `cosf`, `exp2f`, `powf`). On macOS these are the system's
//! own, as the game calls them: Apple's are now and then an ulp from the correctly rounded value
//! (Dash's elastic zoom at step 3378 and its camera drift came from that). Elsewhere they're
//! computed in f64 and rounded, the correctly rounded f32, which equals Apple's almost always.
//! `black_box` keeps the compiler from folding a call with a known argument (e.g. powf(x, 2) into
//! x·x).

#[cfg(target_os = "macos")]
pub fn sinf(x: f32) -> f32 {
    std::hint::black_box(x).sin()
}

#[cfg(target_os = "macos")]
pub fn cosf(x: f32) -> f32 {
    std::hint::black_box(x).cos()
}

#[cfg(target_os = "macos")]
pub fn exp2f(x: f32) -> f32 {
    std::hint::black_box(x).exp2()
}

#[cfg(target_os = "macos")]
pub fn powf(x: f32, y: f32) -> f32 {
    x.powf(std::hint::black_box(y))
}

#[cfg(not(target_os = "macos"))]
pub fn sinf(x: f32) -> f32 {
    (x as f64).sin() as f32
}

#[cfg(not(target_os = "macos"))]
pub fn cosf(x: f32) -> f32 {
    (x as f64).cos() as f32
}

#[cfg(not(target_os = "macos"))]
pub fn exp2f(x: f32) -> f32 {
    (x as f64).exp2() as f32
}

#[cfg(not(target_os = "macos"))]
pub fn powf(x: f32, y: f32) -> f32 {
    (x as f64).powf(y as f64) as f32
}

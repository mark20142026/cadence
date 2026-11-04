//! Trace specs and trace files shared with the real-game harness (`harness/trace-mod`).

use std::collections::BTreeMap;

/// A harness spec: one level and one or more scripted attempts.
#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub level: i32,
    pub steps: u32,
    /// Output path; `%d` stands for the attempt number.
    pub out: String,
    /// Per attempt: jump-button (button 1) state changes keyed by step (`true` = press).
    pub attempts: Vec<BTreeMap<u32, bool>>,
    /// Per attempt: every button event in spec order, keyed by step: (press, button) with
    /// button 1 = jump, 2 = left, 3 = right (`PlayerButton`; left/right only act in platformer
    /// levels). Several events can share a step; they run in this order.
    pub buttons: Vec<BTreeMap<u32, Vec<(bool, u8)>>>,
    /// Game updates per rendered frame (`speed`; the cocos scheduler ticks once per frame).
    pub speed: u32,
    /// `levelstring <file>`: play this raw level string instead of a main level.
    pub level_string: Option<String>,
    /// `watchgroup <group> <file>` lines: the harness writes the group's first objects'
    /// positions every step there.
    pub watch: Vec<(i32, String)>,
    /// `watchmax N`: objects per watched group (4 by default).
    pub watch_max: usize,
}

impl Spec {
    pub fn parse(text: &str) -> Spec {
        let mut spec = Spec {
            level: 1,
            steps: 2400,
            attempts: vec![BTreeMap::new()],
            buttons: vec![BTreeMap::new()],
            speed: 1,
            watch_max: 4,
            ..Spec::default()
        };
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let Some(cmd) = words.next() else { continue };
            let arg = words.next().unwrap_or("");
            match cmd {
                "level" => spec.level = arg.parse().unwrap_or(1),
                "steps" => spec.steps = arg.parse().unwrap_or(2400),
                "out" => spec.out = arg.to_owned(),
                "speed" => spec.speed = arg.parse().unwrap_or(1),
                "levelstring" => spec.level_string = Some(arg.to_owned()),
                "watchmax" => spec.watch_max = arg.parse().unwrap_or(4),
                "watchgroup" => {
                    if let (Ok(group), Some(path)) = (arg.parse(), words.next()) {
                        spec.watch.push((group, path.to_owned()));
                    }
                }
                "attempt" => {
                    if !spec.buttons.last().unwrap().is_empty() {
                        spec.attempts.push(BTreeMap::new());
                        spec.buttons.push(BTreeMap::new());
                    }
                }
                "press" | "release" => {
                    if let Ok(step) = arg.parse() {
                        let button: u8 = words.next().and_then(|b| b.parse().ok()).unwrap_or(1);
                        if button == 1 {
                            spec.attempts.last_mut().unwrap().insert(step, cmd == "press");
                        }
                        spec.buttons.last_mut().unwrap().entry(step).or_default().push((cmd == "press", button));
                    }
                }
                _ => {}
            }
        }
        spec
    }

    /// Output path of one attempt.
    pub fn out_path(&self, attempt: usize) -> String {
        self.out.replace("%d", &attempt.to_string())
    }
}

/// One recorded step. Field order matches the harness CSV.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub step: u32,
    pub x: String,
    pub y: String,
    pub y_velocity: String,
    pub rotation: f32,
    pub flags: u32,
    /// Camera origin when the row was recorded (before this step's camera update), if present.
    pub cam: Option<(f32, f32)>,
}

pub const FLAG_ON_GROUND: u32 = 1 << 0;
pub const FLAG_UPSIDE_DOWN: u32 = 1 << 1;
pub const FLAG_DEAD: u32 = 1 << 2;
pub const FLAG_SHIP: u32 = 1 << 3;
pub const FLAG_BALL: u32 = 1 << 4;
pub const FLAG_UFO: u32 = 1 << 5;
pub const FLAG_WAVE: u32 = 1 << 6;
pub const FLAG_ROBOT: u32 = 1 << 7;
/// m_isSpider (bit 8).
pub const FLAG_SPIDER: u32 = 1 << 8;
/// m_isSwing (bit 9).
pub const FLAG_SWING: u32 = 1 << 9;
/// m_isSideways (bit 10) and m_isGoingLeft (bit 11).
pub const FLAG_SIDEWAYS: u32 = 1 << 10;
pub const FLAG_GOING_LEFT: u32 = 1 << 11;
/// m_isPlatformer (bit 12).
pub const FLAG_PLATFORMER: u32 = 1 << 12;
/// m_isDashing (bit 13).
pub const FLAG_DASHING: u32 = 1 << 13;
/// m_isOnSlope (bit 14 in the harness's flag list).
pub const FLAG_ON_SLOPE: u32 = 1 << 14;
/// m_isAccelerating (bit 15).
pub const FLAG_ACCELERATING: u32 = 1 << 15;

/// Parse a trace CSV (comment lines start with `#`).
pub fn parse_csv(text: &str) -> Vec<Row> {
    text.lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()))
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            Some(Row {
                step: f.first()?.parse().ok()?,
                x: f.get(2)?.to_string(),
                y: f.get(3)?.to_string(),
                y_velocity: f.get(4)?.to_string(),
                rotation: f.get(5)?.parse().ok()?,
                flags: f.get(6)?.parse().ok()?,
                cam: match (f.get(12).and_then(|v| v.parse().ok()), f.get(13).and_then(|v| v.parse().ok())) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                },
            })
        })
        .collect()
}

/// Format like C's `%.9g` (f32 fields) so traces compare as text.
pub fn fmt_f32(v: f32) -> String {
    fmt_g(v as f64, 9)
}

/// Format like C's `%.17g` (f64 fields).
pub fn fmt_f64(v: f64) -> String {
    fmt_g(v, 17)
}

fn fmt_g(v: f64, precision: usize) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    if exp < -4 || exp >= precision as i32 {
        let s = format!("{:.*e}", precision - 1, v);
        let (mantissa, e) = s.split_once('e').unwrap();
        let mantissa = trim_zeros(mantissa);
        let e: i32 = e.parse().unwrap();
        return format!("{mantissa}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    let decimals = (precision as i32 - 1 - exp).max(0) as usize;
    let s = format!("{v:.decimals$}");
    // Rounding can carry into a new digit (9.99.. -> 10.0); re-check the digit budget.
    let digits = s.trim_start_matches('-').replace('.', "").trim_start_matches('0').len();
    let s = if digits > precision && decimals > 0 { format!("{v:.prec$}", prec = decimals - 1) } else { s };
    trim_zeros(&s).to_owned()
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_printf_g() {
        assert_eq!(fmt_f32(1.298_250_4), "1.29825044");
        assert_eq!(fmt_f32(105.0), "105");
        assert_eq!(fmt_f64(10.747_999_999_999_999), "10.747999999999999");
        assert_eq!(fmt_f64(10.964), "10.964");
        assert_eq!(fmt_f64(-0.268_000_000_000_000_02), "-0.26800000000000002");
        assert_eq!(fmt_f32(0.899_999_976), "0.899999976");
    }
}

/// The window size from a trace header (`# win=W,H`).
/// The harness's `# sched speed=N index=I` header: the first recorded update's index within its
/// rendered frame (traces without it predate the deterministic scheduler).
pub fn sched_index(text: &str) -> Option<u32> {
    let line = text.lines().find(|l| l.starts_with("# sched "))?;
    line.split_whitespace().find_map(|w| w.strip_prefix("index=")).and_then(|v| v.parse().ok())
}

pub fn window_size(text: &str) -> Option<(f32, f32)> {
    let line = text.lines().find(|l| l.starts_with("# win="))?;
    let (w, h) = line.trim_start_matches("# win=").split_once(',')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

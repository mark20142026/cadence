//! `CreateParticlePopup`: the custom particle object's (2065) editor. Its four tabs (Motion,
//! Visual, Extra, Texture) are the real popup's (layouts `scenes/particle_*.txt` from captures;
//! the buttons measured from the same screens). Which value each field writes was probed in
//! the real game (harness `probe:N`: every input set, every toggle flipped by bit, the object
//! read back): inputs are fields of the particle string (key 145, 'a'-separated), the toggles
//! fields of it or keys of the object (146 use object color, 147 uniform object color, 123
//! animate on trigger, 214 animate active only, 211 quick start). The preview runs the
//! particles as the level will.

use super::canvas::{self, Canvas, Look};
use super::replay::Scene;
use super::{Btn, Ctx};
use crate::particles::{Emitter, ParticleDef};

/// Each tab's inputs, in the popup's reading order: their particle string field.
const INPUTS: [&[usize]; 3] = [
    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
    &[17, 18, 29, 30, 19, 20, 31, 32, 21, 22, 23, 24, 25, 26, 27, 28, 33, 34, 35, 36, 37, 38, 39, 40],
    &[41, 42, 43, 44, 59, 60, 68, 69, 70, 71, 61, 62],
];

/// A field's slider range (`createParticleSlider`).
fn range(f: usize) -> (f32, f32) {
    match f {
        0 => (0.0, 200.0),
        1 => (-1.0, 10.0),
        2 | 3 | 41..=44 | 59..=62 | 68..=71 => (0.0, 10.0),
        4 => (0.0, 200.0),
        5 => (-180.0, 180.0),
        6 => (0.0, 180.0),
        7..=10 => (0.0, 300.0),
        11..=16 => (-300.0, 300.0),
        17 | 18 | 29 | 30 => (0.0, 60.0),
        19 | 20 | 31 | 32 => (-360.0, 360.0),
        21..=28 | 33..=40 => (0.0, 1.0),
        _ => (0.0, 100.0),
    }
}

/// A toggle: where it is (tab, x, y), its label, and what it sets.
#[derive(Clone, Copy)]
enum Target {
    /// a particle string field set to this value when on
    Field(usize, &'static str),
    /// an object key (1 on)
    Key(u16),
}

const TOGGLES: [(u8, f32, f32, &str, Target); 19] = [
    (0, 74.5, 290.0, "Gravity", Target::Field(51, "0")),
    (0, 74.5, 266.0, "Radius", Target::Field(51, "1")),
    (2, 74.5, 290.0, "Free", Target::Field(52, "0")),
    (2, 74.5, 266.0, "Relative", Target::Field(52, "1")),
    (2, 74.5, 242.0, "Grouped", Target::Field(52, "2")),
    (2, 214.5, 126.0, "Additive", Target::Field(53, "1")),
    (2, 214.5, 102.0, "Start Size\n= End", Target::Field(64, "1")),
    (2, 214.5, 78.0, "Start Spin\n= End", Target::Field(54, "1")),
    (2, 214.5, 54.0, "Start Rad\n= End", Target::Field(65, "1")),
    (2, 214.5, 30.0, "Start rot\nis dir", Target::Field(55, "1")),
    (2, 324.5, 126.0, "Use\nobj color", Target::Key(146)),
    (2, 324.5, 102.0, "Uniform\nobj color", Target::Key(147)),
    (2, 324.5, 78.0, "Dynamic\nrotation", Target::Field(56, "1")),
    (2, 324.5, 54.0, "Animate\non Trigger", Target::Key(123)),
    (2, 324.5, 30.0, "Animate\nActive Only", Target::Key(214)),
    (2, 434.5, 126.0, "Order\nSensitive", Target::Field(63, "1")),
    (2, 434.5, 102.0, "StartRGB\nVar Sync", Target::Field(66, "1")),
    (2, 434.5, 78.0, "EndRGB\nVar Sync", Target::Field(67, "1")),
    (2, 434.5, 54.0, "Quick\nStart", Target::Key(211)),
];

pub const TEXTURES: usize = 362;
const PER_PAGE: usize = 88;

pub struct Editor {
    pub tab: u8,
    pub fields: Vec<String>,
    /// The object keys the popup sets (146, 147, 123, 214, 211): on or off
    pub keys: Vec<(u16, bool)>,
    pub focus: Option<(usize, String)>,
    pub texture_page: usize,
    preview: Option<Emitter>,
    preview_of: String,
    t: f32,
    /// The preview's scale (its 1 / 2 / 3 buttons); C centers it again
    pub zoom: f32,
}

/// The new particle object's string (the game's default for 2065).
pub const DEFAULT: &str = "30a-1a1a0.3a30a90a90a29a0a11a0a0a0a0a0a0a0a2a1a0a0a1a0a1a0a1a0a1a0a1a1a0a0a1a0a1a0a1a0a1a0a0a0a0a0a0a0a0a0a0a0a0a2a1a0a0a0a0a0a0a0a0a0a0";

impl Editor {
    pub fn new(string: Option<&str>, keys: impl Fn(u16) -> bool) -> Editor {
        let mut fields: Vec<String> = string.unwrap_or(DEFAULT).split('a').map(str::to_string).collect();
        fields.resize(72, "0".into());
        Editor { tab: 0, fields, keys: [146, 147, 123, 214, 211].map(|k| (k, keys(k))).to_vec(), focus: None, texture_page: 0, preview: None, preview_of: String::new(), t: 0.0, zoom: 1.0 }
    }

    pub fn string(&self) -> String {
        // the game writes 64 fields when the rest are zero
        let mut n = self.fields.len();
        while n > 64 && self.fields[n - 1] == "0" {
            n -= 1;
        }
        self.fields[..n].join("a")
    }

    fn value(&self, f: usize) -> f32 {
        self.fields.get(f).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }

    fn set(&mut self, f: usize, v: f32) {
        let text = if (21..=44).contains(&f) && !(29..=32).contains(&f) || matches!(f, 1..=3 | 59..=62 | 68..=71) { format!("{v:.2}") } else { format!("{}", v.round() as i64) };
        let text = text.trim_end_matches('0').trim_end_matches('.').to_string();
        self.fields[f] = if text.is_empty() || text == "-" { "0".into() } else { text };
    }

    fn on(&self, t: Target) -> bool {
        match t {
            Target::Field(f, v) => self.fields.get(f).is_some_and(|x| x == v),
            Target::Key(k) => self.keys.iter().any(|(kk, on)| *kk == k && *on),
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        let s = self.string();
        if s != self.preview_of {
            self.preview_of = s.clone();
            self.preview = ParticleDef::from_gd_string(&s).map(|(d, _)| Emitter::new(d, (0.0, 0.0), 7));
        }
        if let Some(e) = self.preview.as_mut() {
            e.update(dt);
            if e.finished() {
                e.reset();
            }
        }
    }

    /// A key typed into the focused input.
    pub fn type_text(&mut self, text: &str) {
        if let Some((_, t)) = self.focus.as_mut() {
            t.extend(text.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-'));
        }
    }

    pub fn backspace(&mut self) {
        if let Some((_, t)) = self.focus.as_mut() {
            t.pop();
        }
    }

    pub fn commit(&mut self) {
        if let Some((f, t)) = self.focus.take()
            && let Ok(v) = t.parse::<f32>()
        {
            // the angle shows negated (`-90` for 90)
            self.set(f, if f == 5 { -v } else { v });
        }
    }

    /// A button; true when the popup closes (OK).
    pub fn click(&mut self, id: Btn) -> bool {
        match id {
            Btn::PartTab(t) => {
                self.commit();
                self.tab = t.min(3);
            }
            Btn::PartInput(f) => {
                self.commit();
                let shown = if f == 5 { -self.value(5) } else { self.value(f as usize) };
                self.focus = Some((f as usize, format!("{}", shown)));
            }
            Btn::PartToggle(i) => {
                if let Some(&(_, _, _, _, t)) = TOGGLES.get(i as usize) {
                    match t {
                        Target::Field(f, v) => {
                            let off = if matches!(f, 51 | 52) { None } else { Some("0") };
                            match off {
                                // the mode / position type radio buttons only switch on
                                None => self.fields[f] = v.into(),
                                Some(o) => self.fields[f] = if self.fields[f] == v { o.into() } else { v.into() },
                            }
                        }
                        Target::Key(k) => {
                            if let Some(e) = self.keys.iter_mut().find(|(kk, _)| *kk == k) {
                                e.1 = !e.1;
                            }
                        }
                    }
                }
            }
            Btn::PartTexture(i) => self.fields[57] = (self.texture_page * PER_PAGE + i as usize).to_string(),
            Btn::PartTexPage(next) => {
                let pages = TEXTURES.div_ceil(PER_PAGE);
                self.texture_page = if next { (self.texture_page + 1) % pages } else { (self.texture_page + pages - 1) % pages };
            }
            // Inf.: forever; Max / Calc: the most particles the emission and lifetime need
            Btn::PartInf => self.fields[1] = "-1".into(),
            Btn::PartMax => {
                let need = (self.value(4) * (self.value(2) + self.value(3))).ceil().max(1.0);
                self.set(0, need);
            }
            Btn::PartCalc => {
                let n = self.value(0);
                let life = (self.value(2) + self.value(3)).max(0.01);
                self.set(4, n / life);
            }
            Btn::PartZoom(z) => self.zoom = if z == 0 { 1.0 } else { z as f32 * 0.5 + 0.5 },
            Btn::PartOk => {
                self.commit();
                return true;
            }
            _ => {}
        }
        false
    }

    /// A slider dragged to `t` (0..1).
    pub fn slide(&mut self, f: usize, t: f32) {
        let (lo, hi) = range(f);
        self.set(f, lo + t.clamp(0.0, 1.0) * (hi - lo));
    }
}

/// The input boxes of a tab's layout (square02_small), in reading order.
fn boxes(scene: Scene) -> Vec<(f32, f32)> {
    let mut v: Vec<(f32, f32)> = super::replay::nine_slices(scene, "square02_small.png");
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0)));
    v
}

fn grooves(scene: Scene) -> Vec<(f32, f32, f32, f32)> {
    let mut v = super::replay::slider_grooves(scene);
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0)));
    v
}

fn scene(tab: u8) -> Scene {
    Scene::Particle(tab.min(2))
}

/// A small `ButtonSprite`-like button.
fn button(c: &mut Canvas, ctx: &mut Ctx, id: Btn, frame: &str, label: &str, x: f32, y: f32, w: f32, h: f32, s: f32) {
    let f = ctx.factor(id);
    c.nine_slice(frame, x, y, w * f, h * f, Look::default());
    if !label.is_empty() {
        c.text("bigFont", label, x - 0.5, y + 1.0, Look::scaled(s * f));
    }
    ctx.area(id, x, y, w, h);
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, e: &Editor) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    let at = |x: f32| x + dx;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    if e.tab < 3 {
        let sc = scene(e.tab);
        // the captured values and thumbs are this object's
        super::replay::draw_where(c, ctx, sc, &|k, name, _, _| match k {
            'T' => !(name.chars().next().is_some_and(|ch| ch.is_ascii_digit() || ch == '-') && name.chars().all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-')) && !name.contains('|'),
            'S' => name != "sliderthumb.png",
            _ => true,
        });
        // two-line labels ("Max|Particles"), right-aligned on their row
        if e.tab == 0 {
            c.text("chatFont", "Max", at(224.5), 252.0, Look::scaled(0.5).anchor(1.0, 0.5));
            c.text("chatFont", "Particles", at(224.5), 240.0, Look::scaled(0.5).anchor(1.0, 0.5));
        }
        let bx = boxes(sc);
        let gr = grooves(sc);
        for (n, &f) in INPUTS[e.tab as usize].iter().enumerate() {
            if let Some(&(x, y)) = bx.get(n) {
                let focused = e.focus.as_ref().filter(|(ff, _)| *ff == f);
                let v = e.value(f);
                let text = match focused {
                    Some((_, t)) => format!("{t}_"),
                    None if f == 5 => format!("{}", -v as i64),
                    None if (21..=28).contains(&f) || (33..=44).contains(&f) || matches!(f, 1..=3 | 59..=62 | 68..=71) => format!("{v:.2}"),
                    None => format!("{}", v as i64),
                };
                let (tw, _) = c.text_size("bigFont", &text);
                let look = Look::scaled((40.0 / tw.max(1.0)).min(0.4));
                c.text("bigFont", &text, at(x), y, if focused.is_some() { look.rgb([255, 255, 150]) } else { look });
                ctx.area(Btn::PartInput(f as u8), at(x), y, 45.0, 16.0);
            }
            if let Some(&(gx, gy, sx, sy)) = gr.get(n) {
                let (gw, _) = c.size("slidergroove.png");
                let track = gw * sx - 14.0 * sy;
                let (lo, hi) = range(f);
                let t = ((e.value(f) - lo) / (hi - lo)).clamp(0.0, 1.0);
                c.sprite("sliderthumb.png", at(gx - track * 0.5 + t * track), gy, Look::scaled(sy));
                ctx.area(Btn::PartSlider(f as u8), at(gx), gy, track, 18.0);
            }
        }
        for (i, &(tab, x, y, label, t)) in TOGGLES.iter().enumerate() {
            if tab != e.tab {
                continue;
            }
            let id = Btn::PartToggle(i as u8);
            let fac = ctx.factor(id);
            c.sprite(if e.on(t) { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, at(x), y, Look::scaled(0.7 * fac));
            ctx.area(id, at(x), y, 24.0, 24.0);
            // the Extra tab's labels sit beside their boxes; Gravity / Radius are captured
            if tab == 2 && x > 100.0 {
                let lines = canvas::rich_lines(c, "bigFont", label, 999.0);
                canvas::draw_rich_left(c, "bigFont", &lines, at(x + 14.0), y + 7.0, 0.32, 30.0);
            }
        }
        if e.tab == 0 {
            button(c, ctx, Btn::PartInf, "GJ_button_04.png", "Inf.", at(393.8), 223.0, 30.0, 14.0, 0.3);
            button(c, ctx, Btn::PartMax, "GJ_button_04.png", "Max", at(393.8), 175.0, 30.0, 14.0, 0.3);
            button(c, ctx, Btn::PartCalc, "GJ_button_04.png", "Calc", at(429.4), 175.0, 30.0, 14.0, 0.3);
        }
    } else {
        c.nine_slice("GJ_square01.png", at(284.5), 160.0, 460.0, 300.0, Look::default());
        c.text("goldFont", "Create Particle Effect", at(284.5), 297.0, Look::scaled(0.6));
        // the textures, 11 × 8 a page; the chosen one framed
        let chosen = e.value(57) as usize;
        for i in 0..PER_PAGE {
            let n = e.texture_page * PER_PAGE + i;
            if n >= TEXTURES {
                break;
            }
            let (x, y) = (228.5 + 24.0 * (i % 11) as f32, 235.6 - 24.0 * (i / 11) as f32);
            let frame = format!("particle_{n:02}_001.png");
            let (fw, fh) = c.size(&frame);
            c.sprite(&frame, at(x), y, Look::scaled((18.0 / fw.max(fh).max(1.0)).min(1.0)));
            if n == chosen {
                c.nine_slice("square02_small.png", at(x), y, 22.0, 22.0, Look::default().alpha(80));
            }
            ctx.area(Btn::PartTexture(i as u8), at(x), y, 22.0, 22.0);
        }
        ctx.button(c, Btn::PartTexPage(false), "GJ_arrow_02_001.png", at(186.0), 135.0, Look::scaled(0.8), 1.2);
        ctx.button(c, Btn::PartTexPage(true), "GJ_arrow_02_001.png", at(485.0), 135.0, Look::scaled(0.8).flip_x(true), 1.2);
        c.text("goldFont", &format!("{}/{}", e.texture_page + 1, TEXTURES.div_ceil(PER_PAGE)), at(350.0), 31.0, Look::scaled(0.5));
    }
    // the tabs, copy / paste (the editor's own), OK
    for (t, label, x) in [(0u8, "Motion", 208.0), (1, "Visual", 258.7), (2, "Extra", 309.4), (3, "Texture", 360.0)] {
        let frame = if e.tab == t { "GJ_button_01.png" } else { "GJ_button_04.png" };
        button(c, ctx, Btn::PartTab(t), frame, label, at(x), 275.6, 40.0, 14.0, 0.25);
    }
    button(c, ctx, Btn::PartOk, "GJ_button_01.png", "OK", at(484.5), 286.0, 36.0, 24.0, 0.6);
    // copy / paste the settings between particle objects
    button(c, ctx, Btn::PartCopy, "GJ_button_04.png", "C", at(404.5), 286.0, 24.0, 24.0, 0.5);
    button(c, ctx, Btn::PartPaste, "GJ_button_04.png", "P", at(439.0), 286.0, 24.0, 24.0, 0.5);
    // the preview's scale: 1, 2, 3, and C back to 1
    for (z, label, x) in [(1u8, "1", 77.3), (2, "2", 101.3), (3, "3", 124.5), (0, "C", 148.5)] {
        button(c, ctx, Btn::PartZoom(z), "GJ_button_04.png", label, at(x), 215.0, 20.0, 15.0, 0.35);
    }
    c.rect(at(173.3) - 8.0, 207.0, 16.0, 16.0, [0.0; 3], 1.0);
    // the preview: black, the particles running in it
    let (px, py, pw, ph) = (at(124.5), 110.2, 119.0, 179.6);
    c.rect(px - pw * 0.5, py - ph * 0.5, pw, ph, [0.0; 3], 1.0);
    if let Some(em) = &e.preview {
        let start = c.mark();
        for ((x, y), size, _rot, col) in em.particles() {
            let (x, y) = (px + x * e.zoom, py + y * e.zoom);
            if (x - px).abs() > pw * 0.5 || (y - py).abs() > ph * 0.5 {
                continue;
            }
            let s = (size * e.zoom * 0.5).max(1.0);
            c.rect(x - s * 0.5, y - s * 0.5, s, s, [col[0], col[1], col[2]], col[3]);
        }
        let _ = start;
    }
}

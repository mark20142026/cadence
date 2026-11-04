//! Drawing helpers for the menus: cocos2d-style sprites (position, anchor, scale, rotation,
//! color, opacity), `CCScale9Sprite`, cropped bars (`setTextureRect`), `CCLabelBMFont` text, and
//! `CCMenuItemSpriteExtra` buttons with their press animation. Everything is in screen points
//! (the game's 320-point-tall view, origin bottom-left).

use cadence_core::easing::eased;

use crate::assets::{Frame, Sheets};
use crate::render::{Blend, Draw, sprite_instance, tint};

use super::font::Font;

/// How a node looks: cocos2d's per-node properties.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub scale: (f32, f32),
    pub rotation: f32,
    pub anchor: (f32, f32),
    pub color: [f32; 3],
    pub opacity: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub blend: Blend,
    /// Drawn in grayscale (`CCSpriteGrayscale`), tinted by `color`.
    pub gray: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look { scale: (1.0, 1.0), rotation: 0.0, anchor: (0.5, 0.5), color: [1.0; 3], opacity: 1.0, flip_x: false, flip_y: false, blend: Blend::Normal, gray: false }
    }
}

impl Look {
    pub fn scaled(s: f32) -> Look {
        Look { scale: (s, s), ..Look::default() }
    }
    pub fn anchor(mut self, x: f32, y: f32) -> Look {
        self.anchor = (x, y);
        self
    }
    pub fn scale(mut self, s: f32) -> Look {
        self.scale = (s, s);
        self
    }
    pub fn scale_xy(mut self, x: f32, y: f32) -> Look {
        self.scale = (x, y);
        self
    }
    /// Color from bytes, like ccColor3B.
    pub fn rgb(mut self, c: [u8; 3]) -> Look {
        self.color = c.map(|v| v as f32 / 255.0);
        self
    }
    /// Opacity from a byte, like GLubyte.
    pub fn alpha(mut self, a: u8) -> Look {
        self.opacity = a as f32 / 255.0;
        self
    }
    pub fn opacity(mut self, o: f32) -> Look {
        self.opacity = o;
        self
    }
    pub fn gray(mut self) -> Look {
        self.gray = true;
        self
    }

    pub fn flip_y(mut self, f: bool) -> Look {
        self.flip_y = f;
        self
    }

    pub fn flip_x(mut self, f: bool) -> Look {
        self.flip_x = f;
        self
    }
    pub fn blend(mut self, b: Blend) -> Look {
        self.blend = b;
        self
    }
}

/// cocos2d `nodeToParentTransform`: translate(pos) · rotate(-rotation°) · scale · translate(-anchor·size).
pub fn node_transform(x: f32, y: f32, rotation: f32, sx: f32, sy: f32, ax: f32, ay: f32, w: f32, h: f32) -> [f32; 6] {
    let (sin, cos) = (-rotation.to_radians()).sin_cos();
    let (ox, oy) = (-ax * w, -ay * h);
    let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    [a, b, c, d, a * ox + c * oy + x, b * ox + d * oy + y]
}

pub struct Canvas<'a> {
    pub sheets: &'a Sheets,
    pub atlas: &'a dyn Fn(usize) -> (f32, f32),
    pub fonts: &'a [Font],
    /// The robot's and spider's rigs (`*_AnimDesc.plist`), for their icons.
    pub rigs: Option<&'a [crate::anim::AnimDesc; 2]>,
    pub draws: Vec<Draw>,
    /// The view in points: width follows the window, height is 320.
    pub width: f32,
    pub height: f32,
    /// Draws are cut to this rect (x0, y0, x1, y1) while set, like a list's stencil: axis-aligned
    /// quads shrink with their texture coordinates; turned ones pass whole.
    pub clip: Option<[f32; 4]>,
}

/// An axis-aligned instance cut to `clip`, its UVs following; None if nothing is left. Turned
/// or sheared quads are kept whole.
pub fn clip_instance(mut inst: crate::render::Instance, clip: [f32; 4]) -> Option<crate::render::Instance> {
    let [a, b, c, d, tx, ty] = inst.m;
    if b != 0.0 || c != 0.0 || a == 0.0 || d == 0.0 {
        return Some(inst);
    }
    // the quad spans u, v in 0..1 → x = tx + a·u, y = ty + d·v
    let (u0, u1) = (((clip[0] - tx) / a), ((clip[2] - tx) / a));
    let (v0, v1) = (((clip[1] - ty) / d), ((clip[3] - ty) / d));
    let (ulo, uhi) = (u0.min(u1).max(0.0), u0.max(u1).min(1.0));
    let (vlo, vhi) = (v0.min(v1).max(0.0), v0.max(v1).min(1.0));
    if ulo >= uhi || vlo >= vhi {
        return None;
    }
    if ulo == 0.0 && uhi == 1.0 && vlo == 0.0 && vhi == 1.0 {
        return Some(inst);
    }
    // corner UVs: bottom-left, bottom-right, top-left, top-right, bilinear in u, v
    let uv = inst.uv;
    let at = |u: f32, v: f32, k: usize| {
        let bottom = uv[k] + (uv[2 + k] - uv[k]) * u;
        let top = uv[4 + k] + (uv[6 + k] - uv[4 + k]) * u;
        bottom + (top - bottom) * v
    };
    inst.uv = [at(ulo, vlo, 0), at(ulo, vlo, 1), at(uhi, vlo, 0), at(uhi, vlo, 1), at(ulo, vhi, 0), at(ulo, vhi, 1), at(uhi, vhi, 0), at(uhi, vhi, 1)];
    inst.m = [a * (uhi - ulo), 0.0, 0.0, d * (vhi - vlo), tx + a * ulo, ty + d * vlo];
    Some(inst)
}

impl Canvas<'_> {
    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.sheets.frames.get(name)
    }

    /// A frame's content size (its untrimmed size).
    pub fn size(&self, name: &str) -> (f32, f32) {
        self.frame(name).map_or((0.0, 0.0), |f| f.source_size)
    }

    fn push_frame(&mut self, frame: &Frame, transform: [f32; 6], look: &Look) {
        let atlas = (self.atlas)(frame.sheet);
        let mut color = tint(look.color, look.opacity);
        if look.gray {
            color[3] = -color[3];
        }
        let instance = sprite_instance(frame, atlas, transform, look.flip_x, look.flip_y, color);
        let instance = match self.clip {
            Some(clip) => match clip_instance(instance, clip) {
                Some(i) => i,
                None => return,
            },
            None => instance,
        };
        self.draws.push(Draw { sheet: frame.sheet, blend: look.blend, instance });
    }

    /// A sprite (`CCSprite::createWithSpriteFrameName`) at (x, y).
    pub fn sprite(&mut self, name: &str, x: f32, y: f32, look: Look) {
        let Some(frame) = self.frame(name).copied() else { return };
        let (w, h) = frame.source_size;
        let t = node_transform(x, y, look.rotation, look.scale.0, look.scale.1, look.anchor.0, look.anchor.1, w, h);
        self.push_frame(&frame, t, &look);
    }

    /// `CCScale9Sprite` with the default insets (the middle third of the image stretches), with
    /// content size `w` × `h`, centered on (x, y) unless `look.anchor` says otherwise.
    pub fn nine_slice(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, look: Look) {
        let Some(frame) = self.frame(name).copied() else { return };
        let (fw, fh) = frame.size;
        let (px, py) = (frame.tex_w / fw, frame.tex_h / fh); // pixels per point
        let (cw, ch) = (fw / 3.0, fh / 3.0);
        let x0 = x - look.anchor.0 * w;
        let y0 = y - look.anchor.1 * h;
        // columns: (dest x, dest w, source x, source w), same for rows (source rows top-down)
        let cols = [(0.0, cw, 0.0, cw), (cw, w - 2.0 * cw, cw, cw), (w - cw, cw, 2.0 * cw, cw)];
        let rows = [(0.0, ch, 2.0 * ch, ch), (ch, h - 2.0 * ch, ch, ch), (h - ch, ch, 0.0, ch)];
        for &(dx, dw, sx, sw) in &cols {
            for &(dy, dh, sy, sh) in &rows {
                if dw <= 0.0 || dh <= 0.0 {
                    continue;
                }
                let part = Frame {
                    tex_x: frame.tex_x + sx * px,
                    tex_y: frame.tex_y + sy * py,
                    tex_w: sw * px,
                    tex_h: sh * py,
                    size: (sw, sh),
                    offset: (0.0, 0.0),
                    source_size: (sw, sh),
                    ..frame
                };
                let t = node_transform(x0 + dx, y0 + dy, 0.0, dw / sw, dh / sh, 0.0, 0.0, sw, sh);
                self.push_frame(&part, t, &look);
            }
        }
    }

    /// A sprite showing only the left `fraction` of its image (`setTextureRect(0, 0, w·f, h)`):
    /// the progress bars. Anchored at its left edge's middle unless `look` says otherwise.
    pub fn bar(&mut self, name: &str, x: f32, y: f32, fraction: f32, look: Look) {
        let Some(frame) = self.frame(name).copied() else { return };
        let fraction = fraction.clamp(0.0, 1.0);
        if fraction <= 0.0 {
            return;
        }
        let (fw, fh) = frame.size;
        let w = fw * fraction;
        let part = Frame { tex_w: frame.tex_w * fraction, size: (w, fh), offset: (0.0, 0.0), source_size: (w, fh), ..frame };
        let t = node_transform(x, y, look.rotation, look.scale.0, look.scale.1, look.anchor.0, look.anchor.1, w, fh);
        self.push_frame(&part, t, &look);
    }

    pub fn font(&self, name: &str) -> Option<&Font> {
        self.fonts.iter().find(|f| f.name == name)
    }

    /// A `CCLabelBMFont` at (x, y). Returns its content size (unscaled).
    pub fn text(&mut self, font: &str, text: &str, x: f32, y: f32, look: Look) -> (f32, f32) {
        let Some(f) = self.font(font) else { return (0.0, 0.0) };
        let (glyphs, (w, h)) = f.layout(text);
        let parent = node_transform(x, y, look.rotation, look.scale.0, look.scale.1, look.anchor.0, look.anchor.1, w, h);
        for g in glyphs {
            let Some(frame) = self.frame(&g.frame).copied() else { continue };
            let (gw, gh) = frame.source_size;
            let local = node_transform(g.x, g.y, 0.0, 1.0, 1.0, 0.5, 0.5, gw, gh);
            self.push_frame(&frame, mul(parent, local), &look);
        }
        (w, h)
    }

    /// A label's content size.
    pub fn text_size(&self, font: &str, text: &str) -> (f32, f32) {
        self.font(font).map_or((0.0, 0.0), |f| f.size(text))
    }

    /// A solid rectangle (`CCLayerColor`): the white texel of the particle texture, stretched.
    /// Where the next draw goes: pass it to `transform_since` / `fade_since`.
    pub fn mark(&self) -> usize {
        self.draws.len()
    }

    /// Scale everything drawn since `start` about (cx, cy), then move it by (dx, dy): an
    /// open animation on a whole layer (the layer's own node scale / position).
    pub fn transform_since(&mut self, start: usize, s: f32, (cx, cy): (f32, f32), (dx, dy): (f32, f32)) {
        let n = self.draws.len();
        for d in &mut self.draws[start.min(n)..] {
            let m = &mut d.instance.m;
            for v in &mut m[..4] {
                *v *= s;
            }
            m[4] = cx + (m[4] - cx) * s + dx;
            m[5] = cy + (m[5] - cy) * s + dy;
        }
    }

    /// Multiply the opacity of everything drawn since `start` (premultiplied colors).
    pub fn fade_since(&mut self, start: usize, f: f32) {
        let n = self.draws.len();
        for d in &mut self.draws[start.min(n)..] {
            for v in &mut d.instance.color {
                *v *= f;
            }
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 3], opacity: f32) {
        let Some(frame) = self.frame(crate::app::PARTICLE_TEXTURE).copied() else { return };
        // a tiny region in the middle of the (white, round-edged) particle texture
        let (cx, cy) = (frame.tex_x + frame.tex_w * 0.5, frame.tex_y + frame.tex_h * 0.5);
        let part = Frame { tex_x: cx - 0.5, tex_y: cy - 0.5, tex_w: 1.0, tex_h: 1.0, size: (1.0, 1.0), offset: (0.0, 0.0), source_size: (1.0, 1.0), ..frame };
        let t = node_transform(x, y, 0.0, w, h, 0.0, 0.0, 1.0, 1.0);
        self.push_frame(&part, t, &Look { color, opacity, ..Look::default() });
    }
}

fn mul(p: [f32; 6], q: [f32; 6]) -> [f32; 6] {
    [
        p[0] * q[0] + p[2] * q[1],
        p[1] * q[0] + p[3] * q[1],
        p[0] * q[2] + p[2] * q[3],
        p[1] * q[2] + p[3] * q[3],
        p[0] * q[4] + p[2] * q[5] + p[4],
        p[1] * q[4] + p[3] * q[5] + p[5],
    ]
}

/// cocos2d's eases, through the game's own `getEasedValue` port.
pub fn ease_bounce_out(t: f32) -> f32 {
    eased(t.clamp(0.0, 1.0), 0.0, 9)
}
pub fn ease_elastic_out(t: f32, period: f32) -> f32 {
    eased(t.clamp(0.0, 1.0), period, 6)
}
pub fn ease_in_out(t: f32, rate: f32) -> f32 {
    eased(t.clamp(0.0, 1.0), rate, 1)
}
pub fn ease_in(t: f32, rate: f32) -> f32 {
    eased(t.clamp(0.0, 1.0), rate, 2)
}

/// `CCMenuItemSpriteExtra`'s press animation (0x20ac34 `selected`, `unselected`, `activate`):
/// pressing scales the item to base × 1.26 over 0.3 s (CCEaseBounceOut); releasing outside
/// scales back over 0.4 s; activating snaps it back at once.
#[derive(Clone, Copy, Debug, Default)]
pub struct Press {
    from: f32,
    to: f32,
    duration: f32,
    t: f32,
}

pub const PRESS_SCALE: f32 = 1.26;

impl Press {
    /// The scale multiplier now (1 at rest).
    pub fn factor(&self) -> f32 {
        if self.duration <= 0.0 {
            return if self.to == 0.0 { 1.0 } else { self.to };
        }
        let k = ease_bounce_out(self.t / self.duration);
        self.from + (self.to - self.from) * k
    }
    pub fn update(&mut self, dt: f32) {
        self.t = (self.t + dt).min(self.duration);
    }
    /// Pressed: grow to `mult` (1.26 by default, `+0x190`).
    pub fn select(&mut self, mult: f32) {
        *self = Press { from: self.factor(), to: mult, duration: 0.3, t: 0.0 };
    }
    pub fn unselect(&mut self) {
        *self = Press { from: self.factor(), to: 1.0, duration: 0.4, t: 0.0 };
    }
    pub fn activate(&mut self) {
        *self = Press::default();
    }
}

/// The colors of `MultilineBitmapFont`'s tags (`<cg>green</c>` …).
pub fn tag_color(tag: &str) -> Option<[u8; 3]> {
    Some(match tag {
        "cb" => [74, 82, 225],
        "cg" => [64, 227, 72],
        "cl" => [96, 171, 239],
        "cj" => [50, 200, 255],
        "cy" => [255, 255, 0],
        "co" => [255, 165, 75],
        "cr" => [255, 90, 90],
        "cp" => [255, 0, 255],
        "ca" => [150, 50, 255],
        "cd" => [255, 150, 255],
        "cc" => [255, 255, 150],
        "cf" => [150, 255, 255],
        "cs" => [255, 220, 65],
        _ => return None,
    })
}

/// One line of tagged text: runs of (text, color).
pub type RichLine = Vec<(String, [u8; 3])>;

/// Split tagged text into words with their colors, wrapped at `max` (unscaled font units, as
/// `TextArea` takes it); '\n' breaks a line.
pub fn rich_lines(c: &Canvas, font: &str, text: &str, max: f32) -> Vec<RichLine> {
    let white = [255, 255, 255];
    let mut lines = Vec::new();
    for para in text.split('\n') {
        // (word, color) pieces; a word can change color mid-way
        let mut words: Vec<RichLine> = vec![Vec::new()];
        let mut color = white;
        let mut rest = para;
        while !rest.is_empty() {
            if let Some(r) = rest.strip_prefix("</c>") {
                color = white;
                rest = r;
                continue;
            }
            // delay / shake / size tags (<d040>, <i050>, <s260>, </s>, </i>): no color
            if let Some(close) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
                let tag = &rest[1..close + 1];
                if tag == "/s" || tag == "/i" || (tag.len() == 4 && matches!(tag.as_bytes()[0], b'd' | b'i' | b's') && tag[1..].bytes().all(|b| b.is_ascii_digit())) {
                    rest = &rest[close + 2..];
                    continue;
                }
            }
            if rest.starts_with('<') && rest.len() >= 4 && rest.as_bytes()[3] == b'>' {
                if let Some(col) = tag_color(&rest[1..3]) {
                    color = col;
                    rest = &rest[4..];
                    continue;
                }
            }
            let ch = rest.chars().next().unwrap();
            rest = &rest[ch.len_utf8()..];
            if ch == ' ' {
                words.push(Vec::new());
                continue;
            }
            let w = words.last_mut().unwrap();
            match w.last_mut() {
                Some((s, c)) if *c == color => s.push(ch),
                _ => w.push((ch.to_string(), color)),
            }
        }
        let mut line: RichLine = Vec::new();
        let mut line_w = 0.0;
        let space = c.text_size(font, " ").0.max(c.text_size(font, "a a").0 - 2.0 * c.text_size(font, "a").0);
        for word in words.into_iter().filter(|w| !w.is_empty()) {
            let ww: f32 = word.iter().map(|(s, _)| c.text_size(font, s).0).sum();
            if !line.is_empty() && line_w + space + ww > max {
                lines.push(std::mem::take(&mut line));
                line_w = 0.0;
            }
            if !line.is_empty() {
                line.push((" ".into(), white));
                line_w += space;
            }
            line.extend(word);
            line_w += ww;
        }
        lines.push(line);
    }
    lines
}

/// Draw lines from `rich_lines` left-aligned at `x` (a `TextArea` anchored left).
pub fn draw_rich_left(c: &mut Canvas, font: &str, lines: &[RichLine], x: f32, top: f32, scale: f32, spacing: f32) {
    let space = c.text_size(font, "a a").0 - 2.0 * c.text_size(font, "a").0;
    let line_h = c.text_size(font, "A").1;
    for (i, line) in lines.iter().enumerate() {
        let mut lx = x;
        let y = top - i as f32 * spacing * scale - line_h * scale * 0.5;
        for (s, col) in line {
            if s == " " {
                lx += space * scale;
                continue;
            }
            c.text(font, s, lx, y, Look::scaled(scale).anchor(0.0, 0.5).rgb(*col));
            lx += c.text_size(font, s).0 * scale;
        }
    }
}

/// Draw lines from `rich_lines` centered on `x`, the first line's top at `top`, `spacing`
/// (unscaled) apart, at `scale`. `alpha` multiplies the colors' opacity.
pub fn draw_rich(c: &mut Canvas, font: &str, lines: &[RichLine], x: f32, top: f32, scale: f32, spacing: f32, alpha: u8) {
    let space = c.text_size(font, "a a").0 - 2.0 * c.text_size(font, "a").0;
    let line_h = c.text_size(font, "A").1;
    for (i, line) in lines.iter().enumerate() {
        let widths: Vec<f32> = line.iter().map(|(s, _)| if s == " " { space } else { c.text_size(font, s).0 }).collect();
        let total: f32 = widths.iter().sum();
        let mut lx = x - total * scale * 0.5;
        let y = top - i as f32 * spacing * scale - line_h * scale * 0.5;
        for ((s, col), w) in line.iter().zip(widths) {
            if s != " " {
                c.text(font, s, lx, y, Look::scaled(scale).anchor(0.0, 0.5).rgb(*col).alpha(alpha));
            }
            lx += w * scale;
        }
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    #[test]
    fn clipping_shrinks_the_quad_and_its_uvs() {
        let inst = crate::render::Instance { m: [10.0, 0.0, 0.0, 20.0, 0.0, 0.0], _pad: [0.0; 2], uv: [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], color: [1.0; 4] };
        // keep the top half (y 10..20)
        let c = clip_instance(inst, [-5.0, 10.0, 50.0, 50.0]).unwrap();
        assert_eq!(c.m, [10.0, 0.0, 0.0, 10.0, 0.0, 10.0]);
        // the texture's top half: v from 0.5 (bottom) to 0 (top)
        assert_eq!(c.uv, [0.0, 0.5, 1.0, 0.5, 0.0, 0.0, 1.0, 0.0]);
        assert!(clip_instance(inst, [0.0, 30.0, 10.0, 40.0]).is_none());
    }
}

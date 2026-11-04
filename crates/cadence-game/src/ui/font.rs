//! Bitmap fonts: the game's BMFont text files (`bigFont-uhd.fnt` + `bigFont-uhd.png`, ...) laid
//! out like cocos2d 2.x `CCLabelBMFont::createFontChars`.
//!
//! Each glyph becomes a frame in the shared `Sheets` (named `"<font>#<char id>"`), so text draws
//! through the same sprite path as everything else.

use std::collections::HashMap;

use crate::assets::{Frame, Sheets, load_png};
use crate::files::Files;

/// The fonts the menus use.
pub const FONTS: &[&str] = &["bigFont", "goldFont", "chatFont"];

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Image size, offset from the pen position (top-down, like the file) and advance, in points.
    pub w: f32,
    pub h: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub advance: f32,
}

#[derive(Debug, Default)]
pub struct Font {
    pub glyphs: HashMap<u32, Glyph>,
    pub kerning: HashMap<(u32, u32), f32>,
    /// `common lineHeight`, in points.
    pub line_height: f32,
    pub name: String,
}

/// One placed glyph: its frame name and center, in the label's content space (origin bottom-left).
pub struct Placed {
    pub frame: String,
    pub x: f32,
    pub y: f32,
}

impl Font {
    /// Parse `<name><suffix>.fnt` and load its page as a sheet (glyphs become frames).
    pub fn load(files: &dyn Files, sheets: &mut Sheets, name: &str) -> Result<Font, String> {
        let suffix = sheets.quality.suffix();
        let scale = sheets.quality.scale();
        let path = format!("{name}{suffix}.fnt");
        let text = files.read(&path).ok_or_else(|| format!("{path}: missing"))?;
        let text = String::from_utf8_lossy(&text);
        let mut font = Font { name: name.to_string(), ..Font::default() };
        let mut page = None;
        let mut rects = Vec::new();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap_or("");
            let fields: HashMap<&str, &str> = words.filter_map(|w| w.split_once('=')).collect();
            let num = |k: &str| fields.get(k).and_then(|v| v.trim_matches('"').parse::<f32>().ok()).unwrap_or(0.0);
            match kind {
                "common" => font.line_height = num("lineHeight") / scale,
                "page" => page = fields.get("file").map(|f| f.trim_matches('"').to_string()),
                "char" => {
                    let id = num("id") as u32;
                    rects.push((id, num("x"), num("y"), num("width"), num("height")));
                    font.glyphs.insert(
                        id,
                        Glyph {
                            w: num("width") / scale,
                            h: num("height") / scale,
                            x_offset: num("xoffset") / scale,
                            y_offset: num("yoffset") / scale,
                            advance: num("xadvance") / scale,
                        },
                    );
                }
                "kerning" => {
                    font.kerning.insert((num("first") as u32, num("second") as u32), num("amount") / scale);
                }
                _ => {}
            }
        }
        let page = page.ok_or_else(|| format!("{path}: no page"))?;
        let image = load_png(files, &page)?;
        let sheet = sheets.images.len();
        sheets.images.push(image);
        for (id, x, y, w, h) in rects {
            let size = (w / scale, h / scale);
            sheets.frames.insert(
                glyph_frame(name, id),
                Frame { sheet, tex_x: x, tex_y: y, tex_w: w, tex_h: h, rotated: false, size, offset: (0.0, 0.0), source_size: size },
            );
        }
        Ok(font)
    }

    /// Lay out `text` (cocos2d 2.x rules): returns the glyphs and the label's content size.
    /// Lines are separated by '\n'.
    pub fn layout(&self, text: &str) -> (Vec<Placed>, (f32, f32)) {
        let lines = text.split('\n').count().max(1) as f32;
        let common = self.line_height;
        let mut x = 0.0f32;
        let mut y = -(common - common * lines);
        let mut longest = 0.0f32;
        let mut prev: Option<u32> = None;
        let mut out = Vec::new();
        let mut last: Option<Glyph> = None;
        for c in text.chars() {
            let id = c as u32;
            if c == '\n' {
                x = 0.0;
                y -= common;
                prev = None;
                continue;
            }
            let Some(g) = self.glyphs.get(&id).copied() else { continue };
            let kern = prev.and_then(|p| self.kerning.get(&(p, id)).copied()).unwrap_or(0.0);
            let cx = x + g.x_offset + g.w * 0.5 + kern;
            let cy = y + (common - g.y_offset) - g.h * 0.5;
            if g.w > 0.0 && g.h > 0.0 {
                out.push(Placed { frame: glyph_frame(&self.name, id), x: cx, y: cy });
            }
            x += g.advance + kern;
            longest = longest.max(x);
            prev = Some(id);
            last = Some(g);
        }
        let mut width = longest;
        // The last character's image can stick out past its advance.
        if let Some(g) = last
            && g.advance < g.w
        {
            width = longest + g.w - g.advance;
        }
        (out, (width, common * lines))
    }

    /// The label's content size without placing glyphs.
    pub fn size(&self, text: &str) -> (f32, f32) {
        self.layout(text).1
    }
}

fn glyph_frame(font: &str, id: u32) -> String {
    format!("{font}#{id}")
}

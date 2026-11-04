//! The icon kit (`GJGarageLayer`, 0x2e1efc; layout measured from the real screen where the
//! decompile is cut short): the icon types' tabs, a page of 36 icons (12 × 3) with page dots,
//! the preview, and the color pages (`CharacterColorPage`). Choices live in Cadence's save and
//! `IconSet` gives them to the level. Every icon is selectable (Cadence has no unlocks).

use super::canvas::{Canvas, Look};
use super::menu::{PALETTE, color_for_idx};
use super::save::Progress;
use super::{Btn, Ctx};

/// The tabs, left to right: (sheet prefix, tab sprite, how many icons the game has).
pub const KINDS: [(&str, &str, u32); 11] = [
    ("player", "gj_iconBtn", 485),
    ("ship", "gj_shipBtn", 169),
    ("player_ball", "gj_ballBtn", 118),
    ("bird", "gj_birdBtn", 149),
    ("dart", "gj_dartBtn", 96),
    ("robot", "gj_robotBtn", 68),
    ("spider", "gj_spiderBtn", 69),
    ("swing", "gj_swingBtn", 43),
    ("jetpack", "gj_jetpackBtn", 8),
    ("streak", "gj_streakBtn", 7),
    ("explosion", "gj_explosionBtn", 20),
];

/// Icon types with sheets of their own (the last two tabs are effects).
pub const ICON_KINDS: usize = 9;

/// The player's icons and colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconSet {
    /// Chosen icon per kind (1-based, as the files are numbered).
    pub ids: [u32; 11],
    /// Palette indices (`GameManager::colorForIdx`): 0 = green, 3 = cyan by default.
    pub color1: usize,
    pub color2: usize,
}

impl Default for IconSet {
    fn default() -> IconSet {
        IconSet { ids: [1; 11], color1: 0, color2: 3 }
    }
}

impl IconSet {
    pub fn from_progress(p: &Progress) -> IconSet {
        let mut s = IconSet::default();
        for (k, id) in s.ids.iter_mut().enumerate() {
            *id = (p.number(&format!("icon{k}"), 1.0) as u32).clamp(1, KINDS[k].2);
        }
        s.color1 = (p.number("color1", 0.0) as usize).min(PALETTE.len() - 1);
        s.color2 = (p.number("color2", 3.0) as usize).min(PALETTE.len() - 1);
        s
    }

    /// The sheet (and frame prefix) of kind `k`'s chosen icon: `player_37`, `ship_02`, ...
    pub fn sheet(&self, k: usize) -> String {
        icon_sheet(k, self.ids[k])
    }

    pub fn colors(&self) -> ([u8; 3], [u8; 3]) {
        (color_for_idx(self.color1), color_for_idx(self.color2))
    }

    /// Every sheet a level needs.
    pub fn sheets(&self) -> Vec<String> {
        (0..ICON_KINDS).map(|k| self.sheet(k)).collect()
    }
}

/// How big each type's icons are in the grid (`GJItemIcon`; measured on the real kit against the
/// cube's 0.8): wide vehicles are shrunk to fit their cell.
pub const GRID_SCALE: [f32; 9] = [0.8, 0.64, 0.68, 0.72, 0.67, 0.6, 0.6, 0.59, 0.57];

pub fn icon_sheet(kind: usize, id: u32) -> String {
    format!("{}_{:02}", KINDS[kind].0, id)
}

/// An icon's layers as the game stacks them: secondary color, the UFO's dome, main color, and
/// the uncolored extra. Robots and spiders are assembled from their rig's `idle01` frame (as
/// `SimplePlayer` shows them), far limbs at 0.7 of the colors.
pub fn draw_icon(c: &mut Canvas, base: &str, x: f32, y: f32, scale: f32, c1: [u8; 3], c2: [u8; 3]) {
    let rig = if base.starts_with("robot_") { Some((0, "robot_01")) } else if base.starts_with("spider_") { Some((1, "spider_01")) } else { None };
    if let (Some((i, stock)), Some(rigs)) = (rig, c.rigs) {
        let parts: Vec<crate::anim::Part> = rigs[i].frame("idle01", 0).map(|p| p.to_vec()).unwrap_or_default();
        if !parts.is_empty() {
            let dark = |c: [u8; 3]| c.map(|v| (v as f32 * 0.7) as u8);
            for part in &parts {
                let tex = part.texture.replacen(stock, base, 1);
                let (a, b) = if part.back { (dark(c1), dark(c2)) } else { (c1, c2) };
                let (px, py) = (x + part.pos.0 * scale, y + part.pos.1 * scale);
                let look = |col: Option<[u8; 3]>| {
                    let mut l = Look::default().scale_xy(part.scale.0 * scale, part.scale.1 * scale).flip_x(part.flip.0).flip_y(part.flip.1);
                    l.rotation = part.rotation;
                    if let Some(col) = col {
                        l = l.rgb(col);
                    }
                    l
                };
                c.sprite(&format!("{tex}_2_001.png"), px, py, look(Some(b)));
                c.sprite(&format!("{tex}_001.png"), px, py, look(Some(a)));
                c.sprite(&format!("{tex}_extra_001.png"), px, py, look(None));
            }
            return;
        }
    }
    let base = if rig.is_some() { format!("{base}_01") } else { base.to_string() };
    c.sprite(&format!("{base}_2_001.png"), x, y, Look::scaled(scale).rgb(c2));
    c.sprite(&format!("{base}_3_001.png"), x, y, Look::scaled(scale));
    c.sprite(&format!("{base}_001.png"), x, y, Look::scaled(scale).rgb(c1));
    c.sprite(&format!("{base}_extra_001.png"), x, y, Look::scaled(scale));
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum View {
    #[default]
    Icons,
    /// The color page: which color the swatches set (1 or 2).
    Colors(u8),
}

#[derive(Clone, Debug, Default)]
pub struct Garage {
    pub kind: usize,
    pub page: u32,
    pub view: View,
}

pub const PER_PAGE: u32 = 36;

impl Garage {
    pub fn pages(&self) -> u32 {
        KINDS[self.kind].2.div_ceil(PER_PAGE)
    }

    /// Icon sheets the current page shows (the app loads them before drawing).
    pub fn wanted_sheets(&self, set: &IconSet) -> Vec<String> {
        let mut out = set.sheets();
        if self.kind < ICON_KINDS {
            let first = self.page * PER_PAGE + 1;
            for id in first..(first + PER_PAGE).min(KINDS[self.kind].2 + 1) {
                out.push(icon_sheet(self.kind, id));
            }
        }
        out
    }

    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx, set: &IconSet, p: &Progress) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        // background: the gradient in gray, the side art in the corners
        let (gw, gh) = c.size("GJ_gradientBG.png");
        if gw > 0.0 {
            c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb([150, 150, 150]));
        }
        c.sprite("GJ_sideArt_001.png", -1.0, h + 1.0, Look::default().anchor(0.0, 1.0).flip_y(true));
        c.sprite("GJ_sideArt_001.png", -1.0, -1.0, Look::default().anchor(0.0, 0.0));
        c.sprite("GJ_sideArt_001.png", w + 1.0, -1.0, Look::default().anchor(1.0, 0.0).flip_x(true));
        // the shop's sign on its rope (the shop isn't in Cadence)
        let (rw, rh) = c.size("shopRope_001.png");
        ctx.button(c, Btn::GarageShop, "shopRope_001.png", cx - 135.0, h + 33.0 - rh * 0.5, Look::default(), 1.0);
        let _ = rw;
        // "What's your name?"
        c.sprite("GJ_nameTxt_001.png", cx + 115.0, h - 24.0, Look::default());
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_03_001.png", 25.0, h - 23.0, Look::default(), 1.6);
        ctx.button(c, Btn::GarageShards, "GJ_shardsBtn_001.png", 30.0, h - 80.0, Look::default(), 1.0);
        ctx.button(c, Btn::GaragePaint, "GJ_paintBtn_001.png", 30.0, h - 119.0, Look::default(), 1.0);

        // the name and the preview
        c.text("bigFont", "Player", cx, h - 36.0, Look::default());
        let (c1, c2) = set.colors();
        // the preview is always the cube (the real kit's player), whatever the tab
        draw_icon(c, &set.sheet(0), cx, h - 85.0, 1.6, c1, c2);
        c.sprite("floorLine_001.png", cx, h - 110.0, Look::default().scale_xy(1.6, 1.0));

        // the stats, top right: number, then icon
        let stats = [
            ("GJ_starsIcon_001.png", 0u32),
            ("GJ_moonsIcon_001.png", 0),
            ("GJ_coinsIcon_001.png", p.secret_coins()),
            ("GJ_coinsIcon2_001.png", 0),
            ("currencyOrbIcon_001.png", 0),
            ("GJ_diamondsIcon_001.png", 0),
            ("currencyDiamondIcon_001.png", 0),
        ];
        for (i, (icon, n)) in stats.iter().enumerate() {
            let y = h - 12.5 - 15.0 * i as f32;
            c.text("bigFont", &n.to_string(), w - 29.0, y, Look::scaled(0.42).anchor(1.0, 0.5));
            c.sprite(icon, w - 18.0, y, Look::scaled(0.55));
        }

        // the tabs at (cx + (i − 5)·34, cy + 30)
        for (i, (_, tab, _)) in KINDS.iter().enumerate() {
            let on = i == self.kind && self.view == View::Icons;
            let name = format!("{tab}_{}_001.png", if on { "on" } else { "off" });
            ctx.button(c, Btn::GarageTab(i as u8), &name, cx + (i as f32 - 5.0) * 34.0, cy + 30.0, Look::scaled(0.85), 1.0);
        }

        match self.view {
            View::Icons => self.draw_icons(c, ctx, set),
            // the color page is a popup over the kit
            View::Colors(which) => {
                let mut under = Vec::new();
                self.draw_icons(c, &mut Ctx { press: ctx.press, buttons: &mut under }, set);
                draw_colors(c, ctx, set, which);
            }
        }
    }

    fn draw_icons(&self, c: &mut Canvas, ctx: &mut Ctx, set: &IconSet) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.sprite("GJ_unlockTxt_001.png", cx, cy - 3.0, Look::default());
        c.nine_slice("square02_001.png", cx, cy - 65.0, 385.0, 100.0, Look::default().rgb([0, 0, 0]).alpha(75));
        let first = self.page * PER_PAGE + 1;
        let last = KINDS[self.kind].2;
        for slot in 0..PER_PAGE {
            let id = first + slot;
            if id > last {
                break;
            }
            let (col, row) = (slot % 12, slot / 12);
            let (x, y) = (cx - 165.0 + col as f32 * 30.0, cy - 35.0 - row as f32 * 30.0);
            if self.kind >= ICON_KINDS {
                // the effects: trails (`player_special_NN`) and death effects (`explosionIcon_NN`),
                // their own icons in the same grid
                let frame = match KINDS[self.kind].0 {
                    "streak" => format!("player_special_{id:02}_001.png"),
                    _ => format!("explosionIcon_{id:02}_001.png"),
                };
                c.sprite(&frame, x, y, Look::scaled(0.8));
            } else {
                draw_icon(c, &icon_sheet(self.kind, id), x, y, GRID_SCALE[self.kind], [175, 175, 175], [255, 255, 255]);
            }
            ctx.area(Btn::GarageIcon(slot as u8), x, y, 28.0, 28.0);
            if set.ids[self.kind] == id {
                c.sprite("GJ_select_001.png", x, y, Look::scaled(0.85));
            }
        }
        let pages = self.pages();
        if pages > 1 {
            ctx.button(c, Btn::GaragePrev, "GJ_arrow_01_001.png", cx - 220.0, cy - 65.0, Look::default(), 1.5);
            ctx.button(c, Btn::GarageNext, "GJ_arrow_01_001.png", cx + 220.0, cy - 65.0, Look::default().flip_x(true), 1.5);
            let x0 = cx - (pages - 1) as f32 * 24.4 * 0.5;
            for i in 0..pages {
                let name = if i == self.page { "gj_navDotBtn_on_001.png" } else { "gj_navDotBtn_off_001.png" };
                ctx.button(c, Btn::GarageDot(i as u8), name, x0 + i as f32 * 24.4, 25.0, Look::default(), 1.0);
            }
        }
    }
}

/// `CharacterColorPage::colorForIndex` (table at 0x1008fbc08): the palette color of each of the
/// page's 143 slots (rows of 18), −1 for a gap.
const COLOR_SLOTS: [i8; 143] = [
    51, 19, 48, 9, 62, 63, 10, 29, -1, 70, 42, 11, 27, 72, 73, 0, 1, -1, 37, 53, 54, 55, 26, 59, 60, 61, -1, 71, 14, 31, 45, 105, 28, 32,
    20, -1, 25, 56, 57, 58, 30, 64, 65, 66, -1, 46, 67, 68, 69, 2, 38, 79, 80, -1, 74, 75, 44, 3, 83, 16, 4, 5, -1, 52, 41, 6, 35, 98,
    8, 36, 103, -1, 40, 76, 77, 78, 22, 39, 84, 50, -1, 47, 23, 92, 93, 7, 13, 24, 104, -1, 33, 21, 81, 82, 34, 85, 86, 87, -1, 49, 95,
    96, 97, 43, 99, 100, 101, -1, -1, -1, -1, -1, 106, 88, 89, 90, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, 12, 91, 17, 102, 18, 94, 15,
];

/// `CharacterColorPage::offsetForIndex` (0x64b60c): a slot's shift in units of 12 points: the
/// second and fourth block of each row a unit right (table at 0x1008fbe44), rows 3–7 0.8 down,
/// the last (gray) row 0.4 up.
fn slot_offset(i: usize) -> (f32, f32) {
    let col = i % 18;
    let x = if (4..8).contains(&col) || (13..17).contains(&col) || i >= 136 { 1.0 } else { 0.0 };
    let y = if i >= 136 {
        0.4
    } else if (54..136).contains(&i) {
        -0.8
    } else {
        0.0
    };
    (x, y)
}

/// A slot's center relative to the page's color menu origin (screen center, 59 down).
fn slot_position(i: usize) -> (f32, f32) {
    let (ox, oy) = slot_offset(i);
    let (col, row) = ((i % 18) as f32, (i / 18) as f32);
    (col * 24.0 - 204.0 + 6.0 + ox * 12.0, 96.0 - row * 24.0 + oy * 12.0)
}

/// `CharacterColorPage::init` (0x649ac0): GJ_square05 450 × 300 over the kit, the close button,
/// the mode buttons (Col1 / Col2 / Glow, bigFont on GJ_button_01, 0.85) at the top right, a strip
/// (square02b, 420 × 60) with the 8 vehicles in the player's colors (1.15, 52 apart), and the
/// color menu (`createColorMenu` 0x64aac0): ColorChannelSprite swatches (0.65), the chosen ones
/// in GJ_select_001 brackets (0.7). Every color is unlocked in Cadence.
fn draw_colors(c: &mut Canvas, ctx: &mut Ctx, set: &IconSet, which: u8) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
    let (pw, ph) = (450.0, 300.0);
    c.nine_slice("GJ_square05.png", cx, cy, pw, ph, Look::default());
    ctx.button(c, Btn::GarageColorClose, "GJ_closeBtn_001.png", cx - pw * 0.5 + 10.0, cy + ph * 0.5 - 10.0, Look::scaled(0.8), 1.6);
    for (i, label) in ["Col1", "Col2", "Glow"].iter().enumerate() {
        let id = Btn::GarageColorTab(i as u8 + 1);
        let (x, y) = (cx + pw * 0.5 - 137.0 + 50.0 * i as f32, cy + ph * 0.5 - 17.0);
        let f = ctx.factor(id) * 0.85;
        let sprite = if which == i as u8 + 1 { "GJ_button_01.png" } else { "GJ_button_04.png" };
        c.nine_slice(sprite, x, y, 48.0 * f, 25.0 * f, Look::default());
        c.text("bigFont", label, x, y + 1.0, Look::scaled(0.4 * f));
        ctx.area(id, x, y, 48.0 * 0.85, 25.0 * 0.85);
    }
    // the vehicles strip
    let sy = cy + ph * 0.5 - 62.0;
    c.nine_slice("square02b_001.png", cx, sy, pw - 30.0, 60.0, Look::default().rgb([0, 0, 0]).alpha(45));
    let (c1, c2) = set.colors();
    for k in 0..8 {
        let base = if k == 5 || k == 6 { format!("{}_01", set.sheet(k)) } else { set.sheet(k) };
        let x = cx + 52.0 * k as f32 - 182.0;
        let (fw, fh) = c.size(&format!("{base}_001.png"));
        let fit = if fw.max(fh) > 0.0 { (40.0 / fw.max(fh)).min(1.15) } else { 1.15 };
        draw_icon(c, &set.sheet(k), x, sy, fit, c1, c2);
    }
    // the color menu
    let (mx, my) = (cx, cy - 59.0);
    for (i, &col) in COLOR_SLOTS.iter().enumerate() {
        if col < 0 {
            continue;
        }
        let (dx, dy) = slot_position(i);
        let (x, y) = (mx + dx, my + dy);
        let rgb = color_for_idx(col as usize);
        ctx.button(c, Btn::GarageColor(col as u8), "GJ_colorBtn_001.png", x, y, Look::scaled(0.65).rgb(rgb), 1.0);
        if col as usize == set.color1 || col as usize == set.color2 {
            c.sprite("GJ_select_001.png", x, y, Look::scaled(0.7));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_sheet_names() {
        let g = Garage::default();
        assert_eq!(g.pages(), 14); // 485 cubes
        assert_eq!(icon_sheet(2, 7), "player_ball_07");
        assert_eq!(icon_sheet(0, 123), "player_123");
        let set = IconSet::default();
        assert_eq!(set.colors(), ([125, 255, 0], [0, 255, 255]));
        assert_eq!(g.wanted_sheets(&set).len(), ICON_KINDS + 36);
    }
}

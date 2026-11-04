//! The level select: `LevelSelectLayer::init` (0x39707c), its pages (`LevelPage::init`
//! 0x39a850, `updateDynamicPage` 0x398008) in a looping `BoomScrollLayer`, and the official
//! levels' data (`LevelTools::getLevel` 0x457480).

use super::canvas::{Canvas, Look, ease_elastic_out, ease_in_out};
use super::menu::color_for_idx;
use super::save::Progress;
use super::{Btn, Ctx};
use crate::render::Blend;

/// An official level as `LevelTools::getLevel` builds it.
#[derive(Clone, Copy, Debug)]
pub struct LevelInfo {
    pub id: i32,
    pub name: &'static str,
    /// 1 easy, 2 normal, 3 hard, 4 harder, 5 insane, 6 demon (`diffIcon_%02d_btn_001.png`).
    pub difficulty: u32,
    pub stars: u32,
    /// Secret coins needed to unlock it (+0x454).
    pub required_coins: u32,
}

const LEVELS: [LevelInfo; 23] = {
    const fn l(id: i32, name: &'static str, difficulty: u32, stars: u32, required_coins: u32) -> LevelInfo {
        LevelInfo { id, name, difficulty, stars, required_coins }
    }
    [
        l(1, "Stereo Madness", 1, 1, 0),
        l(2, "Back On Track", 1, 2, 0),
        l(3, "Polargeist", 2, 3, 0),
        l(4, "Dry Out", 2, 4, 0),
        l(5, "Base After Base", 3, 5, 0),
        l(6, "Cant Let Go", 3, 6, 0),
        l(7, "Jumper", 4, 7, 0),
        l(8, "Time Machine", 4, 8, 0),
        l(9, "Cycles", 4, 9, 0),
        l(10, "xStep", 5, 10, 0),
        l(11, "Clutterfunk", 5, 11, 0),
        l(12, "Theory of Everything", 5, 12, 0),
        l(13, "Electroman Adventures", 5, 10, 0),
        l(14, "Clubstep", 6, 14, 10),
        l(15, "Electrodynamix", 5, 12, 0),
        l(16, "Hexagon Force", 5, 12, 0),
        l(17, "Blast Processing", 4, 10, 0),
        l(18, "Theory of Everything 2", 6, 14, 20),
        l(19, "Geometrical Dominator", 4, 10, 0),
        l(20, "Deadlocked", 6, 15, 30),
        l(21, "Fingerdash", 5, 12, 0),
        l(22, "Dash", 5, 12, 0),
        l(3001, "The Challenge", 3, 3, 0),
    ]
};

const TOWER: [(i32, &str); 4] = [(5001, "The Tower"), (5002, "The Sewers"), (5003, "The Cellar"), (5004, "The Secret Hollow")];

pub fn level_info(id: i32) -> LevelInfo {
    if let Some(l) = LEVELS.iter().find(|l| l.id == id) {
        return *l;
    }
    let name = TOWER.iter().find(|t| t.0 == id).map_or("Level", |t| t.1);
    LevelInfo { id, name, difficulty: 0, stars: 0, required_coins: 0 }
}

/// Orbs a main level awards: `GameStatsManager::getBaseCurrencyForLevel` (0x301f0: 400 for
/// Clubstep, Theory of Everything 2 and Deadlocked, else 20 + 20 per star) and the 2.2 bonus of a
/// quarter more (the real level page and end screen show 50 for Stereo Madness's base 40).
pub fn orbs_for(l: &LevelInfo) -> u32 {
    let base = if matches!(l.id, 14 | 18 | 20) { 400 } else { l.stars * 20 + 20 };
    base * 5 / 4
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Level(i32),
    /// Page −2: The Tower's door.
    Tower,
    /// Page −1: "Coming Soon!" and the hidden door.
    ComingSoon,
}

/// The pages in order: levels 1–22, the Tower, Coming Soon (24, looping).
pub const PAGES: [Page; 24] = {
    let mut p = [Page::ComingSoon; 24];
    let mut i = 0;
    while i < 22 {
        p[i] = Page::Level(i as i32 + 1);
        i += 1;
    }
    p[22] = Page::Tower;
    p
};

/// `LevelSelectLayer::colorForPage` (0x399c60): page 22 (the Tower) is (37, 44, 52), the others
/// cycle through color indices (5, 7, 8, 9, 10, 11, 1, 3, 4).
fn color_for_page(page: i32) -> [u8; 3] {
    if page != 0 && page % 22 == 0 {
        return [37, 44, 52];
    }
    const TABLE: [usize; 9] = [5, 7, 8, 9, 10, 11, 1, 3, 4];
    color_for_idx(TABLE[page.rem_euclid(9) as usize])
}

/// `LevelSelectLayer::getColorValue` (0x3999bc): between two pages the color holds for the
/// first and last fifth of the way and blends linearly in the middle (with the game's double
/// rounding).
fn color_between(frac: f32, a: i32, b: i32) -> [u8; 3] {
    let f = frac / 0.6666667 + -0.2;
    let d: f64 = if f > 0.0 && f < 1.0 { f as f64 } else if f <= 0.0 { 0.0 } else { 1.0 };
    let k: f32 = if d <= 0.2 { 0.0 } else if d < 0.8 { ((d - 0.2) / 0.6) as f32 } else { 1.0 };
    let (ca, cb) = (color_for_page(a), color_for_page(b));
    [0, 1, 2].map(|i| (k * cb[i] as f32 + (1.0 - k) * ca[i] as f32) as i32 as u8)
}

/// A page move (`BoomScrollLayer::moveToPage`): CCMoveTo with CCEaseElasticOut (period 0.5).
#[derive(Clone, Copy, Debug)]
struct Move {
    from: f32,
    to: f32,
    t: f32,
    duration: f32,
}

#[derive(Debug, Default)]
pub struct LevelSelect {
    /// The scroll in pages (page i is centered at scroll = i); unbounded, pages loop.
    scroll: f32,
    target: i32,
    anim: Option<Move>,
    dragging: bool,
    time: f32,
    /// Seconds since the hidden door on the Coming Soon page was revealed.
    pub door_revealed: Option<f32>,
}

impl LevelSelect {
    pub fn enter(&mut self) {
        self.anim = None;
        self.scroll = self.target as f32;
    }

    /// The page shown (0..24).
    pub fn page(&self) -> usize {
        self.target.rem_euclid(PAGES.len() as i32) as usize
    }

    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// Open on a level's page (coming back from it).
    pub fn show_level(&mut self, id: i32) {
        if let Some(i) = PAGES.iter().position(|p| *p == Page::Level(id)) {
            self.jump_to(i);
        } else if (5001..=5004).contains(&id) {
            self.jump_to(22);
        } else if id == 3001 {
            self.jump_to(23);
        }
    }

    /// `instantMoveToPage`.
    pub fn jump_to(&mut self, page: usize) {
        self.target = page as i32;
        self.scroll = page as f32;
        self.anim = None;
    }

    /// onPrev / onNext: `moveToPage(current ± 1)`; 0.96 s for a button press.
    pub fn move_by(&mut self, d: i32) {
        self.move_to(self.target + d, 0.96);
    }

    fn move_to(&mut self, page: i32, duration: f32) {
        self.target = page;
        self.anim = Some(Move { from: self.scroll, to: page as f32, t: 0.0, duration });
    }

    pub fn drag_to(&mut self, scroll: f32) {
        self.dragging = true;
        self.anim = None;
        self.scroll = scroll;
    }

    /// The end of a drag: past a fifth of a page moves on (faster, 0.48 s), else springs back.
    pub fn release_drag(&mut self, moved_pages: f32) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        let page = if moved_pages < -0.2 {
            self.target + 1
        } else if moved_pages > 0.2 {
            self.target - 1
        } else {
            self.target
        };
        self.move_to(page, if page == self.target { 0.72 } else { 0.48 });
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some(t) = self.door_revealed.as_mut() {
            *t += dt;
        }
        if let Some(m) = self.anim.as_mut() {
            m.t += dt;
            let k = ease_elastic_out(m.t / m.duration, 0.5);
            self.scroll = m.from + (m.to - m.from) * k;
            if m.t >= m.duration {
                self.scroll = m.to;
                self.anim = None;
                // keep the numbers small: pages loop
                let n = PAGES.len() as i32;
                let wrap = self.target.div_euclid(n) * n;
                self.target -= wrap;
                self.scroll -= wrap as f32;
            }
        }
    }

    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx, progress: &Progress) {
        let (w, h) = (c.width, c.height);
        let n = PAGES.len() as i32;
        // scrollLayerMoved (0x3997f4): the color between the two pages in view.
        let s = self.scroll.rem_euclid(n as f32);
        let page = s as i32;
        let next = if page == n - 1 { 0 } else { page + 1 };
        let color = color_between(s - page as f32, page, next);
        // The gradient background (z −2), (w + 10) × (h + 10) from (−5, −5).
        let (gw, gh) = c.size("GJ_gradientBG.png");
        if gw > 0.0 {
            c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb(color));
        }
        // The ground (z 0) at min(h/2 − 110, 128), colored 0.8 × the background.
        let gy = (h * 0.5 - 110.0).min(128.0);
        let g = color.map(|v| (v as f32 * 0.8) as i32 as u8);
        let (tw, _) = c.size("groundSquare_01_001.png");
        if tw > 0.0 {
            let mut x = 0.0;
            while x < w {
                c.sprite("groundSquare_01_001.png", x, gy, Look::default().anchor(0.0, 1.0).rgb(g));
                x += tw;
            }
        }
        c.sprite("floorLine_01_001.png", w * 0.5, gy + 0.5, Look::default().anchor(0.5, 1.0).blend(Blend::Additive));
        let shade = Look::default().anchor(0.0, 1.0).scale_xy(0.7, 1.0).alpha(100).blend(Blend::Shade);
        c.sprite("groundSquareShadow_001.png", -1.0, gy, shade);
        c.sprite("groundSquareShadow_001.png", w + 1.0, gy, Look { anchor: (1.0, 1.0), flip_x: true, ..shade });

        // The pages in view (each a full screen wide).
        let first = self.scroll.floor() as i32;
        for p in [first - 1, first, first + 1, first + 2] {
            let ox = (p as f32 - self.scroll) * w;
            if ox <= -w || ox >= w {
                continue;
            }
            let index = p.rem_euclid(n) as usize;
            let current = p == self.target && self.anim.is_none() && !self.dragging;
            self.draw_page(c, ctx, PAGES[index], ox, progress, current);
        }

        // The page dots (BoomScrollLayer::updateDots): 16 apart at (w/2, bottom + 15); the
        // current one white, the rest (125, 125, 125).
        let current = (s + 0.5).floor() as i32 % n;
        for i in 0..n {
            let x = w * 0.5 + (i as f32 + (n as f32 - 1.0) * -0.5) * 16.0;
            let col = if i == current { [255, 255, 255] } else { [125, 125, 125] };
            c.sprite("uiDot_001.png", x, 15.0, Look::default().rgb(col));
        }
        // "Download the soundtrack" (bigFont 0.5) at (w/2, bottom + 35).
        let size = c.text_size("bigFont", "Download the soundtrack");
        let f = ctx.factor(Btn::LevelsDownload);
        c.text("bigFont", "Download the soundtrack", w * 0.5, 35.0, Look::scaled(0.5 * f));
        ctx.area(Btn::LevelsDownload, w * 0.5, 35.0, size.0 * 0.5 * 1.1, size.1 * 0.5 * 1.1);
        // The top bar (z 1) and the side art in both bottom corners.
        c.sprite("GJ_topBar_001.png", w * 0.5, h + 1.0, Look::default().anchor(0.5, 1.0));
        c.sprite("GJ_sideArt_001.png", -1.0, -1.0, Look::default().anchor(0.0, 0.0));
        c.sprite("GJ_sideArt_001.png", w + 1.0, -1.0, Look::default().anchor(1.0, 0.0).flip_x(true));
        // The arrows (z 5, size mult 2) at (left + 25, h/2) and (right − 25, h/2).
        ctx.button(c, Btn::LevelsPrev, "navArrowBtn_001.png", 25.0, h * 0.5, Look::default().flip_x(true), 2.0);
        ctx.button(c, Btn::LevelsNext, "navArrowBtn_001.png", w - 25.0, h * 0.5, Look::default(), 2.0);
        // Back (size mult 1.6) at (left + 25, top − 22).
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 25.0, h - 22.0, Look::default(), 1.6);
        // Info at (right − 20, top − 20); the death-effect button 40 below it, only on level pages.
        ctx.button(c, Btn::LevelsInfo, "GJ_infoIcon_001.png", w - 20.0, h - 20.0, Look::default(), 2.0);
        if matches!(PAGES[self.page()], Page::Level(_)) {
            ctx.button(c, Btn::LevelsDeath, "sideBtn_death_001.png", w - 20.0, h - 60.0, Look::default(), 2.0);
        }
    }

    fn draw_page(&self, c: &mut Canvas, ctx: &mut Ctx, page: Page, ox: f32, progress: &Progress, current: bool) {
        let (w, h) = (c.width, c.height);
        let cx = ox + w * 0.5;
        match page {
            Page::Tower => {
                // updateDynamicPage, page −2: the door (bobbing 5 up and down over 2.5 s each,
                // CCEaseInOut 1.8) at (w/2, h/2 − 5) with its label 23 above its top.
                let cycle = self.time % 5.0;
                let bob = if cycle < 2.5 { ease_in_out(cycle / 2.5, 1.8) } else { 1.0 - ease_in_out((cycle - 2.5) / 2.5, 1.8) } * 5.0;
                let (dw, dh) = c.size("theTowerDoor_001.png");
                let f = ctx.factor(Btn::TowerDoor);
                let y = h * 0.5 - 5.0 + bob;
                c.sprite("theTowerDoor_001.png", cx, y, Look::scaled(f));
                c.sprite("theTowerLabel_001.png", cx, y + (dh * 0.5 + 23.0) * f, Look::scaled(f));
                if current {
                    ctx.area(Btn::TowerDoor, cx, y, dw, dh);
                }
            }
            Page::ComingSoon => {
                c.text("bigFont", "Coming Soon!", cx, h * 0.5 + 50.0, Look::default());
                // addSecretDoor: at (w/2, min(h/2 − 110, 128) + 30), invisible until found.
                let gy = h * 0.5 - 110.0;
                let y = if gy <= 128.0 { gy + 30.0 } else { 158.0 };
                let (dw, dh) = c.size("secretDoorBtn2_closed_001.png");
                let found = progress.flag("door");
                if found {
                    // onSecretDoor: scale 2 → 1 (CCEaseBounceOut, 0.5 s)
                    let t = self.door_revealed.unwrap_or(1.0);
                    let s = 2.0 - super::canvas::ease_bounce_out(t / 0.5);
                    let f = ctx.factor(Btn::SecretDoor);
                    c.sprite("secretDoorBtn2_closed_001.png", cx, y, Look::scaled(s * f));
                }
                if current {
                    ctx.area(Btn::SecretDoor, cx, y, dw * 2.0, dh * 2.0);
                }
            }
            Page::Level(id) => self.draw_level_page(c, ctx, &level_info(id), ox, progress, current),
        }
    }

    fn draw_level_page(&self, c: &mut Canvas, ctx: &mut Ctx, info: &LevelInfo, ox: f32, progress: &Progress, current: bool) {
        let (w, h) = (c.width, c.height);
        let cx = ox + w * 0.5;
        let p = progress.get(info.id);
        // The level box: a 340 × 95 CCScale9Sprite (square02_001, opacity 125) in a menu item
        // at (w/2, h/2 + 60) that grows to 1.1 when pressed.
        let (bw, bh) = (340.0f32, 95.0f32);
        let f = if current { ctx.factor(Btn::LevelBox) } else { 1.0 };
        let (bx, by) = (cx, h * 0.5 + 60.0);
        c.nine_slice("square02_001.png", bx, by, bw * f, bh * f, Look::default().alpha(125));
        if current {
            ctx.area(Btn::LevelBox, bx, by, bw, bh);
        }
        // box-local point → screen, scaled about the box center
        let at = |x: f32, y: f32| (bx + (x - bw * 0.5) * f, by + (y - bh * 0.5) * f);
        let locked = progress.secret_coins() < info.required_coins;
        if locked {
            let (x, y) = at(bw * 0.5, bh * 0.5);
            c.sprite("GJLargeLock_001.png", x, y, Look::scaled(f));
            let (x, y) = at(bw - 15.0, 15.0);
            c.sprite("GJ_coinsIcon_001.png", x, y, Look::scaled(f));
            let (lx, ly) = at(bw - 30.0, 15.5);
            c.text("bigFont", &format!("{}/{}", progress.secret_coins(), info.required_coins), lx, ly, Look::scaled(0.7 * f).anchor(1.0, 0.5));
        } else {
            // The name (bigFont, at most 250 wide) right of the difficulty face (1.1), the pair
            // centered: face x = half its width + (W/2 − (half face + name + 35)/2).
            let (nw, _) = c.text_size("bigFont", info.name);
            let ns = if nw > 250.0 { 250.0 / nw } else { 1.0 };
            let face = format!("diffIcon_{:02}_btn_001.png", info.difficulty);
            let (fw, _) = c.size(&face);
            let half = fw * 1.1 * 0.5;
            let face_x = half + (bw * 0.5 - (half + ns * nw + 5.0 + 30.0) * 0.5);
            let mid = bh * 0.5 + 3.0;
            let (x, y) = at(face_x, mid);
            c.sprite(&face, x, y, Look::scaled(1.1 * f));
            let (x, y) = at(face_x + 30.0, mid);
            c.text("bigFont", info.name, x, y, Look::scaled(ns * f).anchor(0.0, 0.5));
            // Stars: icon (0.7) at (W − 15, H − 13), count (0.5, right-aligned) 12 left of it;
            // yellow (255, 255, 50) once completed.
            let (x, y) = at(bw - 15.0, bh - 13.0);
            c.sprite("GJ_starsIcon_001.png", x, y, Look::scaled(0.7 * f));
            let star_color = if p.best >= 100 { [255, 255, 50] } else { [255, 255, 255] };
            let (x, y) = at(bw - 15.0 - 12.0, bh - 13.0 + 0.5);
            c.text("bigFont", &info.stars.to_string(), x, y, Look::scaled(0.5 * f).anchor(1.0, 0.5).rgb(star_color));
            // The three coins at (W − 16 − 52/26/0, 16): gold if collected, else the gray
            // icon tinted (225, 225, 225).
            for i in 0..3 {
                let (x, y) = at(bw - 16.0 - 26.0 * (2 - i) as f32, 16.0);
                if p.coin(i) {
                    c.sprite("GJ_coinsIcon_001.png", x, y, Look::scaled(f));
                } else {
                    c.sprite("GJ_coinsIcon_gray_001.png", x, y, Look::scaled(f).rgb([225, 225, 225]));
                }
            }
            // Orbs: "got/total" (0.4) at (8, 12), the orb icon (0.7) 9 past its end. Main levels
            // award their orbs on completion.
            let total = orbs_for(info);
            let got = if p.best >= 100 { total } else { 0 };
            let label = format!("{got}/{total}");
            let (lw, _) = c.text_size("bigFont", &label);
            let (x, y) = at(8.0, 12.0);
            c.text("bigFont", &label, x, y, Look::scaled(0.4 * f).anchor(0.0, 0.5));
            let (x, y) = at(8.0 + lw * 0.4 + 9.0, 11.5);
            c.sprite("currencyOrbIcon_001.png", x, y, Look::scaled(0.7 * f));
        }
        // The progress bars: GJ_progressBar_001 in black at opacity 125, the fill (scale 0.992 ×
        // 0.86, anchored at its left middle) green for normal mode, cyan for practice.
        let bars = [(h * 0.5 - 30.0, p.best, [0, 255, 0], "Normal Mode"), (h * 0.5 - 80.0, p.practice, [0, 255, 255], "Practice Mode")];
        for (y, pct, color, title) in bars {
            let (pw, _) = c.size("GJ_progressBar_001.png");
            c.sprite("GJ_progressBar_001.png", cx, y, Look::default().rgb([0, 0, 0]).alpha(125));
            let left = cx - pw * 0.5 + (pw - pw * 0.992) * 0.5;
            c.bar("GJ_progressBar_001.png", left, y, pct as f32 / 100.0, Look::default().anchor(0.0, 0.5).scale_xy(0.992, 0.86).rgb(color));
            c.text("bigFont", &format!("{pct}%"), cx, y, Look::scaled(0.5));
            c.text("bigFont", title, cx, y + 20.0, Look::scaled(0.5));
        }
    }
}

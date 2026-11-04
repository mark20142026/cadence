//! The Treasure Room (`SecretRewardsLayer`, layouts `scenes/treasure0–4.txt` from real
//! captures): five pages (the 1/5/10-key chests, the 25/50/100-key chests, the bonuses for
//! 50/100/200 chests opened, the gold-key chest, the ropes to the four shops). A tier opens
//! its grid of chests, twelve a page (`createSecondaryLayer`); a chest costs its tier's keys
//! (`keyCostForSecretChest`) and holds what the game's own table says (`chests_table`).

use super::canvas::{Canvas, Look};
use super::chests_table::{SPECIAL, TREASURE};
use super::replay::{self, Scene};
use super::save::Progress;
use super::{Btn, Ctx};

#[derive(Clone, Debug, Default)]
pub struct Treasure {
    /// The main layer's page (0–4).
    pub page: u8,
    /// The open tier's grid (3–8) and its page.
    pub tier: Option<u8>,
    pub grid_page: usize,
}

pub enum Effect {
    None,
    Leave,
    /// The Keymaster says something
    Say(String),
    /// A chest opened: its art and contents
    Open(u8, Vec<(u8, u8, u16, u32)>),
    /// A rope: that shop
    Shop(u8),
}

/// Keys a tier's chest costs.
pub fn cost(tier: u8) -> i64 {
    match tier {
        3 => 1,
        4 => 5,
        5 => 10,
        6 => 25,
        7 => 50,
        _ => 100,
    }
}

/// `SecretRewardsLayer::getPageColor`.
fn page_color(page: u8) -> [u8; 3] {
    match page {
        1 => [50, 50, 50],
        2 => [70, 0, 120],
        3 => [36, 27, 40],
        _ => [70, 0, 75],
    }
}

pub fn chests(tier: u8) -> impl Iterator<Item = &'static (u16, u8, &'static [(u8, u8, u16, u32)])> {
    TREASURE.iter().filter(move |c| c.1 == tier)
}

pub fn opened(p: &Progress, id: u16) -> bool {
    p.flag(&format!("chest {id}"))
}

fn opened_count(p: &Progress, tier: Option<u8>) -> usize {
    TREASURE.iter().filter(|c| tier.is_none_or(|t| c.1 == t) && opened(p, c.0)).count()
}

/// The bonuses (`onSpecialItem`): (chests to have opened, special chest key).
const BONUSES: [(usize, &str); 3] = [(50, "0006"), (100, "0005"), (200, "0004")];

fn special(key: &str) -> &'static [(u8, u8, u16, u32)] {
    SPECIAL.iter().find(|s| s.0 == key).map_or(&[], |s| s.1)
}

/// The chest art of a tier (`chest_03` … `chest_09`).
fn art(tier: u8) -> u8 {
    tier.clamp(3, 9)
}

/// A chest as `GJChestSprite` shows it: closed, or opened and dimmed (`switchToState(4)`).
fn draw_chest(c: &mut Canvas, tier: u8, x: f32, y: f32, s: f32, open: bool) {
    let n = art(tier);
    if open {
        let look = Look::scaled(s).rgb([150, 150, 150]).alpha(100);
        c.sprite(&format!("chest_{n:02}_04_back_001.png"), x, y, look);
        c.sprite(&format!("chest_{n:02}_04_001.png"), x, y, look);
    } else {
        c.sprite(&format!("chest_{n:02}_02_001.png"), x, y, Look::scaled(s));
    }
}

fn draw_dots(c: &mut Canvas, n: usize, at: usize, x0: f32, y: f32) {
    for i in 0..n {
        let col = if i == at { [255, 255, 255] } else { [125, 125, 125] };
        c.sprite("smallDot.png", x0 + 16.0 * i as f32, y, Look::default().rgb(col));
    }
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, t: &Treasure, p: &Progress) {
    let dx = (c.width - 569.0) * 0.5;
    let gold = t.page == 3;
    if let Some(tier) = t.tier {
        draw_grid(c, ctx, t, tier, p);
        return;
    }
    replay::draw_where(c, ctx, Scene::Treasure(t.page), &|k, name, _, y| match k {
        // the counts and the page dots are this save's
        'T' => !(name.contains('/') || y > 290.0),
        'S' => !(name == "smallDot.png" && y < 20.0),
        _ => true,
    });
    let keys = p.currency(if gold { 43 } else { 21 });
    c.text("bigFont", &keys.to_string(), 514.0 + dx, 302.0, Look::scaled(0.6).anchor(1.0, 0.5));
    draw_dots(c, 5, t.page as usize, 252.5 + dx, 10.0);
    let tan = [187, 122, 107];
    match t.page {
        0 | 1 => {
            for k in 0..3 {
                let tier = 3 + t.page * 3 + k;
                let total = chests(tier).count();
                let text = format!("{} / {total}", opened_count(p, Some(tier)));
                c.text("bigFont", &text, 184.5 + 100.0 * k as f32 + dx, 81.0, Look::scaled(0.4).rgb(tan));
            }
        }
        2 => {
            let text = format!("{}/{}", opened_count(p, None), TREASURE.len());
            c.text("bigFont", &text, 294.5 + dx, 232.0, Look::scaled(0.5).anchor(1.0, 0.5));
            for (k, (_, key)) in BONUSES.iter().enumerate() {
                if p.flag(&format!("special {key}")) {
                    // the claimed bonus chest, opened over the captured closed one
                    let x = 184.5 + 100.0 * k as f32 + dx;
                    c.sprite("chestPlatform_01_001.png", x, 136.0, Look::default().rgb(page_color(2)).alpha(255));
                    draw_chest(c, 6, x, 170.0, 0.6, true);
                }
            }
        }
        3 => {
            let total = chests(9).count();
            let text = format!("{} / {total}", opened_count(p, Some(9)));
            c.text("bigFont", &text, 284.5 + dx, 232.0, Look::scaled(0.5).rgb([100, 87, 112]));
        }
        _ => {}
    }
}

const PER_PAGE: usize = 12;

fn grid_pos(i: usize) -> (f32, f32) {
    (149.5 + 90.0 * (i % 4) as f32, 240.0 - 80.0 * (i / 4) as f32)
}

/// `createSecondaryLayer`: the tier's chests on a dark panel, "opened/total" at the top.
fn draw_grid(c: &mut Canvas, ctx: &mut Ctx, t: &Treasure, tier: u8, p: &Progress) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    let bg = page_color(0);
    c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / c.size("GJ_gradientBG.png").0.max(1.0), (h + 10.0) / c.size("GJ_gradientBG.png").1.max(1.0)).rgb(bg));
    c.sprite("treasureRoomSpiderweb_001.png", w + 5.0, 325.0, Look::scaled(1.1).anchor(1.0, 1.0).rgb([0, 0, 0]).alpha(63));
    c.sprite("treasureRoomSpiderweb_001.png", -5.0, 325.0, Look::scaled(1.1).flip_x(true).anchor(0.0, 1.0).rgb([0, 0, 0]).alpha(63));
    c.nine_slice("GJ_square05.png", 284.5 + dx, 160.0, 370.0, 255.0, Look::default().rgb(bg).alpha(75));
    for x in [184.5, 384.5] {
        c.sprite("chain_01_001.png", x + dx, 285.5, Look::default().anchor(0.5, 0.0).rgb([0, 0, 0]).alpha(75));
    }
    let all: Vec<_> = chests(tier).collect();
    c.text("goldFont", &format!("{}/{}", opened_count(p, Some(tier)), all.len()), 284.5 + dx, 304.5, Look::scaled(0.6));
    for (i, chest) in all.iter().skip(t.grid_page * PER_PAGE).take(PER_PAGE).enumerate() {
        let (x, y) = grid_pos(i);
        let id = Btn::TreasureChest(i as u8);
        let f = ctx.factor(id);
        draw_chest(c, tier, x + dx, y, 0.55 * f, opened(p, chest.0));
        ctx.area(id, x + dx, y, 55.0, 44.0);
    }
    let pages = all.len().div_ceil(PER_PAGE);
    if pages > 1 {
        ctx.button(c, Btn::TreasurePrev, "GJ_arrow_01_001.png", 24.0 + dx, 130.0, Look::default(), 1.0);
        ctx.button(c, Btn::TreasureNext, "GJ_arrow_01_001.png", 545.0 + dx, 130.0, Look::default().flip_x(true), 1.0);
    }
    draw_dots(c, pages, t.grid_page, 284.5 - 8.0 * (pages as f32 - 1.0) + dx, 10.0);
    ctx.button(c, Btn::TreasureClose, "GJ_closeBtn_001.png", 24.0 + dx, 297.0, Look::default(), 1.5);
    c.text("bigFont", &p.currency(21).to_string(), 514.0 + dx, 302.0, Look::scaled(0.6).anchor(1.0, 0.5));
    c.sprite("GJ_bigKey_001.png", 539.0 + dx, 301.5, Look::default());
}

impl Treasure {
    /// A click on the main layer's button at (x, y) of the page's layout.
    pub fn click(&mut self, (x, y): (f32, f32), p: &mut Progress) -> Effect {
        let near = |a: f32, b: f32| (x - a).abs() < 1.0 && (y - b).abs() < 1.0;
        if near(24.0, 297.0) {
            return Effect::Leave;
        }
        if near(24.0, 130.0) {
            self.page = (self.page + 4) % 5;
            return Effect::None;
        }
        if near(545.0, 130.0) {
            self.page = (self.page + 1) % 5;
            return Effect::None;
        }
        let slot = ((x - 184.5) / 100.0).round().clamp(0.0, 2.0) as usize;
        match self.page {
            0 | 1 => {
                self.tier = Some(3 + self.page * 3 + slot as u8);
                self.grid_page = 0;
                Effect::None
            }
            2 => {
                let (need, key) = BONUSES[slot];
                let have = opened_count(p, None);
                if p.flag(&format!("special {key}")) {
                    Effect::None
                } else if have < need {
                    Effect::Say(format!("Unlock <cy>{}</c> more <cg>chests</c>, and you can collect this bonus.", need - have))
                } else {
                    p.set_flag(&format!("special {key}"));
                    let items = special(key).to_vec();
                    for &(item, k, id, n) in &items {
                        super::paths::add_reward(p, item, k, id, n);
                    }
                    Effect::Open(6, items)
                }
            }
            3 => {
                // `onGoldChest`: the next unopened one, for a gold key (stat 43)
                let Some(chest) = chests(9).find(|c| !opened(p, c.0)) else { return Effect::None };
                if p.currency(43) < 1 {
                    return Effect::Say("You need a <cy>gold key</c> to unlock this <cg>chest</c>.".into());
                }
                p.add_currency(43, -1);
                Self::open(p, chest)
            }
            _ => {
                // the ropes, left to right: the secret shop, the community shop, the mechanic,
                // the diamond shop
                let rope = ((x - 179.5) / 70.0).round().clamp(0.0, 3.0) as u8;
                Effect::Shop(1 + rope)
            }
        }
    }

    fn open(p: &mut Progress, chest: &(u16, u8, &[(u8, u8, u16, u32)])) -> Effect {
        p.set_flag(&format!("chest {}", chest.0));
        for &(item, k, id, n) in chest.2 {
            super::paths::add_reward(p, item, k, id, n);
        }
        Effect::Open(art(chest.1), chest.2.to_vec())
    }

    /// The grid's buttons (`onSelectItem`: a chest for its tier's keys).
    pub fn click_grid(&mut self, id: Btn, p: &mut Progress) -> Effect {
        let Some(tier) = self.tier else { return Effect::None };
        let pages = chests(tier).count().div_ceil(PER_PAGE).max(1);
        match id {
            Btn::TreasureClose => self.tier = None,
            Btn::TreasurePrev => self.grid_page = (self.grid_page + pages - 1) % pages,
            Btn::TreasureNext => self.grid_page = (self.grid_page + 1) % pages,
            Btn::TreasureChest(i) => {
                let Some(chest) = chests(tier).nth(self.grid_page * PER_PAGE + i as usize) else { return Effect::None };
                if opened(p, chest.0) {
                    return Effect::None;
                }
                let need = cost(tier);
                let have = p.currency(21);
                if have < need {
                    return Effect::Say(format!("You need <cl>{}</c> more <cy>keys</c> to unlock this <cg>chest</c>", need - have));
                }
                p.add_currency(21, -need);
                return Self::open(p, chest);
            }
            _ => {}
        }
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chests_cost_keys() {
        assert_eq!([3, 4, 5, 6, 7, 8, 9].map(|t| chests(t).count()), [400, 100, 60, 24, 12, 8, 30]);
        let mut p = Progress::default();
        let mut t = Treasure { tier: Some(4), ..Treasure::default() };
        p.add_currency(21, 7);
        assert!(matches!(t.click_grid(Btn::TreasureChest(0), &mut p), Effect::Open(4, _)));
        assert_eq!(p.currency(21), 2);
        assert!(opened(&p, 1001));
        assert!(matches!(t.click_grid(Btn::TreasureChest(1), &mut p), Effect::Say(ref s) if s.contains("<cl>3</c>")));
        // the bonus wants 50 chests opened
        let mut t = Treasure { page: 2, ..Treasure::default() };
        assert!(matches!(t.click((184.5, 170.0), &mut p), Effect::Say(ref s) if s.contains("<cy>49</c>")));
    }
}

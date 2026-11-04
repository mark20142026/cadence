//! Paths of Power (`GJPathsLayer`, `GJPathPage`; layouts from real captures in
//! shots/gui-real/paths). Each path is the store's item 121–130 (25,000 Mana Orbs, unlocked
//! with "Do you want to unlock The Path of …"); one unlocked path is active and the stars and
//! moons you collect level it (its stat 30–39, up to 1000: `incrementActivePath`). Rank k
//! (k × 100) is the achievement `pathNN.k`, whose icon is the reward shown on the page; at
//! 1000 the path's chest (`pr_N`, contents dumped from the game) can be claimed once.

use super::canvas::{Canvas, Look};
use super::menu::color_for_idx;
use super::save::Progress;
use super::{Btn, Ctx};

pub const NAMES: [&str; 10] = ["Fire", "Ice", "Poison", "Shadow", "Lava", "Earth", "Blood", "Metal", "Light", "Souls"];

/// Each page's color (pathInnerBG's tint) and its icons' two colors, from the real pages.
const COLORS: [([u8; 3], [u8; 3], [u8; 3]); 10] = [
    ([255, 130, 74], [255, 75, 0], [255, 200, 0]),
    ([59, 133, 207], [0, 150, 255], [0, 255, 255]),
    ([73, 160, 56], [0, 160, 0], [0, 255, 0]),
    ([143, 79, 194], [150, 0, 200], [255, 50, 255]),
    ([196, 74, 61], [175, 0, 0], [255, 50, 25]),
    ([165, 84, 51], [163, 98, 70], [253, 224, 160]),
    ([230, 34, 34], [150, 0, 0], [255, 0, 0]),
    ([204, 204, 204], [128, 128, 128], [255, 255, 255]),
    ([255, 178, 79], [255, 250, 127], [255, 255, 192]),
    ([194, 69, 153], [100, 0, 150], [255, 0, 125]),
];

/// The paths' chests at 1000 (`pr_1` … `pr_10`): (special reward item, unlock type, id,
/// count), the game's `SpecialRewardItem`s: 1–5 / 10–14 shards, 6 a key, 8 diamonds, 9 an
/// icon or color.
pub const CHESTS: [&[(u8, u8, u16, u32)]; 10] = [
    &[(9, 13, 19, 1), (8, 0, 0, 300), (1, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 8, 48, 1), (9, 2, 83, 1), (2, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 6, 146, 1), (8, 0, 0, 300), (3, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 8, 40, 1), (9, 2, 104, 1), (4, 0, 0, 10)],
    &[(9, 4, 94, 1), (8, 0, 0, 300), (5, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 4, 91, 1), (9, 2, 61, 1), (10, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 4, 98, 1), (8, 0, 0, 300), (11, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 1, 359, 1), (9, 3, 102, 1), (12, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 8, 66, 1), (8, 0, 0, 300), (13, 0, 0, 10), (6, 0, 0, 1)],
    &[(9, 13, 36, 1), (9, 2, 76, 1), (14, 0, 0, 10), (6, 0, 0, 1)],
];

/// The store item that unlocks path `i` (0-based).
pub fn store_item(i: usize) -> u16 {
    121 + i as u16
}

pub fn unlocked(p: &Progress, i: usize) -> bool {
    super::shop::bought(p, store_item(i))
}

/// Stars and moons collected into path `i` (0–1000).
pub fn progress(p: &Progress, i: usize) -> u32 {
    p.currency(30 + i as u32).clamp(0, 1000) as u32
}

pub fn rank(p: &Progress, i: usize) -> u32 {
    progress(p, i) / 100
}

/// `GameStatsManager::trySelectActivePath`: the chosen path while it's unlocked and not at
/// 1000, else the first unlocked one that isn't.
pub fn active(p: &Progress) -> Option<usize> {
    let chosen = (p.number("activepath", 0.0) as i32 - 30).clamp(-1, 9);
    let open = |i: usize| unlocked(p, i) && progress(p, i) < 1000;
    if chosen >= 0 && open(chosen as usize) {
        return Some(chosen as usize);
    }
    (0..10).find(|&i| open(i))
}

pub fn set_active(p: &mut Progress, i: usize) {
    p.set_number("activepath", 30.0 + i as f32);
}

/// Stars or moons collected: they level the active path (`incrementActivePath`).
pub fn collect(p: &mut Progress, n: i64) {
    if n <= 0 {
        return;
    }
    if let Some(i) = active(p) {
        let have = p.currency(30 + i as u32);
        p.add_currency(30 + i as u32, n.min(1000 - have));
    }
}

/// Path `i`'s eleven rewards (rank 0 to 10): the reward icons of its achievements.
pub fn rewards(i: usize) -> Vec<&'static str> {
    (0..=10)
        .filter_map(|k| {
            let id = format!("geometry.ach.path{:02}.{k:02}", i + 1);
            super::achievements_table::ACHIEVEMENTS.iter().find(|a| a.0 == id).map(|a| a.4)
        })
        .collect()
}

pub fn chest_claimed(p: &Progress, i: usize) -> bool {
    p.flag(&format!("pathchest {}", i + 1))
}

/// Claim path `i`'s chest: its currencies and shards go into the stats. Returns its contents.
pub fn claim(p: &mut Progress, i: usize) -> Option<&'static [(u8, u8, u16, u32)]> {
    if progress(p, i) < 1000 || chest_claimed(p, i) {
        return None;
    }
    p.set_flag(&format!("pathchest {}", i + 1));
    for &(item, kind, id, n) in CHESTS[i] {
        add_reward(p, item, kind, id, n);
    }
    Some(CHESTS[i])
}

/// A `SpecialRewardItem` into the stats (`GameStatsManager::preProcessReward`): orbs 14/22,
/// diamonds 13/29, keys 21, shards 16–20 and 23–27. Icons and colors are kept as owned.
pub fn add_reward(p: &mut Progress, item: u8, kind: u8, id: u16, n: u32) {
    let n = n as i64;
    match item {
        1..=5 => p.add_currency(15 + item as u32, n),
        10..=14 => p.add_currency(13 + item as u32, n),
        6 => p.add_currency(21, n),
        7 => {
            p.add_currency(14, n);
            p.add_currency(22, n);
        }
        8 => {
            p.add_currency(13, n);
            p.add_currency(29, n);
        }
        9 => p.set_flag(&format!("owned {kind} {id}")),
        _ => {}
    }
}

/// A reward string from the achievements (`icon_213`, `color_48`, `robot_35` …) drawn as the
/// path page shows it: in the path's colors when reached, else a dark silhouette under a lock.
fn draw_reward(c: &mut Canvas, reward: &str, x: f32, y: f32, scale: f32, colors: Option<([u8; 3], [u8; 3])>, dark: [u8; 3]) {
    let Some((kind, n)) = reward.rsplit_once('_') else { return };
    let n: u32 = n.parse().unwrap_or(1);
    let start = c.mark();
    match kind {
        "color" | "color2" => {
            let col = colors.map_or([0, 0, 0], |_| color_for_idx(n as usize));
            c.sprite("player_special_01_001.png", x, y, Look::scaled(scale).rgb(col));
        }
        "special" => c.sprite(&format!("player_special_{n:02}_001.png"), x, y, Look::scaled(scale).rgb(colors.map_or([0, 0, 0], |_| [255, 255, 255]))),
        "death" => c.sprite(&format!("explosionIcon_{n:02}_001.png"), x, y, Look::scaled(scale).rgb(colors.map_or([0, 0, 0], |_| [255, 255, 255]))),
        _ => {
            let Some(sheet) = super::achievements::reward_sheet(reward) else { return };
            let dy = if kind == "bird" { -7.0 * scale } else { 0.0 };
            match colors {
                Some((c1, c2)) => super::garage::draw_icon(c, &sheet, x, y + dy, scale, c1, c2),
                // robots and spiders in the bar's dark color, the rest a black main layer
                None if kind == "robot" || kind == "spider" => super::garage::draw_icon(c, &sheet, x, y + dy, scale, dark, dark),
                None => {
                    let base = if kind == "robot" || kind == "spider" { format!("{sheet}_01") } else { sheet };
                    c.sprite(&format!("{base}_001.png"), x, y + dy, Look::scaled(scale).rgb([0, 0, 0]));
                }
            }
        }
    }
    if colors.is_none() {
        let a = if kind == "robot" || kind == "spider" { 190.0 } else { 120.0 };
        c.fade_since(start, a / 255.0);
        c.sprite("GJ_lockGray_001.png", x, y, Look::scaled(scale).rgb([175, 175, 175]).alpha(200));
    }
}

/// The icon sheets a page (or the layer, None) needs.
pub fn wanted_sheets(page: Option<usize>) -> Vec<String> {
    let Some(i) = page else { return Vec::new() };
    rewards(i).iter().filter_map(|r| super::achievements::reward_sheet(r)).collect()
}

/// `GJPathsLayer`: the ten paths around the circle, locked ones gray under a lock, unlocked
/// ones with their rank.
pub fn draw_layer(c: &mut Canvas, ctx: &mut Ctx, p: &Progress) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 175.0 / 255.0);
    c.sprite("pathSelectBG_001.png", cx, cy, Look::scaled(1.2));
    ctx.button(c, Btn::DailyClose, "GJ_closeBtn_001.png", cx - 120.0, cy + 120.0, Look::default(), 1.5);
    for i in 0..10 {
        let a = (90.0 - 36.0 * i as f32).to_radians();
        let (x, y) = (cx + 100.0 * a.cos(), cy + 100.0 * a.sin());
        let id = Btn::PathIcon(i as u8);
        let f = ctx.factor(id);
        let frame = format!("pathIcon_{:02}_001.png", i + 1);
        if unlocked(p, i) {
            c.sprite(&frame, x, y, Look::scaled(f));
            c.text("bigFont", &rank(p, i).to_string(), x, y + f, Look::scaled(0.55 * f));
        } else {
            c.sprite(&frame, x, y, Look::scaled(f).rgb([40, 40, 40]).gray());
            c.sprite("GJ_lockGray_001.png", x, y, Look::scaled(f).rgb([200, 200, 200]));
        }
        ctx.area(id, x, y, 46.0, 60.0);
    }
    c.sprite("label_paths_001.png", cx, cy + 29.0, Look::scaled(0.8));
    c.sprite("label_of_001.png", cx, cy, Look::scaled(0.8));
    c.sprite("label_power_001.png", cx, cy - 34.0, Look::scaled(0.8));
}

/// `GJPathPage` for path `i`: its eleven rewards over the bar, the rank badge, and either
/// the Unlock button with its price or the progress, Active, and the chest.
pub fn draw_page(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, i: usize) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    let at = |x: f32| x + dx;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    let open = unlocked(p, i);
    let (bg, c1, c2) = COLORS[i];
    let dark = if open { bg.map(|v| (v as f32 * 0.22) as u8) } else { [30, 30, 30] };
    // locked: the stone in grayscale (`CCSpriteGrayscale`), as the real page shows it
    c.sprite("pathInnerBG_001.png", at(284.5), 160.0, if open { Look::scaled(1.3).rgb(bg) } else { Look::scaled(1.3).gray().rgb([130, 130, 130]) });
    c.sprite("whiteSquare20_001.png", at(114.5), 129.15, Look::default().scale_xy(34.0, 0.5).anchor(0.0, 0.5).rgb(dark));
    let r = if open { rank(p, i) as usize } else { 0 };
    let prog = if open { progress(p, i) } else { 0 };
    for k in 0..11 {
        let x = at(114.5 + 34.0 * k as f32);
        c.sprite("whiteSquare20_001.png", x, 153.0, Look::default().scale_xy(0.15, 1.6).anchor(0.5, 1.0).rgb([0, 0, 0]).alpha(25));
        c.sprite("smallDot.png", x, 129.15, Look::default().rgb(dark));
    }
    if open && prog > 0 {
        c.sprite("whiteSquare20_001.png", at(116.5), 129.15, Look::default().scale_xy(33.6 * prog as f32 / 1000.0, 0.2).anchor(0.0, 0.5).rgb(bg));
    }
    if open {
        for k in 0..=r.min(10) {
            c.sprite("smallDot.png", at(114.5 + 34.0 * k as f32), 129.15, Look::scaled(0.7).rgb(bg));
        }
    }
    let done = open && prog >= 1000;
    let claimed = chest_claimed(p, i);
    let chest = Btn::PathChest;
    let f = if done && !claimed { ctx.factor(chest) } else { 1.0 };
    c.sprite("chestIcon_001.png", at(470.82), 129.15, Look::scaled(f).rgb(if done { [255, 255, 255] } else { dark }).alpha(if done && !claimed { 255 } else { 200 }));
    if done && !claimed {
        ctx.area(chest, at(470.82), 129.15, 30.0, 30.0);
    }
    // the header: the path's badge (its rank) and name
    let badge = format!("pathIcon_{:02}_001.png", i + 1);
    if open {
        c.sprite(&badge, at(364.75), 222.85, Look::default());
        c.text("bigFont", &r.to_string(), at(364.75), 223.85, Look::scaled(0.55));
    } else {
        c.sprite(&badge, at(364.75), 222.85, Look::default().rgb([40, 40, 40]).gray());
        c.sprite("GJ_lockGray_001.png", at(364.75), 222.85, Look::default().rgb([200, 200, 200]));
    }
    c.sprite(&format!("pathLabel_{:02}_001.png", i + 1), at(257.25), 220.85, if open { Look::default() } else { Look::default().gray().rgb([227, 227, 227]) });
    ctx.button(c, Btn::PathBack, "backArrowPlain_01_001.png", at(81.67), 218.85, Look::default(), 1.0);
    ctx.button(c, Btn::PathInfo, "GJ_infoIcon_001.png", at(485.32), 216.85, Look::scaled(0.8), 1.5);
    for (k, reward) in rewards(i).iter().enumerate() {
        let x = at(114.5 + 34.0 * k as f32);
        draw_reward(c, reward, x, 170.0, 0.8, (open && k <= r).then_some((c1, c2)), dark);
        ctx.area(Btn::PathReward(k as u8), x, 170.0, 30.0, 30.0);
    }
    if open {
        let text = format!("{prog}/1000");
        let (tw, _) = c.text_size("goldFont", &text);
        let (rw, _) = c.text_size("goldFont", "350/1000");
        let lx = at(272.5);
        let gap = 43.3 - rw * 0.3;
        c.text("goldFont", &text, lx, 108.15, Look::scaled(0.6));
        c.sprite("GJ_starsIcon_001.png", lx + tw * 0.3 + gap, 106.65, Look::scaled(0.5));
        c.sprite("GJ_moonsIcon_001.png", lx + tw * 0.3 + gap + 12.0, 106.65, Look::scaled(0.55));
        let on = active(p) == Some(i);
        ctx.button(c, Btn::PathActive, if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, at(86.67), 81.15, Look::scaled(0.6), 1.2);
        c.text("goldFont", "Active", at(103.17), 81.65, Look::scaled(0.5).anchor(0.0, 0.5));
    } else {
        ctx.button(c, Btn::PathUnlock, "GJ_lock_001.png", at(284.5), 104.15, Look::scaled(1.2), 1.0);
        let price = super::shop::item(store_item(i)).map_or(0, |it| it.3);
        let text = super::shop::points(price);
        let (tw, _) = c.text_size("bigFont", &text);
        c.text("bigFont", &text, at(279.7), 79.85, Look::scaled(0.42));
        c.sprite("currencyOrbIcon_001.png", at(279.7) + tw * 0.21 + 17.0, 79.25, Look::scaled(0.6));
    }
}

/// `GJPathRewardPopup`: "Reward!" over the path's chest (`chest_06_02`); a tap opens it.
pub fn draw_reward_popup(c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
    let f = ctx.factor(Btn::PathRewardOpen);
    c.text("goldFont", "Reward!", cx, cy + 45.0, Look::scaled(0.65).rgb([0, 0, 0]).alpha(50));
    c.sprite("chest_06_02_001.png", cx, cy - 1.0, Look::scaled(0.6 * f).rgb([0, 0, 0]).alpha(50));
    c.text("goldFont", "Reward!", cx, cy + 50.0, Look::scaled(0.65));
    c.sprite("chest_06_02_001.png", cx, cy + 4.0, Look::scaled(0.6 * f));
    ctx.area(Btn::PathRewardOpen, cx, cy, 100.0, 100.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_level_like_the_game() {
        let mut p = Progress::default();
        assert_eq!(active(&p), None);
        // stars with no path unlocked go nowhere
        collect(&mut p, 10);
        assert_eq!(progress(&p, 0), 0);
        p.set_flag("bought 122");
        assert_eq!(active(&p), Some(1));
        collect(&mut p, 350);
        assert_eq!((progress(&p, 1), rank(&p, 1)), (350, 3));
        p.set_flag("bought 121");
        set_active(&mut p, 0);
        collect(&mut p, 2000);
        assert_eq!(progress(&p, 0), 1000);
        // a finished path hands over to the next unlocked one
        assert_eq!(active(&p), Some(1));
        assert_eq!(rewards(0)[0], "icon_213");
        assert_eq!(rewards(0).len(), 11);
        let before = p.currency(13);
        assert!(claim(&mut p, 0).is_some());
        assert_eq!(p.currency(13) - before, 300);
        assert_eq!(p.currency(16), 10);
        assert!(claim(&mut p, 0).is_none());
    }
}

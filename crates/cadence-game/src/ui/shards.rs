//! Shards of Power (`ShardsPage`, layouts measured from real captures in
//! shots/gui-real/shards): four pages of three shard kinds; each kind's five icons are its
//! achievements' rewards (`shardFire01`–`05` …: 5, 15, 35, 65, 100 shards), lit in the kind's
//! colors once reached, with the progress line between them.

use super::canvas::{Canvas, Look};
use super::save::Progress;
use super::{Btn, Ctx};

/// A row: (achievement name, label sprite, small shard sprite, stats (the bonus rows: the
/// fewest of their tier), threshold color, icon colors).
type Kind = (&'static str, &'static str, &'static str, &'static [u32], [u8; 3], ([u8; 3], [u8; 3]));

const W: [u8; 3] = [255, 255, 255];
pub const KINDS: [Kind; 12] = [
    ("Fire", "fireShardLabel_001.png", "fireShardSmall_001.png", &[16], [255, 255, 0], ([255, 75, 0], [255, 200, 0])),
    ("Ice", "iceShardLabel_001.png", "iceShardSmall_001.png", &[17], [100, 255, 255], ([0, 150, 255], [0, 255, 255])),
    ("Poison", "poisonShardLabel_001.png", "poisonShardSmall_001.png", &[18], [75, 255, 75], ([0, 160, 0], [0, 255, 0])),
    ("Shadow", "shadowShardLabel_001.png", "shadowShardSmall_001.png", &[19], [255, 100, 255], ([150, 0, 200], [255, 50, 255])),
    ("Lava", "lavaShardLabel_001.png", "lavaShardSmall_001.png", &[20], [255, 50, 50], ([175, 0, 0], [255, 50, 25])),
    ("Bonus", "bonusShardLabel_001.png", "bonusShardSmall_001.png", &[16, 17, 18, 19, 20], W, ([150, 150, 150], W)),
    ("Earth", "shard0201ShardLabel_001.png", "shard0201ShardSmall_001.png", &[23], W, ([163, 98, 70], [253, 224, 160])),
    ("Blood", "shard0202ShardLabel_001.png", "shard0202ShardSmall_001.png", &[24], W, ([150, 0, 0], [255, 0, 0])),
    ("Metal", "shard0203ShardLabel_001.png", "shard0203ShardSmall_001.png", &[25], W, ([128, 128, 128], W)),
    ("Light", "shard0204ShardLabel_001.png", "shard0204ShardSmall_001.png", &[26], W, ([255, 250, 127], [255, 255, 192])),
    ("Soul", "shard0205ShardLabel_001.png", "shard0205ShardSmall_001.png", &[27], W, ([100, 0, 150], [255, 0, 125])),
    ("BonusB", "bonusShardLabel_001.png", "bonusShard2Small_001.png", &[23, 24, 25, 26, 27], W, ([150, 150, 150], W)),
];

pub const STEPS: [i64; 5] = [5, 15, 35, 65, 100];
pub const PAGES: usize = 4;

/// Shards of a kind (the bonus rows: the fewest of their tier's five).
pub fn count(p: &Progress, kind: usize) -> i64 {
    KINDS[kind].3.iter().map(|&s| p.currency(s)).min().unwrap_or(0)
}

/// A row's five rewards (its achievements' icons).
pub fn rewards(kind: usize) -> Vec<&'static str> {
    (1..=5)
        .filter_map(|k| {
            let id = format!("geometry.ach.shard{}{k:02}", KINDS[kind].0);
            super::achievements_table::ACHIEVEMENTS.iter().find(|a| a.0 == id).map(|a| a.4)
        })
        .collect()
}

pub fn wanted_sheets(page: usize) -> Vec<String> {
    (0..3).flat_map(|r| rewards(page * 3 + r)).filter_map(super::achievements::reward_sheet).collect()
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, page: usize) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    let at = |x: f32| x + dx;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square01.png", at(284.5), 160.0, 420.0, 280.0, Look::default());
    c.sprite("label_shards_001.png", at(204.5), 272.0, Look::default());
    c.sprite("label_of_001.png", at(288.5), 272.0, Look::default());
    c.sprite("label_power_001.png", at(369.5), 272.0, Look::default());
    for r in 0..3 {
        let kind = page * 3 + r;
        let (_, label, small, _, color, (c1, c2)) = KINDS[kind];
        let have = count(p, kind);
        let y = 240.0 - 77.0 * r as f32;
        let (lw, _) = c.size(label);
        c.sprite(label, at(264.5), y, Look::default());
        let lx = at(264.5) + lw * 0.5 + 4.25;
        let text = have.to_string();
        let (tw, _) = c.text_size("bigFont", &text);
        c.text("bigFont", &text, lx, y + 1.0, Look::scaled(0.45).anchor(0.0, 0.5));
        c.sprite(small, lx + tw * 0.45 + 2.5, y + 1.0, Look::default().anchor(0.0, 0.5));
        let iy = y - 30.0;
        // the line: dark, then lit up to the shards you have
        for k in 0..4 {
            let x = at(163.5 + 70.0 * k as f32);
            c.sprite("whiteSquare20_001.png", x, iy, Look::default().scale_xy(3.2, 0.5).anchor(0.0, 0.5).rgb([0, 0, 0]).alpha(110));
            let (a, b) = (STEPS[k], STEPS[k + 1]);
            let part = ((have - a) as f32 / (b - a) as f32).clamp(0.0, 1.0);
            if part > 0.0 {
                c.sprite("whiteSquare20_001.png", x + 2.0, iy, Look::default().scale_xy(2.8 * part, 0.2).anchor(0.0, 0.5).rgb(color));
            }
        }
        for (k, reward) in rewards(kind).iter().enumerate() {
            let x = at(144.5 + 70.0 * k as f32);
            let reached = have >= STEPS[k];
            super::paths::draw_reward(c, reward, x, iy, 0.8, reached.then_some((c1, c2)), [87, 48, 29]);
            ctx.area(Btn::ShardIcon((r * 5 + k) as u8), x, iy, 30.0, 30.0);
            if !reached {
                let t = STEPS[k].to_string();
                let tx = x - if k == 0 { 8.0 } else { 10.0 };
                let (ttw, _) = c.text_size("bigFont", &t);
                c.text("bigFont", &t, tx, iy - 22.0, Look::scaled(0.3).anchor(0.0, 0.5).rgb(color));
                c.sprite(small, tx + ttw * 0.3 + 1.5, iy - 22.0, Look::scaled(0.7).anchor(0.0, 0.5));
            }
        }
    }
    for (x, y, sx, sy) in [(99.5, 45.0, 1.0, 1.0), (99.5, 275.0, 1.0, -1.0), (469.5, 275.0, -1.0, -1.0), (469.5, 45.0, -1.0, 1.0)] {
        c.sprite("dailyLevelCorner_001.png", at(x), y, Look::default().scale_xy(sx, sy));
    }
    c.sprite(if page < 2 { "tier1Icon_001.png" } else { "tier2Icon_001.png" }, at(476.5), 282.0, Look::default());
    ctx.button(c, Btn::ShardsClose, "GJ_closeBtn_001.png", at(84.5), 290.0, Look::default(), 1.5);
    ctx.button(c, Btn::ShardsPage(false), "GJ_arrow_01_001.png", at(24.0), 160.0, Look::default(), 1.0);
    ctx.button(c, Btn::ShardsPage(true), "GJ_arrow_01_001.png", at(545.0), 160.0, Look::default().flip_x(true), 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shards_rows() {
        assert!(KINDS.iter().enumerate().all(|(i, _)| rewards(i).len() == 5));
        let mut p = Progress::default();
        for s in [16, 17, 18, 19] {
            p.add_currency(s, 20);
        }
        assert_eq!(count(&p, 5), 0);
        p.add_currency(20, 7);
        assert_eq!(count(&p, 5), 7);
        assert!(super::super::achievements::unlocked("geometry.ach.shardBonus01", "", &p));
        assert!(!super::super::achievements::unlocked("geometry.ach.shardBonus02", "", &p));
    }
}

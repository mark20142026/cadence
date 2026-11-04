//! `RewardUnlockLayer` (0x1b07d0): a chest opening. The dark backdrop and the box (340 × 220,
//! GJ_square02) fade in, the chest drops in with a bounce (1 s), shakes three times (±4,
//! 0.05 s a move), opens (`chest_NN_03` with its back and glow) and its rewards come out one
//! by one with their "+N", then the collect button closes it. Also `AchievementBar`: the
//! banner an achievement drops from the top with its reward.

use super::canvas::{self, Canvas, Look};
use super::{Btn, Ctx};

pub struct Unlock {
    /// The chest's art (`chest_NN_…`): 1 / 2 the small and big daily chests, 6 a path's.
    pub chest: u8,
    /// (special reward item, unlock type, id, count), as `paths::CHESTS`.
    pub items: Vec<(u8, u8, u16, u32)>,
    pub t: f32,
}

const DROP: f32 = 1.0;
const SHAKE: f32 = DROP + 0.3;
const OPEN: f32 = SHAKE + 0.5;
const EACH: f32 = 0.25;

impl Unlock {
    /// When the collect button shows (every reward out).
    fn done_at(&self) -> f32 {
        OPEN + EACH * self.items.len() as f32 + 0.4
    }

    pub fn can_close(&self) -> bool {
        self.t >= self.done_at()
    }
}

/// The sprite a reward item comes out as.
fn item_frame(item: u8) -> &'static str {
    match item {
        1 => "fireShardBig_001.png",
        2 => "iceShardBig_001.png",
        3 => "poisonShardBig_001.png",
        4 => "shadowShardBig_001.png",
        5 => "lavaShardBig_001.png",
        10 => "shard0201ShardBig_001.png",
        11 => "shard0202ShardBig_001.png",
        12 => "shard0203ShardBig_001.png",
        13 => "shard0204ShardBig_001.png",
        14 => "shard0205ShardBig_001.png",
        6 => "GJ_bigKey_001.png",
        15 => "GJ_bigGoldKey_001.png",
        7 => "currencyOrb_001.png",
        _ => "GJ_bigDiamond_001.png",
    }
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, u: &Unlock) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    let fade = (u.t / 0.2).min(1.0);
    c.rect(0.0, 0.0, w, h, [0.0; 3], fade * 150.0 / 255.0);
    let start = c.mark();
    c.nine_slice("GJ_square02.png", cx, cy, 340.0, 220.0, Look::default());
    c.fade_since(start, fade);
    // the drop (CCMoveTo 1 s, bounce out) and the shakes
    let chest_y = cy - 30.0;
    let drop = canvas::ease_bounce_out((u.t / DROP).min(1.0));
    let y = h + 80.0 + (chest_y - h - 80.0) * drop;
    let mut x = cx;
    if (DROP..SHAKE).contains(&u.t) {
        x += if ((u.t - DROP) / 0.05) as i32 % 2 == 0 { 4.0 } else { -4.0 };
    }
    let shadow = Look::scaled(0.6 + 0.35 * drop).alpha((90.0 * drop) as u8);
    c.sprite("chest_shadow_001.png", cx, chest_y - 32.0, shadow);
    let n = u.chest.clamp(1, 6);
    if u.t < OPEN {
        c.sprite(&format!("chest_{n:02}_02_001.png"), x, y, Look::default());
    } else {
        let glow = ((u.t - OPEN) / 0.3).min(1.0);
        c.sprite(&format!("chest_{n:02}_03_back_001.png"), x, y, Look::default());
        c.sprite(&format!("chest_{n:02}_03_glow_001.png"), x, y + 10.0, Look::scaled(1.0 + 0.2 * glow).alpha((255.0 * glow) as u8).blend(crate::render::Blend::Additive));
        c.sprite(&format!("chest_{n:02}_03_001.png"), x, y, Look::default());
    }
    // the rewards rise out of the chest into a row above it
    let count = u.items.len().max(1) as f32;
    for (k, &(item, kind, id, num)) in u.items.iter().enumerate() {
        let t0 = OPEN + EACH * k as f32;
        if u.t < t0 {
            continue;
        }
        let p = canvas::ease_elastic_out(((u.t - t0) / 0.6).min(1.0), 0.5);
        let tx = cx + (k as f32 - (count - 1.0) * 0.5) * 70.0;
        let ty = cy + 55.0;
        let (ix, iy) = (cx + (tx - cx) * p, chest_y + (ty - chest_y) * p);
        if item == 9 {
            super::shop::draw_item(c, kind, id, ix, iy, 0.9 * p.max(0.01), None);
        } else {
            c.sprite(item_frame(item), ix, iy, Look::scaled(0.8 * p.max(0.01)));
        }
        if u.t >= t0 + 0.3 {
            c.text("goldFont", &format!("+{num}"), ix, iy - 30.0, Look::scaled(0.6));
        }
    }
    if u.can_close() {
        let f = ctx.factor(Btn::RewardCollect);
        c.sprite("GJ_rewardBtn_001.png", cx, cy - 110.0, Look::scaled(f));
        let (bw, bh) = c.size("GJ_rewardBtn_001.png");
        ctx.area(Btn::RewardCollect, cx, cy - 110.0, bw, bh);
    }
}

/// An achievement's banner (`AchievementBar`, captured): GJ_square01 300 × 80 at the top with
/// the glow behind, the reward on the left, the title (bigFont, at most 0.7) and "Unlocked new
/// …!" under it; it drops in, stays and goes back up.
pub struct Banner {
    pub title: String,
    pub reward: String,
    pub t: f32,
}

pub const BANNER_TIME: f32 = 3.0;

fn reward_noun(reward: &str) -> &'static str {
    match reward.rsplit_once('_').map_or("", |(k, _)| k) {
        "icon" => "Icon",
        "color" => "Main Color",
        "color2" => "Secondary Color",
        "ship" => "Ship",
        "ball" => "Ball",
        "bird" => "UFO",
        "dart" => "Wave",
        "robot" => "Robot",
        "spider" => "Spider",
        "swing" => "Swing",
        "jetpack" => "Jetpack",
        "special" => "Trail",
        "death" => "Death Effect",
        _ => "Item",
    }
}

pub fn draw_banner(c: &mut Canvas, b: &Banner) {
    let (w, h) = (c.width, c.height);
    let cx = w * 0.5;
    // in over 0.4 s, out over the last 0.4 s
    let inn = canvas::ease_bounce_out((b.t / 0.4).min(1.0));
    let out = ((b.t - (BANNER_TIME - 0.4)) / 0.4).clamp(0.0, 1.0);
    let y = (h - 48.0) + (1.0 - inn) * 100.0 + out * out * 100.0;
    c.sprite("achievementGlow_001.png", cx, y, Look::scaled(4.0).alpha(147).blend(crate::render::Blend::Additive));
    c.nine_slice("GJ_square01.png", cx, y, 300.0, 80.0, Look::default());
    let desc = format!("Unlocked new {}!", reward_noun(&b.reward));
    let (tw, _) = c.text_size("bigFont", &b.title);
    let ts = (200.0 / tw.max(1.0)).min(0.7);
    let (dw, _) = c.text_size("chatFont", &desc);
    let text_w = (tw * ts).max(dw);
    let ix = cx - (30.0 + text_w) * 0.5;
    c.text("bigFont", &b.title, ix + 30.0, y + 11.5, Look::scaled(ts).anchor(0.0, 0.5));
    c.text("chatFont", &desc, ix + 31.0, y - 3.75, Look::default().anchor(0.0, 1.0));
    let Some((kind, n)) = b.reward.rsplit_once('_') else { return };
    let n: u32 = n.parse().unwrap_or(1);
    match kind {
        "color" | "color2" => c.sprite("player_special_01_001.png", ix, y, Look::default().rgb(super::menu::color_for_idx(n as usize))),
        "special" => c.sprite(&format!("player_special_{n:02}_001.png"), ix, y, Look::default()),
        "death" => c.sprite(&format!("explosionIcon_{n:02}_001.png"), ix, y, Look::default()),
        _ => {
            if let Some(sheet) = super::achievements::reward_sheet(&b.reward) {
                super::garage::draw_icon(c, &sheet, ix, y, 1.0, [175, 175, 175], [255, 255, 255]);
            }
        }
    }
}

//! `AchievementsLayer`: a `GJDropDownLayer` listing the game's achievements ten to a page (rows 80
//! high: the reward or a lock, the title, the description), "a to b of n" and page arrows. The
//! list is the real game's (`achievements_table.rs`); what's unlocked comes from Cadence's save:
//! level completions and the counts in the descriptions (jumps, attempts, stars, coins,
//! demons). Online-only ones stay locked.

use super::achievements_table::ACHIEVEMENTS;
use super::canvas::{Canvas, Look};
use super::options::{LIST_H, dropdown_border, dropdown_frame};
use super::save::Progress;
use super::{Btn, Ctx};

pub const PER_PAGE: usize = 10;

pub fn pages() -> usize {
    ACHIEVEMENTS.len().div_ceil(PER_PAGE)
}

/// The first number in a description ("Jump 1000 times" → 1000; "a Demon" → 1).
fn threshold(text: &str) -> u32 {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(1)
}

/// Whether achievement `id` is unlocked by `p`.
pub fn unlocked(id: &str, unachieved: &str, p: &Progress) -> bool {
    // one-time achievements (the vaults' codes) are kept as flags
    if p.flags.contains(id) {
        return true;
    }
    let Some(rest) = id.strip_prefix("geometry.ach.") else { return false };
    // levelNNa: practice, levelNNb: normal (main levels)
    if let Some(l) = rest.strip_prefix("level").filter(|l| l.len() == 3) {
        let (n, kind) = l.split_at(2);
        let Ok(n) = n.parse::<i32>() else { return false };
        let lp = p.get(n);
        return match kind {
            "a" => lp.practice >= 100 || lp.best >= 100,
            "b" => lp.best >= 100,
            _ => false,
        };
    }
    // pathNN.MM: the path unlocked (MM 00) and at rank MM (MM × 100 stars and moons)
    if let Some((n, m)) = rest.strip_prefix("path").and_then(|r| r.split_once('.')) {
        let (Ok(n), Ok(m)) = (n.parse::<usize>(), m.parse::<u32>()) else { return false };
        let i = n.saturating_sub(1);
        return super::paths::unlocked(p, i) && super::paths::progress(p, i) >= m * 100;
    }
    // shardKindNN: that many of the kind's shards (5, 15, 35, 65, 100)
    if let Some(r) = rest.strip_prefix("shard") {
        let (name, n) = r.split_at(r.len().saturating_sub(2));
        let (Some(kind), Ok(n)) = (super::shards::KINDS.iter().position(|k| k.0 == name), n.parse::<usize>()) else { return false };
        return n >= 1 && super::shards::count(p, kind) >= super::shards::STEPS[(n - 1).min(4)];
    }
    let main = |id: &i32| (1..=22).contains(id) || *id == 3001;
    let done = || p.levels.iter().filter(move |(id, l)| main(id) && l.best >= 100).map(|(id, _)| *id);
    let need = threshold(unachieved);
    let have = if rest.starts_with("jump") {
        p.levels.values().map(|l| l.jumps).sum()
    } else if rest.starts_with("attempt") {
        p.levels.values().map(|l| l.attempts).sum()
    } else if rest.starts_with("stars") {
        done().map(|id| super::level_info(id).stars).sum()
    } else if rest.starts_with("coins") {
        p.secret_coins()
    } else if rest.starts_with("demon") {
        done().filter(|id| super::level_info(*id).difficulty == 6).count() as u32
    } else {
        return false;
    };
    have >= need
}

/// The reward's sprite sheet and frame prefix (`icon_05` → player_05): (sheet, kind).
pub fn reward_sheet(icon: &str) -> Option<String> {
    let (kind, n) = icon.rsplit_once('_')?;
    let n: u32 = n.parse().ok()?;
    let prefix = match kind {
        "icon" => "player",
        "ship" => "ship",
        "ball" => "player_ball",
        "bird" => "bird",
        "dart" => "dart",
        "robot" => "robot",
        "spider" => "spider",
        "swing" => "swing",
        "jetpack" => "jetpack",
        _ => return None,
    };
    Some(format!("{prefix}_{n:02}"))
}

/// Sheets the current page's rewards need.
pub fn wanted_sheets(page: usize) -> Vec<String> {
    ACHIEVEMENTS.iter().skip(page * PER_PAGE).take(PER_PAGE).filter_map(|a| reward_sheet(a.4)).collect()
}

pub fn max_scroll() -> f32 {
    (PER_PAGE as f32 * 80.0 - LIST_H).max(0.0)
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, t: f32, closing: bool, scroll: f32, page: usize) {
    let (w, h) = (c.width, c.height);
    let dy = dropdown_frame(c, ctx, p, t, closing, "Achievements");
    let dy_border = dy;
    let (lx, ly) = ((w - 356.0) * 0.5, (h - LIST_H) * 0.5 - 5.0 + dy);
    let top = ly + LIST_H + 6.0;
    let first = page * PER_PAGE;
    let last = (first + PER_PAGE).min(ACHIEVEMENTS.len());
    c.text("goldFont", &format!("{} to {} of {}", first + 1, last, ACHIEVEMENTS.len()), w - 10.0, h - 13.0 + dy, Look::scaled(0.6).anchor(1.0, 0.5));
    if page > 0 {
        ctx.button(c, Btn::AchPrev, "GJ_arrow_01_001.png", 24.5, h * 0.5 + dy, Look::default(), 1.5);
    }
    if page + 1 < pages() {
        ctx.button(c, Btn::AchNext, "GJ_arrow_01_001.png", w - 24.5, h * 0.5 + dy, Look::default().flip_x(true), 1.5);
    }
    for (row, a) in ACHIEVEMENTS[first..last].iter().enumerate() {
        let y = top - 80.0 * (row as f32 + 1.0) + scroll;
        if y + 80.0 <= ly || y >= top {
            continue;
        }
        let (y0, y1) = (y.max(ly), (y + 80.0).min(top));
        let shade = if row % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(lx, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
        if row > 0 && y + 80.0 < top {
            c.rect(lx, y + 80.0 - 0.5, 356.0, 1.0, [1.0; 3], 0.25);
        }
        let cy = y + 40.0;
        let (id, title, achieved, unachieved, icon) = *a;
        let done = unlocked(id, unachieved, p);
        // clip whole items to the list
        if cy + 30.0 < ly || cy - 30.0 > top {
            continue;
        }
        let visible = |yy: f32| yy > ly - 30.0 && yy < top + 30.0;
        if visible(cy) {
            let iy = cy - 4.0;
            match (done, reward_sheet(icon)) {
                (true, Some(sheet)) => super::garage::draw_icon(c, &sheet, lx + 35.0, iy, 0.9, [175, 175, 175], [255, 255, 255]),
                _ => {
                    // the locked item: a gray square with a black edge, the padlock over it
                    c.rect(lx + 35.0 - 14.5, iy - 14.5, 29.0, 29.0, [0.0; 3], 1.0);
                    c.rect(lx + 35.0 - 13.0, iy - 13.0, 26.0, 26.0, [0.69; 3], 1.0);
                    c.sprite("GJ_lock_001.png", lx + 35.0, iy, Look::scaled(0.85));
                }
            }
        }
        if visible(cy + 17.0) {
            c.text("goldFont", title, lx + 68.0, cy + 17.3, Look::scaled(0.7).anchor(0.0, 0.5));
        }
        let text = if done { achieved } else { unachieved };
        for (i, line) in super::wrap(c, text, 190.0, 0.62).iter().take(2).enumerate() {
            let ly2 = cy - 5.0 - i as f32 * 16.4;
            if visible(ly2) {
                c.text("chatFont", line, lx + 68.0, ly2, Look::scaled(0.98).anchor(0.0, 0.5));
            }
        }
    }
    dropdown_border(c, ctx, dy_border, "Achievements");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocks_follow_progress() {
        let mut p = Progress::default();
        assert_eq!(ACHIEVEMENTS.len(), 549);
        assert!(!unlocked("geometry.ach.level01b", "Complete 'Stereo Madness' in Normal mode", &p));
        p.entry(1).best = 100;
        p.entry(1).jumps = 1200;
        assert!(unlocked("geometry.ach.level01b", "", &p));
        assert!(unlocked("geometry.ach.level01a", "", &p));
        assert!(unlocked("geometry.ach.jump01", "Jump 1000 times", &p));
        assert!(!unlocked("geometry.ach.jump02", "Jump 10000 times", &p));
        assert_eq!(reward_sheet("icon_05").as_deref(), Some("player_05"));
    }
}

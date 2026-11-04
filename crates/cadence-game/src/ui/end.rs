//! The level-complete screen: `EndLevelLayer::customSetup` (0x41d404) in a `GJDropDownLayer`
//! (0x5d466c) with a `GJListLayer` frame (0x5bf304), dropped in by `EndLevelLayer::showLayer`
//! (0x41f698); and the "Level Complete!" text before it (`PlayLayer::showCompleteText` 0x78784).

use super::canvas::{Canvas, Look, ease_bounce_out, ease_elastic_out, ease_in};
use super::{Btn, Ctx, SessionView};

/// When the "Level Complete!" text appears, in seconds after the level ends (the end animation
/// flies the player into the portal for about a second first).
pub const COMPLETE_TEXT_AT: f32 = 1.0;
/// `showCompleteText` calls `showEndLayer` 1.5 s later.
pub const END_LAYER_AT: f32 = COMPLETE_TEXT_AT + 1.5;

#[derive(Clone, Copy, Debug, Default)]
pub struct End {
    /// Seconds since the layer was shown.
    t: f32,
    /// Coin sounds already played (bit per coin).
    played: u8,
}

impl End {
    /// Already dropped in and settled (screenshots).
    pub fn settled() -> End {
        End { t: 10.0, played: 7 }
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }

    /// Coins taken this attempt whose entrance just finished (their sound is due).
    pub fn due_sounds(&mut self, s: &SessionView) -> usize {
        let mut n = 0;
        let mut k = 0;
        for i in 0..s.coin_count.min(3) {
            if !s.taken_coins[i] {
                continue;
            }
            // playCoinEffect(0.7): coin k enters after 0.7 + 0.35·k, finishes 0.1 later.
            let at = 0.7 + 0.35 * k as f32 + 0.1;
            k += 1;
            if self.t >= at && self.played & (1 << i) == 0 {
                self.played |= 1 << i;
                n += 1;
            }
        }
        n
    }

    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx, s: &SessionView, hidden: bool) {
        let (w, h) = (c.width, c.height);
        // The hide button (the eye, top right): hides the layer; a click anywhere shows it again.
        ctx.button(c, Btn::EndHide, "hideBtn_001.png", w - 18.0, h - 15.0, Look::default().opacity(0.6), 1.5);
        if hidden {
            ctx.area(Btn::EndHide, w * 0.5, h * 0.5, w, h);
            return;
        }
        // The layer's color fades to opacity 100 over 1 s; the main layer drops from the top
        // (y = h → 0) with CCEaseBounceOut over 1 s.
        c.rect(0.0, 0.0, w, h, [0.0; 3], (self.t.min(1.0) * 100.0) / 255.0);
        let dy = h * (1.0 - ease_bounce_out(self.t / 1.0));

        // GJListLayer (356 × 230) at ((w − 356)/2, (h − 230)/2 − 5): black at 180, the table
        // top and bottom, the sides stretched to its height.
        let (lw, lh) = (356.0, 230.0);
        let (lx, ly) = ((w - lw) * 0.5, (h - lh) * 0.5 - 5.0 + dy);
        // The chains it hangs from, behind the frame (their ends hide under the top bar).
        for dx in [-156.0, 156.0] {
            c.sprite("chain_01_001.png", w * 0.5 + dx, ly + lh + 12.0, Look::default().anchor(0.5, 0.0));
        }
        c.rect(lx, ly, lw, lh, [0.0; 3], 180.0 / 255.0);
        let (_, sh) = c.size("GJ_table_side_001.png");
        if sh > 0.0 {
            c.sprite("GJ_table_side_001.png", lx - 20.0, ly, Look::default().anchor(0.0, 0.0).scale_xy(1.0, lh / sh));
            c.sprite("GJ_table_side_001.png", lx + lw + 20.0, ly, Look::default().anchor(1.0, 0.0).scale_xy(1.0, lh / sh).flip_x(true));
        }
        c.sprite("GJ_table_bottom_001.png", lx + lw * 0.5, ly - 10.0, Look::default());
        c.sprite("GJ_table_top_001.png", lx + lw * 0.5, ly + lh + 15.0, Look::default());

        // "Level Complete" (0.8) at (w/2, h/2 + 75).
        c.sprite("GJ_levelComplete_001.png", w * 0.5, h * 0.5 + 75.0 + dy, Look::scaled(0.8));
        // Attempts, jumps and time (goldFont 0.8), 24 apart from (w/2, h/2 + 35).
        let gold = Look::scaled(0.8);
        let y0 = h * 0.5 + 35.0 + dy;
        let attempts = format!("Attempts: {}", s.attempts);
        let (aw, _) = c.text("goldFont", &attempts, w * 0.5, y0, gold);
        c.text("goldFont", &format!("Jumps: {}", s.jumps), w * 0.5, y0 - 24.0, gold);
        let secs = s.time as i32;
        let time = if secs < 3600 { format!("{:02}:{:02}", secs / 60 % 60, secs % 60) } else { format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) };
        let label = format!("Time: {time}");
        let (tw, _) = c.text_size("goldFont", &label);
        // limitLabelWidth(150, 0.8)
        let ts = if tw * 0.8 > 150.0 { 150.0 / tw } else { 0.8 };
        c.text("goldFont", &label, w * 0.5, y0 - 48.0, Look::scaled(ts));
        // The death-effect button left of the attempts.
        ctx.button(c, Btn::EndDeath, "sideBtn_death_001.png", w * 0.5 - aw * 0.8 * 0.5 - 21.0, y0 - 2.0, Look::default(), 1.0);

        // A first completion's rewards, right of the stats: "+stars" and the big star at
        // (w/2 + 140, h/2 + 31), "+orbs" and the orb icon at (w/2 + 140, h/2 − 11) (measured on the
        // real screen as they settle).
        if let Some((stars, orbs)) = s.reward {
            let t = ((self.t - 0.5) / 0.4).clamp(0.0, 1.0);
            if t > 0.0 && stars > 0 {
                c.text("bigFont", &format!("+{stars}"), w * 0.5 + 96.0, h * 0.5 + 28.0 + dy, Look::scaled(0.9).rgb([210, 210, 210]).opacity(t));
                c.sprite("GJ_bigStar_001.png", w * 0.5 + 140.0, h * 0.5 + 31.0 + dy, Look::scaled(1.05).opacity(t));
            }
            if t > 0.0 && orbs > 0 {
                c.text("bigFont", &format!("+{orbs}"), w * 0.5 + 87.0, h * 0.5 - 11.0 + dy, Look::scaled(0.9).rgb([210, 210, 210]).opacity(t));
                c.sprite("currencyOrbIcon_001.png", w * 0.5 + 140.0, h * 0.5 - 11.0 + dy, Look::scaled(1.6).opacity(t));
            }
        }

        // The coins at (w/2, h/2 − 64), 20 apart: empty slots, the saved ones filled, and the
        // ones taken this attempt flying in (scale 3 → 1, CCEaseBounceOut 0.4 s, fade in 0.1 s,
        // 0.7 s + 0.35 s each after the layer appears).
        let n = s.coin_count.min(3);
        if n > 0 {
            let (cw, _) = c.size("secretCoin_b_01_001.png");
            let total = cw * n as f32 + 20.0 * (n - 1) as f32;
            let mut x = w * 0.5 - total * 0.5 + cw * 0.5;
            let y = h * 0.5 - 64.0 + dy;
            let mut k = 0;
            for i in 0..n {
                c.sprite("secretCoin_b_01_001.png", x, y, Look::default());
                if s.taken_coins[i] {
                    let start = 0.7 + 0.35 * k as f32;
                    k += 1;
                    let t = self.t - start;
                    if t > 0.0 {
                        let scale = 3.0 + (1.0 - 3.0) * ease_bounce_out(t / 0.4);
                        c.sprite("secretCoinUI_001.png", x, y, Look::scaled(scale).opacity((t / 0.1).min(1.0)));
                    }
                } else if s.saved_coins[i] {
                    c.sprite("secretCoinUI_001.png", x, y, Look::default());
                }
                x += cw + 20.0;
            }
        }

        // Replay and menu at (w/2 ∓ 100, h/2 − 125).
        let by = h * 0.5 - 125.0 + dy;
        ctx.button(c, Btn::EndReplay, "GJ_replayBtn_001.png", w * 0.5 - 100.0, by, Look::default(), 1.0);
        ctx.button(c, Btn::EndMenu, "GJ_menuBtn_001.png", w * 0.5 + 100.0, by, Look::default(), 1.0);
    }
}

/// `showCompleteText`: "Level Complete!" at (w/2, h/2 + 35), from scale 0.01 to 1.1 over
/// 0.66 s (CCEaseElasticOut 0.6), held 0.88 s, then back to 0.01 over 0.22 s (CCEaseIn 2).
pub fn draw_complete_text(c: &mut Canvas, end_time: f32) {
    let t = end_time - COMPLETE_TEXT_AT;
    if t < 0.0 {
        return;
    }
    let scale = if t < 0.66 {
        0.01 + (1.1 - 0.01) * ease_elastic_out(t / 0.66, 0.6)
    } else if t < 0.66 + 0.88 {
        1.1
    } else if t < 0.66 + 0.88 + 0.22 {
        1.1 + (0.01 - 1.1) * ease_in((t - 1.54) / 0.22, 2.0)
    } else {
        return;
    };
    c.sprite("GJ_levelComplete_001.png", c.width * 0.5, c.height * 0.5 + 35.0, Look::scaled(scale));
}

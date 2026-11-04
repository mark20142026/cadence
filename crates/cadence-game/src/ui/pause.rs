//! The pause menu: `PauseLayer::customSetup` (0x344754) and its progress bar
//! (`FUN_1003474bc`).

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx, SessionView};

#[derive(Clone, Copy, Debug, Default)]
pub struct Pause;

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, level_name: &str, s: &SessionView, volumes: (f32, f32), hidden: bool) {
    let (w, h) = (c.width, c.height);
    // The hide button (hideBtn_001 at opacity 0) at (right − 17, top − 15): hides the menu so the
    // level can be seen; clicking anywhere brings it back.
    ctx.area(Btn::PauseHide, w - 17.0, h - 15.0, 30.0, 30.0);
    if hidden {
        ctx.area(Btn::PauseHide, w * 0.5, h * 0.5, w, h);
        return;
    }
    // CCBlockLayer: black at opacity 75 over everything.
    c.rect(0.0, 0.0, w, h, [0.0; 3], 75.0 / 255.0);
    // The panel: square04_001 (9-slice), (w − 20) × (h − 20), black at opacity 175.
    c.nine_slice("square04_001.png", w * 0.5, h * 0.5, w - 20.0, h - 20.0, Look::default().rgb([0, 0, 0]).alpha(175));
    // The level's name (bigFont) 20 below the panel's top.
    let half_h = (h - 20.0) * 0.5;
    c.text("bigFont", level_name, w * 0.5, half_h + h * 0.5 - 20.0, Look::default());

    // The progress bar (normal mode) at (w/2, h/2 + 55): this attempt's progress over the
    // best, the percentage (0.5), the mode (0.5) 20 above, and "Best: n%" (gold, 0.48) 18 above.
    let y = h * 0.5 + 55.0;
    let now = (s.percent as u32).min(100);
    let (best, mode, fill) = if s.in_practice { (s.practice, "Practice Mode", [0, 255, 255]) } else { (s.best, "Normal Mode", [0, 255, 0]) };
    progress_bar(c, w * 0.5, y, best, now, fill);
    c.text("bigFont", &format!("{now}%"), w * 0.5, y + 1.0, Look::scaled(0.5));
    c.text("bigFont", mode, w * 0.5, y + 21.0, Look::scaled(0.5));
    if best > 0 {
        c.text("goldFont", &format!("Best: {best}%"), w * 0.5, y + 39.0, Look::scaled(0.48));
    }

    // The buttons, left to right as added: practice, resume, menu, restart; padding 15, at
    // h/2 − 30 (alignItemsHorizontallyWithPadding).
    let row = [
        (Btn::PausePractice, if s.in_practice { "GJ_normalBtn_001.png" } else { "GJ_practiceBtn_001.png" }),
        (Btn::PauseResume, "GJ_playBtn2_001.png"),
        (Btn::PauseMenu, "GJ_menuBtn_001.png"),
        (Btn::PauseRestart, "GJ_replayBtn_001.png"),
    ];
    let widths: Vec<f32> = row.iter().map(|(_, n)| c.size(n).0).collect();
    let total = widths.iter().sum::<f32>() + 15.0 * (row.len() - 1) as f32;
    let mut x = w * 0.5 - total * 0.5;
    for ((id, name), bw) in row.iter().zip(&widths) {
        ctx.button(c, *id, name, x + bw * 0.5, h * 0.5 - 30.0, Look::default(), 1.0);
        x += bw + 15.0;
    }
    // The side buttons (`customSetup`'s loop, tags 0–5) around the row's origin (w/2, h/2 − 30):
    // death, markers and quests at x − 248.5, options and hide at x + 248.5, 34 apart from +154.
    for (tag, name, dx, row) in [
        (2u8, "sideBtn_death_001.png", -248.5, 0.0),
        (3, "sideBtn_marker_001.png", -248.5, 1.0),
        (5, "sideBtn_quests_001.png", -248.5, 2.0),
        (0, "sideBtn_options_001.png", 248.5, 0.0),
        (1, "sideBtn_hide_001.png", 248.5, 1.0),
    ] {
        ctx.button(c, Btn::PauseSide(tag), name, w * 0.5 + dx, h * 0.5 - 30.0 + 154.0 - 34.0 * row, Look::default(), 1.0);
    }
    // The practice hint, left of the buttons (not shown in practice).
    if !s.in_practice {
        c.sprite("GJ_practiceTxt_001.png", w * 0.5 - 190.0, h * 0.5 - 12.0, Look::default());
    }

    // Music and sound effect sliders (scale 0.7) at (w/2 ∓ 100, h/2 − 90), their icons (1.2)
    // 90 left of each; dragging sets the volume (`PauseLayer::musicSliderChanged`).
    for (dx, icon, id, v) in [(-100.0, "gj_songIcon_001.png", Btn::SliderMusic, volumes.0), (100.0, "GJ_sfxIcon_001.png", Btn::SliderSfx, volumes.1)] {
        let (sx, sy) = (w * 0.5 + dx, h * 0.5 - 90.0);
        slider(c, ctx, id, sx, sy, v);
        c.sprite(icon, sx - 90.0, sy, Look::scaled(1.2));
    }

    // The level's coins (main levels: gjCoinUI_01, 1.1) at (w/2, 33), 20 apart: collected ones
    // (in the save) dimmed to half with opacity 125 unless taken this attempt; the rest "off",
    // at 0.75 and opacity 191, or "on" if taken this attempt.
    let n = s.coin_count.min(3);
    if n > 0 {
        let (cw, _) = c.size("gjCoinUI_01_off_001.png");
        let total = cw * 1.1 * n as f32 + 20.0 * (n - 1) as f32;
        let mut x = w * 0.5 - total * 0.5 + cw * 1.1 * 0.5;
        for i in 0..n {
            let saved = s.saved_coins[i];
            let taken = s.taken_coins[i];
            let name = if saved {
                "gjCoinUI_01_collected_001.png"
            } else if taken {
                "gjCoinUI_01_on_001.png"
            } else {
                "gjCoinUI_01_off_001.png"
            };
            let mut look = Look::scaled(1.1);
            if !taken {
                let (k, a) = if saved { (0.5, 125) } else { (0.75, 191) };
                look = look.rgb([(255.0 * k) as u8; 3]).alpha(a);
            }
            c.sprite(name, x, 33.0, look);
            x += cw * 1.1 + 20.0;
        }
    }
}

/// The pause menu's progress bar (`FUN_10044a43c` / `FUN_10044a4d4`): GJ_progressBar_001 in
/// black at opacity 125, the best filled in (40, 40, 40), this attempt's progress over it in
/// green (practice: cyan); fills scaled 0.992 × 0.86 from the left.
fn progress_bar(c: &mut Canvas, x: f32, y: f32, best: u32, now: u32, color: [u8; 3]) {
    let (pw, _) = c.size("GJ_progressBar_001.png");
    c.sprite("GJ_progressBar_001.png", x, y, Look::default().rgb([0, 0, 0]).alpha(125));
    let left = x - pw * 0.5 + (pw - pw * 0.992) * 0.5;
    let fill = Look::default().anchor(0.0, 0.5).scale_xy(0.992, 0.86);
    c.bar("GJ_progressBar_001.png", left, y, best as f32 / 100.0, fill.rgb([40, 40, 40]));
    c.bar("GJ_progressBar_001.png", left, y, now as f32 / 100.0, fill.rgb(color));
}

/// `Slider` (scale 0.7; `Slider::init` 0x2854a8): the groove, and inside it from (2, 4) the bar
/// (`sliderBar`, a repeating texture) `value` × (groove width − 4) long, and the thumb.
pub fn slider(c: &mut Canvas, ctx: &mut Ctx, id: Btn, x: f32, y: f32, value: f32) {
    slider_sized(c, ctx, id, x, y, value, 0.7);
}

/// A `Slider` at scale `s` (the pause menu's are 0.7, the settings dropdown's 1).
pub fn slider_sized(c: &mut Canvas, ctx: &mut Ctx, id: Btn, x: f32, y: f32, value: f32, s: f32) {
    // the track (where the thumb's center can go) is the hit area
    let (groove_w, _) = c.size("slidergroove.png");
    ctx.area(id, x, y, (groove_w - 4.0) * s, 30.0);
    let (gw, gh) = c.size("slidergroove.png");
    let (bw, _) = c.size("sliderBar.png");
    let (left, bottom) = (x - gw * s * 0.5, y - gh * s * 0.5);
    let mut remaining = (gw - 4.0) * value;
    let mut bx = 0.0;
    while remaining > 0.0 && bw > 0.0 {
        let part = remaining.min(bw);
        c.bar("sliderBar.png", left + (2.0 + bx) * s, bottom + 4.0 * s, part / bw, Look::scaled(s).anchor(0.0, 0.0));
        remaining -= part;
        bx += part;
    }
    c.sprite("slidergroove.png", x, y, Look::scaled(s));
    // `Slider`: the thumb travels ±100 around the center (the real dump: +100 at full volume)
    let tx = x + (value - 0.5) * 200.0 * s;
    c.sprite("sliderthumb.png", tx, y, Look::scaled(s));
}

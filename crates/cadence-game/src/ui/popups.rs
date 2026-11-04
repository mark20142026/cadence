//! Popups over the Settings dropdown: `TutorialLayer` (How To Play, 0x438548),
//! `VideoOptionsLayer` (Graphics, 0x77539c) and `ParentalOptionsLayer` (Parent, 0x779210).
//! Positions from the real 1280 × 720 frames (shots/gui-real/settings/opt*.png).

use super::canvas::{self, Canvas, Look};
use super::save::Progress;
use super::subpages::button_sprite;
use super::{Btn, Ctx};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Popup {
    /// How To Play, page 1..=5
    Tutorial(u8),
    /// Graphics: the windowed resolution (index into `RESOLUTIONS`) and texture quality
    /// (0 auto, 1 low, 2 medium, 3 high) being chosen
    Video { res: u8, quality: u8 },
    /// Parental Control, page 0 or 1
    Parental(u8),
    /// `RewardsPage`: the small (4 h) and large (24 h) chests
    Rewards,
}

/// `TutorialLayer::loadPage`: (text, scale, line spacing, y offset) per page.
const TUTORIAL: [(&str, f32, f32, f32); 5] = [
    ("<cg>Tap</c> the screen to jump.\n<cg>Hold</c> down to keep jumping.", 0.7, 40.0, 0.0),
    ("<cg>Hold</c> to fly up.\n<cg>Release</c> to fly down.", 0.7, 40.0, 0.0),
    ("You can enter <cl>practice mode</c> from the pause menu.\nPractice mode lets you place <cg>checkpoints</c>.", 0.6, 40.0, -13.0),
    ("You can place checkpoints manually, or use the auto-checkpoint feature.\nTap the delete button to remove your last checkpoint.", 0.55, 45.0, -13.0),
    ("<cy>Jump Orbs</c> activate when you are on top of them.\n<cg>Tap</c> while touching an orb to interact with it and use its effect.", 0.6, 40.0, -13.0),
];

/// `GameManager::resolutionForKey` (0x303498).
pub const RESOLUTIONS: [(u32, u32); 12] = [
    (640, 480),
    (720, 480),
    (720, 576),
    (800, 600),
    (1024, 768),
    (1152, 864),
    (1176, 664),
    (1280, 720),
    (1280, 768),
    (1280, 800),
    (1360, 768),
    (1920, 1080),
];

const QUALITY: [&str; 4] = ["AUTO", "LOW", "MEDIUM", "HIGH"];

/// `ParentalOptionsLayer::init`: (label, game variable, short text, info text) per row; the first
/// page has the two modes, the second the separate switches.
const PARENTAL: [(&str, &str, &str, &str); 6] = [
    ("Safe Mode", "0218", "Blocks all communication.\nOnly approved online levels.", "<cy>Safe Mode</c> blocks comments, posts, messages, friend requests, user search and uploads. Online levels are limited to <cg>Featured</c>, <cg>Daily</c>, <cg>Weekly</c>, <cg>Events</c>, <cg>Gauntlets</c> and <cg>Map Packs</c>. Social links are hidden. Account privacy is set to block messages, friend requests and comments."),
    ("Offline Mode", "0219", "No online levels or profiles.\nIncludes Safe Mode.", "<cy>Offline Mode</c> blocks everything Safe Mode blocks, plus online levels, profiles and leaderboards."),
    ("Disable\nComments", "0075", "", "Hides level comments and blocks posting them."),
    ("Disable\nPosts", "0076", "", "Hides account posts and blocks posting them."),
    ("Disable\nMessaging", "0243", "", "Blocks messages, group chats and friend requests. Account privacy is set to block them and the comment history is hidden, like Safe Mode."),
    ("Only Curated\nLevels", "0077", "", "Limits online levels to <cg>Featured</c>, <cg>Daily</c>, <cg>Weekly</c>, <cg>Events</c>, <cg>Gauntlets</c> and <cg>Map Packs</c>."),
];

pub fn parental_info(i: u8) -> (&'static str, &'static str) {
    let (l, _, _, d) = PARENTAL[i as usize];
    (l, d)
}

pub fn parental_key(i: u8) -> &'static str {
    PARENTAL[i as usize].1
}

/// The current resolution's index (the save's "resolution" key, 1-based like the game's).
pub fn current_res(p: &Progress) -> u8 {
    (p.number("resolution", 8.0) as i32 - 1).clamp(0, RESOLUTIONS.len() as i32 - 1) as u8
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, popup: Popup, chests: &Option<(crate::gjapi::Rewards, web_time::Instant)>) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    match popup {
        Popup::Tutorial(page) => {
            c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
            c.nine_slice("GJ_square01.png", cx, cy, 420.0, 260.0, Look::default());
            c.text("bigFont", "How To Play", cx, cy + 105.0, Look::default());
            ctx.button(c, Btn::PopClose, "GJ_closeBtn_001.png", cx - 200.0, cy + 120.0, Look::default(), 1.5);
            let (text, scale, spacing, dy) = TUTORIAL[(page.clamp(1, 5) - 1) as usize];
            let lines = canvas::rich_lines(c, "bigFont", text, 400.0 / scale);
            // TextArea anchored (0.5, 1) — the first line's top 14 above the area's middle
            canvas::draw_rich(c, "bigFont", &lines, cx, cy + 69.0 + dy, scale, spacing, 255);
            c.sprite(&format!("tutorial_{page:02}.png"), cx - 200.0, cy - 120.0, Look::default().anchor(0.0, 0.0));
            let label = if page >= 5 { "Exit" } else { "Next" };
            button_sprite(c, ctx, Btn::PopNext, label, "bigFont", "GJ_button_01.png", cx + 158.0, cy - 80.0, 66.4, 40.0, 0.6, false);
        }
        Popup::Video { res, quality } => {
            c.rect(0.0, 0.0, w, h, [0.0; 3], 75.0 / 255.0);
            c.nine_slice("GJ_square01.png", cx, cy, 360.0, 260.0, Look::default());
            ctx.button(c, Btn::PopClose, "GJ_closeBtn_001.png", cx - 175.0, cy + 125.0, Look::default(), 1.5);
            c.text("goldFont", "Video Options", cx, cy + 116.0, Look::scaled(0.7));
            button_sprite(c, ctx, Btn::VidAdvanced, "Advanced", "goldFont", "GJ_button_04.png", cx + 132.0, cy + 110.0, 76.0, 25.0, 0.494, false);
            // full screen: Cadence's window is resized freely (the game's 0025 is "windowed")
            let full = !p.var("0025", true);
            toggle_box(c, ctx, Btn::VidFullscreen, full, cx - 65.0, cy + 85.0, 0.7, false);
            c.text("bigFont", "Fullscreen", cx - 44.7, cy + 85.0, Look::scaled(0.5).anchor(0.0, 0.5));
            // Borderless / Fix only apply in full screen
            let grey = !full;
            let col = if grey { [166, 166, 166] } else { [255, 255, 255] };
            toggle_box(c, ctx, Btn::VidBorderless, p.var("0170", false), cx - 89.0, cy + 48.0, 0.7, grey);
            c.text("bigFont", "Borderless", cx - 68.7, cy + 48.0, Look::scaled(0.5).anchor(0.0, 0.5).rgb(col));
            toggle_box(c, ctx, Btn::VidFix, p.var("0154", false), cx + 51.0, cy + 48.0, 0.7, grey);
            c.text("bigFont", "Fix", cx + 71.3, cy + 48.0, Look::scaled(0.5).anchor(0.0, 0.5).rgb(col));
            c.text("goldFont", "Windowed Resolution", cx, cy + 17.0, Look::scaled(0.8));
            let (rw, rh) = RESOLUTIONS[res as usize];
            c.text("bigFont", &format!("{rw}x{rh}"), cx, cy - 8.0, Look::scaled(0.608));
            ctx.button(c, Btn::VidRes(false), "edit_leftBtn_001.png", cx - 70.0, cy - 8.0, Look::default(), 1.5);
            ctx.button(c, Btn::VidRes(true), "edit_rightBtn_001.png", cx + 70.0, cy - 8.0, Look::default(), 1.5);
            c.text("goldFont", "Texture Quality", cx, cy - 41.0, Look::scaled(0.8));
            c.text("bigFont", QUALITY[quality as usize % 4], cx, cy - 66.0, Look::scaled(0.75));
            ctx.button(c, Btn::VidQuality(false), "edit_leftBtn_001.png", cx - 70.0, cy - 66.0, Look::default(), 1.5);
            ctx.button(c, Btn::VidQuality(true), "edit_rightBtn_001.png", cx + 70.0, cy - 66.0, Look::default(), 1.5);
            button_sprite(c, ctx, Btn::PopClose, "Back", "goldFont", "GJ_button_01.png", cx - 55.0, cy - 107.0, 96.0, 31.5, 1.0, false);
            button_sprite(c, ctx, Btn::VidApply, "Apply", "goldFont", "GJ_button_01.png", cx + 55.0, cy - 107.0, 96.0, 31.5, 1.0, false);
        }
        Popup::Rewards => {
            c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
            c.nine_slice("GJ_square02.png", cx, cy, 340.0, 220.0, Look::default());
            for (sx, sy) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                c.sprite("rewardCorner_001.png", cx + 145.0 * sx, cy + 85.0 * sy, Look::default().flip_x(sx > 0.0).flip_y(sy > 0.0));
            }
            c.sprite("rewardsLabel_001.png", cx, cy + 78.0, Look::default());
            ctx.button(c, Btn::PopClose, "GJ_closeBtn_001.png", cx - 160.0, cy + 100.0, Look::default(), 1.5);
            ctx.button(c, Btn::RewardFree, "GJ_freeChestBtn_001.png", cx + 160.0, cy + 100.0, Look::scaled(1.1), 1.2);
            ctx.button(c, Btn::RewardChest(0), "chest_01_02_001.png", cx - 70.0, cy - 20.1, Look::default(), 1.1);
            ctx.button(c, Btn::RewardChest(1), "chest_02_02_001.png", cx + 70.0, cy - 10.0, Look::default(), 1.1);
            // each chest: "Open" when ready, else the time left (h:mm:ss); nothing until the
            // servers answer
            for (k, x) in [(0usize, cx - 70.0), (1, cx + 70.0)] {
                let label = match chests {
                    None => String::new(),
                    Some((r, at)) => {
                        let left = if k == 0 { r.small_left } else { r.big_left } - at.elapsed().as_secs() as i64;
                        if left <= 0 { "Open".into() } else { format!("{}:{:02}:{:02}", left / 3600, left / 60 % 60, left % 60) }
                    }
                };
                c.text("bigFont", &label, x, cy - 74.0, Look::scaled(0.5));
            }
        }
        Popup::Parental(page) => {
            c.rect(0.0, 0.0, w, h, [0.0; 3], 75.0 / 255.0);
            c.nine_slice("GJ_square02.png", cx, cy, 360.0, 260.0, Look::default());
            c.text("bigFont", "Parental Control", cx, cy + 114.0, Look::scaled(0.6));
            ctx.button(c, Btn::PopClose, "GJ_closeBtn_001.png", cx - 175.0, cy + 125.0, Look::default(), 1.5);
            let locked = p.var("0228", false);
            ctx.button(c, Btn::ParLock, if locked { "GJ_lock_001.png" } else { "GJ_lock_open_001.png" }, cx + 156.0, cy - 106.0, Look::default(), 1.3);
            if page == 0 {
                ctx.button(c, Btn::ParPage, "GJ_arrow_03_001.png", cx + 224.0, cy, Look::default().flip_x(true), 1.5);
                for (k, i) in [0u8, 1].into_iter().enumerate() {
                    let y = cy + 50.0 - 80.0 * k as f32;
                    c.nine_slice("square02_small.png", cx, y, 340.0, 60.0, Look::default().alpha(100));
                    let (label, key, short, _) = PARENTAL[i as usize];
                    toggle_box(c, ctx, Btn::ParToggle(i), p.var(key, false), cx - 120.0, y, 0.9, false);
                    ctx.button(c, Btn::ParInfo(i), "GJ_infoIcon_001.png", cx - 147.0, y + 16.0, Look::scaled(0.6), 2.0);
                    let (lw, _) = c.text_size("bigFont", label);
                    c.text("bigFont", label, cx - 96.3, y, Look::scaled((120.0 / lw.max(1.0)).min(0.55)).anchor(0.0, 0.5));
                    for (j, line) in short.split('\n').enumerate() {
                        c.text("chatFont", line, cx + 35.0, y + 4.95 - j as f32 * 9.9, Look::scaled(0.6).anchor(0.0, 0.5));
                    }
                }
            } else {
                ctx.button(c, Btn::ParPage, "GJ_arrow_03_001.png", cx - 224.0, cy, Look::default(), 1.5);
                for (k, i) in [2u8, 3, 4, 5].into_iter().enumerate() {
                    let x = if k % 2 == 0 { cx - 120.0 } else { cx + 40.0 };
                    let y = cy + 50.0 - 80.0 * (k / 2) as f32;
                    let (label, key, _, _) = PARENTAL[i as usize];
                    toggle_box(c, ctx, Btn::ParToggle(i), p.var(key, false), x, y, 0.9, false);
                    ctx.button(c, Btn::ParInfo(i), "GJ_infoIcon_001.png", x - 27.0, y + 16.0, Look::scaled(0.6), 2.0);
                    for (j, line) in label.split('\n').enumerate() {
                        c.text("bigFont", line, x + 22.0, y + 7.0 - j as f32 * 14.0, Look::scaled(0.42).anchor(0.0, 0.5));
                    }
                }
            }
        }
    }
}

/// A check box (`CCMenuItemToggler`); a greyed one (color 166) has no hit area.
fn toggle_box(c: &mut Canvas, ctx: &mut Ctx, id: Btn, on: bool, x: f32, y: f32, scale: f32, grey: bool) {
    let f = if grey { 1.0 } else { ctx.factor(id) };
    let name = if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" };
    let look = if grey { Look::scaled(scale * f).rgb([166, 166, 166]) } else { Look::scaled(scale * f) };
    c.sprite(name, x, y, look);
    if !grey {
        ctx.area(id, x, y, 31.0 * scale * 1.4, 31.0 * scale * 1.4);
    }
}

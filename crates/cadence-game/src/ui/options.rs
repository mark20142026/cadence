//! The settings: `OptionsLayer` (0x7705b0, a `GJDropDownLayer` that slides down from the top on
//! two chains) and the toggle pages of `MoreOptionsLayer` (0x771f04). Toggles are the game's
//! game variables, kept under the same keys in Cadence's own save (`Progress::var`).

use super::canvas::{self, Canvas, Look};
use super::save::Progress;
use super::{Btn, Ctx};

/// One row of `GameSettingsLayer::buildTabContent` (0x544098): a toggle (label, game variable,
/// description; an empty description has no info button), a greyed one, or a page break.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entry {
    Toggle(&'static str, &'static str, &'static str),
    /// `addToggle(.., disabled = true)`: drawn grey, does nothing (Practice Music Sync needs
    /// the Music Unlocker).
    Disabled(&'static str, &'static str),
    /// `offsetToNextPage(12)`: the rest of the tab starts on a new page.
    Break,
}
use Entry::{Break, Disabled, Toggle};

/// The tabs' names (`GameSettingsLayer::setupTabs`).
pub const TAB_NAMES: [&str; 6] = ["Gameplay", "Visual", "Practice", "Editor", "Audio", "Other"];

/// Each tab's rows in order. "pbar" is the progress bar (a GameManager member, not a variable).
pub const TABS: [&[Entry]; 6] = [
    &[
        Toggle("Auto-Retry", "0026", "Restarts level upon death automatically."),
        Toggle("Faster Reset", "0052", "Restarts in 0.5s instead of 1.0s upon death."),
        Toggle("Click Between Steps", "0177", "Toggles substeps on, allowing you to click in-between the default normal steps. This may affect performance."),
        Toggle("Click On Steps", "0176", "Allows you to click on every step, regardless of your refresh rate. This is beneficial for users below 240 Hz."),
        Toggle("Lock Cursor In-Game", "0128", "Locks and hides cursor during gameplay."),
        Toggle("Flip 2-Player\nControls", "0010", "Flips which side controls which player during 2-player mode."),
        Toggle("Always Limit\nControls", "0011", "Limits player 1 controls to one side even when dual mode is inactive."),
        Toggle("Disable Thumbstick", "0028", "Disables mouse movement when using a controller thumbstick."),
        Toggle("Disable Auto\nCreator Controls", "0203", "Ignores creator controls in platformer levels."),
        Toggle("Progress Bar", "pbar", ""),
        Toggle("Percentage", "0040", ""),
        Toggle("Show Time", "0145", ""),
    ],
    &[
        Toggle("Show Cursor In-Game", "0024", "Shows cursor and pause button during gameplay."),
        Toggle("Hide Attempts", "0135", "Hides the attempt counter when playing levels."),
        Toggle("Flip Pause Button", "0015", "Flips the location of the pause button."),
        Toggle("Disable Portal Guide", "0129", "Disables extra indicators on portals."),
        Toggle("Orb Guide", "0130", "Enables extra indicators on orbs."),
        Toggle("Disable Orb Scale", "0140", "Disables the scaling effect on all orbs."),
        Toggle("Disable Trigger\nOrb Scale", "0141", "Disables the scaling effect on only trigger orbs."),
        Toggle("Disable Shake", "0172", "Disables shake effects."),
        Toggle("Disable Explosion\nShake", "0014", "Disables the shake effect that happens upon death."),
        Toggle("Disable Gravity\nEffect", "0072", "Disables the effect that happens upon changing gravity."),
        Toggle("Default Mini Icon", "0060", "Sets player icon in mini mode to default."),
        Toggle("Switch Spider\nTeleport Color", "0061", "Toggles between main and secondary color for the teleport effect in spider mode."),
        Toggle("Switch Dash\nFire Color", "0062", "Toggles between main and secondary color for the fire effect from dash orbs."),
        Toggle("Switch Wave\nTrail Color", "0096", "Toggles between main and secondary color for the trail in wave mode."),
        Toggle("Disable Player\nTrail", "0196", "Hides the player trail in all modes except wave, including the trail from orbs and pads."),
        Toggle("Disable Wave\nTrail", "0197", "Hides the trail in wave mode."),
        Toggle("Hide Playtest Text", "0174", "Hides text in the top left when using start positions or ignore damage."),
        Toggle("Hitbox On Death", "0179", "Shows hitboxes upon death in both normal and practice mode."),
        Toggle("Hitbox Trail\nOn Death", "0214", "Shows a hitbox trail when you die, in normal and practice mode."),
        Toggle("Hitbox Trail\nHighlight", "0217", "Draws the hitbox trail in solid red with no fade. Easier to see on some backgrounds."),
        Toggle("Legacy Pause\nProgress", "0249", "Shows full Normal and Practice Mode progress bars in the pause menu instead of active session only."),
    ],
    &[
        Toggle("Hide Practice Buttons", "0071", "Hides the checkpoint buttons shown in practice mode."),
        Toggle("Hide Attempts", "0134", "Hides the attempt counter when playing levels in practice mode."),
        Toggle("Auto-Checkpoints", "0027", "Places checkpoints automatically in practice mode."),
        Toggle("No Auto-Checkpoints\nFor Start Pos", "0234", "Disables automatic checkpoints when using StartPos in Practice mode."),
        Toggle("Quick-Checkpoints", "0068", "Tries to place checkpoints more often in practice mode."),
        Toggle("Death Effect", "0100", "Shows death effects in practice mode."),
        Toggle("Skip Death Triggers", "0212", "Skips On Death triggers and extra reset time in practice mode."),
        Toggle("Normal Music\nIn Editor", "0125", "Plays normal music in sync to editor levels in practice mode."),
        Toggle("Show Hitboxes", "0166", "Shows hitboxes while in practice mode."),
        Toggle("Disable Player Hitbox", "0171", "Disables the player's hitbox in practice mode\n(if hitboxes are shown)."),
        Toggle("Show Hitbox Trail", "0213", "Shows a hitbox trail in practice mode."),
        Toggle("Disable Checkpoints", "0146", ""),
    ],
    &[
        Toggle("Draw Trigger Boxes", "0044", "Draws the hitbox for touch triggered objects."),
        Toggle("Playtest No UI", "0046", "Hides part of the editor's user interface during playtesting."),
        Toggle("Playtest Music", "0002", "Plays music during playtesting."),
        Toggle("Effect Lines", "0043", "Shows a vertical line for triggers to indicate where they are activated."),
        Toggle("Duration Lines", "0058", "Shows a horizontal line for triggers to indicate how long they are active."),
        Toggle("Grid On Top", "0039", "Shows the grid on top of objects in editor."),
        Toggle("Playtest No Grid", "0079", "Hides grid during playtesting."),
        Toggle("Hide Background", "0078", "Hides background in editor."),
        Toggle("Link Controls", "0097", "Enables option to link objects together in the editor. Linked objects are always selected and deleted together."),
        Toggle("Hold To Swipe", "0057", "Enables swiping after holding down anywhere on the grid for a short duration. Useful when selecting objects without the need to toggle swiping manually."),
        Break,
        Toggle("Swipe Cycle Mode", "0059", "Cycles through touched objects instead of adding to selection when tapping in the same position while swiping is enabled. Useful if you want to select multiple objects that are mixed with other objects."),
        Toggle("Layer Locking", "0103", "Allows you to click on the editor layer number to lock and unlock a layer."),
        Toggle("Show Clicks", "0149", "Shows a marker during playtesting when the player clicks."),
        Toggle("Auto-Pause", "0150", "Pauses gameplay when starting from a start position to prepare gameplay. Click a second time to actually start playing. This is used to handle start lag in larger levels."),
        Toggle("Hide Path", "0152", "Hides the player path from playtesting."),
        Toggle("Start Optimization", "0151", "Tries to speed up start position loading in editor. Experimental."),
        Toggle("Hide Particle Icons", "0137", "Hides particle icons while preview mode is enabled."),
        Toggle("Increase Undo/Redo", "0013", "Increases maximum undo and redo limit from 200 to 1000 in editor."),
        Toggle("Playtest Smooth Fix", "0102", "Enables smooth fix when playtesting in the editor."),
        Toggle("Increase Scale", "0112", "Increases maximum object scale to 4x and minimum scale to 0.25x. Note that objects larger than 6x grid size may disappear near the screen edge. This may be fixed in the future."),
        Toggle("Disable Paste\nState Groups", "0156", "Makes it so that pasting state no longer updates the group info of the target object(s). Group state can be pasted directly on the edit group page instead."),
        Toggle("Small Warp Buttons", "0169", "Warp control buttons are 50% smaller. (Restart editor)"),
        Toggle("Static Trace Arrows", "0181", "Disable the arrows showing directed connections when using Trace In/Out."),
        Toggle("Legacy Move Menu", "0191", "Uses the classic grid layout for move buttons instead of the d-pad layout."),
        Toggle("Swap Move Menu", "0247", "Place the dpad on the right side of the move menu."),
        Toggle("Disable Zoom Label", "0195", "Hides the zoom label when zooming."),
        Toggle("Hitbox Trail", "0215", "Shows a trail of previous hitboxes while playtesting. The trail stays until the next playtest."),
        Toggle("Disable Anchor Snap", "0242", "Stops the warp anchor point from snapping to corners, midpoints and the grid."),
        Toggle("Disable Pinch", "0245", "Disables pinch to zoom and two-finger panning in the editor."),
        Toggle("Hide Center Cross", "0248", "Hides the cross marking the camera center in preview mode."),
    ],
    &[
        Disabled("Practice Music Sync", "Plays normal music in practice mode in main and online levels. Requires the Music Unlocker."),
        Toggle("Audio Visualizer", "0144", ""),
        Toggle("Change Custom Songs\nLocation", "0033", "Saves custom songs in a different directory. May fix custom songs not working."),
        Toggle("Disable Song Alert", "0083", "Removes the alert shown when starting levels without the song downloaded."),
        Toggle("No Song Limit", "0018", "Stops automatic deletion of custom songs. This is done by default to save space."),
        Toggle("Reduce Quality\n(Read Info)", "0142", "Lowers audio sampling rate from 44100 Hz to 24000 Hz. Requires restarting to take effect."),
        Toggle("Audio Fix 01", "0159", "Increases the audio buffer size, which may fix certain issues. Do not enable if audio is working fine. Causes a slight more audio delay. Requires restarting to take effect."),
    ],
    &[
        Toggle("Increase Draw Capacity", "0066", "Increases draw capacity for batch nodes at level start. Can improve performance on some levels, but may cause issues on low-end devices."),
        Toggle("Low Detail", "0108", "Enables low detail mode on levels that support it automatically."),
        Toggle("Disable High Object\nAlert", "0082", "Removes the alert shown when starting levels with a high object count."),
        Toggle("Extra LDM", "0136", "Removes glow and enter effects while in low detail mode. Levels without LDM show LDM Lite."),
        Toggle("Increase Maximum\nLevels", "0042", "Increases maximum locally saved levels from 10 to 100. This refers to level data, not statistics. Enabling this can make your save file considerably larger, so keeping the option off is recommended for quicker saving."),
        Toggle("Disable Level Saving", "0119", "Saves level statistics as usual, but levels need to be redownloaded every time you restart the game. Makes saving and loading faster."),
        Toggle("Save Gauntlets", "0127", "Saves gauntlet levels locally so they do not have to be redownloaded. Increases save time but helpful if you have poor connection."),
        Toggle("Disable Shader\nAnti-Aliasing", "0155", "Disables anti-aliasing on shader effects."),
        Toggle("Show Info Label", "0109", ""),
        Toggle("More Comments", "0094", "Shows more comments per page. Why not?"),
        Toggle("Load Comments", "0090", "Loads comments automatically."),
        Toggle("New Completed Filter", "0073", "Makes completed levels filter based only on percentage from update 2.1. Useful to rebeat levels for Mana Orbs."),
        Toggle("Increase Local Levels\nPer Page", "0093", "Increases created and saved levels per page from 10 to 20."),
        Toggle("Manual Level Order", "0084", "Places new levels last in the saved levels list. Useful if you want to manually move levels to the top."),
        Toggle("Percentage Decimals", "0126", "Shows decimals in level progress."),
        Toggle("Show Leaderboard\nPercentage", "0099", "Toggles viewing the leaderboard percentage you have on levels. To upload your level progress to the level leaderboard, you need to replay levels completed before 2.11."),
        Toggle("Do Not...", "0095", "Does not do anything... Well, nothing useful."),
        Toggle("Confirm Exit", "0167", "Adds an extra confirmation window when exiting levels."),
        Toggle("Fast\nMenu", "0168", "Makes transitions between menu pages faster."),
        Toggle("Disable Auto-Save\nPlatformer", "0192", "Stops the game from automatically saving checkpoints on platformer levels. Existing saves are kept until you clear them."),
    ],
];

/// Game variables that default to on (`GameManager::firstSetup`): auto-retry, auto checkpoints,
/// the progress bar.
pub fn default_var(key: &str) -> bool {
    matches!(key, "0026" | "0027" | "pbar")
}

/// A tab's rows laid out in pages of twelve: (page, slot, index into the tab's list).
/// The first editor page holds ten (its buttons-per-row controls take the last row).
pub fn layout(tab: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let (mut page, mut slot) = (0usize, 0usize);
    for (i, e) in TABS[tab].iter().enumerate() {
        if *e == Break {
            page += 1;
            slot = 0;
            continue;
        }
        if slot == 12 {
            page += 1;
            slot = 0;
        }
        out.push((page, slot, i));
        slot += 1;
    }
    out
}

pub fn page_count(tab: usize) -> usize {
    layout(tab).last().map_or(1, |l| l.0 + 1)
}

/// The toggle's label, game variable and description.
pub fn entry(tab: u8, i: u8) -> Option<(&'static str, &'static str, &'static str)> {
    match TABS.get(tab as usize)?.get(i as usize)? {
        Toggle(l, k, d) => Some((l, k, d)),
        Disabled(l, d) => Some((l, "", d)),
        Break => None,
    }
}

/// Which settings screen is up, over the main menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Settings {
    /// The dropdown; `t` seconds since it opened (or, closing, since it started to close).
    DropDown { t: f32, closing: bool },
    /// `GameSettingsLayer`: the tabbed toggle pages (over the dropdown, or the pause menu).
    More { tab: u8, page: usize },
    /// `StatsLayer`: the same dropdown with the stats list, scrolled `scroll` points.
    Stats { t: f32, closing: bool, scroll: f32 },
    /// Account, Support (Help) or Soundtrack: another dropdown.
    Sub { sub: super::subpages::Sub, t: f32, closing: bool, scroll: f32 },
    /// `AchievementsLayer`: page `page` of the list, scrolled `scroll`.
    Achievements { t: f32, closing: bool, scroll: f32, page: usize },
}

/// `GJDropDownLayer::showLayer`: the layer moves from (0, h) to (0, 0) in 0.5 s
/// (`CCEaseInOut` rate 2) while the background fades to 125; fast menu (0168) skips it.
pub fn drop_offset(t: f32, closing: bool, h: f32, fast: bool) -> (f32, f32) {
    let k = if fast { 1.0 } else { canvas::ease_in_out((t / 0.5).min(1.0), 2.0) };
    let k = if closing { 1.0 - k } else { k };
    ((1.0 - k) * h, k * 125.0 / 255.0)
}

/// `ButtonSprite`: gold text on GJ_button_01, at least `min_w` wide, 30 tall.
/// A fixed-width `ButtonSprite` (goldFont on GJ_button_01): the label fits `width` at scale 1
/// at most, the sprite is `width` + 16 by 31.5, the label 2 above its center (the real dump:
/// Account 146 × 31.5 at scale 1, How To Play squeezed to 130 / 166 = 0.783).
pub fn text_button(c: &mut Canvas, ctx: &mut Ctx, id: Btn, text: &str, x: f32, y: f32, width: f32) {
    let f = ctx.factor(id);
    let (tw, _) = c.text_size("goldFont", text);
    let w = width + 16.0;
    c.nine_slice("GJ_button_01.png", x, y, w * f, 31.5 * f, Look::default());
    let s = (width / tw.max(1.0)).min(1.0) * f;
    c.text("goldFont", text, x, y + 2.0 * f, Look::scaled(s));
    ctx.area(id, x, y, w, 31.5);
}

/// `GameToolbox::createToggleButton`: the check box, and its label beside it (left anchored, 8
/// right of the box) or above it, squeezed to `width` at `label_scale` at most.
#[allow(clippy::too_many_arguments)]
pub fn toggle(c: &mut Canvas, ctx: &mut Ctx, id: Btn, on: bool, label: &str, x: f32, y: f32, scale: f32, label_scale: f32, width: f32, above: bool) {
    let f = ctx.factor(id);
    let name = if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" };
    c.sprite(name, x, y, Look::scaled(scale * f));
    let (bw, bh) = c.size(name);
    ctx.area(id, x, y, bw * scale * 1.5, bh * scale * 1.5);
    let lines: Vec<&str> = label.split('\n').collect();
    let widest = lines.iter().map(|l| c.text_size("bigFont", l).0).fold(0.0, f32::max);
    let s = if width > 0.0 && widest > 0.0 { (width / widest).min(label_scale) } else { label_scale };
    let line_h = 30.0 * s;
    let n = lines.len() as f32;
    for (i, line) in lines.iter().enumerate() {
        if above {
            let ly = y + scale * bh * 0.5 + 2.0 + (n - 1.0 - i as f32) * line_h + line_h * 0.5;
            c.text("bigFont", line, x, ly, Look::scaled(s));
        } else {
            let ly = y + (n - 1.0) * line_h * 0.5 - i as f32 * line_h;
            c.text("bigFont", line, x + 8.0 + scale * bw * 0.5, ly, Look::scaled(s).anchor(0.0, 0.5));
        }
    }
}

/// `GJListLayer`'s table frame (0x5bf304) around a list at (x, y), w × h, and its title.
pub fn list_border(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, title: &str) {
    let cx = x + w * 0.5;
    let (sw, sh) = c.size("GJ_table_side_001.png");
    if sh > 0.0 {
        c.sprite("GJ_table_side_001.png", x - 20.0, y + 1.0, Look::default().anchor(0.0, 0.0).scale_xy(1.0, (h - 2.0) / sh));
        c.sprite("GJ_table_side_001.png", x + w + 20.0, y + 1.0, Look::default().anchor(1.0, 0.0).scale_xy(1.0, (h - 2.0) / sh).flip_x(true));
    }
    let _ = sw;
    c.sprite("GJ_table_bottom_001.png", cx, y - 10.0, Look::default());
    c.sprite("GJ_table_top_001.png", cx, y + h + 15.0, Look::default());
    let (tw, _) = c.text_size("bigFont", title);
    let s = if tw > 0.0 { (280.0 / tw).min(0.8) } else { 0.8 };
    c.text("bigFont", title, cx, y + h + 18.0, Look::scaled(s));
}

/// A `GJDropDownLayer` under its content: the background, the chains and the list's body.
/// Returns how far the layer is still above its place (it slides down). The content goes on
/// top, then `dropdown_border` (the table frame, title and back arrow cover the list's edges).
pub fn dropdown_frame(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, t: f32, closing: bool, title: &str) -> f32 {
    let _ = (ctx, title);
    let (w, h) = (c.width, c.height);
    let (dy, dim) = drop_offset(t, closing, h, p.var("0168", false));
    c.rect(0.0, 0.0, w, h, [0.0; 3], dim);
    let (lx, ly) = ((w - 356.0) * 0.5, (h - LIST_H) * 0.5 - 5.0 + dy);
    // the chains it hangs on, from the list's top up off screen
    let (_, ch) = c.size("chain_01_001.png");
    if ch > 0.0 {
        let mut cy = ly + LIST_H;
        while cy < h + dy + 10.0 {
            c.sprite("chain_01_001.png", w * 0.5 - 156.0, cy, Look::default().anchor(0.5, 0.0));
            c.sprite("chain_01_001.png", w * 0.5 + 156.0, cy, Look::default().anchor(0.5, 0.0).flip_x(true));
            cy += ch;
        }
    }
    c.rect(lx, ly, 356.0, LIST_H, [0.0; 3], 180.0 / 255.0);
    dy
}

/// The dropdown's table frame, title and back arrow, over its content.
pub fn dropdown_border(c: &mut Canvas, ctx: &mut Ctx, dy: f32, title: &str) {
    let (w, h) = (c.width, c.height);
    let (lx, ly) = ((w - 356.0) * 0.5, (h - LIST_H) * 0.5 - 5.0 + dy);
    list_border(c, lx, ly, 356.0, LIST_H, title);
    ctx.button(c, Btn::DropBack, "GJ_arrow_03_001.png", 24.0, h - 23.0 + dy, Look::default(), 1.6);
}

/// The dropdowns' list height.
pub const LIST_H: f32 = 220.0;

/// The Settings dropdown (`OptionsLayer::customSetup`).
pub fn draw_dropdown(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, t: f32, closing: bool, volumes: (f32, f32)) {
    let (w, h) = (c.width, c.height);
    let dy = dropdown_frame(c, ctx, p, t, closing, "Settings");
    dropdown_border(c, ctx, dy, "Settings");

    // the menu at (w/2, h/2 + 80)
    let (mx, my) = (w * 0.5, h * 0.5 + 80.0 + dy);
    for (id, text, dx, ddy, width) in [
        (Btn::SetAccount, "Account", -79.0, 0.0, 130.0),
        (Btn::SetHowTo, "How To Play", 79.0, 0.0, 130.0),
        (Btn::SetOptions, "Options", -79.0, -40.0, 130.0),
        (Btn::SetGraphics, "Graphics", 79.0, -40.0, 130.0),
        (Btn::SetRate, "Rate", -105.0, -80.0, 78.0),
        (Btn::SetParent, "Parent", 0.0, -80.0, 78.0),
        (Btn::SetHelp, "Help", 105.0, -80.0, 78.0),
    ] {
        text_button(c, ctx, id, text, mx + dx, my + ddy, width);
    }
    // the secret vault's lock, top right, with what opens it under it: "10" (bigFont 0.4) and
    // a user coin (0.5), placed from the lock's bottom left (4, −7.25) and (19, −7.25)
    let (lx, ly) = (w - 20.0, h - 18.0 + dy);
    ctx.button(c, Btn::SetVault, "GJ_lockGray_001.png", lx, ly, Look::default(), 1.5);
    let (kw, kh) = c.size("GJ_lockGray_001.png");
    let (bx, by) = (lx - kw * 0.5, ly - kh * 0.5);
    c.text("bigFont", "10", bx + 4.0, by - 7.25, Look::scaled(0.4));
    c.sprite("GJ_coinsIcon2_001.png", bx + 19.0, by - 7.25, Look::scaled(0.5));

    // Music / SFX: labels (0.6) and sliders 20 below each
    let music_y = h * 0.5 - 34.0 + dy;
    c.text("bigFont", "Music", w * 0.5, music_y, Look::scaled(0.6));
    super::pause::slider_sized(c, ctx, Btn::SliderMusic, w * 0.5, music_y - 20.0, volumes.0, 1.0);
    c.text("bigFont", "SFX", w * 0.5, music_y - 40.0, Look::scaled(0.6));
    super::pause::slider_sized(c, ctx, Btn::SliderSfx, w * 0.5, music_y - 60.0, volumes.1, 1.0);
    // "Menu Music" (game variable 0122 is "menu music off")
    toggle(c, ctx, Btn::SetMenuMusic, !p.var("0122", false), "Menu\nMusic", w * 0.5 + 145.0, music_y - 35.0, 0.6, 0.35, 0.0, true);
}

/// Tab buttons (`TabbedPopupLayer::createTabButtons`): measured centers and widths of the
/// `ButtonSprite`s (bigFont, 25 high), then Keys (goldFont 0.475); 4 apart, at the top.
const TAB_X: [(f32, f32); 7] = [(-152.9, 59.09), (-96.2, 46.19), (-40.7, 56.83), (14.7, 45.93), (62.5, 41.8), (109.6, 44.33), (159.1, 46.64)];

/// `GameSettingsLayer` (0x543c5c): a `SetupTriggerPopup` 400 × 280 (GJ_square07 under
/// GJ_square06 tinted 80,87,105), close button at the top left, tabs, two columns of six
/// toggles 37 apart, page arrows at ±224 when the tab has more than one page.
pub fn draw_more(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, tab: u8, page: usize) {
    let (w, h) = (c.width, c.height);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.nine_slice("GJ_square07.png", cx, cy, 400.0, 280.0, Look::default());
    c.nine_slice("GJ_square06.png", cx, cy, 400.0, 280.0, Look::default().rgb([80, 87, 105]));
    ctx.button(c, Btn::MoreClose, "GJ_closeBtn_001.png", cx - 200.0, cy + 140.0, Look::scaled(0.7), 1.5);
    for (i, &(dx, bw)) in TAB_X.iter().enumerate() {
        let id = if i == 6 { Btn::MoreKeys } else { Btn::MoreTab(i as u8) };
        let f = ctx.factor(id);
        let bg = if i == tab as usize { "GJ_button_02.png" } else { "GJ_button_05.png" };
        c.nine_slice(bg, cx + dx, cy + 120.0, bw * f, 25.0 * f, Look::default());
        if i == 6 {
            c.text("goldFont", "Keys", cx + dx, cy + 122.0, Look::scaled(0.475 * f));
        } else {
            c.text("bigFont", TAB_NAMES[i], cx + dx + 0.3, cy + 122.0, Look::scaled(0.306 * f));
        }
        ctx.area(id, cx + dx, cy + 120.0, bw, 25.0);
    }
    let t = tab as usize;
    if page_count(t) > 1 {
        ctx.button(c, Btn::MorePrev, "GJ_arrow_01_001.png", cx - 224.0, cy, Look::default(), 1.5);
        ctx.button(c, Btn::MoreNext, "GJ_arrow_01_001.png", cx + 224.0, cy, Look::default().flip_x(true), 1.5);
    }
    for (pg, slot, i) in layout(t) {
        if pg != page {
            continue;
        }
        let x = if slot % 2 == 1 { cx + 32.0 } else { cx - 160.0 };
        let y = cy + 75.0 - (slot / 2) as f32 * 37.0;
        let (label, on, desc, disabled) = match TABS[t][i] {
            Toggle(l, k, d) => (l, p.var(k, default_var(k)), d, false),
            Disabled(l, d) => (l, false, d, true),
            Break => continue,
        };
        let id = Btn::MoreToggle(tab, i as u8);
        let f = ctx.factor(id);
        let name = if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" };
        let look = if disabled { Look::scaled(0.7 * f).rgb([150, 150, 150]) } else { Look::scaled(0.7 * f) };
        c.sprite(name, x, y, look);
        if !disabled {
            ctx.area(id, x, y, 31.0 * 0.7 * 1.5, 31.0 * 0.7 * 1.5);
        }
        let col = if disabled { [150, 150, 150] } else { [255, 255, 255] };
        let lines: Vec<&str> = label.split('\n').collect();
        let n = lines.len() as f32;
        for (k, line) in lines.iter().enumerate() {
            let ly = y + (n - 1.0) * 0.5 * 33.5 * 0.35 - k as f32 * 33.5 * 0.35;
            c.text("bigFont", line, x + 20.25, ly, Look::scaled(0.35).anchor(0.0, 0.5).rgb(col));
        }
        if !desc.is_empty() {
            ctx.button(c, Btn::MoreInfo(tab, i as u8), "GJ_infoIcon_001.png", x - 17.0, y + 15.0, Look::scaled(0.5), 2.0);
        }
    }
    // addSpecialContent (0x546110)
    match (t, page) {
        (3, 0) => {
            for (k, (label, key, def, lo, hi, x0)) in [("Buttons\nPer Row:", "bpr", 6.0, 6.0, 12.0, 112.5), ("Button\nRows:", "brows", 2.0, 2.0, 4.0, 304.5)].into_iter().enumerate() {
                let lines: Vec<&str> = label.split('\n').collect();
                for (j, line) in lines.iter().enumerate() {
                    c.text("goldFont", line, cx - 284.5 + x0, cy - 113.0 + 6.9 - j as f32 * 13.8, Look::scaled(0.45).anchor(0.0, 0.5));
                }
                let v = p.number(key, def).clamp(lo, hi);
                let vx = cx - 284.5 + x0 + if k == 0 { 107.0 } else { 95.0 };
                c.text("goldFont", &format!("{}", v as i32), vx, cy - 112.0, Look::scaled(0.85));
                let arrow = if k == 0 { "GJ_arrow_03_001.png" } else { "GJ_arrow_02_001.png" };
                ctx.button(c, Btn::MoreStep(k as u8, false), arrow, vx - 32.0, cy - 114.0, Look::scaled(0.6), 1.5);
                ctx.button(c, Btn::MoreStep(k as u8, true), arrow, vx + 32.0, cy - 114.0, Look::scaled(0.6).flip_x(true), 1.5);
            }
        }
        (4, 0) | (5, 1) => {
            let (label, value) = if t == 4 {
                ("Offset", format!("{}", ((p.number("offset", 0.5) - 0.5) * 2000.0).round() as i32))
            } else {
                ("Scroll", format!("{:.2}", 0.5 + p.number("scroll", 0.5) * 2.0))
            };
            c.text("bigFont", label, cx - 110.0, cy - 100.0, Look::scaled(0.35).anchor(1.0, 0.5));
            ctx.button(c, Btn::MoreInfo(tab, 99), "GJ_infoIcon_001.png", cx - 159.5, cy - 94.0, Look::scaled(0.5), 2.0);
            c.nine_slice("square02_small.png", cx - 78.5, cy - 100.0, 70.0 * 0.7, 30.0 * 0.7, Look::default().alpha(100));
            c.text("bigFont", &value, cx - 78.5, cy - 100.0, Look::scaled(0.42));
            slider_scaled(c, ctx, Btn::SliderSetting(tab), cx - 110.0, cy - 122.4, p.number(if t == 4 { "offset" } else { "scroll" }, 0.5), 0.56);
            if t == 4 {
                ctx.button(c, Btn::MoreSongs, "GJ_savedSongsBtn_001.png", cx + 162.0, cy - 102.0, Look::default(), 1.2);
            } else {
                let id = Btn::MoreClearPlat;
                let f = ctx.factor(id);
                c.nine_slice("GJ_button_04.png", cx + 44.0, cy - 114.0, 128.34 * f, 25.0 * f, Look::default());
                c.text("goldFont", "Clear Plat. Saves", cx + 44.0, cy - 112.0, Look::scaled(0.475 * f));
                ctx.area(id, cx + 44.0, cy - 114.0, 128.34, 25.0);
            }
        }
        _ => {}
    }
}

/// `Slider` at another scale (the settings popup's are 0.56 of the groove's size, and show
/// no fill bar).
pub fn slider_scaled(c: &mut Canvas, ctx: &mut Ctx, id: Btn, x: f32, y: f32, value: f32, s: f32) {
    let (gw, _) = c.size("slidergroove.png");
    ctx.area(id, x, y, (gw - 4.0) * s, 30.0 * s / 0.7);
    let left = x - gw * s * 0.5;
    c.sprite("slidergroove.png", x, y, Look::scaled(s));
    c.sprite("sliderthumb.png", left + (2.0 + (gw - 4.0) * value) * s, y, Look::scaled(s));
}

/// `StatsLayer::customSetup` (0x5bce4): the stats in its order, titles from
/// `StatsCell::getTitleFromKey` (0x1e76cc); values from Cadence's own save.
pub fn stats_rows(p: &Progress) -> Vec<(&'static str, u32)> {
    let main = |id: &i32| (1..=22).contains(id) || *id == 3001;
    let jumps: u32 = p.levels.values().map(|l| l.jumps).sum();
    let attempts: u32 = p.levels.values().map(|l| l.attempts).sum();
    let completed: Vec<i32> = p.levels.iter().filter(|(id, l)| main(id) && l.best >= 100).map(|(id, _)| *id).collect();
    let stars: u32 = completed.iter().map(|id| super::level_info(*id).stars).sum();
    let demons = completed.iter().filter(|id| super::level_info(**id).difficulty == 6).count() as u32;
    let insane = completed.iter().filter(|id| super::level_info(**id).difficulty == 5).count() as u32;
    vec![
        ("Total Jumps:", jumps),
        ("Total Attempts:", attempts),
        ("Collected Stars:", stars),
        ("Collected Moons:", 0),
        ("Collected Diamonds:", 0),
        ("Total orbs collected:", 0),
        ("Completed Levels:", completed.len() as u32),
        ("Completed Online Levels:", 0),
        ("Completed Insane Levels:", insane),
        ("Completed Demon Levels:", demons),
        ("Completed Daily Levels:", 0),
        ("Collected Secret Coins:", p.secret_coins()),
        ("Collected User Coins:", 0),
        ("Completed Map Packs:", 0),
        ("Completed Gauntlets:", 0),
        ("Liked/Disliked Levels:", 0),
        ("Rated Levels:", 0),
        ("Collected List Rewards:", 0),
        ("Players destroyed:", p.number("stat9", 0.0) as u32),
    ]
}

/// The stats list's rows, 40 high, alternating browns (`StatsCell::updateBGColor`).
pub fn draw_stats(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, t: f32, closing: bool, scroll: f32) {
    let (w, h) = (c.width, c.height);
    let dy = dropdown_frame(c, ctx, p, t, closing, "Stats");
    let (lx, ly) = ((w - 356.0) * 0.5, (h - LIST_H) * 0.5 - 5.0 + dy);
    // the rows start 6 above the list's top, under the title bar (measured on the real frame)
    let top = ly + LIST_H + 6.0;
    for (i, (title, value)) in stats_rows(p).iter().enumerate() {
        let y = top - 40.0 * (i as f32 + 1.0) + scroll;
        if y + 40.0 <= ly || y >= top {
            continue;
        }
        // clip the rows to the list (the list's own clipping node)
        let (y0, y1) = (y.max(ly), (y + 40.0).min(top));
        let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(lx, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
        let cy = y + 20.0;
        if cy > ly + 6.0 && cy < top - 6.0 {
            c.text("goldFont", title, lx + 22.0, cy, Look::scaled(0.6).anchor(0.0, 0.5));
            c.text("goldFont", &value.to_string(), lx + 356.0 - 27.0, cy, Look::scaled(0.6).anchor(1.0, 0.5));
        }
    }
    dropdown_border(c, ctx, dy, "Stats");
}

/// How far the stats list scrolls.
pub fn stats_max_scroll(p: &Progress) -> f32 {
    (stats_rows(p).len() as f32 * 40.0 - LIST_H).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_page_by_twelve() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(1), 2);
        assert_eq!(page_count(2), 1);
        assert_eq!(page_count(3), 3);
        assert_eq!(page_count(5), 2);
        // the editor's first page holds ten
        assert_eq!(layout(3).iter().filter(|l| l.0 == 0).count(), 10);
        assert!(layout(1).iter().all(|e| e.1 < 12));
    }
}

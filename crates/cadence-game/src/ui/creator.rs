//! `CreatorLayer::init` (0x2274bc): the 5 × 3 grid of buttons (scale 0.8, 90 apart around the
//! center: x = 90·(column − 2), y = 90·(1 − row)), Versus and The Map grayed out, the vault's
//! lock top right, the treasure room's door bottom right, the back arrow, on the blue gradient.

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx};

/// The buttons in order: sprite, label (for the "not yet" alerts), enabled.
pub const BUTTONS: [(&str, &str, bool); 15] = [
    ("GJ_createBtn_001.png", "Create", true),
    ("GJ_savedBtn_001.png", "Saved", true),
    ("GJ_highscoreBtn_001.png", "Scores", true),
    ("GJ_challengeBtn_001.png", "Quests", true),
    ("GJ_versusBtn_001.png", "Versus", false),
    ("GJ_mapBtn_001.png", "The Map", false),
    ("GJ_dailyBtn_001.png", "Daily", true),
    ("GJ_weeklyBtn_001.png", "Weekly", true),
    ("GJ_eventBtn_001.png", "Event", true),
    ("GJ_gauntletsBtn_001.png", "Gauntlets", true),
    ("GJ_featuredBtn_001.png", "Featured", true),
    ("GJ_listsBtn_001.png", "Lists", true),
    ("GJ_pathsBtn_001.png", "Paths", true),
    ("GJ_mapPacksBtn_001.png", "Map Packs", true),
    ("GJ_searchBtn_001.png", "Search", true),
];

pub fn draw(c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    let (gw, gh) = c.size("GJ_gradientBG.png");
    if gw > 0.0 {
        c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb([0, 102, 255]));
    }
    c.sprite("GJ_sideArt_001.png", -1.0, -1.0, Look::default().anchor(0.0, 0.0));
    c.sprite("GJ_sideArt_001.png", -1.0, h + 1.0, Look::default().anchor(0.0, 1.0).flip_y(true));
    ctx.button(c, Btn::LevelsBack, "GJ_arrow_03_001.png", 25.0, h - 23.0, Look::default(), 1.6);
    for (i, (sprite, _, enabled)) in BUTTONS.iter().enumerate() {
        let (col, row) = (i % 5, i / 5);
        let (x, y) = (cx + 90.0 * (col as f32 - 2.0), cy + 90.0 * (1.0 - row as f32));
        // Versus and The Map: CCSpriteGrayscale, color 175
        let look = if *enabled { Look::scaled(0.8) } else { Look::scaled(0.8).rgb([175, 175, 175]).gray() };
        ctx.button(c, Btn::CreatorItem(i as u8), sprite, x, y, look, 1.0);
    }
    // the vault (50 diamonds) and the treasure room's door
    ctx.button(c, Btn::SetVault, "GJ_lockGray_001.png", w - 22.0, h - 18.0, Look::default(), 1.5);
    c.text("bigFont", "50", w - 31.0, h - 40.0, Look::scaled(0.5));
    c.sprite("GJ_diamondsIcon_001.png", w - 9.0, h - 40.0, Look::scaled(0.7));
    ctx.button(c, Btn::CreatorDoor, "secretDoorBtn_closed_001.png", w - 22.0, 30.0, Look::default(), 1.0);
}

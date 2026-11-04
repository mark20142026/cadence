//! Screens drawn from a real scene's layout (`tools/scene_to_layout.py` turns a harness scene
//! dump into one line per sprite, 9-slice, label, color layer and button). Used for the screens
//! that are mostly fixed art: the Tower's inside, the shops, the vaults, the treasure room.
//! The layout holds positions and frame names only; every texture comes from the player's game.

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx};

/// The screens with a captured layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scene {
    TowerInner,
    /// `GJShopLayer`: 0 the shop, 1 the secret shop, 2 the community shop, 3 the mechanic,
    /// 4 the diamond shop
    Shop(u8),
    /// `SecretLayer` (the vault), `SecretLayer2` (vault of secrets), `SecretLayer3` (the
    /// dungeon), `SecretLayer4` (the chamber of time)
    Secret(u8),
    TreasureRoom,
    /// The level page's popups: `InfoLayer`, `LevelLeaderboard`, `LikeItemLayer`,
    /// `ProfilePage`, `SongInfoLayer`, `AudioAssetsBrowser`
    Info,
    Leaderboard,
    Vote,
    Profile,
    SongInfo,
    Assets,
    /// Settings > Keys (`KeybindingsLayer`)
    Keys,
    /// The icon kit's shards (`ShardsPage`, page 1)
    Shards,
    /// A local level's page (`EditLevelLayer`): name, Edit, Play, Share
    EditLevel,
    /// The editor's Edit Group popup (`SetGroupIDLayer`)
    EditGroup,
    /// Edit Object on a plain object: its color channels (`CustomizeObjectLayer`)
    EditColor,
    /// The editor's pause menu (`EditorPauseLayer`)
    EditorPause,
    /// A text object's text (`CustomizeObjectLayer` in text mode)
    EditText,
    /// Level settings' art pickers (`SelectArtLayer`): 0 background, 1 ground, 2 middleground
    SelectArt(u8),
    /// The editor's level settings (`LevelSettingsLayer`)
    LevelSettings,
    /// A trigger's setup popup (`SetupTriggerPopup` subclasses), by object id
    Trigger(u16),
    /// An object's Edit Special popup (`EditorUI::editObject2`), by object id
    Special(u16),
}

/// The scenes that are FLAlertLayer popups (they open with `FLAlertLayer::show`'s scale-in);
/// the rest are whole screens (a scene transition) or CCBlockLayers (no animation).
pub fn is_popup(scene: Scene) -> bool {
    !matches!(scene, Scene::TowerInner | Scene::Shop(_) | Scene::Secret(_) | Scene::TreasureRoom | Scene::EditLevel | Scene::EditorPause)
}

/// The Edit Special popups with a captured layout.
pub const SPECIAL_POPUPS: [u16; 2] = [1615, 747];

/// The trigger popups with a captured layout.
pub const TRIGGER_POPUPS: [u16; 62] = [1006, 1007, 1049, 1268, 1346, 1347, 1520, 1595, 1611, 1612, 1613, 1616, 1811, 1812, 1814, 1815, 1817, 1818, 1819, 1913, 1914, 1915, 1916, 1917, 1932, 1934, 2015, 2016, 2062, 2066, 2067, 2068, 2899, 2900, 3006, 3007, 3008, 3009, 3010, 3016, 3022, 3029, 3033, 3602, 3603, 3604, 3605, 3606, 3607, 3608, 3609, 3612, 3613, 3614, 3615, 3617, 3619, 3620, 3640, 3662, 899, 901];

/// An editable field of a trigger popup (layout lines I / K / L).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Input { key: u16, x: f32, y: f32, w: f32, h: f32 },
    Toggle { key: u16, x: f32, y: f32, w: f32, h: f32 },
    Slider { key: u16, x: f32, y: f32, min: f32, max: f32 },
}

/// A popup's fields, and the values it showed for a fresh trigger (V lines).
pub fn fields(scene: Scene) -> (Vec<Field>, Vec<(u16, String)>) {
    let mut out = Vec::new();
    let mut values = Vec::new();
    for line in scene.source().lines() {
        let w: Vec<&str> = line.split(' ').collect();
        let key = || w.get(1).and_then(|k| k.parse::<u16>().ok()).unwrap_or(0);
        match w.first().copied() {
            Some("I") => out.push(Field::Input { key: key(), x: f(w.get(2)), y: f(w.get(3)), w: f(w.get(4)), h: f(w.get(5)) }),
            Some("K") => out.push(Field::Toggle { key: key(), x: f(w.get(2)), y: f(w.get(3)), w: f(w.get(4)), h: f(w.get(5)) }),
            Some("L") => out.push(Field::Slider { key: key(), x: f(w.get(2)), y: f(w.get(3)), min: f(w.get(4)), max: f(w.get(5)) }),
            Some("V") => values.push((key(), w.get(2).copied().unwrap_or("").to_string())),
            _ => {}
        }
    }
    (out, values)
}

impl Scene {
    fn source(self) -> &'static str {
        match self {
            Scene::TowerInner => include_str!("scenes/towerinner.txt"),
            Scene::Shop(0) => include_str!("scenes/shop0.txt"),
            Scene::Shop(1) => include_str!("scenes/shop1.txt"),
            Scene::Shop(2) => include_str!("scenes/shop2.txt"),
            Scene::Shop(3) => include_str!("scenes/shop3.txt"),
            Scene::Shop(_) => include_str!("scenes/shop4.txt"),
            Scene::Secret(1) => include_str!("scenes/secret1.txt"),
            Scene::Secret(2) => include_str!("scenes/secret2.txt"),
            Scene::Secret(3) => include_str!("scenes/secret3.txt"),
            Scene::Secret(_) => include_str!("scenes/secret4.txt"),
            Scene::TreasureRoom => include_str!("scenes/secretrewards.txt"),
            Scene::Info => include_str!("scenes/info.txt"),
            Scene::Leaderboard => include_str!("scenes/leaderboard.txt"),
            Scene::Vote => include_str!("scenes/vote.txt"),
            Scene::Profile => include_str!("scenes/profile.txt"),
            Scene::SongInfo => include_str!("scenes/songinfo.txt"),
            Scene::Assets => include_str!("scenes/assets.txt"),
            Scene::Keys => include_str!("scenes/keys.txt"),
            Scene::Shards => include_str!("scenes/shards.txt"),
            Scene::EditLevel => include_str!("scenes/editlevel.txt"),
            Scene::EditGroup => include_str!("scenes/editgroup.txt"),
            Scene::EditColor => include_str!("scenes/editcolor.txt"),
            Scene::EditorPause => include_str!("scenes/editorpause.txt"),
            Scene::EditText => include_str!("scenes/edittext.txt"),
            Scene::SelectArt(0) => include_str!("scenes/artbg.txt"),
            Scene::SelectArt(1) => include_str!("scenes/artg.txt"),
            Scene::SelectArt(_) => include_str!("scenes/artmg.txt"),
            Scene::LevelSettings => include_str!("scenes/levelsettings.txt"),
            Scene::Trigger(id) => trigger_source(id),
            Scene::Special(id) => special_source(id),
        }
    }
}

/// An Edit Special popup's layout (generated: `tools/trigger_popups.py`).
fn special_source(id: u16) -> &'static str {
    match id {
        1615 => include_str!("scenes/s1615.txt"),
        747 => include_str!("scenes/s747.txt"),
        _ => "",
    }
}

/// A trigger popup's layout (generated: `tools/keys_to_layout.py`).
fn trigger_source(id: u16) -> &'static str {
    match id {
        1006 => include_str!("scenes/t1006.txt"),
        1007 => include_str!("scenes/t1007.txt"),
        1049 => include_str!("scenes/t1049.txt"),
        1268 => include_str!("scenes/t1268.txt"),
        1346 => include_str!("scenes/t1346.txt"),
        1347 => include_str!("scenes/t1347.txt"),
        1520 => include_str!("scenes/t1520.txt"),
        1595 => include_str!("scenes/t1595.txt"),
        1611 => include_str!("scenes/t1611.txt"),
        1612 => include_str!("scenes/t1612.txt"),
        1613 => include_str!("scenes/t1613.txt"),
        1616 => include_str!("scenes/t1616.txt"),
        1811 => include_str!("scenes/t1811.txt"),
        1812 => include_str!("scenes/t1812.txt"),
        1814 => include_str!("scenes/t1814.txt"),
        1815 => include_str!("scenes/t1815.txt"),
        1817 => include_str!("scenes/t1817.txt"),
        1818 => include_str!("scenes/t1818.txt"),
        1819 => include_str!("scenes/t1819.txt"),
        1913 => include_str!("scenes/t1913.txt"),
        1914 => include_str!("scenes/t1914.txt"),
        1915 => include_str!("scenes/t1915.txt"),
        1916 => include_str!("scenes/t1916.txt"),
        1917 => include_str!("scenes/t1917.txt"),
        1932 => include_str!("scenes/t1932.txt"),
        1934 => include_str!("scenes/t1934.txt"),
        2015 => include_str!("scenes/t2015.txt"),
        2016 => include_str!("scenes/t2016.txt"),
        2062 => include_str!("scenes/t2062.txt"),
        2066 => include_str!("scenes/t2066.txt"),
        2067 => include_str!("scenes/t2067.txt"),
        2068 => include_str!("scenes/t2068.txt"),
        2899 => include_str!("scenes/t2899.txt"),
        2900 => include_str!("scenes/t2900.txt"),
        3006 => include_str!("scenes/t3006.txt"),
        3007 => include_str!("scenes/t3007.txt"),
        3008 => include_str!("scenes/t3008.txt"),
        3009 => include_str!("scenes/t3009.txt"),
        3010 => include_str!("scenes/t3010.txt"),
        3016 => include_str!("scenes/t3016.txt"),
        3022 => include_str!("scenes/t3022.txt"),
        3029 => include_str!("scenes/t3029.txt"),
        3033 => include_str!("scenes/t3033.txt"),
        3602 => include_str!("scenes/t3602.txt"),
        3603 => include_str!("scenes/t3603.txt"),
        3604 => include_str!("scenes/t3604.txt"),
        3605 => include_str!("scenes/t3605.txt"),
        3606 => include_str!("scenes/t3606.txt"),
        3607 => include_str!("scenes/t3607.txt"),
        3608 => include_str!("scenes/t3608.txt"),
        3609 => include_str!("scenes/t3609.txt"),
        3612 => include_str!("scenes/t3612.txt"),
        3613 => include_str!("scenes/t3613.txt"),
        3614 => include_str!("scenes/t3614.txt"),
        3615 => include_str!("scenes/t3615.txt"),
        3617 => include_str!("scenes/t3617.txt"),
        3619 => include_str!("scenes/t3619.txt"),
        3620 => include_str!("scenes/t3620.txt"),
        3640 => include_str!("scenes/t3640.txt"),
        3662 => include_str!("scenes/t3662.txt"),
        899 => include_str!("scenes/t899.txt"),
        901 => include_str!("scenes/t901.txt"),
        _ => "",
    }
}

fn f(v: Option<&&str>) -> f32 {
    v.and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// Draw a layout; buttons register as `Btn::Replay(index)` (depth-first order in the real
/// scene). `skip` leaves out labels whose text matches (the parts Cadence draws itself).
pub fn draw(c: &mut Canvas, ctx: &mut Ctx, scene: Scene, skip: &dyn Fn(&str) -> bool) {
    // the layout is in a 569 × 320 frame: center it in wider windows
    let dx = (c.width - 569.0) * 0.5;
    for line in scene.source().lines() {
        let w: Vec<&str> = line.split(' ').collect();
        match w.first().copied() {
            Some("S") => {
                let name = w[1].strip_prefix("tex:").unwrap_or(w[1]);
                if skip(&format!("frame:{name}")) {
                    continue;
                }
                let look = Look::default()
                    .scale_xy(f(w.get(4)), f(w.get(5)))
                    .anchor(f(w.get(7)), f(w.get(8)))
                    .rgb([f(w.get(9)) as u8, f(w.get(10)) as u8, f(w.get(11)) as u8])
                    .alpha(f(w.get(12)) as u8);
                let mut look = look;
                look.rotation = f(w.get(6));
                if w.get(13) == Some(&"770,1") || w.get(13) == Some(&"1,1") {
                    look = look.blend(crate::render::Blend::Additive);
                }
                // the gradient background stretches across the window
                if name == "GJ_gradientBG.png" {
                    let (gw, gh) = c.size(name);
                    look = look.scale_xy((c.width + 10.0) / gw.max(1.0), (c.height + 10.0) / gh.max(1.0));
                    c.sprite(name, -5.0, -5.0, look);
                } else {
                    c.sprite(name, f(w.get(2)) + dx, f(w.get(3)), look);
                }
            }
            Some("N") => {
                let look = Look::default().rgb([f(w.get(6)) as u8, f(w.get(7)) as u8, f(w.get(8)) as u8]).alpha(f(w.get(9)) as u8);
                c.nine_slice(w[1], f(w.get(2)) + dx, f(w.get(3)), f(w.get(4)), f(w.get(5)), look);
            }
            Some("T") => {
                let text = w[12..].join(" ");
                if skip(&text) {
                    continue;
                }
                let look = Look::default()
                    .scale_xy(f(w.get(4)), f(w.get(5)))
                    .anchor(f(w.get(6)), f(w.get(7)))
                    .rgb([f(w.get(8)) as u8, f(w.get(9)) as u8, f(w.get(10)) as u8])
                    .alpha(f(w.get(11)) as u8);
                let font = format!("{}", w[1]);
                for (k, part) in text.split('|').enumerate() {
                    c.text(&font, part, f(w.get(2)) + dx, f(w.get(3)) - k as f32 * 30.0 * f(w.get(5)), look);
                }
            }
            Some("R") => {
                let col = [f(w.get(5)) / 255.0, f(w.get(6)) / 255.0, f(w.get(7)) / 255.0];
                c.rect(f(w.get(1)) + dx, f(w.get(2)), f(w.get(3)), f(w.get(4)), col, f(w.get(8)) / 255.0);
            }
            Some("B") => {
                let i = f(w.get(1)) as u8;
                ctx.area(Btn::Replay(i), f(w.get(2)) + dx, f(w.get(3)), f(w.get(4)), f(w.get(5)));
            }
            _ => {}
        }
    }
}

/// A layout's button `i` center (its B line).
pub fn button_center(scene: Scene, i: u8) -> Option<(f32, f32)> {
    scene.source().lines().find_map(|l| {
        let w: Vec<&str> = l.split(' ').collect();
        (w.first() == Some(&"B") && w.get(1).and_then(|v| v.parse::<u8>().ok()) == Some(i)).then(|| (f(w.get(2)), f(w.get(3))))
    })
}

/// The icon sheets a layout's frames come from (`player_37_001.png` → `player_37`).
pub fn icon_sheets(scene: Scene) -> Vec<String> {
    const KINDS: [&str; 9] = ["player_ball_", "player_", "ship_", "bird_", "dart_", "robot_", "spider_", "swing_", "jetpack_"];
    let mut out: Vec<String> = Vec::new();
    for line in scene.source().lines() {
        let Some(frame) = line.strip_prefix("S ").and_then(|l| l.split(' ').next()) else { continue };
        for k in KINDS {
            if let Some(rest) = frame.strip_prefix(k) {
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !num.is_empty() {
                    out.push(format!("{k}{num}"));
                }
                break;
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

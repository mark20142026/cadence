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
    /// `SecretRewardsLayer`'s pages: 0 / 1 the chest tiers, 2 the bonuses, 3 the gold
    /// chest, 4 the shops' ropes
    Treasure(u8),
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
    /// `CreateParticlePopup`'s Motion, Visual and Extra tabs (`ui::particle`)
    Particle(u8),
    /// Your own `ProfilePage`, `FriendsProfilePage`, `FRequestProfilePage`,
    /// `GJAccountSettingsLayer` (`ui::social`)
    MyProfile,
    Friends,
    Requests,
    AcctSettings,
    /// Your level's Share popup (`ShareLevelLayer`, `ui::share`)
    ShareLevel,
    /// The legacy messages (`MessagesProfilePage`) and `NotificationsPage` (`ui::social`)
    OldMessages,
    Notifications,
    /// `MessageThreadsPage`: the newer messages (threads), its rows left out
    Threads,
}

/// The scenes that are FLAlertLayer popups (they open with `FLAlertLayer::show`'s scale-in);
/// the rest are whole screens (a scene transition) or CCBlockLayers (no animation).
pub fn is_popup(scene: Scene) -> bool {
    !matches!(scene, Scene::TowerInner | Scene::Shop(_) | Scene::Secret(_) | Scene::Treasure(_) | Scene::EditLevel | Scene::EditorPause)
}

/// The Edit Special popups with a captured layout.
pub const SPECIAL_POPUPS: [u16; 10] = [1594, 1615, 1704, 1751, 2063, 2065, 2902, 2903, 3027, 747];

/// The trigger popups with a captured layout.
pub const TRIGGER_POPUPS: [u16; 126] = [1006, 1007, 1049, 105, 1268, 1334, 1346, 1347, 1520, 1585, 1595, 1611, 1612, 1613, 1616, 1811, 1812, 1814, 1815, 1817, 1818, 1819, 1912, 1913, 1914, 1915, 1916, 1917, 1931, 1932, 1934, 1935, 2015, 2016, 2062, 2066, 2067, 2068, 22, 23, 24, 25, 26, 27, 28, 2899, 29, 2900, 2901, 2904, 2905, 2907, 2909, 2910, 2911, 2912, 2913, 2914, 2915, 2916, 2917, 2919, 2920, 2921, 2922, 2923, 2924, 2925, 2999, 30, 3006, 3007, 3008, 3009, 3010, 3016, 3017, 3018, 3019, 3020, 3021, 3022, 3023, 3024, 3029, 3030, 3031, 3033, 31, 32, 33, 3600, 3602, 3603, 3604, 3605, 3606, 3607, 3608, 3609, 3612, 3613, 3614, 3615, 3617, 3618, 3619, 3620, 3640, 3641, 3642, 3655, 3660, 3661, 3662, 3663, 55, 56, 57, 58, 59, 744, 899, 900, 901, 915];

/// An editable field of a trigger popup (layout lines I / K / L).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Input { key: u16, x: f32, y: f32, w: f32, h: f32 },
    Toggle { key: u16, x: f32, y: f32, w: f32, h: f32 },
    Slider { key: u16, x: f32, y: f32, min: f32, max: f32 },
}

/// The centers of a layout's 9-slices with this texture.
pub fn nine_slices(scene: Scene, name: &str) -> Vec<(f32, f32)> {
    scene
        .source()
        .lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            (w.first() == Some(&"N") && w.get(1) == Some(&name)).then(|| (f(w.get(2)), f(w.get(3))))
        })
        .collect()
}

/// The popup's slider grooves in layout order (center, scale): the n-th slider field (L
/// lines, in the same order) runs along the n-th groove.
pub fn slider_grooves(scene: Scene) -> Vec<(f32, f32, f32, f32)> {
    scene
        .source()
        .lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            (w.first() == Some(&"S") && w.get(1).is_some_and(|n| n.ends_with("slidergroove.png"))).then(|| (f(w.get(2)), f(w.get(3)), f(w.get(4)), f(w.get(5))))
        })
        .collect()
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
            Scene::Treasure(0) => include_str!("scenes/treasure0.txt"),
            Scene::Treasure(1) => include_str!("scenes/treasure1.txt"),
            Scene::Treasure(2) => include_str!("scenes/treasure2.txt"),
            Scene::Treasure(3) => include_str!("scenes/treasure3.txt"),
            Scene::Treasure(_) => include_str!("scenes/treasure4.txt"),
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
            Scene::Particle(0) => include_str!("scenes/particle_motion.txt"),
            Scene::Particle(1) => include_str!("scenes/particle_visual.txt"),
            Scene::Particle(_) => include_str!("scenes/particle_extra.txt"),
            Scene::MyProfile => include_str!("scenes/myprofile.txt"),
            Scene::Friends => include_str!("scenes/friends.txt"),
            Scene::Requests => include_str!("scenes/requests.txt"),
            Scene::AcctSettings => include_str!("scenes/acctsettings.txt"),
            Scene::ShareLevel => include_str!("scenes/share.txt"),
            Scene::OldMessages => include_str!("scenes/oldmessages.txt"),
            Scene::Notifications => include_str!("scenes/notifications.txt"),
            Scene::Threads => include_str!("scenes/threads.txt"),
        }
    }
}

/// An Edit Special popup's layout (generated: `tools/trigger_popups.py`).
fn special_source(id: u16) -> &'static str {
    match id {
        1594 => include_str!("scenes/s1594.txt"),
        1615 => include_str!("scenes/s1615.txt"),
        1704 => include_str!("scenes/s1704.txt"),
        1751 => include_str!("scenes/s1751.txt"),
        2063 => include_str!("scenes/s2063.txt"),
        2065 => include_str!("scenes/s2065.txt"),
        2902 => include_str!("scenes/s2902.txt"),
        2903 => include_str!("scenes/s2903.txt"),
        3027 => include_str!("scenes/s3027.txt"),
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
        105 => include_str!("scenes/t105.txt"),
        1268 => include_str!("scenes/t1268.txt"),
        1334 => include_str!("scenes/t1334.txt"),
        1346 => include_str!("scenes/t1346.txt"),
        1347 => include_str!("scenes/t1347.txt"),
        1520 => include_str!("scenes/t1520.txt"),
        1585 => include_str!("scenes/t1585.txt"),
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
        1912 => include_str!("scenes/t1912.txt"),
        1913 => include_str!("scenes/t1913.txt"),
        1914 => include_str!("scenes/t1914.txt"),
        1915 => include_str!("scenes/t1915.txt"),
        1916 => include_str!("scenes/t1916.txt"),
        1917 => include_str!("scenes/t1917.txt"),
        1931 => include_str!("scenes/t1931.txt"),
        1932 => include_str!("scenes/t1932.txt"),
        1934 => include_str!("scenes/t1934.txt"),
        1935 => include_str!("scenes/t1935.txt"),
        2015 => include_str!("scenes/t2015.txt"),
        2016 => include_str!("scenes/t2016.txt"),
        2062 => include_str!("scenes/t2062.txt"),
        2066 => include_str!("scenes/t2066.txt"),
        2067 => include_str!("scenes/t2067.txt"),
        2068 => include_str!("scenes/t2068.txt"),
        22 => include_str!("scenes/t22.txt"),
        23 => include_str!("scenes/t23.txt"),
        24 => include_str!("scenes/t24.txt"),
        25 => include_str!("scenes/t25.txt"),
        26 => include_str!("scenes/t26.txt"),
        27 => include_str!("scenes/t27.txt"),
        28 => include_str!("scenes/t28.txt"),
        2899 => include_str!("scenes/t2899.txt"),
        29 => include_str!("scenes/t29.txt"),
        2900 => include_str!("scenes/t2900.txt"),
        2901 => include_str!("scenes/t2901.txt"),
        2904 => include_str!("scenes/t2904.txt"),
        2905 => include_str!("scenes/t2905.txt"),
        2907 => include_str!("scenes/t2907.txt"),
        2909 => include_str!("scenes/t2909.txt"),
        2910 => include_str!("scenes/t2910.txt"),
        2911 => include_str!("scenes/t2911.txt"),
        2912 => include_str!("scenes/t2912.txt"),
        2913 => include_str!("scenes/t2913.txt"),
        2914 => include_str!("scenes/t2914.txt"),
        2915 => include_str!("scenes/t2915.txt"),
        2916 => include_str!("scenes/t2916.txt"),
        2917 => include_str!("scenes/t2917.txt"),
        2919 => include_str!("scenes/t2919.txt"),
        2920 => include_str!("scenes/t2920.txt"),
        2921 => include_str!("scenes/t2921.txt"),
        2922 => include_str!("scenes/t2922.txt"),
        2923 => include_str!("scenes/t2923.txt"),
        2924 => include_str!("scenes/t2924.txt"),
        2925 => include_str!("scenes/t2925.txt"),
        2999 => include_str!("scenes/t2999.txt"),
        30 => include_str!("scenes/t30.txt"),
        3006 => include_str!("scenes/t3006.txt"),
        3007 => include_str!("scenes/t3007.txt"),
        3008 => include_str!("scenes/t3008.txt"),
        3009 => include_str!("scenes/t3009.txt"),
        3010 => include_str!("scenes/t3010.txt"),
        3016 => include_str!("scenes/t3016.txt"),
        3017 => include_str!("scenes/t3017.txt"),
        3018 => include_str!("scenes/t3018.txt"),
        3019 => include_str!("scenes/t3019.txt"),
        3020 => include_str!("scenes/t3020.txt"),
        3021 => include_str!("scenes/t3021.txt"),
        3022 => include_str!("scenes/t3022.txt"),
        3023 => include_str!("scenes/t3023.txt"),
        3024 => include_str!("scenes/t3024.txt"),
        3029 => include_str!("scenes/t3029.txt"),
        3030 => include_str!("scenes/t3030.txt"),
        3031 => include_str!("scenes/t3031.txt"),
        3033 => include_str!("scenes/t3033.txt"),
        31 => include_str!("scenes/t31.txt"),
        32 => include_str!("scenes/t32.txt"),
        33 => include_str!("scenes/t33.txt"),
        3600 => include_str!("scenes/t3600.txt"),
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
        3618 => include_str!("scenes/t3618.txt"),
        3619 => include_str!("scenes/t3619.txt"),
        3620 => include_str!("scenes/t3620.txt"),
        3640 => include_str!("scenes/t3640.txt"),
        3641 => include_str!("scenes/t3641.txt"),
        3642 => include_str!("scenes/t3642.txt"),
        3655 => include_str!("scenes/t3655.txt"),
        3660 => include_str!("scenes/t3660.txt"),
        3661 => include_str!("scenes/t3661.txt"),
        3662 => include_str!("scenes/t3662.txt"),
        3663 => include_str!("scenes/t3663.txt"),
        55 => include_str!("scenes/t55.txt"),
        56 => include_str!("scenes/t56.txt"),
        57 => include_str!("scenes/t57.txt"),
        58 => include_str!("scenes/t58.txt"),
        59 => include_str!("scenes/t59.txt"),
        744 => include_str!("scenes/t744.txt"),
        899 => include_str!("scenes/t899.txt"),
        900 => include_str!("scenes/t900.txt"),
        901 => include_str!("scenes/t901.txt"),
        915 => include_str!("scenes/t915.txt"),
        _ => "",
    }
}

fn f(v: Option<&&str>) -> f32 {
    v.and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// Draw a layout; buttons register as `Btn::Replay(index)` (depth-first order in the real
/// scene). `skip` leaves out labels whose text matches (the parts Cadence draws itself).
pub fn draw(c: &mut Canvas, ctx: &mut Ctx, scene: Scene, skip: &dyn Fn(&str) -> bool) {
    draw_where(c, ctx, scene, &|kind, name, _, _| match kind {
        'S' => !skip(&format!("frame:{name}")),
        'T' => !skip(name),
        _ => true,
    });
}

/// `draw` with a filter that sees each item: its kind (S sprite, N 9-slice, T label, R color
/// rectangle, B button), its frame / text / texture (a button: its index), and its position in the layout's
/// 569 × 320 frame. The pages that draw their own rows leave the captured rows out with it.
pub fn draw_where(c: &mut Canvas, ctx: &mut Ctx, scene: Scene, keep: &dyn Fn(char, &str, f32, f32) -> bool) {
    // the layout is in a 569 × 320 frame: center it in wider windows
    let dx = (c.width - 569.0) * 0.5;
    for line in scene.source().lines() {
        let w: Vec<&str> = line.split(' ').collect();
        let Some(kind) = w.first().and_then(|k| (k.len() == 1).then(|| k.chars().next()).flatten()) else { continue };
        let (x, y) = match kind {
            'R' => (f(w.get(1)), f(w.get(2))),
            _ => (f(w.get(2)), f(w.get(3))),
        };
        let name = match kind {
            'S' => w.get(1).map(|n| n.strip_prefix("tex:").unwrap_or(n)).unwrap_or(""),
            'T' => line.splitn(13, ' ').nth(12).unwrap_or(""),
            'N' => w.get(1).copied().unwrap_or(""),
            // a button's index
            'B' => w.get(1).copied().unwrap_or(""),
            _ => "",
        };
        if matches!(kind, 'S' | 'N' | 'T' | 'R' | 'B') && !keep(kind, name, x, y) {
            continue;
        }
        match w.first().copied() {
            Some("S") => {
                let name = w[1].strip_prefix("tex:").unwrap_or(w[1]);
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

/// Whether a popup's input shows decimals (a D line: the game showed "1.00").
pub fn is_decimal(scene: Scene, key: u16) -> bool {
    scene.source().lines().any(|l| l.strip_prefix("D ").and_then(|k| k.trim().parse::<u16>().ok()) == Some(key))
}

/// Whether a layout draws this frame.
pub fn has_frame(scene: Scene, frame: &str) -> bool {
    scene.source().lines().any(|l| l.strip_prefix("S ").is_some_and(|r| r.split(' ').next() == Some(frame)))
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

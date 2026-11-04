//! The menus, laid out from the decompiled game (addresses in NOTES.md, "GUI"): the main menu
//! (`MenuLayer` over `MenuGameLayer`), the level select (`LevelSelectLayer` / `LevelPage` in a
//! `BoomScrollLayer`), the pause menu (`PauseLayer`), the level-complete screen
//! (`EndLevelLayer`), and Cadence's own progress save.
//!
//! Immediate mode: each frame the screens draw themselves into a `Canvas` (screen points, the
//! game's 320-point-tall view) and register their buttons' hit areas; input is matched against
//! the last frame's buttons. The app owns the level (`Game`); the UI only asks it to do things
//! through `Action`s and hears about it through `LevelStatus`.

pub mod canvas;
mod end;
pub mod font;
mod levels;
mod menu;
pub mod online;
pub mod options;
pub mod garage;
mod creator;
mod achievements;
mod achievements_table;
mod pause;
pub mod save;
mod subpages;
mod popups;
pub mod editor;
mod editor_tabs;
mod replay;
mod gauntlets;
mod login;
mod register;
mod social;
mod share;

use std::collections::HashMap;

use crate::assets::Sheets;
use crate::audio::Audio;
use crate::files::Files;
use crate::render::{Draw, Renderer};

use canvas::{Canvas, Press};
use font::Font;
use save::Progress;

pub use levels::{LevelInfo, level_info};

/// Single images the menus use (loaded with the best resolution the game has for each).
const IMAGES: &[&str] = &[
    "GJ_gradientBG", "GJ_square01", "GJ_square02", "GJ_square03", "GJ_square04", "GJ_square05", "GJ_square06", "GJ_square07",
    "GJ_button_01", "GJ_button_02", "GJ_button_03", "GJ_button_04", "GJ_button_05", "GJ_button_06",
    "square01_001", "square02_001", "square02_small", "square02b_001", "square02b_002", "square02b_small", "square02c_001",
    "square02d_001", "square04_001", "GJ_progressBar_001", "slidergroove", "sliderthumb", "sliderBar", "slidergroove_02", "GJ_moveBtn", "GJ_moveSBtn",
    "tutorial_01", "tutorial_02", "tutorial_03", "tutorial_04", "tutorial_05", "smallDot", "loadingCircle", "dialogIcon_001", "dialogIcon_002",
    "dialogIcon_003", "dialogIcon_004", "dialogIcon_005", "dialogIcon_006", "mapPromo",
];
/// Sheets the menus need beyond the level's.
const SHEETS: &[&str] = &[
    "GJ_LaunchSheet", "GauntletSheet", "GJ_ShopSheet", "GJ_PathSheet", "GJ_ShopSheet01", "GJ_ShopSheet02", "GJ_ShopSheet03", "DungeonSheet",
    "SecretSheet", "TowerSheet", "TreasureRoomSheet", "GJ_GameSheetEditor", "CCControlColourPickerSpriteSheet",
];
/// The menu music (`GameManager::fadeInMenuMusic`).
const MENU_MUSIC: &str = "menuLoop.mp3";
/// `LevelPage` plays this when a level is chosen.
const PLAY_SOUND: &str = "playSound_01.ogg";

/// Files the menus read, for the web build's file list.
pub fn needed_files(suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    for s in SHEETS {
        out.push(format!("{s}{suffix}.plist"));
        out.push(format!("{s}{suffix}.png"));
    }
    for f in font::FONTS {
        out.push(format!("{f}{suffix}.fnt"));
        out.push(format!("{f}{suffix}.png"));
    }
    for i in IMAGES {
        for s in ["-uhd", "-hd", ""] {
            out.push(format!("{i}{s}.png"));
        }
    }
    // (sounds are read by the audio backend straight from the game's folder)
    out
}

/// The menus' fonts, loaded with the sheets (their pages become sheets too).
pub struct UiAssets {
    pub fonts: Vec<Font>,
}

/// Load the menus' sheets, images and fonts into `sheets` (before the sheets are uploaded).
pub fn load_assets(files: &dyn Files, sheets: &mut Sheets) -> UiAssets {
    for name in SHEETS {
        if let Err(e) = sheets.load(files, "", name) {
            crate::app::log(&format!("sheet {name}: {e}"));
        }
    }
    for name in IMAGES {
        if let Err(e) = sheets.load_single_best(files, name) {
            crate::app::log(&format!("{name}: {e}"));
        }
    }
    let mut fonts = Vec::new();
    for name in font::FONTS {
        match Font::load(files, sheets, name) {
            Ok(f) => fonts.push(f),
            Err(e) => crate::app::log(&format!("font {name}: {e}")),
        }
    }
    UiAssets { fonts }
}

/// The menu's links (MenuLayer's onFacebook, onTwitter, onYouTube, onTwitch, onDiscord,
/// onRobTop, onNewgrounds), as the game has them.
/// Then the support page's (7 privacy, 8 terms, 9 cocos2d-x) and Rate (10, the Steam page).
pub const LINKS: [&str; 14] = [
    "https://www.facebook.com/geometrydash",
    "https://twitter.com/robtopgames",
    "https://www.youtube.com/user/RobTopGames",
    "https://www.twitch.tv/directory/category/geometry-dash",
    "https://discord.com/invite/geometrydash",
    "https://www.robtopgames.com",
    "https://www.newgrounds.com/audio",
    "https://www.robtopgames.com/privacy",
    "https://www.robtopgames.com/tos",
    "https://www.cocos2d-x.org",
    "https://store.steampowered.com/recommended/recommendgame/322170",
    // AccountLoginLayer::onForgotUser / onForgotPassword
    "https://www.boomlings.com/database/accounts/lostusername.php",
    "https://www.boomlings.com/database/accounts/lostpassword.php",
    // AccountLayer's Help, AccountHelpLayer's Manage Account
    "https://www.boomlings.com/database/accounts/accountManagement.php",
];

/// `CCTransitionFade`'s length (half out to black, half back in).
const FADE_TIME: f32 = 0.5;

/// The fade's black at time `t`.
fn fade_alpha(t: f32) -> f32 {
    let h = FADE_TIME * 0.5;
    if t < h { t / h } else { (1.0 - (t - h) / h).max(0.0) }
}

/// What the UI asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Start a level (from the level select).
    Play(i32),
    /// Close the app (the main menu's close button, natively).
    Quit,
    /// The pause menu opened: pause the music.
    Pause,
    /// The pause menu closed: resume the music and the level.
    Resume,
    /// The menu music setting changed (play or stop it).
    MenuMusic,
    /// Open one of `LINKS` in the browser (the menu's social buttons).
    OpenLink(u8),
    /// Unlink the logged-in account (`AccountHelpLayer::doUnlink`).
    Unlink,
    /// The Account page's Save and Load, confirmed (`AccountLayer::doBackup` / `doSync`).
    CloudSave,
    CloudLoad,
    /// A received friend request accepted (its row on the Requests page).
    AcceptRequest(u16),
    /// A level reported (`reportGJLevel.php`), confirmed.
    ReportLevel(i32),
    /// My Levels' shown levels deleted (Select All, confirmed); one of your levels deleted.
    DeleteMine,
    DeleteMyLevel(u16),
    /// The editor's music: play its song from this many seconds, or stop (None).
    EditorMusic(Option<f32>),
    /// A saved level deleted, confirmed; its low detail mode switched.
    DeleteSaved(i32),
    ToggleLdm(i32),
    /// Restart the level from the start (pause menu, end screen).
    Restart,
    /// Leave the level for the level select.
    LeaveLevel,
    /// Enter (true) or leave practice mode; the level resumes.
    Practice(bool),
    /// An online level finished downloading (`Ui::take_online`).
    PlayOnline,
    /// Graphics > Apply: full screen, or windowed at this size (points × 2 pixels).
    Video { fullscreen: bool, width: u32, height: u32 },
    /// The editor opened: the app builds its level for drawing.
    EnterEditor,
    /// Leave the editor for the menus.
    LeaveEditor,
}

/// What the app tells the UI about the level each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelStatus {
    pub dead: bool,
    /// Seconds since the level was completed, if it was.
    pub end_time: Option<f32>,
    /// Progress through the level, 0..100 (`PlayLayer::updateProgressbar`).
    pub percent: f32,
    /// The level's secret coins taken this attempt, numbered left to right.
    pub coins: [bool; 3],
    /// How many secret coins the level has.
    pub coin_count: usize,
    /// Physics steps since the attempt started (a drop means a restart).
    pub steps: u32,
    /// Practice mode: deaths and completion count for the practice best only.
    pub practice: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Levels,
    Online,
    Garage,
    Creator,
    Level,
    /// The level editor (the level is drawn under its UI).
    Editor,
}

/// Every clickable thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Btn {
    // MenuLayer
    MenuPlay,
    MenuGarage,
    MenuCreator,
    MenuAchievements,
    MenuOptions,
    MenuStats,
    MenuNewgrounds,
    MenuRobTop,
    // the search's Advanced Options popup
    FilterToggle(u8),
    FilterClose,
    // the level page's Rate Difficulty popup
    RateStar(u8),
    RateCancel,
    RateSubmit,
    /// A color popup's picker (hue ring and saturation / brightness square)
    Picker,
    /// Your account's pages' row buttons (`ui::social::row_btn`)
    Social(u16),
    // the messages page's back / close
    MsgClose,
    // the post / message box
    WriteField(u8),
    WriteCancel,
    WriteSend,
    MenuSocial(u8),
    MenuNews,
    MenuClose,
    MenuProfile,
    MenuDaily,
    MenuMoreGames,
    // LevelSelectLayer / LevelPage
    LevelsBack,
    LevelsPrev,
    LevelsNext,
    LevelsInfo,
    LevelsDeath,
    LevelsDownload,
    LevelBox,
    TowerDoor,
    SecretDoor,
    // PauseLayer
    PausePractice,
    PauseResume,
    PauseMenu,
    PauseRestart,
    PauseHide,
    PauseSide(u8),
    EndHide,
    // UILayer
    PauseButton,
    // EndLevelLayer
    EndReplay,
    EndMenu,
    EndDeath,
    // the online browser
    OnlineSearch,
    OnlineTab(u8),
    OnlinePlay(u8),
    // OptionsLayer / MoreOptionsLayer
    DropBack,
    SetAccount,
    SetHowTo,
    SetOptions,
    SetGraphics,
    SetRate,
    SetParent,
    SetHelp,
    SetVault,
    SetMenuMusic,
    MoreClose,
    MorePrev,
    MoreNext,
    MoreKeys,
    /// (tab, row) in `options::TABS`
    MoreToggle(u8, u8),
    MoreInfo(u8, u8),
    MoreTab(u8),
    /// editor buttons per row (0) / rows (1), up or down
    MoreStep(u8, bool),
    MoreSongs,
    MoreClearPlat,
    // AccountLayer, SupportLayer, SongsLayer
    AccLogin,
    AccHelp,
    AccRegister,
    AccSave,
    AccLoad,
    AccMore,
    // AccountHelpLayer (the logged-in Account page's More)
    AccRefreshLogin,
    AccManage,
    AccUnlink,
    // AccountRegisterLayer: its five fields, Cancel, Submit
    RegField(u8),
    RegCancel,
    RegSubmit,
    // AccountLoginLayer
    LoginUser,
    LoginPass,
    LoginForgotUser,
    LoginForgotPass,
    LoginCancel,
    LoginSubmit,
    SupLinks,
    SupContact,
    SupSfx,
    SupPrivacy,
    SupTos,
    SupRobTop,
    SupCocos,
    SupLowDetail,
    SongView(u8),
    // online::Browser
    OnlineClear,
    OnlineQuick(u8),
    OnlineDiff(u8),
    OnlineLen(u8),
    OnlineStar,
    OnlineClearFilters,
    /// The demon filter's + (`LevelSearchLayer::onSpecialDemon`), a face in its popup (by
    /// tag: 0 all, 7 easy, 8 medium, 6 hard, 9 insane, 10 extreme) and its OK.
    OnlineDemonFilter,
    OnlineDemonPick(u8),
    OnlineDemonOk,
    OnlineUsers,
    OnlineFilters,
    OnlineLists,
    OnlineRefresh,
    OnlineHelp,
    OnlineView(u8),
    OnlinePack(u8),
    OnlineList(u8),
    GauntletCard(u8),
    ScoreTab(u8),
    GauntletPage(bool),
    InfoPlay,
    InfoSide(u8),
    InfoSongMore,
    InfoSongAssets,
    InfoAuthor,
    DailyClose,
    PathIcon(u8),
    NewsClose,
    NewsPage(bool),
    DialogNext,
    ConfirmNo,
    ConfirmYes,
    MapPromoClose,
    DailySafe,
    DailyPlay,
    // editor::Editor
    EdMode(u8),
    EdTab(u8),
    EdPage(bool),
    EdObject(u8),
    EdEdit(u8),
    EdUndo,
    EdRedo,
    EdTrash,
    EdDeselect,
    EdHide,
    EdZoom(bool),
    EdSwipe,
    EdRotateToggle,
    EdFreeMove,
    EdSnap,
    EdCopy,
    EdPaste,
    EdDuplicate,
    EdPlaytest,
    EdMusic,
    EdPause,
    EdSide(u8),
    /// the editor's pause menu: 0 resume, 1 save and play, 2 save and exit, 3 save, 4 exit
    EdPauseItem(u8),
    MyNew,
    MyLevel(u8),
    MyTool(u8),
    /// a button of a replayed scene (its depth-first index)
    Replay(u8),
    /// A trigger popup's text input / toggle, by property key
    TrigInput(u16),
    /// The leaderboard's stat buttons
    ScoreStat(u8),
    /// The editor's layer arrows (0 down, 1 up, 2 all)
    EdLayer(u8),
    TrigToggle(u16),
    // popups::Popup
    PopClose,
    PopNext,
    VidAdvanced,
    VidFullscreen,
    VidBorderless,
    VidFix,
    VidRes(bool),
    VidQuality(bool),
    VidApply,
    ParLock,
    ParPage,
    ParToggle(u8),
    ParInfo(u8),
    RewardFree,
    RewardChest(u8),
    /// the settings popup's slider on tab N (audio offset, other: scroll)
    SliderSetting(u8),
    // AchievementsLayer
    AchPrev,
    AchNext,
    // CreatorLayer
    CreatorItem(u8),
    CreatorDoor,
    // GJGarageLayer
    GarageTab(u8),
    GarageIcon(u8),
    GaragePrev,
    GarageNext,
    GarageDot(u8),
    GaragePaint,
    GarageShards,
    GarageShop,
    GarageColorTab(u8),
    GarageColorClose,
    GarageColor(u8),
    // sliders (dragged, not clicked)
    SliderMusic,
    SliderSfx,
    // FLAlertLayer
    AlertOk,
}

/// A button's hit area this frame (center, half size) and whether it reacts.
#[derive(Clone, Copy, Debug)]
pub struct HitArea {
    pub id: Btn,
    pub x: f32,
    pub y: f32,
    pub hw: f32,
    pub hh: f32,
}

/// The current play session of a level (for the end screen and the save).
#[derive(Clone, Debug, Default)]
struct Session {
    id: i32,
    attempts: u32,
    jumps: u32,
    time: f32,
    last: LevelStatus,
    completed: bool,
    /// Coins newly collected by the completing attempt (for the end screen's animation).
    new_coins: [bool; 3],
    /// The completing attempt was the level's first completion (the end screen's rewards).
    first_completion: bool,
}

/// A finished cloud Save or Load.
pub enum CloudDone {
    /// Backed up: the two saves' sizes in MB (profile, levels).
    Saved(f32, f32),
    Loaded(Box<crate::cloud::CloudSave>),
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// An exported level file: the Downloads folder on the desktop, a download in the browser.
#[cfg(not(target_arch = "wasm32"))]
fn export_level(name: &str, bytes: &[u8]) -> Result<String, String> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").ok_or("no home folder")?);
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    let path = home.join("Downloads").join(format!("{safe}.gmd"));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg(target_arch = "wasm32")]
fn export_level(name: &str, bytes: &[u8]) -> Result<String, String> {
    crate::audio::web_download(&format!("{name}.gmd"), bytes);
    Ok(format!("{name}.gmd"))
}

fn rgb_to_hsv([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

/// Stars from the main levels completed (stat 6, which `onBackup` wants at least 10 of).
fn total_stars(p: &save::Progress) -> u32 {
    p.levels.iter().filter(|(id, l)| ((1..=22).contains(*id) || **id == 3001 || (5001..=5004).contains(*id)) && l.best >= 100).map(|(id, _)| level_info(*id).stars).sum()
}

/// A message box (`FLAlertLayer`): title, text, OK.
#[derive(Clone, Debug)]
pub struct Alert {
    pub title: String,
    pub text: String,
    pub t: f32,
}

/// An FLAlertLayer with two buttons (`btn1` cancels, `btn2` does `action`).
pub struct Confirm {
    pub title: String,
    pub text: String,
    pub btn1: String,
    pub btn2: String,
    pub action: Action,
    pub t: f32,
}

/// `DialogLayer` (0x337818): a character's lines, one page a click.
pub struct Dialog {
    pub title: String,
    pub lines: Vec<String>,
    pub page: usize,
    /// `dialogIcon_%03d.png`
    pub icon: u8,
    /// `GJ_square%02d.png`
    pub style: u8,
    pub t: f32,
}

/// The Keymaster's lines at the vault's door (`CreatorLayer::onSecretVault`), cycled.
const VAULT_LINES: [&str; 11] = [
    "You are not ready...",
    "I can't let you in.",
    "Player used knock.<d040>\nIt's not very effective.",
    "Don't you have something better to do?",
    "That is a bad idea...",
    ".<d030>.<d030>.",
    "I guess you don't like rules...",
    "RubRub told me not to let anyone in.",
    "This is not the door you are looking for.",
    "I don't understand why you keep clicking...",
    "Trust me,<d030> you don't want to come in here.",
];

/// `CreatorLayer::onMultiplayer`: the Keymaster explains Versus.
const VERSUS_LINES: [&str; 11] = [
    "Versus mode has been delayed, but it's in the works.",
    "Yea<d010>.<d010>.<d010>. It's still <cl>delayed</c>.",
    "I blame <cg>RobTop</c>.",
    "Who else could we <cr>blame</c>?\nWe're not even real.",
    "Speak for yourself <co>I'm real!</c>",
    "<cl>Yea, a real pain in the<d010>.<d010>.<d010>.</c>",
    "You're just <cr>mad</c> because you don't have a <cy>shop</c>.",
    "You have a <cy>shop</c>?",
    "<co>I HAVE SAID TOO MUCH!\nQUICKLY TO THE <i050><s260>CHOPPER!</s></i></c>",
    "You don't have a chop<d010>.<d010>.<d010>.",
    ".<d010>.<d010>.<d010>.<d010>.",
];

pub struct Ui {
    pub screen: Screen,
    pub fonts: Vec<Font>,
    pub progress: Progress,
    menu: menu::Menu,
    levels: levels::LevelSelect,
    pub online: online::Browser,
    /// The robot and spider rigs (set by the app), for their icons.
    pub rigs: Option<[crate::anim::AnimDesc; 2]>,
    /// The settings over the main menu (dropdown, then the toggle pages).
    settings: Option<options::Settings>,
    pub garage: garage::Garage,
    /// The online level being played (its name, and where to go back to).
    online_session: Option<crate::online::LevelInfo>,
    pause: Option<pause::Pause>,
    /// The pause menu (or the end screen) hidden with its hide button.
    pause_hidden: bool,
    end: Option<end::End>,
    session: Option<Session>,
    alert: Option<Alert>,
    /// How To Play, Graphics or Parental Control, over the settings
    popup: Option<popups::Popup>,
    /// The level editor's state while it's open; the world rectangle it's drawn in.
    pub editor: Option<editor::Editor>,
    editor_view: (f32, f32, f32, f32),
    /// The editor's pause menu is up.
    editor_paused: bool,
    /// The level being played is the editor's (leaving returns to it).
    editor_playtest: bool,
    dialog: Option<Dialog>,
    /// A screen drawn from a captured layout (tower, shops, vaults), over everything, and the
    /// screen it returns to.
    replay: Option<(replay::Scene, Screen)>,
    /// Held shift and ctrl/cmd (the app sets these), for the editor's shortcuts.
    pub held_mods: (bool, bool),
    /// How long each open popup has been showing (keyed by what it is), for its open
    /// animation; a popup that isn't drawn in a frame is forgotten.
    opened: std::cell::RefCell<HashMap<String, (f32, bool)>>,
    /// How far into its open animation a popup starts (screenshots: `CADENCE_UI_TIME`, so a
    /// popup first drawn in the shot is shown open, not at its first frame).
    open_start: f32,
    confirm: Option<Confirm>,
    /// The login popup (`AccountLoginLayer`), over the account page.
    login: Option<login::LoginPopup>,
    register: Option<register::RegisterPopup>,
    /// Your account's pages (the menu's profile button), and the box for a post or message.
    social: Option<social::Social>,
    /// The chests' state while the rewards popup is open (`RewardsPage`), when it arrived,
    /// and the request in flight.
    pub chests: Option<(crate::gjapi::Rewards, web_time::Instant)>,
    chest_job: Option<std::sync::mpsc::Receiver<(i32, Result<crate::gjapi::Rewards, String>)>>,
    writer: Option<social::Writer>,
    /// Your level's Share popup, and the editor slot a play from its page is verifying.
    share: Option<share::Share>,
    /// The editor's song playing (its music button), and which file.
    editor_music: bool,
    /// Seconds since the editor last auto-saved (`autosave` option).
    autosave_t: f32,
    pub editor_song: Option<String>,
    verify_slot: Option<usize>,
    /// Cloud Save / Load in progress (the Account page shows its loading circle meanwhile).
    cloud: Option<std::sync::mpsc::Receiver<Result<CloudDone, i32>>>,
    /// When Save and Load were last pressed (`bak_account` / `sync_account`: 30 s apart).
    cloud_last: [Option<web_time::Instant>; 2],
    cloud_save: bool,
    /// A color picker being dragged.
    picker_drag: bool,
    /// Which vault line comes next (−1: start at a random one)
    vault_line: i32,
    /// The Vault (`SecretLayer`): the code typed so far, the guard's message (text, color) and
    /// how far into the numbers sequence the player is (+0x1b8).
    vault_input: String,
    /// The Edit Group popup's Add Group ID field.
    group_add: i32,
    /// The Edit Object (color) popup edits the detail channel (key 22) instead of the base (21).
    color_detail: bool,
    /// A trigger popup's focused input: (key, text typed).
    trig_focus: Option<(u16, String)>,
    vault_msg: Option<(String, [u8; 3])>,
    vault_progress: i32,
    /// The Map's promo popup
    map_promo: bool,
    /// Mouse position in points, and the window's size in pixels.
    mouse: (f32, f32),
    window_px: (f32, f32),
    /// The view in points (width follows the window).
    view: (f32, f32),
    buttons: Vec<HitArea>,
    pressed: Option<Btn>,
    press: HashMap<Btn, Press>,
    /// Page dragging in the level select: where the drag started (x in points) and the scroll then.
    drag: Option<(f32, f32)>,
    menu_music: bool,
    /// The slider being dragged, and whether the volumes changed since the app last applied them.
    slider_drag: Option<Btn>,
    volumes_dirty: bool,
    rng: u64,
    pending: Vec<Action>,
    sounds: Vec<&'static str>,
    /// Scene transitions (`CCTransitionFade`, 0.5 s through black): on in the app, off in
    /// headless runs and tests. `fade` is the running one: time, and the screen and actions it
    /// applies at its middle.
    pub transitions: bool,
    fade: Option<(f32, Option<(Screen, Vec<Action>)>)>,
    /// The online screens are separate scenes in the game (LevelSearchLayer, LevelBrowserLayer,
    /// LevelInfoLayer, ...), each entered through a CCTransitionFade: the view on screen, and
    /// the one a fade is taking it to.
    online_shown: online::View,
    fade_view: Option<online::View>,
}

impl Ui {
    pub fn new(screen: Screen, fonts: Vec<Font>, progress: Progress) -> Ui {
        let mut ui = Ui {
            screen,
            fonts,
            progress,
            menu: menu::Menu::default(),
            levels: levels::LevelSelect::default(),
            online: online::Browser::default(),
            rigs: None,
            settings: None,
            garage: garage::Garage::default(),
            online_session: None,
            pause: None,
            pause_hidden: false,
            end: None,
            session: None,
            alert: None,
            popup: None,
            editor: None,
            editor_view: (0.0, 0.0, 1.0, 1.0),
            editor_paused: false,
            editor_playtest: false,
            dialog: None,
            replay: None,
            held_mods: (false, false),
            opened: Default::default(),
            open_start: 0.0,
            confirm: None,
            login: None,
            register: None,
            social: None,
            chests: None,
            chest_job: None,
            writer: None,
            share: None,
            editor_music: false,
            autosave_t: 0.0,
            editor_song: None,
            verify_slot: None,
            cloud: None,
            cloud_last: [None; 2],
            cloud_save: false,
            picker_drag: false,
            vault_line: -1,
            vault_input: String::new(),
            group_add: 0,
            trig_focus: None,
            color_detail: false,
            vault_msg: None,
            vault_progress: 0,
            map_promo: false,
            mouse: (-1.0, -1.0),
            window_px: (1.0, 1.0),
            view: (320.0 * 16.0 / 9.0, 320.0),
            buttons: Vec::new(),
            pressed: None,
            press: HashMap::new(),
            drag: None,
            menu_music: false,
            slider_drag: None,
            volumes_dirty: true,
            rng: 0x9e37_79b9_7f4a_7c15,
            pending: Vec::new(),
            sounds: Vec::new(),
            transitions: false,
            fade: None,
            online_shown: online::View::Search,
            fade_view: None,
        };
        ui.menu.reset_player(&mut ui.rng);
        ui
    }

    /// `rand() / RAND_MAX`.
    fn random(rng: &mut u64) -> f32 {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 7;
        *rng ^= *rng << 17;
        (*rng >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn in_level(&self) -> bool {
        matches!(self.screen, Screen::Level | Screen::Editor)
    }

    fn editor_key(&mut self, key: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode as K;
        if self.alert.is_some() {
            if matches!(key, K::Escape | K::Enter | K::Space) {
                self.alert = None;
            }
            return;
        }
        let (shift, ctrl) = self.held_mods;
        let Some(e) = self.editor.as_mut() else { return };
        // the desktop shortcuts, as buttons (EDIT_BUTTONS: 0-3 move a block, 4-7 two units,
        // 10/11 rotate); ctrl/cmd combinations first
        let edit = |k: u8| Btn::EdEdit(k);
        let popup = self.replay.is_some() || self.editor_paused || self.trig_focus.is_some();
        let shortcut = match key {
            _ if popup => None,
            K::KeyZ if ctrl && shift => Some(Btn::EdRedo),
            K::KeyZ if ctrl => Some(Btn::EdUndo),
            K::KeyY if ctrl => Some(Btn::EdRedo),
            K::KeyD if ctrl => Some(Btn::EdDuplicate),
            K::KeyC if ctrl => Some(Btn::EdCopy),
            K::KeyV if ctrl => Some(Btn::EdPaste),
            _ if ctrl => None,
            K::KeyW => Some(edit(if shift { 4 } else { 0 })),
            K::KeyS => Some(edit(if shift { 5 } else { 1 })),
            K::KeyA => Some(edit(if shift { 6 } else { 2 })),
            K::KeyD => Some(edit(if shift { 7 } else { 3 })),
            K::KeyQ => Some(edit(11)),
            K::KeyE => Some(edit(10)),
            K::KeyR => Some(Btn::EdRotateToggle),
            K::KeyF => Some(Btn::EdFreeMove),
            K::KeyT => Some(Btn::EdSwipe),
            K::KeyG => Some(Btn::EdSnap),
            _ => None,
        };
        if let Some(b) = shortcut {
            let _ = e.click(b);
            return;
        }
        match key {
            K::Escape => self.editor_paused = !self.editor_paused,
            K::ArrowLeft => e.cam.0 -= 30.0,
            K::ArrowRight => e.cam.0 += 30.0,
            K::ArrowUp => e.cam.1 += 30.0,
            K::ArrowDown => e.cam.1 -= 30.0,
            K::Digit1 => e.mode = editor::Mode::Build,
            K::Digit2 => e.mode = editor::Mode::Edit,
            K::Digit3 => e.mode = editor::Mode::Delete,
            K::Digit4 => e.mode = editor::Mode::View,
            K::Delete | K::Backspace => {
                let _ = e.click(Btn::EdTrash);
            }
            _ => {}
        }
    }

    /// The editor's level string when it changed (the app rebuilds the level it draws).
    pub fn take_editor_level(&mut self) -> Option<String> {
        let e = self.editor.as_mut()?;
        std::mem::take(&mut e.dirty).then(|| e.level_string())
    }

    /// The editor's level changed since it was last built.
    pub fn editor_dirty(&self) -> bool {
        self.screen == Screen::Editor && self.editor.as_ref().is_some_and(|e| e.dirty)
    }

    /// The editor's camera (x, y of the view's bottom left, zoom).
    /// The editor layer shown, when the editor is up (−1: all).
    pub fn editor_layer(&self) -> Option<i32> {
        self.editor.as_ref().filter(|_| self.screen == Screen::Editor).map(|e| e.layer)
    }

    /// The editor's camera, kept inside EditorUI::constrainGameLayerPosition's box: 100 blocks
    /// left of and below the start, up to 80 blocks over the ground.
    pub fn editor_camera(&mut self, view_w: f32) -> Option<(f32, f32, f32)> {
        let e = self.editor.as_mut().filter(|_| self.screen == Screen::Editor)?;
        let (vw, vh) = (view_w / e.zoom, cadence_core::camera::VIEW_HEIGHT / e.zoom);
        e.cam.0 = e.cam.0.min(240_030.0 - vw).max(-100.0 * 30.0 - 30.0);
        e.cam.1 = e.cam.1.min(80.0 * 30.0 + 120.0 - vh).max(-100.0 * 30.0 + 90.0);
        Some((e.cam.0, e.cam.1, e.zoom))
    }

    /// Open the editor on a local level (or a new one).
    pub fn open_editor(&mut self, e: editor::Editor) {
        self.editor = Some(e);
        self.editor_paused = false;
        self.screen = Screen::Editor;
        self.pending.push(Action::EnterEditor);
    }

    /// Save the editor's level in the local list (Cadence's own save: base64 level strings).
    fn save_editor(&mut self) {
        let Some(e) = self.editor.as_mut() else { return };
        let slot = e.slot.unwrap_or_else(|| (0..1000).find(|i| !self.progress.settings.contains_key(&format!("mylevel{i}"))).unwrap_or(999));
        e.slot = Some(slot);
        let data = cadence_core::codec::base64_encode_url(e.level_string().as_bytes());
        // a changed level has to be verified again
        if self.progress.settings.get(&format!("mylevel{slot}")) != Some(&data) {
            self.progress.settings.remove(&format!("mylevelverified{slot}"));
        }
        self.progress.settings.insert(format!("mylevel{slot}"), data);
        let name = if e.name.trim().is_empty() { format!("Unnamed {slot}") } else { e.name.clone() };
        self.progress.settings.insert(format!("mylevelname{slot}"), name.replace(' ', "\u{a0}"));
        self.progress.save();
    }

    /// The saved local levels: (slot, name), in the folder shown and matching the find text.
    pub fn my_levels(&self) -> Vec<(usize, String)> {
        let folder = self.online.my_folder;
        let find = self.online.my_find.clone().unwrap_or_default().to_lowercase();
        let mut v: Vec<(usize, String)> = self
            .progress
            .settings
            .iter()
            .filter_map(|(k, _)| k.strip_prefix("mylevel").and_then(|n| n.parse::<usize>().ok()))
            .map(|i| (i, self.progress.settings.get(&format!("mylevelname{i}")).cloned().unwrap_or_default().replace('\u{a0}', " ")))
            .filter(|(i, name)| {
                let f = self.progress.settings.get(&format!("mylevelfolder{i}")).and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
                (folder == 0 || f == folder) && name.to_lowercase().contains(&find)
            })
            .collect();
        v.sort();
        v
    }

    /// The slots of the verified local levels.
    fn my_verified(&self) -> Vec<usize> {
        self.progress.settings.keys().filter_map(|k| k.strip_prefix("mylevelverified").and_then(|n| n.parse().ok())).collect()
    }

    /// A level file (`.gmd`: the game's exported level, a plist with k2 name and k4 data)
    /// dropped on the window: into your levels.
    pub fn import_level(&mut self, bytes: &[u8]) -> bool {
        let text = String::from_utf8_lossy(bytes);
        let Ok(dict) = cadence_core::save::parse_plist(&text) else { return false };
        let get = |k: &str| cadence_core::save::get(&dict, k).and_then(|v| v.as_str()).map(str::to_string);
        let (Some(name), Some(data)) = (get("k2"), get("k4")) else { return false };
        let Ok(level) = cadence_core::codec::decode_level_string(&data) else { return false };
        let slot = (0..1000).find(|i| !self.progress.settings.contains_key(&format!("mylevel{i}"))).unwrap_or(999);
        self.progress.settings.insert(format!("mylevel{slot}"), cadence_core::codec::base64_encode_url(level.as_bytes()));
        self.progress.settings.insert(format!("mylevelname{slot}"), name.replace(' ', "\u{a0}"));
        self.progress.save();
        self.online.my_levels = self.my_levels();
        self.alert = Some(Alert { title: "Imported".into(), text: format!("<cg>{name}</c> is now in your levels."), t: 0.0 });
        true
    }

    /// A saved local level's editor state.
    pub fn load_my_level(&self, slot: usize) -> Option<editor::Editor> {
        let data = self.progress.settings.get(&format!("mylevel{slot}"))?;
        let text = String::from_utf8(cadence_core::codec::base64_decode(data).ok()?).ok()?;
        let name = self.progress.settings.get(&format!("mylevelname{slot}")).cloned().unwrap_or_default().replace('\u{a0}', " ");
        let mut e = editor::Editor::from_level_string(&name, &text);
        e.slot = Some(slot);
        Some(e)
    }

    /// Whether the level should run this frame (not while the pause menu is open).
    pub fn level_running(&self) -> bool {
        self.pause.is_none()
    }

    /// Whether the completed level must wait (the end screen is up): the app keeps the level's
    /// end animation from restarting it.
    pub fn hold_end(&self) -> bool {
        self.end.is_some() || self.session.as_ref().is_some_and(|s| s.completed)
    }

    /// The window's size in pixels (the view is 320 points tall).
    pub fn set_window(&mut self, w: f32, h: f32) {
        self.window_px = (w.max(1.0), h.max(1.0));
        self.view = (320.0 * self.window_px.0 / self.window_px.1, 320.0);
    }

    pub fn mouse_moved(&mut self, x_px: f32, y_px: f32) {
        let s = 320.0 / self.window_px.1;
        self.mouse = (x_px * s, 320.0 - y_px * s);
        if let Some(id) = self.pressed {
            let inside = self.hit() == Some(id);
            let p = self.press.entry(id).or_default();
            // CCMenu: moving off the item unselects it, moving back selects it again.
            if inside && p.factor() < 1.0001 {
                p.select(press_mult(id));
            } else if !inside && p.factor() > 1.0001 {
                p.unselect();
            }
        }
        if let Some(id) = self.slider_drag {
            self.drag_slider(id);
        }
        if self.picker_drag {
            self.drag_picker();
        }
        if let (Some((x0, s0)), Screen::Levels) = (self.drag, self.screen) {
            self.levels.drag_to(s0 - (self.mouse.0 - x0) / self.view.0);
        }
        let (vw, vh) = (self.editor_view.2 / self.view.0, self.editor_view.3 / self.view.1);
        let mouse = self.mouse;
        if let Some(e) = self.editor.as_mut().filter(|_| self.screen == Screen::Editor)
            && let Some((m0, c0)) = e.drag
        {
            e.cam = (c0.0 - (mouse.0 - m0.0) * vw, c0.1 - (mouse.1 - m0.1) * vh);
        }
        let (vx, vy, vw2, vh2) = self.editor_view;
        let (wx, wy) = (vx + mouse.0 / self.view.0 * vw2, vy + mouse.1 / self.view.1 * vh2);
        if let Some(e) = self.editor.as_mut().filter(|_| self.screen == Screen::Editor)
            && e.swipe_at.is_some()
        {
            e.swipe_move(wx, wy);
        }
    }

    /// The player's icons and colors.
    pub fn icons(&self) -> garage::IconSet {
        garage::IconSet::from_progress(&self.progress)
    }

    /// Icon sheets the menus need loaded before they draw.
    pub fn wanted_sheets(&self) -> Vec<String> {
        if let Some(s) = &self.social {
            return social::wanted_sheets(s);
        }
        match self.screen {
            _ if self.replay.is_some() => self.replay.map(|(sc, _)| replay::icon_sheets(sc)).unwrap_or_default(),
            Screen::Garage => self.garage.wanted_sheets(&self.icons()),
            Screen::Online if self.online.popup == Some(replay::Scene::Profile) => {
                self.online.profile.as_ref().map(|p| (0..9).map(|k| garage::icon_sheet(k, p.icons[k])).collect()).unwrap_or_default()
            }
            Screen::Online if matches!(self.online.view, online::View::Scores(_)) => {
                let mut v: Vec<String> = self.online.scores.iter().take(100).map(|s| garage::icon_sheet(0, s.icon.clamp(1, 485))).collect();
                v.sort();
                v.dedup();
                v
            }
            _ => match self.settings {
                Some(options::Settings::Achievements { page, .. }) => achievements::wanted_sheets(page),
                _ => Vec::new(),
            },
        }
    }

    /// Music and sound effect volumes, 0..1.
    pub fn volumes(&self) -> (f32, f32) {
        (self.progress.number("music", 1.0), self.progress.number("sfx", 1.0))
    }

    /// The volumes, when they changed since the last call (the app applies them).
    pub fn take_volumes(&mut self) -> Option<(f32, f32)> {
        std::mem::take(&mut self.volumes_dirty).then(|| self.volumes())
    }

    /// Move the dragged slider's thumb to the mouse.
    fn drag_slider(&mut self, id: Btn) {
        let Some(b) = self.buttons.iter().find(|b| b.id == id) else { return };
        let v = ((self.mouse.0 - (b.x - b.hw)) / (2.0 * b.hw)).clamp(0.0, 1.0);
        let key = match id {
            Btn::SliderMusic => "music",
            Btn::SliderSfx => "sfx",
            Btn::SliderSetting(4) => "offset",
            _ => "scroll",
        };
        self.progress.set_number(key, v);
        self.volumes_dirty |= matches!(id, Btn::SliderMusic | Btn::SliderSfx);
    }

    /// `CCControlColourPicker` in a color popup (keys 7/8/9 the color): the hue ring (150
    /// across at (208.5, 121); its knob 60 from the center) and the saturation / brightness
    /// square (110 across at (227.5, 141); s and v over a 55 box 35 in, held to the circle).
    fn drag_picker(&mut self) {
        let dx = (self.window_px.0 / self.window_px.1 * 320.0 - 569.0) * 0.5;
        let (mx, my) = (self.mouse.0 - dx, self.mouse.1);
        let Some(e) = self.editor.as_mut() else { return };
        let Some(o) = e.selection().next() else { return };
        let rgb = [7, 8, 9].map(|k| o.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(255.0) / 255.0);
        let (mut h, mut sat, mut val) = rgb_to_hsv(rgb);
        let (hx, hy) = (283.5f32, 196.0f32);
        let d = ((mx - hx).powi(2) + (my - hy).powi(2)).sqrt();
        if d > 59.0 {
            // the hue ring (cocos2d-x: angle + 180°)
            h = ((my - hy).atan2(mx - hx).to_degrees() + 180.0) % 360.0;
        } else {
            let (sx, sy) = (227.5f32, 141.0f32);
            let (cx, cy) = (sx + 55.0, sy + 55.0);
            let (mut px, mut py) = (mx, my);
            let dd = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            if dd > 55.0 {
                let a = (py - cy).atan2(px - cx);
                px = cx + 55.0 * a.cos();
                py = cy + 55.0 * a.sin();
            }
            px = px.clamp(sx + 35.0, sx + 89.0);
            py = py.clamp(sy + 35.0, sy + 89.0);
            sat = 1.0 - ((sx + 35.0 - px) / 55.0).abs();
            val = ((sy + 35.0 - py) / 55.0).abs();
        }
        let c = hsv_to_rgb(h, sat, val);
        let vals: Vec<(u16, String)> = [7u16, 8, 9].iter().zip(c).map(|(k, v)| (*k, ((v * 255.0).round() as i32).to_string())).collect();
        e.edit_selected(move |o| {
            for (k, v) in &vals {
                o.set(*k, Some(v.clone()));
            }
        });
    }

    fn hit(&self) -> Option<Btn> {
        let (mx, my) = self.mouse;
        // the last registered button is on top
        self.buttons.iter().rev().find(|b| (mx - b.x).abs() <= b.hw && (my - b.y).abs() <= b.hh).map(|b| b.id)
    }

    /// The left mouse button. Returns true if the UI used it (so the level doesn't jump).
    pub fn mouse_button(&mut self, down: bool) -> bool {
        if down {
            let hit = self.hit();
            if let Some(Btn::Picker) = hit {
                self.picker_drag = true;
                self.drag_picker();
                return true;
            }
            if let Some(id @ (Btn::SliderMusic | Btn::SliderSfx | Btn::SliderSetting(_))) = hit {
                self.slider_drag = Some(id);
                self.drag_slider(id);
                return true;
            }
            if let Some(id) = hit {
                if std::env::var_os("CADENCE_LOG_CLICKS").is_some() {
                    eprintln!("pressed {id:?}");
                }
                self.pressed = Some(id);
                self.press.entry(id).or_default().select(press_mult(id));
                return true;
            }
            if self.screen == Screen::Levels && self.alert.is_none() {
                self.drag = Some((self.mouse.0, self.levels.scroll()));
                return true;
            }
            if self.screen == Screen::Editor && !self.editor_paused && self.alert.is_none() {
                let (vx, vy, vw, vh) = self.editor_view;
                let (wx, wy) = (vx + self.mouse.0 / self.view.0 * vw, vy + self.mouse.1 / self.view.1 * vh);
                let mouse = self.mouse;
                if let Some(e) = self.editor.as_mut() {
                    // the bottom bar (117 high) takes touches, like EditorUI's
                    if mouse.1 < 117.0 && !e.hide_ui {
                        return true;
                    }
                    if e.mode == editor::Mode::View {
                        e.drag = Some((mouse, e.cam));
                    } else if e.swipe {
                        e.swipe_start(wx, wy);
                    } else {
                        e.click_world(wx, wy);
                    }
                }
                return true;
            }
            // In a level, a click anywhere while a menu is up belongs to the menu.
            return self.screen != Screen::Level || self.pause.is_some() || self.end.is_some();
        }
        if std::mem::take(&mut self.picker_drag) {
            return true;
        }
        if self.slider_drag.take().is_some() {
            self.progress.save();
            return true;
        }
        if let Some((x0, _)) = self.drag.take() {
            let moved = (self.mouse.0 - x0) / self.view.0;
            self.levels.release_drag(moved);
            return true;
        }
        if let Some(e) = self.editor.as_mut().filter(|_| self.screen == Screen::Editor) {
            if e.drag.take().is_some() {
                return true;
            }
            if e.swipe_at.is_some() {
                e.swipe_end();
                return true;
            }
        }
        let Some(id) = self.pressed.take() else {
            return self.screen != Screen::Level || self.pause.is_some() || self.end.is_some();
        };
        if self.hit() == Some(id) {
            self.press.entry(id).or_default().activate();
            self.clicked(id);
        } else {
            self.press.entry(id).or_default().unselect();
        }
        true
    }

    /// Keys the UI handles. Returns true if it used the key.
    pub fn key(&mut self, key: winit::keyboard::KeyCode, down: bool) -> bool {
        use winit::keyboard::KeyCode as K;
        if !down {
            return false;
        }
        if self.alert.is_some() {
            if matches!(key, K::Escape | K::Enter | K::Space) {
                self.alert = None;
                return true;
            }
            return false;
        }
        if let Some(w) = self.writer.as_mut() {
            match key {
                K::Backspace => w.backspace(),
                K::Tab if w.to.is_some() => w.field ^= 1,
                K::Escape => self.writer = None,
                _ => {}
            }
            return true;
        }
        if let Some(s) = self.social.as_mut().filter(|_| self.alert.is_none() && self.confirm.is_none()) {
            if key == K::Escape {
                s.stack.pop();
                if s.stack.is_empty() {
                    self.social = None;
                } else {
                    s.load();
                }
            }
            return true;
        }
        if let Some(r) = self.register.as_mut() {
            match key {
                K::Backspace => r.backspace(),
                K::Tab => {
                    r.focus = (r.focus + 1) % 5;
                    r.active = true;
                }
                K::Enter | K::NumpadEnter => r.submit(),
                K::Escape if !r.loading() => self.register = None,
                _ => {}
            }
            return true;
        }
        if let Some(l) = self.login.as_mut() {
            match key {
                K::Backspace => l.backspace(),
                K::Tab => {
                    l.focus ^= 1;
                    l.active = true;
                }
                K::Enter | K::NumpadEnter => l.submit(),
                K::Escape if !l.loading() => self.login = None,
                _ => {}
            }
            return true;
        }
        if let Some(options::Settings::Achievements { .. }) = self.settings {
            match key {
                K::ArrowDown => self.scroll(-80.0),
                K::ArrowUp => self.scroll(80.0),
                K::ArrowLeft => self.clicked(Btn::AchPrev),
                K::ArrowRight => self.clicked(Btn::AchNext),
                _ => {}
            }
        }
        if let Some(options::Settings::Stats { .. }) = self.settings {
            match key {
                K::ArrowDown => self.scroll(-40.0),
                K::ArrowUp => self.scroll(40.0),
                _ => {}
            }
        }
        if matches!(self.replay, Some((replay::Scene::EditText, _))) && key == K::Backspace {
            let mut t = self.object_text();
            t.pop();
            self.set_object_text(t);
            return true;
        }
        if self.trig_focus.is_some() {
            match key {
                K::Backspace => {
                    if let Some((_, t)) = self.trig_focus.as_mut() {
                        t.pop();
                    }
                    return true;
                }
                K::Enter | K::NumpadEnter | K::Escape => {
                    self.commit_trigger_input();
                    return true;
                }
                _ => {}
            }
        }
        if let Some((replay::Scene::Secret(v @ (1 | 2 | 4)), _)) = self.replay
            && self.alert.is_none()
            && self.dialog.is_none()
        {
            match key {
                K::Backspace => {
                    self.vault_input.pop();
                    return true;
                }
                K::Enter | K::NumpadEnter => {
                    self.vault_submit(v);
                    return true;
                }
                _ => {}
            }
        }
        if matches!(self.replay, Some((replay::Scene::EditLevel, _))) && self.alert.is_none() && key == K::Backspace {
            if let Some(e) = self.editor.as_mut() {
                e.name.pop();
            }
            return true;
        }
        if self.replay.is_some() && self.alert.is_none() && self.dialog.is_none() {
            if key == K::Escape
                && let Some((_, back)) = self.replay.take()
            {
                self.screen = back;
            }
            return true;
        }
        if self.confirm.is_some() {
            if key == K::Escape {
                self.confirm = None;
            } else if key == K::Enter {
                self.clicked(Btn::ConfirmYes);
            }
            return true;
        }
        if self.dialog.is_some() || self.map_promo {
            // DialogLayer::keyBackClicked closes it; Space / Enter go to the next line
            if key == K::Escape {
                self.map_promo = false;
                self.dialog = None;
            } else if matches!(key, K::Space | K::Enter) {
                if self.map_promo {
                    self.map_promo = false;
                } else {
                    self.clicked(Btn::DialogNext);
                }
            }
            return true;
        }
        if self.screen == Screen::Menu && self.online.news_page.is_some() {
            if key == K::Escape {
                self.online.news_page = None;
            }
            return true;
        }
        if self.screen == Screen::Creator && (self.online.daily.is_some() || self.online.quests_open || self.online.paths_open) {
            if key == K::Escape {
                self.clicked(Btn::DailyClose);
            }
            return true;
        }
        if self.popup.is_some() {
            if key == K::Escape {
                self.clicked(Btn::PopClose);
            }
            return true;
        }
        if let Some(st) = self.settings {
            if key == K::Escape {
                match st {
                    options::Settings::More { .. } => self.clicked(Btn::MoreClose),
                    _ => self.clicked(Btn::DropBack),
                }
            }
            return true;
        }
        match self.screen {
            Screen::Menu => match key {
                // MenuLayer::keyBackClicked asks to quit; Space/Enter play.
                K::Escape => {
                    self.clicked(Btn::MenuClose);
                    true
                }
                K::Space | K::Enter => {
                    self.clicked(Btn::MenuPlay);
                    true
                }
                _ => false,
            },
            Screen::Garage => match key {
                K::Escape => {
                    if self.garage.view != garage::View::Icons {
                        self.garage.view = garage::View::Icons;
                    } else {
                        self.screen = Screen::Menu;
                    }
                    true
                }
                K::ArrowLeft => {
                    self.clicked(Btn::GaragePrev);
                    true
                }
                K::ArrowRight => {
                    self.clicked(Btn::GarageNext);
                    true
                }
                _ => false,
            },
            Screen::Creator => match key {
                K::Escape => {
                    self.screen = Screen::Menu;
                    true
                }
                _ => false,
            },
            Screen::Online => match key {
                // back one step: a popup, the level page, the list, the search screen
                K::Escape => {
                    if self.online.popup.take().is_none() {
                        self.clicked(Btn::LevelsBack);
                    }
                    true
                }
                K::Enter => {
                    self.clicked(Btn::OnlineSearch);
                    true
                }
                K::Backspace => {
                    self.online.query.pop();
                    true
                }
                // typed characters arrive through `text`
                _ => true,
            },
            Screen::Levels => match key {
                K::Escape => {
                    self.clicked(Btn::LevelsBack);
                    true
                }
                K::ArrowLeft | K::KeyA => {
                    self.clicked(Btn::LevelsPrev);
                    true
                }
                K::ArrowRight | K::KeyD => {
                    self.clicked(Btn::LevelsNext);
                    true
                }
                K::Space | K::Enter => {
                    self.clicked(Btn::LevelBox);
                    true
                }
                _ => false,
            },
            Screen::Editor => {
                self.editor_key(key);
                true
            }
            Screen::Level => {
                if self.end.is_some() {
                    // EndLevelLayer::keyBackClicked → menu; Space replays.
                    match key {
                        K::Escape => self.clicked(Btn::EndMenu),
                        K::Space | K::Enter => self.clicked(Btn::EndReplay),
                        _ => return true,
                    }
                    return true;
                }
                if self.pause.is_some() {
                    // PauseLayer::keyDown: Escape quits to the menu, Space resumes.
                    match key {
                        K::Escape => self.clicked(Btn::PauseMenu),
                        K::Space => self.clicked(Btn::PauseResume),
                        K::KeyR => self.clicked(Btn::PauseRestart),
                        _ => {}
                    }
                    return true;
                }
                if key == K::Escape || key == K::KeyP {
                    self.open_pause();
                    return true;
                }
                false
            }
        }
    }

    /// The mouse wheel (or arrow keys) over a list: positive scrolls the list down the page.
    /// The mouse wheel in the editor (EditorUI::scrollWheel): it moves the view up and down,
    /// sideways with shift (or a sideways swipe), and zooms with ctrl/cmd. False elsewhere.
    pub fn editor_wheel(&mut self, dx: f32, dy: f32, shift: bool, ctrl: bool) -> bool {
        if self.screen != Screen::Editor || self.replay.is_some() {
            return false;
        }
        let Some(e) = self.editor.as_mut() else { return false };
        if ctrl {
            if dy != 0.0 {
                e.click(Btn::EdZoom(dy > 0.0));
            }
        } else if shift {
            e.cam.0 -= (dy + dx) * 2.0 / e.zoom;
        } else {
            e.cam.0 -= dx * 2.0 / e.zoom;
            e.cam.1 += dy * 2.0 / e.zoom;
        }
        true
    }

    pub fn scroll(&mut self, delta: f32) {
        if let Some(s) = self.social.as_mut() {
            s.scroll_by(-delta);
            return;
        }
        if self.screen == Screen::Editor {
            if let Some(e) = self.editor.as_mut() {
                e.cam.1 += delta * 2.0 / e.zoom;
            }
            return;
        }
        if self.settings.is_none() && self.screen == Screen::Online && self.online.view == online::View::List {
            let max = self.online.max_scroll();
            self.online.scroll = (self.online.scroll - delta).clamp(0.0, max);
            return;
        }
        let max = options::stats_max_scroll(&self.progress);
        match self.settings.as_mut() {
            Some(options::Settings::Stats { scroll, .. }) => *scroll = (*scroll - delta).clamp(0.0, max),
            Some(options::Settings::Achievements { scroll, .. }) => *scroll = (*scroll - delta).clamp(0.0, achievements::max_scroll()),
            Some(options::Settings::Sub { sub, scroll, .. }) => *scroll = (*scroll - delta).clamp(0.0, subpages::max_scroll(*sub)),
            _ => {}
        }
    }

    /// Typed text (the online search box). Returns true if the UI used it.
    pub fn text(&mut self, text: &str) -> bool {
        if self.screen == Screen::Online && self.online.my_find.is_some() && self.online.search.kind == -1 {
            if let Some(f) = self.online.my_find.as_mut() {
                f.extend(text.chars().filter(|c| !c.is_control()));
            }
            self.online.my_levels = self.my_levels();
            return true;
        }
        if let Some(w) = self.writer.as_mut().filter(|_| self.alert.is_none()) {
            w.type_text(text);
            return true;
        }
        if let Some(r) = self.register.as_mut().filter(|_| self.alert.is_none()) {
            r.type_text(text);
            return true;
        }
        if let Some(l) = self.login.as_mut().filter(|_| self.alert.is_none()) {
            l.type_text(text);
            return true;
        }
        if matches!(self.replay, Some((replay::Scene::EditLevel, _))) && self.alert.is_none() {
            if let Some(e) = self.editor.as_mut() {
                for ch in text.chars() {
                    if !ch.is_control() && e.name.chars().count() < 20 {
                        e.name.push(ch);
                    }
                }
            }
            return true;
        }
        if matches!(self.replay, Some((replay::Scene::EditText, _))) {
            let mut t = self.object_text();
            for ch in text.chars() {
                if !ch.is_control() && t.chars().count() < 200 {
                    t.push(ch);
                }
            }
            self.set_object_text(t);
            return true;
        }
        if let Some((_, t)) = self.trig_focus.as_mut() {
            for ch in text.chars() {
                // CCTextInputNode with the popup's allowed characters (numbers)
                if (ch.is_ascii_digit() || ch == '.' || ch == '-') && t.len() < 10 {
                    t.push(ch);
                }
            }
            return true;
        }
        if matches!(self.replay, Some((replay::Scene::Secret(1 | 2 | 4), _))) && self.alert.is_none() && self.dialog.is_none() {
            for ch in text.chars() {
                // CCTextInputNode: max 20 characters
                if !ch.is_control() && self.vault_input.chars().count() < 20 {
                    self.vault_input.push(ch);
                }
            }
            return true;
        }
        if self.screen != Screen::Online || self.alert.is_some() {
            return false;
        }
        for ch in text.chars() {
            if !ch.is_control() && self.online.query.chars().count() < 30 {
                self.online.query.push(ch);
            }
        }
        true
    }

    /// The online level that finished downloading, to load and play.
    pub fn take_online(&mut self) -> Option<online::OnlineLevel> {
        let l = self.online.ready.take()?;
        self.online_session = Some(l.info.clone());
        Some(l)
    }

    /// The name shown in the pause menu.
    pub fn level_name(&self, id: i32) -> String {
        match &self.online_session {
            Some(l) if l.id == id => l.name.clone(),
            _ => level_info(id).name.to_string(),
        }
    }

    /// The window lost focus: a level pauses (the game pauses when the app goes to the background).
    pub fn focus_lost(&mut self) {
        if self.screen == Screen::Level && self.pause.is_none() && self.end.is_none() && !self.hold_end() {
            self.open_pause();
        }
    }

    fn open_pause(&mut self) {
        if self.pause.is_none() && !self.hold_end() {
            self.pause_hidden = false;
            self.pause = Some(pause::Pause::default());
            self.pending.push(Action::Pause);
        }
    }

    /// A jump input reached the level (for the end screen's jump count).
    pub fn jumped(&mut self) {
        if let Some(s) = self.session.as_mut()
            && !s.last.dead
            && s.last.end_time.is_none()
        {
            s.jumps += 1;
            self.progress.entry(s.id).jumps += 1;
        }
    }

    fn clicked(&mut self, id: Btn) {
        if std::env::var_os("CADENCE_LOG_CLICKS").is_some() { eprintln!("clicked {id:?}"); }
        if self.fade.is_some() {
            return;
        }
        let (screen, pending) = (self.screen, self.pending.len());
        self.clicked_now(id);
        // A scene change (another screen, a level started or left) waits for the fade's middle.
        let scene = |a: &Action| matches!(a, Action::Play(_) | Action::LeaveLevel | Action::PlayOnline);
        if self.transitions && (self.screen != screen || self.pending[pending..].iter().any(scene)) {
            let actions: Vec<Action> = self.pending.drain(pending..).collect();
            let target = std::mem::replace(&mut self.screen, screen);
            self.fade = Some((0.0, Some((target, actions))));
        }
    }

    fn clicked_now(&mut self, id: Btn) {
        if let (Some(sh), Btn::Replay(i)) = (self.share.as_mut(), id) {
            if let share::Effect::Close = sh.click(i) {
                self.share = None;
            }
            return;
        }
        // another player's profile opened from your lists sits over them
        if self.social.is_some() && self.online.popup == Some(replay::Scene::Profile) {
            if let Btn::Replay(i) = id {
                if let online::Effect::Alert(title, text) = self.online.popup_click(i) {
                    self.alert = Some(Alert { title, text, t: 0.0 });
                }
            }
            return;
        }
        if let Some(s) = self.social.as_mut().filter(|_| self.writer.is_none() && matches!(id, Btn::Replay(_) | Btn::Social(_) | Btn::MsgClose)) {
            let e = s.click(id);
            self.social_effect(e);
            return;
        }
        match id {
            Btn::MenuProfile => self.social = Some(social::Social::open()),
            Btn::WriteField(f) => {
                if let Some(w) = self.writer.as_mut() {
                    w.field = f;
                }
            }
            Btn::WriteCancel => self.writer = None,
            // a custom song's id for your level
            Btn::WriteSend if self.writer.as_ref().is_some_and(|w| w.song_slot.is_some()) => {
                if let Some(w) = self.writer.take() {
                    if let (Some(slot), Ok(id)) = (w.song_slot, w.body.trim().parse::<i32>()) {
                        self.progress.settings.insert(format!("mylevelsong{slot}"), id.to_string());
                        self.progress.save();
                    }
                }
            }
            // a comment on a level: its percent is your best there
            Btn::WriteSend if self.writer.as_ref().is_some_and(|w| w.level.is_some()) => {
                if let Some(w) = self.writer.take() {
                    let level = w.level.unwrap_or(0);
                    let percent = self.progress.get(level as i32).best as i32;
                    let text = w.body;
                    if !text.trim().is_empty() {
                        self.online.post_comment(level, text, percent);
                    }
                }
            }
            Btn::WriteSend => {
                if let (Some(w), Some(s)) = (self.writer.take(), self.social.as_mut()) {
                    match w.to {
                        Some(to) => s.message(to, w.subject, w.body),
                        None if !w.body.trim().is_empty() => s.post(w.body),
                        None => {}
                    }
                }
            }
            Btn::MenuPlay => {
                self.screen = Screen::Levels;
                self.levels.enter();
            }
            // MenuLayer::onQuit / onNewgrounds: FLAlertLayers asking first
            Btn::MenuClose => {
                self.confirm = Some(Confirm {
                    title: "Quit Game".into(),
                    text: "Are you sure you want to <cr>quit</c>?".into(),
                    btn1: "Cancel".into(),
                    btn2: "Yes".into(),
                    action: Action::Quit,
                    t: 0.0,
                })
            }
            Btn::ConfirmNo => self.confirm = None,
            Btn::ConfirmYes => {
                if let Some(c) = self.confirm.take() {
                    match c.action {
                        Action::CloudSave | Action::CloudLoad => self.start_cloud(c.action == Action::CloudSave),
                        Action::DeleteMyLevel(slot) => {
                            for k in ["mylevel", "mylevelname", "mylevelverified", "mylevelid", "mylevelver", "mylevelfolder", "mylevelldm"] {
                                self.progress.settings.remove(&format!("{k}{slot}"));
                            }
                            self.progress.save();
                            self.editor = None;
                            self.replay = None;
                            self.online.my_levels = self.my_levels();
                        }
                        Action::DeleteMine => {
                            for (slot, _) in std::mem::take(&mut self.online.my_levels) {
                                for k in ["mylevel", "mylevelname", "mylevelverified", "mylevelid", "mylevelver", "mylevelfolder"] {
                                    self.progress.settings.remove(&format!("{k}{slot}"));
                                }
                            }
                            self.progress.save();
                            self.online.my_selected = false;
                            self.online.my_levels = self.my_levels();
                        }
                        Action::DeleteSaved(id) => {
                            crate::saved::delete(id);
                            self.online.popup = None;
                            self.online.view = online::View::List;
                            self.online.results = crate::saved::list().into_iter().map(|s| s.info).collect();
                        }
                        Action::ToggleLdm(id) => {
                            let key = format!("ldm{id}");
                            let on = self.progress.var(&key, false);
                            self.progress.set_var(&key, !on);
                            self.progress.save();
                        }
                        Action::ReportLevel(level) => {
                            let _ = crate::net::spawn(async move { crate::net::post("reportGJLevel.php", &crate::gjapi::report_form(level as i64)).await.is_ok() });
                            self.alert = Some(Alert { title: "Reported".into(), text: "Thank you for your report.".into(), t: 0.0 });
                        }
                        Action::AcceptRequest(row) => {
                            if let Some(s) = self.social.as_mut() {
                                s.accept(row as usize);
                            }
                        }
                        a => self.pending.push(a),
                    }
                }
            }
            Btn::MenuSocial(i) => self.pending.push(Action::OpenLink(i)),
            Btn::MenuRobTop => self.pending.push(Action::OpenLink(5)),
            Btn::MenuNewgrounds => {
                self.confirm = Some(Confirm {
                    title: "Newgrounds".into(),
                    text: "Visit Newgrounds to find awesome music?".into(),
                    btn1: "Cancel".into(),
                    btn2: "Open".into(),
                    action: Action::OpenLink(6),
                    t: 0.0,
                })
            }
            Btn::MenuNews => self.online.open_news(),
            // RewardsPage: the chests' state comes from the servers (by device id, no account)
            Btn::MenuDaily => {
                self.popup = Some(popups::Popup::Rewards);
                self.chests = None;
                self.chest_job = Some(crate::net::spawn(async { (0, crate::gjapi::rewards(0).await) }));
            }
            Btn::MenuMoreGames => {
                self.alert = Some(Alert { title: "More Games".into(), text: "Cadence plays the official levels from your own copy of the game.".into(), t: 0.0 });
            }
            Btn::MenuGarage => {
                self.screen = Screen::Garage;
                self.garage.view = garage::View::Icons;
                // open on the page with the chosen icon
                self.garage.page = (self.icons().ids[self.garage.kind].saturating_sub(1)) / garage::PER_PAGE;
            }
            Btn::GarageTab(i) => {
                self.garage.kind = i as usize;
                self.garage.page = (self.icons().ids[i as usize].saturating_sub(1)) / garage::PER_PAGE;
                self.garage.view = garage::View::Icons;
            }
            Btn::GarageIcon(slot) => {
                let id = self.garage.page * garage::PER_PAGE + slot as u32 + 1;
                self.progress.set_number(&format!("icon{}", self.garage.kind), id as f32);
                self.progress.save();
            }
            Btn::GaragePrev | Btn::GarageNext => {
                let n = self.garage.pages();
                self.garage.page = if id == Btn::GaragePrev { (self.garage.page + n - 1) % n } else { (self.garage.page + 1) % n };
            }
            Btn::GarageDot(i) => self.garage.page = i as u32,
            Btn::GaragePaint => self.garage.view = garage::View::Colors(1),
            Btn::GarageColorTab(w) => self.garage.view = garage::View::Colors(w),
            Btn::GarageColorClose => self.garage.view = garage::View::Icons,
            Btn::GarageColor(i) => {
                if let garage::View::Colors(w) = self.garage.view {
                    self.progress.set_number(if w == 1 { "color1" } else { "color2" }, i as f32);
                    self.progress.save();
                }
            }
            Btn::GarageShop => self.replay = Some((replay::Scene::Shop(0), self.screen)),
            Btn::GarageShards => self.replay = Some((replay::Scene::Shards, self.screen)),
            Btn::Replay(i) if self.screen == Screen::Online && self.online.popup.is_some() => match self.online.popup_click(i) {
                online::Effect::Alert(title, text) => self.alert = Some(Alert { title, text, t: 0.0 }),
                online::Effect::Comment(level) => {
                    let mut w = social::Writer::new("Add Comment", None);
                    w.level = Some(level);
                    self.writer = Some(w);
                }
                online::Effect::Report(level) => {
                    self.confirm = Some(Confirm {
                        title: "Confirm Report".into(),
                        text: "Report this level?\n<cy>False reports may result in action against your own account.</c>".into(),
                        btn1: "Cancel".into(),
                        btn2: "Report".into(),
                        action: Action::ReportLevel(level as i32),
                        t: 0.0,
                    })
                }
                _ => {}
            },
            // the editor's pause menu (EditorPauseLayer): Resume … Exit, then its side buttons
            Btn::Replay(i) if self.editor_paused && self.replay.is_none() => {
                if i < 5 {
                    self.clicked(Btn::EdPauseItem(i));
                } else {
                    self.editor_pause_side(i);
                }
            }
            Btn::Replay(i) => {
                self.commit_trigger_input();
                self.replay_click(i)
            }
            Btn::ScoreStat(k) => self.online.score_stat = k,
            Btn::TrigInput(key) => {
                self.commit_trigger_input();
                self.trig_focus = Some((key, String::new()));
            }
            Btn::TrigToggle(key) => {
                self.commit_trigger_input();
                if let Some(e) = self.editor.as_mut() {
                    let on = e.selection().next().and_then(|o| o.get(key)).is_some_and(|v| v != "0");
                    e.edit_selected(move |o| o.set(key, (!on).then(|| "1".to_string())));
                }
            }
            Btn::MenuOptions => self.settings = Some(options::Settings::DropDown { t: 0.0, closing: false }),
            Btn::MenuAchievements => self.settings = Some(options::Settings::Achievements { t: 0.0, closing: false, scroll: 0.0, page: 0 }),
            Btn::AchPrev | Btn::AchNext => {
                if let Some(options::Settings::Achievements { page, scroll, .. }) = self.settings.as_mut() {
                    *page = if id == Btn::AchPrev { page.saturating_sub(1) } else { (*page + 1).min(achievements::pages() - 1) };
                    *scroll = 0.0;
                }
            }
            Btn::MenuStats => self.settings = Some(options::Settings::Stats { t: 0.0, closing: false, scroll: 0.0 }),
            Btn::DropBack => match self.settings {
                Some(options::Settings::DropDown { closing: false, .. }) => self.settings = Some(options::Settings::DropDown { t: 0.0, closing: true }),
                Some(options::Settings::Stats { closing: false, scroll, .. }) => self.settings = Some(options::Settings::Stats { t: 0.0, closing: true, scroll }),
                Some(options::Settings::Sub { closing: false, sub, scroll, .. }) => self.settings = Some(options::Settings::Sub { sub, t: 0.0, closing: true, scroll }),
                Some(options::Settings::Achievements { closing: false, scroll, page, .. }) => {
                    self.settings = Some(options::Settings::Achievements { t: 0.0, closing: true, scroll, page })
                }
                _ => {}
            },
            Btn::SetOptions => self.settings = Some(options::Settings::More { tab: 0, page: 0 }),
            Btn::MoreClose if self.screen == Screen::Level => self.settings = None,
            Btn::MoreClose => self.settings = Some(options::Settings::DropDown { t: 1.0, closing: false }),
            Btn::MorePrev | Btn::MoreNext => {
                // the page arrows wrap around (TabbedPopupLayer::onTabPage)
                if let Some(options::Settings::More { tab, page }) = self.settings {
                    let n = options::page_count(tab as usize);
                    let page = if id == Btn::MorePrev { (page + n - 1) % n } else { (page + 1) % n };
                    self.settings = Some(options::Settings::More { tab, page });
                }
            }
            Btn::MoreTab(t) => self.settings = Some(options::Settings::More { tab: t, page: 0 }),
            Btn::MoreToggle(t, i) => {
                if let Some((_, key, _)) = options::entry(t, i)
                    && !key.is_empty()
                {
                    let on = self.progress.var(key, options::default_var(key));
                    self.progress.set_var(key, !on);
                    self.progress.save();
                }
            }
            Btn::MoreInfo(t, i) => {
                let info = if i == 99 {
                    Some(if t == 4 {
                        ("Music Offset", "Offsets the music in milliseconds. Use if the music seems out of sync.")
                    } else {
                        ("Scroll Speed", "Changes the scroll speed of lists.")
                    })
                } else {
                    options::entry(t, i).map(|(l, _, d)| (l, d))
                };
                if let Some((label, text)) = info {
                    self.alert = Some(Alert { title: label.replace('\n', " "), text: text.to_string(), t: 0.0 });
                }
            }
            Btn::MoreStep(k, up) => {
                let (key, def, lo, hi) = if k == 0 { ("bpr", 6.0, 6.0, 12.0) } else { ("brows", 2.0, 2.0, 4.0) };
                let v = self.progress.number(key, def) + if up { 1.0 } else { -1.0 };
                self.progress.set_number(key, v.clamp(lo, hi));
                self.progress.save();
            }
            Btn::MoreClearPlat => {
                self.alert = Some(Alert { title: "Clear Plat. Saves".into(), text: "There are no saved platformer checkpoints to clear.".into(), t: 0.0 });
            }
            Btn::SetMenuMusic => {
                let off = self.progress.var("0122", false);
                self.progress.set_var("0122", !off);
                self.progress.save();
                // restart or stop the menu music
                self.menu_music = false;
                self.pending.push(Action::MenuMusic);
            }
            // OptionsLayer::onAccount / onSupport: the dropdown leaves and the page drops in
            Btn::SetAccount => self.settings = Some(options::Settings::Sub { sub: subpages::Sub::Account, t: 0.0, closing: false, scroll: 0.0 }),
            Btn::SetHelp => self.settings = Some(options::Settings::Sub { sub: subpages::Sub::Support, t: 0.0, closing: false, scroll: 0.0 }),
            // GameManager::rateGame: the store page
            Btn::SetRate => self.pending.push(Action::OpenLink(10)),
            Btn::SetHowTo => self.popup = Some(popups::Popup::Tutorial(1)),
            Btn::SetGraphics => {
                let res = popups::current_res(&self.progress);
                let quality = self.progress.number("quality", 0.0) as u8;
                self.popup = Some(popups::Popup::Video { res, quality });
            }
            Btn::SetParent => self.popup = Some(popups::Popup::Parental(0)),
            Btn::PopClose => self.popup = None,
            Btn::PopNext => {
                self.popup = match self.popup {
                    Some(popups::Popup::Tutorial(p)) if p < 5 => Some(popups::Popup::Tutorial(p + 1)),
                    _ => None,
                }
            }
            Btn::VidRes(up) | Btn::VidQuality(up) => {
                if let Some(popups::Popup::Video { res, quality }) = self.popup.as_mut() {
                    let (v, n) = if matches!(id, Btn::VidRes(_)) { (res, popups::RESOLUTIONS.len() as u8) } else { (quality, 4) };
                    *v = if up { (*v + 1) % n } else { (*v + n - 1) % n };
                }
            }
            Btn::VidFullscreen | Btn::VidBorderless | Btn::VidFix => {
                let key = match id {
                    Btn::VidFullscreen => "0025",
                    Btn::VidBorderless => "0170",
                    _ => "0154",
                };
                let default = key == "0025";
                let on = self.progress.var(key, default);
                self.progress.set_var(key, !on);
                self.progress.save();
            }
            Btn::VidAdvanced => {
                self.alert = Some(Alert {
                    title: "Advanced".into(),
                    text: "Cadence draws every frame your display shows: there is no frame rate cap or V-Sync setting to change.".into(),
                    t: 0.0,
                })
            }
            Btn::VidApply => {
                if let Some(popups::Popup::Video { res, quality }) = self.popup {
                    self.progress.set_number("resolution", res as f32 + 1.0);
                    self.progress.set_number("quality", quality as f32);
                    self.progress.save();
                    let (width, height) = popups::RESOLUTIONS[res as usize];
                    self.pending.push(Action::Video { fullscreen: !self.progress.var("0025", true), width, height });
                }
                self.popup = None;
            }
            // a chest: ready ones open (`getGJRewards` type 1 / 2), others wait
            Btn::RewardChest(k) if self.chest_job.is_none() => match &self.chests {
                None => self.alert = Some(Alert { title: "Chest".into(), text: "Fetching the chest, tap it again in a moment.".into(), t: 0.0 }),
                Some((r, at)) => {
                    let left = if k == 0 { r.small_left } else { r.big_left } - at.elapsed().as_secs() as i64;
                    if left <= 0 {
                        let kind = k as i32 + 1;
                        self.chest_job = Some(crate::net::spawn(async move { (kind, crate::gjapi::rewards(kind).await) }));
                    }
                }
            },
            Btn::RewardFree => self.alert = Some(Alert { title: "Free Chest".into(), text: "No free chests right now.".into(), t: 0.0 }),
            Btn::ParLock => {
                let on = self.progress.var("0228", false);
                self.progress.set_var("0228", !on);
                self.progress.save();
            }
            Btn::ParPage => {
                if let Some(popups::Popup::Parental(p)) = self.popup {
                    self.popup = Some(popups::Popup::Parental(1 - p.min(1)));
                }
            }
            Btn::ParToggle(i) => {
                let key = popups::parental_key(i);
                let on = self.progress.var(key, false);
                self.progress.set_var(key, !on);
                self.progress.save();
            }
            Btn::ParInfo(i) => {
                let (label, text) = popups::parental_info(i);
                self.alert = Some(Alert { title: label.replace('\n', " "), text: text.into(), t: 0.0 });
            }
            Btn::AccLogin => self.login = Some(login::LoginPopup::new()),
            // AccountLayer::onMore: the Account Help page
            Btn::AccMore => self.settings = Some(options::Settings::Sub { sub: subpages::Sub::AccountHelp, t: 0.0, closing: false, scroll: 0.0 }),
            Btn::AccRefreshLogin => {
                let mut l = login::LoginPopup::new();
                l.user = crate::gjapi::linked_account().map(|(n, _)| n).unwrap_or_default();
                l.focus = 1;
                self.login = Some(l);
            }
            Btn::AccManage => {
                self.confirm = Some(Confirm { title: "Manage".into(), text: "Open the account management page?".into(), btn1: "Cancel".into(), btn2: "Yes".into(), action: Action::OpenLink(13), t: 0.0 })
            }
            Btn::AccUnlink => {
                let name = crate::gjapi::linked_account().map(|(n, _)| n).unwrap_or_default();
                self.confirm = Some(Confirm {
                    title: "Unlink Account".into(),
                    text: format!("Are you sure you want to <cg>unlink</c> from the account <cl>{name}</c>?"),
                    btn1: "Cancel".into(),
                    btn2: "Unlink".into(),
                    action: Action::Unlink,
                    t: 0.0,
                })
            }
            Btn::AccRegister => self.register = Some(register::RegisterPopup::new()),
            Btn::RegField(i) => {
                if let Some(r) = self.register.as_mut() {
                    r.focus = i;
                    r.active = true;
                }
            }
            Btn::RegCancel => {
                if self.register.as_ref().is_some_and(|r| !r.loading()) {
                    self.register = None;
                }
            }
            Btn::RegSubmit => {
                if let Some(r) = self.register.as_mut() {
                    r.submit();
                }
            }
            // AccountLayer::onBackup / onSync: 10 stars to save, 30 s between tries, then the
            // game's question (Cadence's Save merges into the cloud data rather than replacing it)
            Btn::AccSave | Btn::AccLoad if self.cloud.is_none() => {
                let save = id == Btn::AccSave;
                let wait = self.cloud_last[!save as usize].map_or(0, |t| 30u64.saturating_sub(t.elapsed().as_secs()));
                if save && total_stars(&self.progress) < 10 {
                    self.alert = Some(Alert { title: "Collect more".into(), text: "You need to <cl>collect</c> atleast <cy>10 stars</c> to enable <cg>saving</c>.".into(), t: 0.0 });
                } else if wait > 0 {
                    let what = if save { "back up" } else { "sync" };
                    self.alert = Some(Alert { title: if save { "Too Fast" } else { "Too fast" }.into(), text: format!("Please wait {wait} seconds before attempting to {what} your account again."), t: 0.0 });
                } else if save {
                    self.confirm = Some(Confirm {
                        title: "Save Data".into(),
                        text: "Do you want to <cg>save</c> your account data on the <cl>cloud</c>?\n<cy>This will overwrite previously saved data.</c>".into(),
                        btn1: "Cancel".into(),
                        btn2: "Save".into(),
                        action: Action::CloudSave,
                        t: 0.0,
                    });
                } else {
                    self.confirm = Some(Confirm {
                        title: "Load Data".into(),
                        text: "Do you want to <cg>download</c> your account data from the <cl>cloud</c>?\n<cy>This will merge current data with the saved data.</c>".into(),
                        btn1: "Cancel".into(),
                        btn2: "Load".into(),
                        action: Action::CloudLoad,
                        t: 0.0,
                    });
                }
            }
            Btn::LoginUser | Btn::LoginPass => {
                if let Some(l) = self.login.as_mut() {
                    l.focus = (id == Btn::LoginPass) as u8;
                    l.active = true;
                }
            }
            Btn::LoginForgotUser => self.pending.push(Action::OpenLink(11)),
            Btn::LoginForgotPass => self.pending.push(Action::OpenLink(12)),
            Btn::LoginCancel => self.login = None,
            Btn::LoginSubmit => {
                if let Some(l) = self.login.as_mut() {
                    l.submit();
                }
            }
            Btn::AccHelp => {
                self.confirm = Some(Confirm { title: "Help".into(), text: "Open the account help page?".into(), btn1: "Cancel".into(), btn2: "Yes".into(), action: Action::OpenLink(13), t: 0.0 })
            }
            Btn::SupLinks => self.pending.push(Action::OpenLink(5)),
            Btn::SupSfx => {
                self.alert = Some(Alert { title: "SFX".into(), text: "Sound effects in the game are from its own sound library.".into(), t: 0.0 })
            }
            Btn::SupPrivacy | Btn::SupTos => self.pending.push(Action::OpenLink(if id == Btn::SupPrivacy { 7 } else { 8 })),
            Btn::SupRobTop => self.pending.push(Action::OpenLink(5)),
            Btn::SupCocos => self.pending.push(Action::OpenLink(9)),
            Btn::SupLowDetail => {
                let on = self.progress.var("lowdetail", false);
                self.progress.set_var("lowdetail", !on);
                self.progress.save();
            }
            Btn::MoreSongs => self.settings = Some(options::Settings::Sub { sub: subpages::Sub::Songs, t: 0.0, closing: false, scroll: 0.0 }),
            Btn::SongView(i) => {
                let (title, artist) = subpages::SONGS[i as usize];
                self.alert = Some(Alert { title: title.into(), text: format!("By {artist}"), t: 0.0 });
            }
            // GameSettingsLayer's Keys: KeybindingsLayer over the settings
            Btn::MoreKeys => self.replay = Some((replay::Scene::Keys, self.screen)),
            // OptionsLayer::onSecretVault / CreatorLayer::onSecretVault: locked, the Keymaster
            // says one of his lines (a random one first, then in order)
            Btn::SetVault if self.progress.user_coins() >= 10 => {
                // OptionsLayer::onSecretVault: 10 user coins open the Vault
                self.vault_msg = None;
                self.vault_input.clear();
                self.replay = Some((replay::Scene::Secret(1), self.screen));
                self.settings = None;
            }
            Btn::SetVault => {
                self.vault_line = if self.vault_line < 0 { (self.t_random() * 10.0) as i32 } else { (self.vault_line + 1) % 11 };
                self.dialog = Some(Dialog { title: "The Keymaster".into(), lines: vec![VAULT_LINES[self.vault_line as usize].into()], page: 0, icon: 2, style: 2, t: 0.0 });
            }
            Btn::MenuCreator => self.screen = Screen::Creator,
            Btn::CreatorItem(i) => {
                let (_, label, _) = creator::BUTTONS[i as usize];
                let open = |ui: &mut Ui| {
                    ui.screen = Screen::Online;
                    ui.online.t = 0.0;
                };
                match label {
                    "Featured" => {
                        open(self);
                        self.online.open_list(crate::online::Search::new(6), false);
                    }
                    "Search" => {
                        open(self);
                        self.online.view = online::View::Search;
                    }
                    "Create" => {
                        open(self);
                        self.online.open_local("My Levels", "Tap <cg>New</c> to create a <cl>level</c>!");
                        self.online.my_levels = self.my_levels();
                        self.online.my_verified = self.my_verified();
                    }
                    "Saved" => {
                        open(self);
                        self.online.open_local("Saved Levels", "You have not <cg>downloaded</c> any\n<cl>levels</c> yet!");
                        // favorites first, like the game's saved list
                        let mut saved = crate::saved::list();
                        saved.sort_by_key(|s| !s.favorite);
                        self.online.results = saved.into_iter().map(|s| s.info).collect();
                        if !self.online.results.is_empty() {
                            self.online.status.clear();
                            self.online.total = self.online.results.len() as u32;
                        }
                    }
                    "Scores" => {
                        open(self);
                        self.online.open_scores(0);
                    }
                    "Gauntlets" => {
                        open(self);
                        self.online.open_gauntlets();
                    }
                    "Quests" => self.online.open_quests(),
                    "Paths" => self.online.paths_open = true,
                    "Lists" => {
                        open(self);
                        self.online.open_lists();
                    }
                    "Map Packs" => {
                        open(self);
                        self.online.open_packs();
                    }
                    "Daily" | "Weekly" | "Event" => {
                        let kind = match label {
                            "Daily" => 0,
                            "Weekly" => 1,
                            _ => 2,
                        };
                        self.online.open_daily(kind);
                    }
                    // CreatorLayer::onMultiplayer / onAdventureMap: the Keymaster and a notice
                    "Versus" => {
                        self.dialog = Some(Dialog { title: "The Keymaster".into(), lines: VERSUS_LINES.iter().map(|l| l.to_string()).collect(), page: 0, icon: 2, style: 2, t: 0.0 })
                    }
                    "The Map" => self.map_promo = true,
                    _ => {
                        self.alert = Some(Alert { title: label.into(), text: format!("{label} isn't in Cadence yet: it's being built."), t: 0.0 });
                    }
                }
            }
            // CreatorLayer::onTreasureRoom: closed until 5 keys
            Btn::CreatorDoor => {
                self.dialog = Some(Dialog {
                    title: "The Keymaster".into(),
                    lines: vec!["Bring me <cy>5</c> <cg>keys</c>,<d020>\nand I will let you pass.".into()],
                    page: 0,
                    icon: 2,
                    style: 2,
                    t: 0.0,
                });
            }
            Btn::DialogNext => {
                if let Some(d) = self.dialog.as_mut() {
                    d.page += 1;
                    d.t = 0.0;
                    if d.page >= d.lines.len() {
                        self.dialog = None;
                    }
                }
            }
            Btn::MapPromoClose => self.map_promo = false,
            Btn::DailyClose | Btn::DailySafe | Btn::DailyPlay | Btn::PathIcon(_) | Btn::NewsClose | Btn::NewsPage(_) => match self.online.click_daily(id) {
                online::Effect::Alert(title, text) => self.alert = Some(Alert { title, text, t: 0.0 }),
                online::Effect::Sound(s) => {
                    self.sounds.push(s);
                    self.online.daily = None;
                }
                _ => {}
            },
            // the online screens handle their own buttons
            Btn::LevelsBack
            | Btn::LevelsPrev
            | Btn::LevelsNext
            | Btn::OnlineSearch
            | Btn::OnlineClear
            | Btn::OnlineQuick(_)
            | Btn::OnlineDiff(_)
            | Btn::OnlineDemonFilter
            | Btn::OnlineDemonPick(_)
            | Btn::OnlineDemonOk
            | Btn::OnlineLen(_)
            | Btn::OnlineStar
            | Btn::OnlineClearFilters
            | Btn::OnlineUsers
            | Btn::OnlineFilters
            | Btn::OnlineLists
            | Btn::OnlineRefresh
            | Btn::OnlineHelp
            | Btn::OnlineView(_)
            | Btn::OnlinePack(_)
            | Btn::OnlineList(_)
            | Btn::MyTool(_)
            | Btn::GauntletCard(_)
            | Btn::ScoreTab(_)
            | Btn::GauntletPage(_)
            | Btn::InfoPlay
            | Btn::InfoSide(_)
            | Btn::FilterToggle(_)
            | Btn::FilterClose
            | Btn::RateStar(_)
            | Btn::RateCancel
            | Btn::RateSubmit
            | Btn::InfoSongMore
            | Btn::InfoSongAssets
            | Btn::InfoAuthor
                if self.screen == Screen::Online =>
            {
                match self.online.click(id) {
                    online::Effect::Leave => self.screen = Screen::Creator,
                    online::Effect::Alert(title, text) => self.alert = Some(Alert { title, text, t: 0.0 }),
                    online::Effect::Sound(s) => self.sounds.push(s),
                    online::Effect::Delete(id) => {
                        self.confirm = Some(Confirm {
                            title: "Delete Level".into(),
                            text: "Are you sure you want to <cr>delete</c> this level?".into(),
                            btn1: "Cancel".into(),
                            btn2: "Delete".into(),
                            action: Action::DeleteSaved(id),
                            t: 0.0,
                        })
                    }
                    online::Effect::Clone(id) => self.clone_saved(id),
                    online::Effect::RefreshMine => {
                        self.online.my_levels = self.my_levels();
                        self.online.my_verified = self.my_verified();
                        let folder = self.online.my_folder;
                        self.online.title = if folder == 0 { "My Levels".into() } else { format!("My Levels (Folder {folder})") };
                    }
                    online::Effect::DeleteMine => {
                        let n = self.online.my_levels.len();
                        self.confirm = Some(Confirm {
                            title: "Delete Levels".into(),
                            text: format!("Delete the <cy>{n}</c> level{} shown? <cr>You cannot undo this.</c>", if n == 1 { "" } else { "s" }),
                            btn1: "Cancel".into(),
                            btn2: "Delete".into(),
                            action: Action::DeleteMine,
                            t: 0.0,
                        })
                    }
                    online::Effect::LevelOptions(id) => {
                        let on = self.progress.var(&format!("ldm{id}"), false);
                        self.confirm = Some(Confirm {
                            title: "Options".into(),
                            text: format!("<cg>Low Detail Mode</c> for this level is <cy>{}</c>.", if on { "on" } else { "off" }),
                            btn1: "Close".into(),
                            btn2: if on { "Turn Off" } else { "Turn On" }.into(),
                            action: Action::ToggleLdm(id),
                            t: 0.0,
                        })
                    }
                    online::Effect::None | online::Effect::Comment(_) | online::Effect::Report(_) => {}
                }
            }
            Btn::LevelsBack => self.screen = Screen::Menu,
            Btn::LevelsPrev => self.levels.move_by(-1),
            Btn::LevelsNext => self.levels.move_by(1),
            Btn::LevelBox => {
                let page = self.levels.page();
                match levels::PAGES.get(page) {
                    Some(&levels::Page::Level(lid)) => {
                        let info = level_info(lid);
                        let have = self.progress.secret_coins();
                        if have < info.required_coins {
                            // LevelPage (0x399eb0): FLAlertLayer "Locked".
                            self.alert = Some(Alert {
                                title: "Locked".into(),
                                text: format!("Collect {} more Secret Coins to unlock this level!", info.required_coins - have),
                                t: 0.0,
                            });
                        } else {
                            self.sounds.push(PLAY_SOUND);
                            self.pending.push(Action::Play(lid));
                        }
                    }
                    Some(levels::Page::Tower) => self.clicked(Btn::TowerDoor),
                    _ => {}
                }
            }
            // LevelPage::onTheTower opens the Tower's inner layer; Cadence starts The Tower
            // (5001) directly for now.
            // LevelPage::onTheTower: the Tower's inside (LevelAreaInnerLayer), four doors
            Btn::TowerDoor => self.replay = Some((replay::Scene::TowerInner, self.screen)),
            // LevelPage::onSecretDoor: the first click reveals the door; after that the game opens
            // SecretLayer4 (once unlocked). Cadence plays The Challenge (3001) from it.
            Btn::SecretDoor => {
                if self.progress.flag("door") {
                    self.sounds.push(PLAY_SOUND);
                    self.pending.push(Action::Play(3001));
                } else {
                    self.progress.set_flag("door");
                    self.progress.save();
                    self.levels.door_revealed = Some(0.0);
                }
            }
            // PauseLayer::onPracticeMode / onNormalMode
            Btn::PausePractice => {
                let on = !self.session.as_ref().is_some_and(|s| s.last.practice);
                self.pause = None;
                if let Some(s) = self.session.as_mut() {
                    s.last.practice = on;
                }
                self.pending.push(Action::Practice(on));
            }
            Btn::PauseResume => {
                self.pause = None;
                self.pending.push(Action::Resume);
            }
            Btn::PauseRestart => {
                self.pause = None;
                self.pending.push(Action::Restart);
            }
            Btn::PauseMenu | Btn::EndMenu => {
                self.pause = None;
                self.end = None;
                self.leave_level();
            }
            Btn::PauseButton => self.open_pause(),
            Btn::PauseHide | Btn::EndHide => self.pause_hidden = !self.pause_hidden,
            // PauseLayer's side buttons (tags): 0 options, 1 hide, 2 death effect, 3 markers, 5 quests
            Btn::PauseSide(tag) => match tag {
                0 => self.settings = Some(options::Settings::More { tab: 0, page: 0 }),
                1 => self.pause_hidden = true,
                2 => {
                    let on = self.progress.var("0100", false);
                    self.progress.set_var("0100", !on);
                    self.progress.save();
                    let text = if on { "Death effects are off in practice." } else { "Death effects are on in practice." };
                    self.alert = Some(Alert { title: "Death Effect".into(), text: text.into(), t: 0.0 });
                }
                3 => {
                    let on = self.progress.var("0179", false);
                    self.progress.set_var("0179", !on);
                    self.progress.save();
                    let text = if on { "Hitboxes on death: off." } else { "Hitboxes on death: on." };
                    self.alert = Some(Alert { title: "Markers".into(), text: text.into(), t: 0.0 });
                }
                _ => self.alert = Some(Alert { title: "Quests".into(), text: "Quests come with Cadence's online part.".into(), t: 0.0 }),
            },
            Btn::EndReplay => {
                self.end = None;
                if let Some(s) = self.session.as_mut() {
                    s.completed = false;
                }
                self.pending.push(Action::Restart);
            }
            // LevelBrowserLayer::onNew: a new level, saved, on its EditLevelLayer page
            Btn::MyNew => {
                let mut e = editor::Editor::default();
                e.name.clear();
                self.editor = Some(e);
                self.save_editor();
                self.replay = Some((replay::Scene::EditLevel, Screen::Online));
            }
            Btn::MyLevel(i) => {
                if let Some(&(slot, _)) = self.online.my_levels.get(i as usize)
                    && let Some(e) = self.load_my_level(slot)
                {
                    self.editor = Some(e);
                    self.replay = Some((replay::Scene::EditLevel, Screen::Online));
                }
            }
            Btn::EdPauseItem(k) => match k {
                0 => self.editor_paused = false,
                1 => {
                    self.save_editor();
                    self.editor_paused = false;
                    self.clicked_now(Btn::EdPlaytest);
                }
                // EditorPauseLayer::onSaveAndExit / onExitNoSave: back to the level's page
                2 | 4 => {
                    if k == 2 {
                        self.save_editor();
                    } else if let Some(slot) = self.editor.as_ref().and_then(|e| e.slot) {
                        // reload what was saved
                        self.editor = self.load_my_level(slot);
                    }
                    self.editor_paused = false;
                    self.screen = Screen::Online;
                    self.online.open_local("My Levels", "Tap <cg>New</c> to create a <cl>level</c>!");
                    self.online.my_levels = self.my_levels();
                    self.replay = Some((replay::Scene::EditLevel, Screen::Online));
                    self.pending.push(Action::LeaveEditor);
                }
                _ => {
                    self.save_editor();
                    self.alert = Some(Alert { title: "Saved".into(), text: "Level saved.".into(), t: 0.0 });
                }
            },
            id if self.screen == Screen::Editor && matches!(id, Btn::EdMode(_) | Btn::EdTab(_) | Btn::EdPage(_) | Btn::EdObject(_) | Btn::EdEdit(_) | Btn::EdUndo | Btn::EdRedo | Btn::EdTrash | Btn::EdDeselect | Btn::EdHide | Btn::EdZoom(_) | Btn::EdSwipe | Btn::EdRotateToggle | Btn::EdFreeMove | Btn::EdSnap | Btn::EdCopy | Btn::EdPaste | Btn::EdDuplicate | Btn::EdPlaytest | Btn::EdMusic | Btn::EdPause | Btn::EdSide(_)) => {
                let Some(e) = self.editor.as_mut() else { return };
                match e.click(id) {
                    editor::EdEffect::Playtest => {
                        let data = cadence_core::codec::base64_encode_url(e.level_string().as_bytes()).into_bytes();
                        let info = crate::online::LevelInfo { id: -100, name: e.name.clone(), author: "You".into(), ..Default::default() };
                        self.editor_playtest = true;
                        self.online.ready = Some(online::OnlineLevel { info, data, song: online::official_song(0) });
                        self.pending.push(Action::PlayOnline);
                    }
                    editor::EdEffect::Pause => self.editor_paused = true,
                    // EditorUI::onPlayback: the level's song from where the camera is (normal
                    // speed, 311.58 units a second), or stop it
                    editor::EdEffect::Music => {
                        self.editor_music = !self.editor_music;
                        let song = online::official_song(e.setting("kA1").and_then(|v| v.parse().ok()).unwrap_or(0));
                        let offset = (e.cam.0 / 311.58).max(0.0);
                        self.pending.push(Action::EditorMusic(self.editor_music.then_some(offset)));
                        self.editor_song = song;
                    }
                    editor::EdEffect::EditObject(id) if replay::TRIGGER_POPUPS.contains(&id) => {
                        self.trig_focus = None;
                        self.replay = Some((replay::Scene::Trigger(id), Screen::Editor));
                    }
                    editor::EdEffect::EditObject(id) if id != 0 && !cadence_core::objects::info(id as i32).is_some_and(|i| i.kind == cadence_core::objects::ObjectType(20)) => {
                        self.color_detail = false;
                        self.replay = Some((replay::Scene::EditColor, Screen::Editor));
                    }
                    editor::EdEffect::EditObject(_) => {
                        self.alert = Some(Alert { title: "Not yet".into(), text: "This object's settings aren't in Cadence yet.".into(), t: 0.0 });
                    }
                    editor::EdEffect::EditSpecial(id) if replay::SPECIAL_POPUPS.contains(&id) => {
                        self.trig_focus = None;
                        self.replay = Some((replay::Scene::Special(id), Screen::Editor));
                    }
                    editor::EdEffect::EditSpecial(_) => {
                        self.alert = Some(Alert { title: "Not yet".into(), text: "This object's special settings aren't in Cadence yet.".into(), t: 0.0 });
                    }
                    editor::EdEffect::Settings => {
                        self.replay = Some((replay::Scene::LevelSettings, Screen::Editor));
                    }
                    editor::EdEffect::EditGroup => {
                        self.group_add = 0;
                        self.replay = Some((replay::Scene::EditGroup, Screen::Editor));
                    }
                    editor::EdEffect::Options => self.settings = Some(options::Settings::More { tab: 3, page: 0 }),
                    editor::EdEffect::None => {}
                }
            }
            Btn::AlertOk => self.alert = None,
            _ => {}
        }
    }

    /// A button on a replayed screen.
    fn replay_click(&mut self, i: u8) {
        let Some((scene, back)) = self.replay else { return };
        let leave = |ui: &mut Ui| {
            ui.replay = None;
            ui.screen = back;
        };
        match scene {
            // drawn and clicked by `ui::social`, never as a replayed screen
            replay::Scene::MyProfile | replay::Scene::Friends | replay::Scene::Requests | replay::Scene::AcctSettings | replay::Scene::ShareLevel | replay::Scene::OldMessages | replay::Scene::Notifications => {}
            // OK (the first button) closes; the others (easing, pages, info) aren't ported
            replay::Scene::Trigger(_) | replay::Scene::Special(_) => {
                if i == 0 {
                    self.commit_trigger_input();
                    self.replay = None;
                }
            }
            replay::Scene::TowerInner => match i {
                // the doors: The Tower, The Sewers, The Cellar, The Secret Hollow; each opens
                // once the one before is beaten (LevelAreaInnerLayer::onDoor)
                0..=3 => {
                    let id = 5001 + i as i32;
                    if i == 0 || self.progress.get(id - 1).best >= 100 {
                        self.replay = None;
                        self.sounds.push(PLAY_SOUND);
                        self.pending.push(Action::Play(id));
                    } else {
                        self.alert = Some(Alert { title: "Locked".into(), text: "Complete the previous level to unlock this door.".into(), t: 0.0 });
                    }
                }
                4 => leave(self),
                5 => self.alert = Some(Alert { title: "Locked".into(), text: "This lock needs a key you don't have yet.".into(), t: 0.0 }),
                _ => {
                    self.alert = Some(Alert {
                        title: "The Tower".into(),
                        text: "Beat each floor to open the next door. The levels of the Tower are <cg>platformer</c> levels.".into(),
                        t: 0.0,
                    })
                }
            },
            replay::Scene::Shop(_) => match i {
                1 => leave(self),
                0 => self.dialog = Some(Dialog { title: "The Shopkeeper".into(), lines: vec!["Hello there!".into()], page: 0, icon: 1, style: 1, t: 0.0 }),
                _ => {
                    self.alert = Some(Alert {
                        title: "The Shop".into(),
                        text: "Every icon is already unlocked in Cadence, so there's nothing to buy.".into(),
                        t: 0.0,
                    })
                }
            },
            replay::Scene::Info | replay::Scene::Leaderboard | replay::Scene::Vote | replay::Scene::Profile | replay::Scene::SongInfo | replay::Scene::Assets => leave(self),
            replay::Scene::EditLevel => match i {
                // Edit: the editor
                0 => {
                    self.save_editor();
                    self.replay = None;
                    if let Some(mut e) = self.editor.take() {
                        e.dirty = true;
                        self.open_editor(e);
                    }
                }
                // Play: the level as it is
                1 => {
                    if let Some(e) = &self.editor {
                        let data = cadence_core::codec::base64_encode_url(e.level_string().as_bytes()).into_bytes();
                        let info = crate::online::LevelInfo { id: -100, name: e.name.clone(), author: "You".into(), ..Default::default() };
                        // EditLevelLayer::onPlay: completing it from here verifies it
                        self.verify_slot = e.slot;
                        let song = online::official_song(e.setting("kA1").and_then(|v| v.parse().ok()).unwrap_or(0));
                        self.save_editor();
                        self.replay = None;
                        self.sounds.push(PLAY_SOUND);
                        self.online.ready = Some(online::OnlineLevel { info, data, song });
                        self.pending.push(Action::PlayOnline);
                    }
                }
                // EditLevelLayer::onShare: an account, objects and a verified level first
                2 => {
                    self.save_editor();
                    let Some(e) = self.editor.as_ref() else { return };
                    let slot = e.slot.unwrap_or(0);
                    let verified = self.progress.settings.contains_key(&format!("mylevelverified{slot}"));
                    if crate::gjapi::linked_account().is_none_or(|(_, ok)| !ok) {
                        self.alert = Some(Alert { title: "Account".into(), text: "You need an <cl>account</c> to share levels.".into(), t: 0.0 });
                    } else if e.objects.is_empty() {
                        self.alert = Some(Alert { title: "Empty".into(), text: "You cannot share an empty level.".into(), t: 0.0 });
                    } else if !verified {
                        self.alert = Some(Alert { title: "Unverified".into(), text: "Complete the level in <cl>Normal</c> mode to verify that it can be beaten!".into(), t: 0.0 });
                    } else {
                        let gz = cadence_core::codec::base64_encode_url(&cadence_core::codec::gzip(e.level_string().as_bytes()));
                        let max_x = e.objects.iter().map(|o| o.x).fold(0.0f32, f32::max);
                        // levelLength from the level's run time at normal speed (311.58 units/s)
                        let secs = max_x / 311.58;
                        let length = match secs { s if s < 10.0 => 0, s if s < 30.0 => 1, s if s < 60.0 => 2, s if s < 120.0 => 3, _ => 4 };
                        let id = self.progress.settings.get(&format!("mylevelid{slot}")).and_then(|v| v.parse().ok()).unwrap_or(0);
                        let version = self.progress.settings.get(&format!("mylevelver{slot}")).and_then(|v| v.parse().ok()).unwrap_or(0) + 1;
                        self.share = Some(share::Share::new(crate::gjapi::Upload {
                            level_id: id,
                            name: if e.name.trim().is_empty() { format!("Unnamed {slot}") } else { e.name.clone() },
                            version,
                            length,
                            objects: e.objects.len() as i32,
                            coins: e.objects.iter().filter(|o| o.id == 1329).count().min(3) as i32,
                            song_id: self.progress.settings.get(&format!("mylevelsong{slot}")).and_then(|v| v.parse().ok()).unwrap_or(0),
                            audio_track: e.setting("kA1").and_then(|v| v.parse().ok()).unwrap_or(0),
                            level_string: gz,
                            ..Default::default()
                        }));
                    }
                }
                // delete (EditLevelLayer::onDelete asks first; Cadence deletes from its own save)
                3 => {
                    if let Some(slot) = self.editor.as_ref().and_then(|e| e.slot) {
                        self.progress.settings.remove(&format!("mylevel{slot}"));
                        self.progress.settings.remove(&format!("mylevelname{slot}"));
                        self.progress.save();
                    }
                    self.editor = None;
                    self.replay = None;
                    self.online.my_levels = self.my_levels();
                }
                12 => {
                    self.save_editor();
                    self.editor = None;
                    self.replay = None;
                    self.screen = back;
                    self.online.my_levels = self.my_levels();
                }
                // EditLevelLayer's other buttons
                _ => self.edit_level_tool(i),
            },
            replay::Scene::Shards => match i {
                0 => self.replay = None,
                _ => {
                    self.alert = Some(Alert {
                        title: "Shards of Power".into(),
                        text: "Collect <cy>shards</c> from orbs, chests and paths to unlock these icons. Shard rewards come from the game's chests, which Cadence can't open.".into(),
                        t: 0.0,
                    })
                }
            },
            replay::Scene::Keys => match i {
                0 => self.replay = None,
                _ => {
                    self.alert = Some(Alert {
                        title: "Keys".into(),
                        text: "Cadence uses the default bindings: <cg>Space</c>, <cg>W</c>, <cg>Up</c> or click to jump, <cg>A</c>/<cg>D</c> and the arrows to move, <cg>Escape</c> to pause, <cg>Z</c>/<cg>X</c> for checkpoints. Changing them isn't in yet.".into(),
                        t: 0.0,
                    })
                }
            },
            replay::Scene::EditGroup => self.edit_group_click(i),
            replay::Scene::EditColor => self.edit_color_click(i),
            replay::Scene::EditorPause => {}
            replay::Scene::EditText => match i {
                0 => self.replay = None,
                // the Text toggle: back to the colors
                3 => self.replay = Some((replay::Scene::EditColor, Screen::Editor)),
                // clear
                5 => self.set_object_text(String::new()),
                _ => {}
            },
            replay::Scene::LevelSettings => self.level_settings_click(i),
            // SelectArtLayer: the icons pick kA6 / kA7 (0-based), the ground's line buttons kA17,
            // OK goes back to the settings
            replay::Scene::SelectArt(kind) => {
                let (icons, ok) = [(59, 59), (22, 22), (4, 4)][kind.min(2) as usize];
                if let Some(e) = self.editor.as_mut() {
                    if (i as usize) < icons {
                        e.set_setting(["kA6", "kA7", "kA25"][kind.min(2) as usize], &i.to_string());
                    } else if kind == 1 && (23..=25).contains(&i) {
                        e.set_setting("kA17", &(i - 23).to_string());
                    }
                }
                if i == ok {
                    self.replay = Some((replay::Scene::LevelSettings, Screen::Editor));
                }
            }
            replay::Scene::Secret(v @ (1 | 2 | 4)) => match i {
                0 => leave(self),
                // the guard is the submit button (`SecretLayer::onSubmit` and the others')
                1 => self.vault_submit(v),
                _ => {}
            },
            replay::Scene::Secret(_) | replay::Scene::TreasureRoom => match i {
                0 => leave(self),
                _ => {
                    self.vault_line = (self.vault_line + 1).rem_euclid(11);
                    self.dialog = Some(Dialog { title: "The Guard".into(), lines: vec![VAULT_LINES[self.vault_line as usize].into()], page: 0, icon: 3, style: 2, t: 0.0 });
                }
            },
        }
    }

    /// The selected text objects' text (key 31, base64).
    fn object_text(&self) -> String {
        self.editor
            .as_ref()
            .and_then(|e| e.selection().next())
            .and_then(|o| o.get(31))
            .and_then(|t| cadence_core::codec::base64_decode(t).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    fn set_object_text(&mut self, text: String) {
        let enc = cadence_core::codec::base64_encode_url(text.as_bytes());
        if let Some(e) = self.editor.as_mut() {
            e.edit_selected(move |o| o.set(31, Some(enc.clone())));
        }
    }

    /// The color channel buttons of the Edit Object popup (`CustomizeObjectLayer`), by button.
    const COLOR_BUTTONS: [(u8, i32); 13] = [(4, 1005), (5, 1006), (6, 1007), (7, 0), (8, 1), (9, 2), (10, 3), (11, 4), (12, 5), (13, 6), (14, 7), (15, 8), (16, 9)];

    /// A click in the Edit Object (color) popup: the base (21) or detail (22) channel of the
    /// selected objects; Default (0) removes the key and the object uses its own.
    fn edit_color_click(&mut self, i: u8) {
        let key = if self.color_detail { 22 } else { 21 };
        let Some(e) = self.editor.as_mut() else { return };
        let channel = match i {
            0 => {
                self.replay = None;
                return;
            }
            1 => {
                // Base / Detail, for objects with a detail color
                let detail = e.selection().next().and_then(|o| cadence_core::objects::visual(o.id as i32)).is_some_and(|v| v.detail_color >= 0);
                self.color_detail = detail && !self.color_detail;
                return;
            }
            17 => 10,
            // text objects: the Text button (drawn by Cadence, 86 × 30 at (421.5, 284))
            200 => {
                self.replay = Some((replay::Scene::EditText, Screen::Editor));
                return;
            }
            20 => {
                // Next Free: the lowest channel no object uses
                let used: std::collections::HashSet<i32> = e.objects.iter().flat_map(|o| [o.get(21), o.get(22)]).flatten().filter_map(|v| v.parse().ok()).collect();
                (1..1000).find(|c| !used.contains(c)).unwrap_or(999)
            }
            _ => match Self::COLOR_BUTTONS.iter().find(|b| b.0 == i) {
                Some(&(_, c)) => c,
                None => return,
            },
        };
        e.edit_selected(move |o| o.set(key, (channel != 0).then(|| channel.to_string())));
    }

    /// A click in the level settings (`LevelSettingsLayer`): Classic / Platformer (kA22), the
    /// official song (kA1, 0–21), the start speed (kA4: 0 1x, 1 0.5x, 2 2x, 3 3x, 4 4x) and
    /// mode (kA2: cube … swing), OK.
    fn level_settings_click(&mut self, i: u8) {
        let Some(e) = self.editor.as_mut() else { return };
        let num = |e: &editor::Editor, k: &str| e.setting(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        match i {
            19 => self.replay = None,
            13 => e.set_setting("kA22", "0"),
            14 => e.set_setting("kA22", "1"),
            20 | 21 => {
                let song = (num(e, "kA1") + if i == 20 { -1 } else { 1 }).rem_euclid(22);
                e.set_setting("kA1", &song.to_string());
            }
            24 => {
                let speed = (num(e, "kA4") + 1) % 5;
                e.set_setting("kA4", &speed.to_string());
            }
            25 => {
                let mode = (num(e, "kA2") + 1) % 8;
                e.set_setting("kA2", &mode.to_string());
            }
            // the BG / ground art buttons
            15 => self.replay = Some((replay::Scene::SelectArt(0), Screen::Editor)),
            16 => self.replay = Some((replay::Scene::SelectArt(1), Screen::Editor)),
            17 => self.replay = Some((replay::Scene::SelectArt(2), Screen::Editor)),
            // the font text objects use (`kA18`: 0 the default, then gjFont01 … gjFont59)
            18 => {
                let font = (num(e, "kA18") + 1) % 60;
                e.set_setting("kA18", &font.to_string());
                let name = if font == 0 { "the default font".to_string() } else { format!("font {font}") };
                self.alert = Some(Alert { title: "Font".into(), text: format!("Text objects use <cy>{name}</c>."), t: 0.0 });
            }
            // the song: official (Normal, the arrows) or a custom song's id
            22 => {
                if let Some(slot) = e.slot {
                    self.progress.settings.remove(&format!("mylevelsong{slot}"));
                    self.progress.save();
                }
            }
            23 => {
                let mut w = social::Writer::new("Custom Song ID", None);
                w.song_slot = Some(e.slot.unwrap_or(0));
                self.writer = Some(w);
            }
            // two-player mode (`kA10`)
            26 => {
                let on = num(e, "kA10") == 0;
                e.set_setting("kA10", if on { "1" } else { "0" });
                self.alert = Some(Alert { title: "Options".into(), text: format!("<cg>2-Player mode</c> is <cy>{}</c>.", if on { "on" } else { "off" }), t: 0.0 });
            }
            // the color channels (BG, Ground, Ground 2, Line, MG, MG2)
            _ => {
                self.alert = Some(Alert { title: "Colors".into(), text: "Set the level's colors with <cg>Color</c> triggers at the start of the level: a trigger at x 0 sets the channel from the first frame.".into(), t: 0.0 })
            }
        }
    }

    /// The focused trigger input's text, written to the selected objects (empty: unchanged).
    fn commit_trigger_input(&mut self) {
        let Some((key, text)) = self.trig_focus.take() else { return };
        let Ok(v) = text.parse::<f32>() else { return };
        if let Some(e) = self.editor.as_mut() {
            let text = if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") };
            e.edit_selected(move |o| o.set(key, Some(text.clone())));
        }
    }

    /// A click in the Edit Group popup (`SetGroupIDLayer`): the editor layers (keys 20 / 61),
    /// z order (25), z layer (24) and groups (57) of the selected objects.
    fn edit_group_click(&mut self, i: u8) {
        let add = self.group_add;
        let Some(e) = self.editor.as_mut() else { return };
        let step = |key: u16, d: i32, lo: i32, hi: i32, default: i32| {
            move |o: &mut editor::EdObject| {
                let v = o.get(key).and_then(|v| v.parse::<i32>().ok()).unwrap_or(default);
                let v = (v + d).clamp(lo, hi);
                o.set(key, (v != 0).then(|| v.to_string()));
            }
        };
        match i {
            0 => self.replay = None,
            4 => e.edit_selected(step(20, -1, 0, 9999, 0)),
            5 => e.edit_selected(step(20, 1, 0, 9999, 0)),
            6 => e.edit_selected(step(61, -1, 0, 9999, 0)),
            7 => e.edit_selected(step(61, 1, 0, 9999, 0)),
            14 | 15 => {
                let d = if i == 14 { -1 } else { 1 };
                e.edit_selected(move |o| {
                    let def = cadence_core::objects::visual(o.id as i32).map_or(0, |v| v.z_order as i32);
                    let v = o.get(25).and_then(|v| v.parse::<i32>().ok()).unwrap_or(def);
                    o.set(25, Some((v + d).clamp(-100, 100).to_string()));
                });
            }
            8 => self.group_add = (add - 1).max(0),
            9 => self.group_add = (add + 1).min(9999),
            10 => self.group_add = e.next_free_group(),
            12 if add > 0 => e.edit_selected(move |o| {
                let mut g = o.groups();
                if !g.contains(&add) && g.len() < 10 {
                    g.push(add);
                    o.set_groups(&g);
                }
            }),
            16..=24 => {
                let z = editor::Z_LAYERS[i as usize - 16];
                e.edit_selected(move |o| o.set(24, Some(z.to_string())));
            }
            25 => e.edit_selected(|o| o.set(24, None)),
            // the group buttons in the list (100 + index): remove that group
            100..=120 => {
                let Some(&g) = e.selection().next().map(|o| o.groups()).and_then(|g| g.get(i as usize - 100).copied()).as_ref() else { return };
                e.edit_selected(move |o| {
                    let mut gs = o.groups();
                    gs.retain(|x| *x != g);
                    o.set_groups(&gs);
                });
            }
            _ => {}
        }
    }

    /// `SecretLayer::onSubmit`: the code (lower case, spaces removed) against the vault's codes;
    /// each one's reply shows once, with its achievement (kept as a flag). Anything else gets
    /// the guard's chatter.
    fn vault_submit(&mut self, vault: u8) {
        if vault != 1 {
            return self.vault_submit_other(vault);
        }
        let code: String = self.vault_input.to_lowercase().chars().filter(|c| *c != ' ').collect();
        self.vault_input.clear();
        let progress = std::mem::take(&mut self.vault_progress);
        let white = [255, 255, 255];
        let name = self.progress.settings.get("playername").map(|n| n.to_lowercase()).unwrap_or_default();
        let once = |ui: &mut Self, ach: &str| -> bool {
            let id = format!("geometry.ach.{ach}");
            if ui.progress.flags.contains(&id) {
                return false;
            }
            ui.progress.flags.insert(id);
            ui.progress.save();
            true
        };
        let reply: Option<&str> = match code.as_str() {
            "lenny" if once(self, "secret05") => Some("Noooooo!!!!"),
            "sparky" if once(self, "secret06") => {
                self.progress.flags.insert("coin secret06".into());
                self.progress.save();
                self.sounds.push("highscoreGet02.ogg");
                Some("My coin.. NOOOOOO!!!")
            }
            "spooky" if once(self, "secret07") => Some("WHAT!? How did you know my name!?!"),
            "blockbite" if once(self, "secret08") => Some("How do you know my secrets!?"),
            "robotop" if once(self, "secret09") => Some("So he told you?..."),
            "ahead" if once(self, "secret10") => Some("You learn quickly!"),
            "mule" if once(self, "secret13") => Some("Ah, right you are."),
            "neverending" if once(self, "secret14") => Some("You're pretty good at this"),
            "gandalfpotter" if once(self, "secret15") => Some("That was weird..."),
            c if !c.is_empty() && c == name && once(self, "secret17") => Some("He could be watching right now..."),
            "finalboss" if once(self, "secret19") => Some("Maybe Rub should add some of these"),
            _ => None,
        };
        if let Some(r) = reply {
            self.vault_msg = Some((r.into(), white));
            return;
        }
        // 8 16 30 32 46 84, one per submit (the Lost numbers doubled); the guard grows uneasy
        if !self.progress.flags.contains("geometry.ach.secret16") {
            const SEQ: [(&str, &str); 6] = [
                ("8", ""),
                ("16", "..."),
                ("30", "What do you think you're doing?"),
                ("32", "Those numbers, aren't they..."),
                ("46", "NO NO NO NO!!!"),
                ("84", "GAH!! You have doomed us all!"),
            ];
            if let Some(&(want, text)) = SEQ.get(progress as usize)
                && code == want
            {
                self.vault_progress = progress + 1;
                let color = if progress < 3 { white } else { [255, 100, 0] };
                let text = if text.is_empty() { self.vault_chatter() } else { text.to_string() };
                self.vault_msg = Some((text, color));
                if self.vault_progress > 5 {
                    self.vault_progress = 0;
                    once(self, "secret16");
                }
                return;
            }
        }
        let text = self.vault_chatter();
        self.vault_msg = Some((text, white));
    }

    /// `SecretLayer2::onSubmit` (the Vault of Secrets) and `SecretLayer4::onSubmit` (the Chamber
    /// of Time): the codes whose replies are plain strings in the binary. (Vault 2's diamond
    /// and shard count guesses and the chamber's first riddles aren't ported.)
    fn vault_submit_other(&mut self, vault: u8) {
        let code: String = self.vault_input.to_lowercase().chars().filter(|c| *c != ' ').collect();
        self.vault_input.clear();
        let mut once = |ach: &str| -> bool {
            let id = format!("geometry.ach.{ach}");
            let new = self.progress.flags.insert(id);
            if new {
                self.progress.save();
            }
            new
        };
        let reply: Option<&str> = match (vault, code.as_str()) {
            (2, "brainpower") if once("v2.secret01") => Some("O-oooooooooo AAAAE-A-A-I-A-U- JO-oooooooooooo!"),
            (2, "glubfub") if once("v2.secret03") => {
                self.progress.flags.insert("coin secretB03".into());
                self.progress.save();
                Some("NOOOO!! THIEF! THIEF!")
            }
            (2, "octocube") if once("v2.secret04") => Some("Ugh... Slippery"),
            (2, "seven") if once("v2.secret06") => Some("I should have been a doctor..."),
            (2, "gimmiethecolor") if once("v2.secret07") => Some("How many colors do you need?"),
            (2, "thechickenisonfire") if once("v2.secret08") => Some("Indeed it is..."),
            (2, "d4shg30me7ry") if once("v2.secret09") => Some("Good times"),
            (2, "thechickenisready") if once("v2.secret10") => Some("You overcooked it again!"),
            (2, "thechallenge") => Some("My level? You want to try it!?"),
            (2, "unicorn") => Some("Why do they have a horn?"),
            (2, "rubrubpowah123") => Some("Ultimate Haxxor mode enabled. Not..."),
            (2, "battop") => Some("He could be watching us right now..."),
            (2, "kappa") => Some("There is no Kappa icon..."),
            (2, "robtop") => Some("RUBRUBRUBRUBRUBRUB"),
            (4, "hunger") if once("v3.secret04") => Some("A pizza would be nice"),
            (4, "volcano" | "avolcano") if once("v3.secret05") => Some("Hot answer, well done"),
            (4, "backontrack") if once("v3.secret06") => Some("That's some skill"),
            (4, "givemehelper") if once("v3.secret07") => Some("Is that even a real thing..."),
            _ => None,
        };
        let text = match reply {
            Some(r) => r.to_string(),
            None => self.vault_chatter(),
        };
        self.vault_msg = Some((text, [255, 255, 255]));
    }

    /// The guard's chatter when no code matched (`getMessage`; the game's own lines aren't
    /// extracted yet, so the Keymaster's are borrowed).
    fn vault_chatter(&mut self) -> String {
        self.vault_line = (self.vault_line + 1).rem_euclid(11);
        VAULT_LINES[self.vault_line as usize].to_string()
    }

    /// A number in 0..1 from the clock (the vault's first line).
    fn t_random(&self) -> f32 {
        let n = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        (n % 1000) as f32 / 1000.0
    }

    fn leave_level(&mut self) {
        let id = self.session.take().map(|s| s.id);
        if std::mem::take(&mut self.editor_playtest) {
            self.online_session = None;
            self.screen = Screen::Editor;
            if let Some(e) = self.editor.as_mut() {
                e.dirty = true;
            }
            self.pending.push(Action::LeaveLevel);
            self.pending.push(Action::EnterEditor);
            return;
        }
        if self.online_session.take().is_some() {
            self.screen = Screen::Online;
            self.pending.push(Action::LeaveLevel);
            return;
        }
        self.screen = Screen::Levels;
        if let Some(id) = id {
            self.levels.show_level(id);
        }
        self.pending.push(Action::LeaveLevel);
    }

    /// The app started a level.
    pub fn entered_level(&mut self, id: i32) {
        self.screen = Screen::Level;
        self.pause = None;
        self.end = None;
        self.menu_music = false;
        if self.online_session.as_ref().is_some_and(|l| l.id != id) {
            self.online_session = None;
        }
        self.session = Some(Session { id, attempts: 1, ..Session::default() });
        self.progress.entry(id).attempts += 1;
        self.progress.save();
        self.levels.show_level(id);
    }

    /// Start a screenshot on a menu: the pause menu or the end screen (already settled).
    pub fn open_for_screenshot(&mut self, which: &str, page: Option<usize>) {
        match which {
            "pause" => self.pause = Some(pause::Pause::default()),
            "end" => {
                if let Some(s) = self.session.as_mut() {
                    s.completed = true;
                    s.first_completion = true;
                }
                self.end = Some(end::End::settled());
            }
            "garage" => {
                self.screen = Screen::Garage;
                // CADENCE_TAB: which tab (0 cube … 10 death effect), CADENCE_PAGE its page
                if let Some(t) = std::env::var("CADENCE_TAB").ok().and_then(|v| v.parse::<usize>().ok()) {
                    self.garage.kind = t.min(garage::KINDS.len() - 1);
                }
                self.garage.page = page.unwrap_or(0) as u32;
            }
            "creator" => self.screen = Screen::Creator,
            "colors" => {
                self.screen = Screen::Garage;
                self.garage.view = garage::View::Colors(1);
            }
            "achievements" => self.settings = Some(options::Settings::Achievements { t: 1.0, closing: false, scroll: 0.0, page: page.unwrap_or(0) }),
            "stats" => self.settings = Some(options::Settings::Stats { t: 1.0, closing: false, scroll: 0.0 }),
            "settings" => self.settings = Some(options::Settings::DropDown { t: 1.0, closing: false }),
            "howto" => self.popup = Some(popups::Popup::Tutorial(page.unwrap_or(1).max(1) as u8)),
            "video" => self.popup = Some(popups::Popup::Video { res: 7, quality: 0 }),
            "parental" => self.popup = Some(popups::Popup::Parental(page.unwrap_or(0) as u8)),
            // the login popup over the account page (page 1: after submitting empty fields)
            "login" => {
                self.settings = Some(options::Settings::Sub { sub: subpages::Sub::Account, t: 1.0, closing: false, scroll: 0.0 });
                let mut l = login::LoginPopup::new();
                if page == Some(1) {
                    l.submit();
                }
                self.login = Some(l);
            }
            // the register popup over the account page (page 1: after submitting empty fields)
            "register" => {
                self.settings = Some(options::Settings::Sub { sub: subpages::Sub::Account, t: 1.0, closing: false, scroll: 0.0 });
                let mut r = register::RegisterPopup { values: Default::default(), focus: 0, errors: [None; 5], active: false, job: None, t: 0.0 };
                if page == Some(1) {
                    r.submit();
                }
                self.register = Some(r);
            }
            "accounthelp" => self.settings = Some(options::Settings::Sub { sub: subpages::Sub::AccountHelp, t: 1.0, closing: false, scroll: 0.0 }),
            "share" => self.share = Some(share::Share::new(crate::gjapi::Upload { name: "Cadence Upload".into(), ..Default::default() })),
            // trig<id>: one such trigger in the editor, selected, its popup open
            w if w.strip_prefix("trig").is_some_and(|n| n.parse::<u16>().is_ok()) => {
                let id: u16 = w[4..].parse().unwrap_or(901);
                let mut e = editor::Editor::default();
                e.objects.push(editor::EdObject { id, x: 165.0, y: 105.0, ..editor::EdObject::default() });
                e.selected = vec![0];
                self.screen = Screen::Editor;
                self.editor = Some(e);
                self.replay = Some((replay::Scene::Trigger(id), Screen::Editor));
            }
            // your account's pages (CADENCE_SERVER at a stand-in with made-up answers)
            #[cfg(not(target_arch = "wasm32"))]
            "myprofile" | "friends" | "requests" | "acctsettings" | "oldmessages" => {
                let page = match which {
                    "oldmessages" => social::Page::Messages(0),
                    "friends" => social::Page::Friends(0),
                    "requests" => social::Page::Requests(0),
                    "acctsettings" => social::Page::Settings,
                    _ => social::Page::Profile,
                };
                self.social = Some(social::Social::open_at(page));
            }
            "account" | "support" | "songs" => {
                let sub = match which {
                    "account" => subpages::Sub::Account,
                    "support" => subpages::Sub::Support,
                    _ => subpages::Sub::Songs,
                };
                self.settings = Some(options::Settings::Sub { sub, t: 1.0, closing: false, scroll: 0.0 });
            }
            // options N: tab N / 10, page N % 10
            "options" => {
                let n = page.unwrap_or(0) as usize;
                self.settings = Some(options::Settings::More { tab: (n / 10) as u8, page: n % 10 });
            }
            "editor" => {
                let mut e = editor::Editor::default();
                for (i, id) in [1u16, 1, 1, 8, 1, 1, 8, 8, 1, 35].iter().enumerate() {
                    e.objects.push(editor::EdObject { id: *id, x: 15.0 + 30.0 * i as f32, y: 15.0 + if *id == 1 && i > 4 { 30.0 } else { 0.0 }, ..editor::EdObject::default() });
                }
                if let Some(id @ 100..) = page {
                    // any object selected (the popup stress runs): CADENCE_PAGE = its id
                    e.objects[3] = editor::EdObject { id: id as u16, x: 105.0, y: 75.0, ..editor::EdObject::default() };
                } else if let Some(id @ (2 | 3 | 5)) = page {
                    // a move (2) or color (3) trigger selected (the trigger popup screenshots)
                    e.objects[3] = editor::EdObject { id: [901, 899, 0, 914][id - 2], x: 105.0, y: 75.0, ..editor::EdObject::default() };
                }
                e.selected = vec![3];
                if page == Some(4) {
                    // an empty level (the real editor's first screen)
                    e.objects.clear();
                    e.selected.clear();
                }
                e.mode = match page.unwrap_or(0) {
                    1 => editor::Mode::Edit,
                    _ => editor::Mode::Build,
                };
                self.open_editor(e);
            }
            "rewards" => self.popup = Some(popups::Popup::Rewards),
            "tower" => self.replay = Some((replay::Scene::TowerInner, Screen::Levels)),
            "keys" => self.replay = Some((replay::Scene::Keys, Screen::Menu)),
            "shards" => {
                self.screen = Screen::Garage;
                self.replay = Some((replay::Scene::Shards, Screen::Garage));
            }
            "shop" => self.replay = Some((replay::Scene::Shop(page.unwrap_or(0) as u8), Screen::Garage)),
            "secret" => self.replay = Some((replay::Scene::Secret(page.unwrap_or(1) as u8), Screen::Menu)),
            "treasure" => self.replay = Some((replay::Scene::TreasureRoom, Screen::Creator)),
            "news" => {
                self.online.open_news();
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.news.is_empty() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                self.online.t = 0.0;
            }
            "opaths" => {
                self.screen = Screen::Creator;
                self.online.paths_open = true;
            }
            "oquests" => {
                self.screen = Screen::Creator;
                self.online.open_quests();
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.quests.is_empty() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                self.online.t = 0.0;
            }
            "odaily" => {
                self.screen = Screen::Creator;
                self.online.open_daily(page.unwrap_or(0) as i32);
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if self.online.daily_level.is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                self.online.t = 0.0;
            }
            "oscores" => {
                self.screen = Screen::Online;
                self.online.open_scores(page.unwrap_or(0) as u8);
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.busy() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            "ogauntlets" => {
                self.screen = Screen::Online;
                self.online.open_gauntlets();
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.busy() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            "opacks" | "olists" => {
                self.screen = Screen::Online;
                if which == "olists" {
                    self.online.open_lists();
                } else {
                    self.online.open_packs();
                }
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.busy() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            "osearch" => {
                self.screen = Screen::Online;
                self.online.view = online::View::Search;
            }
            "online" | "oinfo" => {
                self.screen = Screen::Online;
                if let Ok(q) = std::env::var("CADENCE_QUERY") {
                    self.online.query = q;
                    self.online.kind = crate::online::Kind::Search;
                }
                if let Some(k) = std::env::var("CADENCE_KIND").ok().and_then(|k| k.parse().ok()) {
                    let mut s = crate::online::Search::new(k);
                    s.query = self.online.query.clone();
                    self.online.open_list(s, true);
                } else {
                    self.online.fetch();
                }
                // wait for the servers (screenshots only)
                for _ in 0..1000 {
                    self.online.update(0.0);
                    if !self.online.busy() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                if which == "oinfo" {
                    self.online.click(Btn::OnlineView(0));
                }
            }
            "levels" | "locked" => {
                self.screen = Screen::Levels;
                if let Some(p) = page {
                    self.levels.jump_to(p);
                }
                if which == "locked" {
                    self.levels.jump_to(13);
                    self.clicked(Btn::LevelBox);
                }
            }
            _ => {}
        }
    }

    /// Advance the menus' animations by `secs` (screenshots of later moments).
    pub fn advance(&mut self, secs: f32) {
        let mut t = 0.0;
        while t < secs {
            self.update(1.0 / 60.0);
            t += 1.0 / 60.0;
        }
    }

    /// Actions for the app to carry out, and sounds to play, since the last call.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.pending)
    }

    /// Play queued sounds and the menu music on `audio` (the app's audio, when no level has it).
    pub fn play_audio(&mut self, audio: &mut dyn Audio) {
        for s in self.sounds.drain(..) {
            audio.play_effect(s);
        }
        let want = self.screen != Screen::Level && !self.progress.var("0122", false);
        if !want && self.menu_music && self.screen != Screen::Level {
            audio.stop_music();
            self.menu_music = false;
        }
        if want && !self.menu_music {
            audio.play_loop(MENU_MUSIC);
            self.menu_music = true;
        } else if !want {
            self.menu_music = false;
        }
    }

    /// Sounds queued while in a level (played on the level's audio).
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// The level's state after this frame's steps: saves progress on deaths and completion,
    /// counts attempts, and brings up the end screen.
    pub fn level_frame(&mut self, status: LevelStatus, dt: f32) {
        let Some(s) = self.session.as_mut() else { return };
        let prev = s.last;
        let id = s.id;
        if self.pause.is_none() {
            s.time += dt;
        }
        if status.practice {
            // Practice: only the practice best is saved; no attempts or coins.
            let pct = (status.percent as u32).min(100);
            let p = self.progress.entry(id);
            if status.end_time.is_some() {
                p.practice = 100;
            } else if status.dead && !prev.dead && pct > p.practice {
                p.practice = pct;
            }
            if (status.dead && !prev.dead) || (status.end_time.is_some() && prev.end_time.is_none()) {
                self.progress.save();
            }
            s.last = status;
            return;
        }
        let restarted = status.steps < prev.steps || (prev.dead && !status.dead) || (prev.end_time.is_some() && status.end_time.is_none());
        if restarted {
            s.attempts += 1;
            s.completed = false;
            self.progress.entry(id).attempts += 1;
            self.progress.save();
        }
        if status.dead && !prev.dead {
            // "Players destroyed" (stat 9)
            let n = self.progress.number("stat9", 0.0);
            self.progress.set_number("stat9", n + 1.0);
            // PlayLayer::destroyPlayer saves the normal-mode best.
            let pct = (status.percent as u32).min(100);
            let p = self.progress.entry(id);
            if pct > p.best {
                p.best = pct;
            }
            self.progress.save();
        }
        if status.end_time.is_some() && prev.end_time.is_none() {
            // PlayLayer::levelComplete: 100%, and the coins taken (secret coins count once
            // the level is completed with them).
            let p = self.progress.entry(id);
            s.first_completion = p.best < 100;
            p.best = 100;
            let mut new = [false; 3];
            for (i, &taken) in status.coins.iter().enumerate() {
                if taken && !p.coin(i) {
                    new[i] = true;
                }
                if taken {
                    p.coins |= 1 << i;
                }
            }
            s.new_coins = new;
            s.completed = true;
            // your own level completed in normal mode from its page: verified
            if id == -100 && !status.practice && !self.editor_playtest {
                if let Some(slot) = self.verify_slot {
                    self.progress.settings.insert(format!("mylevelverified{slot}"), "1".into());
                }
            }
            self.progress.save();
        }
        // PlayLayer::showCompleteText → showEndLayer 1.5 s later.
        if s.completed
            && self.end.is_none()
            && status.end_time.is_some_and(|t| t >= end::END_LAYER_AT)
        {
            self.end = Some(end::End::default());
        }
        s.last = status;
    }

    /// Your level's page's tools (`EditLevelLayer`): delete, folder down / up, the editor
    /// options, duplicate, export, death stats, level options, info, emotes.
    fn edit_level_tool(&mut self, i: u8) {
        let Some(e) = self.editor.as_ref() else { return };
        let slot = e.slot.unwrap_or(0);
        let name = if e.name.trim().is_empty() { format!("Unnamed {slot}") } else { e.name.clone() };
        match i {
            3 => {
                self.confirm = Some(Confirm {
                    title: "Delete Level".into(),
                    text: format!("Are you sure you want to <cr>delete</c> <cg>{name}</c>?"),
                    btn1: "Cancel".into(),
                    btn2: "Delete".into(),
                    action: Action::DeleteMyLevel(slot as u16),
                    t: 0.0,
                })
            }
            4 | 5 => {
                let key = format!("mylevelfolder{slot}");
                let f = self.progress.settings.get(&key).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                let f = if i == 5 { (f + 1).min(9) } else { (f - 1).max(0) };
                self.progress.settings.insert(key, f.to_string());
                self.progress.save();
            }
            7 => self.settings = Some(options::Settings::More { tab: 3, page: 0 }),
            8 => {
                let new = (0..1000).find(|i| !self.progress.settings.contains_key(&format!("mylevel{i}"))).unwrap_or(999);
                if let Some(data) = self.progress.settings.get(&format!("mylevel{slot}")).cloned() {
                    self.progress.settings.insert(format!("mylevel{new}"), data);
                    self.progress.settings.insert(format!("mylevelname{new}"), name.replace(' ', "\u{a0}"));
                    self.progress.save();
                    self.online.my_levels = self.my_levels();
                    self.alert = Some(Alert { title: "Duplicated".into(), text: format!("A copy of <cg>{name}</c> is in your levels."), t: 0.0 });
                }
            }
            9 => self.editor_pause_side(6),
            10 => self.editor_pause_side(9),
            11 => {
                let key = format!("mylevelldm{slot}");
                let on = !self.progress.var(&key, false);
                self.progress.set_var(&key, on);
                self.progress.save();
                self.alert = Some(Alert { title: "Options".into(), text: format!("<cg>Low Detail Mode</c> for this level is <cy>{}</c>.", if on { "on" } else { "off" }), t: 0.0 });
            }
            13 => {
                let verified = self.progress.settings.contains_key(&format!("mylevelverified{slot}"));
                let id = self.progress.settings.get(&format!("mylevelid{slot}")).cloned().unwrap_or_else(|| "na".into());
                self.alert = Some(Alert {
                    title: name.clone(),
                    text: format!("<cl>Objects</c>: {}\n<cy>Level ID</c>: {id}\n{}", e.objects.len(), if verified { "<cg>Verified</c>" } else { "<cr>Unverified</c>" }),
                    t: 0.0,
                });
            }
            _ => self.alert = Some(Alert { title: "Emotes".into(), text: "Put emotes in the description with their names between colons, like <cy>:happy:</c>.".into(), t: 0.0 }),
        }
    }

    /// The editor pause menu's side buttons: Settings (the editor options page), Export (a
    /// `.gmd` file), Editor SFX, Auto-Save, Death Stats, Get Audio, Help.
    fn editor_pause_side(&mut self, i: u8) {
        match i {
            5 => self.settings = Some(options::Settings::More { tab: 3, page: 0 }),
            6 => {
                let Some(e) = self.editor.as_ref() else { return };
                let name = if e.name.trim().is_empty() { "Unnamed".to_string() } else { e.name.clone() };
                let data = cadence_core::codec::base64_encode_url(&cadence_core::codec::gzip(e.level_string().as_bytes()));
                let gmd = format!("<?xml version=\"1.0\"?><plist version=\"1.0\" gjver=\"2.0\"><dict><k>kCEK</k><i>4</i><k>k2</k><s>{}</s><k>k4</k><s>{data}</s><k>k13</k><t /><k>k21</k><i>2</i><k>k50</k><i>45</i></dict></plist>", xml_escape(&name));
                self.alert = Some(match export_level(&name, gmd.as_bytes()) {
                    Ok(where_) => Alert { title: "Exported".into(), text: format!("Saved as <cy>{where_}</c>."), t: 0.0 },
                    Err(e) => Alert { title: "Export".into(), text: format!("The level couldn't be saved ({e})."), t: 0.0 },
                });
            }
            7 => self.alert = Some(Alert { title: "Editor SFX".into(), text: "Sound effects for the level come from the <cg>SFX</c> trigger's library; they download when the level is played.".into(), t: 0.0 }),
            8 => {
                let on = !self.progress.var("autosave", false);
                self.progress.set_var("autosave", on);
                self.progress.save();
                self.alert = Some(Alert { title: "Auto-Save".into(), text: format!("Auto-Save is <cy>{}</c>: the level saves every 5 minutes while you edit.", if on { "on" } else { "off" }), t: 0.0 });
            }
            9 => self.alert = Some(Alert { title: "Death Stats".into(), text: "Death markers show where players die once the level is shared and played.".into(), t: 0.0 }),
            10 => {
                // songs and library sounds the level's triggers use, downloaded now
                let Some(e) = self.editor.as_ref() else { return };
                let (mut songs, mut sfx) = (Vec::new(), Vec::new());
                for o in &e.objects {
                    let id = o.extra.iter().find(|(k, _)| *k == 392).and_then(|(_, v)| v.parse::<i32>().ok()).unwrap_or(0);
                    match o.id {
                        1934 if id > 0 => songs.push(id),
                        3602 if id > 0 => sfx.push(id),
                        _ => {}
                    }
                }
                let n = songs.len() + sfx.len();
                let _ = crate::net::spawn(async move {
                    for id in songs {
                        let _ = crate::online::song_file(id, "").await;
                    }
                    for id in sfx {
                        let _ = crate::online::sfx_file(id).await;
                    }
                });
                self.alert = Some(Alert { title: "Get Audio".into(), text: format!("Downloading <cy>{n}</c> song{} and sound effect{}.", if n == 1 { "" } else { "s" }, if n == 1 { "" } else { "s" }), t: 0.0 });
            }
            _ => self.alert = Some(Alert { title: "Help".into(), text: "<cg>Build</c> places objects, <cl>Edit</c> moves and changes the selected ones, <cr>Delete</c> removes them. Playtest with the play button on the left, and save from this menu.".into(), t: 0.0 }),
        }
    }

    /// Clone a saved level into your own levels (`LevelTools::copy`, as "<name>" in a new slot).
    fn clone_saved(&mut self, id: i32) {
        let Some(s) = crate::saved::get(id) else { return };
        let Some(text) = crate::saved::data(id).and_then(|d| cadence_core::codec::decode_level_bytes(&d).ok()) else {
            self.alert = Some(Alert { title: "Copy".into(), text: "The level's data couldn't be read.".into(), t: 0.0 });
            return;
        };
        let slot = (0..1000).find(|i| !self.progress.settings.contains_key(&format!("mylevel{i}"))).unwrap_or(999);
        self.progress.settings.insert(format!("mylevel{slot}"), cadence_core::codec::base64_encode_url(text.as_bytes()));
        self.progress.settings.insert(format!("mylevelname{slot}"), s.info.name.replace(' ', "\u{a0}"));
        self.progress.save();
        self.online.my_levels = self.my_levels();
        self.alert = Some(Alert { title: "Copied".into(), text: format!("<cg>{}</c> is now in your levels.", s.info.name), t: 0.0 });
    }

    fn social_effect(&mut self, e: social::Effect) {
        match e {
            social::Effect::None => {}
            social::Effect::Close => self.social = None,
            social::Effect::Alert(title, text) => self.alert = Some(Alert { title, text, t: 0.0 }),
            social::Effect::Profile(account) => {
                self.online.open_profile(account);
            }
            social::Effect::Write { to, title } => self.writer = Some(social::Writer::new(title, to)),
            social::Effect::Request { row, name, text } => {
                self.confirm = Some(Confirm {
                    title: name,
                    text: if text.is_empty() { "Accept this friend request?".into() } else { text },
                    btn1: "Cancel".into(),
                    btn2: "Accept".into(),
                    action: Action::AcceptRequest(row as u16),
                    t: 0.0,
                })
            }
        }
    }

    /// Save: the cloud's saves are downloaded first and Cadence's progress written into them
    /// (`cloud::merge`), then backed up. Load: downloaded and read into the progress.
    fn start_cloud(&mut self, save: bool) {
        self.cloud_last[!save as usize] = Some(web_time::Instant::now());
        self.cloud_save = save;
        let progress = self.progress.clone();
        self.cloud = Some(crate::net::spawn(async move {
            let mut data = crate::cloud::sync().await?;
            if !save {
                return Ok(CloudDone::Loaded(Box::new(data)));
            }
            data.game = crate::cloud::merge(&data.game, &progress).map_err(|_| -1)?;
            crate::cloud::backup(&data).await?;
            let mb = |x: &str| crate::cloud::encode_part(x).len() as f32 / (1024.0 * 1024.0);
            Ok(CloudDone::Saved(mb(&data.game), mb(&data.levels)))
        }));
    }

    fn update(&mut self, dt: f32) {
        // a cloud Save / Load finishing (`backupAccountFinished` / `syncAccountFinished` and
        // their failures)
        if let Some(rx) = &self.cloud {
            let done = match rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(-1)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(r) = done {
                self.cloud = None;
                let save = self.cloud_save;
                let (title, text) = match r {
                    Ok(CloudDone::Saved(profile, levels)) => (
                        "Backup successful",
                        format!("Your <cg>save data</c> has been <cy>backed up</c>.\nSize: {:.2}/32MB. (Profile: {profile:.2}, Levels: {levels:.2})", profile + levels),
                    ),
                    Ok(CloudDone::Loaded(data)) => match crate::cloud::import(&data.game, &mut self.progress) {
                        Ok(()) => {
                            self.progress.save();
                            ("Sync successful", "<cg>Saved data</c> has been <cy>downloaded</c>.".to_string())
                        }
                        Err(_) => ("Error", "Sync failed. Please try again later.".to_string()),
                    },
                    // a bad GJP2 (the password changed since logging in)
                    Err(-2) => ("Error", "Login failed. Please login to verify your account.".to_string()),
                    Err(-1) if save => ("Error", "Backup failed. Please try again later.".to_string()),
                    Err(-1) => ("Error", "Sync failed. Please try again later.".to_string()),
                    Err(code) if save => ("Error", format!("Backup failed. Error code: {code}")),
                    Err(code) => ("Error", format!("Sync failed. Error code: {code}")),
                };
                self.alert = Some(Alert { title: title.into(), text, t: 0.0 });
            }
        }
        if let Some(s) = self.social.as_mut() {
            let e = s.update(dt);
            self.social_effect(e);
        }
        // the chests' answer: the state, or an opened chest's contents (added to the stats)
        if let Some(rx) = &self.chest_job {
            if let Ok((kind, r)) = rx.try_recv() {
                self.chest_job = None;
                match r {
                    Ok(state) => {
                        if kind > 0 {
                            let items = if kind == 1 { state.small_items.clone() } else { state.big_items.clone() };
                            if let Some(it) = items {
                                let orbs = self.progress.number("stat14", 0.0) + it.orbs as f32;
                                let diamonds = self.progress.number("stat13", 0.0) + it.diamonds as f32;
                                self.progress.set_number("stat14", orbs);
                                self.progress.set_number("stat13", diamonds);
                                self.progress.save();
                                let mut text = format!("<cl>{}</c> Orbs and <cy>{}</c> Diamonds", it.orbs, it.diamonds);
                                if !it.items.is_empty() {
                                    text.push_str(&format!(" and {} item{}", it.items.len(), if it.items.len() == 1 { "" } else { "s" }));
                                }
                                self.alert = Some(Alert { title: if kind == 1 { "Small Chest" } else { "Big Chest" }.into(), text, t: 0.0 });
                            }
                        }
                        self.chests = Some((state, web_time::Instant::now()));
                    }
                    Err(e) => {
                        crate::app::log(&format!("chests: {e}"));
                        if kind > 0 {
                            self.alert = Some(Alert { title: "Error".into(), text: "Something went wrong. Please try again later.".into(), t: 0.0 });
                        }
                    }
                }
            }
        }
        if let Some(w) = self.writer.as_mut() {
            w.t += dt;
        }
        // the editor's Auto-Save: every 5 minutes of editing
        if self.screen == Screen::Editor && self.progress.var("autosave", false) {
            self.autosave_t += dt;
            if self.autosave_t >= 300.0 {
                self.autosave_t = 0.0;
                self.save_editor();
            }
        }
        // the upload's answer: the level's id (kept for its next version), or a failure
        if let Some(sh) = self.share.as_mut() {
            sh.t += dt;
            if let Some(Ok(r)) = sh.job.as_ref().map(|rx| rx.try_recv()) {
                let version = sh.upload.version;
                self.share = None;
                match r {
                    Ok(id) => {
                        if let Some(slot) = self.editor.as_ref().and_then(|e| e.slot) {
                            self.progress.settings.insert(format!("mylevelid{slot}"), id.to_string());
                            self.progress.settings.insert(format!("mylevelver{slot}"), version.to_string());
                            self.progress.save();
                        }
                        self.alert = Some(Alert { title: "Upload Complete".into(), text: format!("Your level has been shared! Its ID is <cy>{id}</c>."), t: 0.0 });
                    }
                    Err(-2) => self.alert = Some(Alert { title: "Upload Failed".into(), text: "Level already uploaded".into(), t: 0.0 }),
                    Err(_) => self.alert = Some(Alert { title: "Upload Failed".into(), text: "Upload Failed. Please try again later.".into(), t: 0.0 }),
                }
            }
        }
        // the register request (`registerAccountFinished` / `registerAccountFailed`)
        if let Some(r) = self.register.as_mut() {
            r.t += dt;
            let result = r.job.as_ref().and_then(|rx| match rx.try_recv() {
                Ok(v) => Some(v),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(-1)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
            match result {
                Some(Ok(())) => {
                    let email = r.values[3].clone();
                    self.register = None;
                    self.alert = Some(Alert {
                        title: "Created".into(),
                        text: format!("Thank you for registering.\nA confirmation email has been sent to your inbox <cy>{email}</c>.\nPlease <cg>activate</c> your account."),
                        t: 0.0,
                    });
                }
                Some(Err(code)) => r.failed(code),
                None => {}
            }
        }
        for p in self.press.values_mut() {
            p.update(dt);
        }
        // the login request (`loginAccountFinished` / `loginAccountFailed`)
        if let Some(l) = self.login.as_mut() {
            l.t += dt;
            let result = l.job.as_ref().and_then(|rx| match rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(-1)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
            match result {
                Some(Ok(account)) => {
                    self.login = None;
                    self.alert = Some(Alert {
                        title: "Login Success".into(),
                        text: format!("Thank you for logging in.\nYou have now linked your account:\n<cg>{}</c>", account.name),
                        t: 0.0,
                    });
                }
                Some(Err(code)) => l.failed(code),
                None => {}
            }
        }
        match self.screen {
            Screen::Menu => self.menu.update(dt, self.view, &mut self.rng),
            Screen::Levels => self.levels.update(dt),
            Screen::Online | Screen::Garage | Screen::Creator | Screen::Editor => {}
            Screen::Level => {
                if let Some(e) = self.end.as_mut() {
                    e.update(dt);
                }
            }
        }
        match self.settings.as_mut() {
            Some(options::Settings::DropDown { t, closing })
            | Some(options::Settings::Stats { t, closing, .. })
            | Some(options::Settings::Achievements { t, closing, .. })
            | Some(options::Settings::Sub { t, closing, .. }) => {
                *t += dt;
                if *closing && (*t >= 0.5 || self.progress.var("0168", false)) {
                    // Account Help sits over the Account page, which is still there under it
                    self.settings = match self.settings {
                        Some(options::Settings::Sub { sub: subpages::Sub::AccountHelp, .. }) => Some(options::Settings::Sub { sub: subpages::Sub::Account, t: 1.0, closing: false, scroll: 0.0 }),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
        // a new online view goes through a fade (switched halfway, like a scene)
        if self.screen != Screen::Online || self.progress.var("0168", false) {
            self.online_shown = self.online.view;
        } else if self.online.view != self.online_shown && self.fade_view.is_none() {
            if self.fade.is_none() {
                self.fade_view = Some(self.online.view);
                self.online.view = self.online_shown;
                self.fade = Some((0.0, None));
            } else {
                self.online_shown = self.online.view;
            }
        }
        if let Some((t, change)) = self.fade.as_mut() {
            *t += dt;
            if *t >= FADE_TIME * 0.5 {
                if let Some((screen, actions)) = change.take() {
                    self.screen = screen;
                    self.pending.extend(actions);
                }
                if let Some(v) = self.fade_view.take() {
                    self.online.view = v;
                    self.online_shown = v;
                }
            }
            if *t >= FADE_TIME {
                self.fade = None;
            }
        }
        // downloads keep going on any screen
        self.online.update(dt);
        if self.online.ready.is_some() && !self.pending.contains(&Action::PlayOnline) {
            self.pending.push(Action::PlayOnline);
        }
        for v in self.opened.get_mut().values_mut() {
            v.0 += dt;
        }
        if let Some(a) = self.alert.as_mut() {
            a.t += dt;
        }
        if let Some(d) = self.dialog.as_mut() {
            d.t += dt;
        }
        if let Some(c) = self.confirm.as_mut() {
            c.t += dt;
        }
    }

    /// Seconds the popup `key` has been open (0 the first frame it's drawn).
    fn open_t(&self, key: &str) -> f32 {
        let mut o = self.opened.borrow_mut();
        let e = o.entry(key.to_string()).or_insert((self.open_start, true));
        e.1 = true;
        if self.progress.var("0168", false) { 1.0 } else { e.0 }
    }

    /// Forget the popups that weren't drawn this frame (so they animate again next time).
    /// Screenshots: popups first drawn from now on start `secs` into their open animation.
    pub fn skip_open_animations(&mut self, secs: f32) {
        self.open_start = secs;
    }

    fn end_open_frame(&self) {
        let mut o = self.opened.borrow_mut();
        o.retain(|_, v| std::mem::take(&mut v.1));
    }

    /// Draw one FLAlertLayer popup with `FLAlertLayer::show`'s animation: its main layer
    /// from scale 0.1 to 1 (CCEaseElasticOut(CCScaleTo 0.5), period 0.6) about the screen's
    /// middle; its backdrop (the full-screen color) fading in over 0.14 s.
    fn popup_anim(&self, canvas: &mut Canvas, key: &str, draw: impl FnOnce(&mut Canvas)) {
        let t = self.open_t(key);
        let start = canvas.mark();
        draw(canvas);
        open_anim(canvas, start, t);
    }

    /// Draw a menu screen (main menu or level select) and present it.
    pub fn draw_screen(&mut self, renderer: &mut Renderer, sheets: &Sheets, dt: f32, screenshot: Option<&std::path::Path>) {
        let (w, h) = renderer.size();
        self.set_window(w as f32, h as f32);
        self.update(dt);
        let atlas = |s: usize| renderer.atlas_size(s);
        let mut canvas = Canvas { sheets, atlas: &atlas, fonts: &self.fonts, rigs: self.rigs.as_ref(), draws: Vec::with_capacity(1024), width: self.view.0, height: self.view.1, clip: None };
        let mut buttons = Vec::new();
        let ctx = Ctx { press: &self.press, buttons: &mut buttons };
        let mut ctx = ctx;
        match self.screen {
            Screen::Menu => self.menu.draw(&mut canvas, &mut ctx),
            Screen::Levels => self.levels.draw(&mut canvas, &mut ctx, &self.progress),
            Screen::Online => self.online.draw(&mut canvas, &mut ctx),
            Screen::Creator => creator::draw(&mut canvas, &mut ctx),
            Screen::Garage => {
                let set = self.icons();
                self.garage.draw(&mut canvas, &mut ctx, &set, &self.progress)
            }
            Screen::Level | Screen::Editor => {}
        }
        if let Some((scene, _)) = self.replay.filter(|r| r.0 != replay::Scene::Keys && r.0 != replay::Scene::Shards) {
            ctx.buttons.clear();
            self.draw_replay(&mut canvas, &mut ctx, scene);
        }
        match self.settings {
            Some(options::Settings::DropDown { t, closing }) => options::draw_dropdown(&mut canvas, &mut ctx, &self.progress, t, closing, self.volumes()),
            Some(options::Settings::More { tab, page }) => self.popup_anim(&mut canvas, "more", |c| options::draw_more(c, &mut ctx, &self.progress, tab, page)),
            Some(options::Settings::Stats { t, closing, scroll }) => options::draw_stats(&mut canvas, &mut ctx, &self.progress, t, closing, scroll),
            Some(options::Settings::Achievements { t, closing, scroll, page }) => achievements::draw(&mut canvas, &mut ctx, &self.progress, t, closing, scroll, page),
            Some(options::Settings::Sub { sub, t, closing, scroll }) => subpages::draw(&mut canvas, &mut ctx, &self.progress, sub, t, closing, scroll, self.cloud.is_some()),
            None => {}
        }
        if self.screen == Screen::Creator && self.online.daily.is_some() {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "daily", |c| self.online.draw_daily(c, &mut ctx));
        }
        if let Some((scene @ (replay::Scene::Keys | replay::Scene::Shards), _)) = self.replay {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, &format!("{scene:?}"), |canvas| {
                canvas.rect(-1.0, -1.0, canvas.width + 2.0, canvas.height + 2.0, [0.0; 3], 150.0 / 255.0);
                replay::draw(canvas, &mut ctx, scene, &|_| false);
            });
        }
        if self.screen == Screen::Menu && self.online.news_page.is_some() {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "news", |c| self.online.draw_news(c, &mut ctx));
        }
        if self.screen == Screen::Creator && self.online.paths_open {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "paths", |c| self.online.draw_paths(c, &mut ctx));
        }
        if self.screen == Screen::Creator && self.online.quests_open {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "quests", |c| self.online.draw_quests(c, &mut ctx));
        }
        if let Some(p) = self.popup {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, &format!("{p:?}"), |c| popups::draw(c, &mut ctx, &self.progress, p, &self.chests));
        }
        if let Some(l) = &self.login {
            ctx.buttons.clear();
            let clock = l.t;
            self.popup_anim(&mut canvas, "login", |c| login::draw(l, c, &mut ctx, clock));
        }
        if let Some(r) = &self.register {
            ctx.buttons.clear();
            let clock = r.t;
            self.popup_anim(&mut canvas, "register", |c| register::draw(r, c, &mut ctx, clock));
        }
        if let Some(s) = &self.social {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "social", |c| social::draw(s, c, &mut ctx));
        }
        if let Some(sh) = &self.share {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "share", |c| share::draw(sh, c, &mut ctx));
        }
        if let Some(w) = &self.writer {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "writer", |c| social::draw_writer(w, c, &mut ctx));
        }
        if self.social.is_some() && self.screen != Screen::Online {
            self.online.draw_profile_over(&mut canvas, &mut ctx);
        }
        if self.map_promo {
            ctx.buttons.clear();
            self.popup_anim(&mut canvas, "promo", |c| draw_map_promo(c, &mut ctx));
        }
        if let Some(d) = &self.dialog {
            ctx.buttons.clear();
            draw_dialog(d, &mut canvas, &mut ctx);
        }
        if let Some(cf) = &self.confirm {
            ctx.buttons.clear();
            draw_confirm(cf, &mut canvas, &mut ctx);
        }
        if let Some(a) = &self.alert {
            ctx.buttons.clear();
            draw_alert(a, &mut canvas, &mut ctx);
        }
        if let Some((t, _)) = self.fade {
            canvas.rect(-1.0, -1.0, canvas.width + 2.0, canvas.height + 2.0, [0.0; 3], fade_alpha(t));
        }
        self.end_open_frame();
        let draws = std::mem::take(&mut canvas.draws);
        drop(canvas);
        if std::env::var_os("CADENCE_DUMP_BUTTONS").is_some() {
            for b in &buttons {
                eprintln!("button {:?} {:.1} {:.1}", b.id, b.x, b.y);
            }
        }
        self.buttons = buttons;
        renderer.render_to([0.0, 0.0, 0.0], (0.0, 0.0, self.view.0, self.view.1), &draws, screenshot);
    }

    /// A screen drawn from a captured layout (`replay`), with what Cadence draws over it.
    /// A captured screen or popup; the popups (FLAlertLayers) open with their animation.
    fn draw_replay(&self, canvas: &mut Canvas, ctx: &mut Ctx, scene: replay::Scene) {
        let start = canvas.mark();
        self.draw_replay_inner(canvas, ctx, scene);
        if replay::is_popup(scene) {
            open_anim(canvas, start, self.open_t(&format!("{scene:?}")));
        }
    }

    fn draw_replay_inner(&self, canvas: &mut Canvas, ctx: &mut Ctx, scene: replay::Scene) {
        let progress = &self.progress;
        if matches!(
            scene,
            replay::Scene::Keys | replay::Scene::EditGroup | replay::Scene::EditColor | replay::Scene::EditorPause | replay::Scene::LevelSettings | replay::Scene::EditText | replay::Scene::SelectArt(_) | replay::Scene::Trigger(_) | replay::Scene::Special(_)
        ) {
            canvas.rect(-1.0, -1.0, canvas.width + 2.0, canvas.height + 2.0, [0.0; 3], 150.0 / 255.0);
        } else {
            canvas.rect(-1.0, -1.0, canvas.width + 2.0, canvas.height + 2.0, [0.0; 3], 1.0);
        }
        let is_edit = scene == replay::Scene::EditLevel && self.editor.as_ref().is_some_and(|e| !e.name.is_empty());
        let vault = matches!(scene, replay::Scene::Secret(1 | 2 | 4));
        let (typed, said) = (!self.vault_input.is_empty(), self.vault_msg.is_some());
        // each vault's first line (the label its messages replace)
        const FIRST: [&str; 3] = ["zzzZZZZ...", "Don't touch that!", "Why have I been awoken?"];
        let paused = scene == replay::Scene::EditorPause;
        let settings = scene == replay::Scene::LevelSettings;
        replay::draw(canvas, ctx, scene, &|t| {
            (is_edit && t == "Level Name")
                || (vault && ((typed && t == "...") || (said && FIRST.contains(&t))))
                || (paused && (t.starts_with("Objects: ") || t.starts_with("Length: ")))
                || (settings && (t == "01: Stereo Madness" || t == "frame:boost_02_001.png" || t == "frame:gj_iconBtn_off_001.png" || t == "frame:bgIcon_01_001.png" || t == "frame:gIcon_01_001.png" || t == "frame:mgIcon_00_001.png"))
                || (scene == replay::Scene::Trigger(899) && t == "frame:block009c_base_001.png")
        });
        // the color trigger's swatch, in its color (keys 7/8/9)
        if scene == replay::Scene::Trigger(899)
            && let Some(o) = self.editor.as_ref().and_then(|e| e.selection().next())
        {
            let dx = (canvas.width - 569.0) * 0.5;
            let c = |k: u16| o.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(255.0).clamp(0.0, 255.0) as u8;
            canvas.sprite("block009c_base_001.png", 84.5 + dx, 285.0, canvas::Look::default().rgb([c(7), c(8), c(9)]));
        }
        if settings && let Some(e) = &self.editor {
            let dx = (canvas.width - 569.0) * 0.5;
            let num = |k: &str| e.setting(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            let song = num("kA1").clamp(0, 21);
            let name = format!("{:02}: {}", song + 1, level_info(song + 1).name);
            let (tw, _) = canvas.text_size("bigFont", &name);
            canvas.text("bigFont", &name, 284.5 + dx, 95.0, canvas::Look::scaled((200.0 / tw.max(1.0)).min(0.6)));
            // the chosen background and ground
            canvas.sprite(&format!("bgIcon_{:02}_001.png", num("kA6").clamp(0, 58) + 1), 469.5 + dx, 263.0, canvas::Look::default());
            canvas.sprite(&format!("gIcon_{:02}_001.png", num("kA7").clamp(0, 21) + 1), 469.5 + dx, 198.0, canvas::Look::default());
            canvas.sprite(&format!("mgIcon_{:02}_001.png", num("kA25").clamp(0, 3)), 469.5 + dx, 133.0, canvas::Look::default());
            // the start speed's portal and the start mode's icon
            const SPEEDS: [&str; 5] = ["boost_02_001.png", "boost_01_001.png", "boost_03_001.png", "boost_04_001.png", "boost_05_001.png"];
            canvas.sprite(SPEEDS[num("kA4").clamp(0, 4) as usize], 99.5 + dx, 261.0, canvas::Look::scaled(0.6));
            const MODES: [&str; 8] = ["gj_iconBtn_off_001.png", "gj_shipBtn_off_001.png", "gj_ballBtn_off_001.png", "gj_birdBtn_off_001.png", "gj_dartBtn_off_001.png", "gj_robotBtn_off_001.png", "gj_spiderBtn_off_001.png", "gj_swingBtn_off_001.png"];
            canvas.sprite(MODES[num("kA2").clamp(0, 7) as usize], 99.5 + dx, 183.0, canvas::Look::default());
            // Classic / Platformer: the chosen one is GJ_button_01
            if num("kA22") == 1 {
                canvas.nine_slice("GJ_button_04.png", 219.5 + dx, 191.0, 116.0, 24.0, canvas::Look::default());
                canvas.text("bigFont", "Classic", 218.5 + dx, 193.0, canvas::Look::scaled(0.55));
                canvas.nine_slice("GJ_button_01.png", 349.5 + dx, 191.0, 116.0, 24.0, canvas::Look::default());
                canvas.text("bigFont", "Platformer", 348.5 + dx, 193.0, canvas::Look::scaled(0.5063));
            }
        }
        if let replay::Scene::SelectArt(kind) = scene
            && let Some(e) = &self.editor
        {
            // the chosen icon in full color over the gray grid (its button's spot)
            let dx = (canvas.width - 569.0) * 0.5;
            let key = ["kA6", "kA7", "kA25"][kind.min(2) as usize];
            let chosen = e.setting(key).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            if let Some((x, y)) = replay::button_center(scene, chosen as u8) {
                let (frame, sc) = match kind {
                    0 => (format!("bgIcon_{:02}_001.png", chosen + 1), 0.6),
                    1 => (format!("gIcon_{:02}_001.png", chosen + 1), 0.7),
                    _ => (format!("mgIcon_{:02}_001.png", chosen), 0.8),
                };
                canvas.sprite(&frame, x + dx, y, canvas::Look::scaled(sc));
            }
        }
        if scene == replay::Scene::EditText {
            let dx = (canvas.width - 569.0) * 0.5;
            let text = self.object_text();
            let shown = if text.is_empty() { "Text".to_string() } else { format!("{text}_") };
            let (tw, _) = canvas.text_size("bigFont", &shown);
            let look = canvas::Look::scaled((280.0 / tw.max(1.0)).min(0.5));
            canvas.text("bigFont", &shown, 284.5 + dx, 195.0, if text.is_empty() { look.rgb([150, 150, 150]) } else { look });
        }
        // the editor pause menu's level stats (EditorPauseLayer::init): the object count and
        // the length (the last object's x at 1x speed, 311.58 units a second)
        if scene == replay::Scene::EditorPause
            && let Some(e) = &self.editor
        {
            let secs = (e.objects.iter().map(|o| o.x).fold(0.0f32, f32::max) / 311.58).max(1.0) as i32;
            let size = match secs {
                0..10 => "Tiny",
                10..30 => "Short",
                30..60 => "Medium",
                60..120 => "Long",
                _ => "XL",
            };
            let look = canvas::Look::scaled(0.5).anchor(0.0, 1.0);
            canvas.text("goldFont", &format!("Objects: {}/40000", e.objects.len()), 10.0, canvas.height - 5.0, look);
            canvas.text("goldFont", &format!("Length: {secs} seconds ({size})"), 10.0, canvas.height - 25.0, look);
        }
        if vault {
            let dx = (canvas.width - 569.0) * 0.5;
            if typed {
                let (w, _) = canvas.text_size("bigFont", &self.vault_input);
                // the field is 180 wide (setMaxLabelWidth), text at 0.7
                canvas.text("bigFont", &self.vault_input, 284.5 + dx, 210.0, canvas::Look::scaled((180.0 / w.max(1.0)).min(0.7)).rgb([108, 153, 216]));
            }
            if let Some((m, color)) = &self.vault_msg {
                // limitLabelWidth(320, 0.6)
                let (w, _) = canvas.text_size("bigFont", m);
                canvas.text("bigFont", m, 284.5 + dx, 253.0, canvas::Look::scaled((320.0 / w.max(1.0)).min(0.6)).rgb(*color));
            }
        }
        if scene == replay::Scene::EditLevel
            && let Some(e) = &self.editor
        {
            let dx = (canvas.width - 569.0) * 0.5;
            let caret = if (self.t_random() * 2.0) as i32 % 2 == 0 { "" } else { "" };
            let (nw, _) = canvas.text_size("bigFont", &e.name);
            canvas.text("bigFont", &format!("{}{caret}", e.name), 284.5 + dx, 290.0, canvas::Look::scaled((300.0 / nw.max(1.0)).min(0.7)));
        }
        if scene == replay::Scene::EditGroup
            && let Some(o) = self.editor.as_ref().and_then(|e| e.selection().next())
        {
            let dx = (canvas.width - 569.0) * 0.5;
            let num = |k: u16, def: i32| o.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(def);
            let z_def = cadence_core::objects::visual(o.id as i32).map_or(0, |v| v.z_order as i32);
            for (text, x, y) in [
                (num(20, 0).to_string(), 159.5, 269.0),
                (num(61, 0).to_string(), 284.5, 269.0),
                (num(25, z_def).to_string(), 409.5, 269.0),
                (self.group_add.to_string(), 204.5, 209.0),
            ] {
                canvas.text("bigFont", &text, x + dx, y, canvas::Look::scaled(0.6));
            }
            // the key's own value, and the effective layer highlighted (GJ_button_02)
            let raw = num(24, 0);
            canvas.text("chatFont", &format!("({raw})"), 324.5 + dx, 94.0, canvas::Look::scaled(0.6).rgb([0, 0, 0]));
            let layer = if raw != 0 { raw } else { cadence_core::objects::visual(o.id as i32).map_or(5, |v| v.z_layer as i32) };
            const ZB: [(f32, f32, &str); 9] = [
                (114.85, 29.8, "B5"),
                (150.18, 30.85, "B4"),
                (185.58, 29.95, "B3"),
                (220.53, 29.95, "B2"),
                (254.88, 28.75, "B1"),
                (288.62, 28.75, "T1"),
                (322.98, 29.95, "T2"),
                (357.93, 29.95, "T3"),
                (393.32, 30.85, "T4"),
            ];
            if let Some(k) = editor::Z_LAYERS.iter().position(|z| *z == layer) {
                let (x, w, label) = ZB[k];
                canvas.nine_slice("GJ_button_02.png", x + dx, 68.0, w, 25.0, canvas::Look::default());
                canvas.text("bigFont", label, x - 1.0 + dx, 70.0, canvas::Look::scaled(0.3));
            }
            // the groups, two rows of eight in the list box; a click removes one
            for (k, g) in o.groups().iter().enumerate() {
                let (col, row) = ((k % 8) as f32, (k / 8) as f32);
                let (x, y) = (284.5 - 3.5 * 42.0 + 42.0 * col + dx, 157.0 - 28.0 * row);
                canvas.nine_slice("GJ_button_04.png", x, y, 38.0, 24.0, canvas::Look::default());
                let t = g.to_string();
                let (tw, _) = canvas.text_size("goldFont", &t);
                canvas.text("goldFont", &t, x, y + 1.0, canvas::Look::scaled((30.0 / tw.max(1.0)).min(0.6)));
                ctx.area(Btn::Replay(100 + k as u8), x, y, 38.0, 24.0);
            }
        }
        if scene == replay::Scene::EditColor
            && let Some(o) = self.editor.as_ref().and_then(|e| e.selection().next())
        {
            let dx = (canvas.width - 569.0) * 0.5;
            let key = if self.color_detail { 22 } else { 21 };
            let channel = o.get(key).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            // the chosen channel's button, highlighted (GJ_button_02)
            const BUTTONS: [(i32, f32, f32, f32, &str); 14] = [
                (1005, 149.5, 185.0, 68.8, "P-Col 1"),
                (1006, 239.5, 185.0, 70.4, "P-Col 2"),
                (1007, 329.5, 185.0, 77.2, "Light BG"),
                (0, 419.5, 185.0, 68.4, "Default"),
                (1, 154.5, 145.0, 46.0, "1"),
                (2, 219.5, 145.0, 46.0, "2"),
                (3, 284.5, 145.0, 46.0, "3"),
                (4, 349.5, 145.0, 46.0, "4"),
                (5, 414.5, 145.0, 46.0, "5"),
                (6, 154.5, 105.0, 46.0, "6"),
                (7, 219.5, 105.0, 46.0, "7"),
                (8, 284.5, 105.0, 46.0, "8"),
                (9, 349.5, 105.0, 46.0, "9"),
                (10, 414.5, 105.0, 46.0, "10"),
            ];
            if let Some(&(_, x, y, w, label)) = BUTTONS.iter().find(|b| b.0 == channel) {
                canvas.nine_slice("GJ_button_02.png", x + dx, y, w, 25.0, canvas::Look::default());
                let size = if label.len() > 2 { 0.5 } else { 0.4 };
                canvas.text("bigFont", label, x - 1.0 + dx, y + 2.0, canvas::Look::scaled(size));
            }
            if self.color_detail {
                // the Base button reads Detail
                canvas.nine_slice("GJ_button_03.png", 147.5 + dx, 284.0, 86.0, 30.0, canvas::Look::default());
                canvas.text("bigFont", "Detail", 146.5 + dx, 286.0, canvas::Look::scaled(0.5));
            }
            if o.id == 914 {
                canvas.nine_slice("GJ_button_04.png", 421.5 + dx, 284.0, 86.0, 30.0, canvas::Look::default());
                canvas.text("bigFont", "Text", 420.5 + dx, 286.0, canvas::Look::scaled(0.6));
                ctx.area(Btn::Replay(200), 421.5 + dx, 284.0, 86.0, 30.0);
            }
            // a channel past 10 shows its number by the color button
            if channel > 10 && channel < 1000 {
                canvas.text("bigFont", &channel.to_string(), 439.5 + dx, 30.0, canvas::Look::scaled(0.4));
            }
        }
        if let replay::Scene::Trigger(_) | replay::Scene::Special(_) = scene
            && let Some(o) = self.editor.as_ref().and_then(|e| e.selection().next())
        {
            let dx = (canvas.width - 569.0) * 0.5;
            // a color popup's picker, at this object's color: ring, the square tinted with the
            // hue, its overlay and shadow, the two knobs; one hit area for both
            if replay::has_frame(scene, "huePickerBackground.png") {
                let rgb = [7, 8, 9].map(|k| o.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(255.0) / 255.0);
                let (h, sat, val) = rgb_to_hsv(rgb);
                let tint = hsv_to_rgb(h, 1.0, 1.0).map(|v| (v * 255.0) as u8);
                canvas.sprite("huePickerBackground.png", 208.5 + dx, 121.0, canvas::Look::default().anchor(0.0, 0.0));
                canvas.sprite("colourPickerBackground.png", 227.5 + dx, 141.0, canvas::Look::default().anchor(0.0, 0.0).rgb(tint));
                canvas.sprite("colourPickerOverlay.png", 227.5 + dx, 141.0, canvas::Look::default().anchor(0.0, 0.0));
                canvas.sprite("colourPickerShadow.png", 227.5 + dx, 141.0, canvas::Look::default().anchor(0.0, 0.0));
                let a = (h / 360.0) * std::f32::consts::TAU - std::f32::consts::PI;
                canvas.sprite("colourPicker.png", 283.5 + 60.0 * a.cos() + dx, 196.0 + 60.0 * a.sin(), canvas::Look::default());
                canvas.sprite("colourPicker.png", 227.5 + 35.0 + 55.0 * (1.0 - sat) + dx, 141.0 + 35.0 + 55.0 * val, canvas::Look::default());
                ctx.area(Btn::Picker, 283.5 + dx, 196.0, 160.0, 160.0);
            }
            let (fields, defaults) = replay::fields(scene);
            let value = |key: u16| o.get(key).map(str::to_string).or_else(|| defaults.iter().find(|d| d.0 == key).map(|d| d.1.clone())).unwrap_or_default();
            for field in fields {
                match field {
                    replay::Field::Input { key, x, y, w, h } => {
                        let focused = self.trig_focus.as_ref().filter(|f| f.0 == key);
                        // decimals show two places, like the game's inputs ("0.50")
                        // a decimal field (its default has a point) always shows two places
                        let decimal = defaults.iter().any(|d| d.0 == key && d.1.contains('.')) || replay::label_at(scene, x, y).is_some_and(|t| t.contains('.'));
                        let text = match focused {
                            Some((_, t)) => format!("{t}_"),
                            None => match value(key).parse::<f32>() {
                                Ok(v) if v.fract() != 0.0 || decimal => format!("{v:.2}"),
                                _ => value(key),
                            },
                        };
                        let (tw, _) = canvas.text_size("bigFont", &text);
                        let look = canvas::Look::scaled((w * 0.9 / tw.max(1.0)).min(0.5));
                        canvas.text("bigFont", &text, x + dx, y, if focused.is_some() { look.rgb([255, 255, 150]) } else { look });
                        ctx.area(Btn::TrigInput(key), x + dx, y, w.max(30.0), h.max(20.0));
                    }
                    replay::Field::Toggle { key, x, y, w, h } => {
                        if value(key).parse::<f32>().is_ok_and(|v| v != 0.0) {
                            let (sw, _) = canvas.size("GJ_checkOn_001.png");
                            canvas.sprite("GJ_checkOn_001.png", x + dx, y, canvas::Look::scaled(w / sw.max(1.0)));
                        }
                        ctx.area(Btn::TrigToggle(key), x + dx, y, w, h);
                    }
                    replay::Field::Slider { .. } => {}
                }
            }
        }
        // the Tower's doors open as the floors are beaten
        if scene == replay::Scene::TowerInner {
            let dx = (canvas.width - 569.0) * 0.5;
            for (k, (x, y)) in [(388.25f32, 128.23f32), (189.0, 182.98), (342.0, 237.23)].into_iter().enumerate() {
                if progress.get(5001 + k as i32).best >= 100 {
                    canvas.sprite("towerDoor_unlocked_001.png", x + dx, y, canvas::Look::default());
                }
            }
        }
    }

    /// Draw the level's UI (pause button, pause menu, end screen) on top of the level's sprites.
    /// `view` is the level's view (left, bottom, width, height) in world units; the UI is drawn
    /// in screen points and mapped into it.
    pub fn overlay(&mut self, sheets: &Sheets, atlas: &dyn Fn(usize) -> (f32, f32), level_name: &str, view: (f32, f32, f32, f32), draws: &mut Vec<Draw>, dt: f32) {
        self.update(dt);
        let mut canvas = Canvas { sheets, atlas, fonts: &self.fonts, rigs: self.rigs.as_ref(), draws: Vec::with_capacity(256), width: self.view.0, height: self.view.1, clip: None };
        let mut buttons = Vec::new();
        let mut ctx = Ctx { press: &self.press, buttons: &mut buttons };
        if self.screen == Screen::Editor {
            self.editor_view = view;
            if let Some(e) = &self.editor {
                e.draw(&mut canvas, &mut ctx, view);
            }
            if self.editor_paused {
                ctx.buttons.clear();
                self.draw_replay(&mut canvas, &mut ctx, replay::Scene::EditorPause);
            }
            if let Some((scene, _)) = self.replay {
                ctx.buttons.clear();
                self.draw_replay(&mut canvas, &mut ctx, scene);
            }
            if let Some(a) = &self.alert {
                ctx.buttons.clear();
                draw_alert(a, &mut canvas, &mut ctx);
            }
            self.end_open_frame();
            let ui_draws = std::mem::take(&mut canvas.draws);
            drop(canvas);
            self.buttons = buttons;
            push_ui_draws(ui_draws, view, draws);
            return;
        }
        let session = self.session.clone().unwrap_or_default();
        let progress = self.progress.get(session.id);
        let view_s = session_view(&session, progress);
        // the "Level Complete!" text, under the end screen when it drops in
        if let Some(t) = session.last.end_time {
            end::draw_complete_text(&mut canvas, t);
        }
        if let Some(end) = self.end.as_mut() {
            for _ in 0..end.due_sounds(&view_s) {
                self.sounds.push("highscoreGet02.ogg");
            }
            end.draw(&mut canvas, &mut ctx, &view_s, self.pause_hidden);
        } else if self.pause.is_some() {
            pause::draw(&mut canvas, &mut ctx, level_name, &view_s, self.volumes(), self.pause_hidden);
            // the options pages, from the pause menu's gear
            if let Some(options::Settings::More { tab, page }) = self.settings {
                // (only the pages' buttons react while they're up)
                ctx.buttons.clear();
                self.popup_anim(&mut canvas, "more", |c| options::draw_more(c, &mut ctx, &self.progress, tab, page));
            }
            // a captured screen opened from there (Keys)
            if let Some((scene, _)) = self.replay {
                ctx.buttons.clear();
                self.draw_replay(&mut canvas, &mut ctx, scene);
            }
        } else if !self.hold_end() && cfg!(target_arch = "wasm32") {
            // UILayer: the pause button, top right, opacity 75. The game shows it only with the
            // cursor shown in game (game variable 0024, off by default on desktop); the web build
            // shows it for touch screens.
            let (bw, bh) = canvas.size("GJ_pauseBtn_clean_001.png");
            let (x, y) = (canvas.width - 15.0, canvas.height - 15.0);
            canvas.sprite("GJ_pauseBtn_clean_001.png", x, y, canvas::Look::default().alpha(75));
            // setSizeMult(4): a generous touch area
            ctx.buttons.push(HitArea { id: Btn::PauseButton, x, y, hw: bw * 2.0, hh: bh * 2.0 });
        }
        if let Some(a) = &self.alert {
            ctx.buttons.clear();
            draw_alert(a, &mut canvas, &mut ctx);
        }
        if let Some((t, _)) = self.fade {
            canvas.rect(-1.0, -1.0, canvas.width + 2.0, canvas.height + 2.0, [0.0; 3], fade_alpha(t));
        }
        self.end_open_frame();
        let ui_draws = std::mem::take(&mut canvas.draws);
        drop(canvas);
        self.buttons = buttons;
        push_ui_draws(ui_draws, view, draws);
    }
}

/// UI draws (screen points, 320 high) placed into the level's world view.
fn push_ui_draws(ui_draws: Vec<Draw>, view: (f32, f32, f32, f32), draws: &mut Vec<Draw>) {
    let (left, bottom, _, vh) = view;
    let s = vh / 320.0;
    for mut d in ui_draws {
        let m = &mut d.instance.m;
        for v in m.iter_mut() {
            *v *= s;
        }
        m[4] += left;
        m[5] += bottom;
        draws.push(d);
    }
}

/// What the pause menu and end screen show about the session.
pub struct SessionView {
    pub id: i32,
    pub attempts: u32,
    pub jumps: u32,
    pub time: f32,
    pub best: u32,
    pub practice: u32,
    /// Coins: collected in the save, taken this attempt, newly collected by the completion.
    pub saved_coins: [bool; 3],
    pub taken_coins: [bool; 3],
    pub new_coins: [bool; 3],
    /// What a first completion awards: (stars, orbs).
    pub reward: Option<(u32, u32)>,
    pub coin_count: usize,
    /// This attempt's progress, 0..100.
    pub percent: f32,
    pub in_practice: bool,
}

fn session_view(s: &Session, p: save::LevelProgress) -> SessionView {
    SessionView {
        id: s.id,
        attempts: s.attempts,
        jumps: s.jumps,
        time: s.time,
        best: p.best,
        practice: p.practice,
        saved_coins: [p.coin(0), p.coin(1), p.coin(2)],
        taken_coins: s.last.coins,
        new_coins: s.new_coins,
        reward: (s.first_completion && ((1..=22).contains(&s.id) || s.id == 3001)).then(|| {
            let info = level_info(s.id);
            (info.stars, levels::orbs_for(&info))
        }),
        coin_count: s.last.coin_count,
        percent: s.last.percent,
        in_practice: s.last.practice,
    }
}

/// A button's press scale (`CCMenuItemSpriteExtra` +0x190): 1.26, the level box 1.1.
fn press_mult(id: Btn) -> f32 {
    match id {
        Btn::LevelBox => 1.1,
        _ => canvas::PRESS_SCALE,
    }
}

/// Per-frame drawing context: press animations to read, hit areas to register.
pub struct Ctx<'a> {
    pub(crate) press: &'a HashMap<Btn, Press>,
    pub(crate) buttons: &'a mut Vec<HitArea>,
}

impl Ctx<'_> {
    /// A `CCMenuItemSpriteExtra` with sprite `name` at (x, y): drawn at its press scale, and its
    /// hit area (content size × scale × size multiplier) registered.
    pub fn button(&mut self, canvas: &mut Canvas, id: Btn, name: &str, x: f32, y: f32, look: canvas::Look, size_mult: f32) {
        let f = self.press.get(&id).map_or(1.0, |p| p.factor());
        let mut l = look;
        l.scale = (look.scale.0 * f, look.scale.1 * f);
        canvas.sprite(name, x, y, l);
        let (w, h) = canvas.size(name);
        self.area(id, x, y, w * look.scale.0.abs() * size_mult, h * look.scale.1.abs() * size_mult);
    }

    /// The press scale of a button (for buttons drawn from several parts).
    pub fn factor(&self, id: Btn) -> f32 {
        self.press.get(&id).map_or(1.0, |p| p.factor())
    }

    pub fn area(&mut self, id: Btn, x: f32, y: f32, w: f32, h: f32) {
        self.buttons.push(HitArea { id, x, y, hw: w * 0.5, hh: h * 0.5 });
    }
}

/// `FLAlertLayer` (simplified): a dark backdrop, the `square01_001` box, a gold title, the text
/// and an OK button (`ButtonSprite` "OK" on GJ_button_01).
/// `DialogLayer::init`: the box (GJ_square%02d, 380 × 100), a darker text panel, the
/// portrait left, the title (goldFont 0.5) top left of the panel, the text (chatFont) under it;
/// a click anywhere goes on.
fn draw_dialog(d: &Dialog, c: &mut Canvas, ctx: &mut Ctx) {
    use canvas::Look;
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], (d.t / 0.14).min(1.0) * 150.0 / 255.0);
    let s = canvas::ease_elastic_out((d.t / 0.5).min(1.0), 0.6) * 0.3 + 0.7;
    c.nine_slice(&format!("GJ_square{:02}.png", d.style), cx, cy, 380.0 * s, 100.0 * s, Look::default());
    c.nine_slice("square02b_001.png", cx + 40.0 * s, cy, 280.0 * s, 80.0 * s, Look::default().rgb([0, 0, 0]).alpha(50));
    c.text("goldFont", &d.title, cx - 93.0 * s, cy + 36.0 * s, Look::scaled(0.5 * s).anchor(0.0, 1.0));
    c.sprite(&format!("dialogIcon_{:03}.png", d.icon), cx - 143.0 * s, cy, Look::scaled(s));
    let chat = if (d.t * 2.0) as i32 % 2 == 0 { "GJ_chatBtn_01_001.png" } else { "GJ_chatBtn_02_001.png" };
    c.sprite(chat, cx, cy - 50.0 * s, Look::scaled(s));
    if let Some(text) = d.lines.get(d.page) {
        let lines = canvas::rich_lines(c, "chatFont", text, 260.0);
        canvas::draw_rich_left(c, "chatFont", &lines, cx - 92.0 * s, cy + 18.0 * s, s, 20.0);
    }
    ctx.area(Btn::DialogNext, cx, cy, w, h);
}

/// The Map's promo (`CreatorLayer::onAdventureMap`): mapPromo.png in the middle.
fn draw_map_promo(c: &mut Canvas, ctx: &mut Ctx) {
    use canvas::Look;
    let (w, h) = (c.width, c.height);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    let (pw, ph) = c.size("mapPromo.png");
    c.sprite("mapPromo.png", w * 0.5, h * 0.5, Look::default());
    ctx.button(c, Btn::MapPromoClose, "GJ_closeBtn_001.png", w * 0.5 - pw * 0.5 + 5.0, h * 0.5 + ph * 0.5 - 5.0, Look::default(), 1.5);
}

/// FLAlertLayer with two `ButtonSprite`s (goldFont, label + 14 wide), at most 15 apart.
fn draw_confirm(a: &Confirm, canvas: &mut Canvas, ctx: &mut Ctx) {
    use canvas::Look;
    let (w, h) = (canvas.width, canvas.height);
    canvas.rect(0.0, 0.0, w, h, [0.0; 3], (a.t / 0.14).min(1.0) * 150.0 / 255.0);
    let s = canvas::ease_elastic_out((a.t / 0.5).min(1.0), 0.6) * 0.9 + 0.1; // setScale(0.1) → 1
    let lines = canvas::rich_lines(canvas, "chatFont", &a.text, 264.0);
    let text_h = lines.len() as f32 * 20.0;
    let bh = (text_h + 120.0).max(140.0);
    let (cx, cy) = (w * 0.5, h * 0.5);
    canvas.nine_slice("square01_001.png", cx, cy, 300.0 * s, bh * s, Look::default());
    let (tw, _) = canvas.text_size("goldFont", &a.title);
    let ts = (270.0 / tw.max(1.0)).clamp(0.4, 0.9);
    canvas.text("goldFont", &a.title, cx, cy + (bh * 0.5 - 15.0) * s, Look::scaled(ts * s).anchor(0.5, 1.0));
    canvas::draw_rich(canvas, "chatFont", &lines, cx, cy + (5.0 + text_h * 0.5) * s, s, 20.0, 255);
    let w1 = canvas.text_size("goldFont", &a.btn1).0 + 14.0;
    let w2 = canvas.text_size("goldFont", &a.btn2).0 + 14.0;
    let gap = ((300.0 - w1 - w2) * 0.5).min(15.0);
    let total = w1 + w2 + gap;
    let by = cy + (30.0 - bh * 0.5) * s;
    for (id, text, bw, x) in [(Btn::ConfirmNo, &a.btn1, w1, cx - total * 0.5 + w1 * 0.5), (Btn::ConfirmYes, &a.btn2, w2, cx + total * 0.5 - w2 * 0.5)] {
        let f = ctx.factor(id) * s;
        canvas.nine_slice("GJ_button_01.png", x, by, bw * f, 31.5 * f, Look::default());
        canvas.text("goldFont", text, x, by + 2.0 * s, Look::scaled(f));
        ctx.area(id, x, by, bw, 31.5);
    }
}

/// FLAlertLayer::show on what was drawn since `start`, `t` seconds after it opened.
pub fn open_anim(canvas: &mut Canvas, start: usize, t: f32) {
    if t >= 1.5 {
        return; // the elastic has settled
    }
    let (w, h) = (canvas.width, canvas.height);
    let s = 0.1 + 0.9 * canvas::ease_elastic_out((t / 0.5).min(1.0), 0.6);
    let fade = (t / 0.14).min(1.0);
    let n = canvas.draws.len();
    for d in &mut canvas.draws[start.min(n)..] {
        let m = &mut d.instance.m;
        // the backdrop (a CCLayerColor over the whole screen) fades instead of scaling
        if m[0].abs() >= w - 1.0 && m[3].abs() >= h - 1.0 && m[1] == 0.0 && m[2] == 0.0 {
            for v in &mut d.instance.color {
                *v *= fade;
            }
            continue;
        }
        for v in &mut m[..4] {
            *v *= s;
        }
        m[4] = w * 0.5 + (m[4] - w * 0.5) * s;
        m[5] = h * 0.5 + (m[5] - h * 0.5) * s;
    }
}

fn draw_alert(a: &Alert, canvas: &mut Canvas, ctx: &mut Ctx) {
    use canvas::Look;
    let (w, h) = (canvas.width, canvas.height);
    // FLAlertLayer::init (0x3fc534): 300 wide; the text a chatFont TextArea (lines 20 apart,
    // wrapped at (300 − 60) × 1.1); height = text + 120, at least 140; title (goldFont, at most
    // 0.9 and 270 wide) 15 below the top; text centered 5 above the middle; OK 30 above the
    // bottom. The background fades in to 150 over 0.14 s; the box scales in.
    canvas.rect(0.0, 0.0, w, h, [0.0; 3], (a.t / 0.14).min(1.0) * 150.0 / 255.0);
    let s = canvas::ease_elastic_out(a.t / 0.5, 0.6) * 0.3 + 0.7;
    let lines = canvas::rich_lines(canvas, "chatFont", &a.text, 264.0);
    let text_h = lines.len() as f32 * 20.0;
    let bh = (text_h + 120.0).max(140.0);
    let (cx, cy) = (w * 0.5, h * 0.5);
    canvas.nine_slice("square01_001.png", cx, cy, 300.0 * s, bh * s, Look::default());
    let (tw, _) = canvas.text_size("goldFont", &a.title);
    let ts = (270.0 / tw.max(1.0)).clamp(0.4, 0.9);
    canvas.text("goldFont", &a.title, cx, cy + (bh * 0.5 - 15.0) * s, Look::scaled(ts * s).anchor(0.5, 1.0));
    canvas::draw_rich(canvas, "chatFont", &lines, cx, cy + (5.0 + text_h * 0.5) * s, s, 20.0, 255);
    // the OK ButtonSprite scales as one node when pressed
    let f = ctx.factor(Btn::AlertOk) * s;
    let by = cy + (30.0 - bh * 0.5) * s;
    let start = canvas.mark();
    canvas.nine_slice("GJ_button_01.png", cx, by, 60.0, 32.0, Look::default());
    canvas.text("goldFont", "OK", cx, by + 1.0, Look::scaled(0.8));
    canvas.transform_since(start, f, (cx, by), (0.0, 0.0));
    ctx.area(Btn::AlertOk, cx, by, 60.0, 32.0);
}

/// Greedy word wrap at `max` points for chatFont at `scale`.
fn wrap(canvas: &Canvas, text: &str, max: f32, scale: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if canvas.text_size("chatFont", &candidate).0 * scale * 1.6 > max && !line.is_empty() {
            out.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn picker_colors_round_trip() {
        assert_eq!(super::rgb_to_hsv([1.0, 0.0, 0.0]), (0.0, 1.0, 1.0));
        for rgb in [[0.2f32, 0.6, 0.9], [1.0, 1.0, 1.0], [0.5, 0.25, 0.0]] {
            let (h, s, v) = super::rgb_to_hsv(rgb);
            let back = super::hsv_to_rgb(h, s, v);
            assert!(back.iter().zip(rgb).all(|(a, b)| (a - b).abs() < 1e-4), "{rgb:?} -> {back:?}");
        }
    }

    use super::*;

    fn ui() -> Ui {
        Ui::new(Screen::Menu, Vec::new(), Progress::default())
    }

    #[test]
    fn menu_to_level_and_back() {
        let mut ui = ui();
        ui.clicked(Btn::MenuPlay);
        assert_eq!(ui.screen, Screen::Levels);
        ui.clicked(Btn::LevelsNext);
        ui.advance(1.0);
        assert_eq!(ui.levels.page(), 1);
        ui.clicked(Btn::LevelBox);
        assert_eq!(ui.take_actions(), vec![Action::Play(2)]);
        ui.entered_level(2);
        assert!(ui.in_level());
        assert_eq!(ui.progress.get(2).attempts, 1);
        ui.key(winit::keyboard::KeyCode::Escape, true);
        assert!(!ui.level_running());
        assert_eq!(ui.take_actions(), vec![Action::Pause]);
        ui.clicked(Btn::PauseMenu);
        assert_eq!(ui.take_actions(), vec![Action::LeaveLevel]);
        assert_eq!(ui.screen, Screen::Levels);
        assert_eq!(ui.levels.page(), 1);
        ui.clicked(Btn::LevelsPrev);
        ui.clicked(Btn::LevelsPrev);
        ui.advance(1.0);
        assert_eq!(ui.levels.page(), 23, "pages loop");
    }

    #[test]
    fn locked_levels_need_coins() {
        let mut ui = ui();
        ui.levels.jump_to(13); // Clubstep: 10 secret coins
        ui.clicked(Btn::LevelBox);
        assert!(ui.take_actions().is_empty());
        assert!(ui.alert.is_some());
        for id in 1..=4 {
            ui.progress.entry(id).coins = 7;
        }
        ui.alert = None;
        ui.clicked(Btn::LevelBox);
        assert_eq!(ui.take_actions(), vec![Action::Play(14)]);
    }

    #[test]
    fn deaths_attempts_and_completion_are_saved() {
        let mut ui = ui();
        ui.entered_level(1);
        let run = |steps, dead, percent| LevelStatus { dead, percent, steps, coin_count: 3, ..LevelStatus::default() };
        ui.level_frame(run(100, false, 20.0), 0.1);
        ui.level_frame(run(200, true, 43.7), 0.1);
        assert_eq!(ui.progress.get(1).best, 43);
        ui.level_frame(run(0, false, 0.0), 0.1); // respawn: attempt 2
        ui.level_frame(run(50, true, 10.0), 0.1);
        assert_eq!(ui.progress.get(1).best, 43);
        ui.level_frame(run(0, false, 0.0), 0.1); // attempt 3
        assert_eq!(ui.progress.get(1).attempts, 3);
        let mut done = run(900, false, 100.0);
        done.coins = [true, false, true];
        done.end_time = Some(0.0);
        ui.level_frame(done, 0.1);
        assert_eq!(ui.progress.get(1).best, 100);
        assert_eq!(ui.progress.get(1).coins, 0b101);
        assert!(ui.hold_end() && ui.end.is_none());
        done.end_time = Some(end::END_LAYER_AT);
        ui.level_frame(done, 0.1);
        assert!(ui.end.is_some());
        ui.clicked(Btn::EndReplay);
        assert_eq!(ui.take_actions(), vec![Action::Restart]);
        assert!(!ui.hold_end());
    }
}

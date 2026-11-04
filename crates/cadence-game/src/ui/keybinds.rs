//! Keybindings (`KeybindingsLayer`, `KeybindCapturePopup`, `KeybindingsSettingsPopup`;
//! layouts from real captures in shots/gui-real/keys). Every action the game has, its tab and
//! default keys come from `keybinds_table` (dumped from GJKeybindManager); yours are kept in
//! the save (`kb<action>` = chords), the defaults until you change one. A chord is a key code
//! (cocos2d's, Windows' virtual-key numbers) + 0x10000 Ctrl, 0x20000 Alt, 0x40000 Shift.

use super::canvas::{Canvas, Look};
use super::keybinds_table::ACTIONS;
use super::save::Progress;
use super::{Btn, Ctx};
use winit::keyboard::KeyCode as K;

pub const CTRL: u32 = 0x10000;
pub const ALT: u32 = 0x20000;
pub const SHIFT: u32 = 0x40000;

const TABS: [&str; 5] = ["Gameplay", "Practice", "Editor", "Tools", "View"];

/// Gameplay's actions the game and Cadence act on.
pub mod act {
    pub const JUMP_P1: u32 = 10001;
    pub const JUMP_P2: u32 = 10002;
    pub const LEFT_P1: u32 = 10003;
    pub const RIGHT_P1: u32 = 10004;
    pub const LEFT_P2: u32 = 10005;
    pub const RIGHT_P2: u32 = 10006;
    pub const PLACE_CHECKPOINT: u32 = 10007;
    pub const DELETE_CHECKPOINT: u32 = 10008;
    pub const PAUSE: u32 = 10009;
    pub const RESTART: u32 = 10010;
    pub const FULL_RESET: u32 = 10011;
    pub const PRACTICE_HITBOXES: u32 = 10012;
    pub const PRACTICE_MODE: u32 = 10013;
}

/// The key code (cocos2d's `enumKeyCodes`, the Windows virtual keys) of a key.
pub fn code(k: K) -> Option<u32> {
    Some(match k {
        K::Backspace => 8,
        K::Tab => 9,
        K::Enter | K::NumpadEnter => 13,
        K::Pause => 19,
        K::CapsLock => 20,
        K::Escape => 27,
        K::Space => 32,
        K::PageUp => 33,
        K::PageDown => 34,
        K::End => 35,
        K::Home => 36,
        K::ArrowLeft => 37,
        K::ArrowUp => 38,
        K::ArrowRight => 39,
        K::ArrowDown => 40,
        K::Insert => 45,
        K::Delete => 46,
        K::Digit0 => 48,
        K::Digit1 => 49,
        K::Digit2 => 50,
        K::Digit3 => 51,
        K::Digit4 => 52,
        K::Digit5 => 53,
        K::Digit6 => 54,
        K::Digit7 => 55,
        K::Digit8 => 56,
        K::Digit9 => 57,
        K::KeyA => 65,
        K::KeyB => 66,
        K::KeyC => 67,
        K::KeyD => 68,
        K::KeyE => 69,
        K::KeyF => 70,
        K::KeyG => 71,
        K::KeyH => 72,
        K::KeyI => 73,
        K::KeyJ => 74,
        K::KeyK => 75,
        K::KeyL => 76,
        K::KeyM => 77,
        K::KeyN => 78,
        K::KeyO => 79,
        K::KeyP => 80,
        K::KeyQ => 81,
        K::KeyR => 82,
        K::KeyS => 83,
        K::KeyT => 84,
        K::KeyU => 85,
        K::KeyV => 86,
        K::KeyW => 87,
        K::KeyX => 88,
        K::KeyY => 89,
        K::KeyZ => 90,
        K::Numpad0 => 96,
        K::Numpad1 => 97,
        K::Numpad2 => 98,
        K::Numpad3 => 99,
        K::Numpad4 => 100,
        K::Numpad5 => 101,
        K::Numpad6 => 102,
        K::Numpad7 => 103,
        K::Numpad8 => 104,
        K::Numpad9 => 105,
        K::NumpadMultiply => 106,
        K::NumpadAdd => 107,
        K::NumpadSubtract => 109,
        K::NumpadDecimal => 110,
        K::NumpadDivide => 111,
        K::F1 => 112,
        K::F2 => 113,
        K::F3 => 114,
        K::F4 => 115,
        K::F5 => 116,
        K::F6 => 117,
        K::F7 => 118,
        K::F8 => 119,
        K::F9 => 120,
        K::F10 => 121,
        K::F11 => 122,
        K::F12 => 123,
        K::Semicolon => 186,
        K::Equal => 187,
        K::Comma => 188,
        K::Minus => 189,
        K::Period => 190,
        K::Slash => 191,
        K::Backquote => 192,
        K::BracketLeft => 219,
        K::Backslash => 220,
        K::BracketRight => 221,
        K::Quote => 222,
        _ => return None,
    })
}

/// A key's name as `GJKeybindManager::chordToString` writes it.
fn key_name(code: u32) -> String {
    match code {
        8 => "Backspace".into(),
        9 => "Tab".into(),
        13 => "Enter".into(),
        19 => "Pause".into(),
        20 => "CapsLock".into(),
        27 => "Escape".into(),
        32 => "Space".into(),
        33 => "PageUp".into(),
        34 => "PageDown".into(),
        35 => "End".into(),
        36 => "Home".into(),
        37 => "Left".into(),
        38 => "Up".into(),
        39 => "Right".into(),
        40 => "Down".into(),
        45 => "Insert".into(),
        46 => "Delete".into(),
        48..=57 | 65..=90 => char::from_u32(code).map(String::from).unwrap_or_default(),
        96..=105 => format!("Num{}", code - 96),
        106 => "Num*".into(),
        107 => "Num+".into(),
        109 => "Num-".into(),
        110 => "Num.".into(),
        111 => "Num/".into(),
        112..=123 => format!("F{}", code - 111),
        186 => ";".into(),
        187 => "=".into(),
        188 => ",".into(),
        189 => "-".into(),
        190 => ".".into(),
        191 => "/".into(),
        192 => "`".into(),
        219 => "[".into(),
        220 => "\\".into(),
        221 => "]".into(),
        222 => "'".into(),
        _ => "?".into(),
    }
}

pub fn chord_name(chord: u32) -> String {
    let mut s = String::new();
    for (bit, name) in [(CTRL, "Ctrl+"), (ALT, "Alt+"), (SHIFT, "Shift+")] {
        if chord & bit != 0 {
            s.push_str(name);
        }
    }
    s.push_str(&key_name(chord & 0xffff));
    s
}

pub fn chord(k: K, ctrl: bool, alt: bool, shift: bool) -> Option<u32> {
    code(k).map(|c| c | if ctrl { CTRL } else { 0 } | if alt { ALT } else { 0 } | if shift { SHIFT } else { 0 })
}

fn defaults(action: u32) -> &'static [u32] {
    ACTIONS.iter().find(|a| a.0 == action).map_or(&[], |a| a.3)
}

/// An action's chords: yours, or the game's defaults.
pub fn chords(p: &Progress, action: u32) -> Vec<u32> {
    match p.settings.get(&format!("kb{action}")) {
        Some(v) => v.split(',').filter_map(|c| c.parse().ok()).collect(),
        None => defaults(action).to_vec(),
    }
}

fn set_chords(p: &mut Progress, action: u32, v: &[u32]) {
    let text = v.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    p.settings.insert(format!("kb{action}"), text);
}

/// The tab of an action (0 Gameplay, 1 Practice: playing; 2–4: the editor).
fn tab(action: u32) -> u8 {
    ACTIONS.iter().find(|a| a.0 == action).map_or(0, |a| a.1)
}

/// The actions a chord triggers: while playing (`editor` false) the Gameplay and Practice
/// ones, in the editor the Editor, Tools and View ones (`GJKeybindManager::groupsConflict`).
pub fn actions_for(p: &Progress, chord: u32, editor: bool) -> Vec<u32> {
    ACTIONS.iter().filter(|a| (a.1 >= 2) == editor && chords(p, a.0).contains(&chord)).map(|a| a.0).collect()
}

/// Whether a key (any modifiers) is bound to `action` — for releases of held keys.
pub fn key_held_for(p: &Progress, key: u32, action: u32) -> bool {
    chords(p, action).iter().any(|c| c & 0xffff == key)
}

/// The layer's state: its tab (5 search), the list's scroll, the search text, the binding
/// being edited (action, the chord it replaces, the chord pressed), the settings popup.
#[derive(Clone, Debug, Default)]
pub struct Keys {
    pub tab: u8,
    pub scroll: f32,
    pub search: String,
    pub editing: Option<(u32, Option<u32>, Option<u32>)>,
    pub settings: bool,
}

const ROW: f32 = 32.0;
const TOP: f32 = 241.0;
const VISIBLE: (f32, f32) = (24.0, 256.0);

impl Keys {
    /// `actionsToShow`: a tab's actions, or the search's matches (by name or key).
    pub fn rows(&self, p: &Progress) -> Vec<u32> {
        if self.tab == 5 {
            let q = self.search.to_lowercase();
            return ACTIONS
                .iter()
                .filter(|a| !q.is_empty() && (a.2.to_lowercase().contains(&q) || chords(p, a.0).iter().any(|c| chord_name(*c).to_lowercase().contains(&q))))
                .map(|a| a.0)
                .collect();
        }
        if self.tab == 6 {
            return Vec::new();
        }
        ACTIONS.iter().filter(|a| a.1 == self.tab).map(|a| a.0).collect()
    }

    pub fn max_scroll(&self, p: &Progress) -> f32 {
        (self.rows(p).len() as f32 * ROW - (VISIBLE.1 - VISIBLE.0)).max(0.0)
    }

    pub fn wheel(&mut self, d: f32, p: &Progress) {
        self.scroll = (self.scroll - d).clamp(0.0, self.max_scroll(p));
    }

    /// A key pressed while the capture popup is up: the chord to set.
    pub fn capture(&mut self, k: K, ctrl: bool, alt: bool, shift: bool) -> bool {
        let Some((action, old, _)) = self.editing else { return false };
        // the modifiers alone aren't a chord
        if matches!(k, K::ShiftLeft | K::ShiftRight | K::ControlLeft | K::ControlRight | K::AltLeft | K::AltRight | K::SuperLeft | K::SuperRight) {
            return true;
        }
        if let Some(c) = chord(k, ctrl, alt, shift) {
            self.editing = Some((action, old, Some(c)));
        }
        true
    }

    pub fn click(&mut self, id: Btn, p: &mut Progress) -> Option<String> {
        match id {
            Btn::KeysTab(t) => {
                self.tab = t;
                self.scroll = 0.0;
            }
            Btn::KeysAdd(i) => {
                let rows = self.rows(p);
                if let Some(&a) = rows.get(i as usize) {
                    self.editing = Some((a, None, None));
                }
            }
            Btn::KeysChord(i, k) => {
                let rows = self.rows(p);
                if let Some(&a) = rows.get(i as usize)
                    && let Some(&c) = chords(p, a).get(k as usize)
                {
                    self.editing = Some((a, Some(c), Some(c)));
                }
            }
            Btn::KeysSet => {
                if let Some((a, old, Some(new))) = self.editing.take() {
                    let mut v = chords(p, a);
                    v.retain(|c| Some(*c) != old && *c != new);
                    v.push(new);
                    set_chords(p, a, &v);
                    // a key is one action's within its group: it leaves the others
                    for other in ACTIONS.iter().filter(|o| o.0 != a && (o.1 >= 2) == (tab(a) >= 2)) {
                        let mut w = chords(p, other.0);
                        if w.contains(&new) {
                            w.retain(|c| *c != new);
                            set_chords(p, other.0, &w);
                        }
                    }
                    p.save();
                }
            }
            Btn::KeysRemove => {
                if let Some((a, Some(old), _)) = self.editing.take() {
                    let mut v = chords(p, a);
                    v.retain(|c| *c != old);
                    set_chords(p, a, &v);
                    p.save();
                }
            }
            Btn::KeysEditClose => self.editing = None,
            Btn::KeysSettings => self.settings = true,
            Btn::KeysSettingsClose => self.settings = false,
            Btn::KeysReset => {
                p.settings.retain(|k, _| !(k.starts_with("kb") && k[2..].bytes().all(|b| b.is_ascii_digit())));
                p.save();
                self.settings = false;
                return Some("Keybinds reset to the defaults.".into());
            }
            Btn::KeysExport => {
                self.settings = false;
                return Some(match export(p) {
                    Ok(where_) => format!("Keybinds exported to <cy>{where_}</c>."),
                    Err(e) => format!("Export failed: {e}"),
                });
            }
            Btn::KeysImport => {
                self.settings = false;
                return Some(match import(p) {
                    Ok(n) => format!("Imported <cg>{n}</c> keybinds."),
                    Err(e) => format!("Import failed: {e}"),
                });
            }
            _ => {}
        }
        None
    }
}

/// The binds as text: one action a line, `action=chord,chord`.
pub fn to_text(p: &Progress) -> String {
    ACTIONS.iter().map(|a| format!("{}={}", a.0, chords(p, a.0).iter().map(u32::to_string).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join("\n")
}

pub fn from_text(p: &mut Progress, text: &str) -> usize {
    let mut n = 0;
    for line in text.lines() {
        let Some((a, v)) = line.split_once('=') else { continue };
        let Ok(a) = a.trim().parse::<u32>() else { continue };
        if !ACTIONS.iter().any(|x| x.0 == a) {
            continue;
        }
        let v: Vec<u32> = v.split(',').filter_map(|c| c.trim().parse().ok()).collect();
        set_chords(p, a, &v);
        n += 1;
    }
    n
}

#[cfg(not(target_arch = "wasm32"))]
fn binds_file() -> std::path::PathBuf {
    crate::online::cache_dir().with_file_name("keybinds.txt")
}

#[cfg(not(target_arch = "wasm32"))]
fn export(p: &Progress) -> Result<String, String> {
    let f = binds_file();
    std::fs::write(&f, to_text(p)).map_err(|e| e.to_string())?;
    Ok(f.display().to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn import(p: &mut Progress) -> Result<usize, String> {
    let text = std::fs::read_to_string(binds_file()).map_err(|e| e.to_string())?;
    let n = from_text(p, &text);
    p.save();
    Ok(n)
}

#[cfg(target_arch = "wasm32")]
fn export(p: &Progress) -> Result<String, String> {
    crate::audio::web_download("keybinds.txt", to_text(p).as_bytes());
    Ok("keybinds.txt".into())
}

#[cfg(target_arch = "wasm32")]
fn import(p: &mut Progress) -> Result<usize, String> {
    let text = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item("cadence.keybinds").ok().flatten()).ok_or("drop a keybinds.txt on the page first")?;
    let n = from_text(p, &text);
    p.save();
    Ok(n)
}

/// A chord's button (`ButtonSprite` on GJ_button_04): the label at 0.4 (a hair less when it
/// just overflows 40, as "Space" does), at least 38 wide.
fn chord_button(c: &mut Canvas, ctx: &mut Ctx, id: Btn, text: &str, right: f32, y: f32) -> f32 {
    let (tw, _) = c.text_size("bigFont", text);
    let s = if (40.0..44.0).contains(&(tw * 0.4)) { 40.0 / tw } else { 0.4 };
    let w = (tw * s + 16.0).max(38.0);
    let x = right - w * 0.5;
    let f = ctx.factor(id);
    c.nine_slice("GJ_button_04.png", x, y, w * f, 22.0 * f, Look::default());
    c.text("bigFont", text, x - 1.0, y + 2.0, Look::scaled(s * f));
    ctx.area(id, x, y, w, 22.0);
    w
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, k: &Keys, p: &Progress) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    let at = |x: f32| x + dx;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square07.png", at(284.5), 160.0, 400.0, 280.0, Look::default());
    c.nine_slice("GJ_square06.png", at(284.5), 160.0, 400.0, 280.0, Look::default().rgb([80, 87, 105]));
    // the rows, clipped to the list's box
    let rows = k.rows(p);
    for (i, &a) in rows.iter().enumerate() {
        let y = TOP - ROW * i as f32 + k.scroll;
        if y < VISIBLE.0 || y > VISIBLE.1 {
            continue;
        }
        let name = ACTIONS.iter().find(|x| x.0 == a).map_or("", |x| x.2);
        c.nine_slice("square02b_001.png", at(284.5), y, 370.0, 30.0, Look::default().rgb([0, 0, 0]).alpha(70));
        c.text("bigFont", name, at(109.5), y + 1.0, Look::scaled(0.35).anchor(0.0, 0.5));
        let mut right = at(459.5);
        let f = ctx.factor(Btn::KeysAdd(i as u8));
        c.nine_slice("GJ_button_01.png", right - 13.0, y, 26.0 * f, 22.0 * f, Look::default());
        c.text("bigFont", "+", right - 14.0, y + 2.0, Look::scaled(0.4878 * f));
        ctx.area(Btn::KeysAdd(i as u8), right - 13.0, y, 26.0, 22.0);
        right -= 30.0;
        let list = chords(p, a);
        for (j, ch) in list.iter().enumerate().rev() {
            let bw = chord_button(c, ctx, Btn::KeysChord(i as u8, j as u8), &chord_name(*ch), right, y);
            right -= bw + 4.0;
        }
    }
    // the scrollbar
    let max = k.max_scroll(p);
    if max > 0.0 {
        let track = VISIBLE.1 - VISIBLE.0;
        let bar = track * track / (track + max);
        let y = VISIBLE.1 - bar * 0.5 - (track - bar) * (k.scroll / max);
        c.rect(at(472.5), y - bar * 0.5, 3.0, bar, [0.8; 3], 0.6);
    }
    // the tabs: Gameplay … View, search, Mods, the gear
    const TABX: [(f32, f32); 5] = [(150.15, 59.09), (212.12, 56.83), (267.49, 45.92), (315.82, 42.73), (359.63, 36.88)];
    for (t, &(x, tw)) in TABX.iter().enumerate() {
        let id = Btn::KeysTab(t as u8);
        let f = ctx.factor(id);
        c.nine_slice(if k.tab == t as u8 { "GJ_button_02.png" } else { "GJ_button_05.png" }, at(x), 280.0, tw * f, 25.0 * f, Look::default());
        c.text("bigFont", TABS[t], at(x + 0.3), 282.0, Look::scaled(0.3059 * f));
        ctx.area(id, at(x), 280.0, tw, 25.0);
    }
    for (t, x, tw) in [(5u8, 393.53, 22.92), (6, 428.69, 39.41)] {
        let id = Btn::KeysTab(t);
        let f = ctx.factor(id);
        c.nine_slice(if k.tab == t { "GJ_button_02.png" } else { "GJ_button_05.png" }, at(x), 280.0, tw * f, 25.0 * f, Look::default());
        if t == 5 {
            c.sprite("searchIcon_001.png", at(x), 281.0, Look::scaled(0.7 * f));
        } else {
            c.text("bigFont", "Mods", at(x + 0.3), 282.0, Look::scaled(0.3059 * f));
        }
        ctx.area(id, at(x), 280.0, tw, 25.0);
    }
    if k.tab == 5 {
        let text = if k.search.is_empty() { "Search..." } else { k.search.as_str() };
        c.text("bigFont", text, at(284.5), 262.0, Look::scaled(0.4).alpha(if k.search.is_empty() { 120 } else { 255 }));
    }
    ctx.button(c, Btn::KeysSettings, "GJ_optionsBtn02_001.png", at(464.5), 280.0, Look::scaled(0.7), 1.3);
    ctx.button(c, Btn::KeysClose, "GJ_closeBtn_001.png", at(84.5), 300.0, Look::scaled(0.7), 1.5);
    if let Some((a, old, new)) = k.editing {
        ctx.buttons.clear();
        draw_capture(c, ctx, a, old, new);
    } else if k.settings {
        ctx.buttons.clear();
        draw_settings(c, ctx);
    }
}

/// `KeybindCapturePopup`: "Edit Keybind", the action, the chord pressed (or
/// "Ctrl+Alt+Shift+?" before one), "Press Keys...", Set and Remove.
fn draw_capture(c: &mut Canvas, ctx: &mut Ctx, action: u32, old: Option<u32>, new: Option<u32>) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
    c.nine_slice("GJ_square07.png", cx, cy, 300.0, 240.0, Look::default());
    c.nine_slice("GJ_square06.png", cx, cy, 300.0, 240.0, Look::default().rgb([80, 87, 105]));
    c.text("bigFont", "Edit Keybind", cx, cy + 106.0, Look::scaled(0.5));
    let name = ACTIONS.iter().find(|x| x.0 == action).map_or("", |x| x.2);
    c.text("bigFont", name, cx, cy + 70.0, Look::scaled(0.65).rgb([100, 200, 255]));
    let shown = new.map_or("Ctrl+Alt+Shift+?".to_string(), chord_name);
    let (tw, _) = c.text_size("goldFont", &shown);
    c.text("goldFont", &shown, cx, cy + 30.0, Look::scaled((260.0 / tw.max(1.0)).min(0.9)));
    c.text("bigFont", "Press Keys...", cx, cy - 4.0, Look::scaled(0.5).alpha(150));
    for (id, frame, label, x, bw, s) in [(Btn::KeysSet, "GJ_button_01.png", "Set", cx - 55.0, 76.0, 0.7), (Btn::KeysRemove, "GJ_button_06.png", "Remove", cx + 55.0, 96.0, 0.64)] {
        let enabled = if id == Btn::KeysSet { new.is_some() } else { old.is_some() };
        let f = ctx.factor(id);
        let look = if enabled { Look::default() } else { Look::default().rgb([128, 128, 128]) };
        c.nine_slice(frame, x, cy - 94.0, bw * f, 28.0 * f, look);
        c.text("bigFont", label, x - 1.0, cy - 92.0, Look::scaled(s * f).rgb(if enabled { [255, 255, 255] } else { [128, 128, 128] }));
        if enabled {
            ctx.area(id, x, cy - 94.0, bw, 28.0);
        }
    }
    ctx.button(c, Btn::KeysEditClose, "GJ_closeBtn_001.png", cx - 150.0, cy + 120.0, Look::scaled(0.7), 1.5);
}

/// `KeybindingsSettingsPopup`: Export, Import, Reset.
fn draw_settings(c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
    c.nine_slice("GJ_square01.png", cx, cy, 260.0, 180.0, Look::default());
    c.text("goldFont", "Keybind Settings", cx, cy + 70.0, Look::scaled(0.7));
    for (id, frame, label, y) in [(Btn::KeysExport, "GJ_button_02.png", "Export", 28.0), (Btn::KeysImport, "GJ_button_01.png", "Import", -10.0), (Btn::KeysReset, "GJ_button_06.png", "Reset", -48.0)] {
        let f = ctx.factor(id);
        c.nine_slice(frame, cx, cy + y, 146.0 * f, 28.0 * f, Look::default());
        c.text("bigFont", label, cx - 1.0, cy + y + 2.0, Look::scaled(0.7 * f));
        ctx.area(id, cx, cy + y, 146.0, 28.0);
    }
    ctx.button(c, Btn::KeysSettingsClose, "GJ_closeBtn_001.png", cx - 120.0, cy + 80.0, Look::default(), 1.5);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_like_the_game() {
        let mut p = Progress::default();
        assert_eq!(chord_name(65618), "Ctrl+R");
        assert_eq!(chord_name(327770), "Ctrl+Shift+Z");
        assert_eq!(chord_name(262331), "Shift+=");
        assert_eq!(actions_for(&p, 32, false), [act::JUMP_P1]);
        assert_eq!(actions_for(&p, 32, true), [20069]);
        // rebinding Jump P1's Space to J; J leaves nothing else bound
        let mut k = Keys::default();
        k.editing = Some((act::JUMP_P1, Some(32), Some(32)));
        assert!(k.capture(K::KeyJ, false, false, false));
        k.click(Btn::KeysSet, &mut p);
        assert_eq!(chords(&p, act::JUMP_P1), [87, 74]);
        assert!(actions_for(&p, 32, false).is_empty());
        // a key moves from the action that had it
        k.editing = Some((act::RESTART, None, Some(74)));
        k.click(Btn::KeysSet, &mut p);
        assert_eq!(chords(&p, act::JUMP_P1), [87]);
        let text = to_text(&p);
        let mut q = Progress::default();
        assert_eq!(from_text(&mut q, &text), ACTIONS.len());
        assert_eq!(chords(&q, act::RESTART), [82, 74]);
    }
}

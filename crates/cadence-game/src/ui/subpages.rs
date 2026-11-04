//! The Settings dropdown's other pages, each a `GJDropDownLayer` like Settings itself:
//! `AccountLayer` (0x8a2a0), `SupportLayer` (0x30d630) and `SongsLayer` (0x46dd7c).
//! Positions are from the real 1280 × 720 frames (shots/gui-real/settings/opt*.png).

use super::canvas::{Canvas, Look};
use super::options::{self, LIST_H};
use super::save::Progress;
use super::{Btn, Ctx};

/// Which page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sub {
    Account,
    Support,
    Songs,
}

impl Sub {
    pub fn title(self) -> &'static str {
        match self {
            Sub::Account => "Account",
            Sub::Support => "Support",
            Sub::Songs => "Soundtrack",
        }
    }
}

/// `ButtonSprite` with a fixed size: `bg` nine-slice, `font` label at `scale`. A greyed button
/// (a server feature Cadence leaves out) is drawn gray and has no hit area.
#[allow(clippy::too_many_arguments)]
pub fn button_sprite(c: &mut Canvas, ctx: &mut Ctx, id: Btn, text: &str, font: &str, bg: &str, x: f32, y: f32, w: f32, h: f32, scale: f32, grey: bool) {
    let f = if grey { 1.0 } else { ctx.factor(id) };
    let look = if grey { Look::default().gray() } else { Look::default() };
    c.nine_slice(bg, x, y, w * f, h * f, look);
    let tlook = if grey { Look::scaled(scale * f).gray() } else { Look::scaled(scale * f) };
    c.text(font, text, x, y + 2.0 * f, tlook);
    if !grey {
        ctx.area(id, x, y, w, h);
    }
}

/// The official songs in the soundtrack list (`SongsLayer::customSetup` adds `SongObject`s 0..21
/// and −1, the practice song): title and artist (`LevelTools::getAudioTitle` / `nameForArtist`).
pub const SONGS: [(&str, &str); 23] = [
    ("Stereo Madness", "ForeverBound"),
    ("Back On Track", "DJVI"),
    ("Polargeist", "Step"),
    ("Dry Out", "DJVI"),
    ("Base After Base", "DJVI"),
    ("Cant Let Go", "DJVI"),
    ("Jumper", "Waterflame"),
    ("Time Machine", "Waterflame"),
    ("Cycles", "DJVI"),
    ("xStep", "DJVI"),
    ("Clutterfunk", "Waterflame"),
    ("Theory of Everything", "DJ-Nate"),
    ("Electroman Adventures", "Waterflame"),
    ("Clubstep", "DJ-Nate"),
    ("Electrodynamix", "DJ-Nate"),
    ("Hexagon Force", "Waterflame"),
    ("Blast Processing", "Waterflame"),
    ("Theory of Everything 2", "DJ-Nate"),
    ("Geometrical Dominator", "Waterflame"),
    ("Deadlocked", "F-777"),
    ("Fingerdash", "MDK"),
    ("Dash", "MDK"),
    ("Practice: Stay Inside Me", "OcularNebula"),
];

/// The song list's rows: 60 high, the first 11 above the list's top (under the title bar).
const ROW_H: f32 = 60.0;

pub fn max_scroll(sub: Sub) -> f32 {
    match sub {
        Sub::Songs => (SONGS.len() as f32 * ROW_H - 10.0 - LIST_H).max(0.0),
        _ => 0.0,
    }
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, p: &Progress, sub: Sub, t: f32, closing: bool, scroll: f32) {
    let (w, h) = (c.width, c.height);
    let dy = options::dropdown_frame(c, ctx, p, t, closing, sub.title());
    let (cx, cy) = (w * 0.5, h * 0.5 + dy);
    let (lx, ly) = ((w - 356.0) * 0.5, (h - LIST_H) * 0.5 - 5.0 + dy);
    match sub {
        Sub::Account => {
            // `AccountLayer::updatePage`: no account; an account without its GJP2 ("Login
            // required"); or logged in (Save, Load, More instead of Log in, Help, Register).
            let account = crate::gjapi::linked_account();
            match &account {
                None => {
                    c.text("goldFont", "Not logged in", cx, cy + 87.0, Look::scaled(0.8));
                    c.text("bigFont", "Create an account to back up", cx, cy + 70.3, Look::scaled(0.4));
                    c.text("bigFont", "and load your data from the cloud", cx, cy + 56.3, Look::scaled(0.4));
                }
                Some((name, logged_in)) => {
                    let title = format!("Linked to account: {name}");
                    let (tw, _) = c.text_size("goldFont", &title);
                    c.text("goldFont", &title, cx, cy + 87.0, Look::scaled((330.0 / tw.max(1.0)).min(0.8)));
                    if !logged_in {
                        c.text("bigFont", "Login required", cx, cy + 63.3, Look::scaled(0.4));
                    }
                }
            }
            if matches!(account, Some((_, true))) {
                // Save would replace the account's cloud backup with Cadence's progress, which
                // isn't in the game's save format yet: greyed, like Load, until it is.
                button_sprite(c, ctx, Btn::AccSave, "Save", "goldFont", "GJ_button_01.png", cx, cy + 30.0, 176.0, 31.5, 1.0, true);
                button_sprite(c, ctx, Btn::AccLoad, "Load", "goldFont", "GJ_button_01.png", cx, cy - 15.0, 176.0, 31.5, 1.0, true);
                button_sprite(c, ctx, Btn::AccMore, "More", "goldFont", "GJ_button_01.png", cx, cy - 60.0, 176.0, 31.5, 1.0, false);
            } else {
                button_sprite(c, ctx, Btn::AccLogin, "Log in", "goldFont", "GJ_button_01.png", cx, cy + 30.0, 176.0, 31.5, 1.0, false);
                button_sprite(c, ctx, Btn::AccHelp, "Help", "goldFont", "GJ_button_01.png", cx, cy - 15.0, 176.0, 31.5, 1.0, false);
                // Registering is done on RobTop's site for now.
                button_sprite(c, ctx, Btn::AccRegister, "Register", "goldFont", "GJ_button_01.png", cx, cy - 60.0, 176.0, 31.5, 1.0, true);
            }
        }
        Sub::Support => {
            for (id, text, x, y, grey) in [
                (Btn::SupLinks, "Links", -100.0, 80.0, false),
                (Btn::SupContact, "Contact", 0.0, 80.0, true),
                (Btn::SupSfx, "SFX", 100.0, 80.0, false),
                (Btn::SupPrivacy, "Privacy", -50.0, 40.0, false),
                (Btn::SupTos, "ToS", 50.0, 40.0, false),
            ] {
                button_sprite(c, ctx, id, text, "bigFont", "GJ_button_01.png", cx + x, cy + y, 86.0, 30.0, 0.5, grey);
            }
            c.sprite("developedBy_001.png", cx - 55.0, cy, Look::default());
            ctx.button(c, Btn::SupRobTop, "robtoplogo_small.png", cx + 45.0, cy, Look::default(), 1.2);
            c.sprite("poweredBy_001.png", cx - 55.0, cy - 35.0, Look::default());
            ctx.button(c, Btn::SupCocos, "cocos2DxLogo.png", cx + 45.0, cy - 35.0, Look::default(), 1.2);
            c.text("goldFont", "FMOD Studio by Firelight Technologies Pty Ltd.", cx, cy - 61.0, Look::scaled(0.35));
            let on = p.var("lowdetail", false);
            let f = ctx.factor(Btn::SupLowDetail);
            c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, cx - 160.0, cy - 90.0, Look::scaled(0.8 * f));
            ctx.area(Btn::SupLowDetail, cx - 160.0, cy - 90.0, 37.0, 37.0);
            c.text("bigFont", "Low detail mode", cx - 138.0, cy - 90.0, Look::scaled(0.35).anchor(0.0, 0.5));
        }
        Sub::Songs => {
            let top = ly + LIST_H + 11.0;
            for (i, (title, artist)) in SONGS.iter().enumerate() {
                let y = top - ROW_H * (i as f32 + 1.0) + scroll;
                if y + ROW_H <= ly || y >= ly + LIST_H {
                    continue;
                }
                let (y0, y1) = (y.max(ly), (y + ROW_H).min(ly + LIST_H));
                let shade = if i % 2 == 0 { [141.0, 77.0, 39.0] } else { [173.0, 101.0, 56.0] };
                c.rect(lx, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
                let my = y + ROW_H * 0.5;
                if my > ly + 8.0 && my < ly + LIST_H - 8.0 {
                    let (tw, _) = c.text_size("bigFont", title);
                    let s = (240.0 / tw.max(1.0)).min(0.6);
                    c.text("bigFont", title, lx + 11.5, my + 9.0, Look::scaled(s).anchor(0.0, 0.5));
                    c.text("goldFont", &format!("By {artist}"), lx + 11.5, my - 13.0, Look::scaled(0.5).anchor(0.0, 0.5));
                    button_sprite(c, ctx, Btn::SongView(i as u8), "View", "bigFont", "GJ_button_01.png", lx + 316.0, my, 62.0, 30.0, 0.6, false);
                }
            }
        }
    }
    options::dropdown_border(c, ctx, dy, sub.title());
}

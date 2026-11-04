//! `AccountLoginLayer` (init 0x3f9aec): the login popup the account page's Log in opens.
//!
//! Layout from the decompile and its disassembly: a 150-alpha black layer, GJ_square01 400 × 300
//! at the center, "Login" (bigFont) 134 above the center; two fields of square02_small 300 × 30
//! (opacity 100) at +80 and +30, each with its label (bigFont 0.4, bottom left at the field's top
//! left + 1) and a text input (placeholder 0.5 in 120,170,240, text at most 0.8); "Forgot
//! Username" / "Forgot Password" (goldFont 0.8 on GJ_button_05, 120 × 25) 15 apart at −45;
//! Cancel and Login (GJ_button_01) 15 apart at −128. While the request runs, the fields and
//! buttons hide and a loading circle spins.

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx};

/// The username field's characters (`CCTextInputNode::setAllowedChars`).
const USER_CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const MAX_LEN: usize = 20;

pub struct LoginPopup {
    pub user: String,
    pub pass: String,
    /// The field typed into: 0 the username, 1 the password.
    pub focus: u8,
    /// Each field's label: an error in red instead of "Username:" / "Password:".
    pub errors: [Option<&'static str>; 2],
    /// A field was clicked or typed in (the text cursor shows from then on, like a
    /// `CCTextInputNode` once it's touched).
    pub active: bool,
    /// The request in flight (`showLoadingUI`); seconds it has run, for the circle.
    pub job: Option<std::sync::mpsc::Receiver<Result<crate::gjapi::Account, i32>>>,
    pub t: f32,
}

impl LoginPopup {
    pub fn new() -> LoginPopup {
        LoginPopup { user: String::new(), pass: String::new(), focus: 0, errors: [None; 2], active: false, job: None, t: 0.0 }
    }

    pub fn loading(&self) -> bool {
        self.job.is_some()
    }

    /// Typed text into the focused field (the username takes only its allowed characters).
    pub fn type_text(&mut self, text: &str) {
        if self.loading() {
            return;
        }
        self.active = true;
        let field = if self.focus == 0 { &mut self.user } else { &mut self.pass };
        for ch in text.chars() {
            let ok = if self.focus == 0 { USER_CHARS.contains(ch) } else { !ch.is_control() };
            if ok && field.chars().count() < MAX_LEN {
                field.push(ch);
            }
        }
    }

    pub fn backspace(&mut self) {
        if !self.loading() {
            if self.focus == 0 { self.user.pop() } else { self.pass.pop() };
        }
    }

    /// `onSubmit`: labels reset; a username under 3 or a password under 6 characters is refused
    /// in the field's label; else the login request starts.
    pub fn submit(&mut self) {
        if self.loading() {
            return;
        }
        self.errors = [None; 2];
        if self.user.trim().chars().count() < 3 {
            self.errors[0] = Some("Too short. Minimum 3 characters");
        }
        if self.pass.chars().count() < 6 {
            self.errors[1] = Some("Too short. Minimum 6 characters");
        }
        if self.errors.iter().any(Option::is_some) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (user, pass) = (self.user.trim().to_string(), self.pass.clone());
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let _ = tx.send(crate::gjapi::login(&user, &pass));
        });
        // The browser can't reach the servers yet (no relay): fail like an unreachable server.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (user, pass);
            let _ = tx.send(Err(-1));
        }
        self.job = Some(rx);
        self.t = 0.0;
    }

    /// `loginAccountFailed`: the code's text in its field's label (a disabled account also
    /// says why in the password's).
    pub fn failed(&mut self, code: i32) {
        self.job = None;
        let (field, text) = crate::gjapi::login_error(code);
        self.errors[field as usize] = Some(text);
        if code == -12 {
            self.errors[1] = Some("Contact RobTop Support for more info");
        }
    }
}

/// A `ButtonSprite` sized to its label (goldFont 1, GJ_button_01, 16 wider, 31.5 tall).
fn button(c: &mut Canvas, ctx: &mut Ctx, id: Btn, text: &str, x: f32, y: f32) {
    let f = ctx.factor(id);
    let (tw, _) = c.text_size("goldFont", text);
    let w = tw + 16.0;
    c.nine_slice("GJ_button_01.png", x, y, w * f, 31.5 * f, Look::default());
    c.text("goldFont", text, x, y + 2.0 * f, Look::scaled(f));
    ctx.area(id, x, y, w, 31.5);
}

pub fn button_width(c: &Canvas, text: &str) -> f32 {
    c.text_size("goldFont", text).0 + 16.0
}

pub fn draw(l: &LoginPopup, c: &mut Canvas, ctx: &mut Ctx, clock: f32) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square01.png", cx, cy, 400.0, 300.0, Look::default());
    c.text("bigFont", "Login", cx, cy + 134.0, Look::default());
    if l.loading() {
        super::online::loading_circle(c, cx, cy, l.t);
        return;
    }
    let fields = [("Username:", "Username", &l.user, Btn::LoginUser), ("Password:", "Password", &l.pass, Btn::LoginPass)];
    for (i, (label, placeholder, value, id)) in fields.into_iter().enumerate() {
        let fy = cy + 80.0 - 50.0 * i as f32;
        c.nine_slice("square02_small.png", cx, fy, 300.0, 30.0, Look::default().alpha(100));
        ctx.area(id, cx, fy, 300.0, 40.0);
        let (text, color) = match l.errors[i] {
            Some(e) => (e, [255, 90, 90]),
            None => (label, [255, 255, 255]),
        };
        c.text("bigFont", text, cx - 150.0, fy + 16.0, Look::scaled(0.4).anchor(0.0, 0.0).rgb(color));
        let shown = if i == 1 { "*".repeat(value.chars().count()) } else { value.clone() };
        let focused = l.focus as usize == i;
        if shown.is_empty() {
            c.text("bigFont", placeholder, cx, fy, Look::scaled(0.5).rgb([120, 170, 240]));
        } else {
            let (tw, _) = c.text_size("bigFont", &shown);
            let s = (280.0 / tw.max(1.0)).min(0.8);
            c.text("bigFont", &shown, cx, fy, Look::scaled(s));
        }
        // the text cursor (`CCTextInputNode`'s blinking caret) in the focused field
        if focused && l.active && (clock * 2.0).fract() < 0.5 {
            let s = if shown.is_empty() { 0.0 } else { (280.0 / c.text_size("bigFont", &shown).0.max(1.0)).min(0.8) };
            let half = c.text_size("bigFont", &shown).0 * s * 0.5;
            c.rect(cx + half + 2.0, fy - 9.0, 2.0, 18.0, [1.0; 3], 1.0);
        }
    }
    // Forgot Username / Forgot Password: GJ_button_05, goldFont 0.8, 120 × 25, 15 apart
    for (i, (id, text)) in [(Btn::LoginForgotUser, "Forgot Username"), (Btn::LoginForgotPass, "Forgot Password")].into_iter().enumerate() {
        let x = cx + (i as f32 - 0.5) * (120.0 + 15.0);
        let f = ctx.factor(id);
        c.nine_slice("GJ_button_05.png", x, cy - 45.0, 120.0 * f, 25.0 * f, Look::default());
        let (tw, _) = c.text_size("goldFont", text);
        c.text("goldFont", text, x, cy - 45.0 + 1.0 * f, Look::scaled((104.0 / tw.max(1.0)).min(0.8) * f));
        ctx.area(id, x, cy - 45.0, 120.0, 25.0);
    }
    // Cancel and Login, 15 apart
    let (w1, w2) = (button_width(c, "Cancel"), button_width(c, "Login"));
    let total = w1 + 15.0 + w2;
    button(c, ctx, Btn::LoginCancel, "Cancel", cx - total * 0.5 + w1 * 0.5, cy - 128.0);
    button(c, ctx, Btn::LoginSubmit, "Login", cx + total * 0.5 - w2 * 0.5, cy - 128.0);
}

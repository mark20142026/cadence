//! `AccountRegisterLayer` (init 0x3f56d4): the Account page's Register popup.
//!
//! Layout from the real popup's scene dump: a 150-alpha black layer, GJ_square01 400 × 300,
//! "Register Account" (bigFont 0.6) 134 above the center; five fields of square02_small
//! 300 × 30 (opacity 100) at +90, +45, 0, −45, −90, each with its label (bigFont 0.4, bottom
//! left 1 above the field's top left) and a text input (placeholder 0.5 in 120,170,240);
//! Cancel and Submit (goldFont on GJ_button_01) 15 apart at −128.
//!
//! Opening it tells the servers (`accounts/openedRegister.php`, `secret=…`). Submit
//! (`onSubmit` 0x3f6678) checks the fields in the game's order, then sends
//! `accounts/registerGJAccount.php`: `userName=%s&password=%s&email=%s&secret=%s&rsession=%s`
//! (the game's rsession comes out empty; recorded). The password goes to the servers as the
//! game sends it and is never kept.

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx};

const ACCOUNT_SECRET: &str = "Wmfv3899gc9";
const USER_CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const EMAIL_CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_@.";
const MAX_USER: usize = 20;
const MAX_FIELD: usize = 32;

/// The labels, placeholders and the field each is.
const FIELDS: [(&str, &str); 5] = [
    ("Username: (shown to other players)", "Username"),
    ("Password:", "Password"),
    ("Confirm Password:", "Confirm Password"),
    ("Email:", "Email"),
    ("Verify Email:", "Verify Email"),
];

pub struct RegisterPopup {
    pub values: [String; 5],
    pub focus: u8,
    /// A field's label replaced by an error (in red).
    pub errors: [Option<&'static str>; 5],
    pub active: bool,
    pub job: Option<std::sync::mpsc::Receiver<Result<(), i32>>>,
    pub t: f32,
}

/// The common passwords the game refuses ("Too common"): its list, checked after folding case,
/// trimming non-letters from both ends, undoing digit-for-letter swaps and repeats.
const COMMON: &[&str] = &[
    "password", "passwords", "passwd", "pass", "root", "admin", "administrator", "user", "guest", "login", "logon", "welcome", "letmein", "changeme",
    "default", "secret", "access", "master", "manager", "system", "service", "test", "demo", "qwerty", "qwertyuiop", "qwertz", "azerty", "asdf",
    "asdfgh", "asdfghjkl", "zxcvbn", "zxcvbnm", "qazwsx", "qweasd", "wasd", "abc", "abcd", "iloveyou", "loveyou", "love", "lovely", "lover",
    "sweetheart", "darling", "baby", "babygirl", "princess", "prince", "queen", "king", "angel", "beautiful", "gorgeous", "cutie", "kisses",
    "family", "friends", "michael", "jennifer", "jordan", "michelle", "daniel", "hannah", "jessica", "charlie", "thomas", "robert", "andrew",
    "joshua", "ashley", "amanda", "nicole", "taylor", "matthew", "joseph", "william", "samantha", "elizabeth", "anthony", "brandon", "justin",
    "tyler", "hunter", "george", "maggie", "buster", "ginger", "bailey", "cooper", "bella", "lucky", "tigger", "summer", "winter", "spring",
    "autumn", "sunshine", "rainbow", "flower", "butterfly", "ocean", "thunder", "lightning", "storm", "moon", "cloud", "football", "baseball",
    "basketball", "soccer", "hockey", "softball", "golfer", "fishing", "hunting", "tennis", "monkey", "dragon", "tiger", "lion", "eagle",
    "falcon", "panther", "cobra", "viper", "wolf", "bear", "shark", "dolphin", "phoenix", "unicorn", "kitty", "hellokitty", "monster",
    "computer", "internet", "samsung", "google", "facebook", "yahoo", "hotmail", "gmail", "microsoft", "windows", "apple", "oracle", "android",
    "iphone", "xbox", "playstation", "nintendo", "snapchat", "instagram", "youtube", "amazon", "discord", "roblox", "steam", "twitch",
    "amongus", "rockyou", "chocolate", "cheese", "orange", "purple", "banana", "cookie", "peanut", "coffee", "pizza", "candy", "sugar", "honey",
    "pepper", "superman", "batman", "spiderman", "ironman", "gandalf", "matrix", "starwars", "startrek", "pokemon", "pikachu", "naruto",
    "minecraft", "fortnite", "zelda", "mario", "sonic", "money", "freedom", "america", "liberty", "justice", "mystery", "magic", "ninja",
    "warrior", "knight", "wizard",
];

/// The game's password check (FUN_1003f7a48): 0 too short, 1 invalid characters, 2 too common,
/// 3 fine.
pub fn password_check(p: &str) -> u8 {
    if p.len() < 8 {
        return 0;
    }
    if p.bytes().any(|b| !(0x20..0x7f).contains(&b)) {
        return 1;
    }
    let lower = p.to_ascii_lowercase();
    let trimmed = lower.trim_matches(|c: char| !c.is_ascii_lowercase());
    // a repeated unit ("passpass") counts as the unit
    let n = trimmed.len();
    let unit = (1..=n / 2).find(|&u| n % u == 0 && trimmed.as_bytes().chunks(u).all(|c| c == &trimmed.as_bytes()[..u])).map_or(trimmed, |u| &trimmed[..u]);
    let unleet: String = unit
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' => 't',
            c => c,
        })
        .collect();
    if COMMON.contains(&unit) || COMMON.contains(&unleet.as_str()) {
        return 2;
    }
    3
}

/// The strength label beside "Password:" while typing (`textChanged`).
pub fn strength(p: &str) -> &'static str {
    if p.is_empty() {
        return "";
    }
    let classes = [p.bytes().any(|b| b.is_ascii_lowercase()), p.bytes().any(|b| b.is_ascii_uppercase()), p.bytes().any(|b| b.is_ascii_digit()), p.bytes().any(|b| !b.is_ascii_alphanumeric())];
    let score = classes.iter().filter(|c| **c).count() + (p.len() >= 12) as usize + (p.len() >= 16) as usize;
    match (password_check(p), score) {
        (0..=2, _) | (_, 0..=2) => "Weak",
        (_, 3) => "Good",
        (_, 4) => "Strong",
        _ => "Epic",
    }
}

/// `validEmail`: a letter first, then an '@' with a '.' after it, not at the end.
pub fn valid_email(e: &str) -> bool {
    let b = e.as_bytes();
    let (Some(at), Some(dot)) = (e.find('@'), e.rfind('.')) else { return false };
    b.first().is_some_and(|c| c.is_ascii_alphabetic()) && at > 0 && dot > at + 1 && dot + 1 < b.len()
}

/// `registerAccountFailed`: the server's code, as the field label it changes.
pub fn register_error(code: i32) -> (usize, &'static str) {
    match code {
        -2 => (0, "Username is already in use"),
        -3 => (3, "Email is already in use"),
        -4 => (0, "Username is invalid"),
        -5 => (1, "Password is invalid"),
        -6 => (3, "Cannot use this email"),
        -7 => (2, "Passwords do not match"),
        -8 => (1, "Minimum 8 characters"),
        -9 => (0, "Too short. Minimum 3 characters"),
        _ => (0, "Something went wrong."),
    }
}

/// The request body (`userName=%s&password=%s&email=%s&secret=%s&rsession=%s`).
pub fn register_form(user: &str, password: &str, email: &str) -> String {
    let f = crate::gjapi::field;
    format!("userName={}&password={}&email={}&secret={ACCOUNT_SECRET}&rsession=", f(user), f(password), f(email))
}

impl RegisterPopup {
    pub fn new() -> RegisterPopup {
        // `onRegister` tells the servers the page was opened
        let _ = crate::net::spawn(async { crate::net::post("accounts/openedRegister.php", &format!("secret={ACCOUNT_SECRET}")).await.is_ok() });
        RegisterPopup { values: Default::default(), focus: 0, errors: [None; 5], active: false, job: None, t: 0.0 }
    }

    pub fn loading(&self) -> bool {
        self.job.is_some()
    }

    pub fn type_text(&mut self, text: &str) {
        if self.loading() {
            return;
        }
        self.active = true;
        let i = self.focus as usize;
        let (allowed, max) = match i {
            0 => (Some(USER_CHARS), MAX_USER),
            3 | 4 => (Some(EMAIL_CHARS), MAX_FIELD),
            _ => (None, MAX_FIELD),
        };
        for ch in text.chars() {
            let ok = allowed.map_or(!ch.is_control(), |a| a.contains(ch));
            if ok && self.values[i].chars().count() < max {
                self.values[i].push(ch);
            }
        }
        self.live_labels();
    }

    pub fn backspace(&mut self) {
        if !self.loading() {
            self.values[self.focus as usize].pop();
            self.live_labels();
        }
    }

    /// `textChanged`: the confirm field says when the passwords differ, while typing.
    fn live_labels(&mut self) {
        let (p, c) = (&self.values[1], &self.values[2]);
        self.errors[2] = (!p.is_empty() && !c.is_empty() && p != c).then_some("Passwords do not match");
    }

    /// `onSubmit`: the labels reset, then the checks in the game's order; the first failing
    /// field shows why. All good: the request.
    pub fn submit(&mut self) {
        if self.loading() {
            return;
        }
        self.errors = [None; 5];
        let [user, pass, confirm, email, verify] = &self.values;
        let user = user.trim();
        if user.chars().count() < 3 {
            self.errors[0] = Some("Too short. Minimum 3 characters");
        } else if !user.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ') {
            self.errors[0] = Some("Username is invalid");
        }
        match password_check(pass) {
            0 => self.errors[1] = Some("Minimum 8 characters"),
            1 => self.errors[1] = Some("Invalid characters"),
            2 => self.errors[1] = Some("Too common"),
            _ if pass != confirm => self.errors[2] = Some("Passwords do not match"),
            _ => {}
        }
        if !valid_email(email) {
            self.errors[3] = Some("Cannot use this email");
        } else if email != verify {
            self.errors[4] = Some("Emails do not match");
        }
        if self.errors.iter().any(Option::is_some) {
            return;
        }
        let form = register_form(user, pass, email);
        self.job = Some(crate::net::spawn(async move {
            match crate::net::post("accounts/registerGJAccount.php", &form).await {
                Ok(b) if b.trim() == "1" => Ok(()),
                Ok(b) => Err(b.trim().parse().unwrap_or(-1)),
                Err(_) => Err(-1),
            }
        }));
        self.t = 0.0;
    }

    pub fn failed(&mut self, code: i32) {
        self.job = None;
        let (field, text) = register_error(code);
        self.errors[field] = Some(text);
    }
}

pub fn draw(r: &RegisterPopup, c: &mut Canvas, ctx: &mut Ctx, clock: f32) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square01.png", cx, cy, 400.0, 300.0, Look::default());
    c.text("bigFont", "Register Account", cx, cy + 134.0, Look::scaled(0.6));
    if r.loading() {
        super::online::loading_circle(c, cx, cy, r.t);
        return;
    }
    for (i, (label, placeholder)) in FIELDS.iter().enumerate() {
        let fy = cy + 90.0 - 45.0 * i as f32;
        c.nine_slice("square02_small.png", cx, fy, 300.0, 30.0, Look::default().alpha(100));
        ctx.area(Btn::RegField(i as u8), cx, fy, 300.0, 40.0);
        let (text, color) = match r.errors[i] {
            Some(e) => (e, [255, 90, 90]),
            None => (*label, [255, 255, 255]),
        };
        c.text("bigFont", text, cx - 150.0, fy + 16.0, Look::scaled(0.4).anchor(0.0, 0.0).rgb(color));
        if i == 1 && r.errors[1].is_none() {
            let (lw, _) = c.text_size("bigFont", label);
            let s = strength(&r.values[1]);
            let col = match s { "Weak" => [255, 90, 90], "Good" => [255, 200, 0], "Strong" => [120, 255, 120], _ => [0, 200, 255] };
            c.text("bigFont", s, cx - 150.0 + lw * 0.4 + 6.0, fy + 16.0, Look::scaled(0.4).anchor(0.0, 0.0).rgb(col));
        }
        let value = &r.values[i];
        let shown = if i == 1 || i == 2 { "*".repeat(value.chars().count()) } else { value.clone() };
        let s = if shown.is_empty() { 0.0 } else { (280.0 / c.text_size("bigFont", &shown).0.max(1.0)).min(0.8) };
        if shown.is_empty() {
            c.text("bigFont", placeholder, cx, fy, Look::scaled(0.5).rgb([120, 170, 240]));
        } else {
            c.text("bigFont", &shown, cx, fy, Look::scaled(s));
        }
        if r.focus as usize == i && r.active && (clock * 2.0).fract() < 0.5 {
            let half = c.text_size("bigFont", &shown).0 * s * 0.5;
            c.rect(cx + half + 2.0, fy - 9.0, 2.0, 18.0, [1.0; 3], 1.0);
        }
    }
    let (w1, w2) = (super::login::button_width(c, "Cancel"), super::login::button_width(c, "Submit"));
    let total = w1 + 15.0 + w2;
    for (id, text, x, bw) in [(Btn::RegCancel, "Cancel", cx - total * 0.5 + w1 * 0.5, w1), (Btn::RegSubmit, "Submit", cx + total * 0.5 - w2 * 0.5, w2)] {
        let f = ctx.factor(id);
        c.nine_slice("GJ_button_01.png", x, cy - 128.0, bw * f, 31.5 * f, Look::default());
        c.text("goldFont", text, x, cy - 126.0, Look::scaled(f));
        ctx.area(id, x, cy - 128.0, bw, 31.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules_follow_the_game() {
        assert_eq!(password_check("short"), 0);
        assert_eq!(password_check("pässwörd12"), 1);
        assert_eq!(password_check("Password123"), 2);
        assert_eq!(password_check("p4ssw0rd!!"), 2);
        assert_eq!(password_check("dragondragon"), 2);
        assert_eq!(password_check("correct horse battery"), 3);
        assert_eq!(strength("abc"), "Weak");
        assert_eq!(strength("Tr0ub4dor&3xyz"), "Epic");
    }

    #[test]
    fn email_check_and_form() {
        assert!(valid_email("test@example.com"));
        assert!(!valid_email("1test@example.com") && !valid_email("test@com") && !valid_email("test@example."));
        // the recorded request
        assert_eq!(register_form("CadenceTest", "test-pass-123", "test@example.com"), "userName=CadenceTest&password=test-pass-123&email=test%40example.com&secret=Wmfv3899gc9&rsession=");
    }
}

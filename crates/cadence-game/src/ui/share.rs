//! `ShareLevelLayer`: your level's page's Share popup (layout `scenes/share.txt`, captured from
//! the game with a made-up account): request stars (1–10, the difficulty face beside them
//! follows), notify friends / followers, Cancel, Share Level. Sharing uploads the level
//! (`gjapi::upload`) with "Please wait, uploading level..." meanwhile.

use std::sync::mpsc::Receiver;

use super::canvas::{Canvas, Look};
use super::replay::{self, Scene};
use super::Ctx;

pub struct Share {
    pub upload: crate::gjapi::Upload,
    /// The requested stars, 0 for none.
    pub stars: u8,
    pub friends: bool,
    pub followers: bool,
    pub job: Option<Receiver<Result<i64, i32>>>,
    pub t: f32,
}

pub enum Effect {
    None,
    Close,
}

/// The difficulty face for requested stars (none, auto, easy … demon).
fn face(stars: u8) -> &'static str {
    match stars {
        0 => "difficulty_00_btn_001.png",
        1 => "difficulty_auto_btn_001.png",
        2 => "difficulty_01_btn_001.png",
        3 => "difficulty_02_btn_001.png",
        4 | 5 => "difficulty_03_btn_001.png",
        6 | 7 => "difficulty_04_btn_001.png",
        8 | 9 => "difficulty_05_btn_001.png",
        _ => "difficulty_06_btn_001.png",
    }
}

impl Share {
    pub fn new(upload: crate::gjapi::Upload) -> Share {
        Share { upload, stars: 0, friends: false, followers: false, job: None, t: 0.0 }
    }

    pub fn click(&mut self, i: u8) -> Effect {
        if self.job.is_some() {
            return Effect::None;
        }
        match i {
            0..=9 => self.stars = if self.stars == i + 1 { 0 } else { i + 1 },
            10 => self.friends = !self.friends,
            11 => self.followers = !self.followers,
            12 => return Effect::Close,
            13 => {
                let mut u = self.upload.clone();
                u.requested_stars = self.stars as i32;
                u.auto = self.stars == 1;
                u.notify = self.friends as i32 | (self.followers as i32) << 1;
                self.job = Some(crate::net::spawn(async move { crate::gjapi::upload(u).await }));
                self.t = 0.0;
            }
            _ => {}
        }
        Effect::None
    }
}

pub fn draw(s: &Share, c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    if s.job.is_some() {
        c.nine_slice("GJ_square01.png", w * 0.5, h * 0.5, 300.0, 120.0, Look::default());
        c.text("chatFont", "Please wait, uploading level...", w * 0.5, h * 0.5 + 25.0, Look::scaled(0.8));
        super::online::loading_circle(c, w * 0.5, h * 0.5 - 15.0, s.t);
        return;
    }
    replay::draw_where(c, ctx, Scene::ShareLevel, &|k, name, _, _| match k {
        // the captured name, description and face are this level's
        'T' => !name.starts_with("Cadence Upload") && !name.starts_with("(No description"),
        'S' => !name.starts_with("difficulty_") && name != "GJ_checkOff_001.png",
        'N' => name != "GJ_button_04.png",
        _ => true,
    });
    let (cx, top) = (284.5 + dx, 290.0);
    let (tw, _) = c.text_size("goldFont", &s.upload.name);
    c.text("goldFont", &s.upload.name, cx, top, Look::scaled((260.0 / tw.max(1.0)).min(1.0)));
    let desc = if s.upload.desc.is_empty() { "(No description provided)".to_string() } else { s.upload.desc.clone() };
    let lines = super::canvas::rich_lines(c, "chatFont", &desc, 360.0);
    let n = lines.len().min(2) as f32;
    super::canvas::draw_rich(c, "chatFont", &lines[..lines.len().min(2)], cx, 225.5 + (n - 1.0) * 9.0, 1.0, 20.0, 255);
    // the star buttons: the chosen one green
    for i in 0..10u8 {
        let x = 192.9 + 45.8 * (i % 5) as f32 + dx;
        let y = if i < 5 { 147.0 } else { 112.0 };
        let frame = if s.stars == i + 1 { "GJ_button_01.png" } else { "GJ_button_04.png" };
        c.nine_slice(frame, x, y, 28.8, 24.0, Look::default());
        c.text("bigFont", &(i + 1).to_string(), x - 0.8, y + 1.6, Look::scaled(if i == 9 { 0.381 } else { 0.4 }));
    }
    // the `_btn_` faces carry their own label (NA, Auto, Easy …)
    c.sprite(face(s.stars), 439.5 + dx, 129.0, Look::default());
    for (on, y) in [(s.friends, 134.0), (s.followers, 84.0)] {
        c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, 114.5 + dx, y, Look::scaled(0.7));
    }
}

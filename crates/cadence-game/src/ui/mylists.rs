//! My Lists' screens: the browser (`LevelBrowserLayer`, SearchType::MyLists — "Tap New to
//! create a list!") and a list's page (`LevelListLayer`, layout measured from a real capture:
//! the name box, the table, the side buttons delete / update / description / edit mode /
//! difficulty / duplicate / share, "N/M" under it). Levels go in from a level page's list
//! button, which opens this browser to pick the list.

use std::sync::mpsc::Receiver;

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx};
use crate::mylists::{self, MyList};

pub struct Lists {
    /// The list open (its key), else the browser
    pub open: Option<i64>,
    /// A level waiting to be added: the browser picks the list
    pub adding: Option<i32>,
    pub edit_mode: bool,
    /// Typing into the name (0) or the description (1)
    pub typing: Option<u8>,
    pub scroll: f32,
    job: Option<Receiver<Result<String, String>>>,
    pub t: f32,
}

pub enum Effect {
    None,
    Close,
    Alert(String, String),
    /// Ask before deleting the list
    ConfirmDelete(i64),
    /// Open these levels (the list's View)
    View(String, String),
}

impl Lists {
    pub fn new(adding: Option<i32>) -> Lists {
        Lists { open: None, adding, edit_mode: false, typing: None, scroll: 0.0, job: None, t: 0.0 }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The upload's answer: the list's id, or an error.
    pub fn poll(&mut self) -> Option<Effect> {
        let rx = self.job.as_ref()?;
        let msg = rx.try_recv().ok()?;
        self.job = None;
        let key = self.open?;
        Some(match msg {
            Ok(body) if body.trim().parse::<i32>().is_ok_and(|v| v > 0) => {
                let id: i32 = body.trim().parse().unwrap_or(0);
                mylists::update(key, |l| {
                    l.list_id = id;
                    l.version += 1;
                });
                Effect::Alert("Upload".into(), format!("List uploaded! Its ID is <cy>{id}</c>."))
            }
            Ok(_) | Err(_) => Effect::Alert("Upload".into(), "Upload failed. Please try again later.".into()),
        })
    }

    pub fn type_text(&mut self, text: &str) {
        let (Some(key), Some(which)) = (self.open, self.typing) else { return };
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        mylists::update(key, |l| {
            let field = if which == 0 { &mut l.name } else { &mut l.description };
            let max = if which == 0 { 30 } else { 300 };
            for c in text.chars() {
                if field.chars().count() < max {
                    field.push(c);
                }
            }
        });
    }

    pub fn backspace(&mut self) {
        let (Some(key), Some(which)) = (self.open, self.typing) else { return };
        mylists::update(key, |l| {
            if which == 0 {
                l.name.pop();
            } else {
                l.description.pop();
            }
        });
    }

    pub fn click(&mut self, id: Btn, now_ms: i64) -> Effect {
        if self.busy() {
            return Effect::None;
        }
        match (self.open, id) {
            (None, Btn::ListsBack) => return Effect::Close,
            (None, Btn::ListsNew) => {
                let l = mylists::create(now_ms);
                if let Some(level) = self.adding.take() {
                    mylists::update(l.key, |x| x.levels.push(level));
                }
                self.open = Some(l.key);
                self.edit_mode = false;
                self.typing = Some(0);
            }
            (None, Btn::ListsRow(i)) => {
                let all = mylists::list();
                let Some(l) = all.get(i as usize) else { return Effect::None };
                if let Some(level) = self.adding.take() {
                    if l.levels.contains(&level) {
                        return Effect::Alert("Lists".into(), format!("This level is already in <cy>{}</c>.", shown_name(l)));
                    }
                    mylists::update(l.key, |x| x.levels.push(level));
                    return Effect::Alert("Lists".into(), format!("Added to <cy>{}</c>.", shown_name(l)));
                }
                self.open = Some(l.key);
                self.edit_mode = false;
                self.scroll = 0.0;
            }
            (Some(_), Btn::ListsBack) => {
                self.open = None;
                self.typing = None;
            }
            (Some(key), Btn::ListsSide(k)) => {
                let Some(l) = mylists::get(key) else { return Effect::None };
                self.typing = None;
                match k {
                    0 => return Effect::ConfirmDelete(key),
                    // update: the list's levels shown again
                    1 => {}
                    2 => self.typing = Some(1),
                    3 => self.edit_mode = !self.edit_mode,
                    4 => mylists::update(key, |x| x.difficulty = (x.difficulty + 1) % 11),
                    5 => {
                        let mut copy = mylists::create(now_ms);
                        copy.name = format!("{} copy", shown_name(&l));
                        copy.description = l.description.clone();
                        copy.levels = l.levels.clone();
                        copy.difficulty = l.difficulty;
                        let c = copy.clone();
                        mylists::update(copy.key, move |x| *x = c);
                        self.open = Some(copy.key);
                    }
                    _ => {
                        // share: upload (an account, a name and two levels at least)
                        let Some(ident) = crate::gjapi::identity().account.as_ref().map(|_| crate::gjapi::identity()) else {
                            return Effect::Alert("Upload".into(), "You need an <cl>account</c> to upload a list.".into());
                        };
                        if l.name.trim().is_empty() {
                            return Effect::Alert("Upload".into(), "Give the list a <cy>name</c> first.".into());
                        }
                        if l.levels.len() < 2 {
                            return Effect::Alert("Upload".into(), "A list needs at least <cy>2</c> levels.".into());
                        }
                        let Some(form) = mylists::upload_form(&ident, &l, &crate::gjapi::random_string(5)) else { return Effect::None };
                        self.job = Some(crate::net::spawn(async move { crate::net::post("uploadGJLevelList.php", &form).await }));
                        self.t = 0.0;
                    }
                }
            }
            (Some(_), Btn::ListsName) => self.typing = Some(0),
            (Some(key), Btn::ListsLevel(i)) => {
                let Some(l) = mylists::get(key) else { return Effect::None };
                if self.edit_mode {
                    mylists::update(key, |x| {
                        if (i as usize) < x.levels.len() {
                            x.levels.remove(i as usize);
                        }
                    });
                } else if let Some(&level) = l.levels.get(i as usize) {
                    return Effect::View(shown_name(&l), level.to_string());
                }
            }
            (Some(key), Btn::ListsUp(i)) => mylists::update(key, |x| {
                let i = i as usize;
                if i > 0 && i < x.levels.len() {
                    x.levels.swap(i, i - 1);
                }
            }),
            (Some(key), Btn::ListsView) => {
                let Some(l) = mylists::get(key) else { return Effect::None };
                if l.levels.is_empty() {
                    return Effect::None;
                }
                return Effect::View(shown_name(&l), l.levels.iter().map(i32::to_string).collect::<Vec<_>>().join(","));
            }
            _ => {}
        }
        Effect::None
    }
}

fn shown_name(l: &MyList) -> String {
    if l.name.trim().is_empty() { "Unnamed".into() } else { l.name.clone() }
}

/// A level in a list: its saved name and creator when it was played, else its id.
fn level_name(id: i32) -> (String, String) {
    match crate::saved::get(id) {
        Some(s) => (s.info.name, format!("By {}", s.info.author)),
        None => (format!("Level {id}"), String::new()),
    }
}

fn background(c: &mut Canvas) {
    let (w, h) = (c.width, c.height);
    let (gw, gh) = c.size("GJ_gradientBG.png");
    if gw > 0.0 {
        c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb([0, 102, 255]));
    }
    c.sprite("GJ_sideArt_001.png", -1.0, -1.0, Look::default().anchor(0.0, 0.0));
    c.sprite("GJ_sideArt_001.png", w + 1.0, -1.0, Look::default().anchor(1.0, 0.0).flip_x(true));
}

pub fn draw(c: &mut Canvas, ctx: &mut Ctx, s: &Lists) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    let dx = (w - 569.0) * 0.5;
    background(c);
    ctx.button(c, Btn::ListsBack, "GJ_arrow_01_001.png", 24.0, h - 23.0, Look::default(), 1.6);
    match s.open.and_then(mylists::get) {
        None => {
            // the browser: "My Lists" over the brown table, a row a list
            let (lx, ly) = ((w - 356.0) * 0.5, (h - 220.0) * 0.5 - 5.0);
            c.rect(lx, ly, 356.0, 220.0, [191.0 / 255.0, 114.0 / 255.0, 62.0 / 255.0], 1.0);
            let all = mylists::list();
            let top = ly + 220.0;
            c.clip = Some([lx, ly, lx + 356.0, top]);
            for (i, l) in all.iter().enumerate() {
                let y = top - 90.0 * (i as f32 + 1.0) + s.scroll;
                if y + 90.0 <= ly || y >= top {
                    continue;
                }
                let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
                c.rect(lx, y.max(ly), 356.0, (y + 90.0).min(top) - y.max(ly), shade.map(|v| v / 255.0), 1.0);
                c.sprite(&format!("diffIcon_{:02}_btn_001.png", l.difficulty.clamp(0, 10)), lx + 30.0, y + 50.0, Look::default());
                c.text("bigFont", &shown_name(l), lx + 60.0, y + 60.0, Look::scaled(0.7).anchor(0.0, 0.5));
                let n = l.levels.len();
                c.text("goldFont", &format!("{n} level{}", if n == 1 { "" } else { "s" }), lx + 62.0, y + 32.0, Look::scaled(0.5).anchor(0.0, 0.5));
                let label = if s.adding.is_some() { "Add" } else { "View" };
                let f = ctx.factor(Btn::ListsRow(i as u8));
                c.nine_slice("GJ_button_01.png", lx + 314.0, y + 45.0, 66.0 * f, 30.0 * f, Look::default());
                c.text("bigFont", label, lx + 313.0, y + 47.0, Look::scaled(0.6 * f));
                ctx.area(Btn::ListsRow(i as u8), lx + 314.0, y + 45.0, 66.0, 30.0);
            }
            c.clip = None;
            if all.is_empty() {
                let lines = super::canvas::rich_lines(c, "bigFont", "Tap <cg>New</c> to create a <cl>list</c>!", 600.0);
                super::canvas::draw_rich(c, "bigFont", &lines, cx, cy + 10.0, 0.5, 40.0, 255);
            }
            super::options::list_border(c, lx, ly, 356.0, 220.0, "My Lists");
            ctx.button(c, Btn::ListsNew, "GJ_newBtn_001.png", w - 30.7, 30.8, Look::default(), 1.2);
        }
        Some(l) => {
            let at = |x: f32| x + dx;
            // the name box (blue, the name or "List Name" in its blue)
            c.nine_slice("square02b_001.png", at(284.5), 290.0, 365.0, 40.0, Look::default().rgb([0, 56, 141]));
            let (name, col) = if l.name.is_empty() && s.typing != Some(0) { ("List Name".to_string(), [120, 170, 240]) } else { (l.name.clone(), [255, 255, 255]) };
            let shown = if s.typing == Some(0) { format!("{name}_") } else { name };
            let (tw, _) = c.text_size("bigFont", &shown);
            c.text("bigFont", &shown, at(284.5), 290.0, Look::scaled((340.0 / tw.max(1.0)).min(0.7)).rgb(col));
            ctx.area(Btn::ListsName, at(284.5), 290.0, 365.0, 40.0);
            // the table
            c.rect(at(101.5), 40.0, 366.0, 212.0, [191.0 / 255.0, 114.0 / 255.0, 62.0 / 255.0], 1.0);
            c.clip = Some([at(101.5), 40.0, at(467.5), 252.0]);
            for (i, &id) in l.levels.iter().enumerate() {
                let y = 252.0 - 50.0 * (i as f32 + 1.0) + s.scroll;
                if y + 50.0 <= 40.0 || y >= 252.0 {
                    continue;
                }
                let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
                c.rect(at(101.5), y.max(40.0), 366.0, (y + 50.0).min(252.0) - y.max(40.0), shade.map(|v| v / 255.0), 1.0);
                let (n, by) = level_name(id);
                c.text("goldFont", &format!("{}", i + 1), at(118.0), y + 25.0, Look::scaled(0.5));
                c.text("bigFont", &n, at(136.0), y + 32.0, Look::scaled(0.5).anchor(0.0, 0.5));
                if !by.is_empty() {
                    c.text("goldFont", &by, at(136.0), y + 14.0, Look::scaled(0.4).anchor(0.0, 0.5));
                }
                if s.edit_mode {
                    ctx.button(c, Btn::ListsLevel(i as u8), "GJ_deleteIcon_001.png", at(440.0), y + 25.0, Look::scaled(0.7), 1.4);
                    if i > 0 {
                        ctx.button(c, Btn::ListsUp(i as u8), "edit_upBtn_001.png", at(405.0), y + 25.0, Look::scaled(0.6), 1.4);
                    }
                } else {
                    let f = ctx.factor(Btn::ListsLevel(i as u8));
                    c.nine_slice("GJ_button_01.png", at(430.0), y + 25.0, 56.0 * f, 26.0 * f, Look::default());
                    c.text("bigFont", "View", at(429.0), y + 27.0, Look::scaled(0.5 * f));
                    ctx.area(Btn::ListsLevel(i as u8), at(430.0), y + 25.0, 56.0, 26.0);
                }
            }
            c.clip = None;
            if l.levels.is_empty() {
                let lines = super::canvas::rich_lines(c, "bigFont", "Add levels from their page's <cg>list</c> button.", 600.0);
                super::canvas::draw_rich(c, "bigFont", &lines, at(284.5), 160.0, 0.45, 40.0, 255);
            }
            for (x, y, sx, sy, rot) in [(86.5, 47.0, 1.0, 3.0938, 0.0), (482.5, 47.0, -1.0, 3.0938, 0.0)] {
                let mut look = Look::default().scale_xy(1.0, sy).anchor(if sx > 0.0 { 0.0 } else { 1.0 }, 0.0).flip_x(sx < 0.0);
                look.rotation = rot;
                c.sprite("GJ_table_side_001.png", at(x), y, look);
            }
            for (y, rot) in [(40.0, -90.0), (252.0, 90.0)] {
                let mut look = Look::default().scale_xy(1.0, 5.5312);
                look.rotation = rot;
                c.sprite("GJ_table_side_001.png", at(284.5), y, look);
            }
            for (x, y, sx, sy) in [(101.5, 252.0, 1.0, 1.0), (467.5, 252.0, -1.0, 1.0), (101.5, 40.0, 1.0, -1.0), (467.5, 40.0, -1.0, -1.0)] {
                c.sprite("GJ_table_corner_001.png", at(x), y, Look::default().scale_xy(sx, sy));
            }
            // the side buttons
            let face = format!("diffIcon_{:02}_btn_001.png", l.difficulty.clamp(0, 10));
            let side: [(&str, f32, f32); 7] = [("GJ_deleteBtn_001.png", 295.0, 0.8), ("GJ_updateBtn_001.png", 254.0, 0.8), ("GJ_chatBtn_001.png", 213.0, 0.8), ("GJ_editModeBtn_001.png", 172.0, 0.8), ("GJ_plainBtn_001.png", 131.0, 0.8), ("GJ_duplicateBtn_001.png", 90.0, 0.8), ("GJ_shareBtn_001.png", 49.0, 0.4718)];
            for (k, (frame, y, sc)) in side.iter().enumerate() {
                let id = Btn::ListsSide(k as u8);
                let f = ctx.factor(id);
                c.sprite(frame, at(544.0), *y, Look::scaled(sc * f));
                if k == 4 {
                    c.sprite(&face, at(544.0), *y, Look::scaled(0.8 * f));
                }
                ctx.area(id, at(544.0), *y, 36.8, 38.0);
            }
            if s.edit_mode {
                c.sprite("GJ_editModeBtn_001.png", at(544.0), 172.0, Look::scaled(0.8).rgb([125, 255, 125]));
            }
            // the description being typed, under the name
            if s.typing == Some(1) {
                let d = format!("{}_", l.description);
                let (tw, _) = c.text_size("chatFont", &d);
                c.text("chatFont", &d, at(284.5), 262.0, Look::scaled((360.0 / tw.max(1.0)).min(0.6)));
            }
            c.sprite("GJ_progressBar_001.png", at(284.5), 16.0, Look::scaled(0.8).rgb([0, 0, 0]).alpha(125));
            let n = l.levels.len();
            let done = l.levels.iter().filter(|id| crate::saved::get(**id).is_some()).count();
            c.text("bigFont", &format!("{done}/{n}"), at(284.5), 17.0, Look::scaled(0.5));
            if !l.levels.is_empty() {
                ctx.button(c, Btn::ListsView, "GJ_playBtn2_001.png", 30.0, 30.0, Look::scaled(0.5), 1.2);
            }
            if s.busy() {
                c.rect(0.0, 0.0, w, h, [0.0; 3], 120.0 / 255.0);
                super::online::loading_circle(c, cx, cy, s.t);
            }
        }
    }
}

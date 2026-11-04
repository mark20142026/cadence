//! Your account's pages, opened from the main menu's profile button when logged in: your own
//! `ProfilePage` (stats, icons, posts, and the Messages / Friends / Requests / Notifications /
//! Settings buttons with their counts), `FriendsProfilePage` (Friends, Blocked, Following, Muted),
//! `FRequestProfilePage` (received and sent requests), `GJAccountSettingsLayer`, and the legacy
//! messages. Each page is its real layout (`scenes/myprofile.txt` …, captured from the game with
//! a made-up account) with the captured rows left out and this account's drawn in their place,
//! at the rows' captured positions. The requests are in `crate::social`.

use std::sync::mpsc::Receiver;

use super::canvas::{Canvas, Look};
use super::replay::{self, Scene};
use super::{Btn, Ctx};
use crate::social::{self, Act, Counts, Post, User};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Profile,
    /// 0 friends, 1 blocked, 2 following, 3 muted
    Friends(u8),
    /// 0 received, 1 sent, 2 ignored
    Requests(u8),
    Settings,
    /// The legacy messages: 0 received, 1 sent
    Messages(u8),
    Notifications,
}

pub enum Msg {
    Notifications(Result<usize, ()>),
    Messages(Vec<social::Message>),
    Opened(Box<social::Message>),
    Profile(Box<(crate::online::Profile, Counts, Vec<Post>)>),
    Users(Vec<User>),
    Done(Result<(), String>, &'static str),
    Error(String),
}

/// What a click asks of the rest of the UI.
pub enum Effect {
    None,
    Close,
    Alert(String, String),
    /// Open another player's profile (by account).
    Profile(i64),
    /// Open the write-a-post / write-a-message box.
    Write { to: Option<i64>, title: &'static str },
    /// A received request opened (`FriendRequestPopup`): its row, sender and message.
    Request { row: usize, name: String, text: String },
}

pub struct Social {
    /// The notifications' count, or an error (`NotificationsPage`).
    pub notifications: Option<Result<usize, ()>>,
    /// The legacy messages listed, and the one opened.
    pub messages: Option<Vec<social::Message>>,
    pub open_message: Option<social::Message>,
    pub stack: Vec<Page>,
    pub profile: Option<(crate::online::Profile, Counts, Vec<Post>)>,
    pub users: Option<Vec<User>>,
    pub settings: crate::gjapi::AccountSettings,
    pub job: Option<Receiver<Msg>>,
    pub t: f32,
    /// The list's scroll, in points.
    pub scroll: f32,
}

/// A row button: (what, row): 1 name, 2 message, 3 delete post, 4 like post, 5 accept request,
/// 6 remove (unfriend / unblock / delete request).
pub fn row_btn(what: u16, row: usize) -> Btn {
    Btn::Social(what * 1000 + row as u16)
}

impl Social {
    pub fn open() -> Social {
        let mut s = Social { notifications: None, messages: None, open_message: None, stack: vec![Page::Profile], profile: None, users: None, settings: Default::default(), job: None, t: 0.0, scroll: 0.0 };
        s.load();
        s
    }

    /// A page with its data already fetched (headless screenshots, with `CADENCE_SERVER`
    /// pointing at a stand-in).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_at(page: Page) -> Social {
        let mut s = Social::open();
        while s.busy() {
            std::thread::sleep(std::time::Duration::from_millis(10));
            s.update(0.0);
        }
        if page != Page::Profile {
            s.go(page);
            while s.busy() {
                std::thread::sleep(std::time::Duration::from_millis(10));
                s.update(0.0);
            }
        }
        s
    }

    pub fn page(&self) -> Page {
        *self.stack.last().unwrap_or(&Page::Profile)
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Fetch what the current page shows.
    pub fn load(&mut self) {
        self.scroll = 0.0;
        self.job = Some(match self.page() {
            Page::Profile => crate::net::spawn(async {
                match social::own_profile().await {
                    Ok(p) => Msg::Profile(Box::new(p)),
                    Err(e) => Msg::Error(e),
                }
            }),
            Page::Friends(tab) => {
                self.users = None;
                crate::net::spawn(async move {
                    match tab {
                        0 | 1 => social::user_list(tab as i32).await.map_or_else(Msg::Error, Msg::Users),
                        // following and muted are kept by the account on the servers' newer
                        // lists; an empty list until they answer
                        _ => Msg::Users(Vec::new()),
                    }
                })
            }
            Page::Requests(tab) => {
                self.users = None;
                crate::net::spawn(async move {
                    match tab {
                        0 | 1 => social::requests(tab == 1).await.map_or_else(Msg::Error, Msg::Users),
                        _ => Msg::Users(Vec::new()),
                    }
                })
            }
            Page::Settings => return,
            Page::Notifications => {
                self.notifications = None;
                crate::net::spawn(async {
                    let id = crate::gjapi::identity();
                    let body = crate::net::post("getGJNotifications.php", &format!("{}&page=0&secret={}&total=0", id.base(), crate::gjapi::SECRET)).await;
                    Msg::Notifications(match body {
                        Ok(b) if b.trim().starts_with('-') || b.trim().is_empty() => Err(()),
                        Ok(b) => Ok(b.split('#').next().unwrap_or("").split('|').filter(|e| !e.is_empty()).count()),
                        Err(_) => Err(()),
                    })
                })
            }
            Page::Messages(tab) => {
                self.messages = None;
                crate::net::spawn(async move { social::messages(tab == 1, 0).await.map_or_else(Msg::Error, Msg::Messages) })
            }
        });
    }

    fn act(&mut self, a: Act, done: &'static str) {
        self.job = Some(crate::net::spawn(async move { Msg::Done(social::act(a).await, done) }));
    }

    /// Accept the received request in row `row`.
    pub fn accept(&mut self, row: usize) {
        if let Some(u) = self.users.as_ref().and_then(|l| l.get(row)).cloned() {
            self.act(Act::Accept { user: u.account_id, request: u.request_id }, "accept");
        }
    }

    pub fn post(&mut self, text: String) {
        self.act(Act::Post(text), "post");
    }

    pub fn message(&mut self, to: i64, subject: String, body: String) {
        self.act(Act::Message { to, subject, body }, "message");
    }

    pub fn update(&mut self, dt: f32) -> Effect {
        self.t += dt;
        let Some(rx) = &self.job else { return Effect::None };
        let msg = match rx.try_recv() {
            Ok(m) => m,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Effect::None,
            Err(_) => Msg::Error("-1".into()),
        };
        self.job = None;
        match msg {
            Msg::Profile(p) => {
                self.settings = p.1.settings.clone();
                self.profile = Some(*p);
            }
            Msg::Users(u) => self.users = Some(u),
            Msg::Messages(m) => self.messages = Some(m),
            Msg::Notifications(n) => self.notifications = Some(n),
            Msg::Opened(m) => self.open_message = Some(*m),
            Msg::Error(e) => {
                self.users.get_or_insert_with(Vec::new);
                if self.page() == Page::Profile && self.profile.is_none() {
                    crate::app::log(&format!("own profile: {e}"));
                    return Effect::Alert("Error".into(), "Something went wrong...".into());
                }
            }
            Msg::Done(Ok(()), what) => {
                // the page reloads what changed
                match what {
                    "post" | "delete" => self.load(),
                    "settings" => {
                        self.stack.pop();
                        return Effect::Alert("Success".into(), "Settings updated.".into());
                    }
                    "message" => return Effect::Alert("Message sent".into(), "Your message has been sent.".into()),
                    _ => self.load(),
                }
            }
            Msg::Done(Err(code), what) => {
                let text = match (what, code.as_str()) {
                    ("message", _) => "Failed to send the message. The player may not accept messages.",
                    ("post", _) => "Failed to post. Please try again later.",
                    _ => "Something went wrong. Please try again later.",
                };
                return Effect::Alert("Error".into(), text.into());
            }
        }
        Effect::None
    }

    pub fn scroll_by(&mut self, delta: f32) {
        let rows = match self.page() {
            Page::Profile => self.profile.as_ref().map_or(0, |p| p.2.len()) as f32 * 90.0 - 92.0,
            Page::Friends(_) | Page::Requests(_) => self.users.as_ref().map_or(0, Vec::len) as f32 * 40.0 - 180.0,
            Page::Settings => 0.0,
            Page::Messages(_) => self.messages.as_ref().map_or(0, Vec::len) as f32 * 50.0 - 188.0,
            Page::Notifications => 0.0,
        };
        self.scroll = (self.scroll + delta).clamp(0.0, rows.max(0.0));
    }

    /// A click on the page's layout button `i` (`Btn::Replay`) or a row's (`Btn::Social`).
    pub fn click(&mut self, id: Btn) -> Effect {
        let page = self.page();
        match (page, id) {
            // back / close
            (Page::Profile, Btn::Replay(6)) | (Page::Friends(_), Btn::Replay(4)) | (Page::Requests(_), Btn::Replay(3)) => {
                self.stack.pop();
                if self.stack.is_empty() {
                    return Effect::Close;
                }
                self.load();
            }
            (Page::Profile, Btn::Replay(7)) if !self.busy() => self.load(),
            (Page::Profile, Btn::Replay(24)) => self.go(Page::Friends(0)),
            (Page::Profile, Btn::Replay(25)) => self.go(Page::Requests(0)),
            (Page::Profile, Btn::Replay(27)) => self.stack.push(Page::Settings),
            (Page::Profile, Btn::Replay(23)) => self.go(Page::Messages(0)),
            // the messages: the received / sent tabs, a message (opens with its text), its
            // reply, back
            (Page::Messages(tab), Btn::Social(code)) => {
                let (what, row) = (code / 1000, (code % 1000) as usize);
                let Some(m) = self.messages.as_ref().and_then(|l| l.get(row)).cloned() else { return Effect::None };
                match what {
                    1 => {
                        let sent = tab == 1;
                        self.job = Some(crate::net::spawn(async move { social::message(m.id, sent).await.map_or_else(Msg::Error, |x| Msg::Opened(Box::new(x))) }));
                    }
                    2 => {
                        self.open_message = None;
                        return Effect::Write { to: Some(m.account_id), title: "Reply" };
                    }
                    3 => self.act(Act::DeleteMessage { id: m.id, sent: tab == 1 }, "delete"),
                    _ => {}
                }
            }
            (Page::Messages(_), Btn::MsgClose) => {
                if self.open_message.take().is_none() {
                    self.stack.pop();
                    self.load();
                }
            }
            (Page::Profile, Btn::Replay(26)) => self.go(Page::Notifications),
            (Page::Notifications, Btn::Replay(0)) => {
                self.stack.pop();
                self.load();
            }
            (Page::Notifications, Btn::Replay(1)) if !self.busy() => self.load(),
            (Page::Notifications, Btn::Replay(2)) => return Effect::Alert("Notifications".into(), "Notifications show likes, friend requests, rewards and level activity on your account.".into()),
            // the old messages' layout buttons: back, the sent ones, select all, refresh
            (Page::Messages(_), Btn::Replay(8)) => {
                self.stack.pop();
                self.load();
            }
            (Page::Messages(tab), Btn::Replay(9)) => {
                self.stack.pop();
                self.go(Page::Messages(1 - tab.min(1)));
            }
            (Page::Messages(_), Btn::Replay(11)) if !self.busy() => self.load(),
            (Page::Messages(_), Btn::Replay(_)) => {}
            (Page::Profile, Btn::Replay(28)) => return Effect::Write { to: None, title: "Add Comment" },
            (Page::Profile, Btn::Replay(20)) => return Effect::Alert("Comment History".into(), "Your comments on levels show on each level's page.".into()),
            (Page::Profile, Btn::Social(code)) => {
                let (what, row) = (code / 1000, (code % 1000) as usize);
                let Some(post) = self.profile.as_ref().and_then(|p| p.2.get(row)).cloned() else { return Effect::None };
                match what {
                    3 => self.act(Act::DeletePost(post.id), "delete"),
                    4 => {
                        let me = crate::gjapi::identity().account.map_or(0, |a| a.account_id);
                        self.act(Act::LikePost { id: post.id, owner: me, like: true }, "like");
                    }
                    _ => {}
                }
            }
            // the tabs
            (Page::Friends(_), Btn::Replay(i @ 5..=8)) => {
                self.stack.pop();
                self.go(Page::Friends(i - 5));
            }
            (Page::Friends(_), Btn::Replay(11)) | (Page::Requests(_), Btn::Replay(10)) if !self.busy() => self.load(),
            (Page::Requests(_), Btn::Replay(i @ 4..=6)) => {
                self.stack.pop();
                self.go(Page::Requests(i - 4));
            }
            (Page::Friends(tab) | Page::Requests(tab), Btn::Social(code)) => {
                let (what, row) = (code / 1000, (code % 1000) as usize);
                let Some(u) = self.users.as_ref().and_then(|l| l.get(row)).cloned() else { return Effect::None };
                let requests = matches!(page, Page::Requests(_));
                match what {
                    1 if requests && tab == 0 => return Effect::Request { row, name: u.name.clone(), text: u.text.clone() },
                    1 => return Effect::Profile(u.account_id),
                    2 => return Effect::Write { to: Some(u.account_id), title: "Send Message" },
                    5 if requests && tab == 0 => self.act(Act::Accept { user: u.account_id, request: u.request_id }, "accept"),
                    6 if requests => self.act(Act::DeleteRequest { user: u.account_id, sent: tab == 1 }, "remove"),
                    6 if tab == 0 => self.act(Act::Unfriend(u.account_id), "remove"),
                    6 if tab == 1 => self.act(Act::Unblock(u.account_id), "remove"),
                    _ => {}
                }
            }
            (Page::Settings, Btn::Social(code)) => {
                // the settings' check boxes: (row, option)
                let (row, opt) = ((code / 10) % 10, (code % 10) as i32);
                let s = &mut self.settings;
                match row {
                    0 => s.messages = opt,
                    1 => s.friend_requests = opt,
                    2 => s.group_invites = opt,
                    _ => s.comment_history = opt,
                }
            }
            (Page::Settings, Btn::Replay(i)) => match i {
                // Cancel, Update
                11 => {
                    self.stack.pop();
                }
                12 => self.act(Act::Settings(self.settings.clone()), "settings"),
                _ => return Effect::Alert("Social Links".into(), "Your social links are set on the account's page on the game's site for now.".into()),
            },
            _ => {}
        }
        Effect::None
    }

    fn go(&mut self, p: Page) {
        self.stack.push(p);
        self.load();
    }
}

const ICON_PREFIXES: [&str; 10] = ["player_", "ship_", "bird_", "dart_", "robot_", "spider_", "swing_", "jetpack_", "player_ball_", "d_ball_"];

/// The rank icon for a global rank (`ProfilePage::loadPageFromUserInfo`).
fn rank_icon(rank: i64) -> &'static str {
    match rank {
        r if r <= 0 => "",
        1 => "rankIcon_1_001.png",
        2..=10 => "rankIcon_top10_001.png",
        11..=50 => "rankIcon_top50_001.png",
        51..=100 => "rankIcon_top100_001.png",
        101..=200 => "rankIcon_top200_001.png",
        201..=500 => "rankIcon_top500_001.png",
        501..=1000 => "rankIcon_top1000_001.png",
        1001..=5000 => "rankIcon_top2500_001.png",
        5001..=10000 => "rankIcon_top10000_001.png",
        _ => "rankIcon_all_001.png",
    }
}

fn commas(v: i64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A count badge (the red ball and its number) at a button's top right.
fn badge(c: &mut Canvas, x: f32, y: f32, n: i64) {
    if n > 0 {
        c.sprite("d_ball_01_001.png", x + 16.5, y + 17.25, Look::scaled(0.6).rgb([255, 0, 0]));
        c.text("bigFont", &n.min(99).to_string(), x + 16.5, y + 17.85, Look::scaled(0.48));
    }
}

/// A player's icon (cube unless the row says otherwise), with their colors.
fn draw_user_icon(c: &mut Canvas, u: &User, x: f32, y: f32, scale: f32) {
    let kind = (u.icon_type as usize).min(8);
    let base = super::garage::icon_sheet(kind, u.icon);
    let (c1, c2) = (super::menu::color_for_idx(u.color1), super::menu::color_for_idx(u.color2));
    super::garage::draw_icon(c, &base, x, y, scale, c1, c2);
}

pub fn draw(s: &Social, c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    match s.page() {
        Page::Profile => draw_profile(s, c, ctx, dx),
        Page::Friends(tab) => draw_list(s, c, ctx, dx, Scene::Friends, tab, 4),
        Page::Requests(tab) => draw_list(s, c, ctx, dx, Scene::Requests, tab, 3),
        // over your profile, as the game opens it
        Page::Messages(tab) => draw_messages(s, c, ctx, dx, tab),
        Page::Notifications => draw_notifications(s, c, ctx, dx),
        Page::Settings => {
            draw_profile(s, c, ctx, dx);
            ctx.buttons.clear();
            c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
            draw_settings(s, c, ctx, dx);
        }
    }
}

/// The captured posts' parts (their rows, frames, like / delete icons and buttons 0–5), which
/// share the list's height with the bottom buttons.
fn post_part(kind: char, name: &str, y: f32) -> bool {
    match kind {
        'R' => y < 158.0,
        'N' => name == "square02b_001.png" && y < 158.0,
        'S' => matches!(name, "GJ_likesIcon_001.png" | "GJ_deleteIcon_001.png"),
        'B' => name.parse::<u8>().is_ok_and(|i| i <= 5),
        _ => false,
    }
}

fn draw_profile(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    replay::draw_where(c, ctx, Scene::MyProfile, &|k, name, x, y| match k {
        'T' => false,
        'S' => {
            let stat = (y - 245.0).abs() < 0.5;
            !(ICON_PREFIXES.iter().any(|p| name.starts_with(p)) || name.starts_with("rankIcon") || stat || post_part(k, name, y))
        }
        'R' | 'N' => !post_part(k, name, y),
        'B' => !post_part(k, name, y) && (y - 245.0).abs() >= 0.5 && !(x - 284.5).abs().lt(&0.5),
        _ => true,
    });
    let cx = 284.5 + dx;
    let Some((p, counts, posts)) = &s.profile else {
        super::online::loading_circle(c, cx, 160.0, s.t);
        return;
    };
    // the name, the info button after it, the rank
    let (nw, _) = c.text_size("bigFont", &p.name);
    let ns = (200.0 / nw.max(1.0)).min(0.8);
    c.text("bigFont", &p.name, cx, 285.0, Look::scaled(ns));
    let icon = rank_icon(p.rank);
    if !icon.is_empty() {
        c.sprite(icon, 120.5 + dx, 283.0, Look::default());
        c.text("chatFont", &p.rank.to_string(), 160.5 + dx, 277.0, Look::scaled(0.6));
    }
    // the stats row: number (bigFont 0.6) then its icon, 20 between, centered
    let stats: Vec<(i64, &str)> = [(p.stars, "GJ_starsIcon_001.png"), (p.moons, "GJ_moonsIcon_001.png"), (p.coins, "GJ_coinsIcon_001.png"), (p.user_coins, "GJ_coinsIcon2_001.png"), (p.demons, "GJ_demonIcon_001.png")]
        .into_iter()
        .chain((p.creator_points > 0).then_some((p.creator_points, "GJ_hammerIcon_001.png")))
        .collect();
    let widths: Vec<f32> = stats.iter().map(|(v, _)| c.text_size("bigFont", &commas(*v)).0 * 0.6).collect();
    let total: f32 = widths.iter().map(|w| w + 14.0).sum::<f32>() + 20.0 * (stats.len() as f32 - 1.0) + 12.0;
    let mut x = cx - total * 0.5;
    for (i, ((v, icon), tw)) in stats.iter().zip(&widths).enumerate() {
        c.text("bigFont", &commas(*v), x, 245.0, Look::scaled(0.6).anchor(0.0, 0.5));
        let ix = x + tw + 14.0;
        c.sprite(icon, ix, 245.0, Look::scaled(0.9));
        ctx.area(Btn::Replay(8 + i.min(2) as u8), ix, 245.0, 30.0, 30.0);
        x = ix + 20.0;
    }
    // the icons
    let (c1, c2) = (super::menu::color_for_idx(p.color1), super::menu::color_for_idx(p.color2));
    for (k, ix) in [110.5f32, 154.5, 198.5, 242.5, 286.5, 324.5, 368.5, 414.5, 456.5].into_iter().enumerate() {
        let base = super::garage::icon_sheet(k, p.icons[k]);
        // `SimplePlayer` at 0.95 in the profile's icon row (the ball 0.9 of that)
        super::garage::draw_icon(c, &base, ix + dx, 200.0, if k == 2 { 0.855 } else { 0.95 }, c1, c2);
    }
    // the posts: 90-high cells under the list's top (153), scrolled, cut to the list
    c.clip = Some([107.0 + dx, 61.0, 462.0 + dx, 153.0]);
    if posts.is_empty() {
        c.text("goldFont", "Nothing here yet...", cx, 107.0, Look::scaled(0.6));
    }
    for (i, post) in posts.iter().enumerate() {
        let top = 158.0 - 90.0 * i as f32 + s.scroll;
        if top < 61.0 || top - 90.0 > 153.0 {
            continue;
        }
        let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(114.5 + dx, top - 90.0, 340.0, 90.0, shade.map(|v| v / 255.0), 1.0);
        c.nine_slice("square02b_001.png", cx, top - 45.0, 330.0, 78.0, Look::default().rgb([130, 64, 33]).alpha(100));
        c.text("goldFont", &p.name, 124.5 + dx, top - 18.0, Look::scaled(0.7).anchor(0.0, 0.5));
        let lines = super::canvas::rich_lines(c, "chatFont", &post.text, 290.0);
        super::canvas::draw_rich_left(c, "chatFont", &lines[..lines.len().min(3)], 124.5 + dx, top - 45.0, 1.0, 20.0);
        c.sprite("GJ_deleteIcon_001.png", 371.5 + dx, top - 19.0, Look::scaled(0.76));
        ctx.area(row_btn(3, i), 371.5 + dx, top - 19.0, 18.24, 18.24);
        c.sprite("GJ_likesIcon_001.png", 395.5 + dx, top - 18.0, Look::scaled(0.8));
        ctx.area(row_btn(4, i), 395.5 + dx, top - 18.0, 19.2, 18.8);
        c.text("bigFont", &post.likes.to_string(), 422.5 + dx, top - 19.5, Look::scaled(0.4));
        c.text("bigFont", "...", 442.5 + dx, top - 9.0, Look::scaled(0.5).alpha(120));
        c.text("chatFont", &format!("{} ago", post.age), 443.5 + dx, top - 76.0, Look::scaled(0.55).anchor(1.0, 0.5).rgb([0, 0, 0]).alpha(140));
    }
    c.clip = None;
    // the buttons' counts
    badge(c, 158.5 + dx, 25.0, counts.messages);
    badge(c, 221.5 + dx, 25.0, counts.friends);
    badge(c, 284.5 + dx, 25.0, counts.requests);
}

/// Friends and Requests: their layout's frame, tabs and search; this account's rows (340 × 40,
/// alternating shades, from the list's top at 240).
fn draw_list(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32, scene: Scene, tab: u8, first_tab: u8) {
    let tabs: &[&str] = if scene == Scene::Friends {
        &["threadBtn_friendOn_001.png", "threadBtn_blockOn_001.png", "threadBtn_followOn_001.png", "threadBtn_muteOn_001.png"]
    } else {
        &["threadBtn_inboxOn_001.png", "threadBtn_sentOn_001.png", "threadBtn_ignoreOn_001.png"]
    };
    let in_rows = |x: f32, y: f32| (110.0..460.0).contains(&x) && (60.0..241.0).contains(&y);
    replay::draw_where(c, ctx, scene, &|k, name, x, y| match k {
        'T' => !in_rows(x, y) && !name.starts_with("Total") && !name.contains(" to ") && !(name == "Search" && false),
        'S' => !(in_rows(x, y) && !name.starts_with("GJ_comment")) && !tabs.contains(&name),
        'R' => !in_rows(x, y),
        'B' => !in_rows(x, y),
        _ => true,
    });
    // the tabs: the open one in color, the rest grey
    for (i, frame) in tabs.iter().enumerate() {
        let Some((x, y)) = replay::button_center(scene, first_tab + 1 + i as u8 + if scene == Scene::Friends { 0 } else { 0 }) else { continue };
        let look = if i as u8 == tab { Look::default() } else { Look::default().rgb([240, 240, 240]).alpha(215).gray() };
        c.sprite(frame, x + dx, y, look);
    }
    let Some(users) = &s.users else {
        super::online::loading_circle(c, 284.5 + dx, 150.0, s.t);
        return;
    };
    let label = match (scene, tab) {
        (Scene::Friends, 0) => format!("Total friends: {}", users.len()),
        (Scene::Friends, 1) => format!("Blocked: {}", users.len()),
        (Scene::Friends, _) => format!("Total: {}", users.len()),
        _ if users.is_empty() => String::new(),
        _ => format!("1 to {} of {}", users.len(), users.len()),
    };
    if scene == Scene::Friends {
        c.text("goldFont", &label, 284.5 + dx, 40.0, Look::scaled(0.7));
    } else if !label.is_empty() {
        c.text("chatFont", &label, 284.5 + dx, 28.0, Look::scaled(0.6).rgb([0, 0, 0]).alpha(120));
    }
    if users.is_empty() {
        let none = match (scene, tab) {
            (Scene::Friends, 0) => "No friends yet",
            (Scene::Friends, 1) => "Nobody is blocked",
            (Scene::Friends, _) => "Nobody here yet",
            _ => "No friend requests",
        };
        c.text("goldFont", none, 284.5 + dx, 150.0, Look::scaled(0.6));
        return;
    }
    c.clip = Some([114.5 + dx, 60.0, 454.5 + dx, 240.0]);
    for (i, u) in users.iter().enumerate() {
        let y0 = 200.0 - 40.0 * i as f32 + s.scroll;
        if y0 + 40.0 < 60.0 || y0 > 240.0 {
            continue;
        }
        let x0 = 114.5 + dx;
        let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(x0, y0, 340.0, 40.0, shade.map(|v| v / 255.0), 1.0);
        draw_user_icon(c, u, x0 + 22.0, y0 + 20.0, 0.8);
        let (ny, sy) = if scene == Scene::Requests { (31.0, 13.5) } else { (29.5, 12.0) };
        let (nw, _) = c.text_size("goldFont", &u.name);
        c.text("goldFont", &u.name, x0 + 46.0, y0 + ny, Look::scaled(0.6).anchor(0.0, 0.5));
        ctx.area(row_btn(1, i), x0 + 46.0 + nw * 0.3, y0 + ny, nw * 0.6, 16.5);
        if u.new {
            c.text("goldFont", "New!", x0 + 46.0 + nw * 0.6 + 8.0, y0 + ny, Look::scaled(0.45).anchor(0.0, 0.5));
        }
        let mut x = x0 + 46.0;
        for (v, icon, iw) in [(u.stars, "GJ_starsIcon_001.png", 17.2), (u.moons, "GJ_moonsIcon_001.png", 17.2), (u.demons, "GJ_demonIcon_001.png", 17.2)] {
            let t = v.to_string();
            let tw = c.text_size("bigFont", &t).0 * 0.4;
            c.text("bigFont", &t, x, y0 + sy, Look::scaled(0.4).anchor(0.0, 0.5));
            c.sprite(icon, x + tw + iw - 10.2 + 3.8, y0 + sy - 0.5, Look::scaled(0.5));
            x += tw + iw + 13.0;
        }
        // the row's buttons: message; requests also accept / remove
        let my = y0 + if scene == Scene::Requests { 21.5 } else { 20.0 };
        if scene == Scene::Requests {
            c.sprite("accountBtn_messages_001.png", x0 + 288.0, my, Look::scaled(0.55));
            ctx.area(row_btn(2, i), x0 + 288.0, my, 23.65, 22.8);
            c.sprite("GJ_checkOff_001.png", x0 + 318.0, my, Look::scaled(0.7));
            ctx.area(row_btn(6, i), x0 + 318.0, my, 21.7, 21.7);
        } else {
            c.sprite("accountBtn_messages_001.png", x0 + 318.0, my, Look::scaled(0.55));
            ctx.area(row_btn(2, i), x0 + 318.0, my, 23.65, 22.8);
        }
    }
    c.clip = None;
}

/// The legacy messages (`MessagesProfilePage`, "Old Messages"): its layout; rows of 340 × 50
/// (the check box, subject with a green "!" when unread, "From: name", age, View, X); the
/// envelope switches to the sent ones; an opened message's text over it, with Reply.
fn draw_messages(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32, tab: u8) {
    let cx = 284.5 + dx;
    let in_rows = |x: f32, y: f32| (110.0..460.0).contains(&x) && (55.0..254.0).contains(&y);
    replay::draw_where(c, ctx, Scene::OldMessages, &|k, name, x, y| match k {
        'T' => !in_rows(x, y) && name != "Old Messages",
        'S' => !(in_rows(x, y) && !name.starts_with("GJ_comment")),
        'R' | 'B' => !in_rows(x, y),
        _ => true,
    });
    c.text("bigFont", if tab == 0 { "Old Messages" } else { "Old Sent" }, cx, 285.0, Look::scaled(0.7822));
    if let Some(m) = &s.open_message {
        ctx.buttons.clear();
        c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 100.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, 160.0, 400.0, 250.0, Look::default());
        c.text("goldFont", &m.subject, cx, 262.0, Look::scaled(0.8));
        c.text("chatFont", &format!("{} {}  {} ago", if m.sent { "To" } else { "From" }, m.name, m.age), cx, 240.0, Look::scaled(0.6).alpha(200));
        let lines = super::canvas::rich_lines(c, "chatFont", &m.body, 360.0);
        let n = lines.len().min(7);
        super::canvas::draw_rich(c, "chatFont", &lines[..n], cx, 205.0, 0.8, 18.0, 255);
        let row = s.messages.as_ref().and_then(|l| l.iter().position(|x| x.id == m.id)).unwrap_or(0);
        let buttons: Vec<(Btn, &str)> = if m.sent { vec![(Btn::MsgClose, "OK")] } else { vec![(Btn::MsgClose, "OK"), (row_btn(2, row), "Reply")] };
        let widths: Vec<f32> = buttons.iter().map(|(_, t)| super::login::button_width(c, t)).collect();
        let total = widths.iter().sum::<f32>() + 15.0 * (buttons.len() as f32 - 1.0);
        let mut x = cx - total * 0.5;
        for ((id, text), bw) in buttons.into_iter().zip(widths) {
            let f = ctx.factor(id);
            c.nine_slice("GJ_button_01.png", x + bw * 0.5, 60.0, bw * f, 31.5 * f, Look::default());
            c.text("goldFont", text, x + bw * 0.5, 62.0, Look::scaled(f));
            ctx.area(id, x + bw * 0.5, 60.0, bw, 31.5);
            x += bw + 15.0;
        }
        return;
    }
    let Some(list) = &s.messages else {
        super::online::loading_circle(c, cx, 150.0, s.t);
        return;
    };
    if list.is_empty() {
        c.text("goldFont", "No messages", cx, 155.0, Look::scaled(0.6));
        return;
    }
    c.clip = Some([114.5 + dx, 61.0, 454.5 + dx, 249.0]);
    for (i, m) in list.iter().enumerate() {
        let y0 = 203.0 - 50.0 * i as f32 + s.scroll;
        if y0 + 50.0 < 61.0 || y0 > 249.0 {
            continue;
        }
        let x0 = 114.5 + dx;
        let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(x0, y0, 340.0, 50.0, shade.map(|v| v / 255.0), 1.0);
        c.sprite("GJ_checkOff_001.png", x0 + 20.0, y0 + 25.0, Look::scaled(0.7));
        let (sw, _) = c.text_size("bigFont", &m.subject);
        let ss = (200.0 / sw.max(1.0)).min(0.5);
        c.text("bigFont", &m.subject, x0 + 38.0, y0 + 39.0, Look::scaled(ss).anchor(0.0, 0.5));
        if !m.read && !m.sent {
            c.text("bigFont", "!", x0 + 38.0 + sw * ss + 6.0, y0 + 39.0, Look::scaled(0.5).anchor(0.0, 0.5).rgb([0, 255, 0]));
        }
        let from = format!("{} {}", if m.sent { "To:" } else { "From:" }, m.name);
        c.text("goldFont", &from, x0 + 39.0, y0 + 24.0, Look::scaled(0.5).anchor(0.0, 0.5));
        c.text("chatFont", &format!("{} ago", m.age), x0 + 41.0, y0 + 10.0, Look::scaled(0.5).anchor(0.0, 0.5).rgb([0, 0, 0]).alpha(125));
        for (id, frame, text, bx, bw, ts) in [(row_btn(1, i), "GJ_button_01.png", "View", x0 + 263.45, 63.1, 0.6), (row_btn(3, i), "GJ_button_06.png", "X", x0 + 318.0, 26.0, 0.48)] {
            let f = ctx.factor(id);
            c.nine_slice(frame, bx, y0 + 25.0, bw * f, 25.0 * f, Look::default());
            c.text("bigFont", text, bx - 1.0, y0 + 27.0, Look::scaled(ts * f));
            ctx.area(id, bx, y0 + 25.0, bw, 25.0);
        }
    }
    c.clip = None;
}

/// `NotificationsPage`: its layout, and the list's state (none, or an error).
fn draw_notifications(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    replay::draw_where(c, ctx, Scene::Notifications, &|k, name, _, _| !(k == 'T' && name == "Something went wrong"));
    let cx = 284.5 + dx;
    match &s.notifications {
        None => super::online::loading_circle(c, cx, 160.0, s.t),
        Some(Err(())) => {
            c.text("goldFont", "Something went wrong", cx, 160.0, Look::scaled(0.6));
        }
        Some(Ok(0)) => {
            c.text("goldFont", "No notifications", cx, 160.0, Look::scaled(0.6));
        }
        Some(Ok(n)) => {
            c.text("goldFont", &format!("{n} notification{}", if *n == 1 { "" } else { "s" }), cx, 160.0, Look::scaled(0.6));
        }
    }
}

/// `GJAccountSettingsLayer`: the layout with this account's choices checked.
fn draw_settings(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    // the check boxes' rows and their options' x (from the layout: the boxes at y 225, 185,
    // 145, 105)
    let rows: [(f32, &[f32]); 4] = [(236.0, &[214.5, 314.5]), (196.0, &[172.5, 274.5, 352.5]), (156.0, &[214.5, 314.5]), (116.0, &[179.5, 254.5, 354.5])];
    replay::draw_where(c, ctx, Scene::AcctSettings, &|k, name, _, y| match k {
        'S' => !(name.starts_with("GJ_check") && rows.iter().any(|(ry, _)| (y - ry).abs() < 3.0)),
        'B' => !rows.iter().any(|(ry, _)| (y - ry).abs() < 3.0),
        _ => true,
    });
    let chosen = [s.settings.messages, s.settings.friend_requests, s.settings.group_invites, s.settings.comment_history];
    for (r, (y, xs)) in rows.iter().enumerate() {
        for (o, x) in xs.iter().enumerate() {
            let on = chosen[r] == o as i32;
            c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, x + dx, *y, Look::scaled(0.5));
            ctx.area(Btn::Social(5000 + 10 * r as u16 + o as u16), x + dx, *y, 17.5, 17.25);
        }
    }
}

/// The box for a new post (`ShareCommentLayer`, "Add Comment", 140 characters) or a message
/// to a player (subject and body, like the legacy `GJWriteMessagePopup`).
pub struct Writer {
    pub title: &'static str,
    pub to: Option<i64>,
    /// A comment on this level (its id) rather than a profile post.
    pub level: Option<i64>,
    /// A custom song id for your level in this slot.
    pub song_slot: Option<usize>,
    pub subject: String,
    pub body: String,
    /// The field typed into: 0 subject, 1 body (posts have the body only).
    pub field: u8,
    pub t: f32,
}

impl Writer {
    pub fn new(title: &'static str, to: Option<i64>) -> Writer {
        Writer { title, to, level: None, song_slot: None, subject: String::new(), body: String::new(), field: if to.is_some() { 0 } else { 1 }, t: 0.0 }
    }

    fn limit(&self) -> usize {
        match (self.to, self.field) {
            (None, _) => 140,
            (Some(_), 0) => 35,
            _ => 200,
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let limit = self.limit();
        let f = if self.field == 0 { &mut self.subject } else { &mut self.body };
        for ch in text.chars().filter(|c| !c.is_control()) {
            if f.chars().count() < limit {
                f.push(ch);
            }
        }
    }

    pub fn backspace(&mut self) {
        if self.field == 0 { self.subject.pop() } else { self.body.pop() };
    }
}

pub fn draw_writer(wr: &Writer, c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square01.png", cx, cy, 420.0, 240.0, Look::default());
    c.text("goldFont", wr.title, cx, cy + 98.0, Look::scaled(0.8));
    let caret = (wr.t * 2.0).fract() < 0.5;
    let fields: Vec<(u8, f32, f32, &str, &str)> = if wr.to.is_some() {
        vec![(0, cy + 52.0, 30.0, &wr.subject, "Subject"), (1, cy - 12.0, 80.0, &wr.body, "Message")]
    } else {
        vec![(1, cy + 10.0, 110.0, &wr.body, "Write something...")]
    };
    for (id, fy, fh, value, placeholder) in fields {
        c.nine_slice("square02_small.png", cx, fy, 360.0, fh, Look::default().alpha(100));
        ctx.area(Btn::WriteField(id), cx, fy, 360.0, fh);
        if value.is_empty() {
            c.text("chatFont", placeholder, cx, fy, Look::scaled(0.8).rgb([120, 170, 240]));
        } else {
            let lines = super::canvas::rich_lines(c, "chatFont", value, 340.0);
            let n = lines.len().max(1) as f32;
            super::canvas::draw_rich(c, "chatFont", &lines, cx, fy + (n - 1.0) * 9.0, 0.8, 18.0, 255);
        }
        if wr.field == id && caret {
            c.rect(cx + 172.0, fy - 8.0, 2.0, 16.0, [1.0; 3], 1.0);
        }
    }
    let left = wr.limit().saturating_sub(if wr.field == 0 { wr.subject.chars().count() } else { wr.body.chars().count() });
    c.text("chatFont", &left.to_string(), cx + 190.0, cy - 88.0, Look::scaled(0.6).anchor(1.0, 0.5).alpha(160));
    let send = if wr.to.is_some() { "Send" } else { "Post" };
    let (w1, w2) = (super::login::button_width(c, "Cancel"), super::login::button_width(c, send));
    let total = w1 + 15.0 + w2;
    for (id, text, x, bw) in [(Btn::WriteCancel, "Cancel", cx - total * 0.5 + w1 * 0.5, w1), (Btn::WriteSend, send, cx + total * 0.5 - w2 * 0.5, w2)] {
        let f = ctx.factor(id);
        c.nine_slice("GJ_button_01.png", x, cy - 92.0, bw * f, 31.5 * f, Look::default());
        c.text("goldFont", text, x, cy - 90.0, Look::scaled(f));
        ctx.area(id, x, cy - 92.0, bw, 31.5);
    }
}

/// The icon sheets the open page draws (its players' icons).
pub fn wanted_sheets(s: &Social) -> Vec<String> {
    let mut v: Vec<String> = match s.page() {
        Page::Profile => s.profile.as_ref().map(|(p, _, _)| (0..9).map(|k| super::garage::icon_sheet(k, p.icons[k])).collect()).unwrap_or_default(),
        Page::Friends(_) | Page::Requests(_) => s.users.iter().flatten().map(|u| super::garage::icon_sheet((u.icon_type as usize).min(8), u.icon)).collect(),
        Page::Settings | Page::Messages(_) | Page::Notifications => Vec::new(),
    };
    v.sort();
    v.dedup();
    v
}

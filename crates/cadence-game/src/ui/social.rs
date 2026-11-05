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
    /// Your comments on levels (`InfoLayer` in comment-history mode)
    History,
    /// The message threads (`MessageThreadsPage`): 0 all, 1 direct messages, 2 groups
    Threads(u8),
    /// One thread's messages (`ThreadViewPopup`, over the threads), the one in `viewing`
    ThreadView,
    /// Your invites and message requests (`InvitesPopup`): false pending, true ignored
    Invites(bool),
}

pub enum Msg {
    History(Vec<crate::online::Comment>),
    Notifications(Result<(Vec<social::Notification>, usize), ()>),
    Messages(Vec<social::Message>),
    Threads(Result<(Vec<social::Thread>, usize, i32), ()>),
    ThreadPage(Result<social::ThreadPage, ()>),
    /// Older messages ("Load earlier"), to go before those shown.
    ThreadOlder(Result<social::ThreadPage, ()>),
    Invites(Result<Vec<social::Invite>, ()>),
    ThreadSearch(String, Result<Vec<social::ThreadMessage>, ()>),
    Members(Result<social::Members, ()>),
    Opened(Box<social::Message>),
    Profile(Box<(crate::online::Profile, Counts, Vec<Post>)>),
    Users(Vec<User>),
    Friends(Vec<User>),
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
    /// Open the box for a message in this thread.
    WriteThread(i64),
    /// Open `NewThreadPopup` (a direct message or a group).
    NewThread,
    /// Open `RenameThreadPopup` for this thread (its current name).
    RenameThread(i64, String),
    /// Open the box for searching this thread's messages.
    SearchThread(i64),
    /// Open the message box for this thread with this text already in it.
    WriteThreadWith(i64, String),
    /// Open the box for sharing a level or list in this thread.
    ShareInThread(i64),
    /// Find this level online (a shared level in a message).
    OpenLevel(i64),
    /// Open the box for filtering the invites.
    SearchInvites,
    /// Pick friends to add to this group.
    AddMembers(i64),
    /// A received request opened (`FriendRequestPopup`): its row, sender and message.
    Request { row: usize, name: String, text: String },
}

pub struct Social {
    /// Your comment history.
    pub history: Option<Vec<crate::online::Comment>>,
    /// The notifications' count, or an error (`NotificationsPage`).
    pub notifications: Option<Result<(Vec<social::Notification>, usize), ()>>,
    /// The legacy messages listed, and the one opened.
    pub messages: Option<Vec<social::Message>>,
    /// The threads listed (rows, total, invites waiting), and the rows ticked.
    pub threads: Option<Result<(Vec<social::Thread>, usize, i32), ()>>,
    pub ticked: Vec<i64>,
    /// The thread open (`ThreadView`) and its messages.
    pub viewing: Option<social::Thread>,
    /// `ThreadOptionsPopup` open on this thread, and its Clear / Leave question.
    pub options: Option<social::Thread>,
    pub options_confirm: Option<i32>,
    /// The invites listed, the one being ignored (asking first), and the one accepted (its
    /// thread opens once the server says yes).
    pub invites: Option<Result<Vec<social::Invite>, ()>>,
    pub ignoring: Option<i64>,
    /// The invites' search box: only names or titles with this in them.
    pub invite_filter: String,
    /// A search in the open thread: the query and the messages found.
    pub thread_search: Option<(String, Option<Vec<social::ThreadMessage>>)>,
    /// `MemberListPopup` open on this group, its list, a kick / new owner asked about
    /// (mode, account, name), and whether only the owner adds members.
    pub members_of: Option<social::Thread>,
    pub members: Option<Result<social::Members, ()>>,
    pub members_confirm: Option<(i32, i64, String)>,
    pub owner_adds: bool,
    /// `ReportPopup` on this thread: the reasons ticked, and the "Confirm Report" question.
    pub reporting: Option<(social::Thread, Vec<i32>, bool)>,
    /// `EmotePickerPopup` open over the message box, and its scroll.
    pub emote_picker: bool,
    pub picker_scroll: f32,
    pub accepted: Option<social::Thread>,
    /// Your friends, for the new-thread popup's "To".
    pub friends: Option<Vec<User>>,
    pub thread_page: Option<Result<social::ThreadPage, ()>>,
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
        let mut s = Social { history: None, notifications: None, messages: None, threads: None, ticked: Vec::new(), viewing: None, thread_page: None, friends: None, options: None, options_confirm: None, invites: None, ignoring: None, invite_filter: String::new(), accepted: None, thread_search: None, members_of: None, members: None, members_confirm: None, owner_adds: false, reporting: None, emote_picker: false, picker_scroll: 0.0, open_message: None, stack: vec![Page::Profile], profile: None, users: None, settings: Default::default(), job: None, t: 0.0, scroll: 0.0 };
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

    /// The first listed thread opened (headless screenshots, like `open_at`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_first_thread() -> Social {
        let mut s = Social::open_at(Page::Threads(0));
        s.viewing = s.threads.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _, _)| l.first()).cloned();
        s.go(Page::ThreadView);
        while s.busy() {
            std::thread::sleep(std::time::Duration::from_millis(10));
            s.update(0.0);
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
                        // friends, blocked, following, muted: getGJUserList20 types 0–3
                        _ => social::user_list(tab as i32).await.map_or_else(Msg::Error, Msg::Users),
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
            Page::History => {
                self.history = None;
                crate::net::spawn(async {
                    let id = crate::gjapi::identity();
                    let user = id.account.as_ref().map_or(0, |a| a.user_id);
                    match crate::net::post("getGJCommentHistory.php", &crate::gjapi::comment_history_form(&id, user, 0, false)).await {
                        Ok(b) if !b.trim().starts_with('-') => Msg::History(crate::online::parse_comments(&b).0),
                        _ => Msg::History(Vec::new()),
                    }
                })
            }
            Page::Notifications => {
                self.notifications = None;
                crate::net::spawn(async {
                    let id = crate::gjapi::identity();
                    // the game's request (recorded): every kind (mask 65535)
                    let body = crate::net::post("getGJNotifications.php", &format!("{}&page=0&total=0&mask=65535&secret={}", id.base(), crate::gjapi::SECRET)).await;
                    Msg::Notifications(match body {
                        Ok(b) if b.trim() == "-2" || b.trim().is_empty() => Ok((Vec::new(), 0)),
                        Ok(b) if b.trim().starts_with('-') => Err(()),
                        Ok(b) => Ok(social::parse_notifications(b.trim())),
                        Err(_) => Err(()),
                    })
                })
            }
            Page::Invites(_) => {
                self.invites = None;
                crate::net::spawn(async { Msg::Invites(social::invites().await.map_err(|_| ())) })
            }
            Page::ThreadView => {
                self.thread_page = None;
                let id = self.viewing.as_ref().map_or(0, |t| t.id);
                crate::net::spawn(async move { Msg::ThreadPage(social::thread_messages(id, 0).await.map_err(|_| ())) })
            }
            Page::Threads(tab) => {
                self.threads = None;
                crate::net::spawn(async move { Msg::Threads(social::threads(tab as i32, 0).await.map_err(|_| ())) })
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

    /// Whether the open thread takes messages (not locked; status 0 or 1 — 2–7 are the
    /// blocked kinds, 8 removed).
    pub fn can_send(&self) -> bool {
        match self.thread_page.as_ref().and_then(|r| r.as_ref().ok()) {
            Some(p) => !p.locked && p.status <= 1,
            None => false,
        }
    }

    /// An option of `ThreadOptionsPopup` (updateGJThread), then the page reloads.
    pub fn update_thread(&mut self, thread: i64, mode: i32, value: Option<String>) {
        self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(thread, mode, value).await, "thread option") }));
    }

    /// The options popup's buttons for `t`: (label, button sheet, what).
    fn option_buttons(t: &social::Thread) -> Vec<(&'static str, &'static str, u8)> {
        let mute = if t.muted { "Unmute" } else { "Mute" };
        let pin = if t.starred { "Unpin" } else { "Pin" };
        if t.direct {
            vec![("View Profile", "GJ_button_01.png", 0), (mute, "GJ_button_04.png", 2), (pin, "GJ_button_04.png", 3), ("Report", "GJ_button_05.png", 4), ("Clear", "GJ_button_06.png", 5)]
        } else {
            vec![("Members", "GJ_button_01.png", 6), ("Rename", "GJ_button_04.png", 1), (mute, "GJ_button_04.png", 2), (pin, "GJ_button_04.png", 3), ("Report", "GJ_button_05.png", 4), ("Leave", "GJ_button_06.png", 7)]
        }
    }

    /// `ReportPopup`: close, a reason's tick box, Report (at least one reason; then the
    /// game's "Confirm Report").
    fn report_click(&mut self, id: Btn) -> Effect {
        let Some((t, picked, confirm)) = self.reporting.as_mut() else { return Effect::None };
        let Btn::Social(code) = id else { return Effect::None };
        if *confirm {
            match code {
                8191 => {
                    let (thread, reasons) = (t.id, picked.clone());
                    self.reporting = None;
                    self.job = Some(crate::net::spawn(async move { Msg::Done(social::report_thread(thread, reasons).await, "report") }));
                }
                8190 => *confirm = false,
                _ => {}
            }
            return Effect::None;
        }
        match code {
            8100 => self.reporting = None,
            8101..=8105 => {
                let reason = REPORT_REASONS[(code - 8101) as usize].0;
                match picked.iter().position(|&r| r == reason) {
                    Some(i) => {
                        picked.remove(i);
                    }
                    None => picked.push(reason),
                }
            }
            8106 if picked.is_empty() => return Effect::Alert("Report".into(), "Please select at least one reason.".into()),
            8106 => *confirm = true,
            _ => {}
        }
        Effect::None
    }

    pub fn load_members(&mut self) {
        let Some(id) = self.members_of.as_ref().map(|t| t.id) else { return };
        self.members = None;
        self.job = Some(crate::net::spawn(async move { Msg::Members(social::members(id).await.map_err(|_| ())) }));
    }

    /// A click while the members popup is open: close, Add Member, who adds, a member's X
    /// (kick) or crown (make owner) — both asked about first — or their name.
    fn members_click(&mut self, id: Btn) -> Effect {
        let Some(t) = self.members_of.clone() else { return Effect::None };
        if let Some((mode, account, _)) = self.members_confirm.clone() {
            if id == Btn::Social(8091) {
                self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(t.id, mode, Some(account.to_string())).await, "member") }));
            }
            if matches!(id, Btn::Social(8090 | 8091)) {
                self.members_confirm = None;
            }
            return Effect::None;
        }
        let list = self.members.as_ref().and_then(|r| r.as_ref().ok()).cloned().unwrap_or_default();
        let Btn::Social(code) = id else { return Effect::None };
        match code {
            8099 => {
                self.members_of = None;
                self.members = None;
            }
            8098 => return Effect::AddMembers(t.id),
            8097 => {
                self.owner_adds = !self.owner_adds;
                let v = if self.owner_adds { "1" } else { "0" };
                self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(t.id, 13, Some(v.into())).await, "member") }));
            }
            8000..=8019 => {
                if let Some(u) = list.members.get((code - 8000) as usize) {
                    self.members_confirm = Some((6, u.account_id, u.name.clone()));
                } else if let Some((acc, name)) = list.invited.get((code - 8000) as usize - list.members.len()) {
                    self.members_confirm = Some((6, *acc, name.clone()));
                }
            }
            8050..=8069 => {
                if let Some(u) = list.members.get((code - 8050) as usize) {
                    self.members_confirm = Some((7, u.account_id, u.name.clone()));
                }
            }
            8070..=8089 => {
                if let Some(u) = list.members.get((code - 8070) as usize) {
                    return Effect::Profile(u.account_id);
                }
            }
            _ => {}
        }
        Effect::None
    }

    /// Your friends, for the "To" / Add Member lists (once).
    pub fn load_friends(&mut self) {
        if self.friends.is_none() && !self.busy() {
            self.job = Some(crate::net::spawn(async { social::user_list(0).await.map_or_else(|_| Msg::Friends(Vec::new()), Msg::Friends) }));
        }
    }

    pub fn add_members(&mut self, thread: i64, ids: Vec<i64>) {
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(thread, 4, Some(list)).await, "member") }));
    }

    /// A click while the options popup is open.
    fn options_click(&mut self, id: Btn) -> Effect {
        let Some(t) = self.options.clone() else { return Effect::None };
        if let Some(mode) = self.options_confirm {
            match id {
                Btn::Social(7791) => {
                    self.options = None;
                    self.options_confirm = None;
                    if mode == 5 && self.page() == Page::ThreadView {
                        self.stack.pop();
                    }
                    self.update_thread(t.id, mode, None);
                }
                Btn::Social(7790) => self.options_confirm = None,
                _ => {}
            }
            return Effect::None;
        }
        let Btn::Social(code @ 7700..=7799) = id else { return Effect::None };
        if code == 7799 {
            self.options = None;
            return Effect::None;
        }
        let buttons = Social::option_buttons(&t);
        let Some(&(_, _, what)) = buttons.get((code - 7700) as usize) else { return Effect::None };
        match what {
            0 => {
                self.options = None;
                return Effect::Profile(t.account);
            }
            1 => {
                self.options = None;
                return Effect::RenameThread(t.id, t.title.clone());
            }
            2 => {
                self.options = None;
                self.update_thread(t.id, 10, Some(if t.muted { "0" } else { "1" }.into()));
            }
            3 => {
                self.options = None;
                self.update_thread(t.id, 15, Some(if t.starred { "0" } else { "1" }.into()));
            }
            6 => {
                self.options = None;
                self.members_of = Some(t);
                self.load_members();
            }
            5 => self.options_confirm = Some(1),
            7 => self.options_confirm = Some(5),
            4 => {
                self.options = None;
                self.reporting = Some((t, Vec::new(), false));
            }
            _ => {}
        }
        Effect::None
    }

    pub fn send_new_thread(&mut self, n: &NewThread, name: String, text: String) {
        let ids: Vec<i64> = n.to.iter().map(|u| u.0).collect();
        let (dm, targets, desc, body) = if n.group { (None, ids, text, String::new()) } else { (ids.first().copied(), Vec::new(), String::new(), text) };
        self.job = Some(crate::net::spawn(async move { Msg::Done(social::send_new_thread(dm, targets, name, desc, body).await, "new thread") }));
    }

    pub fn search_thread(&mut self, thread: i64, query: String) {
        self.thread_search = Some((query.clone(), None));
        self.job = Some(crate::net::spawn(async move {
            let r = social::search_thread(thread, query.clone()).await.map_err(|_| ());
            Msg::ThreadSearch(query, r)
        }));
    }

    pub fn send_thread(&mut self, thread: i64, text: String) {
        self.job = Some(crate::net::spawn(async move { Msg::Done(social::send_thread_message(thread, text).await, "thread message") }));
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
            Msg::Friends(u) => self.friends = Some(u),
            Msg::Messages(m) => self.messages = Some(m),
            Msg::ThreadPage(p) => self.thread_page = Some(p),
            Msg::Invites(l) => self.invites = Some(l),
            Msg::Members(m) => self.members = Some(m),
            Msg::ThreadSearch(q, r) => self.thread_search = Some((q, Some(r.unwrap_or_default()))),
            Msg::ThreadOlder(Ok(older)) => {
                if let Some(Ok(p)) = self.thread_page.as_mut() {
                    let mut messages = older.messages;
                    messages.retain(|m| !p.messages.iter().any(|n| n.id == m.id));
                    messages.append(&mut p.messages);
                    p.messages = messages;
                    for u in older.users {
                        if !p.users.iter().any(|v| v.account == u.account) {
                            p.users.push(u);
                        }
                    }
                    p.earlier = older.earlier;
                }
            }
            Msg::ThreadOlder(Err(())) => return Effect::Alert("Error".into(), "Something went wrong...".into()),
            Msg::Threads(t) => {
                self.ticked.clear();
                self.threads = Some(t);
            }
            Msg::Notifications(n) => self.notifications = Some(n),
            Msg::History(h) => self.history = Some(h),
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
                    "member" => self.load_members(),
                    "report" => return Effect::Alert("Report".into(), "Report received".into()),
                    "accept invite" => {
                        if let Some(t) = self.accepted.take() {
                            self.viewing = Some(t);
                            self.stack.pop();
                            self.go(Page::ThreadView);
                        }
                    }
                    "post" | "delete" => self.load(),
                    "settings" => {
                        self.stack.pop();
                        return Effect::Alert("Success".into(), "Settings updated.".into());
                    }
                    "message" => return Effect::Alert("Message sent".into(), "Your message has been sent.".into()),
                    // the thread shows it (reloaded below)
                    _ => self.load(),
                }
            }
            Msg::Done(Err(code), what) => {
                let text = match (what, code.as_str()) {
                    ("message", _) => "Failed to send the message. The player may not accept messages.",
                    ("thread message" | "new thread", _) => "Failed to send the message. Please try again later.",
                    ("post", _) => "Failed to post. Please try again later.",
                    _ => "Something went wrong. Please try again later.",
                };
                return Effect::Alert("Error".into(), text.into());
            }
        }
        Effect::None
    }

    pub fn scroll_by(&mut self, delta: f32) {
        if self.emote_picker {
            self.picker_scroll = (self.picker_scroll + delta).clamp(0.0, (picker_height() - 150.0).max(0.0));
            return;
        }
        let rows = match self.page() {
            Page::Profile => self.profile.as_ref().map_or(0, |p| p.2.len()) as f32 * 90.0 - 92.0,
            Page::Friends(_) | Page::Requests(_) => self.users.as_ref().map_or(0, Vec::len) as f32 * 40.0 - 180.0,
            Page::Settings => 0.0,
            Page::Messages(_) => self.messages.as_ref().map_or(0, Vec::len) as f32 * 50.0 - 188.0,
            Page::Notifications => self.notifications.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |(l, _)| l.len()) as f32 * 40.0 - 196.0,
            Page::History => s_len(&self.history) as f32 * 44.0 - 176.0,
            Page::Invites(_) => 0.0,
            Page::ThreadView => self.thread_page.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |p| p.messages.len()) as f32 * 43.0 - 150.0,
            Page::Threads(_) => self.threads.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |(l, _, _)| l.len()) as f32 * 40.0 - 196.0,
        };
        self.scroll = (self.scroll + delta).clamp(0.0, rows.max(0.0));
    }

    /// A click on the page's layout button `i` (`Btn::Replay`) or a row's (`Btn::Social`).
    pub fn click(&mut self, id: Btn) -> Effect {
        if self.emote_picker {
            // an emote goes into a new message; anywhere else closes the picker
            self.emote_picker = false;
            if let Btn::Social(code @ 8200..=8499) = id {
                let thread = self.viewing.as_ref().map_or(0, |t| t.id);
                if let Some(e) = crate::emotes::EMOTES.get((code - 8200) as usize) {
                    return Effect::WriteThreadWith(thread, format!(":{}: ", e.0));
                }
            }
            return Effect::None;
        }
        if self.reporting.is_some() {
            return self.report_click(id);
        }
        if self.members_of.is_some() {
            return self.members_click(id);
        }
        if self.options.is_some() {
            return self.options_click(id);
        }
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
            (Page::Profile, Btn::Replay(23)) => self.go(Page::Threads(0)),
            // the threads page: back, Legacy (the old messages), the tabs, refresh, select all,
            // a row's tick box
            (Page::Threads(_), Btn::Replay(5)) => {
                self.stack.pop();
                self.load();
            }
            (Page::Threads(_), Btn::Replay(6)) => self.go(Page::Messages(0)),
            (Page::Threads(_), Btn::Replay(16)) => self.go(Page::Invites(false)),
            // the invites: Ignore asks first; close, Pending ↔ Ignored, Accept, a name
            (Page::Invites(_), Btn::Social(code)) if self.ignoring.is_some() => {
                let thread = self.ignoring.take().unwrap_or(0);
                if code == 7891 {
                    self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(thread, 9, None).await, "ignore invite") }));
                }
            }
            (Page::Invites(_), Btn::Social(7897)) => return Effect::SearchInvites,
            (Page::Invites(_), Btn::Social(7896)) => self.invite_filter.clear(),
            (Page::Invites(_), Btn::Social(7899)) => {
                self.stack.pop();
                self.load();
            }
            (Page::Invites(ignored), Btn::Social(7898)) => {
                self.stack.pop();
                self.stack.push(Page::Invites(!ignored));
            }
            (Page::Invites(ignored), Btn::Social(code)) if (7800..7990).contains(&code) => {
                let shown: Vec<social::Invite> = self.invites.as_ref().and_then(|r| r.as_ref().ok()).map(|l| l.iter().filter(|i| i.ignored == ignored && invite_matches(i, &self.invite_filter)).cloned().collect()).unwrap_or_default();
                let Some(inv) = shown.get((code % 50) as usize).cloned() else { return Effect::None };
                match code / 50 {
                    // 7800–7849 Accept, 7850–7899 Ignore, 7950–7989 the sender's name
                    156 => {
                        self.accepted = Some(social::Thread { id: inv.thread, account: inv.account, name: inv.name.clone(), title: inv.title.clone(), direct: inv.direct, icon: inv.icon, color1: inv.color1, color2: inv.color2, glow_color: inv.glow_color, icon_type: inv.icon_type, ..Default::default() });
                        let thread = inv.thread;
                        self.job = Some(crate::net::spawn(async move { Msg::Done(social::update_thread(thread, 8, None).await, "accept invite") }));
                    }
                    157 if !ignored => self.ignoring = Some(inv.thread),
                    159 => return Effect::Profile(inv.account),
                    _ => {}
                }
            }
            (Page::Threads(_), Btn::Replay(7)) => {
                self.load_friends();
                return Effect::NewThread;
            }
            (Page::Threads(tab), Btn::Replay(i @ 11..=13)) if i as u8 - 11 != tab => {
                self.stack.pop();
                self.go(Page::Threads(i as u8 - 11));
            }
            (Page::Threads(_), Btn::Replay(14)) if !self.busy() => self.load(),
            (Page::Threads(_), Btn::Replay(15)) => {
                let all: Vec<i64> = self.threads.as_ref().and_then(|r| r.as_ref().ok()).map(|(l, _, _)| l.iter().map(|t| t.id).collect()).unwrap_or_default();
                self.ticked = if self.ticked.len() == all.len() { Vec::new() } else { all };
            }
            // a row opens its thread; a direct message's name, the player's profile
            (Page::Threads(_), Btn::Social(code)) if (7200..7300).contains(&code) => {
                if let Some(t) = self.threads.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _, _)| l.get((code - 7200) as usize)).cloned() {
                    self.viewing = Some(t);
                    self.go(Page::ThreadView);
                }
            }
            (Page::Threads(_), Btn::Social(code)) if (7600..7700).contains(&code) => {
                self.options = self.threads.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _, _)| l.get((code - 7600) as usize)).cloned();
            }
            (Page::ThreadView, Btn::Replay(7)) => self.options = self.viewing.clone(),
            (Page::Threads(_), Btn::Social(code)) if (7000..7100).contains(&code) => {
                if let Some(t) = self.threads.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _, _)| l.get((code - 7000) as usize)) {
                    return Effect::Profile(t.account);
                }
            }
            // the thread: close, refresh, the message box and Send, a sender's name
            (Page::ThreadView, Btn::Replay(6)) => {
                self.stack.pop();
                self.load();
            }
            (Page::ThreadView, Btn::Replay(11)) if self.can_send() => {
                self.emote_picker = true;
                self.picker_scroll = 0.0;
            }
            (Page::ThreadView, Btn::Replay(10)) if self.can_send() => return Effect::ShareInThread(self.viewing.as_ref().map_or(0, |t| t.id)),
            (Page::ThreadView, Btn::Replay(8)) if !self.busy() => {
                self.thread_search = None;
                self.load();
            }
            // search: the box, or back to the messages when a search is shown
            (Page::ThreadView, Btn::Replay(9)) => {
                if self.thread_search.take().is_none() {
                    return Effect::SearchThread(self.viewing.as_ref().map_or(0, |t| t.id));
                }
            }
            (Page::ThreadView, Btn::Social(7401)) if !self.busy() => {
                let (id, before) = (self.viewing.as_ref().map_or(0, |t| t.id), self.thread_page.as_ref().and_then(|r| r.as_ref().ok()).and_then(|p| p.messages.first()).map_or(0, |m| m.id));
                self.job = Some(crate::net::spawn(async move { Msg::ThreadOlder(social::thread_messages(id, before).await.map_err(|_| ())) }));
            }
            (Page::ThreadView, Btn::Replay(12) | Btn::Social(7400)) if self.can_send() => {
                return Effect::WriteThread(self.viewing.as_ref().map_or(0, |t| t.id));
            }
            // a shared level in a message: find it online
            (Page::ThreadView, Btn::Social(code)) if (8600..8700).contains(&code) => {
                let text = self.thread_page.as_ref().and_then(|r| r.as_ref().ok()).and_then(|p| p.messages.get((code - 8600) as usize)).map(|m| m.text.clone()).unwrap_or_default();
                let level = text.split(' ').find_map(|w| w.strip_prefix("l_").and_then(|n| n.parse::<i64>().ok()));
                if let Some(id) = level {
                    return Effect::OpenLevel(id);
                }
            }
            (Page::ThreadView, Btn::Social(code)) if (7300..7400).contains(&code) => {
                let account = self.thread_page.as_ref().and_then(|r| r.as_ref().ok()).and_then(|p| p.messages.get((code - 7300) as usize)).map_or(0, |m| m.account);
                if account > 0 {
                    return Effect::Profile(account);
                }
            }
            (Page::Threads(_), Btn::Social(code)) if (7100..7200).contains(&code) => {
                let row = (code - 7100) as usize;
                if let Some(t) = self.threads.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _, _)| l.get(row)) {
                    match self.ticked.iter().position(|&id| id == t.id) {
                        Some(i) => {
                            self.ticked.remove(i);
                        }
                        None => self.ticked.push(t.id),
                    }
                }
            }
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
            // a row's bell: stop notifications from that player (`muteGJUser.php`)
            (Page::Notifications, Btn::Social(code)) if code >= 6000 => {
                let row = (code - 6000) as usize;
                let Some(n) = self.notifications.as_ref().and_then(|r| r.as_ref().ok()).and_then(|(l, _)| l.get(row)).cloned() else { return Effect::None };
                if n.account > 0 {
                    self.act(Act::Mute { user: n.account, on: true }, "mute");
                }
            }
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
            (Page::Profile, Btn::Replay(20)) => self.go(Page::History),
            (Page::History, Btn::MsgClose) => {
                self.stack.pop();
                self.load();
            }
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
        Page::Threads(tab) => draw_threads(s, c, ctx, dx, tab),
        Page::Invites(ignored) => {
            draw_threads(s, c, ctx, dx, 0);
            ctx.buttons.clear();
            c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 150.0 / 255.0);
            draw_invites(s, c, ctx, dx, ignored);
        }
        Page::ThreadView => {
            let tab = s.stack.iter().rev().find_map(|p| if let Page::Threads(t) = p { Some(*t) } else { None }).unwrap_or(0);
            draw_threads(s, c, ctx, dx, tab);
            ctx.buttons.clear();
            c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 150.0 / 255.0);
            draw_thread_view(s, c, ctx, dx);
        }
        Page::History => draw_history(s, c, ctx, dx),
        Page::Settings => {
            draw_profile(s, c, ctx, dx);
            ctx.buttons.clear();
            c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
            draw_settings(s, c, ctx, dx);
        }
    }
    if let Some(t) = &s.options {
        ctx.buttons.clear();
        draw_thread_options(s, t, c, ctx, dx);
    }
    if let Some(t) = &s.members_of {
        ctx.buttons.clear();
        draw_members(s, t, c, ctx, dx);
    }
    if let Some((t, picked, confirm)) = &s.reporting {
        ctx.buttons.clear();
        draw_report(t, picked, *confirm, c, ctx, dx);
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

fn s_len<T>(v: &Option<Vec<T>>) -> usize {
    v.as_ref().map_or(0, Vec::len)
}

/// Your comment history: the comments popup's frame and its comment rows (44 high: your name,
/// the level's percent, the text, likes, age).
fn draw_history(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    let cx = 284.5 + dx;
    c.nine_slice("GJ_square01.png", cx, 160.0, 440.0, 290.0, Look::default());
    c.text("bigFont", "Comment History", cx, 285.0, Look::scaled(0.8));
    ctx.button(c, Btn::MsgClose, "GJ_closeBtn_001.png", 74.5 + dx, 295.0, Look::default(), 1.5);
    let (ax, top) = (107.5 + dx, 253.0);
    for (x, flip) in [(ax, false), (ax + 354.0, true)] {
        c.sprite("GJ_commentSide_001.png", x, 77.0, Look::default().anchor(if flip { 1.0 } else { 0.0 }, 0.0).scale_xy(if flip { -1.0 } else { 1.0 }, 5.9));
    }
    let Some(list) = &s.history else {
        super::online::loading_circle(c, cx, 160.0, s.t);
        return;
    };
    if list.is_empty() {
        c.text("goldFont", "No comments", cx, 160.0, Look::scaled(0.6));
        return;
    }
    let name = crate::gjapi::linked_account().map(|(n, _)| n).unwrap_or_default();
    c.clip = Some([ax, 77.0, ax + 354.0, top]);
    for (i, cm) in list.iter().enumerate() {
        let y = top - 44.0 * (i as f32 + 1.0) + s.scroll;
        if y + 44.0 < 77.0 || y > top {
            continue;
        }
        let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
        c.rect(ax, y, 354.0, 44.0, shade.map(|v| v / 255.0), 1.0);
        c.text("goldFont", &name, ax + 10.0, y + 33.0, Look::scaled(0.5).anchor(0.0, 0.5));
        if cm.percent > 0 {
            let (uw, _) = c.text_size("goldFont", &name);
            c.text("bigFont", &format!("{}%", cm.percent), ax + 16.0 + uw * 0.5, y + 33.0, Look::scaled(0.35).anchor(0.0, 0.5));
        }
        let (tw, _) = c.text_size("chatFont", &cm.text);
        c.text("chatFont", &cm.text, ax + 10.0, y + 15.0, Look::scaled((330.0 / tw.max(1.0)).min(0.7)).anchor(0.0, 0.5));
        c.text("bigFont", &cm.likes.to_string(), ax + 330.0, y + 33.0, Look::scaled(0.3).anchor(1.0, 0.5));
        c.sprite("GJ_likesIcon_001.png", ax + 342.0, y + 33.0, Look::scaled(0.5));
        c.text("chatFont", &cm.age, ax + 346.0, y + 6.0, Look::scaled(0.5).anchor(1.0, 0.5).rgb([0, 0, 0]).alpha(140));
    }
    c.clip = None;
}

/// `NotificationsPage`: its layout, and the list's state (none, or an error).
fn draw_notifications(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    // the list's sides are drawn with the rows (the capture's right one is flipped in place)
    replay::draw_where(c, ctx, Scene::Notifications, &|k, name, _, _| !(k == 'T' && name == "Something went wrong") && name != "GJ_commentSide_001.png");
    let cx = 284.5 + dx;
    match &s.notifications {
        None => super::online::loading_circle(c, cx, 160.0, s.t),
        Some(Err(())) => {
            c.text("goldFont", "Something went wrong", cx, 160.0, Look::scaled(0.6));
        }
        Some(Ok((list, _))) if list.is_empty() => {
            c.text("goldFont", "No notifications", cx, 160.0, Look::scaled(0.6));
        }
        Some(Ok((list, total))) => {
            // GJNotificationCell (captured with made-up rows): the kind's picture, the title and
            // its age, the body, a green dot when new, the bell (stop notifications from them)
            c.text("goldFont", &format!("1 to {} of {total}", list.len()), 494.5 + dx, 285.0, Look::scaled(0.5).anchor(1.0, 0.5));
            // each kind's EmoteSprite and its scale (captured)
            const PICTURES: [(i32, &str, f32); 6] = [(1, "particle_207_001.png", 1.125), (2, "particle_226_001.png", 1.1666), (3, "particle_62_001.png", 1.575), (4, "particle_222_001.png", 1.3333), (5, "particle_222_001.png", 1.3333), (6, "particle_246_001.png", 1.125)];
            // GJCommentListLayer: 340 × 196 at (114.5, 57), its 40-high rows dark first
            let (lx, ly, top) = (114.5 + dx, 57.0, 253.0);
            let rgb = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
            c.rect(lx, ly, 340.0, 196.0, rgb([191, 114, 62]), 1.0);
            c.clip = Some([lx, ly, lx + 340.0, top]);
            for (i, n) in list.iter().enumerate() {
                let y = 234.0 - 40.0 * i as f32 + s.scroll;
                if !(ly - 20.0..=top + 20.0).contains(&y) {
                    continue;
                }
                c.rect(lx, y - 20.0, 340.0, 40.0, rgb(if i % 2 == 0 { [161, 88, 44] } else { [194, 114, 62] }), 1.0);
                let (_, pic, scale) = PICTURES.iter().find(|p| p.0 == n.kind).copied().unwrap_or(PICTURES[0]);
                c.sprite(pic, 132.5 + dx, y + 1.0, Look::scaled(scale));
                if n.new {
                    c.sprite("d_ball_01_001.png", 120.5 + dx, y + 9.0, Look::scaled(0.214).rgb([0, 255, 0]));
                }
                let (tw, _) = c.text_size("bigFont", &n.title);
                c.text("bigFont", &n.title, 150.5 + dx, y + 8.0, Look::scaled(0.5).anchor(0.0, 0.5));
                if !n.age.is_empty() {
                    c.text("chatFont", &format!("{} ago", n.age), 150.5 + tw * 0.5 + 6.0 + dx, y + 7.0, Look::scaled(0.5).anchor(0.0, 0.5).rgb([0, 0, 0]).alpha(150));
                }
                c.text("chatFont", &n.body, 151.0 + dx, y - 5.5, Look::scaled(0.54).anchor(0.0, 1.0).rgb([0, 0, 0]));
                c.text("chatFont", &n.body, 150.5 + dx, y - 5.0, Look::scaled(0.54).anchor(0.0, 1.0));
                let id = Btn::Social(6000 + i as u16);
                let f = ctx.factor(id);
                c.sprite("accountBtn_notifications_001.png", 380.5 + dx, y, Look::scaled(0.5 * f));
                ctx.area(id, 380.5 + dx, y, 21.5, 20.75);
            }
            c.clip = None;
            for (x, flip) in [(lx - 7.0, false), (lx + 347.0, true)] {
                c.sprite("GJ_commentSide_001.png", x, ly, Look::default().anchor(if flip { 1.0 } else { 0.0 }, 0.0).scale_xy(1.0, 6.3667).flip_x(flip));
            }
            c.sprite("GJ_commentTop_001.png", lx + 170.0, ly + 192.0, Look::default());
            c.sprite("GJ_commentTop_001.png", lx + 170.0, ly + 4.0, Look::default().flip_y(true));
        }
    }
}

/// `MessageThreadsPage` (captured with made-up threads): the page's layout without its rows,
/// the tab icons (the open one in color, the others gray), and the rows: a direct message shows
/// the other player's icon and name, a group its picture and title; then the unread count's
/// green ball, the star, "removed", the age, the muted icon, "..." and the tick box.
fn draw_threads(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32, tab: u8) {
    const TABS: [&str; 3] = ["threadBtn_allOn_001.png", "threadBtn_dm_001.png", "threadBtn_groupOn_001.png"];
    replay::draw_where(c, ctx, Scene::Threads, &|_, name, _, _| !TABS.contains(&name) && name != "GJ_commentSide_001.png");
    for (i, name) in TABS.iter().enumerate() {
        let look = if i as u8 == tab { Look::default() } else { Look::default().gray().rgb([240, 240, 240]).alpha(215) };
        let f = ctx.factor(Btn::Replay(11 + i as u8));
        c.sprite(name, 90.5 + dx, 228.0 - 37.0 * i as f32, look.scale_xy(f, f));
    }
    let (lx, ly, top) = (114.5 + dx, 50.0, 246.0);
    for (x, flip) in [(lx - 7.0, false), (lx + 347.0, true)] {
        c.sprite("GJ_commentSide_001.png", x, ly, Look::default().anchor(if flip { 1.0 } else { 0.0 }, 0.0).scale_xy(1.0, 6.3667).flip_x(flip));
    }
    let list = match &s.threads {
        None => return super::online::loading_circle(c, 284.5 + dx, 148.0, s.t),
        Some(Err(())) => {
            c.text("goldFont", "Something went wrong...", 284.5 + dx, 148.0, Look::scaled(0.6));
            return;
        }
        Some(Ok(l)) => l,
    };
    let (rows, total, invites) = (&list.0, list.1, list.2);
    if invites > 0 {
        c.sprite("d_ball_01_001.png", 491.0 + dx, 240.5, Look::scaled(0.6).rgb([255, 0, 0]));
        c.text("bigFont", &invites.min(99).to_string(), 491.0 + dx, 241.1, Look::scaled(0.48));
    }
    if rows.is_empty() {
        c.text("goldFont", "No messages", 284.5 + dx, 148.0, Look::scaled(0.6));
        return;
    }
    c.text("chatFont", &format!("1 to {} of {total}", rows.len()), 284.5 + dx, 23.0, Look::scaled(0.5).rgb([0, 0, 0]).alpha(150));
    let rgb = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    c.rect(lx, ly, 340.0, 196.0, rgb([191, 114, 62]), 1.0);
    c.clip = Some([lx, ly, lx + 340.0, top]);
    for (i, t) in rows.iter().enumerate() {
        let y = 228.0 - 40.0 * i as f32 + s.scroll;
        if !(ly - 20.0..=top + 20.0).contains(&y) {
            continue;
        }
        c.rect(lx, y - 20.0, 340.0, 40.0, rgb(if i % 2 == 0 { [161, 88, 44] } else { [194, 114, 62] }), 1.0);
        ctx.area(Btn::Social(7200 + i as u16), lx + 170.0, y, 340.0, 40.0);
        // the name (a direct message) or the title, then what follows it on the line
        let mut x;
        if t.direct {
            let base = super::garage::icon_sheet((t.icon_type as usize).min(8), t.icon.max(1) as u32);
            super::garage::draw_icon(c, &base, 136.5 + dx, y, 0.5, super::menu::color_for_idx(t.color1.max(0) as usize), super::menu::color_for_idx(t.color2.max(0) as usize));
            let w = c.text_size("goldFont", &t.name).0 * 0.5;
            let (cx, id) = (153.56 + dx + w * 0.5, Btn::Social(7000 + i as u16));
            let f = ctx.factor(id);
            c.text("goldFont", &t.name, cx, y, Look::scaled(0.5 * f));
            ctx.area(id, cx, y, w * 1.2, 18.2);
            x = cx + w * 0.5 + 1.0;
        } else {
            c.sprite("particle_52_001.png", 136.5 + dx, y, Look::scaled(1.0125));
            // the title, fitted to about 92 points (the capture's 0.3845 for "Group Title B")
            let full = c.text_size("bigFont", &t.title).0;
            let scale = (92.37 / full.max(1.0)).min(0.45);
            c.text("bigFont", &t.title, 152.5 + dx, y, Look::scaled(scale).anchor(0.0, 0.5));
            x = 152.5 + dx + full * scale;
            if t.described {
                c.sprite("GJ_infoIcon_001.png", x + 14.6, y, Look::scaled(0.4));
                x += 27.6;
            }
        }
        if t.status == 8 {
            let w = c.text_size("chatFont", "removed").0 * 0.45;
            c.text("chatFont", "removed", x, y, Look::scaled(0.45).anchor(0.0, 0.5).rgb([255, 90, 90]));
            x += w;
        }
        c.text("chatFont", &format!("{} ago", t.age), x + 6.06, y, Look::scaled(0.5).anchor(0.0, 0.5).rgb([0, 0, 0]).alpha(150));
        if t.starred {
            c.sprite("GJ_starsIcon_001.png", 121.5 + dx, y + 13.0, Look::scaled(0.35));
        }
        if t.unread > 0 {
            let n = t.unread.min(99).to_string();
            let k = if t.unread < 10 { 0.64 } else { 0.504 };
            c.sprite("d_ball_01_001.png", 147.5 + dx, y + 12.0, Look::scaled(0.32).rgb([0, 255, 0]));
            c.text("chatFont", &n, 147.82 + dx, y + 12.32, Look::scaled(k).rgb([0, 0, 0]).alpha(125));
            c.text("chatFont", &n, 147.5 + dx, y + 12.64, Look::scaled(k).rgb([0, 0, 0]));
        }
        if t.muted {
            c.sprite("GJ_musicOffBtn_001.png", 408.5 + dx, y + 10.0, Look::scaled(0.32).alpha(140));
        }
        let more = Btn::Social(7600 + i as u16);
        c.text("bigFont", "...", 408.5 + dx, y + 2.0, Look::scaled(0.45 * ctx.factor(more)).alpha(120));
        ctx.area(more, 408.5 + dx, y + 2.0, 36.0, 34.0);
        let tick = Btn::Social(7100 + i as u16);
        let on = s.ticked.contains(&t.id);
        c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, 432.5 + dx, y, Look::scaled(0.63 * ctx.factor(tick)));
        ctx.area(tick, 432.5 + dx, y, 19.53, 19.53);
    }
    c.clip = None;
    c.sprite("GJ_commentTop_001.png", lx + 170.0, ly + 192.0, Look::default());
    c.sprite("GJ_commentTop_001.png", lx + 170.0, ly + 4.0, Look::default().flip_y(true));
}

/// A piece of a message as the game shows it: text, a `:name:` emote, or a shared level /
/// list (` l_<id>` / ` li_<id>`, shown as "Level <id>" / "List <id>" in color).
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Emote(u16, f32),
    Share(String),
}

/// A message's words, emotes and shares (emotes may sit inside a word: "hi:fire:").
pub fn message_pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(Piece::Text(" ".into()));
        }
        let share = |p: &str, label: &str| word.strip_prefix(p).filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).map(|n| format!("{label} {n}"));
        if let Some(s) = share("li_", "List").or_else(|| share("l_", "Level")) {
            out.push(Piece::Share(s));
            continue;
        }
        let mut rest = word;
        while !rest.is_empty() {
            let emote = rest.find(':').and_then(|a| rest[a + 1..].find(':').map(|b| (a, a + 1 + b))).and_then(|(a, b)| crate::emotes::find(&rest[a + 1..b]).map(|e| (a, b, e)));
            match emote {
                Some((a, b, (sprite, scale))) => {
                    if a > 0 {
                        out.push(Piece::Text(rest[..a].into()));
                    }
                    out.push(Piece::Emote(sprite, scale));
                    rest = &rest[b + 1..];
                }
                None => {
                    out.push(Piece::Text(rest.into()));
                    break;
                }
            }
        }
    }
    out
}

/// The pieces laid out in lines `max` points wide at chatFont `scale` (an emote takes its own
/// width; the spaces around it stay).
/// A piece's width (spaces measured between letters: a lone space measures nothing).
fn piece_width(c: &Canvas, p: &Piece, scale: f32) -> f32 {
    let space = c.text_size("chatFont", "a a").0 - c.text_size("chatFont", "aa").0;
    match p {
        Piece::Text(t) | Piece::Share(t) => (c.text_size("chatFont", t.trim()).0 + space * (t.len() - t.trim().len()) as f32) * scale,
        // its sprite's width and a little (from the capture: centers 36.2 apart around " and ")
        Piece::Emote(_, s) => 16.0 * 0.7617 * s * scale / 0.75 + 1.0,
    }
}

fn message_lines(c: &Canvas, text: &str, max: f32, scale: f32) -> Vec<Vec<Piece>> {
    let width = |c: &Canvas, p: &Piece| piece_width(c, p, scale);
    let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
    let mut x = 0.0;
    for p in message_pieces(text) {
        let w = width(c, &p);
        let line = lines.last_mut().expect("a line");
        if x + w > max && !line.is_empty() && p != Piece::Text(" ".into()) {
            lines.push(Vec::new());
            x = 0.0;
        }
        if x == 0.0 && p == Piece::Text(" ".into()) {
            continue;
        }
        x += w;
        lines.last_mut().expect("a line").push(p);
    }
    lines
}

fn draw_message_lines(c: &mut Canvas, lines: &[Vec<Piece>], left: f32, top: f32, scale: f32, alpha: u8) {
    for (k, line) in lines.iter().enumerate() {
        let (mut x, y) = (left, top - 15.0 * k as f32);
        for p in line {
            match p {
                Piece::Text(t) => {
                    c.text("chatFont", t, x, y, Look::scaled(scale).anchor(0.0, 1.0).alpha(alpha));
                    x += piece_width(c, p, scale);
                }
                Piece::Share(t) => {
                    c.text("chatFont", t, x, y, Look::scaled(scale).anchor(0.0, 1.0).rgb([255, 200, 80]).alpha(alpha));
                    x += piece_width(c, p, scale);
                }
                Piece::Emote(sprite, s) => {
                    let w = piece_width(c, p, scale);
                    // the captured message: 16-point emotes at 0.7617, 6.75 below the line's top
                    c.sprite(&format!("particle_{sprite}_001.png"), x + w * 0.5, y - 6.75, Look::scaled(0.7617 * s * scale / 0.75).alpha(alpha));
                    x += w;
                }
            }
        }
    }
}

#[cfg(test)]
mod piece_tests {
    use super::Piece;

    #[test]
    fn emotes_and_shares() {
        let p = super::message_pieces("Hello :dizzy: and hi:fire: l_128 li_7 :nope:");
        assert!(p.contains(&Piece::Emote(270, 1.0)));
        assert!(p.contains(&Piece::Text("hi".into())));
        assert!(p.contains(&Piece::Emote(228, 1.0)));
        assert!(p.contains(&Piece::Share("Level 128".into())));
        assert!(p.contains(&Piece::Share("List 7".into())));
        assert!(p.contains(&Piece::Text(":nope:".into())));
    }
}

/// `ThreadViewPopup` (captured with made-up messages): its frame, the thread's name, "Load
/// earlier" when there are older ones, then each day's line and date and the messages (the
/// sender's icon, name and time, the text, "deleted", "..."), and the message box — or the
/// notice saying why you can't write here.
fn draw_thread_view(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    let page = s.thread_page.as_ref().and_then(|r| r.as_ref().ok());
    let open = s.can_send();
    replay::draw_where(c, ctx, Scene::ThreadView, &|k, name, _, y| open || !(y < 60.0 && (k != 'B' || matches!(name, "10" | "11" | "12"))));
    let cx = 284.5 + dx;
    let title = s.viewing.as_ref().map_or(String::new(), |t| if t.direct { t.name.clone() } else { t.title.clone() });
    c.text("goldFont", &title, cx, 284.0, Look::scaled(0.7));
    if open {
        ctx.area(Btn::Social(7400), 283.0 + dx, 37.0, 293.0, 28.0);
    } else if let Some(p) = page {
        let text = if p.locked { "This conversation has been locked." } else { "You can't message this user." };
        let w = c.text_size("goldFont", text).0 * 0.5;
        c.text("goldFont", text, cx, 37.0, Look::scaled(0.5));
        c.sprite("GJ_infoIcon_001.png", cx + w * 0.5 + 10.5, 37.0, Look::scaled(0.5));
    }
    let found;
    let page = match &s.thread_search {
        Some((q, results)) => {
            c.text("chatFont", &format!("Search: \"{q}\""), cx, 268.0, Look::scaled(0.5).alpha(170));
            match results {
                None => {
                    super::online::loading_circle(c, cx, 163.0, s.t);
                    return;
                }
                Some(r) if r.is_empty() => {
                    c.text("chatFont", "No messages found", cx, 163.0, Look::scaled(0.7).alpha(150));
                    return;
                }
                Some(r) => {
                    found = social::ThreadPage { messages: r.clone(), users: page.map(|p| p.users.clone()).unwrap_or_default(), ..Default::default() };
                    Some(&found)
                }
            }
        }
        None => page,
    };
    let Some(p) = page else {
        if matches!(s.thread_page, Some(Err(()))) {
            c.text("goldFont", "Something went wrong...", cx, 163.0, Look::scaled(0.6));
        } else {
            super::online::loading_circle(c, cx, 163.0, s.t);
        }
        return;
    };
    let me = crate::gjapi::identity().account.map(|a| (a.account_id, a.name.clone()));
    let offset = crate::clock::local_offset(p.messages.first().map_or(0, |m| m.time));
    c.clip = Some([79.5 + dx, 61.0, 489.5 + dx, 265.0]);
    let mut y = 255.0 + s.scroll;
    if p.earlier {
        let f = ctx.factor(Btn::Social(7401));
        c.nine_slice("GJ_button_05.png", cx, y - 8.0, 106.0 * f, 20.0 * f, Look::default());
        c.text("bigFont", "Load earlier", cx - 1.0, y - 6.0, Look::scaled(0.4 * f));
        ctx.area(Btn::Social(7401), cx, y - 8.0, 106.0, 20.0);
        y -= 26.0;
    }
    let mut day = i64::MIN;
    for (i, m) in p.messages.iter().enumerate() {
        if crate::clock::day(m.time, offset) != day {
            day = crate::clock::day(m.time, offset);
            c.sprite("floorLine_001.png", cx, y, Look::default().scale_xy(0.8784, 1.0).rgb([255, 230, 60]).alpha(125).blend(crate::render::Blend::Additive));
            c.text("chatFont", &format!("- {} -", crate::clock::date_label(m.time, offset)), cx, y - 11.0, Look::scaled(0.5).alpha(150));
            y -= 33.0;
        }
        let user = p.users.iter().find(|u| u.account == m.account);
        // the players' rows, your own name, or (as the game does) the thread's name for the
        // other player of a direct message
        let partner = s.viewing.as_ref().filter(|t| t.direct && t.account == m.account).map(|t| t.name.clone());
        let name = match (user, &me, partner) {
            (Some(u), _, _) if !u.name.is_empty() => u.name.clone(),
            (_, Some((id, n)), _) if *id == m.account => n.clone(),
            (_, _, Some(n)) => n,
            _ => "-".into(),
        };
        match user.filter(|u| u.icon > 0) {
            Some(u) => {
                let base = super::garage::icon_sheet((u.icon_type as usize).min(8), u.icon as u32);
                super::garage::draw_icon(c, &base, 96.5 + dx, y, 0.55, super::menu::color_for_idx(u.color1.max(0) as usize), super::menu::color_for_idx(u.color2.max(0) as usize));
            }
            None => {
                c.sprite("player_01_001.png", 96.5 + dx, y, Look::scaled(0.55).rgb([125, 125, 125]));
                c.sprite("player_01_2_001.png", 96.5 + dx, y, Look::scaled(0.55).rgb([80, 80, 80]));
            }
        }
        let w = c.text_size("bigFont", &name).0 * 0.35;
        let id = Btn::Social(7300 + i as u16);
        c.text("bigFont", &name, 111.5 + dx + w * 0.5, y + 7.0, Look::scaled(0.35 * ctx.factor(id)).rgb([255, 200, 80]));
        ctx.area(id, 111.5 + dx + w * 0.5, y + 7.0, w, 11.38);
        c.text("chatFont", &crate::clock::time_label(m.time, offset), 111.5 + dx + w + 5.0, y + 6.0, Look::scaled(0.5).anchor(0.0, 0.5).alpha(150));
        let lines = message_lines(c, &m.text, 330.0, 0.75);
        // a shared level can be pressed (its line)
        if let Some(k) = lines.iter().position(|l| l.iter().any(|p| matches!(p, Piece::Share(t) if t.starts_with("Level")))) {
            ctx.area(Btn::Social(8600 + i as u16), 111.5 + dx + 165.0, y - 9.5 - 15.0 * k as f32, 330.0, 15.0);
        }
        draw_message_lines(c, &lines, 111.5 + dx, y - 2.0, 0.75, if m.deleted { 115 } else { 255 });
        if m.deleted {
            c.text("bigFont", "deleted", 455.5 + dx, y - 2.0, Look::scaled(0.3).anchor(1.0, 1.0).rgb([255, 120, 120]));
        }
        c.text("bigFont", "...", 469.08 + dx, y - 7.69, Look::scaled(0.35).alpha(120));
        y -= 43.0 + 15.0 * (lines.len().max(1) - 1) as f32;
    }
    c.clip = None;
    if s.emote_picker {
        draw_emote_picker(s, c, ctx, dx);
    }
}

/// The picker's rows: each group starts a row of 8, 26 apart, 12 more between groups.
fn picker_rows() -> Vec<(usize, f32, f32)> {
    let mut out = Vec::new();
    let mut y = 0.0;
    for (g, &(_, first, count)) in crate::emotes::GROUPS.iter().enumerate() {
        if g > 0 {
            y += 12.0;
        }
        for k in 0..count {
            if k > 0 && k % 8 == 0 {
                y += 26.0;
            }
            out.push((first + k, (k % 8) as f32 * 26.0, y));
        }
        y += 26.0;
    }
    out
}

fn picker_height() -> f32 {
    picker_rows().last().map_or(0.0, |r| r.2 + 26.0)
}

/// `EmotePickerPopup` (captured): a gray panel over the message box, the emotes by group at
/// 1.25 × their own scale, scrolled with the wheel.
fn draw_emote_picker(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    let at = |x: f32| x + 38.0 + dx;
    c.nine_slice("GJ_square05.png", at(118.0), 131.5, 224.0, 161.0, Look::default().rgb([150, 150, 150]).alpha(200));
    c.sprite("GJ_deleteIcon_001.png", at(10.0), 208.0, Look::scaled(0.35));
    let total = picker_height();
    if total > 150.0 {
        let h = 150.0 * 150.0 / total;
        let y = 206.0 - h - (150.0 - h) * s.picker_scroll / (total - 150.0);
        c.rect(at(225.0), y, 3.0, h, [1.0; 3], 125.0 / 255.0);
    }
    c.clip = Some([at(8.0), 53.0, at(228.0), 208.0]);
    for (i, x, y) in picker_rows() {
        let y = 193.0 - y + s.picker_scroll;
        if !(40.0..=220.0).contains(&y) {
            continue;
        }
        let (_, sprite, scale) = crate::emotes::EMOTES[i];
        let id = Btn::Social(8200 + i as u16);
        c.sprite(&format!("particle_{sprite}_001.png"), at(27.0 + x), y, Look::scaled(1.25 * scale * ctx.factor(id)));
        if (53.0..=208.0).contains(&y) {
            ctx.area(id, at(27.0 + x), y, 26.0, 26.0);
        }
    }
    c.clip = None;
}

/// `ThreadOptionsPopup` (captured for a direct message and a group): the thread's name, when it
/// was made, and a column of buttons 29 apart; Clear and Leave ask first.
fn draw_thread_options(s: &Social, t: &social::Thread, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 150.0 / 255.0);
    let cx = 284.5 + dx;
    let buttons = Social::option_buttons(t);
    let h = 59.0 + 29.0 * buttons.len() as f32;
    let top = 160.0 + h * 0.5 - 14.0;
    c.nine_slice("GJ_square01.png", cx, 160.0, 240.0, h, Look::default());
    c.text("bigFont", if t.direct { &t.name } else { &t.title }, cx, top, Look::scaled(0.5));
    if t.time > 0 {
        let stamp = crate::clock::stamp_label(t.time, crate::clock::local_offset(t.time));
        c.text("chatFont", &format!("Created: {stamp}"), cx, top - 19.0, Look::scaled(0.6).rgb([0, 0, 0]).alpha(150));
    }
    let close = Btn::Social(7799);
    c.sprite("GJ_closeBtn_001.png", 172.5 + dx, top + 2.0, Look::scaled(0.8 * ctx.factor(close)));
    ctx.area(close, 172.5 + dx, top + 2.0, 36.8, 38.0);
    for (k, (label, sheet, _)) in buttons.iter().enumerate() {
        let y = top - 47.0 - 29.0 * k as f32;
        let id = Btn::Social(7700 + k as u16);
        let f = ctx.factor(id);
        c.nine_slice(sheet, cx, y, 146.0 * f, 22.0 * f, Look::default());
        c.text("bigFont", label, cx - 1.0, y + 2.0, Look::scaled(0.5 * f));
        ctx.area(id, cx, y, 146.0, 22.0);
    }
    if let Some(mode) = s.options_confirm {
        // the game's FLAlertLayer question
        ctx.buttons.clear();
        c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 100.0 / 255.0);
        let (title, text, yes) = if mode == 5 { ("Leave", "Leave this group chat?", "Leave") } else { ("Clear", "Clear this conversation? This can't be undone.", "Clear") };
        c.nine_slice("GJ_square01.png", cx, 160.0, 360.0, 140.0, Look::default());
        c.text("goldFont", title, cx, 205.0, Look::scaled(0.9));
        c.text("chatFont", text, cx, 165.0, Look::scaled(0.8));
        for (id, label, x) in [(Btn::Social(7790), "Cancel", cx - 60.0), (Btn::Social(7791), yes, cx + 60.0)] {
            let f = ctx.factor(id);
            c.nine_slice("GJ_button_01.png", x, 118.0, 100.0 * f, 30.0 * f, Look::default());
            c.text("goldFont", label, x, 120.0, Look::scaled(0.7 * f));
            ctx.area(id, x, 118.0, 100.0, 30.0);
        }
    }
}

/// Whether an invite's sender or group has the search box's text in it (any case).
fn invite_matches(i: &social::Invite, filter: &str) -> bool {
    let f = filter.trim().to_lowercase();
    f.is_empty() || i.name.to_lowercase().contains(&f) || i.title.to_lowercase().contains(&f)
}

/// `InvitesPopup` (captured with a made-up group invite and message request): "Pending" (or
/// "Ignored"), the search box, and rows 54 apart — a group's picture, title and member count,
/// who invited you and when; or a request's sender, "Message Request - … ago" and its message —
/// with Accept and Ignore. Ignore asks first, as the game does.
fn draw_invites(s: &Social, c: &mut Canvas, ctx: &mut Ctx, dx: f32, ignored: bool) {
    let at = |x: f32| x + 38.0 + dx;
    let cx = at(246.5);
    c.nine_slice("GJ_square01.png", cx, 160.0, 340.0, 260.0, Look::default());
    c.nine_slice("square02_small.png", cx, 136.0, 310.0, 180.0, Look::default().rgb([0, 0, 0]).alpha(80));
    c.text("goldFont", if ignored { "Ignored" } else { "Pending" }, cx, 274.0, Look::scaled(0.7));
    c.sprite("GJ_infoIcon_001.png", at(197.5), 274.0, Look::scaled(0.5));
    c.nine_slice("square02b_small.png", at(231.03), 245.0, 200.0, 24.0, Look::default().rgb([93, 52, 31]));
    ctx.area(Btn::Social(7897), at(231.03), 245.0, 200.0, 24.0);
    if s.invite_filter.is_empty() {
        c.text("chatFont", "Search", at(231.03), 245.0, Look::scaled(0.6).rgb([195, 108, 65]));
    } else {
        c.text("chatFont", &s.invite_filter, at(231.03), 245.0, Look::scaled(0.6));
    }
    let clear = Btn::Social(7896);
    c.sprite("GJ_longBtn07_001.png", at(348.5), 244.0, Look::scaled(0.682 * ctx.factor(clear)));
    ctx.area(clear, at(348.5), 244.0, 26.94, 20.8);
    let close = Btn::Social(7899);
    c.sprite("GJ_closeBtn_001.png", at(80.5), 282.0, Look::scaled(0.72 * ctx.factor(close)));
    ctx.area(close, at(80.5), 282.0, 33.12, 34.2);
    let flip = Btn::Social(7898);
    let f = ctx.factor(flip);
    c.nine_slice("GJ_button_04.png", at(380.5), 273.0, 57.6 * f, 17.6 * f, Look::default());
    c.text("bigFont", if ignored { "Pending" } else { "Ignored" }, at(379.7), 274.6, Look::scaled(0.32 * f));
    ctx.area(flip, at(380.5), 273.0, 57.6, 17.6);
    let list: Vec<&social::Invite> = match &s.invites {
        None => return super::online::loading_circle(c, cx, 136.0, s.t),
        Some(Err(())) => {
            c.text("goldFont", "Something went wrong...", cx, 136.0, Look::scaled(0.6));
            return;
        }
        Some(Ok(l)) => l.iter().filter(|i| i.ignored == ignored && invite_matches(i, &s.invite_filter)).collect(),
    };
    if list.is_empty() {
        c.text("chatFont", if ignored { "Nothing ignored" } else { "No invites" }, cx, 136.0, Look::scaled(0.7).alpha(150));
    }
    c.clip = Some([at(91.5), 46.0, at(401.5), 226.0]);
    for (i, inv) in list.iter().enumerate().take(4) {
        let y = 199.0 - 54.0 * i as f32;
        if i > 0 {
            c.rect(at(99.5), y + 27.0, 294.0, 1.0, [0.0; 3], 50.0 / 255.0);
        }
        let name_btn = Btn::Social(7950 + i as u16);
        let icon = |c: &mut Canvas, x: f32, y: f32, scale: f32| {
            let base = super::garage::icon_sheet((inv.icon_type as usize).min(8), inv.icon.max(1) as u32);
            super::garage::draw_icon(c, &base, x, y, scale, super::menu::color_for_idx(inv.color1.max(0) as usize), super::menu::color_for_idx(inv.color2.max(0) as usize));
        };
        if inv.direct {
            icon(c, at(113.5), y, 0.5);
            let w = c.text_size("goldFont", &inv.name).0 * 0.45;
            c.text("goldFont", &inv.name, at(131.5) + w * 0.5, y + 18.0, Look::scaled(0.45 * ctx.factor(name_btn)));
            ctx.area(name_btn, at(131.5) + w * 0.5, y + 18.0, w, 11.7);
            c.text("chatFont", &format!("Message Request - {} ago", inv.age), at(131.5), y + 1.0, Look::scaled(0.5).anchor(0.0, 0.5).alpha(150));
            if !inv.message.is_empty() {
                c.text("chatFont", &format!("\"{}\"", inv.message), at(131.5), y - 15.0, Look::scaled(0.45).anchor(0.0, 0.5).alpha(210));
            }
        } else {
            c.sprite("particle_52_001.png", at(113.5), y, Look::scaled(1.0125));
            let tw = c.text_size("bigFont", &inv.title).0 * 0.45;
            c.text("bigFont", &inv.title, at(131.5), y + 10.0, Look::scaled(0.45).anchor(0.0, 0.5));
            let members = format!("{} Member{}", inv.members, if inv.members == 1 { "" } else { "s" });
            let mw = c.text_size("goldFont", &members).0 * 0.45;
            c.text("goldFont", &members, at(131.5) + tw + 6.0 + mw * 0.5, y + 10.0, Look::scaled(0.45));
            icon(c, at(137.5), y - 9.0, 0.36);
            let w = c.text_size("goldFont", &inv.name).0 * 0.45;
            c.text("goldFont", &inv.name, at(147.5) + w * 0.5, y - 9.0, Look::scaled(0.45 * ctx.factor(name_btn)));
            ctx.area(name_btn, at(147.5) + w * 0.5, y - 9.0, w, 11.7);
            c.text("chatFont", &format!("invited you {} ago", inv.age), at(147.5) + w + 4.0, y - 9.5, Look::scaled(0.5).anchor(0.0, 0.5).alpha(150));
        }
        for (k, (label, sheet)) in [("Accept", "GJ_button_01.png"), ("Ignore", "GJ_button_05.png")].into_iter().enumerate() {
            if ignored && k == 1 {
                continue;
            }
            let id = Btn::Social(7800 + 50 * k as u16 + i as u16);
            let f = ctx.factor(id);
            let by = y + 11.0 - 22.0 * k as f32;
            c.nine_slice(sheet, at(364.5), by, 60.0 * f, 20.0 * f, Look::default());
            c.text("bigFont", label, at(363.5), by + 2.0, Look::scaled(0.35 * f));
            ctx.area(id, at(364.5), by, 60.0, 20.0);
        }
    }
    c.clip = None;
    if s.ignoring.is_some() {
        // the game's question (its FLAlertLayer)
        ctx.buttons.clear();
        c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 100.0 / 255.0);
        c.nine_slice("square01_001.png", cx, 160.0, 380.0, 160.0, Look::default());
        c.text("goldFont", "Ignore", cx, 216.0, Look::scaled(0.9));
        c.text("chatFont", "Ignore this?", cx, 186.0, Look::scaled(0.8));
        c.text("chatFont", "They won't be notified.", cx, 166.0, Look::scaled(0.8).rgb([255, 255, 0]));
        c.text("chatFont", "Change your mind under Ignored.", cx, 146.0, Look::scaled(0.8).rgb([255, 255, 0]));
        for (id, label, x, sheet) in [(Btn::Social(7890), "Back", cx - 70.0, "GJ_button_01.png"), (Btn::Social(7891), "Ignore", cx + 70.0, "GJ_button_05.png")] {
            let f = ctx.factor(id);
            c.nine_slice(sheet, x, 108.0, 116.0 * f, 34.0 * f, Look::default());
            c.text("goldFont", label, x, 110.0, Look::scaled(0.8 * f));
            ctx.area(id, x, 108.0, 116.0, 34.0);
        }
    }
}

/// `MemberListPopup` (captured with made-up members): rows 40 apart — the player's icon, name
/// and stars / moons / demons — with, for the group's owner, a crown (make them owner) and an X
/// (kick; for the invited, "Invited" and an X); then the count of 10, Add Member and who adds.
fn draw_members(s: &Social, t: &social::Thread, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 150.0 / 255.0);
    let at = |x: f32| x + 38.0 + dx;
    let cx = at(246.5);
    c.nine_slice("GJ_square01.png", cx, 160.0, 340.0, 260.0, Look::default());
    c.nine_slice("square02_small.png", cx, 158.0, 310.0, 168.0, Look::default().rgb([0, 0, 0]).alpha(80));
    c.text("goldFont", "Members", cx, 272.0, Look::scaled(0.7));
    let close = Btn::Social(8099);
    c.sprite("GJ_closeBtn_001.png", at(80.5), 282.0, Look::scaled(0.72 * ctx.factor(close)));
    ctx.area(close, at(80.5), 282.0, 33.12, 34.2);
    for (id, x, sheet, label, scale) in [(Btn::Social(8098), 189.5, "GJ_button_01.png", "Add Member", 0.4472), (Btn::Social(8097), 329.5, "GJ_button_04.png", if s.owner_adds { "Adds: Owner" } else { "Adds: Anyone" }, 0.4)] {
        let f = ctx.factor(id);
        c.nine_slice(sheet, at(x), 54.0, 106.0 * f, 26.0 * f, Look::default());
        c.text("bigFont", label, at(x - 1.0), 56.0, Look::scaled(scale * f));
        ctx.area(id, at(x), 54.0, 106.0, 26.0);
    }
    let list = match &s.members {
        None => return super::online::loading_circle(c, cx, 158.0, s.t),
        Some(Err(())) => {
            c.text("chatFont", "Failed to load members", cx, 158.0, Look::scaled(0.6).alpha(150));
            return;
        }
        Some(Ok(l)) => l,
    };
    c.text("goldFont", &format!("{}/10", list.members.len()), at(90.5), 54.0, Look::scaled(0.5).anchor(0.0, 0.5));
    let me = crate::gjapi::identity().account.map_or(0, |a| a.account_id);
    let owner = me == t.owner;
    c.clip = Some([at(91.5), 74.0, at(401.5), 242.0]);
    let stats = |c: &mut Canvas, y: f32, values: [i64; 3]| {
        let mut x = at(137.5);
        for (v, icon) in values.iter().zip(["GJ_starsIcon_001.png", "GJ_moonsIcon_001.png", "GJ_demonIcon_001.png"]) {
            let text = v.to_string();
            let w = c.text_size("bigFont", &text).0 * 0.4;
            c.text("bigFont", &text, x, y, Look::scaled(0.4).anchor(0.0, 0.5));
            c.sprite(icon, x + w + 7.3, y - 1.0, Look::scaled(0.5));
            x += w + 21.2;
        }
    };
    let mut row = 0;
    for (i, u) in list.members.iter().enumerate() {
        let y = 222.0 - 40.0 * row as f32;
        row += 1;
        super::garage::draw_icon(c, &super::garage::icon_sheet((u.icon_type as usize).min(8), u.icon), at(113.5), y, 0.8, super::menu::color_for_idx(u.color1), super::menu::color_for_idx(u.color2));
        let w = c.text_size("goldFont", &u.name).0 * 0.6;
        let name = Btn::Social(8070 + i as u16);
        c.text("goldFont", &u.name, at(137.5) + w * 0.5, y + 9.5, Look::scaled(0.6 * ctx.factor(name)));
        ctx.area(name, at(137.5) + w * 0.5, y + 9.5, w, 15.6);
        stats(c, y - 8.0, [u.stars, u.moons, u.demons]);
        if u.account_id == t.owner {
            c.text("chatFont", "Owner", at(361.5), y, Look::scaled(1.0).anchor(1.0, 0.5).alpha(150));
        } else if owner {
            let (kick, crown) = (Btn::Social(8000 + i as u16), Btn::Social(8050 + i as u16));
            c.sprite("GJ_deleteIcon_001.png", at(383.5), y, Look::scaled(0.7 * ctx.factor(kick)));
            ctx.area(kick, at(383.5), y, 16.8, 16.8);
            c.sprite("particle_313_001.png", at(354.5), y, Look::scaled(1.2 * ctx.factor(crown)));
            ctx.area(crown, at(354.5), y, 19.2, 15.0);
        }
    }
    for (k, (_, name)) in list.invited.iter().enumerate() {
        let y = 222.0 - 40.0 * row as f32;
        row += 1;
        // no icon in an invite's row: the default cube, as the game shows it
        super::garage::draw_icon(c, "player_01", at(113.5), y, 0.8, super::menu::color_for_idx(0), super::menu::color_for_idx(3));
        let w = c.text_size("goldFont", name).0 * 0.6;
        c.text("goldFont", name, at(137.5) + w * 0.5, y + 9.5, Look::scaled(0.6).rgb([175, 175, 175]));
        stats(c, y - 8.0, [0, 0, 0]);
        c.text("chatFont", "Invited", at(361.5), y, Look::scaled(1.0).anchor(1.0, 0.5).alpha(150));
        if owner {
            let kick = Btn::Social(8000 + (list.members.len() + k) as u16);
            c.sprite("GJ_deleteIcon_001.png", at(383.5), y, Look::scaled(0.7 * ctx.factor(kick)));
            ctx.area(kick, at(383.5), y, 16.8, 16.8);
        }
    }
    c.clip = None;
    if let Some((mode, _, name)) = &s.members_confirm {
        // the game's questions (captured)
        ctx.buttons.clear();
        c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 100.0 / 255.0);
        c.nine_slice("square01_001.png", cx, 160.0, 380.0, 150.0, Look::default());
        let (title, yes) = if *mode == 7 { ("Transfer Ownership", "Transfer") } else { ("Kick Member", "Kick") };
        c.text("goldFont", title, cx, 212.0, Look::scaled(0.8));
        let line = if *mode == 7 { format!("Make <cl>{name}</c> the owner of this group chat?") } else { format!("Kick <cl>{name}</c> from this group chat?") };
        let lines = super::canvas::rich_lines(c, "chatFont", &line, 340.0);
        super::canvas::draw_rich(c, "chatFont", &lines, cx, 180.0, 0.8, 18.0, 255);
        if *mode == 7 {
            c.text("chatFont", "Only the owner can kick members.", cx, 160.0, Look::scaled(0.8).rgb([255, 255, 0]));
        }
        for (id, label, x, sheet) in [(Btn::Social(8090), "Back", cx - 70.0, "GJ_button_01.png"), (Btn::Social(8091), yes, cx + 70.0, if *mode == 7 { "GJ_button_01.png" } else { "GJ_button_06.png" })] {
            let f = ctx.factor(id);
            c.nine_slice(sheet, x, 112.0, 116.0 * f, 34.0 * f, Look::default());
            c.text("goldFont", label, x, 114.0, Look::scaled(0.7 * f));
            ctx.area(id, x, 112.0, 116.0, 34.0);
        }
    }
}

/// A thread's report reasons: their ids (as the game sends them) and labels.
const REPORT_REASONS: [(i32, &str); 5] = [(0, "Inappropriate Content"), (1, "Harassment / Hate"), (2, "Spam / Trolling"), (3, "Scam / Phishing"), (7, "Threats / Danger")];

/// `ReportPopup` (captured): "Report", whose, "Select one or more reasons:", the five tick
/// boxes, Report; then the game's "Confirm Report" listing the reasons.
fn draw_report(t: &social::Thread, picked: &[i32], confirm: bool, c: &mut Canvas, ctx: &mut Ctx, dx: f32) {
    c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 150.0 / 255.0);
    let at = |x: f32| x + 38.0 + dx;
    let cx = at(246.5);
    let name = if t.direct { &t.name } else { &t.title };
    c.nine_slice("GJ_square01.png", cx, 160.0, 320.0, 274.0, Look::default());
    c.text("bigFont", "Report", cx, 283.0, Look::scaled(0.5));
    c.text("bigFont", name, cx, 261.0, Look::scaled(0.6));
    c.text("goldFont", "Select one or more reasons:", cx, 231.0, Look::scaled(0.5));
    let close = Btn::Social(8100);
    c.sprite("GJ_closeBtn_001.png", at(94.5), 285.0, Look::scaled(0.8 * ctx.factor(close)));
    ctx.area(close, at(94.5), 285.0, 36.8, 38.0);
    for (i, (id, label)) in REPORT_REASONS.iter().enumerate() {
        let y = 201.0 - 28.0 * i as f32;
        let b = Btn::Social(8101 + i as u16);
        let on = picked.contains(id);
        c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, at(136.5), y, Look::scaled(0.7 * ctx.factor(b)));
        ctx.area(b, at(136.5), y, 24.5, 24.15);
        c.text("bigFont", label, at(152.5), y, Look::scaled(0.5).anchor(0.0, 0.5));
    }
    let go = Btn::Social(8106);
    let f = ctx.factor(go);
    c.nine_slice("GJ_button_06.png", cx, 47.0, 116.0 * f, 30.0 * f, Look::default());
    c.text("bigFont", "Report", cx - 1.0, 49.0, Look::scaled(0.7 * f));
    ctx.area(go, cx, 47.0, 116.0, 30.0);
    if confirm {
        ctx.buttons.clear();
        c.rect(0.0, 0.0, c.width, c.height, [0.0; 3], 100.0 / 255.0);
        let reasons: String = REPORT_REASONS.iter().filter(|r| picked.contains(&r.0)).map(|r| format!("\n- <cy>{}</c>", r.1)).collect();
        let text = format!("Report <cg>{name}</c> for:{reasons}\n\nFalse reports may result in action against your own account.");
        let lines = super::canvas::rich_lines(c, "chatFont", &text, 340.0);
        let h = 110.0 + 18.0 * lines.len() as f32;
        c.nine_slice("square01_001.png", cx, 160.0, 380.0, h, Look::default());
        c.text("goldFont", "Confirm Report", cx, 160.0 + h * 0.5 - 24.0, Look::scaled(0.8));
        super::canvas::draw_rich(c, "chatFont", &lines, cx, 160.0 + h * 0.5 - 52.0, 0.8, 18.0, 255);
        for (id, label, x, sheet) in [(Btn::Social(8190), "Cancel", cx - 70.0, "GJ_button_01.png"), (Btn::Social(8191), "Report", cx + 70.0, "GJ_button_06.png")] {
            let f = ctx.factor(id);
            let y = 160.0 - h * 0.5 + 30.0;
            c.nine_slice(sheet, x, y, 116.0 * f, 34.0 * f, Look::default());
            c.text("goldFont", label, x, y + 2.0, Look::scaled(0.7 * f));
            ctx.area(id, x, y, 116.0, 34.0);
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
    /// A message in this thread (`ThreadViewPopup`'s box).
    pub thread: Option<i64>,
    /// `NewThreadPopup`: the subject is the group's name, the body the message (or the
    /// group's description).
    pub new_thread: Option<NewThread>,
    /// `RenameThreadPopup` for this thread (the body is the name).
    pub rename: Option<i64>,
    /// A search in this thread (the body is the query).
    pub search: Option<i64>,
    /// Friends picked to add to this group (`MemberListPopup`'s Add Member).
    pub add_to: Option<i64>,
    /// A level or list to share in this thread (the body is its ID).
    pub share: Option<i64>,
    /// The invites' search box.
    pub invite_search: bool,
    pub subject: String,
    pub body: String,
    /// The field typed into: 0 subject, 1 body (posts have the body only).
    pub field: u8,
    pub t: f32,
}

impl Writer {
    pub fn new(title: &'static str, to: Option<i64>) -> Writer {
        Writer { title, to, level: None, song_slot: None, thread: None, new_thread: None, rename: None, search: None, add_to: None, share: None, invite_search: false, subject: String::new(), body: String::new(), field: if to.is_some() { 0 } else { 1 }, t: 0.0 }
    }

    fn limit(&self) -> usize {
        match (self.to, self.field) {
            _ if self.thread.is_some() => 300,
            _ if self.rename.is_some() => 20,
            _ if self.search.is_some() => 40,
            _ if self.share.is_some() => 10,
            _ if self.invite_search => 30,
            _ if self.new_thread.is_some() => match (self.new_thread.as_ref().is_some_and(|n| n.group), self.field) {
                (true, 0) => 20,
                (true, _) => 300,
                _ => 500,
            },
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

/// `NewThreadPopup`'s choices: a direct message or a group, who to, and the "To" picker open.
#[derive(Clone, Debug, Default)]
pub struct NewThread {
    pub group: bool,
    pub to: Vec<(i64, String)>,
    pub picking: bool,
}

impl Writer {
    pub fn new_thread() -> Writer {
        let mut w = Writer::new("New Direct Message", None);
        w.new_thread = Some(NewThread::default());
        w
    }

    /// DM ↔ group (the popup's two buttons): a direct message keeps one recipient.
    pub fn set_group(&mut self, group: bool) {
        if let Some(n) = self.new_thread.as_mut() {
            n.group = group;
            n.to.truncate(if group { usize::MAX } else { 1 });
        }
        self.field = 1;
    }

    /// A friend picked in the "To" list: a direct message takes them (and closes the list),
    /// a group adds or removes them.
    pub fn pick(&mut self, account: i64, name: String) {
        let Some(n) = self.new_thread.as_mut() else { return };
        if n.group {
            match n.to.iter().position(|u| u.0 == account) {
                Some(i) => {
                    n.to.remove(i);
                }
                None => n.to.push((account, name)),
            }
        } else {
            n.to = vec![(account, name)];
            n.picking = false;
        }
    }
}

/// `NewThreadPopup` (captured in both modes): title, "To:" and its box, the group's name box,
/// the message (or description) box with the characters left, DM / group, Send — and the
/// friends list when "To" is pressed.
fn draw_new_thread(wr: &Writer, n: &NewThread, friends: Option<&Vec<User>>, c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let dx = (w - 569.0) * 0.5;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    let at = |x: f32| x + 38.0 + dx;
    let brown = Look::default().rgb([93, 52, 31]);
    // adding members to a group: the friends list only
    if wr.add_to.is_none() {
    c.nine_slice("GJ_square01.png", at(246.5), 160.0, 420.0, 230.0, Look::default());
    c.text("goldFont", if n.group { "New Group Chat" } else { "New Direct Message" }, at(246.5), 259.0, Look::scaled(0.7));
    c.sprite("GJ_infoIcon_001.png", at(111.5), 259.0, Look::scaled(0.5));
    c.text("goldFont", "To:", at(66.5), 230.0, Look::scaled(0.5).anchor(0.0, 0.5));
    c.nine_slice("square02b_small.png", at(266.5), 230.0, 320.0, 24.0, brown);
    ctx.area(Btn::WritePick, at(266.5), 230.0, 320.0, 24.0);
    let to = n.to.iter().map(|u| u.1.as_str()).collect::<Vec<_>>().join(", ");
    if to.is_empty() {
        c.text("bigFont", if n.group { "Select users..." } else { "Select user" }, at(114.5), 230.0, Look::scaled(0.45).anchor(0.0, 0.5).alpha(120));
    } else {
        let s = (300.0 / (c.text_size("bigFont", &to).0).max(1.0)).min(0.45);
        c.text("bigFont", &to, at(114.5), 230.0, Look::scaled(s).anchor(0.0, 0.5));
    }
    let caret = (wr.t * 2.0).fract() < 0.5;
    let left = |len: usize, max: usize| max.saturating_sub(len).to_string();
    let (box_y, box_top) = if n.group { (131.0, 169.0) } else { (154.0, 192.0) };
    if n.group {
        c.nine_slice("square02b_small.png", at(246.5), 197.0, 360.0, 26.0, brown);
        ctx.area(Btn::WriteField(0), at(246.5), 197.0, 360.0, 26.0);
        if wr.subject.is_empty() {
            c.text("chatFont", "Group name", at(231.5), 197.0, Look::scaled(0.7).rgb([195, 108, 65]));
        } else {
            c.text("chatFont", &wr.subject, at(231.5), 197.0, Look::scaled(0.7));
        }
        if wr.field == 0 && caret {
            c.rect(at(231.5) + c.text_size("chatFont", &wr.subject).0 * 0.35 + 2.0, 190.0, 1.5, 14.0, [1.0; 3], 1.0);
        }
        c.text("chatFont", &left(wr.subject.chars().count(), 20), at(416.5), 197.0, Look::scaled(1.0).anchor(1.0, 0.5).alpha(100));
    }
    c.nine_slice("square02b_001.png", at(246.5), box_y, 360.0, 90.0, brown);
    ctx.area(Btn::WriteField(1), at(246.5), box_y, 360.0, 90.0);
    if wr.body.is_empty() {
        c.text("chatFont", if n.group { "Description" } else { "Message" }, at(76.5), box_top, Look::scaled(0.75).anchor(0.0, 1.0).rgb([195, 108, 65]));
    } else {
        for (k, line) in super::wrap(c, &wr.body, 340.0, 0.75 / 1.6).iter().enumerate().take(5) {
            c.text("chatFont", line, at(76.5), box_top - 15.0 * k as f32, Look::scaled(0.75).anchor(0.0, 1.0));
        }
    }
    c.text("chatFont", &left(wr.body.chars().count(), if n.group { 300 } else { 500 }), at(446.5), 261.0, Look::scaled(1.0).anchor(1.0, 0.5).alpha(100));
    let close = ctx.factor(Btn::WriteCancel);
    c.sprite("GJ_closeBtn_001.png", at(44.5), 267.0, Look::scaled(0.8 * close));
    ctx.area(Btn::WriteCancel, at(44.5), 267.0, 36.8, 38.0);
    for (i, (name, group)) in [("threadBtn_dm_001.png", false), ("threadBtn_groupOn_001.png", true)].into_iter().enumerate() {
        let id = Btn::WriteMode(group as u8);
        let look = if n.group == group { Look::scaled(0.9 * ctx.factor(id)) } else { Look::scaled(0.9 * ctx.factor(id)).gray().rgb([240, 240, 240]).alpha(215) };
        c.sprite(name, at(66.5 + 38.0 * i as f32), 67.0, look);
        ctx.area(id, at(66.5 + 38.0 * i as f32), 67.0, 31.5, 31.5);
    }
    if n.group {
        c.nine_slice("GJ_button_04.png", at(142.5), 67.0, 26.1, 26.1, Look::default());
        c.sprite("particle_52_001.png", at(142.5), 67.0, Look::scaled(1.0125));
    } else {
        c.sprite("GJ_plusBtn_001.png", at(432.5), 67.0, Look::scaled(0.5));
    }
    c.sprite("particle_53_001.png", at(399.5), 67.0, Look::scaled(1.125));
    let f = ctx.factor(Btn::WriteSend);
    c.nine_slice("GJ_button_01.png", at(246.5), 67.0, 96.0 * f, 28.0 * f, Look::default());
    c.text("bigFont", "Send", at(245.5), 69.0, Look::scaled(0.6 * f));
    ctx.area(Btn::WriteSend, at(246.5), 67.0, 96.0, 28.0);
    }
    if n.picking {
        // the friends to send to (FriendPickerPopup's choice, as a plain list)
        ctx.buttons.clear();
        c.rect(0.0, 0.0, w, h, [0.0; 3], 100.0 / 255.0);
        let cx = 284.5 + dx;
        c.nine_slice("GJ_square01.png", cx, 160.0, 300.0, 240.0, Look::default());
        c.text("goldFont", if n.group { "Select users" } else { "Select user" }, cx, 262.0, Look::scaled(0.7));
        match friends {
            None => super::online::loading_circle(c, cx, 160.0, wr.t),
            Some(l) if l.is_empty() => {
                c.text("goldFont", "No friends", cx, 160.0, Look::scaled(0.6));
            }
            Some(l) => {
                for (i, u) in l.iter().enumerate().take(8) {
                    let y = 230.0 - 22.0 * i as f32;
                    let id = Btn::WritePickRow(i as u16);
                    let on = n.to.iter().any(|t| t.0 == u.account_id);
                    c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, cx - 120.0, y, Look::scaled(0.5));
                    c.text("goldFont", &u.name, cx - 105.0, y, Look::scaled(0.5 * ctx.factor(id)).anchor(0.0, 0.5));
                    ctx.area(id, cx, y, 280.0, 20.0);
                }
            }
        }
        let ok = ctx.factor(Btn::WritePick);
        c.nine_slice("GJ_button_01.png", cx, 55.0, 60.0 * ok, 28.0 * ok, Look::default());
        c.text("goldFont", "OK", cx, 57.0, Look::scaled(0.7 * ok));
        ctx.area(Btn::WritePick, cx, 55.0, 60.0, 28.0);
    }
}

pub fn draw_writer_for(wr: &Writer, friends: Option<&Vec<User>>, c: &mut Canvas, ctx: &mut Ctx) {
    match &wr.new_thread {
        Some(n) => draw_new_thread(wr, n, friends, c, ctx),
        None if wr.rename.is_some() || wr.search.is_some() || wr.share.is_some() || wr.invite_search => draw_rename(wr, c, ctx),
        None => draw_writer(wr, c, ctx),
    }
}

/// `RenameThreadPopup` (captured): "Rename Group Chat", "Everyone sees this name", the name, Save.
fn draw_rename(wr: &Writer, c: &mut Canvas, ctx: &mut Ctx) {
    let (w, h) = (c.width, c.height);
    let cx = w * 0.5;
    c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
    c.nine_slice("GJ_square01.png", cx, 160.0, 300.0, 140.0, Look::default());
    let (title, note, button) = if wr.search.is_some() {
        ("Search Messages", "Words in a message", "Search")
    } else if wr.share.is_some() {
        ("Share", "A level or list ID", "Level")
    } else if wr.invite_search {
        ("Search", "A name or group title", "Search")
    } else {
        ("Rename Group Chat", "Everyone sees this name", "Save")
    };
    c.text("bigFont", title, cx, 216.0, Look::scaled(0.5));
    c.text("chatFont", note, cx, 192.0, Look::scaled(0.5).alpha(160));
    c.nine_slice("square02b_small.png", cx, 162.0, 250.0, 26.0, Look::default().rgb([93, 52, 31]));
    ctx.area(Btn::WriteField(1), cx, 162.0, 250.0, 26.0);
    c.text("chatFont", &wr.body, cx, 162.0, Look::scaled(0.7));
    if (wr.t * 2.0).fract() < 0.5 {
        c.rect(cx + c.text_size("chatFont", &wr.body).0 * 0.35 + 2.0, 155.0, 1.5, 14.0, [1.0; 3], 1.0);
    }
    let close = ctx.factor(Btn::WriteCancel);
    c.sprite("GJ_closeBtn_001.png", cx - 142.0, 218.0, Look::scaled(0.8 * close));
    ctx.area(Btn::WriteCancel, cx - 142.0, 218.0, 36.8, 38.0);
    // sharing has two: a level (Send's place) or a list
    let bx = if wr.share.is_some() { cx - 50.0 } else { cx };
    let f = ctx.factor(Btn::WriteSend);
    c.nine_slice("GJ_button_01.png", bx, 112.0, 86.0 * f, 28.0 * f, Look::default());
    c.text("bigFont", button, bx - 1.0, 114.0, Look::scaled(0.6 * f));
    ctx.area(Btn::WriteSend, bx, 112.0, 86.0, 28.0);
    if wr.share.is_some() {
        let list = Btn::WriteMode(9);
        let f = ctx.factor(list);
        c.nine_slice("GJ_button_01.png", cx + 50.0, 112.0, 86.0 * f, 28.0 * f, Look::default());
        c.text("bigFont", "List", cx + 49.0, 114.0, Look::scaled(0.6 * f));
        ctx.area(list, cx + 50.0, 112.0, 86.0, 28.0);
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
        vec![(1, cy + 10.0, 110.0, &wr.body, if wr.thread.is_some() { "Message" } else { "Write something..." })]
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
    let send = if wr.to.is_some() || wr.thread.is_some() { "Send" } else { "Post" };
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
        Page::Invites(_) => s.invites.iter().flatten().flatten().map(|i| super::garage::icon_sheet((i.icon_type as usize).min(8), i.icon.max(1) as u32)).collect(),
        Page::ThreadView => s.thread_page.iter().flatten().flat_map(|p| p.users.iter()).map(|u| super::garage::icon_sheet((u.icon_type as usize).min(8), u.icon.max(1) as u32)).collect(),
        Page::Threads(_) => s.threads.iter().flatten().flat_map(|(l, _, _)| l.iter()).filter(|t| t.direct).map(|t| super::garage::icon_sheet((t.icon_type as usize).min(8), t.icon.max(1) as u32)).collect(),
        Page::Settings | Page::Messages(_) | Page::Notifications | Page::History => Vec::new(),
    };
    if s.members_of.is_some() {
        v.extend(s.members.iter().flatten().flat_map(|m| m.members.iter()).map(|u| super::garage::icon_sheet((u.icon_type as usize).min(8), u.icon)));
    }
    v.sort();
    v.dedup();
    v
}

//! Your account's own pages: your profile and posts, friends, blocked players, friend requests,
//! messages and the account settings (`ProfilePage` with your own account, `FriendsProfilePage`,
//! `FRequestProfilePage`, `GJAccountSettingsLayer`, the legacy `MessagesProfilePage`).
//!
//! The requests are the game's (bodies recorded from it, see `gjapi`'s `recorded_forms`); the
//! answers are the servers' `key:value` lists, `|` between entries and `#` before the page info.
//! Opening your own profile makes the game upload your stats (`updateGJUserScore22.php`);
//! Cadence doesn't: it doesn't keep all of them (stars from online levels, for one), and sending
//! its own would lower your stats on the leaderboards.

use std::collections::HashMap;

use crate::gjapi::{self, Identity};

/// A player in a list (`GJUserScore` from `getGJUserList20` / `getGJFriendRequests20` /
/// `getGJUsers20`): 1 name, 2 player id, 9 icon, 10/11 colors, 14 icon type, 15 glow,
/// 16 account id, 3 stars, 52 moons, 4 demons, 41 new; requests add 32 request id, 35 text,
/// 37 age.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct User {
    pub name: String,
    pub user_id: i64,
    pub account_id: i64,
    pub icon: u32,
    pub icon_type: u32,
    pub color1: usize,
    pub color2: usize,
    pub glow: bool,
    pub stars: i64,
    pub moons: i64,
    pub demons: i64,
    pub new: bool,
    pub request_id: i64,
    pub text: String,
    pub age: String,
}

/// A profile post (`getGJAccountComments20`: 2 text (base64), 4 likes, 9 age, 6 id).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Post {
    pub id: i64,
    pub text: String,
    pub likes: i64,
    pub age: String,
}

/// Your profile's counts (`getGJUserInfo20` with notifMask): 38 new messages, 39 new friend
/// requests, 40 new friends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counts {
    pub messages: i64,
    pub requests: i64,
    pub friends: i64,
    /// The account settings (keys 18, 19, 50 and the social links).
    pub settings: crate::gjapi::AccountSettings,
}

fn pairs(text: &str, sep: char) -> HashMap<String, String> {
    let parts: Vec<&str> = text.split(sep).collect();
    parts.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())).collect()
}

fn b64_text(s: &str) -> String {
    gjapi::b64_decode(s).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// The entries before the page info, `|` apart.
fn entries(body: &str) -> impl Iterator<Item = &str> {
    body.split('#').next().unwrap_or("").split('|').filter(|e| !e.is_empty())
}

pub fn parse_users(body: &str) -> Vec<User> {
    entries(body)
        .map(|e| {
            let m = pairs(e, ':');
            let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            User {
                name: m.get("1").cloned().unwrap_or_default(),
                user_id: int("2"),
                account_id: int("16"),
                icon: int("9").max(1) as u32,
                icon_type: int("14") as u32,
                color1: int("10") as usize,
                color2: int("11") as usize,
                glow: int("15") > 0,
                stars: int("3"),
                moons: int("52"),
                demons: int("4"),
                new: int("41") > 0,
                request_id: int("32"),
                text: m.get("35").map(|t| b64_text(t)).unwrap_or_default(),
                age: m.get("37").cloned().unwrap_or_default(),
            }
        })
        .collect()
}

pub fn parse_posts(body: &str) -> Vec<Post> {
    entries(body)
        .map(|e| {
            let m = pairs(e, '~');
            Post {
                id: m.get("6").and_then(|v| v.parse().ok()).unwrap_or(0),
                text: b64_text(m.get("2").map_or("", |s| s.as_str())),
                likes: m.get("4").and_then(|v| v.parse().ok()).unwrap_or(0),
                age: m.get("9").cloned().unwrap_or_default(),
            }
        })
        .collect()
}

/// The account's settings as its profile answer has them (18 messages: 0 all, 1 friends,
/// 2 off; 19 friend requests: 0 on, 1 off; 50 comment history: 0 all, 1 friends, 2 me),
/// as the settings page's choices (Direct messages: Friends / Off; Friend requests:
/// messages / On / Off; Comment history: All / Friends / Me).
pub fn parse_settings(user_info: &str) -> crate::gjapi::AccountSettings {
    let m = pairs(user_info, ':');
    let int = |k: &str| m.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    crate::gjapi::AccountSettings {
        messages: (int("18") == 2) as i32,
        friend_requests: if int("19") == 1 { 2 } else { 1 },
        comment_history: int("50").clamp(0, 2),
        youtube: m.get("20").cloned().unwrap_or_default(),
        twitter: m.get("44").cloned().unwrap_or_default(),
        twitch: m.get("45").cloned().unwrap_or_default(),
        ..Default::default()
    }
}

/// A message in the legacy inbox (`getGJMessages20`: 6 name, 3 player id, 2 account id,
/// 1 message id, 4 subject (base64), 8 read, 9 sent by you, 7 age; `downloadGJMessage20` adds
/// 5, the body: base64 of the text XORed with "14251").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub id: i64,
    pub name: String,
    pub account_id: i64,
    pub subject: String,
    pub body: String,
    pub read: bool,
    pub sent: bool,
    pub age: String,
}

pub fn parse_messages(body: &str) -> Vec<Message> {
    entries(body)
        .map(|e| {
            let m = pairs(e, ':');
            let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let text = m.get("5").and_then(|b| crate::gjapi::b64_decode(b)).map(|raw| String::from_utf8_lossy(&crate::gjapi::xor_cycle(&raw, "14251")).into_owned()).unwrap_or_default();
            Message {
                id: int("1"),
                name: m.get("6").cloned().unwrap_or_default(),
                account_id: int("2"),
                subject: b64_text(m.get("4").map_or("", |s| s.as_str())),
                body: text,
                read: int("8") > 0,
                sent: int("9") > 0,
                age: m.get("7").cloned().unwrap_or_default(),
            }
        })
        .collect()
}

/// The legacy inbox, or (`sent`) the messages you sent.
pub async fn messages(sent: bool, page: u32) -> Result<Vec<Message>, String> {
    let id = me()?;
    let tail = if sent { "&getSent=1" } else { "" };
    let body = crate::net::post("getGJMessages20.php", &format!("{}&page={page}&total=0&secret={}{tail}", id.base(), crate::gjapi::SECRET)).await?;
    Ok(ok(body).map(|b| parse_messages(&b)).unwrap_or_default())
}

/// One message with its body (marks it read).
pub async fn message(message_id: i64, sent: bool) -> Result<Message, String> {
    let id = me()?;
    let tail = if sent { "&isSender=1" } else { "" };
    let body = ok(crate::net::post("downloadGJMessage20.php", &format!("{}&messageID={message_id}&secret={}{tail}", id.base(), crate::gjapi::SECRET)).await?)?;
    parse_messages(&body).into_iter().next().ok_or_else(|| "no message".into())
}

pub fn parse_counts(user_info: &str) -> Counts {
    let m = pairs(user_info, ':');
    let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    Counts { messages: int("38"), requests: int("39"), friends: int("40"), settings: Default::default() }
}

/// A server answer that is a bare negative code (or nothing) is an error.
fn ok(body: String) -> Result<String, String> {
    let t = body.trim();
    if t.is_empty() || (t.starts_with('-') && t[1..].bytes().all(|b| b.is_ascii_digit())) {
        return Err(t.to_string());
    }
    Ok(body)
}

fn me() -> Result<Identity, String> {
    let id = gjapi::identity();
    if id.account.is_none() {
        return Err("not logged in".into());
    }
    Ok(id)
}

/// Your own profile: its fields (as `online::Profile`) and the counts.
pub async fn own_profile() -> Result<(crate::online::Profile, Counts, Vec<Post>), String> {
    let id = me()?;
    let account = id.account.as_ref().map_or(0, |a| a.account_id);
    let body = ok(crate::net::post("getGJUserInfo20.php", &gjapi::user_info_form(&id, account)).await?)?;
    let counts = Counts { settings: parse_settings(&body), ..parse_counts(&body) };
    let profile = crate::online::profile_from(&body);
    let posts = posts(account, 0).await.unwrap_or_default();
    Ok((profile, counts, posts))
}

/// A player's posts, page `page`.
pub async fn posts(account: i64, page: u32) -> Result<Vec<Post>, String> {
    let id = gjapi::identity();
    let body = crate::net::post("getGJAccountComments20.php", &format!("{}&accountID={account}&page={page}&total=0&secret={}", id.base(), gjapi::SECRET)).await?;
    Ok(ok(body).map(|b| parse_posts(&b)).unwrap_or_default())
}

/// Friends (0) or blocked players (1). "-2" means an empty list.
pub async fn user_list(kind: i32) -> Result<Vec<User>, String> {
    let id = me()?;
    let body = crate::net::post("getGJUserList20.php", &gjapi::user_list_form(&id, kind)).await?;
    Ok(ok(body).map(|b| parse_users(&b)).unwrap_or_default())
}

/// Friend requests received, or (`sent`) sent.
pub async fn requests(sent: bool) -> Result<Vec<User>, String> {
    let id = me()?;
    let body = crate::net::post("getGJFriendRequests20.php", &gjapi::friend_requests_form(&id, 0, sent)).await?;
    Ok(ok(body).map(|b| parse_users(&b)).unwrap_or_default())
}

/// One of the account's actions; `Ok` when the server answers 1.
async fn action(endpoint: &str, body: Option<String>) -> Result<(), String> {
    let body = body.ok_or("not logged in")?;
    let answer = crate::net::post(endpoint, &body).await?;
    match answer.trim() {
        "1" => Ok(()),
        other => Err(other.to_string()),
    }
}

/// What a page asks the servers to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Post(String),
    DeletePost(i64),
    LikePost { id: i64, owner: i64, like: bool },
    SendRequest { to: i64, text: String },
    Accept { user: i64, request: i64 },
    DeleteRequest { user: i64, sent: bool },
    Unfriend(i64),
    Block(i64),
    Unblock(i64),
    /// followGJUser / unfollowGJUser
    Follow { user: i64, on: bool },
    /// muteGJUser (`&mute=1` / 0)
    Mute { user: i64, on: bool },
    Message { to: i64, subject: String, body: String },
    DeleteMessage { id: i64, sent: bool },
    Settings(gjapi::AccountSettings),
}

pub async fn act(a: Act) -> Result<(), String> {
    let id = me()?;
    match a {
        Act::Post(text) => action("uploadGJAccComment20.php", gjapi::account_comment_form(&id, &text)).await,
        Act::DeletePost(c) => action("deleteGJAccComment20.php", Some(gjapi::delete_account_comment_form(&id, c))).await,
        Act::LikePost { id: item, owner, like } => action("likeGJItem211.php", Some(gjapi::like_form(&id, item, like, 3, owner, &gjapi::random_string(10)))).await,
        Act::SendRequest { to, text } => action("uploadFriendRequest20.php", Some(gjapi::friend_request_form(&id, to, &text))).await,
        Act::Accept { user, request } => action("acceptGJFriendRequest20.php", Some(gjapi::accept_request_form(&id, user, request))).await,
        Act::DeleteRequest { user, sent } => action("deleteGJFriendRequests20.php", Some(gjapi::delete_request_form(&id, user, sent))).await,
        Act::Unfriend(u) => action("removeGJFriend20.php", Some(gjapi::target_form(&id, u))).await,
        Act::Block(u) => action("blockGJUser20.php", Some(gjapi::target_form(&id, u))).await,
        Act::Unblock(u) => action("unblockGJUser20.php", Some(gjapi::target_form(&id, u))).await,
        Act::Follow { user, on } => action(if on { "followGJUser.php" } else { "unfollowGJUser.php" }, Some(gjapi::target_form(&id, user))).await,
        Act::Mute { user, on } => action("muteGJUser.php", Some(format!("{}&targetAccountID={user}&mute={}&secret={}", id.base(), on as i32, gjapi::SECRET))).await,
        Act::DeleteMessage { id: m, sent } => action("deleteGJMessages20.php", Some(format!("{}&messageID={m}&secret={}{}", id.base(), crate::gjapi::SECRET, if sent { "&isSender=1" } else { "" }))).await,
        Act::Message { to, subject, body } => action("uploadGJMessage20.php", Some(gjapi::message_form(&id, to, &subject, &body))).await,
        // the account server (accounts/…) answers 1 too
        Act::Settings(s) => action("updateGJAccSettings20.php", gjapi::account_settings_form(&id, &s)).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_servers_lists() {
        let users = parse_users("1:FriendOne:2:11:9:4:10:1:11:3:14:0:15:0:16:21:18:0:41:1|1:FriendTwo:2:12:9:20:10:5:11:9:14:0:15:2:16:22:18:0:41:0#2:0:20");
        assert_eq!(users.len(), 2);
        assert_eq!((users[0].name.as_str(), users[0].account_id, users[0].icon, users[0].new), ("FriendOne", 21, 4, true));
        assert!(users[1].glow && !users[1].new);
        let reqs = parse_users("1:NewFriend:2:13:9:7:10:2:11:6:14:0:15:0:16:23:32:501:35:aGkh:41:1:37:2 hours#1:0:20");
        assert_eq!((reqs[0].request_id, reqs[0].text.as_str(), reqs[0].age.as_str()), (501, "hi!", "2 hours"));
        let posts = parse_posts("2~SGVsbG8gZnJvbSBDYWRlbmNl~4~5~9~2 days~6~101|2~U2Vjb25kIHBvc3Q=~4~0~9~5 hours~6~102#2:0:10");
        assert_eq!((posts[0].text.as_str(), posts[0].likes, posts[0].id), ("Hello from Cadence", 5, 101));
        let c = parse_counts("1:Me:38:2:39:1:40:0");
        assert_eq!((c.messages, c.requests, c.friends), (2, 1, 0));
        let st = parse_settings("1:Me:18:2:19:0:50:1");
        assert_eq!((st.messages, st.friend_requests, st.comment_history), (1, 1, 1));
        assert!(ok("-2".into()).is_err() && ok("1:a".into()).is_ok());
        let body = crate::gjapi::b64(&crate::gjapi::xor_cycle(b"Body text", "14251"));
        let msgs = parse_messages(&format!("6:FriendOne:3:11:2:21:1:1001:4:TmljZSBsZXZlbCE=:8:0:9:0:7:1 day:5:{body}#1:0:50"));
        assert_eq!((msgs[0].name.as_str(), msgs[0].subject.as_str(), msgs[0].body.as_str(), msgs[0].read), ("FriendOne", "Nice level!", "Body text", false));
    }
}

/// A notification (`getGJNotifications.php`, the game's parser at 0x100493430: rows split by
/// `|`, each `key:value` pairs, then `#total:offset:count`): 1 id, 2 type, 3 the player's name,
/// 4 title (base64), 5 body (base64), 6 age, 7 new, 8 the player's account, 9 the target (level,
/// list…), 10 a picture, 11–15 numbers, 16 text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notification {
    pub id: i64,
    pub kind: i32,
    pub user: String,
    pub title: String,
    pub body: String,
    pub age: String,
    pub new: bool,
    pub account: i64,
    pub target: i64,
    pub picture: String,
}

pub fn parse_notifications(body: &str) -> (Vec<Notification>, usize) {
    let mut parts = body.split('#');
    let rows = parts.next().unwrap_or("");
    let total = parts.next().and_then(|p| p.split(':').next()).and_then(|t| t.parse().ok()).unwrap_or(0);
    let list: Vec<Notification> = rows
        .split('|')
        .filter(|r| !r.is_empty())
        .map(|r| {
            let w: Vec<&str> = r.split(':').collect();
            let get = |k: &str| w.chunks(2).find(|c| c[0] == k).and_then(|c| c.get(1)).copied().unwrap_or("");
            let dec = |v: &str| gjapi::b64_decode(v).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            Notification {
                id: get("1").parse().unwrap_or(0),
                kind: get("2").parse().unwrap_or(0),
                user: get("3").into(),
                title: dec(get("4")),
                body: dec(get("5")),
                age: get("6").into(),
                new: get("7") == "1",
                account: get("8").parse().unwrap_or(0),
                target: get("9").parse().unwrap_or(0),
                picture: get("10").into(),
            }
        })
        .collect();
    let total = total.max(list.len());
    (list, total)
}

#[cfg(test)]
mod notification_tests {
    #[test]
    fn rows_like_the_game() {
        let (n, total) = super::parse_notifications("1:900:2:1:3:Alpha:4:VGl0bGUgb25l:5:Qm9keSB0ZXh0IG9uZQ==:6:2 hours:7:0:8:21:9:128:10:s10|1:901:2:2:3:Beta:4:VHdv:5:Qm9keQ==:6:1 day:7:1:8:22:9:129#6:0:10");
        assert_eq!(total, 6);
        assert_eq!(n.len(), 2);
        assert_eq!((n[0].title.as_str(), n[0].body.as_str(), n[0].age.as_str(), n[0].new), ("Title one", "Body text one", "2 hours", false));
        assert!(n[1].new);
        assert_eq!(n[1].account, 22);
    }
}

/// A message thread (`GJMessageThread`, from `getGJThreads.php`; the game's parser,
/// GameLevelManager::onGetThreadsCompleted, reads each `|` row's `:` fields by position).
/// Fields mapped from a capture of the real MessageThreadsPage with made-up rows: 0 id, 1 the
/// other player's account, 2 their name (base64), 3 the group's title (base64), 4 unread, 6 age,
/// 8 the last message (base64), 10 muted, 12 a direct message (not a group), 13 has a
/// description, 14 starred, 15–20 the other player's icon (icon, colors, glow color, kind,
/// glow 2), 23 a time stamp, 25 your membership (8 removed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    pub id: i64,
    pub account: i64,
    pub name: String,
    pub title: String,
    pub unread: i32,
    pub age: String,
    pub last: String,
    pub muted: bool,
    pub direct: bool,
    pub described: bool,
    pub starred: bool,
    pub icon: i32,
    pub color1: i32,
    pub color2: i32,
    pub glow_color: i32,
    pub icon_type: i32,
    pub glow: bool,
    pub time: i64,
    pub status: i32,
}

/// The thread list's answer: the threads, the total, and the invites waiting (the `#` part is
/// `total:offset:count:invites:…`).
pub fn parse_threads(body: &str) -> (Vec<Thread>, usize, i32) {
    let mut parts = body.split('#');
    let rows = parts.next().unwrap_or("");
    let tail: Vec<i64> = parts.next().unwrap_or("").split(':').map(|v| v.parse().unwrap_or(0)).collect();
    let list: Vec<Thread> = rows
        .split('|')
        .filter(|r| !r.is_empty())
        .filter_map(|r| {
            let f: Vec<&str> = r.split(':').collect();
            // the game skips rows with 6 fields or fewer
            if f.len() < 7 {
                return None;
            }
            let s = |i: usize| f.get(i).copied().unwrap_or("");
            let n = |i: usize| s(i).parse::<i64>().unwrap_or(0);
            let dec = |i: usize| gjapi::b64_decode(s(i)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            Some(Thread {
                id: n(0),
                account: n(1),
                name: dec(2),
                title: dec(3),
                unread: n(4) as i32,
                age: s(6).into(),
                last: dec(8),
                muted: n(10) == 1,
                direct: n(12) == 1,
                described: n(13) == 1,
                starred: n(14) == 1,
                icon: n(15) as i32,
                color1: n(16) as i32,
                color2: n(17) as i32,
                glow_color: n(18) as i32,
                icon_type: n(19) as i32,
                glow: n(20) == 2,
                time: n(23),
                status: n(25) as i32,
            })
        })
        .collect();
    let total = (tail.first().copied().unwrap_or(0) as usize).max(list.len());
    (list, total, tail.get(3).copied().unwrap_or(0) as i32)
}

/// Your threads: `filter` 0 all, 1 direct messages, 2 groups (the page's tabs).
pub async fn threads(filter: i32, page: u32) -> Result<(Vec<Thread>, usize, i32), String> {
    let id = me()?;
    // the game's request (recorded)
    let body = crate::net::post("getGJThreads.php", &format!("{}&page={page}&filter={filter}&removed=1&secret={}", id.base(), gjapi::SECRET)).await?;
    Ok(ok(body).map(|b| parse_threads(b.trim())).unwrap_or_default())
}

#[cfg(test)]
mod thread_tests {
    #[test]
    fn rows_by_position() {
        let (t, total, invites) = super::parse_threads("9001:21:QWxwaGEgTmFtZQ==:R3JvdXAgVGl0bGUgQQ==:3:5:2 hours:2:TGFzdCBtZXNzYWdlIEE=:9:0:11:1:0:1:15:3:12:5:0:2:RmllbGQyMSBB:F22a:1700000000:24:0|9002:22:QmV0YSBOYW1l:R3JvdXAgVGl0bGUgQg==:12:6:3 days:1:TGFzdCBtZXNzYWdlIEI=:19:1:21:0:1:0:25:4:13:6:1:0:RmllbGQyMSBC:F22b:1700000001:34:8|1:2:3#2:0:20:4:5");
        assert_eq!((t.len(), total, invites), (2, 2, 4));
        assert_eq!((t[0].name.as_str(), t[0].unread, t[0].direct, t[0].starred, t[0].icon, t[0].glow), ("Alpha Name", 3, true, true, 15, true));
        assert_eq!((t[1].title.as_str(), t[1].muted, t[1].direct, t[1].status, t[1].last.as_str()), ("Group Title B", true, false, 8, "Last message B"));
    }
}

/// One message in a thread (`GJThreadMessage`; parsed by the game at 0x1004c6374): `id :
/// account : text (base64) : time : deleted`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadMessage {
    pub id: i64,
    pub account: i64,
    pub text: String,
    pub time: i64,
    pub deleted: bool,
}

/// A player named in a thread's answer: `account : user : name (base64) : icon : color 1 :
/// color 2 : glow color : kind : glow`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadUser {
    pub account: i64,
    pub name: String,
    pub icon: i32,
    pub color1: i32,
    pub color2: i32,
    pub icon_type: i32,
}

/// A thread's messages (`getGJThreadMessages.php`, GameLevelManager at 0x100490d04): `#` parts
/// 0 the messages, 1 the players, 4 the thread's state (`owner : … : members : title (base64) :
/// last read : earlier : later : time : description (base64) : locked : can send (0/1 yes,
/// 2–7 blocked, 8 removed)`). Mapped from captures of the real ThreadViewPopup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadPage {
    pub messages: Vec<ThreadMessage>,
    pub users: Vec<ThreadUser>,
    pub earlier: bool,
    pub locked: bool,
    /// -1 when the answer doesn't say
    pub status: i32,
}

pub fn parse_thread_page(body: &str) -> ThreadPage {
    let parts: Vec<&str> = body.split('#').collect();
    let part = |i: usize| parts.get(i).copied().unwrap_or("");
    let messages = part(0)
        .split('|')
        .filter_map(|r| {
            let f: Vec<&str> = r.split(':').collect();
            if f.len() < 4 {
                return None;
            }
            Some(ThreadMessage { id: f[0].parse().ok()?, account: f[1].parse().unwrap_or(0), text: b64_text(f[2]), time: f[3].parse().unwrap_or(0), deleted: f.get(4).is_some_and(|v| *v != "0") })
        })
        .collect();
    let users = part(1)
        .split('|')
        .filter_map(|r| {
            let f: Vec<&str> = r.split(':').collect();
            if f.len() < 3 {
                return None;
            }
            let n = |i: usize| f.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            Some(ThreadUser { account: f[0].parse().ok()?, name: b64_text(f[2]), icon: n(3), color1: n(4), color2: n(5), icon_type: n(7) })
        })
        .collect();
    let meta: Vec<&str> = part(4).split(':').collect();
    let m = |i: usize| meta.get(i).and_then(|v| v.parse::<i64>().ok());
    ThreadPage { messages, users, earlier: m(5).unwrap_or(0) > 0, locked: m(9).unwrap_or(0) != 0, status: m(10).map_or(-1, |v| v as i32) }
}

/// A thread's latest messages, or those before message `before` ("Load earlier").
pub async fn thread_messages(thread: i64, before: i64) -> Result<ThreadPage, String> {
    let id = me()?;
    let body = crate::net::post("getGJThreadMessages.php", &format!("{}&threadID={thread}&beforeID={before}&afterID=0&secret={}", id.base(), gjapi::SECRET)).await?;
    Ok(ok(body).map(|b| parse_thread_page(b.trim())).unwrap_or_default())
}

/// `sendGJThreadMessage.php` as the game sends it (recorded): the text base64'd, and a random
/// 16-digit token (the same send retried isn't posted twice).
pub fn thread_send_form(id: &Identity, thread: i64, text: &str, token: u64) -> String {
    format!("{}&threadID={thread}&secret={}&token={token}&body={}", id.base(), gjapi::SECRET, gjapi::b64(text.as_bytes()))
}

/// A new thread (`NewThreadPopup`), as GameLevelManager::sendThreadMessage builds it (decompiled
/// at 0x1004c4bc0): thread 0, `&isDM=1` for a direct message, the recipient and the group's name
/// (base64), then the group's members, description and emote when set (each base64), the
/// token, and the first message (base64; empty for a group).
pub fn new_thread_form(id: &Identity, dm_to: Option<i64>, targets: &[i64], name: &str, description: &str, body: &str, token: u64) -> String {
    let mut f = format!("{}&threadID=0&secret={}", id.base(), gjapi::SECRET);
    if dm_to.is_some() {
        f.push_str("&isDM=1");
    }
    f.push_str(&format!("&toAccountID={}&subject={}", dm_to.unwrap_or(0), gjapi::b64(name.as_bytes())));
    if !targets.is_empty() {
        let list = targets.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        f.push_str(&format!("&targets={}", gjapi::b64(list.as_bytes())));
    }
    if !description.is_empty() {
        f.push_str(&format!("&description={}", gjapi::b64(description.as_bytes())));
    }
    f.push_str(&format!("&token={token}&body={}", gjapi::b64(body.as_bytes())));
    f
}

pub async fn send_new_thread(dm_to: Option<i64>, targets: Vec<i64>, name: String, description: String, body: String) -> Result<(), String> {
    let id = me()?;
    let token = 1_000_000_000_000_000 + (gjapi::random() % 9_000_000_000_000_000);
    ok(crate::net::post("sendGJThreadMessage.php", &new_thread_form(&id, dm_to, &targets, &name, &description, &body, token)).await?).map(|_| ())
}

pub async fn send_thread_message(thread: i64, text: String) -> Result<(), String> {
    let id = me()?;
    let token = 1_000_000_000_000_000 + (gjapi::random() % 9_000_000_000_000_000);
    ok(crate::net::post("sendGJThreadMessage.php", &thread_send_form(&id, thread, &text, token)).await?).map(|_| ())
}

#[cfg(test)]
mod thread_page_tests {
    #[test]
    fn messages_users_and_state() {
        let p = super::parse_thread_page("7001:21:SGVsbG8gdGhlcmUsIGZpcnN0IG1lc3NhZ2U=:1700000000:0|7002:1:SGkgQWxwaGEhIFRoaXMgb25lIGlzIG1pbmU=:1700000100:0|7003:21:VGhpcmQgbWVzc2FnZSwgZmxhZ2dlZA==:1700000200:1#21:121:QWxwaGE=:15:3:12:5:0:2|1:101:Q2FkZW5jZVRlc3Q=:4:6:7:8:0:0###9001:21:2:TWV0YSBUaXRsZQ==:5:1:0:1700000300:TWV0YSBEZXNj:1:7#P5#P6#11,12,13,14");
        assert_eq!(p.messages.len(), 3);
        assert_eq!((p.messages[0].text.as_str(), p.messages[2].deleted, p.messages[1].account), ("Hello there, first message", true, 1));
        assert_eq!((p.users[0].name.as_str(), p.users[0].icon, p.users[1].account), ("Alpha", 15, 1));
        assert_eq!((p.earlier, p.locked, p.status), (true, true, 7));
    }

    #[test]
    fn new_thread_forms() {
        let id = crate::gjapi::Identity { udid: "U".into(), uuid: 0, account: None };
        let dm = super::new_thread_form(&id, Some(21), &[], "", "", "Hi", 5);
        assert!(dm.ends_with("&threadID=0&secret=Wmfd2893gb7&isDM=1&toAccountID=21&subject=&token=5&body=SGk="), "{dm}");
        let group = super::new_thread_form(&id, None, &[21, 22], "Crew", "About", "", 5);
        assert!(group.ends_with("&threadID=0&secret=Wmfd2893gb7&toAccountID=0&subject=Q3Jldw==&targets=MjEsMjI=&description=QWJvdXQ=&token=5&body="), "{group}");
    }

    #[test]
    fn send_form_as_recorded() {
        let id = crate::gjapi::Identity { udid: "U".into(), uuid: 0, account: None };
        assert!(super::thread_send_form(&id, 9001, "Hello from the harness", 1880999592558863).ends_with("&threadID=9001&secret=Wmfd2893gb7&token=1880999592558863&body=SGVsbG8gZnJvbSB0aGUgaGFybmVzcw=="));
    }
}


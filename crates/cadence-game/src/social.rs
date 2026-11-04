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

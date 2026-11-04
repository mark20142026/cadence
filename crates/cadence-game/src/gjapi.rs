//! The game servers' requests, built the way the game builds them (`GameLevelManager` /
//! `GJAccountManager`), so a Cadence request looks like the game's: the same fields in the same
//! order, the same checksums, sent as the same raw form body.
//!
//! Every request starts with `getBasePostString`:
//! `gameVersion=%i&binaryVersion=%i&udid=%s&uuid=%i&dvs=%i`, then `&accountID=%i&gjp2=%s` when
//! logged in. The endpoint's own fields follow (format strings from the 2.2091 binary).
//!
//! The password is never kept: logging in turns it into GJP2 (SHA-1 of the password and the
//! game's salt), which is what the game stores and sends too.

use base64::Engine;
use sha1::{Digest, Sha1};

/// The game version fields of the base string (2.2091, recorded from the real game).
pub const GAME_VERSION: i32 = 22;
pub const BINARY_VERSION: i32 = 51;
/// `dvs` in the base string.
pub const DVS: i32 = 8;
/// `secret` for most requests, and the accounts server's.
pub const SECRET: &str = "Wmfd2893gb7";
pub const ACCOUNT_SECRET: &str = "Wmfv3899gc9";

/// Who Cadence is to the servers: the install's device id (`udid`) and player id (`uuid`), and
/// the logged-in account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Identity {
    pub udid: String,
    pub uuid: i64,
    pub account: Option<Account>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub account_id: i64,
    pub user_id: i64,
    pub name: String,
    /// GJP2 (never the password).
    pub gjp2: String,
}

impl Identity {
    /// `GameLevelManager::getBasePostString`.
    pub fn base(&self) -> String {
        let mut s = format!("gameVersion={GAME_VERSION}&binaryVersion={BINARY_VERSION}&udid={}&uuid={}&dvs={DVS}", self.udid, self.uuid);
        if let Some(a) = &self.account {
            s.push_str(&format!("&accountID={}&gjp2={}", a.account_id, a.gjp2));
        }
        s
    }
}

impl Identity {
    /// The saved identity as text (Cadence's own `identity.txt` / localStorage): `key=value`
    /// lines. GJP2 is kept like the game keeps it in its save (GJA_005); never the password.
    pub fn to_text(&self) -> String {
        let mut s = format!("udid={}\nuuid={}\n", self.udid, self.uuid);
        if let Some(a) = &self.account {
            s.push_str(&format!("accountID={}\nuserID={}\nname={}\ngjp2={}\n", a.account_id, a.user_id, a.name, a.gjp2));
        }
        s
    }

    pub fn from_text(text: &str) -> Option<Identity> {
        let mut m = std::collections::HashMap::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                m.insert(k.trim(), v.trim().to_string());
            }
        }
        let udid = m.get("udid").filter(|u| !u.is_empty())?.clone();
        let uuid = m.get("uuid").and_then(|u| u.parse().ok()).unwrap_or(0);
        let account = match (m.get("accountID").and_then(|a| a.parse().ok()), m.get("gjp2")) {
            (Some(account_id), Some(gjp2)) if !gjp2.is_empty() => Some(Account {
                account_id,
                user_id: m.get("userID").and_then(|u| u.parse().ok()).unwrap_or(uuid),
                name: m.get("name").cloned().unwrap_or_default(),
                gjp2: gjp2.clone(),
            }),
            _ => None,
        };
        Some(Identity { udid, uuid, account })
    }
}

/// The real game's own device and player ids from its save (read only; Cadence never writes
/// the game's files), so Cadence is the same install to the servers as the player's game.
#[cfg(not(target_arch = "wasm32"))]
pub fn game_identity() -> Option<(String, i64)> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let path = home.join("Library/Application Support/GeometryDash/CCGameManager.dat");
    let data = std::fs::read(path).ok()?;
    let xml = cadence_core::save::decode_any(&data).ok()?;
    let dict = cadence_core::save::parse_plist(&xml).ok()?;
    let udid = cadence_core::save::get(&dict, "playerUDID")?.as_str()?.to_string();
    let uuid = cadence_core::save::get(&dict, "playerUserID").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);
    (udid.len() >= 8).then_some((udid, uuid))
}

#[cfg(not(target_arch = "wasm32"))]
fn identity_path() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("CADENCE_SAVE_DIR") {
        return Some(std::path::PathBuf::from(dir).join("identity.txt"));
    }
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join("Library/Application Support/Cadence/identity.txt"))
}

/// Cadence's identity: the saved one, else the game's ids, else a new device id.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_identity() -> Identity {
    if let Some(id) = identity_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| Identity::from_text(&t)) {
        return id;
    }
    let (udid, uuid) = game_identity().unwrap_or_else(|| (new_udid(), 0));
    let id = Identity { udid, uuid, account: None };
    save_identity(&id);
    id
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_identity(id: &Identity) {
    if let Some(p) = identity_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, id.to_text());
    }
}

/// The identity requests are made with (loaded on first use; login/logout replace it).
#[cfg(not(target_arch = "wasm32"))]
fn identity_cell() -> &'static std::sync::Mutex<Option<Identity>> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<Option<Identity>>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn identity() -> Identity {
    let mut cell = identity_cell().lock().unwrap_or_else(|e| e.into_inner());
    cell.get_or_insert_with(load_identity).clone()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_identity(id: Identity) {
    save_identity(&id);
    *identity_cell().lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
}

/// One request the way the game makes it: the base string and `tail` (the endpoint's own
/// fields), POSTed through the system curl. A bare `-1` or nothing is an error.
#[cfg(not(target_arch = "wasm32"))]
pub fn request(endpoint: &str, tail: &str) -> Result<String, String> {
    let body = post_raw(endpoint, &format!("{}{tail}", identity().base()))?;
    if body.trim() == "-1" || body.trim().is_empty() {
        return Err("the server returned nothing".into());
    }
    Ok(body)
}

/// A check made of five random characters and the base64 of a random six-digit number XORed
/// with `key` (rewards: 59182, quests: 19847).
pub fn random_chk(key: &str) -> String {
    let n = 100_000 + random() % 900_000;
    format!("{}{}", random_string(5), b64(&xor_cycle(n.to_string().as_bytes(), key)))
}

/// Text in a request field: the game sends letters, digits and `-_.~` as they are; the rest
/// percent-encoded.
pub fn field(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `getGJLevels21.php` (GameLevelManager's search request): `&type=%i&str=%s&diff=%s&len=%s&page=%i
/// &total=%i&uncompleted=%i&onlyCompleted=%i&featured=%i&original=%i&twoPlayer=%i&coins=%i`, then
/// the optional filters, and with a difficulty filter a second `&diff=` before the secret (as
/// recorded from the real game).
pub fn search_tail(kind: i32, query: &str, diff: &str, len: &str, page: u32, total: u32, star: bool, demon_filter: u8) -> String {
    let mut s = format!("&type={kind}&str={}&diff={diff}&len={len}&page={page}&total={total}&uncompleted=0&onlyCompleted=0&featured=0&original=0&twoPlayer=0&coins=0", field(query));
    if diff != "-" {
        s.push_str(&format!("&diff={diff}"));
    }
    if star {
        s.push_str("&star=1");
    }
    if demon_filter > 0 && diff == "-2" {
        s.push_str(&format!("&demonFilter={demon_filter}"));
    }
    s.push_str(&format!("&secret={SECRET}"));
    s
}

/// GJP2: SHA-1 (hex) of the password and the salt `mI29fmAnxgTs`.
pub fn gjp2(password: &str) -> String {
    sha1_hex(&format!("{password}mI29fmAnxgTs"))
}

pub fn sha1_hex(s: &str) -> String {
    let d = Sha1::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// XOR with a repeating key (the game's `cycledXOR`).
pub fn xor_cycle(data: &[u8], key: &str) -> Vec<u8> {
    let k = key.as_bytes();
    data.iter().enumerate().map(|(i, b)| b ^ k[i % k.len()]).collect()
}

/// URL-safe base64 (with padding), the game's encoding for checks and texts.
pub fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE.encode(data)
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE.decode(s.trim()).ok()
}

/// A request check: base64(xor(sha1_hex(values + salt), key)).
pub fn chk(values: &str, salt: &str, key: &str) -> String {
    b64(&xor_cycle(sha1_hex(&format!("{values}{salt}")).as_bytes(), key))
}

/// Random letters and digits (the game's random strings: `rs`, seeds, check prefixes).
pub fn random_string(n: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    (0..n).map(|_| CHARS[(random() % CHARS.len() as u64) as usize] as char).collect()
}

/// A random number (xorshift, seeded from the clock and a counter).
pub fn random() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0x9e37_79b9, |d| d.as_nanos() as u64) | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    x
}

/// A new install's device id: an upper-case UUID like the game's (`playerUDID`, 36 chars).
pub fn new_udid() -> String {
    let h = format!("{:016X}{:016X}", random(), random());
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_string_order() {
        let mut id = Identity { udid: "U".into(), uuid: 7, account: None };
        assert_eq!(id.base(), "gameVersion=22&binaryVersion=51&udid=U&uuid=7&dvs=8");
        id.account = Some(Account { account_id: 5, user_id: 7, name: "n".into(), gjp2: "g".into() });
        assert!(id.base().ends_with("&dvs=8&accountID=5&gjp2=g"));
    }

    #[test]
    fn identity_text_round_trip() {
        let id = Identity { udid: "A-B".into(), uuid: 12, account: Some(Account { account_id: 3, user_id: 12, name: "Mark".into(), gjp2: "ff".into() }) };
        assert_eq!(Identity::from_text(&id.to_text()), Some(id.clone()));
        let out = Identity { account: None, ..id };
        assert_eq!(Identity::from_text(&out.to_text()), Some(out));
        assert_eq!(Identity::from_text("uuid=1"), None);
    }

    #[test]
    fn gjp2_and_xor() {
        // SHA-1 known value: sha1("abc") = a9993e36...
        assert_eq!(sha1_hex("abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(gjp2("x").len(), 40);
        let x = xor_cycle(b"hello", "59182");
        assert_eq!(xor_cycle(&x, "59182"), b"hello");
        assert_eq!(new_udid().len(), 36);
    }
}

/// The chests' state from `getGJRewards.php`: seconds until each chest opens, what it gave (when
/// it was just opened) and how many have been opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rewards {
    pub small_left: i64,
    pub small_items: Option<ChestItems>,
    pub small_count: i32,
    pub big_left: i64,
    pub big_items: Option<ChestItems>,
    pub big_count: i32,
    /// 0 the state, 1 the small chest opened, 2 the big one
    pub reward_type: i32,
}

/// A chest's contents: orbs, diamonds and up to two items (shards 1–5, keys, …) by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChestItems {
    pub orbs: i32,
    pub diamonds: i32,
    pub items: Vec<i32>,
}

impl ChestItems {
    fn parse(s: &str) -> Option<ChestItems> {
        let v: Vec<i32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() < 2 || v.iter().all(|&x| x == 0) {
            return None;
        }
        Some(ChestItems { orbs: v[0], diamonds: v[1], items: v[2..].iter().copied().filter(|&x| x != 0).collect() })
    }
}

/// `GameLevelManager::getGJRewards`: the form body (base + `&rewardType=%i&secret=%s&chk=%s&r1=%i&r2=%i`).
/// `chk` is five random characters and the XOR-59182 base64 of a random number; r1/r2 are the
/// numbers of small/big chests this save has opened.
pub fn rewards_form(id: &Identity, reward_type: i32, r1: i32, r2: i32) -> String {
    // the real game's checks decode to six digits
    let n = 100_000 + random() % 900_000;
    let chk = format!("{}{}", random_string(5), b64(&xor_cycle(n.to_string().as_bytes(), "59182")));
    rewards_form_with(id, reward_type, r1, r2, &chk)
}

pub fn rewards_form_with(id: &Identity, reward_type: i32, r1: i32, r2: i32, chk: &str) -> String {
    format!("{}&rewardType={reward_type}&secret={SECRET}&chk={chk}&r1={r1}&r2={r2}", id.base())
}

/// `onGetGJRewardsCompleted`: `<5 junk chars><base64 xor 59182>|<hash>`, fields split by ':'
/// (12 or more): random, userID, chk, udid, accountID, small seconds left, small items, small
/// count, big seconds left, big items, big count, reward type.
pub fn parse_rewards(body: &str, id: &Identity) -> Result<Rewards, String> {
    let body = body.trim();
    if body.starts_with('-') {
        return Err(format!("the server said {body}"));
    }
    let data = body.split('|').next().unwrap_or_default();
    if data.len() <= 5 {
        return Err("empty reward data".into());
    }
    let plain = b64_decode(&data[5..]).map(|d| xor_cycle(&d, "59182")).ok_or("bad reward data")?;
    let text = String::from_utf8_lossy(&plain).to_string();
    let f: Vec<&str> = text.split(':').collect();
    if f.len() < 12 {
        return Err(format!("{} reward fields", f.len()));
    }
    if f[3] != id.udid {
        return Err("the reward data is for another device".into());
    }
    let num = |i: usize| f[i].trim().parse::<i64>().unwrap_or(0);
    Ok(Rewards {
        small_left: num(5),
        small_items: ChestItems::parse(f[6]),
        small_count: num(7) as i32,
        big_left: num(8),
        big_items: ChestItems::parse(f[9]),
        big_count: num(10) as i32,
        reward_type: num(11) as i32,
    })
}

/// `GJAccountManager::loginAccount`: `udid=%s&userName=%s&gjp2=%s`, the Steam id (`sID`, from
/// the Steam client like the game's) and the accounts secret (recorded from the real game).
pub fn login_form(id: &Identity, user: &str, gjp2: &str, steam_id: &str) -> String {
    format!("udid={}&userName={}&gjp2={gjp2}&sID={steam_id}&secret={ACCOUNT_SECRET}", id.udid, user)
}

/// The logged-in account id for checks (0 when logged out).
fn account_id(id: &Identity) -> i64 {
    id.account.as_ref().map_or(0, |a| a.account_id)
}

/// `downloadGJLevel22.php`: `&levelID=%i&inc=%i&secret=%s`, then `&rs=` and a check of
/// levelID, inc, rs, accountID, udid, uuid (salt xI25fpAapCQg, key 41274).
pub fn download_form(id: &Identity, level_id: i64, inc: bool, rs: &str) -> String {
    let inc = inc as i32;
    let c = chk(&format!("{level_id}{inc}{rs}{}{}{}", account_id(id), id.udid, id.uuid), "xI25fpAapCQg", "41274");
    format!("{}&levelID={level_id}&inc={inc}&secret={SECRET}&rs={rs}&chk={c}", id.base())
}

/// `likeGJItem211.php`: `&itemID=%i&like=%i&type=%i&secret=%s&special=%i`, `&rs=` and a check of
/// special, itemID, like, type, rs, accountID, udid, uuid (salt ysg6pUrtjn0J, key 58281).
/// type: 1 level, 2 level comment (special = the level), 3 profile comment, 4 list.
pub fn like_form(id: &Identity, item: i64, like: bool, kind: i32, special: i64, rs: &str) -> String {
    let like = like as i32;
    let c = chk(&format!("{special}{item}{like}{kind}{rs}{}{}{}", account_id(id), id.udid, id.uuid), "ysg6pUrtjn0J", "58281");
    format!("{}&itemID={item}&like={like}&type={kind}&secret={SECRET}&special={special}&rs={rs}&chk={c}", id.base())
}

/// `getGJComments21.php`: `&page=%i&total=%i&secret=%s&mode=%i` and `&levelID=%i`
/// (mode 0 recent, 1 most liked); the full comments page adds `&count=20`, the level page's
/// preview doesn't.
pub fn comments_form(id: &Identity, level_id: i64, page: u32, total: u32, mode: i32, count: Option<u32>) -> String {
    let count = count.map_or(String::new(), |c| format!("&count={c}"));
    format!("{}&page={page}&total={total}&secret={SECRET}&mode={mode}&levelID={level_id}{count}", id.base())
}

/// `uploadGJComment21.php`: `&userName=%s&comment=%s&secret=%s&levelID=%i` (the comment in
/// URL-safe base64), then `&percent=%i` when the player has progress, and a check of userName,
/// comment, levelID, percent and 0 (salt xPT6iUrtws0J, key 29481). Logged in only.
pub fn comment_upload_form(id: &Identity, level_id: i64, text: &str, percent: i32) -> Option<String> {
    let a = id.account.as_ref()?;
    let comment = b64(text.as_bytes());
    let c = chk(&format!("{}{comment}{level_id}{percent}0", a.name), "xPT6iUrtws0J", "29481");
    let pct = if percent > 0 { format!("&percent={percent}") } else { String::new() };
    Some(format!("{}&userName={}&comment={comment}&secret={SECRET}&levelID={level_id}{pct}&chk={c}", id.base(), a.name))
}

/// `getGJGauntlets21.php`: `&secret=%s&special=1&vkey=%i` (vkey: the binary version).
pub fn gauntlets_form(id: &Identity) -> String {
    format!("{}&secret={SECRET}&special=1&vkey={BINARY_VERSION}", id.base())
}

/// `getGJContentStatus.php`: `&secret=%s&special=1`.
pub fn content_status_form(id: &Identity) -> String {
    format!("{}&secret={SECRET}&special=1", id.base())
}

/// The Steam account id the game sends as `sID` (the most recent Steam login), or empty.
#[cfg(not(target_arch = "wasm32"))]
pub fn steam_id() -> String {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let Ok(text) = std::fs::read_to_string(home.join("Library/Application Support/Steam/config/loginusers.vdf")) else { return String::new() };
    // "7656119…" { … "MostRecent" "1" … }: the block marked most recent, else the first
    let mut first = None;
    let mut current = None;
    for line in text.lines() {
        let t = line.trim().trim_matches('"');
        if t.len() == 17 && t.starts_with("7656") && t.bytes().all(|b| b.is_ascii_digit()) {
            current = Some(t.to_string());
            first.get_or_insert_with(|| t.to_string());
        }
        let l = line.to_ascii_lowercase();
        if l.contains("\"mostrecent\"") && l.contains("\"1\"") {
            if let Some(c) = &current {
                return c.clone();
            }
        }
    }
    first.unwrap_or_default()
}

/// The login answer (`GJAccountManager::onLoginAccountCompleted`): `accountID,userID`, else
/// the server's negative code (−1 for anything else).
pub fn parse_login(body: &str) -> Result<(i64, i64), i32> {
    let body = body.trim();
    let mut it = body.split(',');
    match (it.next().and_then(|a| a.parse::<i64>().ok()), it.next().and_then(|u| u.parse::<i64>().ok())) {
        (Some(a), Some(u)) if a > 0 => Ok((a, u)),
        (Some(code), None) if code < 0 => Err(code as i32),
        _ => Err(-1),
    }
}

/// Log in (`GJAccountManager::loginAccount`): the password becomes GJP2 here and only that is
/// kept. On success the identity takes the account and its user id as `uuid` (the real game's
/// requests switch to it after logging in), saved in Cadence's identity file. An account other
/// than the one already linked fails with −10, like the game's.
#[cfg(not(target_arch = "wasm32"))]
pub fn login(user: &str, password: &str) -> Result<Account, i32> {
    let id = identity();
    let g = gjp2(password);
    let body = post_raw("accounts/loginGJAccount.php", &login_form(&id, user, &g, &steam_id())).map_err(|_| -1)?;
    let (account_id, user_id) = parse_login(&body)?;
    if id.account.as_ref().is_some_and(|a| a.account_id != account_id) {
        return Err(-10);
    }
    let account = Account { account_id, user_id, name: user.to_string(), gjp2: g };
    set_identity(Identity { uuid: user_id, account: Some(account.clone()), ..id });
    Ok(account)
}

/// Unlink the account (`AccountHelpLayer::doUnlink`): requests are anonymous again.
#[cfg(not(target_arch = "wasm32"))]
pub fn unlink() {
    set_identity(Identity { account: None, ..identity() });
}

/// The linked account's name and whether it has its GJP2 (else the page says "Login required").
#[cfg(not(target_arch = "wasm32"))]
pub fn linked_account() -> Option<(String, bool)> {
    identity().account.map(|a| (a.name, !a.gjp2.is_empty()))
}

#[cfg(target_arch = "wasm32")]
pub fn linked_account() -> Option<(String, bool)> {
    None
}

/// `AccountLoginLayer::loginAccountFailed`: which field's label shows the error (0 username,
/// 1 password) and its text.
pub fn login_error(code: i32) -> (u8, &'static str) {
    match code {
        -14 => (1, "Password contains invalid characters"),
        -13 => (0, "Already linked to different Steam account"),
        -12 => (0, "Account has been disabled"),
        -10 => (0, "Already linked to different account"),
        -9 => (0, "Too short. Minimum 3 characters"),
        -8 => (1, "Too short. Minimum 6 characters"),
        _ => (0, "Login failed"),
    }
}

/// Login against a local stand-in server (`CADENCE_SERVER`) with a scratch save folder: the
/// request is the game's, the identity takes the account and its user id, codes come back.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod login_tests {
    use super::*;
    use std::io::{Read, Write};

    fn serve(answer: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = s.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&buf).to_string();
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{answer}", answer.len()).unwrap();
            text
        });
        (addr, handle)
    }

    #[test]
    fn login_round_trip_with_a_stand_in_server() {
        let dir = std::env::temp_dir().join(format!("cadence-login-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: this test is the only one that sets these variables.
        unsafe {
            std::env::set_var("CADENCE_SAVE_DIR", &dir);
        }
        set_identity(Identity { udid: "S-test".into(), uuid: 377181558, account: None });

        let (addr, server) = serve("34391908,299760057");
        unsafe { std::env::set_var("CADENCE_SERVER", &addr) };
        let account = login("someone", "secret-pw").unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /accounts/loginGJAccount.php "), "{request}");
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let expected = login_form(&Identity { udid: "S-test".into(), uuid: 377181558, account: None }, "someone", &gjp2("secret-pw"), &steam_id());
        assert_eq!(body, expected);
        assert!(!body.contains("secret-pw"), "the password itself is never sent");
        assert_eq!((account.account_id, account.user_id), (34391908, 299760057));
        let saved = identity();
        assert_eq!(saved.uuid, 299760057, "requests switch to the account's user id");
        assert_eq!(saved.account.as_ref().map(|a| a.gjp2.clone()), Some(gjp2("secret-pw")));
        let file = std::fs::read_to_string(dir.join("identity.txt")).unwrap();
        assert!(!file.contains("secret-pw"), "the password is never saved");

        // another account than the linked one: −10, like the game
        let (addr, server) = serve("111,222");
        unsafe { std::env::set_var("CADENCE_SERVER", &addr) };
        assert_eq!(login("other", "another-pw").map(|a| a.account_id), Err(-10));
        server.join().unwrap();

        // a refusal code passes through
        unlink();
        let (addr, server) = serve("-12");
        unsafe { std::env::set_var("CADENCE_SERVER", &addr) };
        assert_eq!(login("someone", "secret-pw").map(|a| a.account_id), Err(-12));
        server.join().unwrap();
        assert!(identity().account.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod reward_tests {
    use super::*;

    #[test]
    fn rewards_round_trip() {
        let id = Identity { udid: "U-1".into(), uuid: 9, account: None };
        let plain = "12345:9:abc:U-1:0:3600:0,0,0,0:4:0:250,5,3,0:12:2";
        let body = format!("abcde{}|hash", b64(&xor_cycle(plain.as_bytes(), "59182")));
        let r = parse_rewards(&body, &id).unwrap();
        assert_eq!(r.small_left, 3600);
        assert_eq!(r.small_items, None);
        assert_eq!(r.big_items, Some(ChestItems { orbs: 250, diamonds: 5, items: vec![3] }));
        assert_eq!((r.small_count, r.big_count, r.reward_type), (4, 12, 2));
        assert!(rewards_form(&id, 0, 4, 12).contains("&rewardType=0&secret=Wmfd2893gb7&chk="));
        assert!(login_form(&id, "u", "g", "765").ends_with("&gjp2=g&sID=765&secret=Wmfv3899gc9"));
        assert_eq!(parse_login("123,456"), Ok((123, 456)));
        assert_eq!(parse_login("-13"), Err(-13));
        assert_eq!(parse_login("<html>"), Err(-1));
        assert_eq!(login_error(-12), (0, "Account has been disabled"));
    }
}

/// The game servers' address (`CADENCE_SERVER` overrides it, for recording requests locally).
#[cfg(not(target_arch = "wasm32"))]
pub fn server() -> String {
    std::env::var("CADENCE_SERVER").unwrap_or_else(|_| "https://www.boomlings.com/database".into())
}

/// POST a form body exactly as the game does: the game's CCHttpClient hands it to the system
/// libcurl with only the POST fields set, so curl sends `Host`, `Accept: */*`,
/// `Content-Length`, `Content-Type: application/x-www-form-urlencoded` and no user agent
/// (recorded from the real game). Cadence uses the same libcurl the same way; the body is sent as
/// built, not re-encoded.
#[cfg(not(target_arch = "wasm32"))]
pub fn post_raw(endpoint: &str, body: &str) -> Result<String, String> {
    let mut easy = curl::easy::Easy::new();
    easy.url(&format!("{}/{endpoint}", server())).map_err(|e| e.to_string())?;
    easy.post(true).map_err(|e| e.to_string())?;
    easy.post_fields_copy(body.as_bytes()).map_err(|e| e.to_string())?;
    easy.timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer
            .write_function(|data| {
                out.extend_from_slice(data);
                Ok(data.len())
            })
            .map_err(|e| e.to_string())?;
        transfer.perform().map_err(|e| e.to_string())?;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod http_probe {
    /// With CADENCE_SERVER pointing at a local recorder: sends one request (ignored otherwise).
    #[test]
    fn probe() {
        if std::env::var_os("CADENCE_SERVER").is_some() {
            let _ = super::post_raw("probe.php", "gameVersion=22&secret=x");
        }
    }
}

/// With CADENCE_CAPTURE=<the harness's request log>: every captured request this module builds
/// is rebuilt from the captured values and must match the real game's body byte for byte.
/// (The capture holds the player's ids, so it stays out of the repository.)
#[cfg(all(test, not(target_arch = "wasm32")))]
mod capture_diff {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn rebuild_captured_requests() {
        let Some(path) = std::env::var_os("CADENCE_CAPTURE") else { return };
        let text = std::fs::read_to_string(path).unwrap();
        let (mut checked, mut failed) = (0, Vec::new());
        for line in text.lines() {
            let cols: Vec<&str> = line.splitn(5, '\t').collect();
            if cols.len() < 5 {
                continue;
            }
            let endpoint = cols[1].rsplit("/database/").next().unwrap_or_default();
            let body = cols[4];
            let mut f: HashMap<&str, &str> = HashMap::new();
            for kv in body.split('&') {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                f.entry(k).or_insert(v);
            }
            let num = |k: &str| f.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let id = Identity {
                udid: f.get("udid").unwrap_or(&"").to_string(),
                uuid: num("uuid"),
                account: f.get("accountID").map(|a| Account { account_id: a.parse().unwrap_or(0), user_id: num("uuid"), name: f.get("userName").unwrap_or(&"").to_string(), gjp2: f.get("gjp2").unwrap_or(&"").to_string() }),
            };
            let ours = match endpoint {
                "getGJRewards.php" => Some(rewards_form_with(&id, num("rewardType") as i32, num("r1") as i32, num("r2") as i32, f["chk"])),
                "downloadGJLevel22.php" => Some(download_form(&id, num("levelID"), num("inc") == 1, f["rs"])),
                "likeGJItem211.php" => Some(like_form(&id, num("itemID"), num("like") == 1, num("type") as i32, num("special"), f["rs"])),
                "getGJComments21.php" => Some(comments_form(&id, num("levelID"), num("page") as u32, num("total") as u32, num("mode") as i32, f.get("count").and_then(|c| c.parse().ok()))),
                "uploadGJComment21.php" => {
                    let text = String::from_utf8(b64_decode(f["comment"]).unwrap()).unwrap();
                    comment_upload_form(&id, num("levelID"), &text, num("percent") as i32)
                }
                "getGJGauntlets21.php" => Some(gauntlets_form(&id)),
                "getGJContentStatus.php" => Some(content_status_form(&id)),
                "getGJLevels21.php" => Some(format!("{}{}", id.base(), search_tail(num("type") as i32, f["str"], f["diff"], f["len"], num("page") as u32, num("total") as u32, f.contains_key("star"), num("demonFilter") as u8))),
                "loginGJAccount.php" => Some(login_form(&id, f["userName"], f["gjp2"], f["sID"])),
                _ => None,
            };
            let Some(ours) = ours else { continue };
            checked += 1;
            if ours != body {
                failed.push(format!("{endpoint}:\n  real {body}\n  ours {ours}"));
            }
        }
        eprintln!("capture: {checked} requests rebuilt, {} differ", failed.len());
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        assert!(checked > 0);
    }
}

//! Online levels, read-only: search and download from the game's level servers (the requests the
//! real game makes: `getGJLevels21.php`, `downloadGJLevel22.php`), and custom songs from the URLs
//! the server gives. Nothing is uploaded and nothing is stored except Cadence's own song cache.
//! Requests go through `net` (the relay on the web).

use std::collections::HashMap;

/// One level from a search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelInfo {
    pub id: i32,
    pub name: String,
    pub author: String,
    pub description: String,
    /// 0 N/A, 1 easy … 5 insane (difficulty numerator / 10), `demon` and `auto` apart.
    pub difficulty: i32,
    pub demon: bool,
    /// Key 43: 3 easy, 4 medium, 0 hard, 5 insane, 6 extreme.
    pub demon_difficulty: i32,
    pub auto: bool,
    pub stars: i32,
    pub downloads: i64,
    pub likes: i64,
    /// 0 tiny, 1 short, 2 medium, 3 long, 4 XL, 5 platformer.
    pub length: i32,
    /// Official song index (key 12) when there is no custom song.
    pub official_song: i32,
    pub custom_song: i32,
    pub song_name: String,
    pub song_url: Option<String>,
    pub song_artist: String,
    /// The song's size in MB (song key 5).
    pub song_size: f32,
    pub coins: i32,
    /// Key 38: the coins are verified (silver) rather than bronze.
    pub coins_verified: bool,
    /// Key 19: featured score (> 0 is featured).
    pub featured: i64,
    /// Key 42: 1 epic, 2 legendary, 3 mythic.
    pub epic: i32,
    /// Key 45: object count.
    pub objects: i64,
    /// Key 31: two player.
    pub two_player: bool,
    /// Key 30: copied from this level id.
    pub original: i32,
    /// The server's player id of the creator (key 6), and their account id (from the creators
    /// section; 0 for unregistered players).
    pub player_id: i64,
    pub account_id: i64,
    /// Keys 52 / 53: the songs and sound effects the level uses (comma lists).
    pub songs: i32,
    /// The level's other songs (key 52: song triggers' songs), to download with it.
    pub song_ids: Vec<i32>,
    /// The library sounds its SFX triggers play (key 53).
    pub sfx_ids: Vec<i32>,
    /// A daily / weekly / event level's number (key 41: 1xxxxx weekly, 2xxxxx event).
    pub daily_index: i64,
    pub sfx: i32,
}

impl LevelInfo {
    /// The mana orbs for completing a rated level (`GameStatsManager::getBaseCurrencyForLevel`).
    pub fn orbs(&self) -> i32 {
        match self.stars {
            2 => 50,
            3 => 75,
            4 => 125,
            5 => 175,
            6 => 225,
            7 => 275,
            8 => 350,
            9 => 425,
            10 => 500,
            _ => 0,
        }
    }

    /// `LevelTools::lengthString`.
    pub fn length_text(&self) -> &'static str {
        match self.length {
            0 => "Tiny",
            1 => "Short",
            2 => "Medium",
            3 => "Long",
            4 => "XL",
            5 => "Plat.",
            _ => "?",
        }
    }
}

/// A search: getGJLevels21's form fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Search {
    /// `type`: 0 search, 1 downloads, 2 likes, 3 trending, 4 recent, 5 user, 6 featured,
    /// 7 magic, 11 awarded, 12 followed, 13 friends, 16 hall of fame, 31 hyped, ...
    pub kind: i32,
    pub query: String,
    /// "-" or a comma list of difficulties (-1 N/A, 1..5, -2 demon, -3 auto)
    pub diff: String,
    /// "-" or a comma list of lengths 0..5
    pub len: String,
    pub page: u32,
    pub star: bool,
    /// `demonFilter` with the demon difficulty alone: 1 easy … 5 extreme (0: all demons)
    pub demon_filter: u8,
    /// Advanced Options (`gjapi::filter` bits) and the levels you've completed, for them.
    pub filters: u16,
    pub completed: String,
}

impl Search {
    pub fn new(kind: i32) -> Search {
        Search { kind, query: String::new(), diff: "-".into(), len: "-".into(), page: 0, star: false, demon_filter: 0, filters: 0, completed: String::new() }
    }
}

/// One page of results and where it sits in the whole list (the response's "total:offset:count").
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub levels: Vec<LevelInfo>,
    pub total: u32,
    pub offset: u32,
}

/// What a search asks for (`type` in getGJLevels21).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Search,
    MostDownloaded,
    MostLiked,
    Trending,
    Recent,
    Featured,
    Magic,
    Awarded,
    HallOfFame,
}

impl Kind {
        fn code(self) -> i32 {
        match self {
            Kind::Search => 0,
            Kind::MostDownloaded => 1,
            Kind::MostLiked => 2,
            Kind::Trending => 3,
            Kind::Recent => 4,
            Kind::Featured => 6,
            Kind::Magic => 7,
            Kind::Awarded => 11,
            Kind::HallOfFame => 16,
        }
    }
}

const SECRET: &str = "Wmfd2893gb7";

/// `a:b:c:d` → {a: b, c: d}.
fn pairs(text: &str, sep: &str) -> HashMap<String, String> {
    let parts: Vec<&str> = text.split(sep).collect();
    parts.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())).collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn b64_text(s: &str) -> String {
    cadence_core::codec::base64_decode(s).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// A level from its key → value map (a search result or a download), with the response's
/// creators (player id → name) and songs (id → fields).
pub fn info_from_map(m: &HashMap<String, String>, creators: &HashMap<String, String>, songs: &HashMap<String, HashMap<String, String>>) -> LevelInfo {
    let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let custom = int("35") as i32;
    let song = songs.get(&custom.to_string());
    LevelInfo {
        id: int("1") as i32,
        name: m.get("2").cloned().unwrap_or_default(),
        author: creators.get(&m.get("6").cloned().unwrap_or_default()).cloned().unwrap_or_else(|| "-".into()),
        description: b64_text(m.get("3").map_or("", |s| s.as_str())),
        difficulty: (int("9") / 10) as i32,
        demon: int("17") == 1,
        demon_difficulty: int("43") as i32,
        auto: int("25") == 1,
        stars: int("18") as i32,
        downloads: int("10"),
        likes: int("14"),
        length: int("15") as i32,
        official_song: int("12") as i32,
        custom_song: custom,
        song_name: song.and_then(|s| s.get("2").cloned()).unwrap_or_default(),
        song_url: song.and_then(|s| s.get("10")).map(|u| percent_decode(u)).filter(|u| !u.is_empty()),
        song_artist: song.and_then(|s| s.get("4").cloned()).unwrap_or_default(),
        song_size: song.and_then(|s| s.get("5")).and_then(|v| v.parse().ok()).unwrap_or(0.0),
        coins: int("37") as i32,
        coins_verified: int("38") == 1,
        featured: int("19"),
        epic: int("42") as i32,
        objects: int("45"),
        two_player: int("31") == 1,
        original: int("30") as i32,
        player_id: int("6"),
        account_id: 0,
        songs: m.get("52").map_or(0, |v| v.split(',').filter(|x| !x.is_empty()).count() as i32),
        song_ids: m.get("52").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default(),
        sfx_ids: m.get("53").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default(),
        daily_index: int("41"),
        sfx: m.get("53").map_or(0, |v| v.split(',').filter(|x| !x.is_empty()).count() as i32),
    }
}

/// The creators section's account ids: player id → account id.
fn account_ids(body: &str) -> HashMap<String, i64> {
    body.split('#')
        .nth(1)
        .unwrap_or("")
        .split('|')
        .filter_map(|c| {
            let f: Vec<&str> = c.split(':').collect();
            Some((f.first()?.to_string(), f.get(2)?.parse().ok()?))
        })
        .collect()
}

/// Parse a getGJLevels21 response with its page info.
pub fn parse_page(body: &str) -> Page {
    let accounts = account_ids(body);
    let mut levels = parse_levels(body);
    for l in &mut levels {
        l.account_id = accounts.get(&l.player_id.to_string()).copied().unwrap_or(0);
    }
    let info: Vec<u32> = body.split('#').nth(3).unwrap_or("").split(':').filter_map(|v| v.parse().ok()).collect();
    Page { total: info.first().copied().unwrap_or(levels.len() as u32), offset: info.get(1).copied().unwrap_or(0), levels }
}

/// Parse a getGJLevels21 response.
pub fn parse_levels(body: &str) -> Vec<LevelInfo> {
    let sections: Vec<&str> = body.split('#').collect();
    let creators: HashMap<String, String> = sections
        .get(1)
        .map(|s| {
            s.split('|')
                .filter_map(|c| {
                    let f: Vec<&str> = c.split(':').collect();
                    Some((f.first()?.to_string(), f.get(1)?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let songs: HashMap<String, HashMap<String, String>> = sections
        .get(2)
        .map(|s| {
            s.split("~:~")
                .filter(|x| !x.is_empty())
                .map(|x| {
                    let m = pairs(x, "~|~");
                    (m.get("1").cloned().unwrap_or_default(), m)
                })
                .collect()
        })
        .unwrap_or_default();
    let Some(levels) = sections.first() else { return Vec::new() };
    levels
        .split('|')
        .filter(|l| !l.is_empty() && *l != "-1")
        .map(|l| info_from_map(&pairs(l, ":"), &creators, &songs))
        .collect()
}

/// Search the servers (page 0, 10 levels a page).
pub async fn search(kind: Kind, query: &str, page: u32) -> Result<Vec<LevelInfo>, String> {
    let mut s = Search::new(kind.code());
    s.query = query.into();
    s.page = page;
    Ok(search_page(&s).await?.levels)
}

/// One page of a search, with the totals.
pub async fn search_page(s: &Search) -> Result<Page, String> {
    let tail = crate::gjapi::search_tail_filtered(s.kind, &s.query, &s.diff, &s.len, s.page, 0, s.star, s.demon_filter, s.filters, &s.completed);
    let body = crate::gjapi::request("getGJLevels21.php", &tail).await?;
    Ok(parse_page(&body))
}

/// Download a level's whole response: (key → value of the level, level data).
pub async fn download_full(id: i32) -> Result<(HashMap<String, String>, Vec<u8>), String> {
    let id = id.to_string();
    let body = crate::net::post("downloadGJLevel22.php", &crate::gjapi::download_form(&crate::gjapi::identity(), id.parse().unwrap_or(0), true, &crate::gjapi::random_string(10))).await.and_then(|b| if b.trim() == "-1" || b.trim().is_empty() { Err("the server returned nothing".to_string()) } else { Ok(b) })?;
    let first = body.split('#').next().unwrap_or("");
    let m = pairs(first, ":");
    let data = m.get("4").map(|s| s.clone().into_bytes()).filter(|s| !s.is_empty()).ok_or("no level data")?;
    Ok((m, data))
}

/// Download a level: its level data (key 4: base64 + gzip, what `decode_level_bytes` reads).
pub async fn download(id: i32) -> Result<Vec<u8>, String> {
    let id = id.to_string();
    let body = crate::net::post("downloadGJLevel22.php", &crate::gjapi::download_form(&crate::gjapi::identity(), id.parse().unwrap_or(0), true, &crate::gjapi::random_string(10))).await.and_then(|b| if b.trim() == "-1" || b.trim().is_empty() { Err("the server returned nothing".to_string()) } else { Ok(b) })?;
    let first = body.split('#').next().unwrap_or("");
    let m = pairs(first, ":");
    m.get("4").map(|s| s.clone().into_bytes()).filter(|s| !s.is_empty()).ok_or_else(|| "no level data".into())
}

/// Today's daily (0), weekly (1) or event (2) level (`getGJDailyLevel.php`, then the level as id
/// −1/−2/−3), its data and the seconds until the next one.
pub async fn daily(kind: i32) -> Result<(LevelInfo, Vec<u8>, i64), String> {
    let ty = kind.to_string();
    // the schedule (`index|seconds left`); the servers may answer −1 and still have the level
    let body = crate::gjapi::request("getGJDailyLevel.php", &format!("&type={ty}&secret={SECRET}")).await.unwrap_or_default();
    let left = body.split('|').nth(1).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let id = (-1 - kind).to_string();
    let resp = crate::net::post("downloadGJLevel22.php", &crate::gjapi::download_form(&crate::gjapi::identity(), id.parse().unwrap_or(0), true, &crate::gjapi::random_string(10))).await.and_then(|b| if b.trim() == "-1" || b.trim().is_empty() { Err("the server returned nothing".to_string()) } else { Ok(b) })?;
    let sections: Vec<&str> = resp.split('#').collect();
    let m = pairs(sections.first().copied().unwrap_or(""), ":");
    let data = m.get("4").map(|s| s.clone().into_bytes()).filter(|s| !s.is_empty()).ok_or("no level data")?;
    // after the hashes: "playerID:name:accountID", then the song's fields
    let mut creators = HashMap::new();
    let mut songs = HashMap::new();
    for sec in &sections[1..] {
        let f: Vec<&str> = sec.split(':').collect();
        if sec.contains("~|~") {
            let song = pairs(sec.trim_end_matches("~:~"), "~|~");
            songs.insert(song.get("1").cloned().unwrap_or_default(), song);
        } else if f.len() == 3 && f[0].parse::<i64>().is_ok() {
            creators.insert(f[0].to_string(), f[1].to_string());
        }
    }
    let custom = m.get("35").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    if custom > 0 && !songs.contains_key(&custom.to_string()) {
        if let Ok(song) = song_info(custom).await {
            songs.insert(custom.to_string(), song);
        }
    }
    Ok((info_from_map(&m, &creators, &songs), data, left))
}

/// A custom song's fields (`getGJSongInfo.php`: 1 id, 2 name, 4 artist, 5 size, 10 url).
pub async fn song_info(id: i32) -> Result<HashMap<String, String>, String> {
    let id = id.to_string();
    let body = crate::gjapi::request("getGJSongInfo.php", &format!("&songID={id}&secret={SECRET}")).await?;
    Ok(pairs(body.trim_end_matches("~:~"), "~|~"))
}

/// A map pack (`getGJMapPacks21.php`): 1 id, 2 name, 3 level ids, 4 stars, 5 coins,
/// 6 difficulty, 7 text color, 8 bar color.
#[derive(Clone, Debug, Default)]
pub struct MapPack {
    pub id: i32,
    pub name: String,
    pub levels: String,
    pub stars: i32,
    pub coins: i32,
    pub difficulty: i32,
    pub text_color: [u8; 3],
    pub bar_color: [u8; 3],
}

fn rgb(s: Option<&String>) -> [u8; 3] {
    let v: Vec<u8> = s.map_or("", |s| s.as_str()).split(',').filter_map(|x| x.parse().ok()).collect();
    if v.len() == 3 { [v[0], v[1], v[2]] } else { [255, 255, 255] }
}

/// Parse a getGJMapPacks21 response: the packs and the "total:offset:count" section.
pub fn parse_packs(body: &str) -> (Vec<MapPack>, u32) {
    let mut sec = body.split('#');
    let packs = sec
        .next()
        .unwrap_or("")
        .split('|')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let m = pairs(p, ":");
            let int = |k: &str| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0);
            MapPack {
                id: int("1"),
                name: m.get("2").cloned().unwrap_or_default(),
                levels: m.get("3").cloned().unwrap_or_default(),
                stars: int("4"),
                coins: int("5"),
                difficulty: int("6"),
                text_color: rgb(m.get("7")),
                bar_color: rgb(m.get("8")),
            }
        })
        .collect();
    let total = sec.next().unwrap_or("").split(':').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (packs, total)
}

pub async fn map_packs(page: u32) -> Result<(Vec<MapPack>, u32), String> {
    let page = page.to_string();
    let body = crate::gjapi::request("getGJMapPacks21.php", &format!("&page={page}&secret={SECRET}")).await?;
    Ok(parse_packs(&body))
}

/// The gauntlets (`getGJGauntlets21.php`, special=1): (type, level ids).
pub async fn gauntlets() -> Result<Vec<(i32, String)>, String> {
    let body = crate::gjapi::request("getGJGauntlets21.php", &format!("&secret={SECRET}&special=1&vkey={}", crate::gjapi::BINARY_VERSION)).await?;
    Ok(body
        .split('#')
        .next()
        .unwrap_or("")
        .split('|')
        .filter(|g| !g.is_empty())
        .map(|g| {
            let m = pairs(g, ":");
            (m.get("1").and_then(|v| v.parse().ok()).unwrap_or(0), m.get("3").cloned().unwrap_or_default())
        })
        .collect())
}

/// A leaderboard row (`getGJScores20.php`): 1 name, 2 player id, 3 stars, 4 demons, 6 rank,
/// 8 creator points, 9 icon, 10/11 colors, 13 coins, 14 icon type, 15 glow, 17 user coins,
/// 46 diamonds, 52 moons.
#[derive(Clone, Debug, Default)]
pub struct Score {
    pub name: String,
    pub rank: i64,
    pub stars: i64,
    pub moons: i64,
    pub demons: i64,
    pub diamonds: i64,
    pub coins: i64,
    pub user_coins: i64,
    pub creator_points: i64,
    pub icon: u32,
    pub icon_type: u32,
    pub color1: usize,
    pub color2: usize,
}

pub fn parse_scores(body: &str) -> Vec<Score> {
    body.split('|')
        .filter(|r| !r.is_empty())
        .map(|r| {
            let m = pairs(r, ":");
            let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            Score {
                name: m.get("1").cloned().unwrap_or_default(),
                rank: int("6"),
                stars: int("3"),
                moons: int("52"),
                demons: int("4"),
                diamonds: int("46"),
                coins: int("13"),
                user_coins: int("17"),
                creator_points: int("8"),
                icon: int("9").max(1) as u32,
                icon_type: int("14") as u32,
                color1: int("10") as usize,
                color2: int("11") as usize,
            }
        })
        .collect()
}

/// `type`: top, relative (global, around you), creators; friends needs an account.
pub async fn scores(kind: &str) -> Result<Vec<Score>, String> {
    let body = crate::gjapi::request("getGJScores20.php", &format!("&type={kind}&count=100&stat=0&secret={SECRET}")).await?;
    Ok(parse_scores(&body))
}

/// A level list (`getGJLevelLists.php`): 1 id, 2 name, 3 description, 7 difficulty,
/// 10 downloads, 14 likes, 19 featured, 50 creator, 51 level ids, 55 diamonds, 56 levels for
/// the reward.
#[derive(Clone, Debug, Default)]
pub struct LevelList {
    pub id: i32,
    pub name: String,
    pub author: String,
    pub difficulty: i32,
    pub downloads: i64,
    pub likes: i64,
    pub levels: String,
    pub diamonds: i32,
    pub needed: i32,
}

pub fn parse_lists(body: &str) -> (Vec<LevelList>, u32) {
    let sections: Vec<&str> = body.split('#').collect();
    let lists = sections
        .first()
        .unwrap_or(&"")
        .split('|')
        .filter(|l| !l.is_empty())
        .map(|l| {
            let m = pairs(l, ":");
            let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            LevelList {
                id: int("1") as i32,
                name: m.get("2").cloned().unwrap_or_default(),
                author: m.get("50").cloned().unwrap_or_else(|| "-".into()),
                difficulty: int("7") as i32,
                downloads: int("10"),
                likes: int("14"),
                levels: m.get("51").cloned().unwrap_or_default(),
                diamonds: int("55") as i32,
                needed: int("56") as i32,
            }
        })
        .collect();
    let total = sections.get(2).unwrap_or(&"").split(':').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (lists, total)
}

pub async fn level_lists(kind: i32, query: &str, page: u32) -> Result<(Vec<LevelList>, u32), String> {
    let (ty, page) = (kind.to_string(), page.to_string());
    let body = crate::gjapi::request("getGJLevelLists.php", &format!("&str={}&type={ty}&page={page}&secret={SECRET}", crate::gjapi::field(query))).await?;
    Ok(parse_lists(&body))
}

/// The three quests (`getGJChallenges.php`): (type 1 orbs / 2 coins / 3 stars, amount,
/// diamonds, name) and the seconds until new ones. The request and answer are XORed with
/// "19847" and base64'd behind 5 salt characters (`GameLevelManager::getGJChallenges`).
pub async fn quests() -> Result<(Vec<(i32, i32, i32, String)>, i64), String> {
    fn xor(data: &[u8]) -> Vec<u8> {
        data.iter().enumerate().map(|(i, b)| b ^ b"19847"[i % 5]).collect()
    }
    // getGJChallenges: the base string, `&world=%i&gdl=%i&secret=%s&chk=%s`
    let chk = crate::gjapi::random_chk("19847");
    let body = crate::gjapi::request("getGJChallenges.php", &format!("&world=0&gdl=0&secret={SECRET}&chk={chk}")).await?;
    let data = body.split('|').next().unwrap_or("");
    let data = data.get(5..).ok_or("short answer")?;
    let raw = cadence_core::codec::base64_decode(data).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&xor(&raw)).into_owned();
    let f: Vec<&str> = text.split(':').collect();
    let left = f.get(5).and_then(|v| v.parse().ok()).unwrap_or(0);
    let quests = f
        .iter()
        .skip(6)
        .filter_map(|q| {
            let p: Vec<&str> = q.splitn(5, ',').collect();
            Some((p.get(1)?.parse().ok()?, p.get(2)?.parse().ok()?, p.get(3)?.parse().ok()?, p.get(4)?.to_string()))
        })
        .collect();
    Ok((quests, left))
}

/// One news post (`/news/news.dat` on the game's content server: base64 + gzip of
/// "version,count|1:id;2:date;3:title;4:blocks|..."; blocks "~"-separated: 0,text /
/// 1,image,w,h,linked,link / 2,link,link).
#[derive(Clone, Debug, Default)]
pub struct News {
    pub date: String,
    pub title: String,
    /// (text, or an image's size)
    pub blocks: Vec<NewsBlock>,
}

#[derive(Clone, Debug)]
pub enum NewsBlock {
    Text(String),
    Image { file: String, w: f32, h: f32 },
    Link(String),
}

pub fn parse_news(data: &str) -> Vec<News> {
    let text = cadence_core::codec::base64_decode(data)
        .ok()
        .and_then(|b| cadence_core::codec::inflate(&b).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    text.split('|')
        .skip(1)
        .map(|item| {
            let mut n = News::default();
            for field in item.split(';') {
                let Some((k, v)) = field.split_once(':') else { continue };
                match k {
                    "2" => n.date = b64_text(v),
                    "3" => n.title = b64_text(v),
                    "4" => {
                        for block in v.split('~') {
                            let p: Vec<&str> = block.split(',').collect();
                            match p.first().copied() {
                                Some("0") => n.blocks.push(NewsBlock::Text(b64_text(p.get(1).copied().unwrap_or("")))),
                                Some("1") => n.blocks.push(NewsBlock::Image {
                                    file: p.get(1).unwrap_or(&"").to_string(),
                                    w: p.get(2).and_then(|v| v.parse().ok()).unwrap_or(800.0),
                                    h: p.get(3).and_then(|v| v.parse().ok()).unwrap_or(450.0),
                                }),
                                Some("2") => n.blocks.push(NewsBlock::Link(b64_text(p.get(1).copied().unwrap_or("")))),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            n
        })
        .collect()
}

pub async fn news() -> Result<Vec<News>, String> {
    let bytes = crate::net::get("https://geometrydashfiles.b-cdn.net/news/news.dat").await?;
    Ok(parse_news(&String::from_utf8_lossy(&bytes)))
}

/// A level comment (`getGJComments21.php`): "2~text~3~player~4~likes~9~age~10~percent..." then
/// ":1~name~9~icon~10~color1~11~color2...".
#[derive(Clone, Debug, Default)]
pub struct Comment {
    pub user: String,
    pub text: String,
    pub likes: i64,
    pub age: String,
    pub percent: i32,
}

pub fn parse_comments(body: &str) -> (Vec<Comment>, u32) {
    let mut sec = body.split('#');
    let list = sec
        .next()
        .unwrap_or("")
        .split('|')
        .filter(|c| !c.is_empty())
        .map(|c| {
            let (a, b) = c.split_once(':').unwrap_or((c, ""));
            let m = pairs(a, "~");
            let u = pairs(b, "~");
            Comment {
                user: u.get("1").cloned().unwrap_or_else(|| "-".into()),
                text: b64_text(m.get("2").map_or("", |s| s.as_str())),
                likes: m.get("4").and_then(|v| v.parse().ok()).unwrap_or(0),
                age: m.get("9").cloned().unwrap_or_default(),
                percent: m.get("10").and_then(|v| v.parse().ok()).unwrap_or(0),
            }
        })
        .collect();
    let total = sec.next().unwrap_or("").split(':').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (list, total)
}

/// A level's comments, page `page` (mode 0 recent, 1 most liked).
pub async fn comments(level: i32, page: u32, liked: bool) -> Result<(Vec<Comment>, u32), String> {
    let (id, page) = (level.to_string(), page.to_string());
    let mode = if liked { "1" } else { "0" };
    let body = crate::gjapi::request("getGJComments21.php", &format!("&page={page}&total=0&secret={SECRET}&mode={mode}&levelID={id}&count=20")).await?;
    Ok(parse_comments(&body))
}

/// A player's profile (`getGJUserInfo20.php` keys) and their posts.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub name: String,
    pub stars: i64,
    pub moons: i64,
    pub diamonds: i64,
    pub coins: i64,
    pub user_coins: i64,
    pub demons: i64,
    pub creator_points: i64,
    pub rank: i64,
    pub color1: usize,
    pub color2: usize,
    /// cube, ship, ball, UFO, wave, robot, spider, swing, jetpack
    pub icons: [u32; 9],
    /// (text, likes, age)
    pub posts: Vec<(String, i64, String)>,
}

pub async fn profile(account: i64) -> Result<Profile, String> {
    let id = account.to_string();
    let body = crate::gjapi::request("getGJUserInfo20.php", &format!("&targetAccountID={id}&secret={SECRET}")).await?;
    let mut p = profile_from(&body);
    if let Ok(body) = crate::gjapi::request("getGJAccountComments20.php", &format!("&accountID={id}&page=0&total=0&secret={SECRET}")).await {
        p.posts = crate::social::parse_posts(&body).into_iter().map(|c| (c.text, c.likes, c.age)).collect();
    }
    Ok(p)
}

/// A `getGJUserInfo20.php` answer's fields.
pub fn profile_from(body: &str) -> Profile {
    let m = pairs(body, ":");
    let int = |k: &str| m.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    Profile {
        name: m.get("1").cloned().unwrap_or_default(),
        stars: int("3"),
        moons: int("52"),
        diamonds: int("46"),
        coins: int("13"),
        user_coins: int("17"),
        demons: int("4"),
        creator_points: int("8"),
        rank: int("30"),
        color1: int("10") as usize,
        color2: int("11") as usize,
        icons: [int("21"), int("22"), int("23"), int("24"), int("25"), int("26"), int("43"), int("53"), int("54")].map(|v| v.max(1) as u32),
        posts: Vec::new(),
    }
}

/// Cadence's own cache folder (never the game's).
#[cfg(not(target_arch = "wasm32"))]
pub fn cache_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/Cadence/songs")
}

/// The name the audio plays a downloaded custom song by: its file in the song cache on the
/// desktop, `net/songs/<id>.mp3` in the page's sound store on the web.
pub fn song_name(id: i32) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    return cache_dir().join(format!("{id}.mp3")).to_string_lossy().into_owned();
    #[cfg(target_arch = "wasm32")]
    return format!("net/songs/{id}.mp3");
}

/// The same for a library sound effect (`s<id>.ogg`).
pub fn sfx_name(id: i32) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    return cache_dir().with_file_name("sfx").join(format!("s{id}.ogg")).to_string_lossy().into_owned();
    #[cfg(target_arch = "wasm32")]
    return format!("net/sfx/s{id}.ogg");
}

/// Whether a downloaded song or sound (by its `song_name` / `sfx_name`) is there to play.
pub fn have(name: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return std::path::Path::new(name).exists();
    #[cfg(target_arch = "wasm32")]
    return crate::audio::web_has_file(name);
}

/// Keep a downloaded song or sound under its name.
fn store(name: &str, bytes: Vec<u8>) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::path::Path::new(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::audio::web_add_file(name, &bytes);
        Ok(())
    }
}

/// A news picture (`news_9_3_b903b61e.jpg` from the news data), downloaded from the news CDN
/// (once, on the desktop) and decoded (JPEG or PNG) to RGBA.
pub async fn news_image(file: &str) -> Result<crate::assets::Image, String> {
    if file.is_empty() || file.contains('/') || file.contains("..") {
        return Err(format!("bad news image name {file:?}"));
    }
    let url = format!("https://geometrydashfiles.b-cdn.net/news/{file}");
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = {
        let dir = cache_dir().with_file_name("news");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(file);
        match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                let b = crate::net::get(&url).await?;
                let _ = std::fs::write(&path, &b);
                b
            }
        }
    };
    #[cfg(target_arch = "wasm32")]
    let bytes = crate::net::get(&url).await?;
    if file.ends_with(".png") {
        return crate::assets::decode_png(bytes, file);
    }
    let options = zune_jpeg::zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(&bytes[..]), options);
    let rgba = decoder.decode().map_err(|e| format!("{file}: {e:?}"))?;
    let (w, h) = decoder.dimensions().ok_or("no size")?;
    Ok(crate::assets::Image { width: w as u32, height: h as u32, rgba })
}

/// A sound effect from the game's SFX library (`/sfx/s<id>.ogg` on the files CDN, the one
/// `sfxlibrary.dat` lists), downloaded once; its `sfx_name`.
pub async fn sfx_file(id: i32) -> Result<String, String> {
    let name = sfx_name(id);
    if !have(&name) {
        store(&name, crate::net::get(&format!("https://geometrydashfiles.b-cdn.net/sfx/s{id}.ogg")).await?)?;
    }
    Ok(name)
}

/// A custom song, downloaded once (from the URL the search gave); its `song_name`.
pub async fn song_file(id: i32, url: &str) -> Result<String, String> {
    let name = song_name(id);
    if have(&name) {
        return Ok(name);
    }
    // a list entry without a usable URL (no scheme): ask getGJSongInfo for the song's own
    let looked_up;
    let url = if url.contains("://") {
        url
    } else {
        crate::app::log(&format!("song {id}: list URL {url:?} has no scheme, asking getGJSongInfo"));
        looked_up = song_info(id).await?.get("10").map(|u| percent_decode(u)).filter(|u| u.contains("://")).ok_or("the song has no download URL")?;
        &looked_up
    };
    store(&name, crate::net::get(url).await?)?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_search_response() {
        let body = "1:10565740:2:Bloodbath:6:503085:9:50:10:195:12:0:14:21:15:3:17:1:18:10:25::35:467339:37:3|1:5:2:X:6:7:9:0:10:1:12:3:35:0#503085:Riot:37415#1~|~467339~|~2~|~At the Speed of Light~|~10~|~https%3A%2F%2Faudio.ngfiles.com%2F467000%2F467339_At_the_Speed_of_Light.mp3~:~#1:0:10#abc";
        let l = parse_levels(body);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].id, l[0].name.as_str(), l[0].author.as_str(), l[0].stars, l[0].demon), (10565740, "Bloodbath", "Riot", 10, true));
        assert_eq!(l[0].song_url.as_deref(), Some("https://audio.ngfiles.com/467000/467339_At_the_Speed_of_Light.mp3"));
        assert_eq!((l[1].official_song, l[1].custom_song), (3, 0));
        let p = parse_page(body);
        assert_eq!((p.total, p.offset), (1, 0));
    }
}

/// With CADENCE_SEARCH="query|query|…": search the servers (most downloaded first) and print
/// each level's id, objects, downloads and name (for finding test levels).
#[cfg(all(test, not(target_arch = "wasm32")))]
mod search_probe {
    #[test]
    fn search() {
        let Ok(queries) = std::env::var("CADENCE_SEARCH") else { return };
        for q in queries.split('|') {
            let mut s = super::Search::new(0);
            s.query = q.into();
            match crate::net::block(super::search_page(&s)) {
                Ok(p) => {
                    for l in p.levels {
                        eprintln!("{q:>14} | {:>10} | {:>8} obj | {:>9} dl | {}", l.id, l.objects, l.downloads, l.name);
                    }
                }
                Err(e) => eprintln!("{q}: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
}

/// With CADENCE_SAVE_LEVELS="id,id,…" and CADENCE_SAVE_DIR_LEVELS=<dir>: download those online
/// levels' data into <dir>/<id>.txt (the format of the game's own level files), for benchmarks
/// (`CADENCE_LEVEL_FILE`) and the web build's development mode. Use `bench-levels/`, never the
/// game's folder (web/gd links to it).
#[cfg(all(test, not(target_arch = "wasm32")))]
mod save_levels {
    #[test]
    fn save() {
        let (Ok(ids), Ok(dir)) = (std::env::var("CADENCE_SAVE_LEVELS"), std::env::var("CADENCE_SAVE_DIR_LEVELS")) else { return };
        for id in ids.split(',').filter_map(|v| v.trim().parse::<i32>().ok()) {
            match crate::net::block(super::download(id)) {
                Ok(bytes) => {
                    std::fs::write(format!("{dir}/{id}.txt"), &bytes).unwrap();
                    eprintln!("{id}: {} bytes", bytes.len());
                }
                Err(e) => eprintln!("{id}: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
}

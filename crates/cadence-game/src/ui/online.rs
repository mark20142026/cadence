//! Online levels, read-only, as the game shows them: `LevelSearchLayer` (the search screen and
//! its quick searches and filters), `LevelBrowserLayer` (a page of `LevelCell`s) and
//! `LevelInfoLayer` (one level: play, stats, song). Requests run on a thread so the menus keep
//! animating; the web build needs the relay (not deployed yet), so it says so.
//! Positions come from the real 1280 × 720 frames (shots/gui-real/creator/, `scene_brief.py`).

use std::sync::mpsc::{Receiver, channel};

use super::canvas::{Canvas, Look};
use super::subpages::button_sprite;
use super::{Btn, Ctx};
use crate::online::{Kind, LevelInfo, Search};

/// A downloaded level ready to play.
#[derive(Clone, Debug)]
pub struct OnlineLevel {
    pub info: LevelInfo,
    /// The level string as the server sends it (base64 + gzip).
    pub data: Vec<u8>,
    /// The song: a path the audio backend reads (an official track, or a cached custom song).
    pub song: Option<String>,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
enum Msg {
    Profile(Box<crate::online::Profile>),
    Comments(Vec<crate::online::Comment>),
    News(Vec<crate::online::News>),
    Quests(Vec<(i32, i32, i32, String)>, i64),
    Scores(Vec<crate::online::Score>),
    Lists(Vec<crate::online::LevelList>, u32),
    Daily(Box<LevelInfo>, i64),
    Gauntlets(Vec<(i32, String)>),
    Packs(Vec<crate::online::MapPack>, u32),
    Results(crate::online::Page),
    Ready(Box<OnlineLevel>),
    Error(String),
}

/// Which of the three screens is up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Search,
    List,
    Info,
    /// `GauntletSelectLayer`, page n (three gauntlets a page)
    Gauntlets(usize),
    /// `LeaderboardsLayer`: 0 top 100, 1 friends, 2 global, 3 creators
    Scores(u8),
}

/// The quick searches (`LevelSearchLayer`, left to right, top to bottom): label, search type,
/// icon, and whether it needs an account (greyed in Cadence).
pub const QUICK: [(&str, i32, &str, bool); 9] = [
    ("Downloads", 1, "GJ_sDownloadIcon_001.png", false),
    ("Likes", 2, "GJ_sLikeIcon_001.png", false),
    ("Hyped", 31, "particle_228_001.png", false),
    ("Trending", 3, "GJ_sTrendingIcon_001.png", false),
    ("Recent", 4, "GJ_sRecentIcon_001.png", false),
    ("Magic", 7, "GJ_sMagicIcon_001.png", false),
    ("Awarded", 11, "GJ_sStarsIcon_001.png", false),
    ("Followed", 12, "GJ_followIcon_001.png", true),
    ("Friends", 13, "GJ_sFriendsIcon_001.png", true),
];

/// The difficulty filter (`-1` N/A, 1..5, `-2` demon, `-3` auto) and the length filter.
const DIFFS: [(i32, f32); 8] = [(-1, -162.8), (1, -120.8), (2, -73.6), (3, -26.4), (4, 20.6), (5, 70.8), (-2, 118.4), (-3, 162.8)];
const LENS: [(&str, f32); 6] = [("Tiny", -122.8), ("Short", -72.6), ("Medium", -13.4), ("Long", 40.7), ("XL", 79.6), ("Plat.", 119.7)];

pub struct Browser {
    /// Each quest's progress (collected since it arrived) and whether it was claimed, filled
    /// by the menus from the progress each frame.
    pub quest_state: Vec<(i64, bool)>,
    /// The Rate Difficulty popup: the stars chosen (0 none).
    pub rating: Option<u8>,
    /// My Levels: the folder shown (0 all), the name filter and whether it's being typed, the
    /// verified levels' slots, and the levels selected with Select All.
    pub my_folder: u8,
    pub my_find: Option<String>,
    pub my_verified: Vec<usize>,
    pub my_selected: bool,
    /// Advanced Options (`gjapi::filter` bits) and whether their popup is open.
    pub filters: u16,
    pub filters_open: bool,
    /// The next Play downloads the level again (the level page's Refresh).
    pub refresh_next: bool,
    /// Levels liked or disliked this session (`GameLevelManager::markItemAsLiked`).
    pub liked: std::collections::HashSet<i64>,
    pub view: View,
    /// The search box.
    pub query: String,
    pub diffs: [bool; 8],
    pub lens: [bool; 6],
    pub star: bool,
    /// The demon filter (`demon_filter`, DemonFilterSelectLayer's tag: 0 all demons, 7 easy,
    /// 8 medium, 6 hard, 9 insane, 10 extreme) and when its popup opened (`t`).
    pub demon_filter: u8,
    pub demon_popup: Option<f32>,
    /// When the level page's popup (`popup`) last changed, for its open animation.
    popup_since: (Option<super::replay::Scene>, f32),
    /// The list's search and title (`LevelBrowserLayer::getSearchTitle`).
    pub search: Search,
    pub title: String,
    pub results: Vec<LevelInfo>,
    pub total: u32,
    pub scroll: f32,
    pub status: String,
    /// The list was opened from the search screen (back returns there) or the creator menu.
    pub from_search: bool,
    /// My Levels: (save slot, name).
    pub my_levels: Vec<(usize, String)>,
    /// The map pack list (search kind −10).
    pub packs: Vec<crate::online::MapPack>,
    /// The list shows a map pack's levels (back returns to the packs).
    pub from_pack: bool,
    /// The gauntlets (type, level ids), and whether the list shows one of them.
    pub gauntlets: Vec<(i32, String)>,
    pub from_gauntlet: bool,
    /// The levels you've beaten and the gauntlets whose chest you've claimed (kept current by
    /// the menus from your progress), for the cards' "N/5" and their chests.
    pub beaten: std::collections::HashSet<i32>,
    pub gauntlets_claimed: std::collections::HashSet<i32>,
    gauntlet_page: usize,
    pub scores: Vec<crate::online::Score>,
    /// Top Lists (search kind −25), and whether the level list came from one.
    pub lists: Vec<crate::online::LevelList>,
    pub from_list: bool,
    /// The level page's level, and its downloaded data once ready.
    pub info: Option<LevelInfo>,
    job: Option<Receiver<Msg>>,
    /// A level finished downloading; the app takes it.
    pub ready: Option<OnlineLevel>,
    pub t: f32,
    /// A popup over the level page (info, leaderboard, vote, song info, audio assets), and the
    /// level's comments once loaded.
    pub popup: Option<super::replay::Scene>,
    pub comments: Option<Vec<crate::online::Comment>>,
    pub profile: Option<crate::online::Profile>,
    /// The profile's account (its follow button)
    pub profile_account: i64,
    /// `NewsPopup` over the main menu: the page shown, and the posts once loaded.
    pub news_page: Option<usize>,
    pub news: Vec<crate::online::News>,
    /// The posts' pictures: downloaded and decoded, waiting for the app to upload them as
    /// textures (frame `news:<file>`), and the fetch still running.
    pub news_pictures: Vec<(String, crate::assets::Image)>,
    news_picture_job: Option<std::sync::mpsc::Receiver<(String, crate::assets::Image)>>,
    /// `GJPathsLayer` over the creator menu.
    pub paths_open: bool,
    /// `ChallengesPage` over the creator menu, and its quests once loaded.
    pub quests_open: bool,
    pub quests: Vec<(i32, i32, i32, String)>,
    pub quests_left: i64,
    /// `DailyLevelPage` over the creator menu: 0 daily, 1 weekly, 2 event; its level and the
    /// seconds left when it loaded.
    pub daily: Option<i32>,
    pub daily_level: Option<LevelInfo>,
    /// The leaderboard's stat to rank by (0 stars, 1 moons, 2 demons, 3 coins).
    pub score_stat: u8,
    /// The event page's bottom line (an index, picked when it opens).
    event_line: usize,
    pub daily_left: i64,
    daily_job: Option<Receiver<Msg>>,
    // kept for the old command-line paths
    pub kind: Kind,
    pub page: u32,
}

impl Default for Browser {
    fn default() -> Browser {
        Browser {
            quest_state: Vec::new(),
            rating: None,
            my_folder: 0,
            my_find: None,
            my_verified: Vec::new(),
            my_selected: false,
            filters: 0,
            filters_open: false,
            liked: Default::default(),
            refresh_next: false,
            view: View::Search,
            query: String::new(),
            diffs: [false; 8],
            lens: [false; 6],
            star: false,
            demon_filter: 0,
            demon_popup: None,
            popup_since: (None, 0.0),
            search: Search::new(6),
            title: String::new(),
            results: Vec::new(),
            total: 0,
            scroll: 0.0,
            status: String::new(),
            from_search: true,
            my_levels: Vec::new(),
            packs: Vec::new(),
            from_pack: false,
            gauntlets: Vec::new(),
            from_gauntlet: false,
            beaten: Default::default(),
            gauntlets_claimed: Default::default(),
            gauntlet_page: 0,
            scores: Vec::new(),
            lists: Vec::new(),
            from_list: false,
            info: None,
            job: None,
            ready: None,
            t: 0.0,
            popup: None,
            comments: None,
            profile: None,
            profile_account: 0,
            news_page: None,
            news: Vec::new(),
            news_pictures: Vec::new(),
            news_picture_job: None,
            paths_open: false,
            quests_open: false,
            quests: Vec::new(),
            quests_left: 0,
            daily: None,
            daily_level: None,
            score_stat: 0,
            event_line: 0,
            daily_left: 0,
            daily_job: None,
            kind: Kind::Featured,
            page: 0,
        }
    }
}

/// The song file for an official song index (key 12; 0 = the first level's track).
pub fn official_song(index: i32) -> Option<String> {
    match index {
        0..=21 => crate::audio::level_song(index + 1),
        _ => None,
    }
}

/// What a click asks the rest of the UI to do.
pub enum Effect {
    None,
    /// Back to the creator menu.
    Leave,
    Alert(String, String),
    Sound(&'static str),
    /// Write a comment on this level (`InfoLayer::onComment`): its id and your best percent there.
    Comment(i64),
    /// Ask before reporting this level (`InfoLayer::onReport`).
    Report(i64),
    /// My Levels' list changed (folder, find); Select All asks to delete them.
    RefreshMine,
    DeleteMine,
    /// A saved level's Delete (asks first), Clone (into your levels) and Options.
    Delete(i32),
    Clone(i32),
    LevelOptions(i32),
    /// A gauntlet beaten: open its chest (`g_N`).
    GauntletChest(i32),
}

impl Browser {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// A local list (My Levels, Saved Levels): no request; `empty` is its tagged empty text.
    pub fn open_local(&mut self, title: &str, empty: &str) {
        self.search = Search::new(-1);
        self.title = title.into();
        self.from_search = false;
        self.view = View::List;
        self.scroll = 0.0;
        self.results.clear();
        self.total = 0;
        self.job = None;
        self.status = empty.into();
    }

    /// Open a list (`LevelBrowserLayer`) for a search.
    pub fn open_list(&mut self, search: Search, from_search: bool) {
        self.title = match search.kind {
            31 => "Hyped".into(),
            _ => "Online Levels".into(),
        };
        self.search = search;
        self.from_search = from_search;
        self.view = View::List;
        self.scroll = 0.0;
        self.fetch_list();
    }

    /// The legacy entry: a category or search by `kind`/`query`/`page`.
    pub fn fetch(&mut self) {
        let mut s = Search::new(match self.kind {
            Kind::Search => 0,
            Kind::MostDownloaded => 1,
            Kind::MostLiked => 2,
            Kind::Trending => 3,
            Kind::Recent => 4,
            Kind::Featured => 6,
            Kind::Magic => 7,
            Kind::Awarded => 11,
            Kind::HallOfFame => 16,
        });
        s.query = self.query.clone();
        s.page = self.page;
        self.open_list(s, false);
    }

    /// The leaderboards on tab `tab`.
    pub fn open_scores(&mut self, tab: u8) {
        self.view = View::Scores(tab);
        self.scroll = 0.0;
        self.scores.clear();
        self.status.clear();
        if tab == 1 {
            self.status = "Friends need an account".into();
            return;
        }
        let kind = match tab {
            2 => "relative",
            3 => "creators",
            _ => "top",
        };
        self.status = "Loading...".into();
        self.spawn(async move {
            match crate::online::scores(kind).await {
                Ok(s) => Msg::Scores(s),
                Err(e) => Msg::Error(format!("Something went wrong ({e})")),
            }
        });
    }

    /// A gauntlet's levels beaten, of how many.
    fn gauntlet_done(&self, levels: &str) -> (usize, usize) {
        let ids: Vec<i32> = levels.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        (ids.iter().filter(|id| self.beaten.contains(id)).count(), ids.len())
    }

    /// The gauntlets.
    pub fn open_gauntlets(&mut self) {
        self.view = View::Gauntlets(0);
        if self.gauntlets.is_empty() && !self.busy() {
            self.status = "Loading...".into();
            self.spawn(async move {
                match crate::online::gauntlets().await {
                    Ok(g) => Msg::Gauntlets(g),
                    Err(e) => Msg::Error(format!("Something went wrong ({e})")),
                }
            });
        }
    }

    /// Top Lists (`getGJLevelLists.php` type 6, the featured lists).
    pub fn open_lists(&mut self) {
        self.search = Search::new(-25);
        self.title = String::new();
        self.from_search = false;
        self.view = View::List;
        self.scroll = 0.0;
        self.fetch_list();
    }

    /// The Map Packs list.
    pub fn open_packs(&mut self) {
        self.search = Search::new(-10);
        self.title = "Map Packs".into();
        self.from_search = false;
        self.view = View::List;
        self.scroll = 0.0;
        self.fetch_list();
    }

    fn fetch_list(&mut self) {
        let s = self.search.clone();
        if s.kind == -26 {
            self.status = "Loading...".into();
            self.lists.clear();
            self.spawn(async move {
                match crate::online::level_lists(0, &s.query, s.page).await {
                    Ok((l, total)) => Msg::Lists(l, total),
                    Err(e) => Msg::Error(format!("Something went wrong ({e})")),
                }
            });
            return;
        }
        if s.kind == -25 {
            self.status = "Loading...".into();
            self.lists.clear();
            self.spawn(async move {
                match crate::online::level_lists(6, "", s.page).await {
                    Ok((l, total)) => Msg::Lists(l, total),
                    Err(e) => Msg::Error(format!("Something went wrong ({e})")),
                }
            });
            return;
        }
        if s.kind == -10 {
            self.status = "Loading...".into();
            self.packs.clear();
            self.spawn(async move {
                match crate::online::map_packs(s.page).await {
                    Ok((p, total)) => Msg::Packs(p, total),
                    Err(e) => Msg::Error(format!("Something went wrong ({e})")),
                }
            });
            return;
        }
        self.status = "Loading...".into();
        self.results.clear();
        self.spawn(async move {
            match crate::online::search_page(&s).await {
                Ok(p) => Msg::Results(p),
                Err(e) if e.contains("returned nothing") => Msg::Results(crate::online::Page::default()),
                Err(e) => Msg::Error(format!("Something went wrong ({e})")),
            }
        });
    }

    /// The search screen's search: the box's text, or the filters alone.
    fn search_from_box(&self, kind: i32) -> Search {
        let mut s = Search::new(kind);
        s.query = self.query.trim().to_string();
        let diffs: Vec<String> = DIFFS.iter().zip(self.diffs).filter(|(_, on)| *on).map(|((d, _), _)| d.to_string()).collect();
        if !diffs.is_empty() {
            s.diff = diffs.join(",");
        }
        // getSearchObject: the demon filter goes with the demon difficulty (1 easy … 5 extreme)
        s.demon_filter = match self.demon_filter { 7 => 1, 8 => 2, 6 => 3, 9 => 4, 10 => 5, _ => 0 };
        let lens: Vec<String> = (0..6).filter(|i| self.lens[*i]).map(|i| i.to_string()).collect();
        if !lens.is_empty() {
            s.len = lens.join(",");
        }
        s.star = self.star;
        s.filters = self.filters;
        s
    }

    /// Download the level page's level and its song, then hand it to the app.
    pub fn download_info(&mut self) {
        let Some(info) = self.info.clone() else { return };
        self.status = "Downloading...".into();
        // a saved level plays from its saved copy, unless Refresh asked for a new one
        let refresh = std::mem::take(&mut self.refresh_next);
        self.spawn(async move {
            let data = match crate::saved::data(info.id).filter(|_| !refresh) {
                Some(d) => d,
                None => match crate::online::download_full(info.id).await {
                    Ok((m, d)) => {
                        crate::saved::save(&info, &d, m.get("27").is_some_and(|p| crate::saved::copyable(p)));
                        d
                    }
                    Err(e) => return Msg::Error(format!("Download failed ({e})")),
                },
            };
            let song = if info.custom_song > 0 {
                match &info.song_url {
                    Some(url) => match crate::online::song_file(info.custom_song, url).await {
                        Ok(p) => Some(p),
                        Err(e) => {
                            crate::app::log(&format!("song {}: {e}", info.custom_song));
                            None
                        }
                    },
                    None => None,
                }
            } else {
                official_song(info.official_song)
            };
            // the songs and library sounds its song (1934) and SFX (3602) triggers play: from
            // the server's lists (keys 52 / 53) and the level's own triggers (key 392)
            let (mut song_ids, mut sfx_ids) = (info.song_ids.clone(), info.sfx_ids.clone());
            if let Ok(text) = cadence_core::codec::decode_level_bytes(&data) {
                for obj in text.split(';').skip(1) {
                    let f: Vec<&str> = obj.split(',').collect();
                    let get = |k: &str| f.chunks(2).find(|kv| kv[0] == k).and_then(|kv| kv.get(1)).and_then(|v| v.parse::<i32>().ok());
                    match get("1") {
                        Some(1934) => song_ids.extend(get("392")),
                        Some(3602) => sfx_ids.extend(get("392")),
                        _ => {}
                    }
                }
            }
            song_ids.sort_unstable();
            song_ids.dedup();
            sfx_ids.sort_unstable();
            sfx_ids.dedup();
            // getGJSongInfo finds each song's URL
            for &id in song_ids.iter().filter(|&&id| id > 0 && id != info.custom_song) {
                if let Err(e) = crate::online::song_file(id, "").await {
                    crate::app::log(&format!("song {id}: {e}"));
                }
            }
            // and the library sounds its SFX triggers play
            for &id in sfx_ids.iter().filter(|&&id| id > 0) {
                if let Err(e) = crate::online::sfx_file(id).await {
                    crate::app::log(&format!("sfx {id}: {e}"));
                }
            }
            Msg::Ready(Box::new(OnlineLevel { info, data, song }))
        });
    }

    /// Kept for the old list's Play button and the command line.
    pub fn download(&mut self, i: usize) {
        self.info = self.results.get(i).cloned();
        self.download_info();
    }

    /// Find and download level `id` right away (command line and screenshots).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn fetch_now(id: i32) -> Result<OnlineLevel, String> {
        let info = crate::net::block(crate::online::search(Kind::Search, &id.to_string(), 0))?.into_iter().find(|l| l.id == id).ok_or("not found")?;
        let mut b = Browser { info: Some(info), ..Browser::default() };
        b.download_info();
        while b.busy() {
            std::thread::sleep(std::time::Duration::from_millis(20));
            b.update(0.0);
        }
        b.ready.ok_or(b.status)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&mut self, work: impl std::future::Future<Output = Msg> + Send + 'static) {
        self.job = Some(crate::net::spawn(work));
    }

    #[cfg(target_arch = "wasm32")]
    fn spawn(&mut self, work: impl std::future::Future<Output = Msg> + 'static) {
        self.job = Some(crate::net::spawn(work));
    }

    /// Fetch the posts' pictures one by one in the background.
    fn fetch_news_pictures(&mut self) {
        let files: Vec<String> = self
            .news
            .iter()
            .flat_map(|n| n.blocks.iter())
            .filter_map(|b| match b {
                crate::online::NewsBlock::Image { file, .. } => Some(file.clone()),
                _ => None,
            })
            .collect();
        // one by one, each sent as it arrives
        let (tx, rx) = channel();
        let _ = crate::net::spawn(async move {
            for file in files {
                match crate::online::news_image(&file).await {
                    Ok(image) => {
                        if tx.send((file, image)).is_err() {
                            return;
                        }
                    }
                    Err(e) => crate::app::log(&format!("news picture {file}: {e}")),
                }
            }
        });
        self.news_picture_job = Some(rx);
    }

    /// Open the news and fetch the posts.
    pub fn open_news(&mut self) {
        self.news_page = Some(0);
        self.t = 0.0;
        if self.news.is_empty() {
            self.daily_job = Some(crate::net::spawn(async move {
                match crate::online::news().await {
                    Ok(n) => Msg::News(n),
                    Err(e) => Msg::Error(e),
                }
            }));
        }
    }

    /// Open the quests and fetch them.
    pub fn open_quests(&mut self) {
        self.quests_open = true;
        self.quests.clear();
        self.t = 0.0;
        self.daily_job = Some(crate::net::spawn(async move {
            match crate::online::quests().await {
                Ok((q, left)) => Msg::Quests(q, left),
                Err(e) => Msg::Error(e),
            }
        }));
    }

    /// Open the daily (0), weekly (1) or event (2) page and fetch its level.
    pub fn open_daily(&mut self, kind: i32) {
        self.daily = Some(kind);
        self.daily_level = None;
        self.t = 0.0;
        self.event_line = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize / 1000);
        self.daily_job = Some(crate::net::spawn(async move {
            match crate::online::daily(kind).await {
                Ok((info, _, left)) => Msg::Daily(Box::new(info), left),
                Err(e) => Msg::Error(e),
            }
        }));
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(rx) = &self.news_picture_job {
            while let Ok(p) = rx.try_recv() {
                self.news_pictures.push(p);
            }
        }
        self.t += dt;
        if self.popup_since.0 != self.popup {
            self.popup_since = (self.popup, self.t);
        }
        if let Some(rx) = &self.daily_job {
            match rx.try_recv() {
                Ok(Msg::Daily(info, left)) => {
                    self.daily_level = Some(*info);
                    self.daily_left = left;
                    self.daily_job = None;
                }
                Ok(Msg::Profile(p)) => {
                    self.profile = Some(*p);
                    self.daily_job = None;
                }
                Ok(Msg::Comments(c)) => {
                    self.comments = Some(c);
                    self.daily_job = None;
                }
                Ok(Msg::News(n)) => {
                    self.news = n;
                    self.daily_job = None;
                    self.fetch_news_pictures();
                }
                Ok(Msg::Quests(q, left)) => {
                    self.quests = q;
                    self.quests_left = left;
                    self.daily_job = None;
                }
                Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.daily_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let Some(rx) = &self.job else { return };
        match rx.try_recv() {
            Ok(Msg::Results(p)) => {
                self.status = if p.levels.is_empty() { "No levels found".into() } else { String::new() };
                self.results = p.levels;
                self.total = p.total;
                self.job = None;
            }
            Ok(Msg::Ready(l)) => {
                self.status.clear();
                self.ready = Some(*l);
                self.job = None;
            }
            Ok(Msg::Error(e)) => {
                self.status = e;
                self.job = None;
            }
            Ok(Msg::Daily(..)) | Ok(Msg::Quests(..)) | Ok(Msg::News(..)) | Ok(Msg::Comments(..)) | Ok(Msg::Profile(..)) => self.job = None,
            Ok(Msg::Lists(l, total)) => {
                self.status = if l.is_empty() { "Nothing found".into() } else { String::new() };
                self.lists = l;
                self.total = total;
                self.job = None;
            }
            Ok(Msg::Scores(sc)) => {
                self.status = if sc.is_empty() { "Nothing found".into() } else { String::new() };
                self.scores = sc;
                self.job = None;
            }
            Ok(Msg::Gauntlets(g)) => {
                self.status.clear();
                self.gauntlets = g;
                self.job = None;
            }
            Ok(Msg::Packs(p, total)) => {
                self.status = if p.is_empty() { "Nothing found".into() } else { String::new() };
                self.packs = p;
                self.total = total;
                self.job = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => {
                self.status = "Something went wrong".into();
                self.job = None;
            }
        }
    }

    pub fn max_scroll(&self) -> f32 {
        if let View::Scores(_) = self.view {
            return (self.scores.len() as f32 * 60.0 - 220.0).max(0.0);
        }
        if self.search.kind == -10 {
            return (self.packs.len() as f32 * 100.0 - 220.0).max(0.0);
        }
        if self.search.kind == -25 {
            return (self.lists.len() as f32 * 90.0 - 220.0).max(0.0);
        }
        (self.results.len() as f32 * 90.0 - 220.0).max(0.0)
    }

    /// A click on one of the browser's buttons.
    pub fn click(&mut self, id: Btn) -> Effect {
        match id {
            Btn::LevelsBack => match self.view {
                View::Info => self.view = View::List,
                View::List if self.from_gauntlet => {
                    self.from_gauntlet = false;
                    self.view = View::Gauntlets(self.gauntlet_page);
                }
                View::List if self.from_list => {
                    self.from_list = false;
                    self.open_lists();
                }
                View::List if self.from_pack => {
                    self.from_pack = false;
                    self.open_packs();
                }
                View::List if self.from_search => self.view = View::Search,
                View::Gauntlets(_) | View::Scores(_) => return Effect::Leave,
                _ => return Effect::Leave,
            },
            Btn::OnlineSearch if !self.busy() => {
                let s = self.search_from_box(0);
                self.open_list(s, true);
            }
            Btn::OnlineClear => self.query.clear(),
            Btn::OnlineQuick(i) => {
                let (_, kind, _, needs_account) = QUICK[i as usize];
                if needs_account {
                    // without an account the game shows an empty list
                    self.open_local("Online Levels", "");
                    self.from_search = true;
                    return Effect::None;
                }
                if !self.busy() {
                    let mut s = self.search_from_box(kind);
                    s.query.clear();
                    self.open_list(s, true);
                }
            }
            Btn::OnlineDiff(i) => {
                // LevelSearchLayer::toggleDifficulty: N/A, demon and auto go alone; the five
                // star difficulties go together
                let i = i as usize;
                let on = !self.diffs[i];
                if on {
                    for k in 0..8 {
                        if k != i && (matches!(i, 0 | 6 | 7) || matches!(k, 0 | 6 | 7)) {
                            self.diffs[k] = false;
                        }
                    }
                }
                self.diffs[i] = on;
            }
            Btn::OnlineDemonFilter if self.diffs[6] => self.demon_popup = Some(self.t),
            Btn::OnlineDemonPick(tag) => self.demon_filter = tag,
            Btn::OnlineDemonOk => self.demon_popup = None,
            Btn::OnlineLen(i) => self.lens[i as usize] = !self.lens[i as usize],
            Btn::OnlineStar => self.star = !self.star,
            Btn::OnlineClearFilters => {
                self.demon_filter = 0;
                self.diffs = [false; 8];
                self.lens = [false; 6];
                self.star = false;
            }
            // the user search (`getGJUsers20.php`): the best match's profile
            Btn::OnlineUsers => {
                let query = self.query.trim().to_string();
                if query.is_empty() {
                    return Effect::None;
                }
                self.popup = Some(super::replay::Scene::Profile);
                self.profile = None;
                self.t = 0.0;
                self.daily_job = Some(crate::net::spawn(async move {
                    let id = crate::gjapi::identity();
                    let found = match crate::net::post("getGJUsers20.php", &crate::gjapi::user_search_form(&id, &query, 0)).await {
                        Ok(body) => crate::social::parse_users(&body).into_iter().find(|u| u.account_id > 0),
                        Err(_) => None,
                    };
                    match found {
                        Some(u) => match crate::online::profile(u.account_id).await {
                            Ok(p) => Msg::Profile(Box::new(p)),
                            Err(e) => Msg::Error(e),
                        },
                        None => Msg::Error("Nothing found".into()),
                    }
                }));
            }
            // level lists by name
            Btn::OnlineLists => {
                let mut s = Search::new(-26);
                s.query = self.query.trim().to_string();
                self.search = s;
                self.title = "Lists".into();
                self.from_search = true;
                self.view = View::List;
                self.scroll = 0.0;
                self.fetch_list();
            }
            Btn::OnlineFilters => self.filters_open = true,
            Btn::FilterToggle(i) => self.filters ^= 1 << i,
            Btn::FilterClose => self.filters_open = false,
            Btn::LevelsPrev if self.search.page > 0 && !self.busy() => {
                self.search.page -= 1;
                self.scroll = 0.0;
                self.fetch_list();
            }
            Btn::LevelsNext if !self.busy() && (self.search.page + 1) * 10 < self.total => {
                self.search.page += 1;
                self.scroll = 0.0;
                self.fetch_list();
            }
            Btn::OnlineRefresh if !self.busy() && self.search.kind != -1 => self.fetch_list(),
            Btn::MyTool(k) => {
                match k {
                    // your uploaded levels (search type 5 by your player id)
                    0 => {
                        let Some(user) = crate::gjapi::identity().account.map(|a| a.user_id) else {
                            return Effect::Alert("My Server".into(), "You need an <cl>account</c> to see your uploaded levels.".into());
                        };
                        let mut s = Search::new(5);
                        s.query = user.to_string();
                        self.open_list(s, false);
                        self.title = "My Levels".into();
                    }
                    1 => return Effect::Alert("Import".into(), "Drop a level file (<cy>.gmd</c>) onto the window to import it into your levels.".into()),
                    2 => return Effect::Alert("Backup".into(), "Your levels are kept in Cadence's save on this device, and in your account's cloud save when you <cg>Save</c> from the Account page.".into()),
                    3 => return Effect::Alert("My Lists".into(), "Lists group levels together. Make one from a level's page with the <cg>Lists</c> button.".into()),
                    4 => {
                        self.my_folder = (self.my_folder + 1) % 10;
                        return Effect::RefreshMine;
                    }
                    5 => self.my_find = Some(self.my_find.take().map_or(String::new(), |_| String::new())),
                    _ => {
                        self.my_selected = !self.my_selected;
                        if self.my_selected && !self.my_levels.is_empty() {
                            return Effect::DeleteMine;
                        }
                    }
                }
            }
            Btn::OnlineHelp => {
                return Effect::Alert("Online Levels".into(), "Levels other players made, straight from the game's servers. Tap <cg>Get It</c> to see a level and play it.".into());
            }
            Btn::ScoreTab(t) if !self.busy() => self.open_scores(t),
            Btn::GauntletCard(i) => {
                if let Some((kind, levels)) = self.gauntlets.get(i as usize).cloned() {
                    let (done, count) = self.gauntlet_done(&levels);
                    if done == count && count > 0 && !self.gauntlets_claimed.contains(&kind) {
                        return Effect::GauntletChest(kind);
                    }
                    if let View::Gauntlets(p) = self.view {
                        self.gauntlet_page = p;
                    }
                    let mut s = Search::new(10);
                    s.query = levels;
                    self.open_list(s, false);
                    self.title = format!("{} Gauntlet", super::gauntlets::info(kind).0);
                    self.from_gauntlet = true;
                }
            }
            Btn::GauntletPage(next) => {
                if let View::Gauntlets(p) = self.view {
                    let pages = self.gauntlets.len().div_ceil(3).max(1);
                    self.view = View::Gauntlets(if next { (p + 1) % pages } else { (p + pages - 1) % pages });
                }
            }
            Btn::OnlineList(i) => {
                if let Some(l) = self.lists.get(i as usize).cloned() {
                    let mut s = Search::new(10);
                    s.query = l.levels;
                    self.open_list(s, false);
                    self.title = l.name;
                    self.from_list = true;
                }
            }
            Btn::OnlinePack(i) => {
                if let Some(p) = self.packs.get(i as usize) {
                    // LevelBrowserLayer for the pack's levels (search type 10, the ids)
                    let mut s = Search::new(10);
                    s.query = p.levels.clone();
                    let name = p.name.clone();
                    self.open_list(s, false);
                    self.title = name;
                    self.from_pack = true;
                }
            }
            Btn::OnlineView(i) => {
                if let Some(l) = self.results.get(i as usize) {
                    self.info = Some(l.clone());
                    self.view = View::Info;
                }
            }
            Btn::InfoPlay if !self.busy() => {
                self.download_info();
                return Effect::Sound(super::PLAY_SOUND);
            }
            Btn::InfoSide(2) => {
                self.popup = Some(super::replay::Scene::Info);
                self.comments = None;
            }
            Btn::InfoSide(3) => self.popup = Some(super::replay::Scene::Leaderboard),
            // the creator's name: ProfilePage (registered players only)
            Btn::InfoAuthor => {
                let Some(acc) = self.info.as_ref().map(|l| l.account_id) else { return Effect::None };
                if acc <= 0 {
                    return Effect::Alert("Unregistered".into(), "<cg>Green</c> users are unregistered: they have no profile.".into());
                }
                self.popup = Some(super::replay::Scene::Profile);
                self.profile = None;
                self.daily_job = Some(crate::net::spawn(async move {
                    match crate::online::profile(acc).await {
                        Ok(p) => Msg::Profile(Box::new(p)),
                        Err(e) => Msg::Error(e),
                    }
                }));
            }
            Btn::InfoSide(4) => self.popup = Some(super::replay::Scene::Vote),
            Btn::InfoSide(13) => self.rating = Some(0),
            Btn::InfoSide(14) => self.rating = Some(100),
            Btn::RateStar(n) => self.rating = Some(n),
            Btn::RateCancel => self.rating = None,
            // RateStarsLayer::onRate: rateGJStars211 (the stars suggested), then closes
            // RateDemonLayer: easy … extreme (1–5) → rateGJDemon21
            Btn::RateSubmit if self.rating.is_some_and(|n| n > 100) => {
                if let (Some(n), Some(l)) = (self.rating, &self.info) {
                    let (level, rating) = (l.id as i64, (n - 100) as i32);
                    let _ = crate::net::spawn(async move {
                        let id = crate::gjapi::identity();
                        let _ = crate::net::post("rateGJDemon21.php", &crate::gjapi::rate_demon_form(&id, level, rating)).await;
                    });
                    self.rating = None;
                    return Effect::Alert("Rate Demon".into(), "Thank you for your vote.".into());
                }
            }
            Btn::RateSubmit => {
                if let (Some(n), Some(l)) = (self.rating.filter(|n| *n > 0 && *n <= 10), &self.info) {
                    let level = l.id as i64;
                    let _ = crate::net::spawn(async move {
                        let id = crate::gjapi::identity();
                        let _ = crate::net::post("rateGJStars211.php", &crate::gjapi::rate_stars_form(&id, level, n as i32, &crate::gjapi::random_string(10))).await;
                    });
                    self.rating = None;
                    return Effect::Alert("Rate Difficulty".into(), format!("You suggested <cy>{n}</c> star{} for this level.", if n == 1 { "" } else { "s" }));
                }
            }
            // the level page's side buttons on a saved level: delete, refresh, clone, options,
            // favorite, folder
            Btn::InfoSide(k @ (0 | 1 | 6 | 7 | 9 | 10 | 11)) => {
                let Some(l) = self.info.clone() else { return Effect::None };
                let saved = crate::saved::get(l.id);
                match (k, saved) {
                    (_, None) if k != 1 => return Effect::Alert("Saved Levels".into(), "Play the level first: it's saved when it downloads.".into()),
                    (0, _) => return Effect::Delete(l.id),
                    (1, _) => {
                        self.refresh_next = true;
                        return Effect::Alert("Refresh".into(), "The level downloads again the next time you play it.".into());
                    }
                    (6, Some(s)) if s.copyable => return Effect::Clone(l.id),
                    (6, Some(_)) => return Effect::Alert("Copy".into(), "The creator of this level doesn't allow copying it.".into()),
                    (7, _) => return Effect::LevelOptions(l.id),
                    (9, Some(s)) => {
                        crate::saved::update(l.id, |x| x.favorite = !x.favorite);
                        return Effect::Alert("Favorite".into(), if s.favorite { "Removed from your favorites." } else { "Added to your favorites." }.into());
                    }
                    (10, Some(s)) => {
                        let folder = (s.folder + 1) % 10;
                        crate::saved::update(l.id, |x| x.folder = folder);
                        return Effect::Alert("Folder".into(), if folder == 0 { "Not in a folder.".to_string() } else { format!("Moved to folder <cy>{folder}</c>.") });
                    }
                    _ => return Effect::Alert("Lists".into(), "Make a list from <cg>My Lists</c> to add saved levels to it.".into()),
                }
            }
            Btn::InfoSide(k) => {
                let (title, text) = match k {
                    8 => ("Help", "Tap <cg>Play</c> to download and play the level. Its song downloads with it."),
                    12 => ("Icon Kit", "Open the icon kit from the main menu."),
                    2 => ("Level Info", ""),
                    3 => ("Leaderboard", "Level leaderboards open from the level page's leaderboard button."),
                    _ => ("Rate", "Rating levels is on the level page's like button."),
                };
                if k == 2 {
                    if let Some(l) = &self.info {
                        let desc = if l.description.is_empty() { "(No description provided)".to_string() } else { l.description.clone() };
                        return Effect::Alert(
                            l.name.clone(),
                            format!("{desc}\n\n<cy>Level ID</c>: {}\n<cl>Objects</c>: {}{}", l.id, l.objects, if l.original > 0 { format!("\n<co>Original</c>: {}", l.original) } else { String::new() }),
                        );
                    }
                }
                return Effect::Alert(title.into(), text.into());
            }
            Btn::InfoSongMore => self.popup = Some(super::replay::Scene::SongInfo),
            Btn::InfoSongAssets => self.popup = Some(super::replay::Scene::Assets),
            _ => {}
        }
        Effect::None
    }

    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx) {
        match self.view {
            View::Search => self.draw_search(c, ctx),
            View::List => self.draw_list(c, ctx),
            View::Info => self.draw_info(c, ctx),
            View::Gauntlets(p) => self.draw_gauntlets(c, ctx, p),
            View::Scores(t) => self.draw_scores(c, ctx, t),
        }
        if self.view != View::Info && self.popup == Some(super::replay::Scene::Profile) {
            self.draw_profile_over(c, ctx);
        }
        if self.filters_open {
            self.draw_filters(c, ctx);
        }
    }

    /// `RateStarsLayer` ("Rate Difficulty"): 1–10 stars, the chosen one green, Submit / Cancel.
    fn draw_rating(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        let chosen = self.rating.unwrap_or(0);
        ctx.buttons.clear();
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, cy, 360.0, 200.0, Look::default());
        let demon = chosen >= 100;
        c.text("goldFont", if demon { "Rate Demon" } else { "Rate Difficulty" }, cx, cy + 75.0, Look::scaled(0.8));
        // the demon faces: easy, medium, hard, insane, extreme (7, 8, 6, 9, 10 as the game numbers them)
        if demon {
            for (k, face) in [7, 8, 6, 9, 10].into_iter().enumerate() {
                let x = cx - 120.0 + 60.0 * k as f32;
                let id = Btn::RateStar(101 + k as u8);
                let f = ctx.factor(id);
                c.sprite(&format!("difficulty_{face:02}_btn_001.png"), x, cy + 15.0, Look::scaled(0.8 * f));
                if chosen == 101 + k as u8 {
                    c.sprite("GJ_select_001.png", x, cy + 15.0, Look::scaled(1.2));
                }
                ctx.area(id, x, cy + 15.0, 50.0, 50.0);
            }
        }
        for i in (0..10u8).filter(|_| !demon) {
            let x = cx - 92.0 + 46.0 * (i % 5) as f32;
            let y = if i < 5 { cy + 30.0 } else { cy - 5.0 };
            let f = ctx.factor(Btn::RateStar(i + 1));
            c.nine_slice(if chosen == i + 1 { "GJ_button_01.png" } else { "GJ_button_04.png" }, x, y, 28.8 * f, 24.0 * f, Look::default());
            c.text("bigFont", &(i + 1).to_string(), x - 0.8, y + 1.6, Look::scaled(0.4 * f));
            ctx.area(Btn::RateStar(i + 1), x, y, 28.8, 24.0);
        }
        let (w1, w2) = (super::login::button_width(c, "Cancel"), super::login::button_width(c, "Submit"));
        let total = w1 + 15.0 + w2;
        for (id, text, x, bw) in [(Btn::RateCancel, "Cancel", cx - total * 0.5 + w1 * 0.5, w1), (Btn::RateSubmit, "Submit", cx + total * 0.5 - w2 * 0.5, w2)] {
            let f = ctx.factor(id);
            c.nine_slice("GJ_button_01.png", x, cy - 65.0, bw * f, 31.5 * f, Look::default());
            c.text("goldFont", text, x, cy - 63.0, Look::scaled(f));
            ctx.area(id, x, cy - 65.0, bw, 31.5);
        }
    }

    /// `MoreSearchLayer` ("Advanced Options"): the filters as check boxes, two columns.
    fn draw_filters(&self, c: &mut Canvas, ctx: &mut Ctx) {
        const NAMES: [&str; 10] = ["Uncompleted", "Completed", "Featured", "Original", "2-Player", "Coins", "No Star", "Epic", "Legendary", "Mythic"];
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        ctx.buttons.clear();
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, cy, 440.0, 290.0, Look::default());
        c.text("goldFont", "Advanced Options", cx, cy + 120.0, Look::scaled(0.8));
        for (i, name) in NAMES.iter().enumerate() {
            let x = cx - 165.0 + 200.0 * (i / 5) as f32;
            let y = cy + 75.0 - 37.0 * (i % 5) as f32;
            let on = self.filters & (1 << i) != 0;
            let f = ctx.factor(Btn::FilterToggle(i as u8));
            c.sprite(if on { "GJ_checkOn_001.png" } else { "GJ_checkOff_001.png" }, x, y, Look::scaled(0.8 * f));
            ctx.area(Btn::FilterToggle(i as u8), x, y, 30.0, 30.0);
            c.text("bigFont", name, x + 22.0, y, Look::scaled(0.5).anchor(0.0, 0.5));
        }
        let bw = super::login::button_width(c, "OK");
        let f = ctx.factor(Btn::FilterClose);
        c.nine_slice("GJ_button_01.png", cx, cy - 120.0, bw * f, 31.5 * f, Look::default());
        c.text("goldFont", "OK", cx, cy - 118.0, Look::scaled(f));
        ctx.area(Btn::FilterClose, cx, cy - 120.0, bw, 31.5);
    }

    /// `LevelSearchLayer::init` (0x6154b8).
    fn draw_search(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        background(c);
        let panel = |c: &mut Canvas, x: f32, y: f32, pw: f32, ph: f32, col: [u8; 3]| c.nine_slice("square02b_001.png", x, y, pw, ph, Look::default().rgb(col));
        panel(c, cx, h - 30.0, 365.0, 40.0, [0, 56, 141]);
        c.nine_slice("square02b_small.png", cx - 73.0, h - 30.0, 210.0, 30.0, Look::default().rgb([0, 39, 98]));
        panel(c, cx, cy + 28.0, 365.0, 115.0, [0, 46, 117]);
        panel(c, cx, cy - 77.0, 365.0, 50.0, [0, 36, 91]);
        panel(c, cx, cy - 128.0, 365.0, 35.0, [0, 31, 79]);
        // the search box
        if self.query.is_empty() {
            c.text("bigFont", "Enter a level, user or id", cx - 171.0, h - 30.0, Look::scaled(0.439).anchor(0.0, 0.5).rgb([108, 153, 216]));
        } else {
            let caret = if (self.t * 2.0) as i32 % 2 == 0 { "_" } else { "" };
            c.text("bigFont", &format!("{}{caret}", self.query), cx - 171.0, h - 30.0, Look::scaled(0.5).anchor(0.0, 0.5));
        }
        ctx.button(c, Btn::OnlineSearch, "GJ_longBtn06_001.png", cx + 63.0, h - 30.0, Look::default(), 1.2);
        ctx.button(c, Btn::OnlineUsers, "GJ_longBtn05_001.png", cx + 110.0, h - 30.0, Look::default(), 1.2);
        ctx.button(c, Btn::OnlineClear, "GJ_longBtn07_001.png", cx + 157.0, h - 30.0, Look::default(), 1.2);
        c.text("bigFont", "Quick Search", cx, cy + 97.5, Look::scaled(0.5));
        for (i, (label, _, icon, grey)) in QUICK.iter().enumerate() {
            let x = cx + [-119.0, 0.0, 119.0][i % 3];
            let y = cy + [64.0, 28.0, -8.0][i / 3];
            let id = Btn::OnlineQuick(i as u8);
            let f = ctx.factor(id);
            let _ = grey;
            c.sprite("GJ_longBtn04_001.png", x, y, Look::scaled(f));
            let (tw, _) = c.text_size("bigFont", label);
            let ts = (78.5 / tw.max(1.0)).min(0.5);
            let (iw, _) = c.size(icon);
            // the label is centered 10.3 left of the button's middle, the icon 5 after it
            let lc = x - 10.3 * f;
            c.text("bigFont", label, lc, y + 1.0, Look::scaled(ts * f));
            c.sprite(icon, lc + (tw * ts * 0.5 + 5.0 + iw * 0.55) * f, y, Look::scaled(1.1 * f));
            ctx.area(id, x, y, 112.5, 30.5);
        }
        c.text("bigFont", "Filters", cx, cy - 40.0, Look::scaled(0.5));
        for (i, (_, dx)) in DIFFS.iter().enumerate() {
            let name = match i {
                0 => "difficulty_00_btn_001.png".to_string(),
                7 => "difficulty_auto_btn_001.png".to_string(),
                6 if self.demon_filter != 0 => format!("difficulty_{:02}_btn2_001.png", self.demon_filter),
                n => format!("difficulty_{n:02}_btn_001.png"),
            };
            let on = self.diffs[i];
            let id = Btn::OnlineDiff(i as u8);
            let f = ctx.factor(id);
            let look = if on { Look::scaled(0.8 * f) } else { Look::scaled(0.8 * f).rgb([125, 125, 125]) };
            c.sprite(&name, cx + dx, cy - 77.0, look);
            ctx.area(id, cx + dx, cy - 77.0, 34.0, 36.0);
        }
        // the demon filter's + (shown with the demon difficulty on)
        if self.diffs[6] {
            ctx.button(c, Btn::OnlineDemonFilter, "GJ_plus2Btn_001.png", cx + 202.5, cy - 77.0, Look::default(), 1.26);
        }
        c.sprite("GJ_timeIcon_001.png", cx - 162.1, cy - 128.0, Look::default());
        for (i, (label, dx)) in LENS.iter().enumerate() {
            let id = Btn::OnlineLen(i as u8);
            let f = ctx.factor(id);
            let col = if self.lens[i] { [255, 255, 255] } else { [125, 125, 125] };
            c.text("bigFont", label, cx + dx, cy - 128.0, Look::scaled(0.42 * f).rgb(col));
            let (tw, _) = c.text_size("bigFont", label);
            ctx.area(id, cx + dx, cy - 128.0, tw * 0.42 + 6.0, 20.0);
        }
        let col = if self.star { [255, 255, 255] } else { [125, 125, 125] };
        let f = ctx.factor(Btn::OnlineStar);
        c.sprite("GJ_starsIcon_001.png", cx + 161.9, cy - 128.0, Look::scaled(f).rgb(col));
        ctx.area(Btn::OnlineStar, cx + 161.9, cy - 128.0, 28.0, 28.0);
        ctx.button(c, Btn::OnlineClearFilters, "GJ_closeBtn_001.png", cx + 259.5, cy + 135.0, Look::scaled(0.8), 1.2);
        ctx.button(c, Btn::OnlineFilters, "GJ_plusBtn_001.png", cx + 259.5, cy + 85.0, Look::scaled(0.8), 1.2);
        ctx.button(c, Btn::OnlineLists, "GJ_viewListsBtn_001.png", cx + 259.5, cy + 35.0, Look::scaled(0.8), 1.2);
        side_art(c);
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 24.0, h - 23.0, Look::default(), 1.6);
        if let Some(t0) = self.demon_popup {
            ctx.buttons.clear();
            let start = c.mark();
            self.draw_demon_filter(c, ctx);
            super::open_anim(c, start, self.t - t0);
        }
    }

    /// `DemonFilterSelectLayer::init`: GJ_square01 380 × 180, "Demon Filter" 20 under its top,
    /// the six faces (scale 1.1) in a row 18 apart 3 over the middle, the chosen one white and
    /// the rest grey, OK 25 over the bottom. Backdrop opacity 75.
    fn draw_demon_filter(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.rect(-1.0, -1.0, w + 2.0, h + 2.0, [0.0; 3], 75.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, cy, 380.0, 180.0, Look::default());
        c.text("bigFont", "Demon Filter", cx, cy + 70.0, Look::default());
        let faces: [(u8, String); 6] = [
            (0, "difficulty_06_btn_001.png".into()),
            (7, "difficulty_07_btn2_001.png".into()),
            (8, "difficulty_08_btn2_001.png".into()),
            (6, "difficulty_06_btn2_001.png".into()),
            (9, "difficulty_09_btn2_001.png".into()),
            (10, "difficulty_10_btn2_001.png".into()),
        ];
        let widths: Vec<f32> = faces.iter().map(|(_, f)| c.size(f).0 * 1.1).collect();
        let total: f32 = widths.iter().sum::<f32>() + 18.0 * 5.0;
        let mut x = cx - total * 0.5;
        for ((tag, frame), fw) in faces.iter().zip(&widths) {
            let id = Btn::OnlineDemonPick(*tag);
            let f = ctx.factor(id);
            let col = if self.demon_filter == *tag { [255, 255, 255] } else { [125, 125, 125] };
            let (_, fh) = c.size(frame);
            c.sprite(frame, x + fw * 0.5, cy + 3.0, Look::scaled(1.1 * f).rgb(col));
            ctx.area(id, x + fw * 0.5, cy + 3.0, *fw, fh * 1.1);
            x += fw + 18.0;
        }
        // the OK ButtonSprite: the whole node scales when pressed (border and text together)
        let f = ctx.factor(Btn::OnlineDemonOk);
        let by = cy - 65.0;
        let start = c.mark();
        c.nine_slice("GJ_button_01.png", cx, by, 60.0, 32.0, Look::default());
        c.text("goldFont", "OK", cx, by + 1.0, Look::scaled(0.8));
        c.transform_since(start, f, (cx, by), (0.0, 0.0));
        ctx.area(Btn::OnlineDemonOk, cx, by, 60.0, 32.0);
    }

    /// `LeaderboardsLayer` (0x... init): four tab toggles above the list, `GJScoreCell`s 60 high.
    fn draw_scores(&self, c: &mut Canvas, ctx: &mut Ctx, tab: u8) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        background(c);
        let (lx, ly) = ((w - 356.0) * 0.5, (h - 220.0) * 0.5 - 5.0);
        c.rect(lx, ly, 356.0, 220.0, [191.0 / 255.0, 114.0 / 255.0, 62.0 / 255.0], 1.0);
        let top = ly + 220.0 - 1.4;
        for (i, s) in self.scores.iter().enumerate() {
            let y = top - 60.0 * (i as f32 + 1.0) + self.scroll;
            if y + 60.0 <= ly || y >= ly + 220.0 {
                continue;
            }
            let (y0, y1) = (y.max(ly), (y + 60.0).min(ly + 220.0));
            let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
            c.rect(lx, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
            let my = y + 30.0;
            if my < ly + 6.0 || my > ly + 214.0 {
                continue;
            }
            // GJScoreCell (captured): icon and rank at +30, the name from +60 (at most 120 wide,
            // 0.8), stars and moons right after it, the other counts on a second line; every
            // count shrinks to fit its slot, more when the creator points show
            let base = super::garage::icon_sheet(0, s.icon.clamp(1, 485));
            super::garage::draw_icon(c, &base, lx + 30.0, my + 7.4, 0.95, super::menu::color_for_idx(s.color1), super::menu::color_for_idx(s.color2));
            c.text("goldFont", &s.rank.to_string(), lx + 30.0, my - 17.6, Look::scaled(0.65));
            let (nw, _) = c.text_size("goldFont", &s.name);
            let ns = (120.0 / nw.max(1.0)).min(0.8);
            c.text("goldFont", &s.name, lx + 60.0, my + 14.4, Look::scaled(ns).anchor(0.0, 0.5));
            let creator = s.creator_points > 0;
            let top = if creator { 0.45 } else { 0.5 };
            let fit = |c: &Canvas, t: &str, max: f32, limit: f32| -> (f32, f32) {
                let (tw, _) = c.text_size("bigFont", t);
                let sc = (limit / tw.max(1.0)).min(max);
                (sc, tw * sc)
            };
            let mut x = lx + 60.0 + nw * ns + 5.0;
            for (v, icon) in [(s.stars, "GJ_starsIcon_001.png"), (s.moons, "GJ_moonsIcon_001.png")] {
                let t = commas(v);
                let (sc, tw) = fit(c, &t, top, 58.0);
                c.text("bigFont", &t, x, my + 12.4, Look::scaled(sc).anchor(0.0, 0.5));
                c.sprite(icon, x + tw + 12.0, my + 12.4, Look::scaled(0.7));
                x += tw + 28.2;
            }
            let mut x = lx + 62.0;
            let limits: [f32; 5] = if creator { [35.5, 99.0, 38.4, 43.3, 99.0] } else { [49.3, 99.0, 59.4, 55.0, 99.0] };
            for (k, (v, icon)) in [
                (s.diamonds, "GJ_diamondsIcon_001.png"),
                (s.coins, "GJ_coinsIcon_001.png"),
                (s.user_coins, "GJ_coinsIcon2_001.png"),
                (s.demons, "GJ_demonIcon_001.png"),
                (s.creator_points, "GJ_hammerIcon_001.png"),
            ]
            .into_iter()
            .enumerate()
            {
                if k == 4 && !creator {
                    break;
                }
                let t = commas(v);
                let (sc, tw) = fit(c, &t, top, limits[k]);
                c.text("bigFont", &t, x, my - 12.6, Look::scaled(sc).anchor(0.0, 0.5));
                c.sprite(icon, x + tw + 9.5, my - 12.6, Look::scaled(0.7));
                x += tw + 25.0;
            }
        }
        // the stat to rank by (stars, moons, demons, coins): 40 × 40 buttons at 0.6, the chosen
        // one cyan (GJ_button_02), the others green
        for (k, icon) in ["GJ_starsIcon_001.png", "GJ_moonsIcon_001.png", "GJ_demonIcon_001.png", "GJ_coinsIcon2_001.png"].into_iter().enumerate() {
            let (bx, by) = (cx + 220.0, cy + 52.5 - 35.0 * k as f32);
            let f = ctx.factor(Btn::ScoreStat(k as u8));
            let bg = if k as u8 == self.score_stat { "GJ_button_02.png" } else { "GJ_button_01.png" };
            c.nine_slice(bg, bx, by, 24.0 * f, 24.0 * f, Look::default());
            c.sprite(icon, bx, by + 0.3, Look::scaled(0.72 * f));
            ctx.area(Btn::ScoreStat(k as u8), bx, by, 24.0, 24.0);
        }
        if !self.status.is_empty() {
            if self.busy() {
                loading_circle(c, cx, cy - 5.0, self.t);
            } else {
                c.text("goldFont", &self.status, cx, cy - 5.0, Look::scaled(0.6));
            }
        }
        super::options::list_border(c, lx, ly, 356.0, 220.0, "");
        // the tabs (CCMenuItemToggler: on / off sprites)
        for (k, (name, x)) in [("top", 142.0), ("friends", 237.0), ("global", 332.0), ("creators", 427.0)].into_iter().enumerate() {
            let on = k as u8 == tab;
            let frame = format!("{name}_tab_{}_001.png", if on { "on" } else { "off" });
            ctx.button(c, Btn::ScoreTab(k as u8), &frame, x + (w - 569.0) * 0.5, h - 27.0, Look::default(), 1.1);
        }
        side_art(c);
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 24.0, h - 23.0, Look::default(), 1.6);
        ctx.button(c, Btn::OnlineHelp, "GJ_infoIcon_001.png", 30.0, 30.0, Look::default(), 1.5);
    }

    /// `GauntletSelectLayer`: three `GauntletNode` cards (110 × 234, the gauntlet's color) a
    /// page, page dots 16 apart, gray gradient with corners.
    fn draw_gauntlets(&self, c: &mut Canvas, ctx: &mut Ctx, page: usize) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        let (gw, gh) = c.size("GJ_gradientBG.png");
        if gw > 0.0 {
            c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb([51, 51, 51]));
        }
        c.sprite("theLostGauntletsLabel_001.png", cx, h - 28.0, Look::default());
        if self.busy() {
            loading_circle(c, cx, cy, self.t);
        }
        for k in 0..3 {
            let i = page * 3 + k;
            let Some((kind, levels)) = self.gauntlets.get(i) else { break };
            let (name, island, color) = super::gauntlets::info(*kind);
            let x = cx + (k as f32 - 1.0) * 112.0;
            let y = cy - 11.0;
            let id = Btn::GauntletCard(i as u8);
            let f = ctx.factor(id);
            c.nine_slice("GJ_square06.png", x, y, 110.0 * f, 234.0 * f, Look::default().rgb(color));
            let shadow = Look::default().rgb([0, 0, 0]).alpha(60);
            let gold = [255, 200, 0];
            for (dx, dy, look) in [(2.0, -3.0, shadow), (0.0, 0.0, Look::default())] {
                let tint = |l: Look, s: f32| if dx == 0.0 { Look { scale: (s * f, s * f), ..l }.rgb(gold) } else { Look { scale: (s * f, s * f), ..l } };
                c.text("bigFont", name, x + dx, y + 94.0 + dy, tint(look, 0.7));
                c.text("bigFont", "Gauntlet", x + dx, y + 75.0 + dy, tint(look, 0.45));
                c.sprite(island, x + dx, y + 16.0 + dy, Look { scale: (f, f), ..look });
                let (done, count) = self.gauntlet_done(levels);
                c.text("bigFont", &format!("{done}/{count}"), x + dx, y - 24.0 + dy, Look { scale: (0.4 * f, 0.4 * f), ..look });
                c.text("goldFont", "Reward", x + dx, y - 47.0 + dy, Look { scale: (0.45 * f, 0.45 * f), ..look });
                // the chest: opened once claimed
                let chest = if self.gauntlets_claimed.contains(kind) { "chest_02_04_001.png" } else { "chest_02_02_001.png" };
                c.sprite(chest, x + dx, y - 78.5 + dy, Look { scale: (0.4 * f, 0.4 * f), ..look });
            }
            ctx.area(id, x, y, 110.0, 234.0);
        }
        let pages = self.gauntlets.len().div_ceil(3);
        let start = cx - (pages as f32 - 1.0) * 8.0;
        for p in 0..pages {
            let look = if p == page { Look::default() } else { Look::default().rgb([125, 125, 125]) };
            c.sprite("uiDot_001.png", start + p as f32 * 16.0, 15.0, look);
        }
        if pages > 1 {
            ctx.button(c, Btn::GauntletPage(false), "navArrowBtn_001.png", 30.0, cy, Look::default().flip_x(true), 1.2);
            ctx.button(c, Btn::GauntletPage(true), "navArrowBtn_001.png", w - 30.0, cy, Look::default(), 1.2);
        }
        for (ax, ay, fx, fy) in [(0.0f32, 0.0f32, false, false), (1.0, 0.0, true, false), (0.0, 1.0, false, true), (1.0, 1.0, true, true)] {
            c.sprite("gauntletCorner_001.png", -1.0 + ax * (w + 2.0), -1.0 + ay * (h + 2.0), Look::default().anchor(ax, ay).flip_x(fx).flip_y(fy).rgb([76, 76, 76]));
        }
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 25.0, h - 22.0, Look::default(), 1.6);
        ctx.button(c, Btn::OnlineHelp, "GJ_infoIcon_001.png", 30.0, 30.0, Look::default(), 1.5);
        ctx.button(c, Btn::OnlineRefresh, "GJ_updateBtn_001.png", w - 25.0, 25.0, Look::scaled(0.75), 1.2);
    }

    /// `LevelBrowserLayer::init` (0x3ea564) with its `GJListLayer` of `LevelCell`s.
    fn draw_list(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        background(c);
        let (lx, ly) = ((w - 356.0) * 0.5, (h - 220.0) * 0.5 - 5.0);
        c.rect(lx, ly, 356.0, 220.0, [191.0 / 255.0, 114.0 / 255.0, 62.0 / 255.0], 1.0);
        let top = ly + 220.0;
        // the rows are cut to the list (its stencil): a row half in view shows half
        c.clip = Some([lx, ly, lx + 356.0, top]);
        for (i, l) in self.results.iter().enumerate() {
            let y = top - 90.0 * (i as f32 + 1.0) + self.scroll;
            if y + 90.0 <= ly || y >= top {
                continue;
            }
            draw_cell(c, ctx, l, i, lx, y, ly, top);
        }
        if self.search.kind == -1 && self.title == "My Levels" {
            for (i, (_, name)) in self.my_levels.iter().enumerate() {
                let y = top - 90.0 * (i as f32 + 1.0) + self.scroll;
                if y + 90.0 <= ly || y >= top {
                    continue;
                }
                let (y0, y1) = (y.max(ly), (y + 90.0).min(top));
                let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
                c.rect(lx, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
                if y + 45.0 > ly && y + 45.0 < top {
                    c.text("bigFont", name, lx + 20.0, y + 55.0, Look::scaled(0.7).anchor(0.0, 0.5));
                    let verified = self.my_levels.get(i).is_some_and(|(slot, _)| self.my_verified.contains(slot));
                    c.text("goldFont", if verified { "Verified" } else { "Unverified" }, lx + 22.0, y + 28.0, Look::scaled(0.5).anchor(0.0, 0.5));
                    button_sprite(c, ctx, Btn::MyLevel(i as u8), "View", "bigFont", "GJ_button_01.png", lx + 314.0, y + 45.0, 66.0, 30.0, 0.6, false);
                }
            }
        }
        if self.search.kind == -25 {
            for (i, l) in self.lists.iter().enumerate() {
                let y = top - 90.0 * (i as f32 + 1.0) + self.scroll;
                if y + 90.0 <= ly || y >= top {
                    continue;
                }
                draw_list_cell(c, ctx, l, i, lx, y, ly, top);
            }
        }
        if self.search.kind == -10 {
            for (i, p) in self.packs.iter().enumerate() {
                let y = top - 100.0 * (i as f32 + 1.0) + self.scroll;
                if y + 100.0 <= ly || y >= top {
                    continue;
                }
                draw_pack(c, ctx, p, i, lx, y, ly, top);
            }
        }
        c.clip = None;
        let mine = self.search.kind == -1 && self.title == "My Levels";
        if !self.status.is_empty() && !(mine && !self.my_levels.is_empty()) {
            if self.busy() {
                loading_circle(c, cx, cy - 5.0, self.t);
            } else if self.search.kind < 0 {
                // the local lists' empty text (chatFont? no: bigFont 0.5 with tags)
                let lines = super::canvas::rich_lines(c, "bigFont", &self.status, 600.0);
                super::canvas::draw_rich(c, "bigFont", &lines, cx, cy + 10.0, 0.5, 40.0, 255);
            } else {
                c.text("goldFont", &self.status, cx, cy - 5.0, Look::scaled(0.6));
            }
        }
        // the frame covers the list's edges
        let label = match self.search.kind {
            6 => Some("featuredLabel_001.png"),
            -25 => Some("topListsLabel_001.png"),
            _ => None,
        };
        super::options::list_border(c, lx, ly, 356.0, 220.0, if label.is_some() { "" } else { &self.title });
        if let Some(label) = label {
            c.sprite(label, cx, h - 26.0, Look::default());
        }
        side_art(c);
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 24.0, h - 23.0, Look::default(), 1.6);
        if mine {
            // LevelBrowserLayer for local levels: New, the folders and tools instead of refresh
            ctx.button(c, Btn::MyNew, "GJ_newBtn_001.png", w - 30.7, 30.8, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(0), "GJ_myServerBtn_001.png", 30.7, 30.8, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(1), "GJ_importBtn_001.png", 25.7, 76.8, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(2), "GJ_manageBackupBtn_001.png", 25.7, 112.8, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(3), "GJ_viewListsBtn_001.png", w - 28.7, 82.8, Look::scaled(0.8), 1.2);
            ctx.button(c, Btn::MyTool(4), "gj_folderBtn_001.png", w - 23.0, 247.0, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(5), "gj_findBtn_001.png", 24.0, 252.0, Look::default(), 1.2);
            ctx.button(c, Btn::MyTool(6), "threadBtn_selectAll_001.png", cx - 150.0, 38.5, Look::scaled(0.715), 1.2);
        } else if self.search.kind != -1 {
            // (the map packs page has no refresh button)
            if self.search.kind != -10 {
                ctx.button(c, Btn::OnlineRefresh, "GJ_updateBtn_001.png", w - 27.0, 27.0, Look::default(), 1.2);
            }
            ctx.button(c, Btn::OnlineHelp, "GJ_infoIcon_001.png", 30.0, 30.0, Look::default(), 1.5);
        }
        if self.search.page > 0 {
            ctx.button(c, Btn::LevelsPrev, "GJ_arrow_03_001.png", 24.0, cy, Look::default(), 1.3);
        }
        if (self.search.page + 1) * 10 < self.total {
            ctx.button(c, Btn::LevelsNext, "GJ_arrow_03_001.png", w - 24.0, cy, Look::default().flip_x(true), 1.3);
        }
        if self.total > 0 {
            let from = self.search.page * 10 + 1;
            // the rows shown: levels, or the map packs / lists the page holds
            let shown = [self.results.len(), self.packs.len(), self.lists.len()].into_iter().max().unwrap_or(0);
            let to = (from + shown as u32).saturating_sub(1).min(self.total);
            c.text("goldFont", &format!("{from} to {to} of {}", self.total), w - 7.0, h - 3.0, Look::scaled(0.441).anchor(1.0, 1.0));
            // the page button: the page number on a small box
            c.nine_slice("GJ_button_02.png", w - 23.0, h - 38.0, 28.0, 28.0, Look::default());
            c.text("bigFont", &(self.search.page + 1).to_string(), w - 23.0, h - 37.0, Look::scaled(0.5));
        }
    }

    /// `LevelInfoLayer::init`.
    fn draw_info(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let Some(l) = &self.info else { return };
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        background(c);
        let (nw, _) = c.text_size("bigFont", &l.name);
        c.text("bigFont", &l.name, cx, h - 17.0, Look::scaled((300.0 / nw.max(1.0)).min(0.8)));
        let by = format!("By {}", l.author);
        let (bw, _) = c.text_size("goldFont", &by);
        let bs = (300.0 / bw.max(1.0)).min(0.8) * ctx.factor(Btn::InfoAuthor);
        // unregistered creators are green (LevelInfoLayer: m_creatorLabel)
        let look = if l.account_id > 0 { Look::scaled(bs) } else { Look::scaled(bs).rgb([90, 255, 15]) };
        c.text("goldFont", &by, cx, h - 43.0, look);
        ctx.area(Btn::InfoAuthor, cx, h - 43.0, bw * bs, 22.0);
        // the difficulty and its stars
        draw_difficulty(c, l, cx - 100.0, cy + 66.0, 1.0);
        if l.stars > 0 {
            c.text("bigFont", &l.stars.to_string(), cx - 100.0, cy + 36.0, Look::scaled(0.4).anchor(1.0, 0.5));
            c.sprite("star_small01_001.png", cx - 92.0, cy + 36.0, Look::default());
        }
        // stats on the right
        for (icon, text, y, s) in [
            ("GJ_downloadsIcon_001.png", commas(l.downloads), 249.0, 0.433),
            ("GJ_likesIcon_001.png", commas(l.likes), 222.0, 0.5),
            ("GJ_timeIcon_001.png", l.length_text().to_string(), 193.0, 0.5),
            ("currencyOrbIcon_001.png", format!("0/{}", l.orbs()), 165.0, 0.5),
        ] {
            let y = cy - 160.0 + y;
            c.sprite(icon, cx + 80.0, y, Look::default().anchor(0.0, 0.5));
            c.text("bigFont", &text, cx + 113.0, y, Look::scaled(s).anchor(0.0, 0.5));
        }
        // play
        let f = ctx.factor(Btn::InfoPlay);
        c.sprite("GJ_playBtn2_001.png", cx, cy + 51.0, Look::scaled(f));
        ctx.area(Btn::InfoPlay, cx, cy + 51.0, 78.5, 81.5);
        if self.busy() {
            loading_circle(c, cx, cy + 51.0, self.t);
        }
        // progress bars
        for (label, y, col) in [("Normal Mode", -24.0, [0u8, 255, 0]), ("Practice Mode", -56.0, [0, 255, 255])] {
            c.sprite("GJ_progressBar_001.png", cx, cy + y, Look::scaled(0.7).rgb([0, 0, 0]).alpha(125));
            let _ = col;
            c.text("bigFont", "0%", cx, cy + y, Look::scaled(0.4));
            c.text("bigFont", label, cx, cy + y + 15.0, Look::scaled(0.4));
        }
        ctx.button(c, Btn::InfoSide(5), "sideBtn_death_001.png", cx + 141.0, cy - 24.0, Look::scaled(0.9), 1.3);
        // the side buttons
        for (k, (icon, y)) in [("GJ_deleteBtn_001.png", 130.0), ("GJ_updateBtn_001.png", 80.0), ("GJ_infoBtn_001.png", 30.0), ("GJ_levelLeaderboardBtn_001.png", -20.0), ("GJ_like2Btn_001.png", -70.0)]
            .into_iter()
            .enumerate()
        {
            ctx.button(c, Btn::InfoSide(k as u8), icon, cx + 254.5, cy + y, Look::default(), 1.2);
        }
        // an unrated level can be rated (RateStarsLayer), a rated demon's difficulty voted
        // (RateDemonLayer), under the like button
        if l.stars == 0 {
            ctx.button(c, Btn::InfoSide(13), "GJ_rateDiffBtn_001.png", cx + 254.5, cy - 120.0, Look::default(), 1.2);
        } else if l.demon {
            ctx.button(c, Btn::InfoSide(14), "GJ_rateDiffBtn2_001.png", cx + 254.5, cy - 120.0, Look::default(), 1.2);
        }
        if self.rating.is_some() {
            self.draw_rating(c, ctx);
        }
        // the song
        let (sx, sy) = (cx, 45.0);
        // CustomSongWidget: GJ_square01 for a custom song (captured), the dark box otherwise
        let box_frame = if l.custom_song > 0 { "GJ_square01.png" } else { "square01_001.png" };
        c.nine_slice(box_frame, sx, sy, 300.0, 90.0, Look::default());
        let (name, artist) = song_text(l);
        let (tw, _) = c.text_size("bigFont", &name);
        c.text("bigFont", &name, sx - 140.0, sy + 29.0, Look::scaled((230.0 / tw.max(1.0)).min(0.8)).anchor(0.0, 0.5));
        let by = format!("By: {artist}");
        let (aw, _) = c.text_size("goldFont", &by);
        c.text("goldFont", &by, sx - 139.0, sy + 5.0, Look::scaled((170.0 / aw.max(1.0)).min(0.8)).anchor(0.0, 0.5));
        if l.custom_song > 0 {
            let size = if l.song_size > 0.0 { format!("  Size: {:.1} MB", l.song_size) } else { String::new() };
            c.text("bigFont", &format!("Songs: {}  SFX: {}{size}", l.songs.max(1), l.sfx), sx - 139.0, sy - 31.0, Look::scaled(0.4).anchor(0.0, 0.5));
            button_sprite(c, ctx, Btn::InfoSongMore, "More", "bigFont", "GJ_button_01.png", sx + 70.8, sy + 3.0, 68.8 * 0.7, 25.0 * 0.7, 0.42, false);
            ctx.button(c, Btn::InfoSongMore, "GJ_infoIcon_001.png", sx + 136.0, sy + 30.0, Look::scaled(0.65), 1.5);
            ctx.button(c, Btn::InfoSongAssets, "statInfoIcon_001.png", sx + 115.0, sy + 30.0, Look::scaled(0.65), 1.5);
            ctx.button(c, Btn::InfoPlay, "GJ_downloadBtn_001.png", sx + 127.0, sy - 20.0, Look::default(), 1.2);
        }
        // the left and lower buttons
        c.sprite("GJ_viewProfileTxt_001.png", cx + 121.2, h - 44.0, Look::default());
        ctx.button(c, Btn::InfoSide(6), "GJ_duplicateBtn_001.png", 30.0, 180.5, Look::default(), 1.2);
        ctx.button(c, Btn::InfoSide(7), "GJ_optionsBtn_001.png", 30.0, 70.0, Look::scaled(0.6), 1.2);
        ctx.button(c, Btn::InfoSide(8), "GJ_infoIcon_001.png", 30.0, 30.0, Look::default(), 1.5);
        ctx.button(c, Btn::InfoSide(9), "gj_heartOff_001.png", 68.0, 30.0, Look::default(), 1.2);
        ctx.button(c, Btn::InfoSide(10), "gj_folderBtn_001.png", cx + 211.5, 80.0, Look::scaled(0.8), 1.2);
        c.text("goldFont", "0", cx + 211.5, 80.8, Look::scaled(0.44));
        ctx.button(c, Btn::InfoSide(11), "GJ_listAddBtn_001.png", cx + 211.5, 115.0, Look::scaled(0.9), 1.2);
        let mut up = Look::scaled(0.9);
        up.rotation = 90.0;
        let mut down = Look::scaled(0.9);
        down.rotation = -90.0;
        c.sprite("edit_leftBtn_001.png", cx + 211.5, 55.0, up);
        c.sprite("edit_leftBtn_001.png", cx + 211.5, 25.0, down);
        ctx.button(c, Btn::InfoSide(12), "garageRope_001.png", w - 85.0, h - 25.0 + 0.0, Look::default(), 1.0);
        side_art(c);
        ctx.button(c, Btn::LevelsBack, "GJ_arrow_01_001.png", 24.0, h - 23.0, Look::default(), 1.6);
        if !self.status.is_empty() && !self.busy() {
            c.text("goldFont", &self.status, cx, cy + 5.0, Look::scaled(0.5));
        }
        if let Some(scene) = self.popup {
            ctx.buttons.clear();
            let start = c.mark();
            self.draw_popup(c, ctx, scene, l);
            super::open_anim(c, start, self.t - self.popup_since.1);
        }
    }

    /// Post a comment on a level (`uploadGJComment21.php`), then reload its comments.
    pub fn post_comment(&mut self, level: i64, text: String, percent: i32) {
        self.comments = None;
        self.daily_job = Some(crate::net::spawn(async move {
            let id = crate::gjapi::identity();
            if let Some(form) = crate::gjapi::comment_upload_form(&id, level, &text, percent) {
                let _ = crate::net::post("uploadGJComment21.php", &form).await;
            }
            match crate::online::comments(level as i32, 0, false).await {
                Ok((c, _)) => Msg::Comments(c),
                Err(_) => Msg::Comments(Vec::new()),
            }
        }));
    }

    /// Another player's `ProfilePage` over whatever is open (your friends and requests lists).
    pub fn open_profile(&mut self, account: i64) {
        self.profile_account = account;
        self.popup = Some(super::replay::Scene::Profile);
        self.profile = None;
        self.t = 0.0;
        self.daily_job = Some(crate::net::spawn(async move {
            match crate::online::profile(account).await {
                Ok(p) => Msg::Profile(Box::new(p)),
                Err(e) => Msg::Error(e),
            }
        }));
    }

    /// That profile, drawn outside the online screens.
    pub fn draw_profile_over(&self, c: &mut Canvas, ctx: &mut Ctx) {
        if self.popup == Some(super::replay::Scene::Profile) {
            ctx.buttons.clear();
            self.draw_popup(c, ctx, super::replay::Scene::Profile, &LevelInfo::default());
        }
    }

    /// The level page's popups, drawn from their real layouts with this level's text.
    fn draw_popup(&self, c: &mut Canvas, ctx: &mut Ctx, scene: super::replay::Scene, l: &LevelInfo) {
        use super::replay::Scene;
        let (w, h) = (c.width, c.height);
        let (cx, _) = (w * 0.5, h * 0.5);
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        // the capture's own level / song text is replaced by this level's
        const CAPTURED: [&str; 12] = ["GOLIATH", "By Subwoofer", "Version: 1", "ID: 151099654", "Final Macademia", "Autodidactic", "SongID: 1636518", "More By Autodidactic", "ID: 1636518  Size: 1.4MB", "GOLIATH Leaderboard", "Behold", "original soundtrack"];
        let comments_shown = scene == Scene::Info && self.comments.is_some();
        let profile = scene == Scene::Profile;
        super::replay::draw(c, ctx, scene, &|t: &str| {
            if let Some(frame) = t.strip_prefix("frame:") {
                // the capture's player icons
                return profile && ["player_", "ship_", "bird_", "dart_", "robot_", "spider_", "swing_", "jetpack_"].iter().any(|k| frame.starts_with(k));
            }
            profile && t != "..." || CAPTURED.iter().any(|k| t.starts_with(k)) || (comments_shown && t == "Load Comments")
        });
        let (song, artist) = song_text(l);
        let dx = cx - 284.5;
        match scene {
            Scene::Info => {
                let (tw, _) = c.text_size("bigFont", &l.name);
                c.text("bigFont", &l.name, cx, 285.0, Look::scaled((300.0 / tw.max(1.0)).min(1.0)));
                c.text("goldFont", &format!("By {}", l.author), cx, 259.0, Look::scaled(0.8));
                let desc = if l.description.is_empty() { "(No description provided)".to_string() } else { l.description.clone() };
                let lines = super::canvas::rich_lines(c, "chatFont", &desc, 400.0);
                let n = lines.len().min(3) as f32;
                super::canvas::draw_rich(c, "chatFont", &lines[..lines.len().min(3)], cx, 212.0 + n * 8.1, 0.81, 20.0, 255);
                c.text("goldFont", &format!("Version: {}", 1), cx - 50.0, 29.0, Look::scaled(0.6));
                c.text("goldFont", &format!("ID: {}", l.id), cx + 50.0, 29.0, Look::scaled(0.6));
                match &self.comments {
                    None if self.daily_job.is_some() => loading_circle(c, cx, 107.0, self.t),
                    None => {}
                    Some(list) if list.is_empty() => {
                        c.text("goldFont", "No comments", cx, 107.0, Look::scaled(0.6));
                    }
                    Some(list) => {
                        // CommentCell (small mode): 36 high inside the 354 × 132 area
                        let (ax, top) = (107.5 + dx, 173.0);
                        for (i, cm) in list.iter().take(3).enumerate() {
                            let y = top - 44.0 * (i as f32 + 1.0);
                            let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
                            c.rect(ax, y, 354.0, 44.0, shade.map(|v| v / 255.0), 1.0);
                            c.text("goldFont", &cm.user, ax + 10.0, y + 33.0, Look::scaled(0.5).anchor(0.0, 0.5));
                            if cm.percent > 0 {
                                let (uw, _) = c.text_size("goldFont", &cm.user);
                                c.text("bigFont", &format!("{}%", cm.percent), ax + 16.0 + uw * 0.5, y + 33.0, Look::scaled(0.35).anchor(0.0, 0.5));
                            }
                            let (tw, _) = c.text_size("chatFont", &cm.text);
                            c.text("chatFont", &cm.text, ax + 10.0, y + 15.0, Look::scaled((330.0 / tw.max(1.0)).min(0.7)).anchor(0.0, 0.5));
                            c.text("bigFont", &cm.likes.to_string(), ax + 330.0, y + 33.0, Look::scaled(0.3).anchor(1.0, 0.5));
                            c.sprite("GJ_likesIcon_001.png", ax + 342.0, y + 33.0, Look::scaled(0.5));
                            c.text("chatFont", &cm.age, ax + 346.0, y + 6.0, Look::scaled(0.5).anchor(1.0, 0.5).rgb([0, 0, 0]).alpha(140));
                        }
                    }
                }
            }
            Scene::SongInfo => {
                c.text("bigFont", &song, cx, 259.0 + 0.0, Look::scaled(0.7));
                c.text("goldFont", &artist, cx, 236.0, Look::scaled(0.6));
                c.text("goldFont", &format!("More By {artist}"), cx, 125.0, Look::scaled(0.6));
                c.text("bigFont", &format!("SongID: {}", l.custom_song), cx + 155.0, 38.0, Look::scaled(0.4).anchor(1.0, 0.5));
            }
            Scene::Assets => {
                c.text("bigFont", &song, 166.0 + dx, 196.0, Look::scaled(0.4).anchor(0.0, 0.5));
                c.text("goldFont", &artist, 166.0 + dx + c.text_size("bigFont", &song).0 * 0.4 + 6.0, 196.0, Look::scaled(0.4).anchor(0.0, 0.5));
                c.text("chatFont", &format!("ID: {}  Size: {:.1}MB", l.custom_song, l.song_size), 166.0 + dx, 182.0, Look::scaled(0.5).anchor(0.0, 0.5));
            }
            Scene::Leaderboard => {
                c.text("goldFont", &format!("{} Leaderboard", l.name), cx, 291.0, Look::scaled(0.6));
            }
            Scene::Profile => match &self.profile {
                None => loading_circle(c, cx, 160.0, self.t),
                Some(p) => {
                    if crate::follow::contains("following", self.profile_account) {
                        c.sprite("followBtnOn_001.png", 126.5 + dx, 37.0, Look::scaled(1.1));
                    }
                    let (nw, _) = c.text_size("bigFont", &p.name);
                    c.text("bigFont", &p.name, cx, 285.0, Look::scaled((200.0 / nw.max(1.0)).min(0.76)));
                    if p.rank > 0 {
                        c.text("chatFont", &p.rank.to_string(), 160.5 + dx, 277.0, Look::scaled(0.6));
                    }
                    for (v, x) in [p.stars, p.moons, p.coins, p.user_coins, p.demons, p.diamonds, p.creator_points].into_iter().zip([74.5, 139.59, 204.68, 258.0, 323.09, 382.91, 445.68]) {
                        let t = commas(v);
                        let (tw, _) = c.text_size("bigFont", &t);
                        c.text("bigFont", &t, x + dx, 245.0, Look::scaled((40.0 / tw.max(1.0)).min(0.47)).anchor(0.0, 0.5));
                    }
                    let (c1, c2) = (super::menu::color_for_idx(p.color1), super::menu::color_for_idx(p.color2));
                    for (k, x) in [110.5f32, 154.5, 198.5, 242.5, 286.5, 324.5, 368.5, 414.5, 456.5].into_iter().enumerate() {
                        let base = super::garage::icon_sheet(k, p.icons[k]);
                        super::garage::draw_icon(c, &base, x + dx, 200.0, super::garage::GRID_SCALE[k] * 1.25, c1, c2);
                    }
                    for (i, (text, likes, age)) in p.posts.iter().take(1).enumerate() {
                        let y = 139.0 - 90.0 * i as f32;
                        c.text("goldFont", &p.name, 124.5 + dx, y, Look::scaled(0.7).anchor(0.0, 0.5));
                        let lines = super::canvas::rich_lines(c, "chatFont", text, 290.0);
                        super::canvas::draw_rich_left(c, "chatFont", &lines[..lines.len().min(3)], 124.5 + dx, y - 19.0, 1.0, 20.0);
                        c.text("bigFont", &likes.to_string(), 422.5 + dx, y - 1.5, Look::scaled(0.31));
                        c.text("chatFont", &format!("{age} ago"), 443.5 + dx, y - 58.0, Look::scaled(0.55).anchor(1.0, 0.5).rgb([0, 0, 0]).alpha(140));
                    }
                }
            },
            _ => {}
        }
    }

    /// A button of the level page's popup (its index in the real layout).
    pub fn popup_click(&mut self, i: u8) -> Effect {
        use super::replay::Scene;
        let Some(scene) = self.popup else { return Effect::None };
        let close = match scene {
            Scene::Info | Scene::Profile => 4,
            _ => 0,
        };
        if i == close {
            self.popup = None;
            return Effect::None;
        }
        match (scene, i) {
            (Scene::Info, 5) | (Scene::Info, 7) => {
                if let Some(l) = &self.info {
                    let id = l.id;
                    self.comments = None;
                    self.daily_job = Some(crate::net::spawn(async move {
                        match crate::online::comments(id, 0, false).await {
                            Ok((c, _)) => Msg::Comments(c),
                            Err(_) => Msg::Comments(Vec::new()),
                        }
                    }));
                }
            }
            // InfoLayer's comment and report buttons
            (Scene::Info, 2) => {
                if crate::gjapi::linked_account().is_none_or(|(_, ok)| !ok) {
                    return Effect::Alert("Account".into(), "You need an <cl>account</c> to comment.".into());
                }
                if let Some(l) = &self.info {
                    return Effect::Comment(l.id as i64);
                }
            }
            (Scene::Info, 3) => {
                if let Some(l) = &self.info {
                    return Effect::Report(l.id as i64);
                }
            }
            // LikeItemLayer: like (1) or dislike (2), once per level (likes need no account)
            (Scene::Vote, 1 | 2) => {
                if let Some(l) = &self.info {
                    let (id, like) = (l.id as i64, i == 1);
                    self.popup = None;
                    if !self.liked.insert(id) {
                        return Effect::Alert("Liked".into(), "You have already liked or disliked this level.".into());
                    }
                    let _ = crate::net::spawn(async move {
                        let id_ = crate::gjapi::identity();
                        let _ = crate::net::post("likeGJItem211.php", &crate::gjapi::like_form(&id_, id, like, 1, 0, &crate::gjapi::random_string(10))).await;
                    });
                    if let Some(info) = self.info.as_mut() {
                        info.likes += if like { 1 } else { -1 };
                    }
                }
            }
            // ProfilePage::onFollow: follow (or stop following) this player, on this device
            (Scene::Profile, 6) => {
                if let Some(p) = &self.profile {
                    let on = crate::follow::toggle("following", self.profile_account, &p.name);
                    return Effect::Alert(p.name.clone(), if on { "You are now <cg>following</c> this player." } else { "You are no longer <cr>following</c> this player." }.into());
                }
            }
            (Scene::Info, 0) => {
                if let Some(l) = &self.info {
                    return Effect::Alert("Level ID".into(), format!("This level's ID is <cy>{}</c>.", l.id));
                }
            }
            (Scene::SongInfo, _) | (Scene::Assets, _) => {
                return Effect::Alert("Song".into(), "The song downloads with the level when you play it.".into());
            }
            _ => {}
        }
        Effect::None
    }
}

impl Browser {
    /// `DailyLevelPage` (daily, weekly, event): a popup over the creator menu.
    pub fn draw_daily(&self, c: &mut Canvas, ctx: &mut Ctx) {
        /// The event page's bottom line, one picked at random each time it opens.
        const EVENT_LINES: [&str; 28] = [
            "Events are pretty random", "Next event in random hours from now", "When is the next event?", "NEW EVENT! .... soon",
            "Why did I program this...", "I believe in you", "You can do it", "Next one is a banger", "This is fun",
            "I can't find anything", "Why is this a thing?", "Do or do not. There is no try", "The cake is a lie",
            "Challenge accepted", "I wonder what's next", "Welcome to the event", "Jump cube, jump", "No event for you",
            "This looks interesting", "Something epic is coming...", "Unlocking something... or not", "This could be legendary",
            "I wonder if it's a demon?", "A wild event appears!", "Let me cook", "Please wait, cooking.", "I don't think it's coming",
            "Free event is free",
        ];
        let Some(kind) = self.daily else { return };
        let k = kind as usize % 3;
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        // DailyLevelPage (captured): daily GJ_square01 / GJ_square02, weekly GJ_square05 /
        // GJ_square05 at 175, event GJ_square05 in (190, 47, 242) / GJ_square04
        let (bg, bg_rgb, cell, cell_rgb) = [
            ("GJ_square01.png", [255, 255, 255], "GJ_square02.png", [255, 255, 255]),
            ("GJ_square05.png", [255, 255, 255], "GJ_square05.png", [175, 175, 175]),
            ("GJ_square05.png", [190, 47, 242], "GJ_square04.png", [255, 255, 255]),
        ][k];
        c.nine_slice(bg, cx, cy, 420.0, 280.0, Look::default().rgb(bg_rgb));
        // three corners (the top right one is under the safe button): bottom left as is, top
        // left flipped up, bottom right flipped across; the weekly page uses the gauntlet's
        let (corner, cs) = if k == 1 { ("gauntletCorner_001.png", 0.63) } else { ("dailyLevelCorner_001.png", 1.0) };
        c.sprite(corner, cx - 185.0, cy - 115.0, Look::scaled(cs));
        c.sprite(corner, cx - 185.0, cy + 115.0, Look::default().scale_xy(cs, -cs));
        c.sprite(corner, cx + 185.0, cy - 115.0, Look::default().scale_xy(-cs, cs));
        let label = ["dailyLevelLabel_001.png", "weeklyLevelLabel_001.png", "eventLevelLabel_001.png"][k];
        c.sprite(label, cx, cy + 111.0, Look::default());
        ctx.button(c, Btn::DailyClose, "GJ_closeBtn_001.png", cx - 200.0, cy + 130.0, Look::default(), 1.5);
        ctx.button(c, Btn::DailySafe, "GJ_safeBtn_001.png", cx + 180.0, cy + 110.0, Look::default(), 1.2);
        // the level's box (DailyLevelNode: 380 × 116 at y − 8)
        c.nine_slice(cell, cx, cy - 8.0, 380.0, 116.0, Look::default().rgb(cell_rgb));
        let crown = ["gj_dailyCrown_001.png", "gj_weeklyCrown_001.png", "gj_eventCrown_001.png"][k];
        c.sprite(crown, cx, cy + 62.0, Look::default());
        match &self.daily_level {
            None => loading_circle(c, cx, cy - 8.0, self.t),
            Some(l) => {
                // its LevelCell, origin (cx − 180, cy − 55)
                let (x, y) = (cx - 180.0, cy - 55.0);
                draw_difficulty(c, l, x + 26.0, y + 69.0, 1.0);
                if l.stars > 0 {
                    c.text("bigFont", &l.stars.to_string(), x + 26.0, y + 39.0, Look::scaled(0.4).anchor(1.0, 0.5));
                    c.sprite("star_small01_001.png", x + 34.0, y + 39.0, Look::default());
                    // diamonds: stars + 2
                    if l.stars >= 2 {
                        c.text("bigFont", &format!("0/{}", l.stars + 2), x + 32.6, y + 24.5, Look::scaled(0.35).anchor(1.0, 0.5));
                        c.sprite("diamond_small01_001.png", x + 40.6, y + 24.5, Look::default());
                    }
                }
                // user coins under the difficulty, centered
                let n = l.coins.clamp(0, 3);
                for i in 0..n {
                    let look = if l.coins_verified { Look::default() } else { Look::default().rgb([255, 175, 75]) };
                    c.sprite("usercoin_small01_001.png", x + 26.0 - 5.0 * (n - 1) as f32 + 10.0 * i as f32, y + 9.0, look);
                }
                let (nw, _) = c.text_size("bigFont", &l.name);
                c.text("bigFont", &l.name, x + 50.0, y + 75.0, Look::scaled((200.0 / nw.max(1.0)).min(0.8)).anchor(0.0, 0.5));
                let by = format!("By {}", l.author);
                let (bw, _) = c.text_size("goldFont", &by);
                let bs = (200.0 / bw.max(1.0)).min(0.7);
                c.text("goldFont", &by, x + 52.0, y + 53.0, Look::scaled(bs).anchor(0.0, 0.5));
                if l.objects > 40000 {
                    c.sprite("highObjectIcon_001.png", x + 52.0 + bw * bs + 6.0, y + 52.0, Look::default());
                }
                let (song, _) = song_text(l);
                let col = if l.custom_song > 0 { [255, 130, 255] } else { [100, 230, 255] };
                c.text("bigFont", &song, x + 52.0, y + 33.0, Look::scaled(0.45).anchor(0.0, 0.5).rgb(col));
                // the stats row, numbers shortened (787K)
                let short = short_count;
                let mut sx = x + 59.0;
                for (icon, text, gap) in [
                    ("GJ_timeIcon_001.png", l.length_text().to_string(), 10.0),
                    ("GJ_downloadsIcon_001.png", short(l.downloads), 9.0),
                    ("GJ_likesIcon_001.png", short(l.likes), 10.0),
                    ("currencyOrbIcon_001.png", format!("0/{}", l.orbs()), 10.0),
                ] {
                    if icon.starts_with("currency") && l.orbs() == 0 {
                        break;
                    }
                    c.sprite(icon, sx, y + 14.0, Look::scaled(0.6));
                    let (tw, _) = c.text_size("bigFont", &text);
                    c.text("bigFont", &text, sx + gap, y + 14.0, Look::scaled(0.4).anchor(0.0, 0.5));
                    sx += gap + tw * 0.4 + 18.0;
                }
                let f = ctx.factor(Btn::DailyPlay);
                c.sprite("GJ_playBtn2_001.png", x + 329.75, y + 45.0, Look::scaled(0.7 * f));
                ctx.area(Btn::DailyPlay, x + 329.75, y + 45.0, 55.0, 57.0);
                c.text("goldFont", "New!", cx + 182.0, cy + 45.0, Look::scaled(0.6).anchor(1.0, 1.0));
                if l.daily_index > 0 {
                    c.text("chatFont", &format!("#{}", l.daily_index % 100000), cx + 183.0, cy - 59.0, Look::scaled(0.6).anchor(1.0, 0.0).rgb([0, 0, 0]).alpha(150));
                }
            }
        }
        c.text("goldFont", "Bonus:", cx + if k == 0 { -1.1 } else { 5.0 }, cy - 92.0, Look::scaled(0.6).anchor(1.0, 0.5));
        c.sprite("chest_03_02_001.png", cx + if k == 0 { 24.0 } else { 30.0 }, cy - 92.0, Look::scaled(0.3));
        let left = (self.daily_left as f32 - self.t).max(0.0) as i64;
        let (text, size) = match kind {
            2 => (EVENT_LINES[self.event_line % EVENT_LINES.len()].to_string(), 0.4),
            1 => (format!("New Weekly Demon in: {}", time_left(left)), 0.5),
            _ => (format!("New Daily Level in: {}", time_left(left)), 0.5),
        };
        c.text("goldFont", &text, cx, cy - 125.0, Look::scaled(size));
    }

    /// `ChallengesPage`: three `ChallengeNode`s (300 × 70) on GJ_square01 with corners.
    pub fn draw_quests(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, cy, 420.0, 280.0, Look::default());
        for (sx, sy) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            c.sprite("dailyLevelCorner_001.png", cx + 185.0 * sx, cy + 115.0 * sy, Look::default().flip_x(sx > 0.0).flip_y(sy > 0.0));
        }
        ctx.button(c, Btn::DailyClose, "GJ_closeBtn_001.png", cx - 200.0, cy + 130.0, Look::default(), 1.5);
        ctx.button(c, Btn::DailySafe, "GJ_infoIcon_001.png", cx + 205.0, cy + 135.0, Look::default(), 1.5);
        if self.quests.is_empty() {
            loading_circle(c, cx, cy, self.t);
        }
        for (k, (kind, amount, reward, name)) in self.quests.iter().enumerate() {
            let y = cy + 85.0 - 80.0 * k as f32;
            c.nine_slice("GJ_square02.png", cx, y, 300.0, 70.0, Look::default());
            let (icon, what) = match kind {
                1 => ("currencyOrbIcon_001.png", "mana orbs"),
                2 => ("GJ_coinsIcon2_001.png", "silver coins"),
                _ => ("GJ_starsIcon_001.png", "stars/moons"),
            };
            c.text("bigFont", name, cx - 140.0, y + 29.0, Look::scaled(0.5).anchor(0.0, 1.0));
            let (nw, _) = c.text_size("bigFont", name);
            c.sprite(icon, cx - 140.0 + nw * 0.5 + 11.0, y + 19.6, Look::scaled(0.7));
            c.text("goldFont", "New!", cx - 140.0 + nw * 0.5 + 24.0, y + 20.6, Look::scaled(0.6).anchor(0.0, 0.5));
            c.text("goldFont", &format!("Collect {amount} {what}."), cx - 140.0, y + 9.0, Look::scaled(0.5).anchor(0.0, 1.0));
            c.sprite("GJ_bigDiamond_001.png", cx + 118.0, y, Look::scaled(0.8));
            c.text("bigFont", &format!("{reward}x"), cx + 98.0, y, Look::scaled(0.6).anchor(1.0, 0.5));
            let (got, claimed) = self.quest_state.get(k).copied().unwrap_or((0, false));
            let got = got.clamp(0, *amount as i64);
            if claimed {
                c.text("goldFont", "Claimed", cx - 55.0, y - 18.0, Look::scaled(0.5));
            } else if got >= *amount as i64 {
                // ChallengeNode: done, the Claim button
                let id = Btn::QuestClaim(k as u8);
                let f = ctx.factor(id);
                c.nine_slice("GJ_button_01.png", cx - 55.0, y - 18.0, 80.0 * f, 25.0 * f, Look::default());
                c.text("goldFont", "Claim", cx - 55.0, y - 16.5, Look::scaled(0.6 * f));
                ctx.area(id, cx - 55.0, y - 18.0, 80.0, 25.0);
            } else {
                c.sprite("GJ_progressBar_001.png", cx - 140.0, y - 13.0, Look::default().anchor(0.0, 1.0).scale_xy(0.5, 0.525).rgb([0, 0, 0]).alpha(125));
                let frac = got as f32 / (*amount).max(1) as f32;
                if frac > 0.0 {
                    c.sprite("GJ_progressBar_001.png", cx - 139.0, y - 14.0, Look::default().anchor(0.0, 1.0).scale_xy(0.5 * 0.98 * frac, 0.45).rgb([0, 255, 0]));
                }
                c.text("bigFont", &format!("{got}/{amount}"), cx - 55.0, y - 18.0, Look::scaled(0.28));
            }
        }
        let left = (self.quests_left as f32 - self.t).max(0.0) as i64;
        if !self.quests.is_empty() {
            c.text("goldFont", &format!("New quests in {}h {}min", left / 3600, left % 3600 / 60), cx - 76.8, cy - 125.0, Look::scaled(0.5).anchor(0.0, 0.5));
        }
    }

    /// `GJPathsLayer` (Paths of Power): the ten paths around a circle, locked until their
    /// path's first level is beaten.
    pub fn draw_paths(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.rect(0.0, 0.0, w, h, [0.0; 3], 175.0 / 255.0);
        c.sprite("pathSelectBG_001.png", cx, cy, Look::scaled(1.2));
        ctx.button(c, Btn::DailyClose, "GJ_closeBtn_001.png", cx - 120.0, cy + 120.0, Look::default(), 1.5);
        for i in 0..10 {
            let a = (90.0 - 36.0 * i as f32).to_radians();
            let (x, y) = (cx + 100.0 * a.cos(), cy + 100.0 * a.sin());
            let id = Btn::PathIcon(i as u8);
            let f = ctx.factor(id);
            let frame = format!("pathIcon_{:02}_001.png", i + 1);
            c.sprite(&frame, x, y, Look::scaled(f).rgb([40, 40, 40]).gray());
            c.sprite("GJ_lockGray_001.png", x, y, Look::scaled(f).rgb([200, 200, 200]));
            ctx.area(id, x, y, 46.0, 60.0);
        }
        c.sprite("label_paths_001.png", cx, cy + 29.0, Look::scaled(0.8));
        c.sprite("label_of_001.png", cx, cy, Look::scaled(0.8));
        c.sprite("label_power_001.png", cx, cy - 34.0, Look::scaled(0.8));
    }

    /// `NewsPopup`: one post a page in a dark list (340 × 232) on GJ_square01, "N of M".
    pub fn draw_news(&self, c: &mut Canvas, ctx: &mut Ctx) {
        let Some(page) = self.news_page else { return };
        let (w, h) = (c.width, c.height);
        let (cx, cy) = (w * 0.5, h * 0.5);
        c.rect(0.0, 0.0, w, h, [0.0; 3], 150.0 / 255.0);
        c.nine_slice("GJ_square01.png", cx, cy, 420.0, 280.0, Look::default());
        for (sx, sy) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            c.sprite("dailyLevelCorner_001.png", cx + 185.0 * sx, cy + 115.5 * sy, Look::default().flip_x(sx > 0.0).flip_y(sy > 0.0));
        }
        c.text("goldFont", "News", cx, cy + 123.0, Look::scaled(0.9));
        let (ax, ay, aw, ah) = (cx - 170.0, cy - 128.0, 340.0, 232.0);
        c.rect(ax, ay, aw, ah, [0.0; 3], 50.0 / 255.0);
        ctx.button(c, Btn::NewsClose, "GJ_closeBtn_001.png", cx - 204.0, cy + 134.0, Look::scaled(0.8), 1.5);
        match self.news.get(page) {
            None => {
                if self.daily_job.is_some() {
                    loading_circle(c, cx, cy - 12.0, self.t);
                } else {
                    c.text("goldFont", "Could not load news.", cx, cy - 12.0, Look::scaled(0.6));
                }
            }
            Some(n) => {
                c.text("goldFont", &format!("{} of {}", page + 1, self.news.len()), cx + 170.0, cy + 109.0, Look::scaled(0.5).anchor(1.0, 0.0));
                let mut y = ay + ah - 10.0;
                c.text("goldFont", &n.date, ax + 10.0, y - 6.0, Look::scaled(0.45).anchor(0.0, 0.5));
                y -= 22.0;
                let (tw, _) = c.text_size("bigFont", &n.title);
                c.text("bigFont", &n.title, ax + 10.0, y - 6.0, Look::scaled((320.0 / tw.max(1.0)).min(0.55)).anchor(0.0, 0.5));
                y -= 22.0;
                for b in &n.blocks {
                    if y < ay + 10.0 {
                        break;
                    }
                    match b {
                        crate::online::NewsBlock::Text(t) => {
                            let lines = super::canvas::rich_lines(c, "chatFont", t, 380.0);
                            let shown: Vec<_> = lines.into_iter().take_while(|_| true).collect();
                            let n_lines = shown.len() as f32;
                            super::canvas::draw_rich_left(c, "chatFont", &shown, ax + 10.0, y, 0.8, 18.0);
                            y -= n_lines * 18.0 * 0.8 + 8.0;
                        }
                        crate::online::NewsBlock::Image { file, w: iw, h: ih } => {
                            let pw = 300.0;
                            let ph = (pw * ih / iw.max(1.0)).min((y - ay - 8.0).max(0.0));
                            let frame = format!("news:{file}");
                            let (fw, fh) = c.size(&frame);
                            if fw > 0.0 && fh > 0.0 {
                                c.sprite(&frame, cx, y - ph * 0.5, Look::default().scale_xy(pw / fw, ph / fh));
                            } else {
                                c.rect(cx - pw * 0.5, y - ph, pw, ph, [0.0; 3], 0.6);
                            }
                            y -= ph + 8.0;
                        }
                        crate::online::NewsBlock::Link(l) => {
                            c.text("chatFont", l, ax + 10.0, y - 6.0, Look::scaled(0.8).anchor(0.0, 0.5).rgb([50, 200, 255]));
                            y -= 18.0;
                        }
                    }
                }
                if page + 1 < self.news.len() {
                    ctx.button(c, Btn::NewsPage(true), "GJ_arrow_01_001.png", cx + 238.0, cy, Look::default().flip_x(true), 1.5);
                }
                if page > 0 {
                    ctx.button(c, Btn::NewsPage(false), "GJ_arrow_01_001.png", cx - 238.0, cy, Look::default(), 1.5);
                }
            }
        }
        c.sprite("GJ_commentTop_001.png", cx, cy + 100.0, Look::default());
        c.sprite("GJ_commentTop_001.png", cx, cy - 124.0, Look::default().flip_y(true));
    }

    pub fn click_daily(&mut self, id: Btn) -> Effect {
        match id {
            Btn::NewsClose => self.news_page = None,
            Btn::NewsPage(next) => {
                if let Some(p) = self.news_page {
                    self.news_page = Some(if next { (p + 1).min(self.news.len().saturating_sub(1)) } else { p.saturating_sub(1) });
                }
            }
            Btn::PathIcon(i) => {
                const PATHS: [&str; 10] = ["Fire", "Ice", "Poison", "Shadow", "Lava", "Earth", "Blood", "Metal", "Light", "Souls"];
                return Effect::Alert(
                    format!("Path of {}", PATHS[i as usize % 10]),
                    "This path is <cr>locked</c>. Paths are unlocked and leveled up with the <cy>Path</c> shards you earn from levels, which Cadence doesn't track yet.".into(),
                );
            }
            Btn::DailySafe if self.quests_open => {
                return Effect::Alert("Quests".into(), "Complete <cg>quests</c> to earn <cl>diamonds</c>. New quests arrive over time.".into());
            }
            Btn::DailyClose => {
                self.daily = None;
                self.paths_open = false;
                self.quests_open = false;
                self.daily_job = None;
            }
            // DailyLevelPage::onTheSafe: the past dailies (search type 21), weeklies (22) or
            // events (23)
            Btn::DailySafe => {
                let kind = 21 + self.daily.unwrap_or(0).clamp(0, 2);
                self.daily = None;
                self.daily_job = None;
                self.open_list(Search::new(kind), false);
                self.title = "The Safe".into();
            }
            Btn::DailyPlay if !self.busy() => {
                if let Some(l) = self.daily_level.clone() {
                    self.info = Some(l);
                    self.view = View::Info;
                    self.from_search = false;
                    self.download_info();
                    return Effect::Sound(super::PLAY_SOUND);
                }
            }
            _ => {}
        }
        Effect::None
    }
}

/// "6 Days", "5 Hours", "12 Minutes" (`GameToolbox::getTimeString`).
fn time_left(secs: i64) -> String {
    let (d, h, m) = (secs / 86400, secs / 3600, secs / 60);
    match () {
        _ if d > 0 => format!("{d} Day{}", if d == 1 { "" } else { "s" }),
        _ if h > 0 => format!("{h} Hour{}", if h == 1 { "" } else { "s" }),
        _ if m > 0 => format!("{m} Minute{}", if m == 1 { "" } else { "s" }),
        _ => format!("{secs} Seconds"),
    }
}

/// The level's song title and artist as the cell and the song widget show them.
fn song_text(l: &LevelInfo) -> (String, String) {
    if l.custom_song > 0 {
        (l.song_name.clone(), l.song_artist.clone())
    } else {
        let (t, a) = super::subpages::SONGS.get(l.official_song.clamp(0, 21) as usize).copied().unwrap_or(("Unknown", "-"));
        (t.to_string(), a.to_string())
    }
}

/// `GJ_gradientBG` tinted (0, 102, 255), the online screens' background.
fn background(c: &mut Canvas) {
    let (w, h) = (c.width, c.height);
    let (gw, gh) = c.size("GJ_gradientBG.png");
    if gw > 0.0 {
        c.sprite("GJ_gradientBG.png", -5.0, -5.0, Look::default().anchor(0.0, 0.0).scale_xy((w + 10.0) / gw, (h + 10.0) / gh).rgb([0, 102, 255]));
    }
}

fn side_art(c: &mut Canvas) {
    let w = c.width;
    c.sprite("GJ_sideArt_001.png", -1.0, -1.0, Look::default().anchor(0.0, 0.0));
    c.sprite("GJ_sideArt_001.png", w + 1.0, -1.0, Look::default().anchor(1.0, 0.0).flip_x(true));
}

/// `LoadingCircle`: loadingCircle.png spinning.
pub(crate) fn loading_circle(c: &mut Canvas, x: f32, y: f32, t: f32) {
    let mut look = Look::default().blend(crate::render::Blend::Additive);
    look.rotation = t * 360.0;
    c.sprite("loadingCircle.png", x, y, look);
}

/// 274579 → "274,579" (`GameToolbox::intToString` with separators, the level page's stats).
fn commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// `GJDifficultySprite` with the featured/epic coin behind it.
fn draw_difficulty(c: &mut Canvas, l: &LevelInfo, x: f32, y: f32, s: f32) {
    let coin = match l.epic {
        1 => Some("GJ_epicCoin_001.png"),
        2 => Some("GJ_epicCoin2_001.png"),
        3 => Some("GJ_epicCoin3_001.png"),
        _ if l.featured > 0 => Some("GJ_featuredCoin_001.png"),
        _ => None,
    };
    if let Some(coin) = coin {
        c.sprite(coin, x, y, Look::scaled(s));
    }
    c.sprite(&difficulty_face(l), x, y, Look::scaled(s));
}

/// `LevelCell::loadCustomLevelCell` (0x1e34c4): one 356 × 90 row; `clip` keeps the text inside
/// the list.
#[allow(clippy::too_many_arguments)]
fn draw_cell(c: &mut Canvas, ctx: &mut Ctx, l: &LevelInfo, i: usize, x: f32, y: f32, clip_lo: f32, clip_hi: f32) {
    let (y0, y1) = (y.max(clip_lo), (y + 90.0).min(clip_hi));
    let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
    c.rect(x, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
    // only rows that are mostly visible get their content (the list clips them)
    if y + 90.0 <= clip_lo || y >= clip_hi {
        return;
    }
    draw_difficulty(c, l, x + 26.0, y + 50.0, 1.0);
    if l.stars > 0 {
        c.text("bigFont", &l.stars.to_string(), x + 26.0, y + 20.0, Look::scaled(0.4).anchor(1.0, 0.5));
        c.sprite("star_small01_001.png", x + 34.0, y + 20.0, Look::default());
    }
    let (nw, _) = c.text_size("bigFont", &l.name);
    let ns = (200.0 / nw.max(1.0)).min(0.8);
    c.text("bigFont", &l.name, x + 50.0, y + 75.0, Look::scaled(ns).anchor(0.0, 0.5));
    // user coins under the stars, 10 apart (gray until verified)
    let n = l.coins.clamp(0, 3);
    for k in 0..n {
        let look = if l.coins_verified { Look::default() } else { Look::default().rgb([165, 165, 165]) };
        c.sprite("usercoin_small01_001.png", x + 26.0 - 5.0 * (n - 1) as f32 + 10.0 * k as f32, y + 4.5, look);
    }
    let by = format!("By {}", l.author);
    let (bw, _) = c.text_size("goldFont", &by);
    c.text("goldFont", &by, x + 52.0, y + 53.0, Look::scaled((128.0 / bw.max(1.0)).min(0.7)).anchor(0.0, 0.5));
    let (song, _) = song_text(l);
    let col = if l.custom_song > 0 { [255, 130, 255] } else { [100, 230, 255] };
    let (sw, _) = c.text_size("bigFont", &song);
    c.text("bigFont", &song, x + 52.0, y + 33.0, Look::scaled((200.0 / sw.max(1.0)).min(0.45)).anchor(0.0, 0.5).rgb(col));
    // stats: each icon 18 after the previous label
    let mut sx = x + 59.0;
    for (icon, text, gap) in [
        ("GJ_timeIcon_001.png", l.length_text().to_string(), 10.0),
        ("GJ_downloadsIcon_001.png", short_count(l.downloads), 9.0),
        ("GJ_likesIcon_001.png", short_count(l.likes), 10.0),
        ("currencyOrbIcon_001.png", format!("0/{}", l.orbs()), 10.0),
    ] {
        if icon.starts_with("currency") && l.orbs() == 0 {
            break;
        }
        c.sprite(icon, sx, y + 14.0, Look::scaled(0.6));
        let (tw, _) = c.text_size("bigFont", &text);
        c.text("bigFont", &text, sx + gap, y + 14.0, Look::scaled(0.4).anchor(0.0, 0.5));
        sx += gap + tw * 0.4 + 18.0;
    }
    button_sprite(c, ctx, Btn::OnlineView(i as u8), "Get It", "bigFont", "GJ_button_02.png", x + 314.0, y + 45.0, 66.0, 30.0, 0.453, false);
}

/// `LevelListCell` (356 × 90): like a level's row, with the list's downloads, likes and its
/// diamond reward.
#[allow(clippy::too_many_arguments)]
fn draw_list_cell(c: &mut Canvas, ctx: &mut Ctx, l: &crate::online::LevelList, i: usize, x: f32, y: f32, clip_lo: f32, clip_hi: f32) {
    let (y0, y1) = (y.max(clip_lo), (y + 90.0).min(clip_hi));
    let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
    c.rect(x, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
    if y + 90.0 <= clip_lo || y >= clip_hi {
        return;
    }
    // LevelListCell (captured): the list's face with its diamond reward under it, the name
    // (≤ 200 wide) and an info icon after it, "By X", the progress bar toward the reward, then
    // downloads, likes and levels completed, numbers shortened
    let face = match l.difficulty {
        -1 | 0 => "difficulty_00_btn_001.png".to_string(),
        d @ 1..=10 => format!("difficulty_{d:02}_btn_001.png"),
        _ => "difficulty_auto_btn_001.png".to_string(),
    };
    c.sprite(&face, x + 26.0, y + 50.0, Look::default());
    if l.diamonds > 0 {
        c.text("bigFont", &l.diamonds.to_string(), x + 26.0, y + 20.0, Look::scaled(0.4).anchor(1.0, 0.5));
        c.sprite("diamond_small01_001.png", x + 34.0, y + 20.0, Look::default());
    }
    let (nw, _) = c.text_size("bigFont", &l.name);
    let ns = (200.0 / nw.max(1.0)).min(0.6);
    c.text("bigFont", &l.name, x + 50.0, y + 75.0, Look::scaled(ns).anchor(0.0, 0.5));
    c.sprite("GJ_infoIcon_001.png", x + 50.0 + nw * ns + 5.0, y + 74.0, Look::scaled(0.65));
    c.text("goldFont", &format!("By {}", l.author), x + 52.0, y + 54.0, Look::scaled(0.7).anchor(0.0, 0.5));
    let count = l.levels.split(',').filter(|v| !v.is_empty()).count();
    let needed = if l.needed > 0 { l.needed as usize } else { count };
    c.sprite("GJ_progressBar_001.png", x + 131.6, y + 33.0, Look::default().scale_xy(0.48, 0.64).rgb([0, 0, 0]).alpha(125));
    c.text("bigFont", &format!("0/{needed}"), x + 131.6, y + 33.5, Look::scaled(0.4));
    c.sprite("diamond_small01_001.png", x + 204.6, y + 33.0, Look::default());
    let short = short_count;
    for (icon, text, ix) in [
        ("GJ_downloadsIcon_001.png", short(l.downloads), 59.0),
        ("GJ_likesIcon_001.png", short(l.likes), 123.6),
        ("GJ_completesIcon_001.png", format!("0/{count}"), 184.0),
    ] {
        c.sprite(icon, x + ix, y + 14.0, Look::scaled(0.6));
        c.text("bigFont", &text, x + ix + 9.0, y + 14.0, Look::scaled(0.4).anchor(0.0, 0.5));
    }
    button_sprite(c, ctx, Btn::OnlineList(i as u8), "View", "bigFont", "GJ_button_01.png", x + 315.45, y + 45.0, 63.1, 30.0, 0.6, false);
}

/// A count as the level cells show it: 9.4K under 10,000, 17K, 276K, 1.4M (truncated).
fn short_count(n: i64) -> String {
    let trim = |v: f64, unit: &str| {
        let t = (v * 10.0).floor() / 10.0;
        if t.fract() == 0.0 { format!("{}{unit}", t as i64) } else { format!("{t:.1}{unit}") }
    };
    if n >= 1_000_000 {
        trim(n as f64 / 1e6, "M")
    } else if n >= 10_000 {
        format!("{}K", n / 1000)
    } else if n >= 1000 {
        trim(n as f64 / 1000.0, "K")
    } else {
        n.to_string()
    }
}

/// `MapPackCell` (356 × 100): name in its color, View, stars and coins, the progress bar.
#[allow(clippy::too_many_arguments)]
fn draw_pack(c: &mut Canvas, ctx: &mut Ctx, p: &crate::online::MapPack, i: usize, x: f32, y: f32, clip_lo: f32, clip_hi: f32) {
    let (y0, y1) = (y.max(clip_lo), (y + 100.0).min(clip_hi));
    let shade = if i % 2 == 0 { [161.0, 88.0, 44.0] } else { [194.0, 114.0, 62.0] };
    c.rect(x, y0, 356.0, y1 - y0, shade.map(|v| v / 255.0), 1.0);
    if y + 100.0 <= clip_lo || y >= clip_hi {
        return;
    }
    let face = match p.difficulty {
        0 => "difficulty_auto_btn_001.png".to_string(),
        d @ 1..=5 => format!("difficulty_{d:02}_btn_001.png"),
        _ => "difficulty_06_btn_001.png".to_string(),
    };
    c.sprite(&face, x + 31.0, y + 50.0, Look::scaled(1.1));
    let (nw, _) = c.text_size("bigFont", &p.name);
    c.text("bigFont", &p.name, x + 162.0, y + 80.0, Look::scaled((220.0 / nw.max(1.0)).min(0.9)).rgb(p.text_color));
    c.sprite("GJ_progressBar_001.png", x + 164.0, y + 48.0, Look::default().scale_xy(0.6, 0.8).rgb([0, 0, 0]).alpha(125));
    c.text("bigFont", &format!("0/{}", p.levels.split(',').count()), x + 164.0, y + 48.0, Look::scaled(0.5));
    c.text("bigFont", &p.stars.to_string(), x + 117.0, y + 20.0, Look::scaled(0.7).anchor(1.0, 0.5));
    c.sprite("GJ_starsIcon_001.png", x + 134.0, y + 20.0, Look::default());
    c.text("bigFont", &p.coins.to_string(), x + 195.0, y + 20.0, Look::scaled(0.7).anchor(1.0, 0.5));
    c.sprite("GJ_coinsIcon_001.png", x + 214.0, y + 20.0, Look::default());
    button_sprite(c, ctx, Btn::OnlinePack(i as u8), "View", "bigFont", "GJ_button_01.png", x + 315.4, y + 50.0, 63.1, 50.0, 0.6, false);
}

/// `GJDifficultySprite`: 0 N/A … 5 insane, demons 6 (hard) … 10 (extreme).
pub fn difficulty_face(l: &LevelInfo) -> String {
    if l.auto {
        return "difficulty_auto_btn_001.png".into();
    }
    let n = if l.demon {
        match l.demon_difficulty {
            3 => 7,
            4 => 8,
            5 => 9,
            6 => 10,
            _ => 6,
        }
    } else {
        l.difficulty.clamp(0, 5)
    };
    format!("difficulty_{n:02}_btn_001.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands() {
        assert_eq!(commas(274579), "274,579");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1000000), "1,000,000");
    }
}

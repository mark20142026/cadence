//! Saved online levels (the creator menu's Saved, like the game's own `GLM_03`): every level
//! downloaded to play is kept with what its page shows, so it plays again without the servers,
//! can be deleted, refreshed, favorited, put in a folder, or (when its creator allows copying)
//! cloned into your own levels. On the desktop the level data are files in Cadence's own
//! folder and the details one line each in an index; on the web both are in browser storage.

use crate::online::LevelInfo;

/// One saved level's details, as a tab-separated line (tabs and newlines in text escaped).
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A saved level: its page's details, whether it's a favorite, its folder (0 none), and
/// whether its creator lets it be copied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub info: LevelInfo,
    pub favorite: bool,
    pub folder: u8,
    pub copyable: bool,
}

pub fn to_line(s: &Saved) -> String {
    let l = &s.info;
    let fields: Vec<String> = vec![
        l.id.to_string(),
        esc(&l.name),
        esc(&l.author),
        esc(&l.description),
        l.difficulty.to_string(),
        (l.demon as i32).to_string(),
        l.demon_difficulty.to_string(),
        (l.auto as i32).to_string(),
        l.stars.to_string(),
        l.downloads.to_string(),
        l.likes.to_string(),
        l.length.to_string(),
        l.official_song.to_string(),
        l.custom_song.to_string(),
        esc(&l.song_name),
        esc(l.song_url.as_deref().unwrap_or("")),
        esc(&l.song_artist),
        l.song_size.to_string(),
        l.coins.to_string(),
        (l.coins_verified as i32).to_string(),
        l.featured.to_string(),
        l.epic.to_string(),
        l.objects.to_string(),
        (l.two_player as i32).to_string(),
        l.original.to_string(),
        l.player_id.to_string(),
        l.account_id.to_string(),
        (s.favorite as i32).to_string(),
        s.folder.to_string(),
        (s.copyable as i32).to_string(),
    ];
    fields.join("\t")
}

pub fn from_line(line: &str) -> Option<Saved> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 30 {
        return None;
    }
    let i = |k: usize| f[k].parse::<i64>().unwrap_or(0);
    let info = LevelInfo {
        id: i(0) as i32,
        name: unesc(f[1]),
        author: unesc(f[2]),
        description: unesc(f[3]),
        difficulty: i(4) as i32,
        demon: i(5) != 0,
        demon_difficulty: i(6) as i32,
        auto: i(7) != 0,
        stars: i(8) as i32,
        downloads: i(9),
        likes: i(10),
        length: i(11) as i32,
        official_song: i(12) as i32,
        custom_song: i(13) as i32,
        song_name: unesc(f[14]),
        song_url: Some(unesc(f[15])).filter(|u| !u.is_empty()),
        song_artist: unesc(f[16]),
        song_size: f[17].parse().unwrap_or(0.0),
        coins: i(18) as i32,
        coins_verified: i(19) != 0,
        featured: i(20),
        epic: i(21) as i32,
        objects: i(22),
        two_player: i(23) != 0,
        original: i(24) as i32,
        player_id: i(25),
        account_id: i(26),
        ..LevelInfo::default()
    };
    Some(Saved { info, favorite: i(27) != 0, folder: i(28) as u8, copyable: i(29) != 0 })
}

/// Whether a level's copy password (download key 27: XOR "26364", base64) lets anyone copy
/// it: the game's "free copy" is 1.
pub fn copyable(key27: &str) -> bool {
    let raw = crate::gjapi::b64_decode(key27).map(|b| crate::gjapi::xor_cycle(&b, "26364")).unwrap_or_default();
    String::from_utf8_lossy(&raw).trim() == "1"
}

#[cfg(not(target_arch = "wasm32"))]
mod store {
    fn dir() -> std::path::PathBuf {
        crate::online::cache_dir().with_file_name("saved")
    }

    pub fn read_index() -> String {
        std::fs::read_to_string(dir().join("index.txt")).unwrap_or_default()
    }

    pub fn write_index(text: &str) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(dir().join("index.txt"), text);
    }

    pub fn read_data(id: i32) -> Option<Vec<u8>> {
        std::fs::read(dir().join(format!("{id}.txt"))).ok()
    }

    pub fn write_data(id: i32, data: &[u8]) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(dir().join(format!("{id}.txt")), data);
    }

    pub fn remove_data(id: i32) {
        let _ = std::fs::remove_file(dir().join(format!("{id}.txt")));
    }
}

#[cfg(target_arch = "wasm32")]
mod store {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn read_index() -> String {
        storage().and_then(|s| s.get_item("cadence.saved").ok().flatten()).unwrap_or_default()
    }

    pub fn write_index(text: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item("cadence.saved", text);
        }
    }

    pub fn read_data(id: i32) -> Option<Vec<u8>> {
        storage()?.get_item(&format!("cadence.saved.{id}")).ok()?.map(String::into_bytes)
    }

    pub fn write_data(id: i32, data: &[u8]) {
        // a level too big for the browser's storage simply isn't kept
        if let Some(s) = storage() {
            let _ = s.set_item(&format!("cadence.saved.{id}"), &String::from_utf8_lossy(data));
        }
    }

    pub fn remove_data(id: i32) {
        if let Some(s) = storage() {
            let _ = s.remove_item(&format!("cadence.saved.{id}"));
        }
    }
}

/// The saved levels, newest first.
pub fn list() -> Vec<Saved> {
    store::read_index().lines().filter_map(from_line).collect()
}

fn write_list(list: &[Saved]) {
    store::write_index(&list.iter().map(to_line).collect::<Vec<_>>().join("\n"));
}

/// Keep a downloaded level (replacing an older copy, keeping its favorite and folder).
pub fn save(info: &LevelInfo, data: &[u8], copyable: bool) {
    if info.id <= 0 {
        return;
    }
    let mut all = list();
    let old = all.iter().position(|s| s.info.id == info.id).map(|i| all.remove(i));
    let (favorite, folder) = old.map_or((false, 0), |o| (o.favorite, o.folder));
    all.insert(0, Saved { info: info.clone(), favorite, folder, copyable });
    store::write_data(info.id, data);
    write_list(&all);
}

pub fn data(id: i32) -> Option<Vec<u8>> {
    store::read_data(id)
}

pub fn get(id: i32) -> Option<Saved> {
    list().into_iter().find(|s| s.info.id == id)
}

pub fn delete(id: i32) {
    let mut all = list();
    all.retain(|s| s.info.id != id);
    store::remove_data(id);
    write_list(&all);
}

/// Change a saved level's favorite / folder.
pub fn update(id: i32, f: impl FnOnce(&mut Saved)) {
    let mut all = list();
    if let Some(s) = all.iter_mut().find(|s| s.info.id == id) {
        f(s);
        write_list(&all);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        let info = LevelInfo { id: 10565740, name: "Bloodbath".into(), author: "Riot".into(), description: "Whose blood\twill be\nspilt?".into(), stars: 10, demon: true, song_url: Some("https://audio.ngfiles.com/x.mp3".into()), ..Default::default() };
        let s = Saved { info, favorite: true, folder: 3, copyable: false };
        assert_eq!(from_line(&to_line(&s)), Some(s));
        // "1" XOR "26364", base64: a free copy
        assert!(copyable(&crate::gjapi::b64(&crate::gjapi::xor_cycle(b"1", "26364"))));
        assert!(!copyable(&crate::gjapi::b64(&crate::gjapi::xor_cycle(b"1012345", "26364"))));
    }
}

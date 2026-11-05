//! My Lists (`LevelListLayer` on your own `GJLevelList`s): lists of levels you make on this
//! device — a name, a description, the levels in order, a difficulty face — and upload
//! (`uploadGJLevelList.php`, its body from the decompile of `GameLevelManager::uploadLevelList`).
//! Kept one list a line, like the saved levels.

use crate::gjapi::{self, Identity};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MyList {
    /// This device's key for the list (when it was made, ms)
    pub key: i64,
    pub name: String,
    pub description: String,
    pub levels: Vec<i32>,
    /// The face picked (0 none / auto … 10 demons, as the game's list difficulty)
    pub difficulty: i32,
    /// The servers' id once uploaded (0 before)
    pub list_id: i32,
    pub version: i32,
    pub unlisted: i32,
}

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

pub fn to_line(l: &MyList) -> String {
    let levels = l.levels.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{levels}", l.key, esc(&l.name), esc(&l.description), l.difficulty, l.list_id, l.version, l.unlisted)
}

pub fn from_line(line: &str) -> Option<MyList> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 8 {
        return None;
    }
    let n = |i: usize| f[i].parse::<i64>().unwrap_or(0);
    Some(MyList {
        key: n(0),
        name: unesc(f[1]),
        description: unesc(f[2]),
        difficulty: n(3) as i32,
        list_id: n(4) as i32,
        version: n(5) as i32,
        unlisted: n(6) as i32,
        levels: f[7].split(',').filter_map(|v| v.parse().ok()).collect(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
mod store {
    fn file() -> std::path::PathBuf {
        // a test run's own folder (CADENCE_SAVE_DIR), else Cadence's
        match std::env::var_os("CADENCE_SAVE_DIR") {
            Some(d) => std::path::PathBuf::from(d).join("mylists.txt"),
            None => crate::online::cache_dir().with_file_name("mylists.txt"),
        }
    }

    pub fn read() -> String {
        std::fs::read_to_string(file()).unwrap_or_default()
    }

    pub fn write(text: &str) {
        let f = file();
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, text);
    }
}

#[cfg(target_arch = "wasm32")]
mod store {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn read() -> String {
        storage().and_then(|s| s.get_item("cadence.mylists").ok().flatten()).unwrap_or_default()
    }

    pub fn write(text: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item("cadence.mylists", text);
        }
    }
}

/// Your lists, newest first.
pub fn list() -> Vec<MyList> {
    store::read().lines().filter_map(from_line).collect()
}

pub fn write_all(all: &[MyList]) {
    store::write(&all.iter().map(to_line).collect::<Vec<_>>().join("\n"));
}

/// `LevelBrowserLayer::onNew` on My Lists: a new empty list, first.
pub fn create(now_ms: i64) -> MyList {
    let mut all = list();
    let l = MyList { key: now_ms, name: String::new(), version: 1, ..Default::default() };
    all.insert(0, l.clone());
    write_all(&all);
    l
}

pub fn get(key: i64) -> Option<MyList> {
    list().into_iter().find(|l| l.key == key)
}

pub fn update(key: i64, f: impl FnOnce(&mut MyList)) {
    let mut all = list();
    if let Some(l) = all.iter_mut().find(|l| l.key == key) {
        f(l);
        write_all(&all);
    }
}

pub fn delete(key: i64) {
    let mut all = list();
    all.retain(|l| l.key != key);
    write_all(&all);
}

/// `uploadGJLevelList.php`: the list's fields, then `seed` (5 random characters) and `seed2`:
/// the levels string (or 50 evenly spaced characters of it) with your account id, SHA-1 hexed,
/// XORed with `seed` and base64'd (`base64EncodeEnc`).
pub fn upload_form(id: &Identity, l: &MyList, seed: &str) -> Option<String> {
    let account = id.account.as_ref()?;
    let levels = l.levels.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    let b = levels.as_bytes();
    let sample: String = if b.len() < 50 { levels.clone() } else { (0..50).map(|i| b[i * (b.len() / 50)] as char).collect() };
    let seed2 = gjapi::b64(&gjapi::xor_cycle(gjapi::sha1_hex(&format!("{sample}{}", account.account_id)).as_bytes(), seed));
    let desc = gjapi::b64(l.description.replace([':', '|', '#'], "").as_bytes());
    let name: String = l.name.chars().filter(|c| !matches!(c, ':' | '|' | '#' | '~')).collect();
    Some(format!(
        "{}&listID={}&listName={}&listDesc={}&listLevels={levels}&difficulty={}&listVersion={}&original=0&unlisted={}&seed={seed}&seed2={seed2}&notify=0&secret={}",
        id.base(),
        l.list_id,
        gjapi::field(&name),
        desc,
        l.difficulty,
        l.version.max(1),
        l.unlisted,
        gjapi::SECRET
    ))
}

/// `deleteGJLevelList.php`.
pub fn delete_form(id: &Identity, list_id: i32) -> String {
    format!("{}&listID={list_id}&secret={}", id.base(), gjapi::SECRET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        let l = MyList { key: 5, name: "Hard\tones".into(), description: "line\nbreak".into(), levels: vec![128, 4284013], difficulty: 5, list_id: 77, version: 2, unlisted: 1 };
        assert_eq!(from_line(&to_line(&l)), Some(l));
    }
}

//! Cadence's own progress save: per official level, the normal-mode best %, practice best %,
//! attempts, jumps and the secret coins collected. Never the game's save: natively it is a text
//! file in Cadence's own app-support folder (`~/Library/Application Support/Cadence/progress.txt`
//! on macOS); on the web it is `localStorage["cadence.progress"]`.
//!
//! Format: one line per level, `<id> best=<n> practice=<n> attempts=<n> jumps=<n> coins=<bits>`
//! (bit i set = coin i + 1 collected). Unknown fields are ignored, so the format can grow.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelProgress {
    pub best: u32,
    pub practice: u32,
    pub attempts: u32,
    pub jumps: u32,
    pub coins: u8,
}

impl LevelProgress {
    pub fn coin(&self, i: usize) -> bool {
        self.coins & (1 << i) != 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub levels: BTreeMap<i32, LevelProgress>,
    /// Named one-time flags (`flag <name>` lines), like the game's UGVs (the secret door).
    pub flags: std::collections::BTreeSet<String>,
    /// Settings (`set <key> <value>` lines): volumes and the options' game variables.
    pub settings: BTreeMap<String, String>,
    /// Off for headless screenshots and tests: nothing is written.
    pub persist: bool,
}

impl Progress {
    pub fn parse(text: &str) -> Progress {
        let mut levels = BTreeMap::new();
        let mut flags = std::collections::BTreeSet::new();
        let mut settings = BTreeMap::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("set ") {
                if let Some((k, v)) = rest.trim().split_once(' ') {
                    settings.insert(k.to_string(), v.to_string());
                }
                continue;
            }
            if let Some(name) = line.strip_prefix("flag ") {
                flags.insert(name.trim().to_string());
                continue;
            }
            let mut words = line.split_whitespace();
            let Some(id) = words.next().and_then(|w| w.parse::<i32>().ok()) else { continue };
            let mut p = LevelProgress::default();
            for w in words {
                let Some((k, v)) = w.split_once('=') else { continue };
                let v: u32 = v.parse().unwrap_or(0);
                match k {
                    "best" => p.best = v.min(100),
                    "practice" => p.practice = v.min(100),
                    "attempts" => p.attempts = v,
                    "jumps" => p.jumps = v,
                    "coins" => p.coins = (v & 7) as u8,
                    _ => {}
                }
            }
            levels.insert(id, p);
        }
        Progress { levels, flags, settings, persist: false }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (id, p) in &self.levels {
            out.push_str(&format!("{id} best={} practice={} attempts={} jumps={} coins={}\n", p.best, p.practice, p.attempts, p.jumps, p.coins));
        }
        for f in &self.flags {
            out.push_str(&format!("flag {f}\n"));
        }
        for (k, v) in &self.settings {
            out.push_str(&format!("set {k} {v}\n"));
        }
        out
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    pub fn set_flag(&mut self, name: &str) {
        self.flags.insert(name.to_string());
    }

    /// A number setting (volumes: 0..1).
    pub fn number(&self, key: &str, default: f32) -> f32 {
        self.settings.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    pub fn set_number(&mut self, key: &str, v: f32) {
        self.settings.insert(key.to_string(), format!("{v:.3}"));
    }

    /// A game variable toggle (`GameManager::getGameVariable`), with its default.
    pub fn var(&self, key: &str, default: bool) -> bool {
        self.settings.get(key).map_or(default, |v| v == "1")
    }

    pub fn set_var(&mut self, key: &str, on: bool) {
        self.settings.insert(key.to_string(), if on { "1" } else { "0" }.to_string());
    }

    pub fn get(&self, id: i32) -> LevelProgress {
        self.levels.get(&id).copied().unwrap_or_default()
    }

    pub fn entry(&mut self, id: i32) -> &mut LevelProgress {
        self.levels.entry(id).or_default()
    }

    /// Secret coins collected over all main levels (the game's stat "8", which unlocks
    /// Clubstep, Theory of Everything 2 and Deadlocked).
    pub fn secret_coins(&self) -> u32 {
        let levels: u32 = self.levels.iter().filter(|(id, _)| (1..=22).contains(*id) || **id == 3001).map(|(_, p)| p.coins.count_ones()).sum();
        // the vaults' coins (`storeSecretCoin("secret06")`, "secretB03")
        levels + self.flags.iter().filter(|f| f.starts_with("coin ")).count() as u32
    }

    /// User coins (stat 12): coins taken in levels that aren't the main, tower or gauntlet ones.
    pub fn user_coins(&self) -> u32 {
        self.levels.iter().filter(|(id, _)| **id > 5100 || (23..1000).contains(*id) && **id != 3001).map(|(_, p)| p.coins.count_ones()).sum()
    }

    /// Load from Cadence's save (empty if there is none).
    pub fn load() -> Progress {
        let mut p = read().map(|t| Progress::parse(&t)).unwrap_or_default();
        p.persist = true;
        p
    }

    pub fn save(&self) {
        if self.persist {
            write(&self.to_text());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn path() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("CADENCE_SAVE_DIR") {
        return Some(std::path::PathBuf::from(dir).join("progress.txt"));
    }
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Cadence")
    } else if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        std::path::PathBuf::from(xdg).join("cadence")
    } else {
        home.join(".local/share/cadence")
    };
    Some(dir.join("progress.txt"))
}

#[cfg(not(target_arch = "wasm32"))]
fn read() -> Option<String> {
    std::fs::read_to_string(path()?).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write(text: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // write then rename, so a crash never leaves half a file
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(target_arch = "wasm32")]
const KEY: &str = "cadence.progress";

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn read() -> Option<String> {
    storage()?.get_item(KEY).ok()?
}

#[cfg(target_arch = "wasm32")]
fn write(text: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(KEY, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut p = Progress::default();
        *p.entry(1) = LevelProgress { best: 57, practice: 100, attempts: 12, jumps: 300, coins: 0b101 };
        p.entry(14).attempts = 3;
        let q = Progress::parse(&p.to_text());
        assert_eq!(q.get(1), p.get(1));
        assert_eq!(q.get(14).attempts, 3);
        assert_eq!(q.secret_coins(), 2);
        assert!(q.get(1).coin(0) && !q.get(1).coin(1) && q.get(1).coin(2));
    }
}

//! The players you follow, kept on this device too (the profile's follow button shows it and
//! the Followed search sends it even logged out); with an account the button also follows them
//! on the servers (`followGJUser.php`), whose lists the Friends page's Following / Muted tabs
//! show (`getGJUserList20` types 2 / 3). One `account name` a line.

#[cfg(not(target_arch = "wasm32"))]
mod store {
    fn file(which: &str) -> std::path::PathBuf {
        crate::online::cache_dir().with_file_name(format!("{which}.txt"))
    }

    pub fn read(which: &str) -> String {
        std::fs::read_to_string(file(which)).unwrap_or_default()
    }

    pub fn write(which: &str, text: &str) {
        let f = file(which);
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

    pub fn read(which: &str) -> String {
        storage().and_then(|s| s.get_item(&format!("cadence.{which}")).ok().flatten()).unwrap_or_default()
    }

    pub fn write(which: &str, text: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item(&format!("cadence.{which}"), text);
        }
    }
}

/// The accounts in a list ("following" or "muted"): (account id, name).
pub fn list(which: &str) -> Vec<(i64, String)> {
    store::read(which)
        .lines()
        .filter_map(|l| {
            let (id, name) = l.split_once(' ').unwrap_or((l, ""));
            Some((id.parse().ok()?, name.to_string()))
        })
        .collect()
}

pub fn contains(which: &str, account: i64) -> bool {
    list(which).iter().any(|(id, _)| *id == account)
}

/// Add the account, or take it out if it's there. Returns whether it's in the list now.
pub fn toggle(which: &str, account: i64, name: &str) -> bool {
    let mut all = list(which);
    let had = all.iter().any(|(id, _)| *id == account);
    if had {
        all.retain(|(id, _)| *id != account);
    } else {
        all.push((account, name.to_string()));
    }
    store::write(which, &all.iter().map(|(id, n)| format!("{id} {n}")).collect::<Vec<_>>().join("\n"));
    !had
}

/// The followed accounts for the Followed search (`followed=` ids).
pub fn followed_ids() -> String {
    list("following").iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",")
}

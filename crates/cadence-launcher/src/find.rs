//! Finding the game's files: every Steam library on this computer (the Steam folder's
//! `steamapps/libraryfolders.vdf` lists them), the copy the launcher downloaded itself, or a
//! folder you pick. The files are the game's `Resources` folder: inside `Geometry Dash.app`
//! on macOS, next to the executable on Windows.

use std::path::{Path, PathBuf};

/// The game's Steam app id.
pub const APP_ID: u32 = 322170;

/// Whether `dir` is the game's Resources folder (it has the game's sprite sheets).
pub fn has_files(dir: &Path) -> bool {
    ["GJ_GameSheet-uhd.plist", "GJ_GameSheet-hd.plist", "GJ_GameSheet.plist"].iter().any(|f| dir.join(f).exists())
}

/// The Resources folder for a picked folder: the folder itself, the .app or the install folder
/// around it.
pub fn resources_in(dir: &Path) -> Option<PathBuf> {
    [dir.to_path_buf(), dir.join("Contents/Resources"), dir.join("Geometry Dash.app/Contents/Resources"), dir.join("Resources")]
        .into_iter()
        .find(|p| has_files(p))
}

/// Where Steam keeps its own folder on this system.
fn steam_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = dirs::home_dir() {
        v.push(home.join("Library/Application Support/Steam"));
        v.push(home.join(".steam/steam"));
        v.push(home.join(".local/share/Steam"));
        v.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    for drive in ["C", "D", "E"] {
        v.push(PathBuf::from(format!("{drive}:\\Program Files (x86)\\Steam")));
        v.push(PathBuf::from(format!("{drive}:\\Program Files\\Steam")));
        v.push(PathBuf::from(format!("{drive}:\\Steam")));
    }
    v.into_iter().filter(|p| p.join("steamapps").exists()).collect()
}

/// The library folders `libraryfolders.vdf` names (`"path"  "…"` lines).
pub fn library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("\"path\"")?.trim();
            let p = rest.trim_matches('"').replace("\\\\", "\\");
            Some(PathBuf::from(p))
        })
        .collect()
}

/// The game in any Steam library, then the launcher's own download.
pub fn find_game(own: &Path) -> Option<PathBuf> {
    let mut libraries = Vec::new();
    for root in steam_roots() {
        libraries.push(root.clone());
        if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            libraries.extend(library_paths(&vdf));
        }
    }
    libraries.into_iter().map(|l| l.join("steamapps/common/Geometry Dash")).chain(std::iter::once(own.to_path_buf())).find_map(|d| resources_in(&d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraries_from_the_vdf() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/home/me/Games/SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("/home/me/Games/SteamLibrary")]);
    }
}

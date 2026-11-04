//! Where the player's game files come from: a folder on disk (native) or files the player
//! dropped into the web page (kept in memory, never uploaded).

use std::collections::HashMap;
use std::path::PathBuf;

/// Read-only access to the game's `Resources` folder. Paths are relative and use `/`
/// (`"GJ_GameSheet-hd.plist"`, `"icons/player_01-hd.png"`, `"levels/1.txt"`).
pub trait Files {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

pub struct DiskFiles {
    pub root: PathBuf,
}

impl Files for DiskFiles {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path)).ok()
    }
}

#[derive(Default)]
pub struct MemoryFiles {
    pub files: HashMap<String, Vec<u8>>,
}

impl Files for MemoryFiles {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }
}

/// Files the game needs from `Resources` at a given texture quality suffix. The web page uses
/// this to read only these out of a dropped folder.
pub fn needed_files(suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sheet in crate::app::SHEETS {
        out.push(format!("{sheet}{suffix}.plist"));
        out.push(format!("{sheet}{suffix}.png"));
    }
    for icon in crate::app::ICON_SHEETS {
        out.push(format!("icons/{icon}{suffix}.plist"));
        out.push(format!("icons/{icon}{suffix}.png"));
    }
    // the death effects' sheets (only the chosen one is decoded, when a level starts)
    for sheet in (2..=20).filter_map(crate::death::sheet) {
        out.push(format!("{sheet}{suffix}.plist"));
        out.push(format!("{sheet}{suffix}.png"));
    }
    for single in crate::app::SINGLE_IMAGES {
        out.push(format!("{single}{suffix}.png"));
    }
    // Every level's possible background and ground art (loaded per level on first draw).
    for n in 1..=59 {
        out.push(format!("game_bg_{n:02}_001{suffix}.png"));
    }
    for n in 1..=22 {
        out.push(format!("groundSquare_{n:02}_001{suffix}.png"));
        if n >= 8 {
            out.push(format!("groundSquare_{n:02}_2_001{suffix}.png"));
        }
    }
    for n in 1..=3 {
        out.push(format!("fg_{n:02}_001{suffix}.png"));
        out.push(format!("fg_{n:02}_2_001{suffix}.png"));
    }
    out.push(crate::app::PARTICLE_TEXTURE.to_string());
    out.push(crate::app::STREAK_TEXTURE.to_string());
    out.push("Robot_AnimDesc.plist".to_string());
    out.push("Spider_AnimDesc.plist".to_string());
    out.extend(crate::animobj::FILES.iter().map(|f| f.to_string()));
    out.extend(crate::particles::ParticleLibrary::FILES.iter().map(|f| f.to_string()));
    out.extend(crate::ui::needed_files(suffix));
    for level in (1..=22).chain([3001, 5001, 5002, 5003, 5004]) {
        out.push(format!("levels/{level}.txt"));
    }
    out
}

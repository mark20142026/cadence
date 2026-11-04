//! Web entry points. The page reads the player's own `Resources` folder (dropped or picked),
//! puts the files the game needs into a `FileStore`, and calls `start`. Nothing is uploaded.

use wasm_bindgen::prelude::*;

use crate::assets::Quality;
use crate::files::MemoryFiles;

#[wasm_bindgen]
#[derive(Default)]
pub struct FileStore {
    files: MemoryFiles,
}

#[wasm_bindgen]
impl FileStore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> FileStore {
        FileStore::default()
    }

    /// Add one file by its path relative to `Resources` (e.g. "icons/player_01-hd.png").
    pub fn add(&mut self, path: String, bytes: Vec<u8>) {
        self.files.files.insert(path, bytes);
    }

    pub fn len(&self) -> usize {
        self.files.files.len()
    }
}

/// The files the game needs, so the page reads only these out of the dropped folder.
#[wasm_bindgen]
pub fn needed_files(quality: &str) -> Vec<String> {
    crate::files::needed_files(parse_quality(quality).suffix())
}

/// The song file a level plays, so the page can start decoding it while the level loads.
#[wasm_bindgen]
pub fn level_song(level_id: i32) -> Option<String> {
    crate::audio::level_song(level_id)
}

/// The death sound, for the same reason.
#[wasm_bindgen]
pub fn death_effect() -> String {
    crate::audio::DEATH_EFFECT.to_string()
}

/// Start the game in the page (appends a canvas to the document body).
#[wasm_bindgen]
pub fn start(files: FileStore, level_id: i32, quality: &str, force_webgl: bool) {
    console_error_panic_hook::set_once();
    crate::render::FORCE_GL.store(force_webgl, std::sync::atomic::Ordering::Relaxed);
    crate::app::run(Box::new(files.files), Box::new(crate::audio::WebAudio), level_id, parse_quality(quality));
}

fn parse_quality(quality: &str) -> Quality {
    match quality {
        "low" => Quality::Low,
        "high" => Quality::High,
        _ => Quality::Medium,
    }
}

/// Level files (`.gmd`) dropped on the page while the game runs, waiting for the next frame.
pub static IMPORTS: std::sync::Mutex<Vec<Vec<u8>>> = std::sync::Mutex::new(Vec::new());

/// The page hands over a dropped level file.
#[wasm_bindgen]
pub fn import_level_file(bytes: Vec<u8>) {
    IMPORTS.lock().unwrap_or_else(|e| e.into_inner()).push(bytes);
}

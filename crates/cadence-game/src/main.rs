//! Cadence, native build (Metal on macOS).
//!
//! Usage: cadence-game [level id]   (game files: $CADENCE_GD_RESOURCES or the macOS Steam path;
//! texture quality: $CADENCE_QUALITY = low | medium | high)

use std::path::PathBuf;

use cadence_game::assets::Quality;
use cadence_game::audio::{Audio, NativeAudio, NoAudio};
use cadence_game::files::DiskFiles;

fn main() {
    let given = std::env::var_os("CADENCE_GD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        home.join("Library/Application Support/Steam/steamapps/common/Geometry Dash/Geometry Dash.app/Contents/Resources")
    });
    // the Resources folder itself, or the app / install folder around it
    let candidates = [given.clone(), given.join("Contents/Resources"), given.join("Geometry Dash.app/Contents/Resources"), given.join("Resources")];
    let has_files = |p: &PathBuf| ["GJ_GameSheet-uhd.plist", "GJ_GameSheet-hd.plist", "GJ_GameSheet.plist"].iter().any(|f| p.join(f).exists());
    let root = match candidates.iter().find(|p| has_files(p)) {
        Some(p) => p.clone(),
        None => {
            let message = format!(
                "Cadence needs your own copy of Geometry Dash 2.2091, and couldn't find its files in:\n{}\n\nSet CADENCE_GD_RESOURCES to the game's Resources folder (or its .app / install folder) and start Cadence again.",
                given.display()
            );
            eprintln!("{message}");
            #[cfg(target_os = "macos")]
            if std::env::var_os("CADENCE_SCREENSHOT").is_none() {
                let script = format!("display dialog \"{}\" with title \"Cadence\" buttons {{\"OK\"}} with icon caution", message.replace('\\', "\\\\").replace('"', "\\\""));
                let _ = std::process::Command::new("osascript").arg("-e").arg(script).status();
            }
            std::process::exit(1);
        }
    };
    // CADENCE_CHECK_DEATH=<dir>: compare the death effects with the harness's captures there
    if let Some(dir) = std::env::var_os("CADENCE_CHECK_DEATH") {
        print!("{}", cadence_game::death_check::check_real(&DiskFiles { root }, std::path::Path::new(&dir)));
        return;
    }
    let level_id = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let quality = match std::env::var("CADENCE_QUALITY").as_deref() {
        Ok("low") => Quality::Low,
        Ok("medium") => Quality::Medium,
        _ => Quality::High,
    };
    // Offscreen screenshots and benchmarks run silent.
    let headless = std::env::var_os("CADENCE_SCREENSHOT").is_some() || std::env::var_os("CADENCE_BENCH").is_some() || std::env::var_os("CADENCE_UI_SCRIPT").is_some();
    // CADENCE_BENCH_AUDIO: a benchmark keeps its audio running, muted (the real game's bench
    // decodes and mixes its music too).
    let bench_audio = std::env::var_os("CADENCE_BENCH_AUDIO").is_some();
    let audio: Box<dyn Audio> = match NativeAudio::new(Box::new(DiskFiles { root: root.clone() })) {
        Some(a) if !headless => Box::new(a),
        Some(mut a) if bench_audio => {
            a.set_volumes(0.0, 0.0);
            Box::new(a)
        }
        _ => Box::new(NoAudio),
    };
    cadence_game::app::run(Box::new(DiskFiles { root }), audio, level_id, quality);
}

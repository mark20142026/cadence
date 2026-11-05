//! Cadence's launcher: finds your copy of the game (any Steam library, or a folder you pick),
//! or gets it with your Steam login (SteamCMD), then starts Cadence on it. Its own window
//! (egui), since Cadence itself draws with the game's files.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod find;
mod steam;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Color32, RichText};

const RELEASES: &str = "https://github.com/mark20142026/cadence/releases";
const RELEASES_API: &str = "https://api.github.com/repos/mark20142026/cadence/releases/latest";

/// The launcher's folder: settings, SteamCMD, the downloaded game.
fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Cadence")
}

/// The game Cadence downloads itself into.
fn own_install() -> PathBuf {
    data_dir().join("game")
}

/// Settings, one `key=value` a line: the game folder picked, the texture quality.
#[derive(Clone, Debug, Default)]
struct Settings {
    game: Option<PathBuf>,
    quality: String,
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("launcher.txt")
    }

    fn load() -> Settings {
        let mut s = Settings { quality: "high".into(), ..Default::default() };
        for line in std::fs::read_to_string(Self::file()).unwrap_or_default().lines() {
            match line.split_once('=') {
                Some(("game", v)) if !v.is_empty() => s.game = Some(PathBuf::from(v)),
                Some(("quality", v)) => s.quality = v.into(),
                _ => {}
            }
        }
        s
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        let game = self.game.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        let _ = std::fs::write(Self::file(), format!("game={game}\nquality={}\n", self.quality));
    }
}

/// Cadence's own executable: next to the launcher (in the .app, the zip or the install).
fn game_exe() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    let name = if cfg!(windows) { "cadence-game.exe" } else { "cadence-game" };
    [dir.join(name), dir.join("../../../target/release").join(name), dir.join("../release").join(name)].into_iter().find(|p| p.exists())
}

struct Launcher {
    settings: Settings,
    /// The game's Resources folder, when found
    resources: Option<PathBuf>,
    user: String,
    password: String,
    code: String,
    need_code: bool,
    confirm: bool,
    steam: Option<Receiver<steam::Event>>,
    progress: f32,
    log: Vec<String>,
    status: String,
    latest: Option<Receiver<Option<String>>>,
    newer: Option<String>,
    show_steam: bool,
    /// CADENCE_LAUNCHER_SHOT=<png>: save the window there after a few frames and quit
    shot: Option<PathBuf>,
    frames: u32,
}

impl Launcher {
    fn new() -> Launcher {
        let settings = Settings::load();
        let resources = settings.game.as_deref().and_then(find::resources_in).or_else(|| find::find_game(&own_install()));
        // is there a newer Cadence? (one request to GitHub's releases)
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let tag = ureq::get(RELEASES_API).set("User-Agent", "cadence-launcher").call().ok().and_then(|r| r.into_string().ok()).and_then(|body| {
                let i = body.find("\"tag_name\"")?;
                let rest = &body[i + 10..];
                let start = rest.find('"')? + 1;
                let end = rest[start..].find('"')? + start;
                Some(rest[start..end].trim_start_matches('v').to_string())
            });
            let _ = tx.send(tag);
        });
        Launcher {
            show_steam: resources.is_none() || std::env::var_os("CADENCE_LAUNCHER_STEAM").is_some(),
            settings,
            resources,
            user: String::new(),
            password: String::new(),
            code: String::new(),
            need_code: false,
            confirm: false,
            steam: None,
            progress: 0.0,
            log: Vec::new(),
            status: String::new(),
            latest: Some(rx),
            newer: None,
            shot: std::env::var_os("CADENCE_LAUNCHER_SHOT").map(PathBuf::from),
            frames: 0,
        }
    }

    fn play(&mut self) {
        let Some(res) = self.resources.clone() else { return };
        let Some(exe) = game_exe() else {
            self.status = "Cadence's game file (cadence-game) isn't next to the launcher. Reinstall Cadence.".into();
            return;
        };
        match std::process::Command::new(exe).env("CADENCE_GD_RESOURCES", res).env("CADENCE_QUALITY", &self.settings.quality).spawn() {
            Ok(_) => self.status = "Cadence is starting…".into(),
            Err(e) => self.status = format!("Cadence didn't start: {e}"),
        }
    }

    fn pick_folder(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Your Geometry Dash folder (or Geometry Dash.app)").pick_folder() else { return };
        match find::resources_in(&dir) {
            Some(res) => {
                self.settings.game = Some(dir);
                self.settings.save();
                self.resources = Some(res);
                self.status = "Found your copy of the game.".into();
                self.show_steam = false;
            }
            None => self.status = "That folder doesn't have the game's files. Pick the folder Steam installed the game in.".into(),
        }
    }

    fn start_steam(&mut self) {
        if self.user.trim().is_empty() || self.password.is_empty() {
            self.status = "Enter your Steam account name and password.".into();
            return;
        }
        let (tx, rx) = channel();
        steam::install(data_dir().join("steamcmd"), own_install(), self.user.trim().to_string(), self.password.clone(), self.code.clone(), tx);
        self.steam = Some(rx);
        self.progress = 0.0;
        self.log.clear();
        self.need_code = false;
        self.confirm = false;
        self.status = "Getting the game with your Steam account…".into();
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.latest
            && let Ok(tag) = rx.try_recv()
        {
            self.latest = None;
            if let Some(tag) = tag
                && newer(&tag, env!("CARGO_PKG_VERSION"))
            {
                self.newer = Some(tag);
            }
        }
        let Some(rx) = &self.steam else { return };
        let mut done = false;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                steam::Event::Line(l) => {
                    self.log.push(l);
                    if self.log.len() > 200 {
                        self.log.remove(0);
                    }
                }
                steam::Event::Progress(p) => self.progress = p,
                steam::Event::NeedCode => {
                    self.need_code = true;
                    self.status = "Steam sent a Steam Guard code (in the Steam app or your email). Enter it and press Get the game again.".into();
                }
                steam::Event::Confirm => {
                    self.confirm = true;
                    self.status = "Approve the login in your Steam app…".into();
                }
                steam::Event::Failed(e) => {
                    if !self.need_code {
                        self.status = format!("Steam: {e}");
                    }
                    done = true;
                }
                steam::Event::Done => {
                    self.resources = find::resources_in(&own_install());
                    self.status = if self.resources.is_some() { "The game is installed. Press Play!".into() } else { "Steam finished, but the game's files aren't there.".into() };
                    self.show_steam = self.resources.is_none();
                    done = true;
                }
            }
        }
        if done {
            self.steam = None;
            // the password isn't kept once SteamCMD has it
            self.password.clear();
            self.code.clear();
        }
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) {
    let Ok(file) = std::fs::File::create(path) else { return };
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), image.size[0] as u32, image.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
        let _ = w.write_image_data(&bytes);
    }
}

/// Whether release `tag` is newer than `mine` ("0.0.3" > "0.0.2").
fn newer(tag: &str, mine: &str) -> bool {
    let v = |s: &str| s.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    v(tag) > v(mine)
}

const ACCENT: Color32 = Color32::from_rgb(64, 227, 72);

impl eframe::App for Launcher {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        if let Some(path) = self.shot.clone() {
            self.frames += 1;
            root.ctx().request_repaint();
            if self.frames == 20 {
                let ctx = root.ctx().clone();
                root.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                    save_png(&path, &image);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx.request_repaint();
                })));
            }
        }
        if self.steam.is_some() || self.latest.is_some() {
            root.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::CentralPanel::default().show(root, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Cadence").size(30.0).strong());
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
                if let Some(tag) = &self.newer {
                    ui.hyperlink_to(RichText::new(format!("v{tag} is out — update")).color(ACCENT), RELEASES);
                }
            });
            ui.label(RichText::new("A from-scratch reimplementation of Geometry Dash 2.2091. It plays with your own copy of the game.").weak());
            ui.add_space(12.0);
            match &self.resources {
                Some(res) => {
                    ui.label(RichText::new("✔ Your copy of the game is ready").color(ACCENT));
                    ui.label(RichText::new(res.display().to_string()).small().weak());
                }
                None => {
                    ui.label(RichText::new("Cadence couldn't find your copy of the game.").color(Color32::from_rgb(255, 165, 75)));
                    ui.label(RichText::new("Install Geometry Dash from Steam, pick its folder, or get it below with your Steam login.").weak());
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let play = egui::Button::new(RichText::new("  Play  ").size(22.0).strong()).fill(Color32::from_rgb(40, 150, 50)).min_size(egui::vec2(160.0, 48.0));
                if ui.add_enabled(self.resources.is_some(), play).clicked() {
                    self.play();
                }
                ui.vertical(|ui| {
                    if ui.button("Pick the game's folder…").clicked() {
                        self.pick_folder();
                    }
                    let label = if self.show_steam { "Hide Steam login" } else { "Get the game with Steam…" };
                    if ui.button(label).clicked() {
                        self.show_steam = !self.show_steam;
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Texture quality");
                let before = self.settings.quality.clone();
                egui::ComboBox::from_id_salt("quality").selected_text(&self.settings.quality).show_ui(ui, |ui| {
                    for q in ["high", "medium", "low"] {
                        ui.selectable_value(&mut self.settings.quality, q.to_string(), q);
                    }
                });
                if before != self.settings.quality {
                    self.settings.save();
                }
            });
            if self.show_steam {
                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label(RichText::new("Get the game with your Steam account").strong());
                    ui.label(RichText::new("You need to own Geometry Dash on Steam. Valve's SteamCMD logs in and downloads it into Cadence's folder. Your password goes to SteamCMD only; Cadence doesn't keep it.").small().weak());
                    let busy = self.steam.is_some();
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::Grid::new("steam").num_columns(2).show(ui, |ui| {
                            ui.label("Account name");
                            ui.text_edit_singleline(&mut self.user);
                            ui.end_row();
                            ui.label("Password");
                            ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                            ui.end_row();
                            if self.need_code {
                                ui.label("Steam Guard code");
                                ui.text_edit_singleline(&mut self.code);
                                ui.end_row();
                            }
                        });
                        if ui.button("Get the game").clicked() {
                            self.start_steam();
                        }
                    });
                    if busy {
                        ui.add(egui::ProgressBar::new(self.progress).show_percentage().animate(true));
                    }
                    if !self.log.is_empty() {
                        egui::CollapsingHeader::new("SteamCMD output").show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
                                for l in &self.log {
                                    ui.label(RichText::new(l).monospace().small());
                                }
                            });
                        });
                    }
                });
            }
            if !self.status.is_empty() {
                ui.add_space(8.0);
                ui.label(&self.status);
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.label(RichText::new("Cadence isn't affiliated with or endorsed by RobTop Games. It ships none of the game's files.").small().weak());
                ui.hyperlink_to("Cadence on GitHub", "https://github.com/mark20142026/cadence");
            });
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Cadence").with_inner_size([620.0, 440.0]).with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native("Cadence", options, Box::new(|cc| {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Ok(Box::new(Launcher::new()))
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions() {
        assert!(super::newer("0.0.3", "0.0.2"));
        assert!(super::newer("0.1.0", "0.0.9"));
        assert!(!super::newer("0.0.2", "0.0.2"));
    }
}

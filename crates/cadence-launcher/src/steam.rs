//! Getting the game with your Steam account: Valve's SteamCMD (downloaded from Valve on first
//! use) logs in and installs the game you own into the launcher's own folder. Your password
//! and Steam Guard code go to SteamCMD only; the launcher never writes them anywhere.
//!
//! The run is `+login <user> <password> [code] +app_update 322170 validate +quit` with password
//! prompts off, so a missing Steam Guard code ends the run with a message the launcher reads
//! and asks you for the code, then runs again with it.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

/// What a SteamCMD run reports to the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Line(String),
    /// Download progress, 0..1
    Progress(f32),
    /// Steam wants a Steam Guard code (from the app or the email)
    NeedCode,
    /// Waiting for the login to be approved in the Steam app
    Confirm,
    Failed(String),
    Done,
}

fn steamcmd_url() -> &'static str {
    if cfg!(target_os = "macos") {
        "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_osx.tar.gz"
    } else if cfg!(windows) {
        "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip"
    } else {
        "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz"
    }
}

fn steamcmd_exe(dir: &Path) -> PathBuf {
    if cfg!(windows) { dir.join("steamcmd.exe") } else { dir.join("steamcmd.sh") }
}

/// The platform build SteamCMD installs: the Mac's own on macOS, else the Windows one (the
/// game has no Linux build; Cadence reads the Windows files just the same).
pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else { "windows" }
}

/// SteamCMD in `dir`, downloaded from Valve when it isn't there yet.
fn ensure_steamcmd(dir: &Path, tx: &Sender<Event>) -> Result<PathBuf, String> {
    let exe = steamcmd_exe(dir);
    if exe.exists() {
        return Ok(exe);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let _ = tx.send(Event::Line("Downloading SteamCMD from Valve…".into()));
    let resp = ureq::get(steamcmd_url()).call().map_err(|e| format!("SteamCMD download: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if cfg!(windows) {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        zip.extract(dir).map_err(|e| e.to_string())?;
    } else {
        let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
        tar::Archive::new(gz).unpack(dir).map_err(|e| e.to_string())?;
    }
    if exe.exists() { Ok(exe) } else { Err("SteamCMD wasn't in Valve's download".into()) }
}

/// What a SteamCMD output line means.
pub fn classify(line: &str) -> Option<Event> {
    let l = line.to_ascii_lowercase();
    if l.contains("two-factor") || l.contains("steam guard code") || l.contains("authentication code") || l.contains("account logon denied") {
        return Some(Event::NeedCode);
    }
    if l.contains("confirm the login") || l.contains("mobile authenticator") && l.contains("waiting") {
        return Some(Event::Confirm);
    }
    if l.contains("invalid password") || l.contains("login failure") || l.contains("rate limit") || l.contains("no subscription") || l.contains("error! app") {
        return Some(Event::Failed(line.trim().to_string()));
    }
    if l.contains("success! app") && l.contains("fully installed") {
        return Some(Event::Done);
    }
    // "Update state (0x61) downloading, progress: 45.23 (123 / 456)"
    if let Some(i) = l.find("progress: ") {
        let num: String = l[i + 10..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(p) = num.parse::<f32>() {
            return Some(Event::Progress(p / 100.0));
        }
    }
    None
}

/// Log in and install the game into `install`. Runs on its own thread; reports on `tx`.
pub fn install(tool_dir: PathBuf, install: PathBuf, user: String, password: String, code: String, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let exe = match ensure_steamcmd(&tool_dir, &tx) {
            Ok(e) => e,
            Err(e) => {
                let _ = tx.send(Event::Failed(e));
                return;
            }
        };
        let _ = std::fs::create_dir_all(&install);
        let mut cmd = Command::new(&exe);
        cmd.arg("+@ShutdownOnFailedCommand").arg("1").arg("+@NoPromptForPassword").arg("1");
        cmd.arg("+@sSteamCmdForcePlatformType").arg(platform());
        cmd.arg("+force_install_dir").arg(&install);
        cmd.arg("+login").arg(&user).arg(&password);
        if !code.trim().is_empty() {
            cmd.arg(code.trim());
        }
        cmd.arg("+app_update").arg(super::find::APP_ID.to_string()).arg("validate").arg("+quit");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
        let _ = tx.send(Event::Line("Logging in to Steam…".into()));
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Event::Failed(format!("SteamCMD didn't start: {e}")));
                return;
            }
        };
        let mut finished = false;
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                // the password never shows: SteamCMD doesn't echo it, and no line with it passes
                if line.contains(&password) && !password.is_empty() {
                    continue;
                }
                match classify(&line) {
                    Some(Event::Done) => finished = true,
                    Some(e) => {
                        let _ = tx.send(e);
                    }
                    None => {}
                }
                let _ = tx.send(Event::Line(line));
            }
        }
        let status = child.wait();
        if finished || status.as_ref().is_ok_and(|s| s.success()) && super::find::resources_in(&install).is_some() {
            let _ = tx.send(Event::Done);
        } else {
            let _ = tx.send(Event::Failed("SteamCMD stopped before the game was installed.".into()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steamcmd() {
        assert!(matches!(classify(" Update state (0x61) downloading, progress: 45.23 (1000 / 2211)"), Some(Event::Progress(p)) if (p - 0.4523).abs() < 1e-4));
        assert_eq!(classify("Success! App '322170' fully installed."), Some(Event::Done));
        assert_eq!(classify("This computer has not been authenticated for your account using Steam Guard."), None);
        assert_eq!(classify("Please enter your Steam Guard code from the email"), Some(Event::NeedCode));
        assert!(matches!(classify("FAILED (Invalid Password)"), Some(Event::Failed(_))));
    }
}

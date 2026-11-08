//! "Update available", for the Windows and Linux tray.
//!
//! The Mac menu bar asks GitHub for the latest release and writes the answer to
//! `update.json` in the data directory; the dashboard reads that file, so every
//! surface agrees without each one asking. On Windows and Linux the tray does the
//! asking, on the same schedule (daily or weekly, as settings say; "manual" or
//! updates turned off means never) and into the same file. A check sends GitHub
//! this computer's address and the Geniuz version, nothing from any folder. It
//! downloads nothing and installs nothing: Download opens the release in a browser.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::settings::Settings;

const LATEST_RELEASE: &str = "https://api.github.com/repos/jackccrawford/Geniuz/releases/latest";
/// Only links under the project's own releases open from Geniuz.
pub const RELEASES_PREFIX: &str = "https://github.com/jackccrawford/Geniuz/releases/";

/// A release newer than this one.
#[derive(Debug, Clone, PartialEq)]
pub struct Available {
    pub version: String,
    pub url: String,
}

fn file() -> PathBuf {
    crate::data_dir().join("update.json")
}

/// The last answer, always as an object: anything else in the file counts as empty.
fn read() -> Value {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn write(v: &Value) -> Result<(), String> {
    let path = file();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, v.to_string())
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("Couldn't save {}: {}", path.display(), e))
}

fn time(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    v.get(key)?.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc))
}

/// Whether `candidate` is a newer version than `current`, by number: 5.0.10 > 5.0.9.
/// A pre-release suffix counts as not newer.
pub fn version_newer(candidate: &str, current: &str) -> bool {
    let clean = |v: &str| v.trim().trim_start_matches(['v', 'V']).to_string();
    let (cand, cur) = (clean(candidate), clean(current));
    if cand.contains('-') {
        return false;
    }
    let parts = |v: &str| -> Vec<u64> { v.split('-').next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(&cand), parts(&cur));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// The last check's answer, if it found a release newer than this one.
pub fn available() -> Option<Available> {
    let v = read();
    let version = v.get("latest")?.as_str()?.to_string();
    let url = v.get("url")?.as_str()?.to_string();
    (url.starts_with(RELEASES_PREFIX) && version_newer(&version, env!("CARGO_PKG_VERSION")))
        .then_some(Available { version, url })
}

/// Whether a check is due now. A failed check waits an hour; a clock set back
/// can't stop checks for good.
pub fn due(settings: &Settings, now: DateTime<Utc>) -> bool {
    if !settings.autoupdate_enabled {
        return false;
    }
    let every = match settings.update_check_frequency.as_str() {
        "daily" => Duration::days(1),
        "weekly" => Duration::days(7),
        _ => return false,
    };
    let v = read();
    if let Some(retry) = time(&v, "retry_at") {
        if retry > now && retry - now <= Duration::hours(1) {
            return false;
        }
    }
    match time(&v, "checked_at") {
        Some(last) => now - last >= every || last > now,
        None => true,
    }
}

/// Ask GitHub for the latest release and record the answer in `update.json`.
/// Blocks for up to 15 seconds, so call it off the UI thread.
pub fn check() -> Result<Option<Available>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .new_agent();
    let answer: Result<Value, String> = agent
        .get(LATEST_RELEASE)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", &format!("Geniuz/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())
        .and_then(|mut r| r.body_mut().read_json::<Value>().map_err(|e| e.to_string()));
    let mut v = read();
    let now = Utc::now();
    let release = match answer {
        Ok(r) if !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false) => r,
        Ok(_) | Err(_) => {
            // A failed check doesn't count as a check: try again in an hour.
            v["retry_at"] = json!((now + Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true));
            write(&v)?;
            return Err("Couldn't get the latest release from GitHub".to_string());
        }
    };
    let latest = release["tag_name"].as_str().unwrap_or("").trim_start_matches(['v', 'V']).to_string();
    // The installer for this system if the release has one, else the release page.
    let asset = if cfg!(windows) { "Geniuz-Setup.exe" } else { "" };
    let url = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| !asset.is_empty() && x["name"].as_str() == Some(asset)))
        .and_then(|x| x["browser_download_url"].as_str())
        .or_else(|| release["html_url"].as_str())
        .unwrap_or("")
        .to_string();
    let found = (version_newer(&latest, env!("CARGO_PKG_VERSION")) && url.starts_with(RELEASES_PREFIX))
        .then(|| Available { version: latest, url });
    let out = json!({
        "checked_at": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "latest": found.as_ref().map(|f| f.version.clone()),
        "url": found.as_ref().map(|f| f.url.clone()),
    });
    write(&out)?;
    Ok(found)
}

/// Open a release link in the browser, without a shell in between. Only the
/// project's own release links are opened.
pub fn open(url: &str) -> Result<(), String> {
    if !url.starts_with(RELEASES_PREFIX) {
        return Err("Not a Geniuz release link".to_string());
    }
    let program = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let mut child = std::process::Command::new(program).arg(url).spawn().map_err(|e| e.to_string())?;
    // Waited on, so the finished opener doesn't linger as a zombie process.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_number() {
        assert!(version_newer("5.0.10", "5.0.9"));
        assert!(version_newer("v5.1", "5.0.3"));
        assert!(!version_newer("5.0.3", "5.0.3"));
        assert!(!version_newer("5.0.2", "5.0.3"));
        assert!(!version_newer("5.0.7-rc.1", "5.0.6"));
    }

    /// Asks the real GitHub. Run by hand: cargo test --lib update -- --ignored
    #[test]
    #[ignore]
    fn asks_github_and_records_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("GENIUZ_HOME", dir.path());
        let found = check().expect("GitHub answered");
        let recorded: Value = serde_json::from_str(&std::fs::read_to_string(file()).unwrap()).unwrap();
        assert!(time(&recorded, "checked_at").is_some(), "{recorded}");
        assert_eq!(found.is_some(), recorded["latest"].is_string(), "{recorded}");
        let settings = Settings::default();
        assert!(!due(&settings, Utc::now()), "just checked, so not due");
        eprintln!("recorded: {recorded}");
    }
}

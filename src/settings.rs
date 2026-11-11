//! User settings — persisted as JSON at `$GENIUZ_HOME/settings.json`.
//!
//! Source of truth for cross-platform user preferences (autoupdate cadence,
//! recent-memory display count, launch-at-login). Both the Mac menubar app
//! and the Windows tray read/write through this layer (via `geniuz settings`
//! CLI commands or by reading the file directly).
//!
//! Platform-specific enforcement (LaunchAgents on Mac, Run registry on
//! Windows, Sparkle/WinSparkle update schedulers) syncs to these values —
//! the JSON is the user-visible truth, the platform mechanism is the
//! enforcement. Keeps the user model simple: "change a setting in one place,
//! every surface respects it."
//!
//! Path: `$GENIUZ_HOME/settings.json` — moves with the data dir if the user
//! relocates GENIUZ_HOME.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SETTINGS_VERSION: u32 = 1;

/// All user-settable preferences. New fields must have `#[serde(default)]`
/// so older settings.json files keep loading after a schema bump.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Schema version. Bump on breaking changes; add migration logic. A file
    /// without one is version 1, as the menu bar app reads it.
    #[serde(default = "default_version")]
    pub version: u32,

    /// Auto-launch the menubar app / tray icon at login.
    /// Mac: LaunchAgent. Windows: HKCU\...\Run registry key.
    #[serde(default = "default_launch_at_login")]
    pub launch_at_login: bool,

    /// Whether the app checks for updates automatically.
    /// Mac: Sparkle SUEnableAutomaticChecks. Windows: WinSparkle equivalent.
    #[serde(default = "default_autoupdate_enabled")]
    pub autoupdate_enabled: bool,

    /// How often to check for updates when autoupdate is enabled.
    #[serde(default)]
    pub update_check_frequency: UpdateFrequency,

    /// How many recent memories to surface in the menubar/tray dropdown.
    /// 0 hides the recent section entirely.
    #[serde(default = "default_recent_memories_count")]
    pub recent_memories_count: u32,

    /// Whether the system may correct words as a person types a memory or a
    /// folder's description in the window. Off unless asked for: the reader is an
    /// AI, which understands a typo at once, and what is recorded should be what was
    /// typed (Jack, 10 Nov 2026). With it on, macOS can also change the last word on
    /// the click meant for Keep, and that click is spent on the correction.
    #[serde(default)]
    pub autocorrect: bool,

    /// Fields this version doesn't know (written by another app or a newer
    /// version), kept as they were so saving never drops them.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Update check cadence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateFrequency {
    #[default]
    Daily,
    Weekly,
    Manual,
}

impl UpdateFrequency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

fn default_launch_at_login() -> bool { true }
fn default_autoupdate_enabled() -> bool { true }
fn default_recent_memories_count() -> u32 { 5 }

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            launch_at_login: default_launch_at_login(),
            autoupdate_enabled: default_autoupdate_enabled(),
            update_check_frequency: UpdateFrequency::default(),
            recent_memories_count: default_recent_memories_count(),
            autocorrect: false,
            extra: serde_json::Map::new(),
        }
    }
}

fn default_version() -> u32 {
    SETTINGS_VERSION
}

impl Settings {
    /// Path to the settings file inside the current GENIUZ_HOME.
    pub fn path() -> PathBuf {
        crate::data_dir().join("settings.json")
    }

    /// Load from disk. Returns defaults if the file doesn't exist or is
    /// unreadable/unparseable — settings should never block app startup.
    pub fn load() -> Self {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!("[settings] Failed to parse {}: {}. Using defaults", path.display(), e);
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Load from disk, refusing a file that exists but can't be read as settings.
    /// For the command line: showing defaults as if they were the person's settings,
    /// or saving defaults over a damaged file, would lose what's in it.
    pub fn load_strict() -> Result<Self, String> {
        Self::load_strict_from(&Self::path())
    }

    /// [`Settings::load_strict`] for a given file.
    pub fn load_strict_from(path: &std::path::Path) -> Result<Self, String> {
        let s: Self = match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).map_err(|e| {
                format!("{} can't be read as settings ({}). Fix it, or move it aside to start from defaults.", path.display(), e)
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Couldn't read {}: {}", path.display(), e)),
        };
        if s.version > SETTINGS_VERSION {
            return Err(format!(
                "{} was written by a newer Geniuz (settings version {}). Update Geniuz to change it here.",
                path.display(),
                s.version
            ));
        }
        Ok(s)
    }

    /// Save to disk atomically (write to temp file, then rename).
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create dir {}: {}", parent.display(), e))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {}", e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename {} -> {}: {}", tmp.display(), path.display(), e))?;
        Ok(())
    }

    /// Apply a single key/value setting. Returns the parsed-and-stored value
    /// for echo. Used by the CLI `settings set KEY VALUE` subcommand.
    pub fn set(&mut self, key: &str, value: &str) -> Result<String, String> {
        match key {
            "launch_at_login" => {
                let v = parse_bool(value)?;
                self.launch_at_login = v;
                Ok(v.to_string())
            }
            "autoupdate_enabled" => {
                let v = parse_bool(value)?;
                self.autoupdate_enabled = v;
                Ok(v.to_string())
            }
            "update_check_frequency" => {
                let v = UpdateFrequency::parse(value)
                    .ok_or_else(|| format!("invalid value '{}': expected daily, weekly or manual", value))?;
                self.update_check_frequency = v;
                Ok(v.as_str().to_string())
            }
            "recent_memories_count" => {
                let v: u32 = value.parse().map_err(|_| format!("invalid count '{}': expected a whole number, 0 or more", value))?;
                self.recent_memories_count = v;
                Ok(v.to_string())
            }
            "autocorrect" => {
                let v = parse_bool(value)?;
                self.autocorrect = v;
                Ok(v.to_string())
            }
            _ => Err(format!("unknown setting '{}'", key)),
        }
    }

    /// Read a single setting by key. Used by `settings get KEY`.
    pub fn get(&self, key: &str) -> Result<String, String> {
        match key {
            "launch_at_login" => Ok(self.launch_at_login.to_string()),
            "autoupdate_enabled" => Ok(self.autoupdate_enabled.to_string()),
            "update_check_frequency" => Ok(self.update_check_frequency.as_str().to_string()),
            "recent_memories_count" => Ok(self.recent_memories_count.to_string()),
            "autocorrect" => Ok(self.autocorrect.to_string()),
            _ => Err(format!("unknown setting '{}'", key)),
        }
    }

    /// All settings as `key = value` lines. Used by `settings list` and
    /// helpful for debugging.
    pub fn list(&self) -> String {
        format!(
            "launch_at_login = {}\nautoupdate_enabled = {}\nupdate_check_frequency = {}\nrecent_memories_count = {}\nautocorrect = {}",
            self.launch_at_login,
            self.autoupdate_enabled,
            self.update_check_frequency.as_str(),
            self.recent_memories_count,
            self.autocorrect,
        )
    }
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid value '{}': expected true or false", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_json() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let json = r#"{"version": 1}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn strict_loading_refuses_what_it_cannot_keep_safely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        // Missing: defaults.
        assert_eq!(Settings::load_strict_from(&path).unwrap(), Settings::default());
        // Damaged: an error, and the file is left alone.
        std::fs::write(&path, "{broken").unwrap();
        assert!(Settings::load_strict_from(&path).unwrap_err().contains("can't be read"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{broken");
        // No version, as some writers leave it: version 1.
        std::fs::write(&path, r#"{"recent_memories_count": 7}"#).unwrap();
        assert_eq!(Settings::load_strict_from(&path).unwrap().recent_memories_count, 7);
        // A newer version: refused rather than rewritten.
        std::fs::write(&path, r#"{"version": 99}"#).unwrap();
        assert!(Settings::load_strict_from(&path).unwrap_err().contains("newer Geniuz"));
    }

    #[test]
    fn fields_this_version_does_not_know_are_kept() {
        let s: Settings = serde_json::from_str(r#"{"version": 1, "menu_theme": "dark"}"#).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""menu_theme":"dark""#), "{json}");
    }

    #[test]
    fn list_shows_every_setting() {
        let out = Settings::default().list();
        for key in ["launch_at_login", "autoupdate_enabled", "update_check_frequency", "recent_memories_count", "autocorrect"] {
            assert!(out.contains(key), "{key} missing from:\n{out}");
        }
    }

    #[test]
    fn set_and_get_round_trip() {
        let mut s = Settings::default();
        s.set("recent_memories_count", "10").unwrap();
        assert_eq!(s.get("recent_memories_count").unwrap(), "10");
        s.set("autoupdate_enabled", "false").unwrap();
        assert_eq!(s.get("autoupdate_enabled").unwrap(), "false");
        s.set("update_check_frequency", "weekly").unwrap();
        assert_eq!(s.get("update_check_frequency").unwrap(), "weekly");
        // Off unless asked for, and it goes both ways.
        assert_eq!(s.get("autocorrect").unwrap(), "false");
        s.set("autocorrect", "true").unwrap();
        assert_eq!(s.get("autocorrect").unwrap(), "true");
        s.set("autocorrect", "false").unwrap();
        assert_eq!(s.get("autocorrect").unwrap(), "false");
        assert!(s.set("autocorrect", "sometimes").is_err());
    }

    #[test]
    fn unknown_key_errors() {
        let mut s = Settings::default();
        assert!(s.set("nonexistent", "value").is_err());
        assert!(s.get("nonexistent").is_err());
    }

    #[test]
    fn bool_parses_common_forms() {
        assert!(parse_bool("true").unwrap());
        assert!(parse_bool("True").unwrap());
        assert!(parse_bool("1").unwrap());
        assert!(parse_bool("yes").unwrap());
        assert!(parse_bool("on").unwrap());
        assert!(!parse_bool("false").unwrap());
        assert!(!parse_bool("0").unwrap());
        assert!(parse_bool("garbage").is_err());
    }
}

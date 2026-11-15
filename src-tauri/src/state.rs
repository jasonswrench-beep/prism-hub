// SPDX-License-Identifier: Apache-2.0
//! What the hub keeps on this computer, all under %LOCALAPPDATA%\<the hub's identifier>:
//!   settings.json   the person's settings, which release of each app the hub installed, which new versions were already announced, and when
//!                   the catalog was last fetched
//!   catalog\        the last good catalog and its signature (checked again every time they are read)
//!   downloads\      installers while they are checked and run; each is deleted afterwards
//! and, beside the program itself, the IT policy file policy.json (docs/policy.md), which the hub only reads.
//!
//! Nothing here ever leaves the computer.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(&crate::brand::brand().identifier)
}

pub fn catalog_dir() -> PathBuf {
    data_dir().join("catalog")
}

pub fn downloads_dir() -> PathBuf {
    data_dir().join("downloads")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub offer_previews: bool,
    /// Closing the window leaves the hub in the tray. Turned on with Start with Windows; can be changed on its own.
    pub keep_in_tray: bool,
    /// While in the tray, check once a day.
    pub daily_check: bool,
    /// The first-run question about Start with Windows has been asked (it is asked once).
    pub asked_start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Previews are on by default while the suite is in preview: almost every OpenStage app has only preview releases.
        Settings { offer_previews: true, keep_in_tray: false, daily_check: true, asked_start_with_windows: false }
    }
}

/// The exact release the hub installed for an app, and what Windows listed once it was in. Windows lists only the three numbers (0.2.0), so
/// this is how the hub knows an installed 0.2.0 is alpha 4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub version: String,
    pub windows_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    pub settings: Settings,
    pub installed: BTreeMap<String, Record>,
    /// "appid@version" for each new version already announced with a Windows notification, so each is announced once.
    pub announced: Vec<String>,
    /// When the catalog was last fetched (seconds since 1970).
    pub last_check: Option<u64>,
    /// The hub version that ran last: a self-update shows "Updated to ..." once, after the restart.
    pub last_hub_version: Option<String>,
}

impl Saved {
    pub fn load_from(path: &Path) -> Saved {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn load() -> Saved {
        Saved::load_from(&data_dir().join("settings.json"))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_atomic(path, &bytes)
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&data_dir().join("settings.json"))
    }
}

/// Writes a file whole or not at all: to a temporary name beside it, then renamed over it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let folder = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("partial");
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| e.to_string())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// What a college's IT staff can set in policy.json beside the hub's program file. Anything not set is left to the person.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Policy {
    /// false: the hub never fetches the catalog (it shows what is installed, and Open works).
    pub update_checks: Option<bool>,
    /// false: Start with Windows is off and cannot be turned on.
    pub start_with_windows: Option<bool>,
    /// App ids (from the catalog) the hub does not list.
    pub hide_apps: Vec<String>,
    /// false: only stable releases are offered; an app with only a preview cannot be installed from the hub.
    pub previews: Option<bool>,
}

impl Policy {
    pub fn read_from(folder: &Path) -> Policy {
        std::fs::read(folder.join("policy.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn read() -> Policy {
        std::env::current_exe().ok().and_then(|e| e.parent().map(Policy::read_from)).unwrap_or_default()
    }

    pub fn checks_allowed(&self) -> bool {
        self.update_checks != Some(false)
    }

    pub fn start_allowed(&self) -> bool {
        self.start_with_windows != Some(false)
    }

    pub fn previews_allowed(&self) -> bool {
        self.previews != Some(false)
    }

    pub fn hides(&self, id: &str) -> bool {
        self.hide_apps.iter().any(|h| h.eq_ignore_ascii_case(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hub-state-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_start_with_previews_on_and_everything_else_off() {
        let s = Settings::default();
        assert!(s.offer_previews && s.daily_check);
        assert!(!s.keep_in_tray && !s.asked_start_with_windows);
    }

    #[test]
    fn saved_state_round_trips_and_a_damaged_file_gives_the_defaults() {
        let dir = temp("saved");
        let file = dir.join("settings.json");
        let mut saved = Saved::default();
        saved.installed.insert("openexplainer".into(), Record { version: "0.2.0-alpha.4".into(), windows_version: "0.2.0".into() });
        saved.settings.keep_in_tray = true;
        saved.save_to(&file).unwrap();
        assert_eq!(Saved::load_from(&file), saved);
        std::fs::write(&file, b"{ not json").unwrap();
        assert_eq!(Saved::load_from(&file), Saved::default());
        assert_eq!(Saved::load_from(&dir.join("missing.json")), Saved::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_policy_file_turns_things_off_and_its_absence_leaves_them_to_the_person() {
        let dir = temp("policy");
        assert_eq!(Policy::read_from(&dir), Policy::default());
        assert!(Policy::default().checks_allowed() && Policy::default().previews_allowed() && Policy::default().start_allowed());
        std::fs::write(dir.join("policy.json"), br#"{"updateChecks": false, "startWithWindows": false, "hideApps": ["OpenAssess"], "previews": false}"#)
            .unwrap();
        let p = Policy::read_from(&dir);
        assert!(!p.checks_allowed() && !p.start_allowed() && !p.previews_allowed());
        assert!(p.hides("openassess"));
        assert!(!p.hides("openexplainer"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

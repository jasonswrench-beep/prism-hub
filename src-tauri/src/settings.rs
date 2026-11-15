//! The person's settings, and the IT policy that can switch things off.

use crate::brand::BRAND;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub offer_previews: bool,
    pub start_with_windows: bool,
    pub keep_in_tray: bool,
    pub daily_check: bool,
    /// The start-with-Windows question was asked (it is asked once).
    pub first_run_done: bool,
    /// The newest version a notification was shown for, per app (and "hub"): never repeated.
    pub notified: BTreeMap<String, String>,
    /// When the catalog was last fetched successfully (seconds since 1970).
    pub last_check: Option<i64>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // The suite is in preview: with previews off, almost nothing would be offered.
            offer_previews: true,
            start_with_windows: false,
            keep_in_tray: false,
            daily_check: true,
            first_run_done: false,
            notified: BTreeMap::new(),
            last_check: None,
        }
    }
}

fn file() -> PathBuf {
    BRAND.data_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = crate::util::write_atomic(&path, serde_json::to_string_pretty(s).unwrap_or_default().as_bytes());
}

// ---------------------------------------------------------------------------------------------
// Policy

/// What IT staff turned off. Policy only ever restricts: a value that would switch something
/// on is ignored, so a planted file can at most hide or disable things.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Policy {
    pub no_update_checks: bool,
    pub no_start_with_windows: bool,
    pub no_previews: bool,
    pub hidden_apps: BTreeSet<String>,
    /// Where the policy came from, for Settings › About.
    pub sources: Vec<String>,
    /// Policy files that were found but not used, and why.
    pub ignored: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PolicyFile {
    update_checks: Option<bool>,
    start_with_windows: Option<bool>,
    offer_previews: Option<bool>,
    #[serde(default)]
    hide_apps: Vec<String>,
}

impl Policy {
    fn apply(&mut self, p: PolicyFile, source: String) {
        self.no_update_checks |= p.update_checks == Some(false);
        self.no_start_with_windows |= p.start_with_windows == Some(false);
        self.no_previews |= p.offer_previews == Some(false);
        self.hidden_apps.extend(p.hide_apps.into_iter().map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()));
        self.sources.push(source);
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Policy from, in any combination (each can only switch things off):
/// - `policy.json` beside the hub's program file (where a deployment tool puts it);
/// - `%ProgramData%\<policy name>\policy.json`, used only when an administrator owns it and
///   no one else can change it (anyone may create folders in ProgramData);
/// - Group Policy values under `Software\Policies\<policy name>` (computer or user).
pub fn load_policy() -> Policy {
    let mut p = Policy::default();
    if let Some(dir) = exe_dir() {
        let f = dir.join("policy.json");
        if f.is_file() {
            match read_policy_file(&f) {
                Ok(pf) => p.apply(pf, format!("{}", f.display())),
                Err(e) => p.ignored.push(format!("{}: {e}", f.display())),
            }
        }
    }
    if let Some(pd) = std::env::var_os("ProgramData") {
        let dir = PathBuf::from(pd).join(&BRAND.policy_name);
        let f = dir.join("policy.json");
        if f.is_file() {
            match crate::trust::admin_only(&dir).and_then(|_| crate::trust::admin_only(&f)).and_then(|_| read_policy_file(&f)) {
                Ok(pf) => p.apply(pf, format!("{}", f.display())),
                Err(e) => p.ignored.push(format!("{}: {e}", f.display())),
            }
        }
    }
    for (pf, source) in registry_policy() {
        p.apply(pf, source);
    }
    p
}

fn read_policy_file(f: &Path) -> Result<PolicyFile, String> {
    let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("it could not be read ({e})"))
}

#[cfg(windows)]
fn registry_policy() -> Vec<(PolicyFile, String)> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    let key = format!(r"Software\Policies\{}", BRAND.policy_name);
    let mut out = vec![];
    for (hive, label) in [(HKEY_LOCAL_MACHINE, "Group Policy (computer)"), (HKEY_CURRENT_USER, "Group Policy (user)")] {
        let Ok(k) = winreg::RegKey::predef(hive).open_subkey_with_flags(&key, KEY_READ) else { continue };
        let flag = |name: &str| -> Option<bool> { k.get_value::<u32, _>(name).ok().map(|n| n != 0) };
        let hide: Vec<String> = k
            .get_value::<Vec<String>, _>("HideApps")
            .or_else(|_| k.get_value::<String, _>("HideApps").map(|s| s.split([',', ';']).map(str::to_string).collect()))
            .unwrap_or_default();
        let pf = PolicyFile { update_checks: flag("UpdateChecks"), start_with_windows: flag("StartWithWindows"), offer_previews: flag("OfferPreviews"), hide_apps: hide };
        out.push((pf, label.to_string()));
    }
    out
}

#[cfg(not(windows))]
fn registry_policy() -> Vec<(PolicyFile, String)> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_can_only_switch_things_off() {
        let mut p = Policy::default();
        p.apply(serde_json::from_str(r#"{"update_checks": true, "start_with_windows": false, "offer_previews": true, "hide_apps": ["PrismQDA", " "]}"#).unwrap(), "a".into());
        assert!(!p.no_update_checks);
        assert!(p.no_start_with_windows);
        assert!(!p.no_previews);
        assert_eq!(p.hidden_apps.iter().collect::<Vec<_>>(), vec!["prismqda"]);
        p.apply(serde_json::from_str(r#"{"start_with_windows": true}"#).unwrap(), "b".into());
        assert!(p.no_start_with_windows, "a later source cannot switch it back on");
    }

    #[test]
    fn settings_default_to_previews_on_and_start_up_off() {
        let s = Settings::default();
        assert!(s.offer_previews && !s.start_with_windows && !s.keep_in_tray && s.daily_check);
        let partial: Settings = serde_json::from_str(r#"{"keep_in_tray": true}"#).unwrap();
        assert!(partial.keep_in_tray && partial.offer_previews);
    }
}

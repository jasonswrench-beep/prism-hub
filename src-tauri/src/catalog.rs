//! The suite catalog: one signed JSON file that names every app, its installers and its web links.
//!
//! A catalog counts only when its signature checks out against the public key built into the
//! hub (brand.json). The cached copy is checked again every time it is read, so a file changed
//! on disk is ignored just like one changed on the way.

use crate::brand::BRAND;
use base64::Engine;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Download {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub release_page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extra {
    pub label: String,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mark {
    pub letters: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub mark: Mark,
    /// "desktop" or "coming-soon".
    pub section: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repository: String,
    /// The name Windows lists in installed apps (the uninstall entry's DisplayName).
    #[serde(default)]
    pub installed_as: String,
    /// The program's file name inside its install folder.
    #[serde(default)]
    pub program_file: String,
    /// The installer's switch for a no-questions install that shows only a progress bar.
    #[serde(default)]
    pub quiet_switch: String,
    #[serde(default = "default_uninstall_switch")]
    pub uninstall_switch: String,
    #[serde(default)]
    pub expected_signer: String,
    #[serde(default)]
    pub stable: Option<Download>,
    #[serde(default)]
    pub preview: Option<Download>,
    #[serde(default)]
    pub extras: Vec<Extra>,
    #[serde(default)]
    pub minimum_hub_version: Option<String>,
}

fn default_uninstall_switch() -> String {
    "/P".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    #[serde(default)]
    pub mark: Option<Mark>,
}

/// The part of the catalog every hub version must be able to read, whatever else changes:
/// it is what tells an old hub that it needs to update before it reads the rest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub format: u32,
    pub generated: String,
    #[serde(default)]
    pub minimum_hub_version: Option<String>,
    #[serde(default)]
    pub hub: Option<Download>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub format: u32,
    pub generated: String,
    #[serde(default)]
    pub minimum_hub_version: Option<String>,
    #[serde(default)]
    pub hub: Option<Download>,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
    #[serde(default)]
    pub web: Vec<WebEntry>,
}

/// What reading a catalog file produced.
#[derive(Debug, Clone)]
pub enum Read {
    Usable(Catalog),
    /// Signed and genuine, but made for a newer hub: only the envelope may be used.
    NeedsNewerHub(Envelope),
}

pub fn parse_version(s: &str) -> Option<Version> {
    Version::parse(s.trim().trim_start_matches(['v', 'V'])).ok()
}

pub fn hub_version() -> Version {
    parse_version(crate::brand::VERSION).unwrap_or_else(|| Version::new(0, 0, 0))
}

/// The hub's own built-in key, plus (debug builds only) a test key from the environment so the
/// signing path can be exercised without the real key.
fn public_keys() -> Vec<String> {
    let mut keys = vec![];
    if !BRAND.catalog_public_key.trim().is_empty() {
        keys.push(BRAND.catalog_public_key.trim().to_string());
    }
    #[cfg(debug_assertions)]
    if let Ok(k) = std::env::var("PRISM_HUB_TEST_PUBKEY") {
        keys.push(k.trim().to_string());
    }
    keys
}

fn b64_text(s: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|_| "it is not in the expected format".to_string())?;
    String::from_utf8(bytes).map_err(|_| "it is not in the expected format".to_string())
}

/// Check `data` against a signature file made by `tauri signer sign` (base64 of a minisign
/// signature) with any of the given public keys (base64 of a minisign public key file).
pub fn verify_with(data: &[u8], signature_file: &str, keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("this hub was built without its catalog key".into());
    }
    let sig_text = b64_text(signature_file)?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|_| "the signature file is damaged".to_string())?;
    for k in keys {
        let Ok(text) = b64_text(k) else { continue };
        let Ok(pk) = minisign_verify::PublicKey::decode(&text) else { continue };
        // Only prehashed (current minisign) signatures are accepted.
        if pk.verify(data, &sig, false).is_ok() {
            return Ok(());
        }
    }
    Err("its signature does not match the hub's key".into())
}

pub fn verify(data: &[u8], signature_file: &str) -> Result<(), String> {
    verify_with(data, signature_file, &public_keys())
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A plain file name: no folders, no drive, no parent references.
fn is_file_name(s: &str, ext: &str) -> bool {
    !s.is_empty() && s.len() < 128 && !s.contains(['/', '\\', ':', '"', '<', '>', '|', '?', '*']) && !s.starts_with('.') && s.to_ascii_lowercase().ends_with(ext)
}

fn is_switch(s: &str) -> bool {
    s.len() < 64 && s.split_whitespace().all(|w| w.starts_with('/') && w[1..].bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn check_download(d: &Download, what: &str) -> Result<(), String> {
    crate::net::check_download_url(&d.url).map_err(|e| format!("{what}: {e}"))?;
    if !is_sha256(&d.sha256) {
        return Err(format!("{what}: the SHA-256 is not 64 lowercase hex characters"));
    }
    if parse_version(&d.version).is_none() {
        return Err(format!("{what}: the version {:?} is not a version number", d.version));
    }
    if d.size == 0 || d.size > 2_000_000_000 {
        return Err(format!("{what}: the size is missing or too large"));
    }
    if !d.release_page.is_empty() {
        crate::net::check_page_url(&d.release_page).map_err(|e| format!("{what}: release page: {e}"))?;
    }
    Ok(())
}

/// Checks that go beyond the signature: a signed catalog still cannot point the hub at a file
/// on another site, pass a path as a program name, or hand an installer unexpected arguments.
pub fn validate(c: &Catalog) -> Result<(), String> {
    if c.format != FORMAT {
        return Err(format!("format {} is not one this hub reads", c.format));
    }
    if let Some(h) = &c.hub {
        check_download(h, "hub")?;
    }
    let mut seen = std::collections::HashSet::new();
    for a in &c.apps {
        let what = format!("app {:?}", a.id);
        if !is_id(&a.id) || a.id == crate::jobs::HUB || !seen.insert(a.id.clone()) {
            return Err(format!("{what}: the id is not valid or is used twice"));
        }
        if a.name.trim().is_empty() || a.name.len() > 80 {
            return Err(format!("{what}: the name is missing or too long"));
        }
        if !is_color(&a.mark.color) || a.mark.letters.chars().count() != 2 {
            return Err(format!("{what}: the mark needs two letters and a #rrggbb color"));
        }
        match a.section.as_str() {
            "coming-soon" => {}
            "desktop" => {
                if a.installed_as.trim().is_empty() {
                    return Err(format!("{what}: installed_as is missing"));
                }
                if !is_file_name(&a.program_file, ".exe") {
                    return Err(format!("{what}: program_file must be a plain .exe file name"));
                }
                if !is_switch(&a.quiet_switch) || !is_switch(&a.uninstall_switch) {
                    return Err(format!("{what}: an installer switch is not a plain /SWITCH"));
                }
                if a.expected_signer.trim().is_empty() {
                    return Err(format!("{what}: expected_signer is missing"));
                }
                for (d, label) in [(&a.stable, "stable"), (&a.preview, "preview")] {
                    if let Some(d) = d {
                        check_download(d, &format!("{what} {label}"))?;
                    }
                }
                for x in &a.extras {
                    crate::net::check_download_url(&x.url).map_err(|e| format!("{what} extra: {e}"))?;
                    if !is_sha256(&x.sha256) || !is_file_name(&x.file_name, "") || x.size == 0 {
                        return Err(format!("{what}: extra {:?} is incomplete", x.label));
                    }
                }
            }
            other => return Err(format!("{what}: unknown section {other:?}")),
        }
    }
    for w in &c.web {
        if !is_id(&w.id) {
            return Err(format!("web link {:?}: the id is not valid", w.id));
        }
        crate::net::check_page_url(&w.url).map_err(|e| format!("web link {:?}: {e}", w.id))?;
    }
    Ok(())
}

/// Verify, then read: first the envelope (which every hub understands), then the rest.
pub fn read(data: &[u8], signature_file: &str) -> Result<Read, String> {
    verify(data, signature_file)?;
    read_verified(data)
}

pub fn read_verified(data: &[u8]) -> Result<Read, String> {
    let env: Envelope = serde_json::from_slice(data).map_err(|e| format!("it could not be read ({e})"))?;
    if let Some(min) = env.minimum_hub_version.as_deref().and_then(parse_version) {
        if hub_version() < min {
            if let Some(h) = &env.hub {
                check_download(h, "hub")?;
            }
            return Ok(Read::NeedsNewerHub(env));
        }
    }
    let c: Catalog = serde_json::from_slice(data).map_err(|e| format!("it could not be read ({e})"))?;
    validate(&c)?;
    Ok(Read::Usable(c))
}

// ---------------------------------------------------------------------------------------------
// Which version to offer

#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub download: Download,
    pub preview: bool,
}

/// The release to offer for one app.
///
/// - Stable is offered unless previews are on and the preview is newer.
/// - An app with only a preview still has it offered (marked Preview), unless administrator
///   policy turned previews off altogether.
/// - Never a version older than the one installed: someone on a preview who turns previews off
///   keeps their version until a stable one passes it (`needs_update` decides that).
pub fn pick(app: &AppEntry, previews_on: bool, previews_allowed: bool) -> Option<Offer> {
    let stable = app.stable.clone().map(|d| Offer { download: d, preview: false });
    let preview = if previews_allowed { app.preview.clone().map(|d| Offer { download: d, preview: true }) } else { None };
    match (stable, preview) {
        (Some(s), Some(p)) if previews_on => {
            let (sv, pv) = (parse_version(&s.download.version), parse_version(&p.download.version));
            if pv > sv {
                Some(p)
            } else {
                Some(s)
            }
        }
        (Some(s), _) => Some(s),
        (None, Some(p)) => Some(p),
        (None, None) => None,
    }
}

/// Whether the offer is newer than what is installed. An installed version that cannot be read
/// is treated as needing the update only if Windows lists no version at all.
pub fn needs_update(installed: &str, offer: &Offer) -> bool {
    match (parse_version(installed), parse_version(&offer.download.version)) {
        (Some(i), Some(o)) => o > i && !same_release(&i, &o),
        (None, Some(_)) => installed.trim().is_empty(),
        _ => false,
    }
}

/// Some installers record "0.1.0" for the release "0.1.0-beta.1". Treat that as the same release
/// so a correct install is not reported as a mismatch or offered again.
pub fn same_release(installed: &Version, expected: &Version) -> bool {
    if installed == expected {
        return true;
    }
    installed.pre.is_empty() && installed.build.is_empty() && (installed.major, installed.minor, installed.patch) == (expected.major, expected.minor, expected.patch) && !expected.pre.is_empty()
}

pub fn installed_matches(installed: &str, expected: &str) -> bool {
    match (parse_version(installed), parse_version(expected)) {
        (Some(i), Some(e)) => same_release(&i, &e),
        _ => installed.trim() == expected.trim(),
    }
}

// ---------------------------------------------------------------------------------------------
// The cached copy on disk

pub fn cache_dir() -> PathBuf {
    BRAND.data_dir().join("catalog")
}

fn files_in(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(&BRAND.catalog_file), dir.join(format!("{}.sig", BRAND.catalog_file)))
}

/// The last good catalog. Falls back to the copy shipped beside the program (in `catalog\`), so a
/// first start without a network, or with update checks turned off, still lists the apps.
pub fn load_cached() -> Option<(Read, Vec<u8>)> {
    let mut dirs = vec![cache_dir()];
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("catalog"));
    }
    let mut best: Option<(Read, Vec<u8>)> = None;
    for d in dirs {
        let (json, sig) = files_in(&d);
        let (Ok(data), Ok(sig)) = (std::fs::read(&json), std::fs::read_to_string(&sig)) else { continue };
        if let Ok(r) = read(&data, &sig) {
            let newer = match &best {
                None => true,
                Some((b, _)) => generated(&r) > generated(b),
            };
            if newer {
                best = Some((r, data));
            }
        }
    }
    best
}

pub fn generated(r: &Read) -> &str {
    match r {
        Read::Usable(c) => &c.generated,
        Read::NeedsNewerHub(e) => &e.generated,
    }
}

pub fn store(data: &[u8], signature_file: &str) -> std::io::Result<()> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;
    let (json, sig) = files_in(&dir);
    crate::util::write_atomic(&json, data)?;
    crate::util::write_atomic(&sig, signature_file.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(v: &str) -> Download {
        Download { version: v.into(), url: "https://github.com/o/r/releases/download/v/x.exe".into(), sha256: "a".repeat(64), size: 10, release_page: String::new() }
    }

    fn app(stable: Option<&str>, preview: Option<&str>) -> AppEntry {
        AppEntry {
            id: "x".into(),
            name: "X".into(),
            mark: Mark { letters: "xx".into(), color: "#123456".into() },
            section: "desktop".into(),
            description: String::new(),
            repository: String::new(),
            installed_as: "X".into(),
            program_file: "x.exe".into(),
            quiet_switch: "/P".into(),
            uninstall_switch: "/P".into(),
            expected_signer: "Someone".into(),
            stable: stable.map(dl),
            preview: preview.map(dl),
            extras: vec![],
            minimum_hub_version: None,
        }
    }

    #[test]
    fn offers_stable_unless_previews_are_on_and_newer() {
        let a = app(Some("1.0.0"), Some("1.1.0-beta.1"));
        assert_eq!(pick(&a, false, true).unwrap().download.version, "1.0.0");
        let p = pick(&a, true, true).unwrap();
        assert!(p.preview);
        assert_eq!(p.download.version, "1.1.0-beta.1");
        // An older preview never beats the stable release.
        let b = app(Some("1.2.0"), Some("1.1.0-beta.1"));
        assert_eq!(pick(&b, true, true).unwrap().download.version, "1.2.0");
    }

    #[test]
    fn only_a_preview_is_still_offered_unless_policy_forbids() {
        let a = app(None, Some("0.1.0-beta.1"));
        assert!(pick(&a, false, true).unwrap().preview);
        assert!(pick(&a, false, false).is_none());
    }

    #[test]
    fn turning_previews_off_never_goes_backward() {
        let a = app(Some("1.0.0"), Some("1.1.0-beta.1"));
        let offer = pick(&a, false, true).unwrap();
        assert!(!needs_update("1.1.0-beta.1", &offer));
        let later = app(Some("1.1.0"), Some("1.1.0-beta.1"));
        assert!(needs_update("1.1.0-beta.1", &pick(&later, false, true).unwrap()));
    }

    #[test]
    fn installer_that_drops_the_prerelease_tag_counts_as_the_same_release() {
        assert!(installed_matches("0.1.0", "0.1.0-beta.1"));
        assert!(installed_matches("v1.2.0", "1.2.0"));
        assert!(!installed_matches("0.1.0", "0.1.1"));
        assert!(!installed_matches("0.1.0-beta.1", "0.1.0"));
        let offer = pick(&app(None, Some("0.1.0-beta.1")), true, true).unwrap();
        assert!(!needs_update("0.1.0", &offer));
        assert!(!needs_update("0.1.1", &offer));
        assert!(needs_update("0.0.9", &offer));
    }

    #[test]
    fn validation_refuses_paths_foreign_hosts_and_odd_switches() {
        let mut c = Catalog { format: 1, generated: "2026-11-15T00:00:00Z".into(), minimum_hub_version: None, hub: None, apps: vec![app(Some("1.0.0"), None)], web: vec![] };
        assert!(validate(&c).is_ok());
        c.apps[0].program_file = "..\\evil.exe".into();
        assert!(validate(&c).is_err());
        c.apps[0].program_file = "x.exe".into();
        c.apps[0].quiet_switch = "/P & calc".into();
        assert!(validate(&c).is_err());
        c.apps[0].quiet_switch = "/P".into();
        c.apps[0].stable.as_mut().unwrap().url = "https://example.com/x.exe".into();
        assert!(validate(&c).is_err());
        c.apps[0].stable.as_mut().unwrap().url = "http://github.com/o/r/releases/download/v/x.exe".into();
        assert!(validate(&c).is_err());
    }

    // A key pair made with `tauri signer generate` for these tests only (password "test").
    const TEST_PUB: &str = include_str!("../tests/test-key.pub");

    #[test]
    fn signature_checks_and_a_changed_byte_fails() {
        let data = include_bytes!("../tests/sample-catalog.json");
        let sig = include_str!("../tests/sample-catalog.json.sig");
        let keys = vec![TEST_PUB.trim().to_string()];
        assert!(verify_with(data, sig, &keys).is_ok());
        let mut changed = data.to_vec();
        let i = changed.iter().position(|&b| b == b'1').unwrap();
        changed[i] = b'2';
        assert!(verify_with(&changed, sig, &keys).is_err());
        assert!(verify_with(data, sig, &[]).is_err());
        // The sample is also a valid catalog.
        assert!(matches!(read_verified(data), Ok(Read::Usable(_))));
    }

    #[test]
    fn a_catalog_for_a_newer_hub_yields_only_the_envelope() {
        let data = br#"{"format":2,"generated":"2030-01-01T00:00:00Z","minimum_hub_version":"99.0.0","hub":{"version":"99.0.0","url":"https://github.com/o/r/releases/download/v99/h.exe","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":5},"apps":"something new"}"#;
        assert!(matches!(read_verified(data), Ok(Read::NeedsNewerHub(_))));
    }
}

// SPDX-License-Identifier: Apache-2.0
//! The suite catalog: one small signed file that says which apps exist, where each one's installer is, and its SHA-256. The hub reads it instead
//! of asking GitHub about each app (docs/catalog.md has the format and why). A catalog is used only when its signature checks against the public
//! half of the hub key built into the brand file, and every address and value in it is checked before anything is downloaded.

use crate::version;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The catalog format this hub reads. A catalog with a higher number also carries a minimumHubVersion this hub does not meet.
pub const FORMAT: u32 = 1;
/// The most a catalog may be. Today's is a few kilobytes.
pub const MOST_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub format: u32,
    /// When the catalog script wrote it (UTC, 2026-11-15T18:04:00Z). A catalog older than the one kept on disk is refused, so an old signed
    /// catalog cannot be replayed to offer older installers.
    pub generated: String,
    #[serde(default)]
    pub minimum_hub_version: Option<String>,
    #[serde(default)]
    pub hub: Option<Release>,
    #[serde(default)]
    pub apps: Vec<App>,
    #[serde(default)]
    pub web: Vec<WebLink>,
    #[serde(default)]
    pub coming_soon: Vec<ComingSoon>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    pub name: String,
    pub mark: Mark,
    #[serde(default)]
    pub description: String,
    /// owner/name on GitHub.
    pub repository: String,
    /// The name Windows lists in installed apps (DisplayName in the per-user uninstall list).
    pub installed_as: String,
    /// The program's file name inside its install folder.
    pub program_file: String,
    /// The installer's switches for an install that asks no questions but shows its own progress bar.
    pub quiet_switch: Vec<String>,
    /// The uninstaller's switches for an uninstall that asks no questions, when Windows' list has no QuietUninstallString.
    #[serde(default = "default_uninstall_switch")]
    pub quiet_uninstall_switch: Vec<String>,
    pub expected_signer: String,
    #[serde(default)]
    pub stable: Option<Release>,
    #[serde(default)]
    pub preview: Option<Release>,
    #[serde(default)]
    pub extras: Vec<Extra>,
    #[serde(default)]
    pub minimum_hub_version: Option<String>,
}

fn default_uninstall_switch() -> Vec<String> {
    vec!["/S".into()]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mark {
    pub letters: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    /// What Windows' installed apps list shows once this release is installed (0.2.0 for 0.2.0-alpha.4).
    pub windows_version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub release_page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Extra {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebLink {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComingSoon {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The app's own mark, when it has one already (an app still being made, such as OpenStage Studio).
    #[serde(default)]
    pub mark: Option<Mark>,
}

/// Checks the signature (Ed25519 over the exact bytes of the file; the signature file holds it in base64) with the built-in public key.
pub fn verify_signature(public_key_b64: &str, bytes: &[u8], signature_text: &str) -> Result<(), String> {
    let engine = base64::engine::general_purpose::STANDARD;
    let key: [u8; 32] = engine
        .decode(public_key_b64.trim())
        .ok()
        .and_then(|k| k.try_into().ok())
        .ok_or("This copy of the hub has no catalog key built in, so it cannot trust any catalog.")?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| "The catalog key built into this hub is not a valid key.")?;
    let sig: [u8; 64] = engine
        .decode(signature_text.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or("The catalog's signature file could not be read, so the catalog was not used.")?;
    key.verify_strict(bytes, &Signature::from_bytes(&sig))
        .map_err(|_| "The catalog's signature did not match, so the catalog was not used.".to_string())
}

/// Reads a catalog whose signature has already been checked, and checks every value the hub will act on.
pub fn parse(bytes: &[u8]) -> Result<Catalog, String> {
    if bytes.len() > MOST_BYTES {
        return Err("The catalog was larger than expected, so it was not used.".into());
    }
    let catalog: Catalog = serde_json::from_slice(bytes).map_err(|_| "The catalog could not be read.".to_string())?;
    validate(&catalog)?;
    Ok(catalog)
}

/// Every app's id, name and mark, every address and every checksum: an address must be on GitHub, in that app's own repository's releases.
pub fn validate(c: &Catalog) -> Result<(), String> {
    let bad = |what: String| Err(format!("The catalog has a problem ({what}), so it was not used."));
    if c.generated.len() != 20 || !c.generated.ends_with('Z') {
        return bad("its date".into());
    }
    if let Some(min) = &c.minimum_hub_version {
        if version::key(min).is_none() {
            return bad("its minimum hub version".into());
        }
    }
    if let Some(hub) = &c.hub {
        check_release(hub, &crate::brand::brand().repository).or_else(|w| bad(format!("the hub's own release: {w}")))?;
    }
    let mut ids = std::collections::HashSet::new();
    for app in &c.apps {
        if !plain_id(&app.id) || !ids.insert(app.id.clone()) {
            return bad(format!("app id {:?}", app.id));
        }
        let what = |w: &str| format!("{}: {w}", app.id);
        if app.name.trim().is_empty() || app.installed_as.trim().is_empty() {
            return bad(what("its name"));
        }
        if !plain_repository(&app.repository) {
            return bad(what("its repository"));
        }
        if !plain_program_file(&app.program_file) {
            return bad(what("its program file"));
        }
        if app.quiet_switch.is_empty() || !app.quiet_switch.iter().chain(&app.quiet_uninstall_switch).all(|s| plain_switch(s)) {
            return bad(what("its installer switches"));
        }
        if app.expected_signer.trim().is_empty() {
            return bad(what("its signer"));
        }
        if !(1..=2).contains(&app.mark.letters.chars().count()) || !app.mark.letters.chars().all(|ch| ch.is_alphanumeric()) || !hex_color(&app.mark.color) {
            return bad(what("its mark"));
        }
        if let Some(min) = &app.minimum_hub_version {
            if version::key(min).is_none() {
                return bad(what("its minimum hub version"));
            }
        }
        for r in app.stable.iter().chain(app.preview.iter()) {
            check_release(r, &app.repository).or_else(|w| bad(what(&w)))?;
        }
        if app.stable.as_ref().is_some_and(|s| version::is_preview(&s.version)) {
            return bad(what("its stable release is a preview"));
        }
        for x in &app.extras {
            if !in_releases(&x.url, &app.repository) || !sha256_text(&x.sha256) || !plain_file_name(&x.file_name) || x.size == 0 {
                return bad(what(&format!("the extra download {}", x.name)));
            }
        }
    }
    for s in &c.coming_soon {
        if !plain_id(&s.id) || s.mark.as_ref().is_some_and(|m| !hex_color(&m.color) || m.letters.chars().count() > 2) {
            return bad(format!("coming soon {:?}", s.id));
        }
    }
    for w in &c.web {
        if !plain_id(&w.id) || !w.url.starts_with("https://") || w.url.contains(char::is_whitespace) || w.url.len() > 400 {
            return bad(format!("web link {:?}", w.id));
        }
    }
    Ok(())
}

fn check_release(r: &Release, repository: &str) -> Result<(), String> {
    if version::key(&r.version).is_none() || version::key(&r.windows_version).is_none() {
        return Err("a version".into());
    }
    if !in_releases(&r.url, repository) {
        return Err("an installer address outside its repository's releases".into());
    }
    if !r.release_page.starts_with(&format!("https://github.com/{repository}/releases")) {
        return Err("a release page outside its repository".into());
    }
    if !sha256_text(&r.sha256) || r.size == 0 {
        return Err("a checksum or size".into());
    }
    Ok(())
}

/// https://github.com/<repository>/releases/download/<tag>/<file>, with nothing that could step out of that place.
pub fn in_releases(url: &str, repository: &str) -> bool {
    let Some(rest) = url.strip_prefix(&format!("https://github.com/{repository}/releases/download/")) else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.+%".contains(&b)))
}

fn plain_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn plain_repository(s: &str) -> bool {
    let mut parts = s.split('/');
    let ok = |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p != "." && p != ".." && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)));
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

fn plain_file_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 120 && !s.starts_with('.') && s.chars().all(|c| c.is_ascii_alphanumeric() || " -_.+()".contains(c))
}

fn plain_program_file(s: &str) -> bool {
    plain_file_name(s) && s.to_ascii_lowercase().ends_with(".exe")
}

fn plain_switch(s: &str) -> bool {
    s.len() >= 2 && s.len() <= 40 && s.starts_with('/') && s[1..].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn sha256_text(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Which release the hub offers for an app.
///
/// - Previews on: the newer of the stable and the preview release.
/// - Previews off: the stable release; an app that has only a preview still has it offered (marked Preview).
/// - Previews turned off by the computer's policy file: stable only, and nothing when there is no stable release.
pub fn offer(app: &App, previews: bool, previews_allowed: bool) -> Option<&Release> {
    let stable = app.stable.as_ref();
    let preview = app.preview.as_ref();
    if !previews_allowed {
        return stable;
    }
    match (stable, preview) {
        (Some(s), Some(p)) if previews && version::newer(&p.version, &s.version) => Some(p),
        (Some(s), _) => Some(s),
        (None, p) => p,
    }
}

/// Whether the hub may use this catalog: one that needs a newer hub is shown only as the hub's update banner.
pub fn needs_newer_hub(c: &Catalog, hub_version: &str) -> bool {
    c.format > FORMAT || c.minimum_hub_version.as_deref().is_some_and(|m| version::newer(m, hub_version))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn release(repo: &str, v: &str) -> Release {
        Release {
            version: v.into(),
            windows_version: v.split('-').next().unwrap().into(),
            url: format!("https://github.com/{repo}/releases/download/v{v}/Setup.exe"),
            sha256: "ab".repeat(32),
            size: 10,
            release_page: format!("https://github.com/{repo}/releases/tag/v{v}"),
        }
    }

    pub fn app(id: &str, stable: Option<&str>, preview: Option<&str>) -> App {
        let repo = format!("someone/{id}");
        App {
            id: id.into(),
            name: format!("App {id}"),
            mark: Mark { letters: "ab".into(), color: "#6B5BD2".into() },
            description: String::new(),
            repository: repo.clone(),
            installed_as: format!("App {id}"),
            program_file: format!("{id}.exe"),
            quiet_switch: vec!["/P".into()],
            quiet_uninstall_switch: vec!["/S".into()],
            expected_signer: "Jason Wrench".into(),
            stable: stable.map(|v| release(&repo, v)),
            preview: preview.map(|v| release(&repo, v)),
            extras: vec![],
            minimum_hub_version: None,
        }
    }

    pub fn catalog(apps: Vec<App>) -> Catalog {
        Catalog { format: 1, generated: "2026-11-15T18:04:00Z".into(), minimum_hub_version: None, hub: None, apps, web: vec![], coming_soon: vec![] }
    }

    #[test]
    fn previews_on_offers_the_newer_previews_off_offers_stable_unless_there_is_none() {
        let both = app("a", Some("0.1.1"), Some("0.2.0-alpha.4"));
        assert_eq!(offer(&both, true, true).unwrap().version, "0.2.0-alpha.4");
        assert_eq!(offer(&both, false, true).unwrap().version, "0.1.1");
        let older_preview = app("b", Some("0.2.0"), Some("0.2.0-alpha.4"));
        assert_eq!(offer(&older_preview, true, true).unwrap().version, "0.2.0", "a stable release that passed the preview wins");
        let only_preview = app("c", None, Some("0.1.0-alpha.5"));
        assert_eq!(offer(&only_preview, false, true).unwrap().version, "0.1.0-alpha.5", "still offered, marked Preview");
        assert!(offer(&only_preview, true, false).is_none(), "the computer's policy turned previews off");
        assert_eq!(offer(&both, true, false).unwrap().version, "0.1.1");
    }

    #[test]
    fn a_good_catalog_passes_and_each_kind_of_bad_value_is_refused() {
        let good = catalog(vec![app("openexplainer", Some("0.1.1"), Some("0.2.0-alpha.4"))]);
        assert_eq!(validate(&good), Ok(()));
        let bytes = serde_json::to_vec(&good).unwrap();
        assert_eq!(parse(&bytes).unwrap(), good);

        let spoil = |f: &dyn Fn(&mut App)| {
            let mut c = good.clone();
            f(&mut c.apps[0]);
            validate(&c).is_err()
        };
        assert!(spoil(&|a| a.stable.as_mut().unwrap().url = "https://example.org/Setup.exe".into()));
        assert!(spoil(&|a| a.stable.as_mut().unwrap().url = "https://github.com/someone/other/releases/download/v1/Setup.exe".into()));
        assert!(spoil(&|a| a.stable.as_mut().unwrap().url = "https://github.com/someone/openexplainer/releases/download/v1/../x/Setup.exe".into()));
        assert!(spoil(&|a| a.stable.as_mut().unwrap().url = "http://github.com/someone/openexplainer/releases/download/v1/Setup.exe".into()));
        assert!(spoil(&|a| a.stable.as_mut().unwrap().sha256 = "xyz".into()));
        assert!(spoil(&|a| a.stable.as_mut().unwrap().version = "0.2.0-alpha.1".into()), "a stable release must not be a preview");
        assert!(spoil(&|a| a.program_file = "..\\cmd.exe".into()));
        assert!(spoil(&|a| a.program_file = "notes.txt".into()));
        assert!(spoil(&|a| a.quiet_switch = vec!["/D=C:\\Windows".into()]));
        assert!(spoil(&|a| a.quiet_switch = vec!["& calc".into()]));
        assert!(spoil(&|a| a.quiet_switch = vec![]));
        assert!(spoil(&|a| a.id = "Bad Id".into()));
        assert!(spoil(&|a| a.mark.color = "red".into()));
        assert!(spoil(&|a| a.repository = "a/b/c".into()));
        let mut twice = good.clone();
        twice.apps.push(twice.apps[0].clone());
        assert!(validate(&twice).is_err(), "two apps with one id");
        let mut web = good.clone();
        web.web.push(WebLink { id: "site".into(), name: "Site".into(), description: String::new(), url: "javascript:alert(1)".into() });
        assert!(validate(&web).is_err());
    }

    #[test]
    fn only_a_catalog_signed_with_the_built_in_key_is_trusted() {
        use ed25519_dalek::{Signer, SigningKey};
        let engine = base64::engine::general_purpose::STANDARD;
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public = engine.encode(signing.verifying_key().to_bytes());
        let bytes = br#"{"format":1}"#.to_vec();
        let sig = engine.encode(signing.sign(&bytes).to_bytes());
        assert_eq!(verify_signature(&public, &bytes, &sig), Ok(()));
        assert_eq!(verify_signature(&public, &bytes, &format!("{sig}\r\n")), Ok(()), "a line ending after the signature is fine");
        let mut changed = bytes.clone();
        changed[3] ^= 1;
        assert!(verify_signature(&public, &changed, &sig).is_err(), "one changed byte");
        let other = engine.encode(SigningKey::from_bytes(&[8u8; 32]).verifying_key().to_bytes());
        assert!(verify_signature(&other, &bytes, &sig).is_err(), "another key");
        assert!(verify_signature("", &bytes, &sig).unwrap_err().contains("no catalog key"));
        assert!(verify_signature(&public, &bytes, "nonsense").is_err());
    }

    #[test]
    fn a_catalog_that_needs_a_newer_hub_is_recognised() {
        let mut c = catalog(vec![]);
        assert!(!needs_newer_hub(&c, "1.0.0"));
        c.minimum_hub_version = Some("1.1.0".into());
        assert!(needs_newer_hub(&c, "1.0.0"));
        assert!(!needs_newer_hub(&c, "1.1.0"));
        c.minimum_hub_version = None;
        c.format = 2;
        assert!(needs_newer_hub(&c, "9.0.0"));
    }
}

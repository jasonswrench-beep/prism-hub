// SPDX-License-Identifier: Apache-2.0
//! The hub's model: the last good catalog, this computer's installed apps, the settings and the policy, and from them what each row shows.

use crate::brand::{brand, HUB_VERSION};
use crate::catalog::{self, App, Catalog, ComingSoon, Release, WebLink};
use crate::state::{self, Policy, Saved, Settings};
use crate::system::{self, Installed};
use crate::version;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// How the last attempt to fetch the catalog went.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Fetch {
    /// Not tried yet in this run.
    NotYet,
    Fresh,
    Offline,
    /// The policy file turned update checks off.
    Off,
    /// GitHub answered, but the catalog was refused (its signature, its contents, or an older date than the one on disk).
    Refused { message: String },
}

pub struct Hub {
    pub saved: Mutex<Saved>,
    pub policy: Policy,
    pub catalog: Mutex<Option<Catalog>>,
    pub fetch: Mutex<Fetch>,
    /// The apps (by id, or "hub") with a job running, each with its Cancel flag.
    pub jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Started with --tray (by Windows at sign-in).
    pub started_in_tray: bool,
    /// The hub was updated since it last ran: "Updated to 1.1.0" is shown once.
    pub just_updated: Mutex<Option<String>>,
}

impl Hub {
    pub fn new(started_in_tray: bool) -> Hub {
        let mut saved = Saved::load();
        let just_updated = match &saved.last_hub_version {
            Some(last) if version::newer(HUB_VERSION, last) => Some(HUB_VERSION.to_string()),
            _ => None,
        };
        saved.last_hub_version = Some(HUB_VERSION.to_string());
        let _ = saved.save();
        let _ = clear_downloads();
        Hub {
            saved: Mutex::new(saved),
            policy: Policy::read(),
            catalog: Mutex::new(load_cached()),
            fetch: Mutex::new(Fetch::NotYet),
            jobs: Mutex::new(HashMap::new()),
            started_in_tray,
            just_updated: Mutex::new(just_updated),
        }
    }

    pub fn settings(&self) -> Settings {
        self.saved.lock().unwrap().settings.clone()
    }

    pub fn app(&self, id: &str) -> Option<App> {
        self.catalog.lock().unwrap().as_ref()?.apps.iter().find(|a| a.id == id && !self.policy.hides(&a.id)).cloned()
    }

    /// The release the hub would install now for this app.
    pub fn offer_for(&self, app: &App) -> Option<Release> {
        catalog::offer(app, self.settings().offer_previews, self.policy.previews_allowed()).cloned()
    }

    /// Fetches the catalog from GitHub, checks it and keeps it. Does nothing (and says so) when the policy file turned checks off.
    pub fn refresh(&self) -> Fetch {
        let outcome = if !self.policy.checks_allowed() {
            Fetch::Off
        } else {
            match fetch_catalog() {
                Ok((bytes, sig, fresh)) => {
                    let older = self.catalog.lock().unwrap().as_ref().is_some_and(|kept| fresh.generated < kept.generated);
                    if older {
                        Fetch::Refused { message: "The catalog on GitHub is older than the one already on this computer, so it was not used.".into() }
                    } else {
                        let _ = keep(&bytes, &sig);
                        *self.catalog.lock().unwrap() = Some(fresh);
                        let mut saved = self.saved.lock().unwrap();
                        saved.last_check = Some(state::now());
                        let _ = saved.save();
                        Fetch::Fresh
                    }
                }
                Err(f) => f,
            }
        };
        *self.fetch.lock().unwrap() = outcome.clone();
        outcome
    }

    pub fn view(&self) -> View {
        let catalog = self.catalog.lock().unwrap().clone();
        let saved = self.saved.lock().unwrap().clone();
        let installed = system::installed_apps();
        let jobs: Vec<String> = self.jobs.lock().unwrap().keys().cloned().collect();
        let needs_newer_hub = catalog.as_ref().is_some_and(|c| catalog::needs_newer_hub(c, HUB_VERSION));
        let previews_allowed = self.policy.previews_allowed();

        let mut apps = Vec::new();
        let (mut web, mut coming_soon) = (Vec::new(), Vec::new());
        if let Some(c) = &catalog {
            for app in c.apps.iter().filter(|a| !self.policy.hides(&a.id)) {
                apps.push(app_view(app, &installed, &saved, previews_allowed));
            }
            web = c.web.clone();
            coming_soon = c.coming_soon.clone();
        }
        let hub_update = catalog
            .as_ref()
            .and_then(|c| c.hub.clone())
            .filter(|h| version::newer(&h.version, HUB_VERSION))
            .map(|h| HubUpdate { version: h.version, size: h.size });
        let updates_ready = apps.iter().filter(|a| a.update).count();
        View {
            hub_version: HUB_VERSION.to_string(),
            catalog_date: catalog.as_ref().map(|c| c.generated.clone()),
            fetch: self.fetch.lock().unwrap().clone(),
            last_check: saved.last_check,
            key_missing: public_key().trim().is_empty(),
            needs_newer_hub,
            hub_update,
            just_updated: self.just_updated.lock().unwrap().clone(),
            apps,
            web,
            coming_soon,
            updates_ready,
            settings: saved.settings.clone(),
            start_with_windows: system::starts_with_windows(&brand().product_name),
            policy: PolicyView {
                checks: self.policy.checks_allowed(),
                start_with_windows: self.policy.start_allowed(),
                previews: previews_allowed,
            },
            jobs,
            repository: brand().repository_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub hub_version: String,
    pub catalog_date: Option<String>,
    pub fetch: Fetch,
    pub last_check: Option<u64>,
    /// This copy was built before the hub key existed, so it can trust no catalog.
    pub key_missing: bool,
    pub needs_newer_hub: bool,
    pub hub_update: Option<HubUpdate>,
    pub just_updated: Option<String>,
    pub apps: Vec<AppView>,
    pub web: Vec<WebLink>,
    pub coming_soon: Vec<ComingSoon>,
    pub updates_ready: usize,
    pub settings: Settings,
    pub start_with_windows: bool,
    pub policy: PolicyView,
    pub jobs: Vec<String>,
    pub repository: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubUpdate {
    pub version: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub checks: bool,
    pub start_with_windows: bool,
    pub previews: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    pub id: String,
    pub name: String,
    pub letters: String,
    pub color: String,
    /// The app's own icon (a data: address from the signed catalog), shown in place of the round mark when there is one.
    pub icon: Option<String>,
    pub description: String,
    pub installed: Option<InstalledView>,
    pub offer: Option<OfferView>,
    /// A newer release than the installed one is offered.
    pub update: bool,
    pub running: bool,
    /// This app's entry needs a newer hub.
    pub needs_newer_hub: bool,
    /// An app with only preview releases, on a computer whose policy turned previews off.
    pub preview_blocked: bool,
    pub extras: Vec<ExtraView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledView {
    /// The installed version: the exact release when the hub installed it, otherwise what Windows lists (the three numbers only).
    pub version: String,
    /// Whether `version` is the exact release (0.2.0-alpha.4) or only what Windows lists (0.2.0).
    pub exact: bool,
    pub folder_known: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OfferView {
    pub version: String,
    pub preview: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtraView {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub size: u64,
}

/// The installed version of an app as the hub knows it: (version, exact).
pub fn installed_version(entry: &Installed, record: Option<&state::Record>) -> (String, bool) {
    match record {
        Some(r) if r.windows_version.trim() == entry.display_version.trim() => (r.version.clone(), true),
        _ => (entry.display_version.trim().to_string(), false),
    }
}

/// Whether `offer` is newer than what is installed. When only Windows' three numbers are known, only a newer three numbers counts: an app
/// installed before the hub, at the same numbers, is taken to be current (its menu still offers Reinstall).
pub fn is_update(offer: &Release, installed: &str, exact: bool) -> bool {
    if exact {
        version::newer(&offer.version, installed)
    } else {
        version::newer(&offer.windows_version, installed)
    }
}

pub fn app_view(app: &App, installed: &[Installed], saved: &Saved, previews_allowed: bool) -> AppView {
    let offer = catalog::offer(app, saved.settings.offer_previews, previews_allowed);
    let entry = system::find_installed(installed, &app.installed_as);
    let installed_view = entry.map(|e| {
        let (version, exact) = installed_version(e, saved.installed.get(&app.id));
        InstalledView { version, exact, folder_known: e.folder().is_some() }
    });
    let update = match (offer, &installed_view) {
        (Some(o), Some(i)) => is_update(o, &i.version, i.exact),
        _ => false,
    };
    let running = entry.and_then(|e| e.folder()).is_some_and(|f| system::is_running(&f, &app.program_file));
    AppView {
        id: app.id.clone(),
        name: app.name.clone(),
        letters: app.mark.letters.clone(),
        color: app.mark.color.clone(),
        icon: app.mark.icon.clone(),
        description: app.description.clone(),
        installed: installed_view,
        offer: offer.map(|o| OfferView { version: o.version.clone(), preview: version::is_preview(&o.version), size: o.size }),
        update,
        running,
        needs_newer_hub: app.minimum_hub_version.as_deref().is_some_and(|m| version::newer(m, HUB_VERSION)),
        preview_blocked: !previews_allowed && app.stable.is_none() && app.preview.is_some(),
        extras: app
            .extras
            .iter()
            .enumerate()
            .map(|(index, x)| ExtraView { index, name: x.name.clone(), description: x.description.clone(), size: x.size })
            .collect(),
    }
}

/// The public half of the hub key. A development build can be given a test key in HUB_TEST_KEY; a release build cannot.
pub fn public_key() -> String {
    #[cfg(debug_assertions)]
    if let Ok(k) = std::env::var("HUB_TEST_KEY") {
        return k;
    }
    brand().catalog_public_key.clone()
}

fn check(bytes: &[u8], sig: &[u8]) -> Result<Catalog, String> {
    catalog::verify_signature(&public_key(), bytes, &String::from_utf8_lossy(sig))?;
    catalog::parse(bytes)
}

/// The last good catalog on disk, checked again (a file changed on disk is refused like one from the network).
pub fn load_cached() -> Option<Catalog> {
    let dir = state::catalog_dir();
    let b = brand();
    let bytes = std::fs::read(dir.join(&b.catalog.file)).ok()?;
    let sig = std::fs::read(dir.join(&b.catalog.signature)).ok()?;
    check(&bytes, &sig).ok()
}

fn keep(bytes: &[u8], sig: &[u8]) -> Result<(), String> {
    let dir = state::catalog_dir();
    let b = brand();
    state::write_atomic(&dir.join(&b.catalog.file), bytes)?;
    state::write_atomic(&dir.join(&b.catalog.signature), sig)
}

/// A development build can read the catalog from a folder instead of GitHub (HUB_CATALOG_DIR), to try a catalog before it is published.
fn local_catalog_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("HUB_CATALOG_DIR") {
        return Some(PathBuf::from(d));
    }
    None
}

fn fetch_catalog() -> Result<(Vec<u8>, Vec<u8>, Catalog), Fetch> {
    let b = brand();
    let (bytes, sig) = if let Some(dir) = local_catalog_dir() {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|_| Fetch::Offline);
        (read(&b.catalog.file)?, read(&b.catalog.signature)?)
    } else {
        let get = |url: String, most: usize| {
            crate::net::get_bytes(&url, most).map_err(|e| match e {
                crate::net::NetError::Offline | crate::net::NetError::Broken => Fetch::Offline,
                other => Fetch::Refused { message: other.words("catalog") },
            })
        };
        (get(b.catalog_url(), catalog::MOST_BYTES)?, get(b.signature_url(), 4096)?)
    };
    let fresh = check(&bytes, &sig).map_err(|message| Fetch::Refused { message })?;
    Ok((bytes, sig, fresh))
}

/// Anything left in the download folder from a run that ended part way.
pub fn clear_downloads() -> std::io::Result<()> {
    let dir = state::downloads_dir();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let _ = std::fs::remove_file(entry?.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::tests::app;
    use crate::state::Record;

    fn entry(name: &str, version: &str) -> Installed {
        Installed { display_name: name.into(), display_version: version.into(), install_location: Some(r"C:\nowhere".into()), ..Default::default() }
    }

    #[test]
    fn not_installed_shows_install_with_the_offer() {
        let v = app_view(&app("x", None, Some("0.1.0-alpha.5")), &[], &Saved::default(), true);
        assert!(v.installed.is_none());
        assert!(!v.update);
        assert_eq!(v.offer.as_ref().unwrap().version, "0.1.0-alpha.5");
        assert!(v.offer.unwrap().preview);
    }

    #[test]
    fn the_exact_release_is_known_when_the_hub_installed_it() {
        let a = app("x", None, Some("0.2.0-alpha.5"));
        let mut saved = Saved::default();
        saved.installed.insert("x".into(), Record { version: "0.2.0-alpha.4".into(), windows_version: "0.2.0".into() });
        let v = app_view(&a, &[entry("App x", "0.2.0")], &saved, true);
        assert_eq!(v.installed.as_ref().unwrap().version, "0.2.0-alpha.4");
        assert!(v.installed.unwrap().exact);
        assert!(v.update, "alpha 5 is newer than alpha 4");

        saved.installed.insert("x".into(), Record { version: "0.2.0-alpha.5".into(), windows_version: "0.2.0".into() });
        assert!(!app_view(&a, &[entry("App x", "0.2.0")], &saved, true).update);
    }

    #[test]
    fn an_app_installed_before_the_hub_compares_by_windows_numbers_only() {
        let a = app("x", None, Some("0.2.0-alpha.5"));
        let v = app_view(&a, &[entry("App x", "0.2.0")], &Saved::default(), true);
        assert_eq!(v.installed.as_ref().unwrap().version, "0.2.0");
        assert!(!v.installed.unwrap().exact);
        assert!(!v.update, "same numbers: taken as current");
        assert!(app_view(&a, &[entry("App x", "0.1.0")], &Saved::default(), true).update, "older numbers: an update");
        // A record that no longer matches what Windows lists (the app was updated some other way) is not trusted.
        let mut saved = Saved::default();
        saved.installed.insert("x".into(), Record { version: "0.1.0-alpha.9".into(), windows_version: "0.1.0".into() });
        let v = app_view(&a, &[entry("App x", "0.2.0")], &saved, true);
        assert_eq!(v.installed.unwrap().version, "0.2.0");
    }

    #[test]
    fn turning_previews_off_never_pushes_someone_backward() {
        let a = app("x", Some("0.1.1"), Some("0.2.0-alpha.4"));
        let mut saved = Saved::default();
        saved.settings.offer_previews = false;
        saved.installed.insert("x".into(), Record { version: "0.2.0-alpha.4".into(), windows_version: "0.2.0".into() });
        let v = app_view(&a, &[entry("App x", "0.2.0")], &saved, true);
        assert_eq!(v.offer.unwrap().version, "0.1.1");
        assert!(!v.update, "0.1.1 is older than the installed alpha, so nothing is offered until a stable release passes it");
    }

    #[test]
    fn the_policy_can_block_a_preview_only_app() {
        let v = app_view(&app("x", None, Some("0.1.0-alpha.5")), &[], &Saved::default(), false);
        assert!(v.offer.is_none());
        assert!(v.preview_blocked);
    }
}

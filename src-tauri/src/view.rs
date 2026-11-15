//! What the window shows, worked out in one place from the catalog, Windows' installed apps
//! list, the running jobs and the settings.

use crate::brand::{BRAND, VERSION};
use crate::catalog::{self, AppEntry, Download};
use crate::jobs::{Job, JobView};
use crate::winsys::{self, Installed};
use crate::Inner;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Clone)]
pub struct OfferView {
    pub version: String,
    pub preview: bool,
    pub size: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct ExtraView {
    pub label: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppRow {
    pub id: String,
    pub name: String,
    pub letters: String,
    pub color: String,
    pub description: String,
    /// not_installed, current, update, busy, problem, unavailable
    pub state: &'static str,
    pub installed_version: Option<String>,
    pub offer: Option<OfferView>,
    /// Why Install or Update cannot be pressed right now ("Offline", ...).
    pub blocked: Option<String>,
    /// Why there is nothing to install (state "unavailable").
    pub unavailable: Option<String>,
    pub only_preview: bool,
    pub job: Option<JobView>,
    pub problem: Option<String>,
    pub running: bool,
    pub can_open: bool,
    pub can_reinstall: bool,
    pub can_uninstall: bool,
    pub can_folder: bool,
    pub whats_new: bool,
    pub extras: Vec<ExtraView>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ComingRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub letters: String,
    pub color: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct WebRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub letters: String,
    pub color: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct Status {
    pub checking: bool,
    pub offline: bool,
    pub last_check: Option<i64>,
    pub updates_ready: usize,
    pub checks_disabled: bool,
    pub check_error: Option<String>,
    pub catalog_date: Option<String>,
    pub no_catalog: bool,
    pub hub_update: Option<String>,
    pub hub_required: bool,
    pub hub_job: Option<JobView>,
    pub hub_problem: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SettingsView {
    pub offer_previews: bool,
    pub start_with_windows: bool,
    pub keep_in_tray: bool,
    pub daily_check: bool,
    pub previews_locked: bool,
    pub start_locked: bool,
    pub checks_locked: bool,
    pub policy_sources: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BrandView {
    pub product_name: String,
    pub suite_name: String,
    pub accent: String,
    pub accent_dark: String,
    pub sibling_label: String,
    pub repository_url: String,
    pub version: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct View {
    pub brand: BrandView,
    pub apps: Vec<AppRow>,
    pub coming: Vec<ComingRow>,
    pub web: Vec<WebRow>,
    pub status: Status,
    pub settings: SettingsView,
    pub first_run: bool,
    pub tray: bool,
}

/// The installed copy of an app, if Windows lists it.
pub fn installed_for<'a>(app: &AppEntry, installed: &'a HashMap<String, Installed>) -> Option<&'a Installed> {
    installed.get(app.installed_as.trim())
}

pub fn program_path(app: &AppEntry, inst: &Installed) -> Option<PathBuf> {
    inst.location.as_ref().map(|l| l.join(&app.program_file))
}

/// The catalog release matching the installed version (for Reinstall).
pub fn matching_release(app: &AppEntry, installed_version: &str) -> Option<Download> {
    [&app.stable, &app.preview].into_iter().flatten().find(|d| catalog::installed_matches(installed_version, &d.version)).cloned()
}

pub fn app_needs_newer_hub(app: &AppEntry) -> bool {
    app.minimum_hub_version.as_deref().and_then(catalog::parse_version).map(|m| catalog::hub_version() < m).unwrap_or(false)
}

/// The offer for an app under the current settings and policy.
pub fn offer_for(g: &Inner, app: &AppEntry) -> Option<catalog::Offer> {
    catalog::pick(app, g.settings.offer_previews && !g.policy.no_previews, !g.policy.no_previews)
}

/// Apps with an update ready, as (id, name, version), for the status line, the tray and notifications.
pub fn updates_ready(g: &Inner, installed: &HashMap<String, Installed>) -> Vec<(String, String, String)> {
    let Some(c) = &g.catalog else { return vec![] };
    c.apps
        .iter()
        .filter(|a| a.section == "desktop" && !g.policy.hidden_apps.contains(&a.id) && !app_needs_newer_hub(a))
        .filter_map(|a| {
            let inst = installed_for(a, installed)?;
            let o = offer_for(g, a)?;
            catalog::needs_update(&inst.version, &o).then(|| (a.id.clone(), a.name.clone(), o.download.version.clone()))
        })
        .collect()
}

pub fn hub_update(g: &Inner) -> Option<Download> {
    let h = g.newer_hub_needed.as_ref().and_then(|e| e.hub.clone()).or_else(|| g.catalog.as_ref().and_then(|c| c.hub.clone()))?;
    let v = catalog::parse_version(&h.version)?;
    (v > catalog::hub_version()).then_some(h)
}

fn job_view(j: Option<&Job>) -> Option<JobView> {
    j.map(Job::view)
}

pub fn build(g: &Inner, installed: &HashMap<String, Installed>, running: &[PathBuf]) -> View {
    let hub_required = g.newer_hub_needed.is_some();
    let mut apps = vec![];
    let mut coming = vec![];
    let mut web = vec![];
    if let Some(c) = &g.catalog {
        for a in &c.apps {
            if g.policy.hidden_apps.contains(&a.id) {
                continue;
            }
            if a.section == "coming-soon" {
                coming.push(ComingRow { id: a.id.clone(), name: a.name.clone(), description: a.description.clone(), letters: a.mark.letters.clone(), color: a.mark.color.clone() });
                continue;
            }
            let inst = installed_for(a, installed);
            let program = inst.and_then(|i| program_path(a, i));
            let is_running = program.as_ref().map(|p| running.iter().any(|r| winsys::same_path(r, p))).unwrap_or(false);
            let offer = offer_for(g, a);
            let needs_hub = hub_required || app_needs_newer_hub(a);
            let blocked = if needs_hub {
                Some(format!("Update {} first", BRAND.product_name))
            } else if g.offline {
                Some("Offline".to_string())
            } else {
                None
            };
            let job = g.jobs.get(&a.id);
            let problem = g.problems.get(&a.id).cloned();
            let update = match (inst, &offer) {
                (Some(i), Some(o)) => catalog::needs_update(&i.version, o),
                _ => false,
            };
            let unavailable = if offer.is_none() {
                Some(if g.policy.no_previews && a.preview.is_some() { "Only a preview is available, and previews are turned off by your organization".to_string() } else { "No release yet".to_string() })
            } else {
                None
            };
            let state = if job.is_some() {
                "busy"
            } else if problem.is_some() {
                "problem"
            } else if inst.is_some() {
                if update {
                    "update"
                } else {
                    "current"
                }
            } else if offer.is_some() {
                "not_installed"
            } else {
                "unavailable"
            };
            apps.push(AppRow {
                id: a.id.clone(),
                name: a.name.clone(),
                letters: a.mark.letters.clone(),
                color: a.mark.color.clone(),
                description: a.description.clone(),
                state,
                installed_version: inst.map(|i| i.version.clone()),
                offer: offer.as_ref().map(|o| OfferView { version: o.download.version.clone(), preview: o.preview, size: o.download.size }),
                blocked,
                unavailable,
                only_preview: a.stable.is_none() && a.preview.is_some(),
                job: job_view(job),
                problem,
                running: is_running,
                can_open: program.as_ref().map(|p| p.is_file()).unwrap_or(false),
                can_reinstall: inst.map(|i| matching_release(a, &i.version).is_some()).unwrap_or(false),
                can_uninstall: inst.and_then(|i| i.uninstall.as_deref()).and_then(winsys::uninstaller_path).is_some(),
                can_folder: inst.and_then(|i| i.location.as_ref()).map(|l| l.is_dir()).unwrap_or(false),
                whats_new: offer.as_ref().map(|o| !o.download.release_page.is_empty()).unwrap_or(false),
                extras: a.extras.iter().map(|x| ExtraView { label: x.label.clone(), size: x.size }).collect(),
            });
        }
        for w in &c.web {
            let (letters, color) = w.mark.as_ref().map(|m| (m.letters.clone(), m.color.clone())).unwrap_or_else(|| (w.name.chars().take(2).collect::<String>().to_lowercase(), BRAND.accent.clone()));
            web.push(WebRow { id: w.id.clone(), name: w.name.clone(), description: w.description.clone(), letters, color });
        }
    }
    let ready = updates_ready(g, installed).len();
    let hub_dl = hub_update(g);
    View {
        brand: BrandView {
            product_name: BRAND.product_name.clone(),
            suite_name: BRAND.suite_name.clone(),
            accent: BRAND.accent.clone(),
            accent_dark: BRAND.accent_dark.clone(),
            sibling_label: BRAND.sibling_label.clone(),
            repository_url: BRAND.repository_url(),
            version: VERSION.to_string(),
        },
        apps,
        coming,
        web,
        status: Status {
            checking: g.checking,
            offline: g.offline,
            last_check: g.settings.last_check,
            updates_ready: ready,
            checks_disabled: g.policy.no_update_checks,
            check_error: g.check_error.clone(),
            catalog_date: g.catalog.as_ref().map(|c| c.generated.clone()),
            no_catalog: g.catalog.is_none(),
            hub_update: hub_dl.map(|d| d.version),
            hub_required,
            hub_job: job_view(g.jobs.get(crate::jobs::HUB)),
            hub_problem: g.problems.get(crate::jobs::HUB).cloned(),
        },
        settings: SettingsView {
            offer_previews: g.settings.offer_previews && !g.policy.no_previews,
            start_with_windows: g.settings.start_with_windows && !g.policy.no_start_with_windows,
            keep_in_tray: g.settings.keep_in_tray,
            daily_check: g.settings.daily_check && !g.policy.no_update_checks,
            previews_locked: g.policy.no_previews,
            start_locked: g.policy.no_start_with_windows,
            checks_locked: g.policy.no_update_checks,
            policy_sources: g.policy.sources.clone(),
        },
        first_run: !g.settings.first_run_done && !g.policy.no_start_with_windows,
        tray: g.tray,
    }
}

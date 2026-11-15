//! What the window can ask for. Every command names an app by its catalog id; addresses and
//! paths always come from the signed catalog or Windows, never from the window.

use crate::catalog::AppEntry;
use crate::jobs::{self, Kind};
use crate::view::{self, View};
use crate::{changed, hub, settings, winsys};
use tauri::AppHandle;

fn current_view(app: &AppHandle) -> View {
    let installed = winsys::installed_apps();
    let running = winsys::running_programs();
    let h = hub(app);
    let g = h.lock();
    view::build(&g, &installed, &running)
}

fn entry(app: &AppHandle, id: &str) -> Result<AppEntry, String> {
    let h = hub(app);
    let g = h.lock();
    if g.policy.hidden_apps.contains(id) {
        return Err("This app is hidden by your organization.".into());
    }
    g.catalog.as_ref().and_then(|c| c.apps.iter().find(|a| a.id == id && a.section == "desktop")).cloned().ok_or_else(|| "This app is not in the catalog.".into())
}

#[tauri::command]
pub fn get_view(app: AppHandle) -> View {
    current_view(&app)
}

#[tauri::command]
pub fn check_now(app: AppHandle) {
    crate::check(&app);
}

/// Install or update to the offered version.
#[tauri::command]
pub fn install(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let (offer, blocked) = {
        let h = hub(&app);
        let g = h.lock();
        (view::offer_for(&g, &e), g.newer_hub_needed.is_some() || view::app_needs_newer_hub(&e))
    };
    if blocked {
        return Err("Update the hub before installing this app.".into());
    }
    let offer = offer.ok_or("There is no release of this app to install.")?;
    let kind = if view::installed_for(&e, &winsys::installed_apps()).is_some() { Kind::Update } else { Kind::Install };
    jobs::start_install(&app, e, offer.download, kind)
}

/// Install the installed version again (only when the catalog lists that version).
#[tauri::command]
pub fn reinstall(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let installed = winsys::installed_apps();
    let inst = view::installed_for(&e, &installed).ok_or("This app is not installed.")?;
    let d = view::matching_release(&e, &inst.version).ok_or("The catalog no longer lists the installed version.")?;
    jobs::start_install(&app, e, d, Kind::Reinstall)
}

#[tauri::command]
pub fn cancel(app: AppHandle, id: String) {
    jobs::cancel(&app, &id);
}

#[tauri::command]
pub fn dismiss_problem(app: AppHandle, id: String) {
    hub(&app).lock().problems.remove(&id);
    changed(&app);
}

/// Open never waits on the network: it runs the program file from the folder Windows recorded.
#[tauri::command]
pub fn open_app(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let installed = winsys::installed_apps();
    let inst = view::installed_for(&e, &installed).ok_or("This app is not installed.")?;
    let program = view::program_path(&e, inst).ok_or("Windows did not record where this app is installed.")?;
    winsys::start_program(&program)
}

#[tauri::command]
pub fn open_folder(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let installed = winsys::installed_apps();
    let dir = view::installed_for(&e, &installed).and_then(|i| i.location.clone()).ok_or("Windows did not record where this app is installed.")?;
    if !dir.is_dir() {
        return Err("The install folder is missing.".into());
    }
    winsys::shell_open(&dir.to_string_lossy())
}

/// The release page of the offered version, in the person's own browser.
#[tauri::command]
pub fn whats_new(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let page = {
        let h = hub(&app);
        let g = h.lock();
        view::offer_for(&g, &e).map(|o| o.download.release_page).filter(|p| !p.is_empty())
    };
    let page = page.ok_or("There is no release page for this app.")?;
    crate::net::check_page_url(&page)?;
    winsys::shell_open(&page)
}

#[tauri::command]
pub fn uninstall(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&app, &id)?;
    jobs::start_uninstall(&app, e)
}

#[tauri::command]
pub fn download_extra(app: AppHandle, id: String, index: usize) -> Result<(), String> {
    let e = entry(&app, &id)?;
    let x = e.extras.get(index).cloned().ok_or("That download is not in the catalog.")?;
    jobs::start_extra(&app, &e, x)
}

#[tauri::command]
pub fn open_web(app: AppHandle, id: String) -> Result<(), String> {
    let url = {
        let h = hub(&app);
        let g = h.lock();
        g.catalog.as_ref().and_then(|c| c.web.iter().find(|w| w.id == id)).map(|w| w.url.clone())
    };
    let url = url.ok_or("That link is not in the catalog.")?;
    crate::net::check_page_url(&url)?;
    winsys::shell_open(&url)
}

/// The two fixed links: the hub's repository (About) and the companion hub (footer).
#[tauri::command]
pub fn open_link(which: String) -> Result<(), String> {
    let b = &crate::brand::BRAND;
    let url = match which.as_str() {
        "repository" => b.repository_url(),
        "sibling" => b.sibling_url.clone(),
        _ => return Err("Unknown link.".into()),
    };
    crate::net::check_page_url(&url)?;
    winsys::shell_open(&url)
}

#[tauri::command]
pub fn set_setting(app: AppHandle, key: String, value: bool) -> Result<View, String> {
    let mut tray_change = None;
    {
        let h = hub(&app);
        let mut g = h.lock();
        match key.as_str() {
            "offer_previews" => {
                if g.policy.no_previews && value {
                    return Err("Your organization turned previews off.".into());
                }
                g.settings.offer_previews = value;
            }
            "start_with_windows" => {
                if g.policy.no_start_with_windows && value {
                    return Err("Your organization turned this off.".into());
                }
                winsys::set_start_with_windows(value)?;
                g.settings.start_with_windows = value;
                // Starting with Windows means starting in the tray, so the tray comes on with it.
                if value && !g.settings.keep_in_tray {
                    g.settings.keep_in_tray = true;
                    tray_change = Some(true);
                }
            }
            "keep_in_tray" => {
                g.settings.keep_in_tray = value;
                tray_change = Some(value);
            }
            "daily_check" => g.settings.daily_check = value,
            _ => return Err("Unknown setting.".into()),
        }
        settings::save(&g.settings);
    }
    match tray_change {
        Some(true) => crate::tray::create(&app),
        Some(false) => crate::tray::remove(&app),
        None => {}
    }
    changed(&app);
    Ok(current_view(&app))
}

/// The one-time question on first run. "Not now" is the default answer.
#[tauri::command]
pub fn first_run_answer(app: AppHandle, start_with_windows: bool) -> Result<View, String> {
    {
        let h = hub(&app);
        let mut g = h.lock();
        g.settings.first_run_done = true;
        settings::save(&g.settings);
    }
    if start_with_windows {
        return set_setting(app, "start_with_windows".into(), true);
    }
    changed(&app);
    Ok(current_view(&app))
}

#[tauri::command]
pub fn update_hub(app: AppHandle) -> Result<(), String> {
    let d = {
        let h = hub(&app);
        let g = h.lock();
        view::hub_update(&g)
    };
    let d = d.ok_or("This is the newest version of the hub.")?;
    jobs::start_hub_update(&app, d)
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

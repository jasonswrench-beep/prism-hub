// SPDX-License-Identifier: Apache-2.0
//! The commands the window uses. The window never reaches the network or the file system itself; everything goes through these, and each one
//! acts only on what the signed catalog names.

use crate::brand::brand;
use crate::hub::{Hub, View};
use crate::jobs::{self, JobEvent};
use crate::{system, tray};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

type HubState<'a> = State<'a, Arc<Hub>>;

/// Sends the newest view to the window (if it is open) and brings the tray up to date.
pub fn publish(app: &AppHandle) {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let view = hub.view();
    let _ = app.emit("view", &view);
    tray::refresh(app, &view);
}

#[tauri::command]
pub async fn get_view(hub: HubState<'_>) -> Result<View, String> {
    let hub = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || hub.view()).await.map_err(|e| e.to_string())
}

/// Fetches the catalog (when the window opens, and on Check for updates) and gives the new view. New versions seen here are counted as
/// announced, so the tray does not notify about something the person has already seen.
#[tauri::command]
pub async fn refresh(app: AppHandle, hub: HubState<'_>) -> Result<View, String> {
    let hub = hub.inner().clone();
    let view = tauri::async_runtime::spawn_blocking(move || {
        hub.refresh();
        let view = hub.view();
        tray::mark_announced(&hub, &view);
        view
    })
    .await
    .map_err(|e| e.to_string())?;
    tray::refresh(&app, &view);
    Ok(view)
}

/// Runs a job on its own thread: its progress goes to the window as "job" events, and the view is sent again when it ends.
fn spawn_job(app: AppHandle, id: String, work: impl FnOnce(&Hub, &AtomicBool, &dyn Fn(JobEvent)) -> Result<String, String> + Send + 'static) -> Result<(), String> {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = hub.jobs.lock().unwrap();
        if running.contains_key(&id) {
            return Err("That is already under way.".into());
        }
        running.insert(id.clone(), cancel.clone());
    }
    std::thread::spawn(move || {
        let report = |e: JobEvent| {
            let _ = app.emit("job", &e);
        };
        let outcome = work(&hub, &cancel, &report);
        hub.jobs.lock().unwrap().remove(&id);
        let end = match outcome {
            Ok(message) => JobEvent { id: id.clone(), stage: "done".into(), received: 0, total: 0, message },
            Err(_) if cancel.load(Ordering::Relaxed) => {
                JobEvent { id: id.clone(), stage: "cancelled".into(), received: 0, total: 0, message: "Cancelled.".into() }
            }
            Err(message) => JobEvent { id: id.clone(), stage: "failed".into(), received: 0, total: 0, message },
        };
        let _ = app.emit("job", &end);
        publish(&app);
    });
    Ok(())
}

fn blocked_by_catalog(hub: &Hub) -> Option<String> {
    let view_needs = hub.catalog.lock().unwrap().as_ref().is_some_and(|c| crate::catalog::needs_newer_hub(c, crate::brand::HUB_VERSION));
    view_needs.then(|| "This hub needs updating before it can install apps from the new catalog.".to_string())
}

/// Install, Update and Reinstall: the six steps, with the release the hub offers now.
#[tauri::command]
pub fn start_job(app: AppHandle, hub: HubState<'_>, id: String) -> Result<(), String> {
    if let Some(why) = blocked_by_catalog(&hub) {
        return Err(why);
    }
    let entry = hub.app(&id).ok_or("That app is not in the catalog.")?;
    if entry.minimum_hub_version.as_deref().is_some_and(|m| crate::version::newer(m, crate::brand::HUB_VERSION)) {
        return Err("This app needs a newer hub. Update the hub first.".into());
    }
    let release = hub.offer_for(&entry).ok_or("No release of this app can be installed from the hub on this computer.")?;
    spawn_job(app, id, move |hub, cancel, report| {
        jobs::install(hub, &entry, &release, cancel, report)?;
        Ok(format!("{} {} is installed.", entry.name, crate::version::spoken(&release.version)))
    })
}

#[tauri::command]
pub fn download_extra(app: AppHandle, hub: HubState<'_>, id: String, index: usize) -> Result<(), String> {
    let entry = hub.app(&id).ok_or("That app is not in the catalog.")?;
    spawn_job(app, id, move |_hub, cancel, report| {
        let file = jobs::download_extra(&entry, index, cancel, report)?;
        Ok(format!("Saved in your Downloads folder as {}.", file.file_name().unwrap_or_default().to_string_lossy()))
    })
}

#[tauri::command]
pub fn cancel_job(hub: HubState<'_>, id: String) {
    if let Some(flag) = hub.jobs.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Runs the app's own uninstaller. The window has already asked, saying that the app's documents and projects stay where they are.
#[tauri::command]
pub fn uninstall(app: AppHandle, hub: HubState<'_>, id: String) -> Result<(), String> {
    let entry = hub.app(&id).ok_or("That app is not in the catalog.")?;
    spawn_job(app, id, move |hub, cancel, report| {
        jobs::uninstall(hub, &entry, cancel, report)?;
        Ok(format!("{} was removed. Its documents and projects are where they were.", entry.name))
    })
}

/// Opens an installed app from the folder Windows recorded. It never waits on the network.
pub fn open_installed(hub: &Hub, id: &str) -> Result<(), String> {
    let entry = hub.app(id).ok_or("That app is not in the catalog.")?;
    let list = system::installed_apps();
    let folder = system::find_installed(&list, &entry.installed_as)
        .and_then(|e| e.folder())
        .ok_or(format!("Windows does not list where {} is installed.", entry.name))?;
    let program = folder.join(&entry.program_file);
    if !program.is_file() {
        return Err(format!("{} is not in its install folder any more. Reinstall it from the menu.", entry.program_file));
    }
    system::launch(&program)
}

#[tauri::command]
pub fn open_app(hub: HubState<'_>, id: String) -> Result<(), String> {
    open_installed(&hub, &id)
}

#[tauri::command]
pub fn open_folder(hub: HubState<'_>, id: String) -> Result<(), String> {
    let entry = hub.app(&id).ok_or("That app is not in the catalog.")?;
    let list = system::installed_apps();
    let folder = system::find_installed(&list, &entry.installed_as).and_then(|e| e.folder()).ok_or("Windows does not list its install folder.")?;
    if !folder.is_dir() {
        return Err("The install folder Windows lists is not there.".into());
    }
    system::shell_open(&folder.to_string_lossy())
}

/// What's new: the page of the release on offer (or the app's list of releases), in the person's own browser.
#[tauri::command]
pub fn open_release_page(hub: HubState<'_>, id: String) -> Result<(), String> {
    let entry = hub.app(&id).ok_or("That app is not in the catalog.")?;
    let page = hub.offer_for(&entry).map(|r| r.release_page).unwrap_or_else(|| format!("https://github.com/{}/releases", entry.repository));
    system::shell_open(&page)
}

#[tauri::command]
pub fn open_web(hub: HubState<'_>, id: String) -> Result<(), String> {
    let url = hub
        .catalog
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.web.iter().find(|w| w.id == id).map(|w| w.url.clone()))
        .ok_or("That link is not in the catalog.")?;
    system::shell_open(&url)
}

/// The hub's own links: its repository (About) and the other hub's download page (the footer).
#[tauri::command]
pub fn open_link(which: String) -> Result<(), String> {
    let b = brand();
    match which.as_str() {
        "repository" => system::shell_open(&b.repository_url()),
        "sibling" => system::shell_open(&b.sibling.url),
        "releases" => system::shell_open(&format!("{}/releases", b.repository_url())),
        _ => Err("Unknown link.".into()),
    }
}

/// Changes one setting. Start with Windows also turns on Keep running in the tray; each can then be changed on its own.
#[tauri::command]
pub fn set_setting(app: AppHandle, hub: HubState<'_>, name: String, value: bool) -> Result<View, String> {
    let product = brand().product_name.clone();
    {
        let mut saved = hub.saved.lock().unwrap();
        match name.as_str() {
            "offerPreviews" => saved.settings.offer_previews = value,
            "keepInTray" => saved.settings.keep_in_tray = value,
            "dailyCheck" => saved.settings.daily_check = value,
            "startWithWindows" => {
                if value && !hub.policy.start_allowed() {
                    return Err("Start with Windows is turned off on this computer by whoever looks after it.".into());
                }
                system::set_start_with_windows(&product, value)?;
                if value {
                    saved.settings.keep_in_tray = true;
                }
            }
            _ => return Err("Unknown setting.".into()),
        }
        saved.save()?;
    }
    if hub.settings().keep_in_tray {
        tray::create(&app);
    } else {
        tray::remove(&app);
    }
    let view = hub.view();
    tray::refresh(&app, &view);
    Ok(view)
}

/// The first-run question about Start with Windows, asked once ("Not now" is the first choice).
#[tauri::command]
pub fn answer_first_run(app: AppHandle, hub: HubState<'_>, start: bool) -> Result<View, String> {
    {
        let mut saved = hub.saved.lock().unwrap();
        saved.settings.asked_start_with_windows = true;
        saved.save()?;
    }
    if start {
        return set_setting(app, hub, "startWithWindows".into(), true);
    }
    Ok(hub.view())
}

/// The hub's own update: the same checks, then the new installer runs once this hub closes, and the new hub starts.
#[tauri::command]
pub fn update_hub(app: AppHandle, hub: HubState<'_>) -> Result<(), String> {
    let release = hub
        .catalog
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.hub.clone())
        .filter(|h| crate::version::newer(&h.version, crate::brand::HUB_VERSION))
        .ok_or("No newer hub is in the catalog.")?;
    let handle = app.clone();
    spawn_job(app, "hub".into(), move |_hub, cancel, report| {
        jobs::update_hub(&release, cancel, report)?;
        // The installer waits for nothing but this program to end; give the window a moment to say so first.
        let quit = handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            quit.exit(0);
        });
        Ok("Updating the hub. It restarts by itself.".into())
    })
}

/// Whether Settings was chosen in the tray before this window had loaded (then the window opens Settings as it starts).
#[tauri::command]
pub fn take_pending_settings() -> bool {
    std::mem::take(&mut *tray::PENDING_SETTINGS.lock().unwrap())
}

#[tauri::command]
pub fn quit_hub(app: AppHandle) {
    app.exit(0);
}

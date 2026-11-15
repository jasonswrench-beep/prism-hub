// SPDX-License-Identifier: Apache-2.0
//! The tray, when the person keeps the hub running there (Start with Windows turns it on):
//! - left click opens the window;
//! - right click lists the installed apps (each opens directly), Check for updates, Settings and Quit;
//! - a small dot on the icon means an update is ready, and each new version gets one Windows notification, never repeated;
//! - once a day (and only then) it fetches the catalog. There are no other timers.

use crate::brand::brand;
use crate::hub::{Fetch, Hub, View};
use crate::state;
use std::sync::Arc;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const ID: &str = "hub";
const DAY: u64 = 24 * 60 * 60;

fn icon(dot: bool) -> Option<Image<'static>> {
    let bytes: &'static [u8] = if dot { include_bytes!("../icons/tray-dot.png") } else { include_bytes!("../icons/tray.png") };
    Image::from_bytes(bytes).ok()
}

pub fn exists(app: &AppHandle) -> bool {
    app.tray_by_id(ID).is_some()
}

pub fn create(app: &AppHandle) {
    if exists(app) {
        return;
    }
    let Some(picture) = icon(false) else { return };
    let mut builder = TrayIconBuilder::with_id(ID)
        .icon(picture)
        .tooltip(&brand().product_name)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::show_window(tray.app_handle());
            }
        });
    let view = app.state::<Arc<Hub>>().view();
    if let Ok(menu) = menu(app, &view) {
        builder = builder.menu(&menu);
    }
    let _ = builder.build(app);
    refresh(app, &view);
}

pub fn remove(app: &AppHandle) {
    let _ = app.remove_tray_by_id(ID);
}

fn menu(app: &AppHandle, view: &View) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let mut any = false;
    for a in view.apps.iter().filter(|a| a.installed.as_ref().is_some_and(|i| i.folder_known)) {
        menu.append(&MenuItem::with_id(app, format!("open:{}", a.id), format!("Open {}", a.name), true, None::<&str>)?)?;
        any = true;
    }
    if any {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    let check = if view.updates_ready > 0 { format!("Check for updates ({} ready)", view.updates_ready) } else { "Check for updates".into() };
    menu.append(&MenuItem::with_id(app, "check", check, view.policy.checks, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "check" => {
            // A window that is already open checks now; a new one checks as it opens.
            let was_open = app.get_webview_window(crate::WINDOW).is_some();
            crate::show_window(app);
            if was_open {
                let _ = app.emit("check-now", ());
            }
        }
        "settings" => {
            crate::show_window(app);
            // A window made just now is still loading; it asks for the settings itself when it is ready (see main.ts).
            let _ = app.emit("show-settings", ());
            if let Ok(mut pending) = PENDING_SETTINGS.lock() {
                *pending = true;
            }
        }
        other => {
            if let Some(app_id) = other.strip_prefix("open:") {
                let hub = app.state::<Arc<Hub>>().inner().clone();
                if crate::commands::open_installed(&hub, app_id).is_err() {
                    crate::show_window(app);
                }
            }
        }
    }
}

/// Settings was chosen in the tray before the window had loaded; the window takes this when it starts.
pub static PENDING_SETTINGS: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// The menu and the dot, from the newest view.
pub fn refresh(app: &AppHandle, view: &View) {
    let Some(tray) = app.tray_by_id(ID) else { return };
    let ready = view.updates_ready > 0 || view.hub_update.is_some();
    let _ = tray.set_icon(icon(ready));
    let tip = match view.updates_ready + usize::from(view.hub_update.is_some()) {
        0 => brand().product_name.clone(),
        1 => format!("{}: 1 update ready", brand().product_name),
        n => format!("{}: {n} updates ready", brand().product_name),
    };
    let _ = tray.set_tooltip(Some(&tip));
    if let Ok(menu) = menu(app, view) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// The new versions in this view that have not been announced yet: (key, title).
fn unannounced(hub: &Hub, view: &View) -> Vec<(String, String)> {
    let saved = hub.saved.lock().unwrap();
    let mut out = Vec::new();
    for a in view.apps.iter().filter(|a| a.update) {
        if let Some(o) = &a.offer {
            let key = format!("{}@{}", a.id, o.version);
            if !saved.announced.contains(&key) {
                out.push((key, format!("{} {} is ready", a.name, crate::version::spoken(&o.version))));
            }
        }
    }
    if let Some(h) = &view.hub_update {
        let key = format!("hub@{}", h.version);
        if !saved.announced.contains(&key) {
            out.push((key, format!("{} {} is ready", brand().product_name, crate::version::spoken(&h.version))));
        }
    }
    out
}

/// Counts every new version in the view as announced (the person saw it in the window).
pub fn mark_announced(hub: &Hub, view: &View) {
    let fresh = unannounced(hub, view);
    if fresh.is_empty() {
        return;
    }
    let mut saved = hub.saved.lock().unwrap();
    saved.announced.extend(fresh.into_iter().map(|(k, _)| k));
    let _ = saved.save();
}

fn notify(app: &AppHandle, hub: &Hub, view: &View) {
    use tauri_plugin_notification::NotificationExt;
    let fresh = unannounced(hub, view);
    for (_, title) in &fresh {
        let _ = app.notification().builder().title(title).body(format!("Open {} to update it.", brand().product_name)).show();
    }
    mark_announced(hub, view);
}

/// The once-a-day check while the hub is in the tray. It waits a minute after start (the network may not be up yet at sign-in), then
/// checks when a day has passed since the last fetch. The thread sleeps in between; it wakes at most every six hours to look at the clock,
/// because a computer that sleeps overnight does not count that time.
pub fn start_daily_check(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            let hub = app.state::<Arc<Hub>>().inner().clone();
            let settings = hub.settings();
            let due = hub.saved.lock().unwrap().last_check.map_or(0, |t| t + DAY);
            let now = state::now();
            let mut wait = due.saturating_sub(now).clamp(60, 6 * 60 * 60);
            if exists(&app) && settings.daily_check && hub.policy.checks_allowed() && now >= due {
                let outcome = hub.refresh();
                let view = hub.view();
                notify(&app, &hub, &view);
                let _ = app.emit("view", &view);
                refresh(&app, &view);
                // Offline: try again in an hour rather than a day.
                wait = if outcome == Fetch::Offline { 60 * 60 } else { DAY.min(6 * 60 * 60) };
            }
            std::thread::sleep(Duration::from_secs(wait));
        }
    });
}

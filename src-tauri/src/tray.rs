//! The tray icon: left click opens the window, right click lists the installed apps, Check for
//! updates, Settings and Quit. A dot on the icon means an update is ready.

use crate::brand::BRAND;
use crate::view;
use crate::{hub, show_window, winsys};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

const ID: &str = "main";
static DAILY_STARTED: AtomicBool = AtomicBool::new(false);

fn plain_icon() -> Option<Image<'static>> {
    Image::from_bytes(include_bytes!("../icons/32x32.png")).ok().map(|i| i.to_owned())
}

/// The icon with a small accent-colored dot in the lower right corner.
fn dot_icon() -> Option<Image<'static>> {
    let base = plain_icon()?;
    let (w, h) = (base.width() as i32, base.height() as i32);
    let mut rgba = base.rgba().to_vec();
    let (r, g, b) = hex_rgb(&BRAND.accent).unwrap_or((21, 26, 61));
    let radius = (w as f32 * 0.22).max(3.0);
    let (cx, cy) = (w as f32 - radius - 0.5, h as f32 - radius - 0.5);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if d <= radius {
                rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            } else if d <= radius + 1.2 {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]); // a white ring so the dot shows on any icon
            }
        }
    }
    Some(Image::new_owned(rgba, w as u32, h as u32))
}

fn hex_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#')?;
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let installed = winsys::installed_apps();
    let apps: Vec<(String, String)> = {
        let h = hub(app);
        let g = h.lock();
        g.catalog
            .as_ref()
            .map(|c| {
                c.apps
                    .iter()
                    .filter(|a| a.section == "desktop" && !g.policy.hidden_apps.contains(&a.id))
                    .filter(|a| view::installed_for(a, &installed).and_then(|i| view::program_path(a, i)).map(|p| p.is_file()).unwrap_or(false))
                    .map(|a| (a.id.clone(), a.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let menu = Menu::new(app)?;
    for (id, name) in &apps {
        menu.append(&MenuItem::with_id(app, format!("open:{id}"), format!("Open {name}"), true, None::<&str>)?)?;
    }
    if !apps.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    let checks_off = hub(app).lock().policy.no_update_checks;
    menu.append(&MenuItem::with_id(app, "check", "Check for updates", !checks_off, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", format!("Quit {}", BRAND.product_name), true, None::<&str>)?)?;
    Ok(menu)
}

fn has_updates(app: &AppHandle) -> bool {
    let installed = winsys::installed_apps();
    let h = hub(app);
    let g = h.lock();
    !view::updates_ready(&g, &installed).is_empty() || view::hub_update(&g).is_some()
}

pub fn create(app: &AppHandle) {
    if app.tray_by_id(ID).is_some() {
        hub(app).lock().tray = true;
        return;
    }
    let Ok(menu) = build_menu(app) else { return };
    let mut b = TrayIconBuilder::with_id(ID)
        .tooltip(&BRAND.product_name)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "check" => crate::check(app),
                "settings" => show_window(app, Some("settings")),
                "quit" => app.exit(0),
                other => {
                    if let Some(app_id) = other.strip_prefix("open:") {
                        let _ = crate::commands_open(app, app_id);
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle(), None);
            }
        });
    if let Some(i) = if has_updates(app) { dot_icon() } else { plain_icon() } {
        b = b.icon(i);
    }
    if b.build(app).is_ok() {
        hub(app).lock().tray = true;
        start_daily(app);
    }
}

pub fn remove(app: &AppHandle) {
    let _ = app.remove_tray_by_id(ID);
    hub(app).lock().tray = false;
}

/// Rebuild the menu (the installed apps may have changed) and set the dot.
pub fn refresh(app: &AppHandle) {
    let Some(t) = app.tray_by_id(ID) else { return };
    if let Ok(menu) = build_menu(app) {
        let _ = t.set_menu(Some(menu));
    }
    let dot = has_updates(app);
    let _ = t.set_icon(if dot { dot_icon() } else { plain_icon() });
    let _ = t.set_tooltip(Some(if dot { format!("{}: an update is ready", BRAND.product_name) } else { BRAND.product_name.clone() }));
}

/// One Windows notification per new version, never repeated, and only while the window is
/// closed (with the window open the status line says it).
pub fn notify_new_versions(app: &AppHandle) {
    let installed = winsys::installed_apps();
    let mut toasts = vec![];
    {
        let h = hub(app);
        let mut g = h.lock();
        let quiet = g.window_open || !g.tray;
        let mut ready = view::updates_ready(&g, &installed);
        if let Some(d) = view::hub_update(&g) {
            ready.push((crate::jobs::HUB.to_string(), BRAND.product_name.clone(), d.version));
        }
        let mut changed = false;
        for (id, name, version) in ready {
            if g.settings.notified.get(&id) != Some(&version) {
                g.settings.notified.insert(id, version.clone());
                changed = true;
                if !quiet {
                    toasts.push(format!("{name} {version} is ready to install."));
                }
            }
        }
        if changed {
            crate::settings::save(&g.settings);
        }
    }
    for body in toasts {
        let _ = app.notification().builder().title(format!("{}: update ready", BRAND.product_name)).body(body).show();
    }
}

/// While the tray is on: one catalog check a day. The thread wakes once an hour only to compare
/// the clock with the last check (a sleeping computer does not count down a single long wait).
fn start_daily(app: &AppHandle) {
    if DAILY_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // Give sign-in a couple of minutes before the first look.
        std::thread::sleep(Duration::from_secs(120));
        loop {
            let due = {
                let h = hub(&app);
                let g = h.lock();
                g.tray && g.settings.daily_check && !g.policy.no_update_checks && g.settings.last_check.map(|t| crate::util::now_secs() - t >= 24 * 3600).unwrap_or(true)
            };
            if due {
                crate::check(&app);
            }
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

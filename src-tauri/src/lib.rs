// SPDX-License-Identifier: Apache-2.0
//! The hub (OpenStage Hub or Prism Hub, by brand/brand.json): lists every app in the suite and installs, updates and opens each one from its signed GitHub release.

pub mod brand;
pub mod catalog;
pub mod hub;
pub mod jobs;
pub mod net;
pub mod state;
pub mod system;
pub mod trust;
pub mod version;

mod commands;
mod tray;

use hub::Hub;
use std::sync::Arc;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

pub const WINDOW: &str = "main";

/// Shows the hub's window, making it when it is not there (in the tray the window is fully closed, not hidden, to keep memory low).
pub fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title(&brand::brand().product_name)
        .inner_size(470.0, 720.0)
        .min_inner_size(360.0, 420.0)
        // Ctrl and plus or minus (or Ctrl and the mouse wheel) make everything larger or smaller, to 200 percent and beyond.
        .zoom_hotkeys_enabled(true)
        // The web view's options: Tauri's own three (no pop-up windows of the web view's, no PDF viewer, no SmartScreen lookups), plus no
        // separate graphics process (a list of apps draws well without one, and it saves memory) and none of the web view's own background
        // network requests.
        .additional_browser_args(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu --disable-background-networking --no-first-run",
        )
        .visible(true)
        .build();
    if let Ok(window) = built {
        let _ = window.set_focus();
    }
}

pub fn run() {
    let in_tray = std::env::args().any(|a| a == "--tray");
    // A development copy can print what the window would show (after fetching the catalog) and stop: <program> --print-view
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--print-view") {
        let hub = Hub::new(false);
        hub.refresh();
        println!("{}", serde_json::to_string_pretty(&hub.view()).unwrap_or_default());
        return;
    }
    // A development copy can run the six install steps for one app, printing each stage: <program> --install <app id>
    #[cfg(debug_assertions)]
    if let Some(id) = std::env::args().skip_while(|a| a != "--install").nth(1) {
        let hub = Hub::new(false);
        hub.refresh();
        let Some(app) = hub.app(&id) else { return println!("{id} is not in the catalog.") };
        let Some(release) = hub.offer_for(&app) else { return println!("No release of {id} is offered.") };
        println!("Installing {} {}", app.name, release.version);
        let report = |e: jobs::JobEvent| {
            if e.stage != "downloading" || e.received == 0 || e.received == e.total {
                println!("  {}: {} {}", e.stage, e.message, if e.total > 0 { format!("({} of {} bytes)", e.received, e.total) } else { String::new() });
            }
        };
        match jobs::install(&hub, &app, &release, &std::sync::atomic::AtomicBool::new(false), &report) {
            Ok(()) => println!("Done: {}", serde_json::to_string(&hub.view().apps.iter().find(|a| a.id == id)).unwrap_or_default()),
            Err(why) => println!("Failed: {why}"),
        }
        return;
    }
    let hub = Arc::new(Hub::new(in_tray));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_notification::init())
        .manage(hub.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_view,
            commands::refresh,
            commands::start_job,
            commands::download_extra,
            commands::cancel_job,
            commands::uninstall,
            commands::open_app,
            commands::open_folder,
            commands::open_release_page,
            commands::open_web,
            commands::open_link,
            commands::set_setting,
            commands::answer_first_run,
            commands::update_hub,
            commands::take_pending_settings,
            commands::quit_hub,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let settings = hub.settings();
            if in_tray || settings.keep_in_tray {
                tray::create(&handle);
            }
            if !in_tray {
                show_window(&handle);
            }
            tray::start_daily_check(handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the hub could not start");

    app.run(|handle, event| {
        // Closing the window quits the hub, unless the tray is on: then the window is closed and the hub stays in the tray.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            if tray::exists(handle) {
                api.prevent_exit();
            }
        }
    });
}

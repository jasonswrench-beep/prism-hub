//! Prism Hub: lists the suite's apps from the signed catalog and installs, updates and opens them.

pub mod authenticode;
pub mod brand;
pub mod catalog;
mod commands;
pub mod jobs;
pub mod net;
pub mod settings;
mod tray;
pub mod trust;
pub mod util;
pub mod view;
pub mod winsys;

use brand::BRAND;
use catalog::{Catalog, Envelope, Read};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

pub use jobs::Job;

/// Everything the hub knows. Held briefly: no network or disk work happens under the lock.
pub struct Inner {
    pub catalog: Option<Catalog>,
    /// A genuine catalog that needs a newer hub than this one (its hub entry is still usable).
    pub newer_hub_needed: Option<Envelope>,
    pub settings: settings::Settings,
    pub policy: settings::Policy,
    pub offline: bool,
    pub checking: bool,
    pub check_error: Option<String>,
    pub jobs: HashMap<String, Job>,
    pub problems: HashMap<String, String>,
    /// The tray icon is showing (start-up in the tray, or "keep running in the tray").
    pub tray: bool,
    pub window_open: bool,
}

pub struct Hub(pub Mutex<Inner>);

impl Hub {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn hub(app: &AppHandle) -> tauri::State<'_, Hub> {
    app.state::<Hub>()
}

/// The UI re-reads the view whenever this fires.
pub fn changed(app: &AppHandle) {
    let _ = app.emit("hub://changed", ());
    tray::refresh(app);
}

fn load() -> Inner {
    let mut s = settings::load();
    let policy = settings::load_policy();
    if policy.no_start_with_windows && s.start_with_windows {
        s.start_with_windows = false;
        let _ = winsys::set_start_with_windows(false);
    }
    let (catalog, newer) = match catalog::load_cached() {
        Some((Read::Usable(c), _)) => (Some(c), None),
        Some((Read::NeedsNewerHub(e), _)) => (None, Some(e)),
        None => (None, None),
    };
    Inner { catalog, newer_hub_needed: newer, settings: s, policy, offline: false, checking: false, check_error: None, jobs: HashMap::new(), problems: HashMap::new(), tray: false, window_open: false }
}

/// Leftover downloads from an install that was interrupted (or the hub's own installer after a
/// self-update) are removed at start.
fn clean_downloads() {
    let dir = BRAND.data_dir().join("downloads");
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Fetch the catalog and its signature, in the background. Does nothing while a check is
/// already running or when policy turned checks off.
pub fn check(app: &AppHandle) {
    {
        let h = hub(app);
        let mut g = h.lock();
        if g.checking || g.policy.no_update_checks {
            return;
        }
        g.checking = true;
    }
    changed(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let fetch = || net::fetch_small(&BRAND.catalog_url, 4 * 1024 * 1024).and_then(|data| net::fetch_small(&BRAND.signature_url(), 16 * 1024).map(|sig| (data, sig)));
        let mut result = fetch();
        // The two files are uploaded one after the other; fetched in between, they do not match.
        // One more try a few seconds later settles that.
        if let Ok((data, sig)) = &result {
            if catalog::verify(data, &String::from_utf8_lossy(sig)).is_err() {
                std::thread::sleep(std::time::Duration::from_secs(4));
                result = fetch();
            }
        }
        {
            let h = hub(&app);
            let mut g = h.lock();
            g.checking = false;
            match result {
                Ok((data, sig)) => {
                    g.offline = false;
                    let sig = String::from_utf8_lossy(&sig).to_string();
                    match catalog::read(&data, &sig) {
                        Ok(r) => {
                            g.check_error = None;
                            g.settings.last_check = Some(util::now_secs());
                            let current = g.catalog.as_ref().map(|c| c.generated.clone()).unwrap_or_default();
                            match r {
                                // An older catalog than the one in hand (a replayed file) is ignored.
                                Read::Usable(c) if c.generated.as_str() >= current.as_str() => {
                                    let _ = catalog::store(&data, &sig);
                                    g.catalog = Some(c);
                                    g.newer_hub_needed = None;
                                }
                                Read::Usable(_) => {}
                                // Kept in memory only, so the cached catalog this hub can read stays on disk.
                                Read::NeedsNewerHub(e) => g.newer_hub_needed = Some(e),
                            }
                            settings::save(&g.settings);
                        }
                        Err(e) => g.check_error = Some(format!("The catalog from GitHub was ignored because {e}.")),
                    }
                }
                Err(net::NetError::Offline(_)) => g.offline = true,
                Err(e) => {
                    g.offline = false;
                    g.check_error = Some(format!("The app list could not be downloaded ({e})."));
                }
            }
        }
        tray::notify_new_versions(&app);
        changed(&app);
    });
}

/// Open an installed app (from the tray menu).
pub(crate) fn commands_open(app: &AppHandle, id: &str) -> Result<(), String> {
    commands::open_app(app.clone(), id.to_string())
}

pub fn show_window(app: &AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = w.emit("hub://page", p);
        }
        return;
    }
    let url = match page {
        Some(p) => format!("index.html#{p}"),
        None => "index.html".into(),
    };
    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title(&BRAND.product_name)
        .inner_size(470.0, 720.0)
        .min_inner_size(360.0, 460.0)
        .resizable(true)
        .visible(true)
        .build();
    if built.is_ok() {
        hub(app).lock().window_open = true;
        check(app);
    }
}

pub fn run() {
    let tray_start = std::env::args().any(|a| a == "--tray");
    clean_downloads();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_window(app, None)))
        .plugin(tauri_plugin_notification::init())
        .manage(Hub(Mutex::new(load())))
        .invoke_handler(tauri::generate_handler![
            commands::get_view,
            commands::check_now,
            commands::install,
            commands::reinstall,
            commands::cancel,
            commands::open_app,
            commands::open_folder,
            commands::whats_new,
            commands::uninstall,
            commands::download_extra,
            commands::open_web,
            commands::open_link,
            commands::set_setting,
            commands::first_run_answer,
            commands::update_hub,
            commands::dismiss_problem,
            commands::quit,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let keep = hub(&handle).lock().settings.keep_in_tray;
            if tray_start || keep {
                tray::create(&handle);
            }
            if !tray_start {
                show_window(&handle, None);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the hub could not start")
        .run(|app, event| match event {
            // The window closing ends the hub unless the tray is on (the window is fully closed
            // either way, so the tray-only hub stays small).
            RunEvent::ExitRequested { api, code: None, .. } => {
                if hub(app).lock().tray {
                    api.prevent_exit();
                }
            }
            RunEvent::WindowEvent { label, event: tauri::WindowEvent::Destroyed, .. } if label == "main" => {
                hub(app).lock().window_open = false;
            }
            _ => {}
        });
}

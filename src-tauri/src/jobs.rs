// SPDX-License-Identifier: Apache-2.0
//! Install and update use the same six steps (the spec's "Installing, updating and opening"):
//! 1. download the installer named in the catalog into the hub's own download folder (GitHub addresses only);
//! 2. check its SHA-256 against the catalog; a mismatch deletes the file and stops;
//! 3. check its code signature with Windows' own check: valid, timestamped, signed by the expected signer; anything else deletes it and stops;
//! 4. if the app is running, ask the person to close it and wait (the hub never closes an app itself: that could lose unsaved work);
//! 5. run the installer with its quiet switch (it shows its own progress bar, not its pages; installs are per user, so no administrator);
//! 6. read the installed version back from Windows' installed apps list and confirm it, then delete the download.

use crate::catalog::{App, Release};
use crate::hub::Hub;
use crate::state::{self, Record};
use crate::system;
use crate::trust;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    /// The app's id, or "hub" for the hub's own update.
    pub id: String,
    /// downloading, checking, waiting, installing, removing, done, failed or cancelled.
    pub stage: String,
    pub received: u64,
    pub total: u64,
    /// One plain sentence: what is happening, or what failed.
    pub message: String,
}

pub type Report<'a> = &'a dyn Fn(JobEvent);

fn event(id: &str, stage: &str, message: impl Into<String>) -> JobEvent {
    JobEvent { id: id.into(), stage: stage.into(), received: 0, total: 0, message: message.into() }
}

/// A file name for the download that cannot step out of the download folder.
fn download_path(id: &str, version: &str, ext: &str) -> PathBuf {
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect::<String>();
    state::downloads_dir().join(format!("{}-{}-setup.{ext}", clean(id), clean(version)))
}

/// Steps 1 to 3: the installer downloaded and checked, or why not. A file that fails a check is deleted.
pub fn fetch_and_check(id: &str, release: &Release, signer: &str, cancel: &AtomicBool, report: Report) -> Result<PathBuf, String> {
    std::fs::create_dir_all(state::downloads_dir()).map_err(|_| "The hub's download folder could not be made.".to_string())?;
    let file = download_path(id, &release.version, "exe");
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut progress = |received: u64, _total: u64| {
        if last.elapsed() >= Duration::from_millis(200) || received == release.size {
            last = Instant::now();
            report(JobEvent { id: id.into(), stage: "downloading".into(), received, total: release.size, message: "Downloading".into() });
        }
    };
    report(JobEvent { id: id.into(), stage: "downloading".into(), received: 0, total: release.size, message: "Downloading".into() });
    crate::net::download(&release.url, &file, release.size, cancel, &mut progress).map_err(|e| e.words("installer"))?;

    report(event(id, "checking", "Checking"));
    let refuse = |why: String| {
        let _ = std::fs::remove_file(&file);
        Err(why)
    };
    match trust::sha256_file(&file) {
        Ok(sum) if sum.eq_ignore_ascii_case(&release.sha256) => {}
        Ok(_) => return refuse("The download did not match its checksum, so nothing was installed.".into()),
        Err(_) => return refuse("The download could not be read back to check it, so nothing was installed.".into()),
    }
    if cancel.load(Ordering::Relaxed) {
        return refuse("Cancelled. Nothing was installed.".into());
    }
    if let Err(why) = trust::check_signed_by(&file, signer) {
        return refuse(format!("{why} Nothing was installed."));
    }
    Ok(file)
}

/// Step 4: waits while the app is open. Returns false when the person cancelled.
fn wait_until_closed(app: &App, folder: Option<&Path>, cancel: &AtomicBool, report: Report) -> bool {
    let Some(folder) = folder else { return true };
    let mut said = false;
    while system::is_running(folder, &app.program_file) {
        if !said {
            report(event(&app.id, "waiting", format!("Close {} to continue. The hub waits; it never closes an app itself.", app.name)));
            said = true;
        }
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(800));
    }
    true
}

/// Runs a program and waits for it. Its exit code, or why it could not start.
fn run_and_wait(program: &Path, args: &[String]) -> Result<i32, String> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(folder) = program.parent() {
        command.current_dir(folder);
    }
    match command.status() {
        Ok(status) => Ok(status.code().unwrap_or(-1)),
        // ERROR_ELEVATION_REQUIRED
        Err(e) if e.raw_os_error() == Some(740) => {
            Err(format!("The installer asked for administrator rights. {} apps install for one person and never need them, so it was not run.", crate::brand::brand().short_name))
        }
        Err(_) => Err("Windows could not start the installer.".into()),
    }
}

/// The whole install or update of an app.
pub fn install(hub: &Hub, app: &App, release: &Release, cancel: &AtomicBool, report: Report) -> Result<(), String> {
    let file = fetch_and_check(&app.id, release, &app.expected_signer, cancel, report)?;
    let result = (|| {
        let before = system::installed_apps();
        let folder = system::find_installed(&before, &app.installed_as).and_then(|e| e.folder());
        if !wait_until_closed(app, folder.as_deref(), cancel, report) {
            return Err("Cancelled. Nothing was installed.".to_string());
        }
        report(event(&app.id, "installing", "Installing"));
        let code = run_and_wait(&file, &app.quiet_switch)?;
        if code != 0 {
            return Err(format!("The installer stopped with an error (code {code}), so {} may not have been installed.", app.name));
        }
        let after = system::installed_apps();
        let listed = system::find_installed(&after, &app.installed_as).map(|e| e.display_version.trim().to_string());
        match listed {
            Some(v) if v == release.windows_version.trim() => {
                let mut saved = hub.saved.lock().unwrap();
                saved.installed.insert(app.id.clone(), Record { version: release.version.clone(), windows_version: v });
                let _ = saved.save();
                Ok(())
            }
            Some(v) => Err(format!("The installer finished, but Windows lists {} as version {v}, not {}.", app.name, release.windows_version)),
            None => Err(format!("The installer finished, but Windows does not list {} as installed.", app.name)),
        }
    })();
    let _ = std::fs::remove_file(&file);
    result
}

/// Other downloads (a portable copy or an add-on): downloaded, its SHA-256 checked, and put in the person's Downloads folder, shown in File
/// Explorer. Nothing is run.
pub fn download_extra(app: &App, index: usize, cancel: &AtomicBool, report: Report) -> Result<PathBuf, String> {
    let extra = app.extras.get(index).ok_or("That download is not in the catalog.")?;
    std::fs::create_dir_all(state::downloads_dir()).map_err(|_| "The hub's download folder could not be made.".to_string())?;
    let file = download_path(&app.id, &format!("extra-{index}"), "part");
    report(JobEvent { id: app.id.clone(), stage: "downloading".into(), received: 0, total: extra.size, message: "Downloading".into() });
    let mut progress = |received: u64, _| {
        report(JobEvent { id: app.id.clone(), stage: "downloading".into(), received, total: extra.size, message: "Downloading".into() })
    };
    crate::net::download(&extra.url, &file, extra.size, cancel, &mut progress).map_err(|e| e.words("download"))?;
    report(event(&app.id, "checking", "Checking"));
    if !trust::sha256_file(&file).is_ok_and(|s| s.eq_ignore_ascii_case(&extra.sha256)) {
        let _ = std::fs::remove_file(&file);
        return Err("The download did not match its checksum, so it was deleted.".into());
    }
    let downloads = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Downloads")).ok_or("Your Downloads folder could not be found.")?;
    let mut target = downloads.join(&extra.file_name);
    let mut n = 1;
    while target.exists() {
        let stem = Path::new(&extra.file_name).file_stem().unwrap_or_default().to_string_lossy().to_string();
        let ext = Path::new(&extra.file_name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        target = downloads.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    if std::fs::rename(&file, &target).is_err() {
        std::fs::copy(&file, &target).map_err(|_| "The download could not be put in your Downloads folder.".to_string())?;
        let _ = std::fs::remove_file(&file);
    }
    let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", target.display())).spawn();
    Ok(target)
}

/// Runs the app's own uninstaller (the one Windows lists) without its questions, and waits until Windows no longer lists the app.
pub fn uninstall(hub: &Hub, app: &App, cancel: &AtomicBool, report: Report) -> Result<(), String> {
    let list = system::installed_apps();
    let entry = system::find_installed(&list, &app.installed_as).ok_or(format!("Windows does not list {} as installed.", app.name))?;
    if !wait_until_closed(app, entry.folder().as_deref(), cancel, report) {
        return Err("Cancelled. Nothing was removed.".into());
    }
    let (program, args) = if !entry.quiet_uninstall_string.trim().is_empty() {
        system::split_command(&entry.quiet_uninstall_string).ok_or("The uninstaller Windows lists could not be read.")?
    } else {
        let (p, mut a) = system::split_command(&entry.uninstall_string).ok_or("The uninstaller Windows lists could not be read.")?;
        a.extend(app.quiet_uninstall_switch.iter().cloned());
        (p, a)
    };
    if !program.is_file() {
        return Err("The uninstaller Windows lists is not there any more.".into());
    }
    report(event(&app.id, "removing", "Removing"));
    let code = run_and_wait(&program, &args)?;
    if code != 0 {
        return Err(format!("The uninstaller stopped with an error (code {code})."));
    }
    // Most uninstallers copy themselves elsewhere and carry on after the first one ends, so the hub watches Windows' list for up to two minutes.
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        if system::find_installed(&system::installed_apps(), &app.installed_as).is_none() {
            let mut saved = hub.saved.lock().unwrap();
            saved.installed.remove(&app.id);
            let _ = saved.save();
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    Err(format!("The uninstaller finished, but Windows still lists {}.", app.name))
}

/// The hub's own update: the same checks on its own installer, which then runs (showing its progress bar) once this hub has closed, and
/// starts the new hub when it is done.
pub fn update_hub(release: &Release, cancel: &AtomicBool, report: Report) -> Result<(), String> {
    let signer = crate::brand::brand().expected_signer.clone();
    let file = fetch_and_check("hub", release, &signer, cancel, report)?;
    report(event("hub", "installing", "Installing"));
    let exe = std::env::current_exe().map_err(|_| "The hub could not find its own program file.".to_string())?;
    // cmd waits for the setup (start /wait), then starts the new hub. The two paths are under this person's local app data and are quoted.
    let line = format!("/d /s /c \"start \"\" /wait \"{}\" /P & start \"\" \"{}\"\"", file.display(), exe.display());
    let mut command = std::process::Command::new("cmd.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.raw_arg(line);
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    command.arg(line);
    command.spawn().map(|_| ()).map_err(|_| "Windows could not start the hub's installer.".to_string())
}

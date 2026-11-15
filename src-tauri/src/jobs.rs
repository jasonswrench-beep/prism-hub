//! Installing, updating, uninstalling, extra downloads and the hub's own update.
//!
//! Install and update follow the same six steps:
//! 1. download the installer named in the catalog into the hub's download folder;
//! 2. check its SHA-256 against the catalog (a mismatch deletes it and stops);
//! 3. check its code signature with Windows (valid, timestamped, the expected signer);
//! 4. if the app is running, ask the person to close it and wait (the hub never closes it);
//! 5. run the installer with its quiet switch (a progress bar, no pages, no administrator);
//! 6. read the installed version back from Windows and confirm it, then delete the download.

use crate::brand::BRAND;
use crate::catalog::{AppEntry, Download, Extra};
use crate::net::{self, NetError};
use crate::view;
use crate::winsys;
use crate::{changed, hub};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// The job key for the hub's own update.
pub const HUB: &str = "hub";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Install,
    Update,
    Reinstall,
    Uninstall,
    Extra,
    Hub,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Downloading,
    Checking,
    /// Waiting for the person to close the app.
    Waiting,
    Installing,
    Uninstalling,
}

pub struct Job {
    pub kind: Kind,
    pub stage: Stage,
    pub received: u64,
    pub total: u64,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobView {
    pub kind: Kind,
    pub stage: Stage,
    pub received: u64,
    pub total: u64,
    /// Cancel stops the job safely only before the installer or uninstaller starts.
    pub cancellable: bool,
}

impl Job {
    pub fn view(&self) -> JobView {
        JobView { kind: self.kind, stage: self.stage, received: self.received, total: self.total, cancellable: matches!(self.stage, Stage::Downloading | Stage::Checking | Stage::Waiting) }
    }
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    id: &'a str,
    received: u64,
    total: u64,
}

enum Fail {
    Cancelled,
    Problem(String),
}

impl From<String> for Fail {
    fn from(s: String) -> Self {
        Fail::Problem(s)
    }
}

fn downloads_dir() -> Result<PathBuf, Fail> {
    let d = BRAND.data_dir().join("downloads");
    std::fs::create_dir_all(&d).map_err(|e| Fail::Problem(format!("The download folder could not be made ({e}).")))?;
    Ok(d)
}

fn safe_part(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect()
}

fn set_stage(app: &AppHandle, id: &str, stage: Stage) {
    if let Some(j) = hub(app).lock().jobs.get_mut(id) {
        j.stage = stage;
    }
    changed(app);
}

/// Begin a job for `id`, or refuse if one is already running for it.
fn begin(app: &AppHandle, id: &str, kind: Kind, total: u64) -> Result<Arc<AtomicBool>, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let h = hub(app);
        let mut g = h.lock();
        if g.jobs.contains_key(id) {
            return Err("This app is already busy.".into());
        }
        g.problems.remove(id);
        let stage = if kind == Kind::Uninstall { Stage::Uninstalling } else { Stage::Downloading };
        g.jobs.insert(id.to_string(), Job { kind, stage, received: 0, total, cancel: cancel.clone() });
    }
    changed(app);
    Ok(cancel)
}

fn finish(app: &AppHandle, id: &str, result: Result<(), Fail>) {
    {
        let h = hub(app);
        let mut g = h.lock();
        g.jobs.remove(id);
        match result {
            Ok(()) | Err(Fail::Cancelled) => {
                g.problems.remove(id);
            }
            Err(Fail::Problem(msg)) => {
                g.problems.insert(id.to_string(), msg);
            }
        }
    }
    changed(app);
}

/// Download with progress, throttled to about ten updates a second.
fn fetch(app: &AppHandle, id: &str, url: &str, dest: &Path, size: u64, cancel: &AtomicBool) -> Result<String, Fail> {
    let mut last = Instant::now() - Duration::from_secs(1);
    let result = net::download(url, dest, size, cancel, |received, total| {
        if last.elapsed() >= Duration::from_millis(100) || received == total {
            last = Instant::now();
            if let Some(j) = hub(app).lock().jobs.get_mut(id) {
                j.received = received;
                j.total = total;
            }
            let _ = app.emit("hub://progress", Progress { id, received, total });
        }
    });
    result.map_err(|e| {
        let _ = std::fs::remove_file(dest);
        match e {
            NetError::Cancelled => Fail::Cancelled,
            NetError::Offline(_) => Fail::Problem("The download stopped because the connection was lost, so nothing was installed.".into()),
            NetError::Failed(why) => Fail::Problem(format!("The download failed ({why}), so nothing was installed.")),
        }
    })
}

/// Open the file so no one can change it until the hub is done with it, and hash that handle.
fn lock_and_hash(path: &Path) -> Result<(std::fs::File, String), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.share_mode(0x0000_0001); // FILE_SHARE_READ: others may read, not write or delete
    }
    let mut f = opts.open(path).map_err(|e| format!("The downloaded file could not be opened ({e})."))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("The downloaded file could not be read ({e})."))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok((f, hex::encode(h.finalize())))
}

const CHECKSUM: &str = "The download did not match its checksum, so nothing was installed.";

/// Steps 1 to 3: download, checksum, signature. Returns the path and the open, locked file.
fn download_and_check(app: &AppHandle, id: &str, d: &Download, signer: &str, cancel: &AtomicBool) -> Result<(PathBuf, std::fs::File), Fail> {
    let path = downloads_dir()?.join(format!("{}-{}-setup.exe", safe_part(id), safe_part(&d.version)));
    let _ = std::fs::remove_file(&path);
    let streamed = fetch(app, id, &d.url, &path, d.size, cancel)?;
    set_stage(app, id, Stage::Checking);
    let bad = |msg: String| {
        let _ = std::fs::remove_file(&path);
        Fail::Problem(msg)
    };
    if streamed != d.sha256 {
        return Err(bad(CHECKSUM.into()));
    }
    let (file, sha) = lock_and_hash(&path).map_err(bad)?;
    if sha != d.sha256 {
        drop(file);
        return Err(bad(CHECKSUM.into()));
    }
    if let Err(why) = crate::authenticode::check(&path, signer) {
        drop(file);
        return Err(bad(format!("The installer's signature could not be confirmed ({why}), so nothing was installed.")));
    }
    if cancel.load(Ordering::Relaxed) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(Fail::Cancelled);
    }
    Ok((path, file))
}

/// Step 4: wait for the person to close the app. The hub never closes it: that could lose
/// unsaved work or leave a project or library half written.
fn wait_for_close(app: &AppHandle, id: &str, program: Option<&Path>, cancel: &AtomicBool) -> Result<(), Fail> {
    let Some(program) = program else { return Ok(()) };
    if !winsys::is_running(program) {
        return Ok(());
    }
    set_stage(app, id, Stage::Waiting);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Fail::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(800));
        if !winsys::is_running(program) {
            return Ok(());
        }
    }
}

fn run_and_wait(program: &Path, switches: &str) -> Result<i32, String> {
    let status = std::process::Command::new(program).args(switches.split_whitespace()).status().map_err(|e| format!("Windows could not start the installer ({e})."))?;
    Ok(status.code().unwrap_or(-1))
}

/// Install, update or reinstall one app.
pub fn start_install(app: &AppHandle, entry: AppEntry, d: Download, kind: Kind) -> Result<(), String> {
    let cancel = begin(app, &entry.id, kind, d.size)?;
    let app = app.clone();
    std::thread::spawn(move || {
        let r = install_steps(&app, &entry, &d, &cancel);
        finish(&app, &entry.id, r);
    });
    Ok(())
}

fn install_steps(app: &AppHandle, entry: &AppEntry, d: &Download, cancel: &AtomicBool) -> Result<(), Fail> {
    let id = entry.id.as_str();
    let (path, file) = download_and_check(app, id, d, &entry.expected_signer, cancel)?;
    let cleanup = |file: std::fs::File| {
        drop(file);
        let _ = std::fs::remove_file(&path);
    };
    let program = winsys::installed_apps().get(entry.installed_as.trim()).and_then(|i| view::program_path(entry, i));
    if let Err(e) = wait_for_close(app, id, program.as_deref(), cancel) {
        cleanup(file);
        return Err(e);
    }
    set_stage(app, id, Stage::Installing);
    // Checked once more right before the installer starts: an installer in quiet mode would
    // close a running copy by force, which is exactly what the hub must not do.
    if program.as_deref().map(winsys::is_running).unwrap_or(false) {
        cleanup(file);
        return Err(Fail::Problem(format!("{} was opened again before the update could start. Close it and press Retry.", entry.name)));
    }
    let code = run_and_wait(&path, &entry.quiet_switch);
    cleanup(file);
    let code = code?;
    if code != 0 {
        return Err(Fail::Problem(format!("The installer stopped before it finished (it returned code {code}).")));
    }
    // Step 6: Windows must now list the expected version.
    for attempt in 0..10 {
        if let Some(i) = winsys::installed_apps().get(entry.installed_as.trim()) {
            if crate::catalog::installed_matches(&i.version, &d.version) {
                return Ok(());
            }
            if attempt == 9 {
                return Err(Fail::Problem(format!("The installer finished, but Windows lists version {} instead of {}.", i.version, d.version)));
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err(Fail::Problem(format!("The installer finished, but Windows does not list {} as installed.", entry.name)))
}

/// Run the app's own uninstaller (the one Windows lists). Projects, libraries and data stay.
pub fn start_uninstall(app: &AppHandle, entry: AppEntry) -> Result<(), String> {
    let inst = winsys::installed_apps().get(entry.installed_as.trim()).cloned().ok_or_else(|| format!("Windows does not list {} as installed.", entry.name))?;
    let uninstaller = inst.uninstall.as_deref().and_then(winsys::uninstaller_path).ok_or("Windows does not list an uninstaller for this app.")?;
    let cancel = begin(app, &entry.id, Kind::Uninstall, 0)?;
    let app = app.clone();
    std::thread::spawn(move || {
        let r = (|| {
            let program = view::program_path(&entry, &inst);
            wait_for_close(&app, &entry.id, program.as_deref(), &cancel)?;
            set_stage(&app, &entry.id, Stage::Uninstalling);
            if program.as_deref().map(winsys::is_running).unwrap_or(false) {
                return Err(Fail::Problem(format!("{} was opened again before it could be uninstalled. Close it and press Retry.", entry.name)));
            }
            let code = run_and_wait(&uninstaller, &entry.uninstall_switch)?;
            // The uninstaller usually hands over to a copy of itself and returns at once, so
            // wait for Windows to stop listing the app.
            let until = Instant::now() + Duration::from_secs(180);
            while Instant::now() < until {
                if !winsys::installed_apps().contains_key(entry.installed_as.trim()) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(Fail::Problem(if code != 0 { format!("The uninstaller stopped before it finished (it returned code {code}).") } else { format!("The uninstaller finished, but Windows still lists {} as installed.", entry.name) }))
        })();
        finish(&app, &entry.id, r);
    });
    Ok(())
}

fn free_name(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };
    (1..1000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap_or(p)
}

/// An extra file (a portable copy, a browser add-on): checked against its SHA-256, saved to
/// the person's Downloads folder and shown in File Explorer. Nothing is run.
pub fn start_extra(app: &AppHandle, entry: &AppEntry, extra: Extra) -> Result<(), String> {
    let id = entry.id.clone();
    let cancel = begin(app, &id, Kind::Extra, extra.size)?;
    let app = app.clone();
    std::thread::spawn(move || {
        let r = (|| {
            let dir = winsys::downloads_folder();
            let part = dir.join(format!("{}.part", extra.file_name));
            let sha = fetch(&app, &id, &extra.url, &part, extra.size, &cancel)?;
            set_stage(&app, &id, Stage::Checking);
            if sha != extra.sha256 {
                let _ = std::fs::remove_file(&part);
                return Err(Fail::Problem("The download did not match its checksum, so it was deleted.".into()));
            }
            let dest = free_name(&dir, &extra.file_name);
            std::fs::rename(&part, &dest).map_err(|e| Fail::Problem(format!("The file could not be saved to Downloads ({e}).")))?;
            let _ = winsys::reveal(&dest);
            Ok(())
        })();
        finish(&app, &id, r);
    });
    Ok(())
}

/// The hub's own update: the same checks on its own installer, then the installer replaces the
/// hub and starts it again.
pub fn start_hub_update(app: &AppHandle, d: Download) -> Result<(), String> {
    let cancel = begin(app, HUB, Kind::Hub, d.size)?;
    let app = app.clone();
    std::thread::spawn(move || match download_and_check(&app, HUB, &d, &BRAND.expected_signer, &cancel) {
        Ok((path, file)) => {
            set_stage(&app, HUB, Stage::Installing);
            drop(file);
            // /P: progress bar only; /R: start the hub again when done. The file is removed
            // at the next start.
            match std::process::Command::new(&path).args(["/P", "/R"]).spawn() {
                Ok(_) => app.exit(0),
                Err(e) => {
                    let _ = std::fs::remove_file(&path);
                    finish(&app, HUB, Err(Fail::Problem(format!("Windows could not start the hub's installer ({e})."))));
                }
            }
        }
        Err(e) => finish(&app, HUB, Err(e)),
    });
    Ok(())
}

pub fn cancel(app: &AppHandle, id: &str) {
    if let Some(j) = hub(app).lock().jobs.get(id) {
        if j.view().cancellable {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// The checked installer stays open with sharing limited to reading while it runs: Windows
    /// must still start it, and no one may change or delete it meanwhile.
    #[test]
    fn a_locked_file_still_runs_but_cannot_be_changed() {
        let dir = std::env::temp_dir().join(format!("prism-hub-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("where-copy.exe");
        std::fs::copy(r"C:\Windows\System32\where.exe", &exe).unwrap();
        let (file, sha) = lock_and_hash(&exe).unwrap();
        assert_eq!(sha.len(), 64);
        assert!(std::fs::OpenOptions::new().write(true).open(&exe).is_err(), "writing must be refused while locked");
        assert!(std::fs::remove_file(&exe).is_err(), "deleting must be refused while locked");
        let code = run_and_wait(&exe, "/?").unwrap();
        assert_eq!(code, 0);
        drop(file);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

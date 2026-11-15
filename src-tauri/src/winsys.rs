//! Windows itself: the per-user installed apps list, running programs, the start-up entry,
//! opening programs, folders and web pages.

use std::path::{Path, PathBuf};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    pub key: String,
    pub version: String,
    pub location: Option<PathBuf>,
    pub uninstall: Option<String>,
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

/// The per-user installed apps list, by display name. Machine-wide installs are not listed:
/// the hub only manages the per-user installs it can update without an administrator.
#[cfg(windows)]
pub fn installed_apps() -> std::collections::HashMap<String, Installed> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    let mut out = std::collections::HashMap::new();
    let Ok(root) = winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(UNINSTALL, KEY_READ) else { return out };
    for name in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey_with_flags(&name, KEY_READ) else { continue };
        let Ok(display) = k.get_value::<String, _>("DisplayName") else { continue };
        let version = k.get_value::<String, _>("DisplayVersion").unwrap_or_default();
        let location = k.get_value::<String, _>("InstallLocation").ok().map(|s| unquote(&s)).filter(|s| !s.is_empty()).map(PathBuf::from);
        let uninstall = k.get_value::<String, _>("UninstallString").ok().filter(|s| !s.trim().is_empty());
        out.insert(display.trim().to_string(), Installed { key: name, version: version.trim().to_string(), location, uninstall });
    }
    out
}

#[cfg(not(windows))]
pub fn installed_apps() -> std::collections::HashMap<String, Installed> {
    Default::default()
}

/// The program named in an UninstallString: the quoted path, or everything up to ".exe".
pub fn uninstaller_path(s: &str) -> Option<PathBuf> {
    let s = s.trim();
    let path = if let Some(rest) = s.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let lower = s.to_ascii_lowercase();
        let end = lower.find(".exe")? + 4;
        s[..end].to_string()
    };
    let p = PathBuf::from(path);
    (p.is_absolute() && p.extension().map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false)).then_some(p)
}

/// Full paths of every running program this user can see.
#[cfg(windows)]
pub fn running_programs() -> Vec<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    let mut out = vec![];
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                    out.push(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])));
                }
                let _ = CloseHandle(h);
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
pub fn running_programs() -> Vec<PathBuf> {
    vec![]
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

pub fn is_running(program: &Path) -> bool {
    running_programs().iter().any(|p| same_path(p, program))
}

/// Start a program on its own (the hub does not wait for it or keep a hold on it).
pub fn start_program(program: &Path) -> Result<(), String> {
    if !program.is_file() {
        return Err("The program file is missing from its install folder. Reinstall the app to fix this.".into());
    }
    let mut cmd = std::process::Command::new(program);
    if let Some(dir) = program.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Windows could not start the program ({e})."))
}

/// Open a folder in File Explorer, or a web page in the person's own browser.
#[cfg(windows)]
pub fn shell_open(target: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let r = unsafe { ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL) };
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err("Windows could not open it.".into())
    }
}

#[cfg(not(windows))]
pub fn shell_open(_target: &str) -> Result<(), String> {
    Err("only on Windows".into())
}

/// Show a file selected in File Explorer.
pub fn reveal(file: &Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(format!("/select,{}", file.display())).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// The person's Downloads folder.
#[cfg(windows)]
pub fn downloads_folder() -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as _));
            if !s.is_empty() {
                return PathBuf::from(s);
            }
        }
    }
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Downloads")).unwrap_or_else(std::env::temp_dir)
}

#[cfg(not(windows))]
pub fn downloads_folder() -> PathBuf {
    std::env::temp_dir()
}

/// The per-user start-up entry (no administrator needed). It starts the hub in the tray.
#[cfg(windows)]
pub fn set_start_with_windows(on: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    let name = &crate::brand::BRAND.product_name;
    let (key, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN).map_err(|e| format!("the start-up list could not be changed ({e})"))?;
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        key.set_value(name, &format!("\"{}\" --tray", exe.display())).map_err(|e| e.to_string())
    } else {
        match key.delete_value(name) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

#[cfg(not(windows))]
pub fn set_start_with_windows(_on: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_out_of_an_uninstall_string() {
        assert_eq!(uninstaller_path(r#""C:\Users\a\AppData\Local\X\uninstall.exe""#), Some(PathBuf::from(r"C:\Users\a\AppData\Local\X\uninstall.exe")));
        assert_eq!(uninstaller_path(r#""C:\P Q\unins000.exe" /SILENT"#), Some(PathBuf::from(r"C:\P Q\unins000.exe")));
        assert_eq!(uninstaller_path(r"C:\X\uninstall.exe /S"), Some(PathBuf::from(r"C:\X\uninstall.exe")));
        assert_eq!(uninstaller_path("MsiExec.exe /X{GUID}"), None);
        assert_eq!(uninstaller_path(""), None);
    }
}

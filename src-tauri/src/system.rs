// SPDX-License-Identifier: Apache-2.0
//! What the hub reads from and asks of Windows:
//! - the per-user installed apps list (HKCU\...\Uninstall), which is how it knows what is installed, including apps installed before the hub;
//! - the per-user start-up entry (HKCU\...\Run), for Start with Windows, which needs no administrator;
//! - which programs are running, so it never installs over an open app;
//! - opening a program, a folder or a web page.

use std::path::{Path, PathBuf};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// One entry of Windows' per-user installed apps list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Installed {
    pub display_name: String,
    pub display_version: String,
    pub install_location: Option<PathBuf>,
    pub uninstall_string: String,
    pub quiet_uninstall_string: String,
}

/// A registry path value may be written with quotes around it ("C:\...\OpenExplainer"); this is the folder without them.
pub fn unquote(text: &str) -> String {
    let t = text.trim();
    t.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(t).to_string()
}

/// A command line from the installed apps list split into its program and the rest: "C:\x\uninstall.exe" /S gives (C:\x\uninstall.exe, [/S]).
pub fn split_command(command: &str) -> Option<(PathBuf, Vec<String>)> {
    let c = command.trim();
    let (program, rest) = if let Some(after) = c.strip_prefix('"') {
        let end = after.find('"')?;
        (&after[..end], &after[end + 1..])
    } else {
        // Unquoted: the program runs to the end of ".exe".
        let end = c.to_ascii_lowercase().find(".exe")? + 4;
        (&c[..end], &c[end..])
    };
    let args = rest.split_whitespace().map(str::to_string).collect();
    Some((PathBuf::from(program), args))
}

/// The installed copy of an app: the entry whose DisplayName is `name`. Its folder is InstallLocation, or else the folder of its uninstaller.
pub fn find_installed<'a>(list: &'a [Installed], name: &str) -> Option<&'a Installed> {
    list.iter().find(|i| i.display_name.trim().eq_ignore_ascii_case(name.trim()))
}

impl Installed {
    pub fn folder(&self) -> Option<PathBuf> {
        self.install_location
            .clone()
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| split_command(&self.uninstall_string).and_then(|(p, _)| p.parent().map(Path::to_path_buf)))
    }
}

#[cfg(windows)]
mod reg {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::*;

    pub struct Key(pub HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    pub fn open(parent: HKEY, path: &str, access: REG_SAM_FLAGS) -> Option<Key> {
        let mut key = HKEY::default();
        let path = HSTRING::from(path);
        (unsafe { RegOpenKeyExW(parent, &path, None, access, &mut key) } == ERROR_SUCCESS).then_some(Key(key))
    }

    pub fn subkeys(key: &Key) -> Vec<String> {
        let mut out = Vec::new();
        for index in 0.. {
            let mut name = vec![0u16; 512];
            let mut len = name.len() as u32;
            if unsafe { RegEnumKeyExW(key.0, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) } != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&name[..len as usize]));
        }
        out
    }

    /// A text value (REG_SZ, or REG_EXPAND_SZ expanded), or empty.
    pub fn text(key: &Key, sub: &str, name: &str) -> String {
        let sub = HSTRING::from(sub);
        let name = HSTRING::from(name);
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
        let mut size = 0u32;
        if unsafe { RegGetValueW(key.0, &sub, &name, flags, None, None, Some(&mut size)) } != ERROR_SUCCESS || size == 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut bytes = (buffer.len() * 2) as u32;
        if unsafe { RegGetValueW(key.0, &sub, &name, flags, None, Some(buffer.as_mut_ptr() as *mut _), Some(&mut bytes)) } != ERROR_SUCCESS {
            return String::new();
        }
        let chars = (bytes as usize / 2).min(buffer.len());
        String::from_utf16_lossy(&buffer[..chars]).trim_end_matches('\0').to_string()
    }

    pub fn set_text(key: &Key, name: &str, value: &str) -> bool {
        let name = HSTRING::from(name);
        let data: Vec<u8> = value.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect();
        unsafe { RegSetValueExW(key.0, &name, None, REG_SZ, Some(&data)) == ERROR_SUCCESS }
    }

    pub fn delete(key: &Key, name: &str) {
        let name = HSTRING::from(name);
        unsafe {
            let _ = RegDeleteValueW(key.0, &name);
        }
    }
}

/// Every entry in the per-user installed apps list that has a name.
#[cfg(windows)]
pub fn installed_apps() -> Vec<Installed> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ};
    let Some(root) = reg::open(HKEY_CURRENT_USER, UNINSTALL, KEY_READ) else {
        return vec![];
    };
    reg::subkeys(&root)
        .into_iter()
        .map(|sub| Installed {
            display_name: reg::text(&root, &sub, "DisplayName"),
            display_version: reg::text(&root, &sub, "DisplayVersion"),
            install_location: Some(unquote(&reg::text(&root, &sub, "InstallLocation"))).filter(|s| !s.is_empty()).map(PathBuf::from),
            uninstall_string: reg::text(&root, &sub, "UninstallString"),
            quiet_uninstall_string: reg::text(&root, &sub, "QuietUninstallString"),
        })
        .filter(|i| !i.display_name.is_empty())
        .collect()
}

#[cfg(not(windows))]
pub fn installed_apps() -> Vec<Installed> {
    vec![]
}

/// Start with Windows: the per-user start-up entry, named after the product, that starts the hub in the tray.
#[cfg(windows)]
pub fn set_start_with_windows(name: &str, on: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    let key = reg::open(HKEY_CURRENT_USER, RUN, KEY_SET_VALUE).ok_or("Windows' start-up list could not be opened.")?;
    if on {
        let exe = std::env::current_exe().map_err(|_| "The hub could not find its own program file.")?;
        if !reg::set_text(&key, name, &format!("\"{}\" --tray", exe.display())) {
            return Err("The hub could not be added to Windows' start-up list.".into());
        }
    } else {
        reg::delete(&key, name);
    }
    Ok(())
}

#[cfg(windows)]
pub fn starts_with_windows(name: &str) -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ};
    reg::open(HKEY_CURRENT_USER, RUN, KEY_READ).is_some_and(|k| !reg::text(&k, "", name).is_empty())
}

#[cfg(not(windows))]
pub fn set_start_with_windows(_name: &str, _on: bool) -> Result<(), String> {
    Err("Start with Windows works only on Windows.".into())
}

#[cfg(not(windows))]
pub fn starts_with_windows(_name: &str) -> bool {
    false
}

/// Whether a program with this file name is running from inside `folder` (for this person; other people's programs cannot be read and are
/// not counted).
#[cfg(windows)]
pub fn is_running(folder: &Path, program_file: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;

    let folder = folder.to_string_lossy().trim_end_matches('\\').to_ascii_lowercase();
    let wanted = program_file.to_ascii_lowercase();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more && !found {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase() == wanted {
                if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                    let mut path = vec![0u16; 1024];
                    let mut size = path.len() as u32;
                    if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(path.as_mut_ptr()), &mut size).is_ok() {
                        let full = String::from_utf16_lossy(&path[..size as usize]).to_ascii_lowercase();
                        found = full.starts_with(&format!("{folder}\\"));
                    }
                    let _ = CloseHandle(process);
                }
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
pub fn is_running(_folder: &Path, _program_file: &str) -> bool {
    false
}

/// Asks Windows to open an address with the person's own browser, or a folder in File Explorer (the same as opening it from the Start menu).
#[cfg(windows)]
pub fn shell_open(target: &str) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let result = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Windows could not open it.".into())
    }
}

#[cfg(not(windows))]
pub fn shell_open(_target: &str) -> Result<(), String> {
    Err("Opening works only on Windows.".into())
}

/// Starts a program from its own folder, on its own: the hub does not wait for it, and closing the hub does not close it.
pub fn launch(program: &Path) -> Result<(), String> {
    let folder = program.parent().ok_or("The app's folder could not be found.")?;
    let mut command = std::process::Command::new(program);
    command.current_dir(folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: the app is not tied to the hub.
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    command.spawn().map(|_| ()).map_err(|_| "Windows could not start the app.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_from_the_installed_apps_list_are_split() {
        assert_eq!(
            split_command(r#""C:\Users\a\AppData\Local\OpenExplainer\uninstall.exe""#),
            Some((PathBuf::from(r"C:\Users\a\AppData\Local\OpenExplainer\uninstall.exe"), vec![]))
        );
        assert_eq!(
            split_command(r#""C:\Program Files\x\unins000.exe" /SILENT"#),
            Some((PathBuf::from(r"C:\Program Files\x\unins000.exe"), vec!["/SILENT".into()]))
        );
        assert_eq!(split_command(r"C:\x\uninstall.exe /S"), Some((PathBuf::from(r"C:\x\uninstall.exe"), vec!["/S".into()])));
        assert_eq!(split_command(r#""C:\x\no-end"#), None);
        assert_eq!(unquote(r#""C:\Users\a\AppData\Local\OpenStage Author""#), r"C:\Users\a\AppData\Local\OpenStage Author");
    }

    #[test]
    fn an_app_is_found_by_the_name_windows_lists_and_its_folder_known() {
        let list = vec![
            Installed { display_name: "OpenStage Author".into(), install_location: Some(r"C:\A".into()), ..Default::default() },
            Installed { display_name: "OpenStage Capture Express".into(), uninstall_string: r#""C:\P\CE\unins000.exe""#.into(), ..Default::default() },
        ];
        assert_eq!(find_installed(&list, "openstage author").unwrap().folder(), Some(PathBuf::from(r"C:\A")));
        assert_eq!(find_installed(&list, "OpenStage Capture Express").unwrap().folder(), Some(PathBuf::from(r"C:\P\CE")));
        assert!(find_installed(&list, "OpenStage").is_none(), "a name must match whole");
    }

    /// Reads this computer's own list: cargo test -- --ignored --nocapture
    #[cfg(windows)]
    #[test]
    #[ignore = "reads this computer's installed apps"]
    fn this_computers_installed_apps_are_read() {
        let list = installed_apps();
        // Only the suite's own entries are printed, not everything this person has installed.
        for i in list.iter().filter(|i| i.display_name.starts_with("Open") || i.display_name == "PDF Remediator") {
            println!("{} | {} | {:?} | {}", i.display_name, i.display_version, i.folder(), i.uninstall_string);
        }
        assert!(!list.is_empty());
    }
}

// SPDX-License-Identifier: Apache-2.0
//! The hub's only way onto the network: Windows' own HTTPS code (WinHTTP), as in the OpenStage apps' Check for updates.
//!
//! - HTTPS only, and only to github.com and GitHub's release download hosts. GitHub answers a release download with a redirect to its download
//!   host; each redirect is followed here by hand and its address checked, so nothing outside those hosts is ever reached.
//! - No cookies, no sign-in, nothing about the person: a request carries only what any web request carries (the plain name below as its agent).
//! - The window itself cannot reach the network at all (its content security policy allows only the app's own channel).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const ALLOWED_HOSTS: [&str; 4] =
    ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com", "github-releases.githubusercontent.com"];
#[cfg_attr(not(windows), allow(dead_code))]
/// The hub's name and nothing else (no version, no computer details): "Prism-Hub" or "OpenStage-Hub".
fn user_agent() -> String {
    crate::brand::brand().product_name.replace(' ', "-")
}
#[cfg_attr(not(windows), allow(dead_code))]
const MOST_REDIRECTS: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub enum NetError {
    /// GitHub could not be reached at all: no connection, or nothing answered in time.
    Offline,
    /// GitHub answered, but not with the file.
    Status(u32),
    /// An address outside GitHub's releases.
    Refused(String),
    Cancelled,
    /// More arrived than the catalog said the file is.
    TooLarge,
    /// The answer stopped part way.
    Broken,
    Disk(String),
}

impl NetError {
    pub fn words(&self, what: &str) -> String {
        match self {
            NetError::Offline => "GitHub could not be reached. Check the internet connection and try again.".into(),
            // The catalog's own address is built into the hub, so a 404 there means it is not published yet, or the hub's repository
            // is private (GitHub answers 404 to anyone not signed in).
            NetError::Status(404) if what == "catalog" => {
                "GitHub does not have the list of apps yet (404). Its repository may still be private, or the list not published.".into()
            }
            NetError::Status(404) => format!("GitHub does not have the {what} at the address in the catalog (404)."),
            NetError::Status(code) => format!("GitHub answered with an error ({code}) for the {what}. Try again later."),
            NetError::Refused(host) => format!("The {what} would have come from {host}, which is not GitHub, so nothing was downloaded."),
            NetError::Cancelled => "Cancelled. Nothing was installed.".into(),
            NetError::TooLarge => format!("The {what} was larger than the catalog says, so it was not used."),
            NetError::Broken => format!("The {what} did not arrive whole. Try again."),
            NetError::Disk(why) => format!("The {what} could not be saved on this computer ({why})."),
        }
    }
}

/// An https address split into its host and its path (with any query). Anything else, or an address with a user name or a port, is None.
pub fn split_https(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let cut = rest.find(['/', '?']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(cut);
    if host.is_empty() || host.contains(['@', ':', '\\']) || path.contains(char::is_whitespace) {
        return None;
    }
    let path = if path.is_empty() { "/".to_string() } else if path.starts_with('?') { format!("/{path}") } else { path.to_string() };
    Some((host.to_ascii_lowercase(), path))
}

pub fn allowed(url: &str) -> Result<(String, String), NetError> {
    match split_https(url) {
        Some((host, path)) if ALLOWED_HOSTS.contains(&host.as_str()) => Ok((host, path)),
        Some((host, _)) => Err(NetError::Refused(host)),
        None => Err(NetError::Refused(url.chars().take(60).collect())),
    }
}

/// Where a redirect points: an absolute address, or a path on the same host.
pub fn resolve_location(host: &str, location: &str) -> String {
    if location.starts_with('/') && !location.starts_with("//") {
        format!("https://{host}{location}")
    } else {
        location.to_string()
    }
}

/// A small file (the catalog and its signature) into memory, at most `most` bytes.
pub fn get_bytes(url: &str, most: usize) -> Result<Vec<u8>, NetError> {
    let mut body = Vec::new();
    fetch(url, most as u64, &AtomicBool::new(false), &mut |chunk| {
        body.extend_from_slice(chunk);
        Ok(())
    }, &mut |_, _| {})?;
    Ok(body)
}

/// A file to disk, at most `most` bytes, with progress (bytes so far, bytes expected) and a way to cancel. A file that is not finished is deleted.
pub fn download(url: &str, dest: &Path, most: u64, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), NetError> {
    use std::io::Write;
    let file = std::fs::File::create(dest).map_err(|e| NetError::Disk(e.kind().to_string()))?;
    let mut out = std::io::BufWriter::with_capacity(256 * 1024, file);
    let result = fetch(url, most, cancel, &mut |chunk| out.write_all(chunk).map_err(|e| NetError::Disk(e.kind().to_string())), progress)
        .and_then(|_| out.flush().map_err(|e| NetError::Disk(e.kind().to_string())));
    drop(out);
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

#[cfg(not(windows))]
fn fetch(_: &str, _: u64, _: &AtomicBool, _: &mut dyn FnMut(&[u8]) -> Result<(), NetError>, _: &mut dyn FnMut(u64, u64)) -> Result<(), NetError> {
    Err(NetError::Offline)
}

#[cfg(windows)]
fn fetch(
    url: &str,
    most: u64,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), NetError>,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), NetError> {
    let mut address = url.to_string();
    for _ in 0..=MOST_REDIRECTS {
        let (host, path) = allowed(&address)?;
        match winhttp::request(&host, &path, most, cancel, sink, progress)? {
            winhttp::Outcome::Done => return Ok(()),
            winhttp::Outcome::Redirect(location) => address = resolve_location(&host, &location),
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(NetError::Cancelled);
        }
    }
    Err(NetError::Status(310))
}

#[cfg(windows)]
mod winhttp {
    use super::{user_agent, NetError};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::*;

    pub enum Outcome {
        Done,
        Redirect(String),
    }

    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn opened(raw: *mut c_void) -> Result<Handle, NetError> {
        if raw.is_null() {
            Err(NetError::Offline)
        } else {
            Ok(Handle(raw))
        }
    }

    fn set_number(handle: &Handle, option: u32, value: u32) -> Result<(), NetError> {
        let bytes = value.to_ne_bytes();
        unsafe { WinHttpSetOption(Some(handle.0 as *const c_void), option, Some(&bytes)) }.map_err(|_| NetError::Offline)
    }

    fn status(request: &Handle) -> Result<u32, NetError> {
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut status as *mut u32 as *mut c_void),
                &mut size,
                std::ptr::null_mut(),
            )
        }
        .map_err(|_| NetError::Broken)?;
        Ok(status)
    }

    fn header_text(request: &Handle, which: u32) -> Option<String> {
        let mut buffer = vec![0u16; 4096];
        let mut size = (buffer.len() * 2) as u32;
        unsafe {
            WinHttpQueryHeaders(request.0, which, PCWSTR::null(), Some(buffer.as_mut_ptr() as *mut c_void), &mut size, std::ptr::null_mut())
        }
        .ok()?;
        Some(String::from_utf16_lossy(&buffer[..(size as usize / 2)]))
    }

    /// One GET https://host/path with redirects not followed. A redirect comes back as its Location; a 200 answer is passed to `sink`.
    pub fn request(
        host: &str,
        path: &str,
        most: u64,
        cancel: &AtomicBool,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), NetError>,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Outcome, NetError> {
        let agent = HSTRING::from(user_agent());
        let host_w = HSTRING::from(host);
        let path_w = HSTRING::from(path);
        let verb = HSTRING::from("GET");
        unsafe {
            let session = opened(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0))?;
            // Finding the server, connecting and sending: 15 seconds each. Each wait for more of the answer: 30 seconds.
            WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 30_000).map_err(|_| NetError::Offline)?;
            set_number(&session, WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER)?;
            let connection = opened(WinHttpConnect(session.0, &host_w, INTERNET_DEFAULT_HTTPS_PORT, 0))?;
            let request = opened(WinHttpOpenRequest(connection.0, &verb, &path_w, PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE))?;
            set_number(&request, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES)?;
            WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|_| NetError::Offline)?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| NetError::Offline)?;

            let code = status(&request)?;
            if matches!(code, 301 | 302 | 303 | 307 | 308) {
                return header_text(&request, WINHTTP_QUERY_LOCATION).map(Outcome::Redirect).ok_or(NetError::Broken);
            }
            if code != 200 {
                return Err(NetError::Status(code));
            }
            let length: u64 = header_text(&request, WINHTTP_QUERY_CONTENT_LENGTH).and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            if length > most {
                return Err(NetError::TooLarge);
            }
            let total = if length > 0 { length } else { most };
            let mut so_far = 0u64;
            let mut chunk = vec![0u8; 256 * 1024];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(NetError::Cancelled);
                }
                let mut read = 0u32;
                WinHttpReadData(request.0, chunk.as_mut_ptr() as *mut c_void, chunk.len() as u32, &mut read).map_err(|_| NetError::Broken)?;
                if read == 0 {
                    break;
                }
                so_far += read as u64;
                if so_far > most {
                    return Err(NetError::TooLarge);
                }
                sink(&chunk[..read as usize])?;
                progress(so_far, total);
            }
            if length > 0 && so_far != length {
                return Err(NetError::Broken);
            }
            Ok(Outcome::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_github_release_hosts_over_https_are_allowed() {
        assert_eq!(
            allowed("https://github.com/o/r/releases/download/v1/S.exe").unwrap(),
            ("github.com".into(), "/o/r/releases/download/v1/S.exe".into())
        );
        assert!(allowed("https://release-assets.githubusercontent.com/github-production-release-asset/1?sp=r&sig=x").is_ok());
        assert!(allowed("https://objects.githubusercontent.com/x").is_ok());
        for bad in [
            "http://github.com/o/r",
            "https://github.com.example.org/o/r",
            "https://evil.example/github.com",
            "https://github.com:8443/o/r",
            "https://user@github.com/o/r",
            "ftp://github.com/o/r",
            "https://GITHUB.com.evil/x",
            "https://example.org",
            "",
        ] {
            assert!(allowed(bad).is_err(), "{bad}");
        }
        assert_eq!(allowed("https://GitHub.com/o").unwrap().0, "github.com", "a host in capitals is the same host");
    }

    #[test]
    fn a_redirect_to_a_path_stays_on_the_same_host() {
        assert_eq!(resolve_location("github.com", "/o/r/x"), "https://github.com/o/r/x");
        assert_eq!(resolve_location("github.com", "//evil.example/x"), "//evil.example/x", "and is then refused");
        assert!(allowed(&resolve_location("github.com", "//evil.example/x")).is_err());
    }

    /// Reaches GitHub, so it is run only by hand: cargo test -- --ignored
    #[cfg(windows)]
    #[test]
    #[ignore = "reaches GitHub over the network"]
    fn a_real_release_file_is_followed_through_its_redirect() {
        let bytes = get_bytes(
            "https://github.com/jasonswrench-beep/OpenExplainer/releases/download/v0.2.0-alpha.4/OpenExplainer-0.2.0-alpha.4-Setup.exe.sha256.txt",
            4096,
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("OpenExplainer-0.2.0-alpha.4-Setup.exe"));
        assert_eq!(get_bytes("https://example.org/", 4096), Err(NetError::Refused("example.org".into())));
    }
}

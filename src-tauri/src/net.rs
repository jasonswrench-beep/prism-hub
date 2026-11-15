//! The only network code in the hub: HTTPS GETs to GitHub's release hosts.
//!
//! Redirects are followed by hand so every hop is checked: HTTPS only, and only GitHub and its
//! release download hosts. Requests carry no cookies, no identifier and nothing about the
//! computer beyond what any web request sends.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Hosts a download (installer or catalog) may come from.
const DOWNLOAD_HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

fn parse_https(u: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(u).map_err(|_| "the address is not valid".to_string())?;
    if url.scheme() != "https" {
        return Err("the address does not use HTTPS".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err("the address carries a user name or a port".into());
    }
    Ok(url)
}

pub fn check_download_url(u: &str) -> Result<url::Url, String> {
    let url = parse_https(u)?;
    match url.host_str() {
        Some(h) if DOWNLOAD_HOSTS.contains(&h) => Ok(url),
        _ => Err("downloads are accepted only from github.com and its release download host".into()),
    }
}

/// A page the hub opens in the person's own browser: any HTTPS address.
pub fn check_page_url(u: &str) -> Result<url::Url, String> {
    let url = parse_https(u)?;
    if url.host_str().unwrap_or("").is_empty() {
        return Err("the address has no host".into());
    }
    Ok(url)
}

fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("secure connections are not available ({e})"))?;
    Ok(ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("PrismHub/{}", crate::brand::VERSION))
        .tls_connector(Arc::new(tls))
        .try_proxy_from_env(true)
        .build())
}

#[derive(Debug)]
pub enum NetError {
    /// No connection at all (no network, DNS failed, the host could not be reached).
    Offline(String),
    /// Connected, but the answer was not what was expected.
    Failed(String),
    Cancelled,
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Offline(s) | NetError::Failed(s) => f.write_str(s),
            NetError::Cancelled => f.write_str("cancelled"),
        }
    }
}

fn get(start: &str) -> Result<ureq::Response, NetError> {
    let agent = agent().map_err(NetError::Failed)?;
    let mut url = check_download_url(start).map_err(NetError::Failed)?;
    for _ in 0..6 {
        let resp = match agent.request_url("GET", &url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => return Err(NetError::Failed("it is not on GitHub".into())),
            Err(ureq::Error::Status(code, _)) => return Err(NetError::Failed(format!("the server answered {code}"))),
            Err(ureq::Error::Transport(t)) => return Err(NetError::Offline(t.to_string())),
        };
        if (300..400).contains(&resp.status()) {
            let next = resp.header("location").ok_or_else(|| NetError::Failed("a redirect had no address".into()))?;
            let next = url.join(next).map_err(|_| NetError::Failed("a redirect address was not valid".into()))?;
            url = check_download_url(next.as_str()).map_err(NetError::Failed)?;
            continue;
        }
        return Ok(resp);
    }
    Err(NetError::Failed("too many redirects".into()))
}

/// A small file (the catalog or its signature), at most `limit` bytes.
pub fn fetch_small(url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
    let resp = get(url)?;
    let mut out = vec![];
    resp.into_reader().take(limit + 1).read_to_end(&mut out).map_err(|e| NetError::Offline(e.to_string()))?;
    if out.len() as u64 > limit {
        return Err(NetError::Failed("the file is larger than expected".into()));
    }
    Ok(out)
}

/// Download to `dest`, reporting (received, total) and stopping when `cancel` is set or the
/// file grows past `expected_size`. Returns the SHA-256 of what was written.
pub fn download(url: &str, dest: &Path, expected_size: u64, cancel: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> Result<String, NetError> {
    let resp = get(url)?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| NetError::Failed(format!("the download folder could not be written ({e})")))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(NetError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| NetError::Offline(e.to_string()))?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > expected_size {
            return Err(NetError::Failed("the download is larger than the catalog says".into()));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| NetError::Failed(format!("the download could not be saved ({e})")))?;
        progress(received, expected_size);
    }
    file.sync_all().map_err(|e| NetError::Failed(format!("the download could not be saved ({e})")))?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_github_release_hosts_over_https() {
        assert!(check_download_url("https://github.com/o/r/releases/download/v1/a.exe").is_ok());
        assert!(check_download_url("https://release-assets.githubusercontent.com/x").is_ok());
        assert!(check_download_url("http://github.com/o/r").is_err());
        assert!(check_download_url("https://github.com.evil.example/x").is_err());
        assert!(check_download_url("https://user@github.com/x").is_err());
        assert!(check_download_url("https://github.com:444/x").is_err());
        assert!(check_page_url("https://prism.example/docs").is_ok());
        assert!(check_page_url("file:///c:/windows").is_err());
        assert!(check_page_url("javascript:alert(1)").is_err());
    }

    /// Steps 1 to 3 against a real release, without installing anything. Opt in with
    /// PRISM_HUB_NET_TESTS=1 (it downloads about 8 MB).
    #[test]
    fn real_release_downloads_matches_its_checksum_and_signature() {
        if std::env::var("PRISM_HUB_NET_TESTS").as_deref() != Ok("1") {
            return;
        }
        let url = "https://github.com/jasonswrench-beep/prism_reference_manager/releases/download/v0.1.2/Prism-Reference-Manager-0.1.2-setup.exe";
        let sha = "ab7ad53521b79fab475f7f5457c1b753f781d3144cde3301b741d9a3452638cb";
        let size = 8713432;
        let dir = std::env::temp_dir().join(format!("prism-hub-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("setup.exe");
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let got = download(url, &dest, size, &cancel, |_, _| calls += 1).expect("download");
        assert_eq!(got, sha);
        assert!(calls > 0);
        crate::authenticode::check(&dest, "Jason Wrench").expect("signature");
        // A file larger than the catalog says is refused part way.
        assert!(matches!(download(url, &dest, 1000, &cancel, |_, _| {}), Err(NetError::Failed(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

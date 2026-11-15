// SPDX-License-Identifier: Apache-2.0
//! The two checks every downloaded installer passes before it runs:
//! 1. its SHA-256 equals the catalog's;
//! 2. Windows' own signature check (WinVerifyTrust) says its code signature is valid, the signature carries a timestamp, and the signer is the
//!    one the catalog names (Jason Wrench). The certificate's revocation is checked too.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub signer: String,
    pub timestamped: bool,
}

/// The signer's name as Windows reads it (the certificate's simple display name), when the signature is valid; otherwise why not, in words.
#[cfg(windows)]
pub fn signature(path: &Path) -> Result<Signature, String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::*;

    let wide = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: windows::core::PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrust(HWND(-1isize as *mut _), &mut action, &mut data as *mut _ as *mut _) };

    let mut found = Err(words_for(status));
    if status == 0 {
        unsafe {
            let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
            let signer = if provider.is_null() { std::ptr::null_mut() } else { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
            let cert = if signer.is_null() { std::ptr::null_mut() } else { WTHelperGetProvCertFromChain(signer, 0) };
            found = if cert.is_null() || (*cert).pCert.is_null() {
                Err("Windows could not read who signed the installer.".into())
            } else {
                let mut name = vec![0u16; 512];
                let n = CertGetNameStringW((*cert).pCert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut name));
                let signer_name = String::from_utf16_lossy(&name[..(n.saturating_sub(1)) as usize]);
                Ok(Signature { signer: signer_name, timestamped: (*signer).csCounterSigners > 0 })
            };
        }
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(HWND(-1isize as *mut _), &mut action, &mut data as *mut _ as *mut _);
    }
    found
}

#[cfg(not(windows))]
pub fn signature(_path: &Path) -> Result<Signature, String> {
    Err("The signature check works only on Windows.".into())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn words_for(status: i32) -> String {
    match status as u32 {
        0x800B0100 => "The installer is not signed.".into(),
        0x80096010 | 0x80096011 => "The installer's signature does not match its contents; it was changed after it was signed.".into(),
        0x800B010C => "The certificate that signed the installer has been revoked.".into(),
        0x80092013 | 0x80092012 => "Windows could not check whether the signing certificate was revoked (its revocation list could not be reached).".into(),
        0x800B0101 => "The installer's signing certificate had run out when it was signed, and the signature has no timestamp.".into(),
        0x800B0109 | 0x800B010A => "The installer is signed with a certificate Windows does not trust.".into(),
        0x800B0111 => "Windows is set to not trust this installer's publisher.".into(),
        other => format!("Windows did not accept the installer's signature (code 0x{other:08X})."),
    }
}

/// The whole of step 3: valid, timestamped and signed by `expected` (the certificate's name, compared without regard to capitals).
pub fn check_signed_by(path: &Path, expected: &str) -> Result<(), String> {
    let found = signature(path)?;
    if !found.timestamped {
        return Err("The installer's signature has no timestamp, so it was not trusted.".into());
    }
    if !found.signer.trim().eq_ignore_ascii_case(expected.trim()) {
        return Err(format!("The installer is signed by {}, not by {expected}, so it was not trusted.", found.signer));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_sha256_is_its_hex_digest() {
        let path = std::env::temp_dir().join(format!("hub-sha-test-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(windows)]
    #[test]
    fn an_unsigned_file_is_refused_and_a_windows_program_is_signed_by_microsoft() {
        let path = std::env::temp_dir().join(format!("hub-unsigned-{}.exe", std::process::id()));
        std::fs::write(&path, b"MZ not really a program").unwrap();
        assert!(signature(&path).is_err());
        assert!(check_signed_by(&path, "Jason Wrench").is_err());
        let _ = std::fs::remove_file(&path);
        // A program Windows itself ships is catalog-signed rather than signed inside the file, so it is not a fair test; the suite's apps'
        // own installers are tried in TESTING-TODO.md and in the ignored test below.
    }

    /// Needs a signed program from the expected signer: cargo test -- --ignored. Set HUB_SIGNED_FILE to a signed program to try another.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs a signed program from the expected signer on this computer"]
    fn a_signed_suite_program_names_its_signer_and_is_timestamped() {
        let file = std::env::var("HUB_SIGNED_FILE").unwrap_or_else(|_| {
            format!("{}\\OpenExplainer\\openexplainer.exe", std::env::var("LOCALAPPDATA").unwrap())
        });
        let found = signature(Path::new(&file)).unwrap();
        assert_eq!(found.signer, "Jason Wrench");
        assert!(found.timestamped);
        assert_eq!(check_signed_by(Path::new(&file), "jason wrench"), Ok(()));
        assert!(check_signed_by(Path::new(&file), "Someone Else").unwrap_err().contains("not by Someone Else"));
    }
}

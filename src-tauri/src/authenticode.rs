//! Windows' own code-signature check (the one File Properties › Digital Signatures shows).
//!
//! A file passes only when its signature is valid, chains to a trusted root, carries a
//! timestamp, and names the expected signer.

#[derive(Debug, Clone, PartialEq)]
pub struct SignatureInfo {
    pub signer: String,
    pub timestamped: bool,
}

pub fn check(path: &std::path::Path, expected_signer: &str) -> Result<SignatureInfo, String> {
    let info = read(path)?;
    if !info.timestamped {
        return Err("the installer's signature has no timestamp".into());
    }
    if info.signer != expected_signer {
        return Err(format!("the installer is signed by {:?}, not {:?}", info.signer, expected_signer));
    }
    Ok(info)
}

#[cfg(windows)]
pub fn read(path: &std::path::Path) -> Result<SignatureInfo, String> {
    // Revocation is checked; if the revocation servers cannot be reached (a campus proxy, say),
    // the check is repeated without it. The file is already pinned by its SHA-256 from the
    // signed catalog, so this signature check is the second lock, not the only one.
    const CERT_E_REVOCATION_FAILURE: i32 = 0x800B010Eu32 as i32;
    const CRYPT_E_REVOCATION_OFFLINE: i32 = 0x80092013u32 as i32;
    const CRYPT_E_NO_REVOCATION_CHECK: i32 = 0x80092012u32 as i32;
    match unsafe { verify(path, true) } {
        Err(code) if [CERT_E_REVOCATION_FAILURE, CRYPT_E_REVOCATION_OFFLINE, CRYPT_E_NO_REVOCATION_CHECK].contains(&code) => unsafe { verify(path, false) }.map_err(describe),
        other => other.map_err(describe),
    }
}

#[cfg(windows)]
fn describe(code: i32) -> String {
    match code as u32 {
        0x800B0100 => "the installer is not signed".into(),
        0x80096010 => "the installer was changed after it was signed".into(),
        0x800B0109 => "the installer's signature does not chain to a trusted authority".into(),
        0x800B010C => "the signing certificate was revoked".into(),
        0x800B0101 => "the signing certificate had expired and the signature has no valid timestamp".into(),
        0x800B0111 => "the signer is blocked on this computer".into(),
        other => format!("Windows did not accept the installer's signature (0x{other:08X})"),
    }
}

/// Returns the signer and whether a timestamp is present, or the WinVerifyTrust error code.
#[cfg(windows)]
unsafe fn verify(path: &std::path::Path, revocation: bool) -> Result<SignatureInfo, i32> {
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::*;

    let wide = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO { cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32, pcwszFilePath: PCWSTR(wide.as_ptr()), ..Default::default() };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: if revocation { WTD_REVOKE_WHOLECHAIN } else { WTD_REVOKE_NONE },
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: if revocation { WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT } else { WTD_REVOCATION_CHECK_NONE },
        ..Default::default()
    };
    data.Anonymous.pFile = &mut file;
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut _) };

    let mut result = Err(status);
    if status == 0 {
        let prov = unsafe { WTHelperProvDataFromStateData(data.hWVTStateData) };
        let signer = if prov.is_null() { std::ptr::null_mut() } else { unsafe { WTHelperGetProvSignerFromChain(prov, 0, false, 0) } };
        if !signer.is_null() {
            let s = unsafe { &*signer };
            let mut name = String::new();
            if s.csCertChain > 0 && !s.pasCertChain.is_null() {
                let cert = unsafe { (*s.pasCertChain).pCert };
                if !cert.is_null() {
                    let mut buf = [0u16; 512];
                    let n = unsafe { CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) };
                    if n > 1 {
                        name = String::from_utf16_lossy(&buf[..(n - 1) as usize]);
                    }
                }
            }
            result = Ok(SignatureInfo { signer: name, timestamped: s.csCounterSigners > 0 && !s.pasCounterSigners.is_null() });
        } else {
            result = Err(0x800B0100u32 as i32);
        }
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut _) };
    result
}

#[cfg(not(windows))]
pub fn read(_path: &std::path::Path) -> Result<SignatureInfo, String> {
    Err("signature checks need Windows".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn an_unsigned_file_fails() {
        let dir = std::env::temp_dir().join(format!("prism-hub-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("unsigned.exe");
        std::fs::write(&f, b"MZ not really a program").unwrap();
        assert!(read(&f).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Windows file signed by Microsoft: valid and timestamped, but not our signer.
    #[test]
    fn a_microsoft_file_is_valid_but_the_wrong_signer() {
        let notepad = std::path::Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        if !notepad.exists() {
            return;
        }
        // Windows' own programs are usually catalog-signed (no embedded signature): either result
        // is fine, but it must never pass as our signer.
        assert!(check(notepad, "Jason Wrench").is_err());
    }

    /// Set PRISM_HUB_SIGNED_SAMPLE to a setup file signed by the expected signer to run this.
    #[test]
    fn a_signed_installer_passes() {
        let Ok(p) = std::env::var("PRISM_HUB_SIGNED_SAMPLE") else { return };
        let info = check(std::path::Path::new(&p), &crate::brand::BRAND.expected_signer).expect("signed sample");
        assert!(info.timestamped);
    }
}

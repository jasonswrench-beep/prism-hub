//! Whether only administrators can change a file or folder (the same check Prism Reference
//! Manager uses for its policy file).

use std::path::Path;

/// Accounts that may own or change a policy file: Administrators, SYSTEM, TrustedInstaller.
pub const TRUSTED_SIDS: &[&str] = &["S-1-5-32-544", "S-1-5-18", "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"];
/// Rights that let an account change a file or folder, or replace what is inside a folder.
const WRITE_RIGHTS: u32 = 0x0000_0002 | 0x0000_0004 | 0x0000_0010 | 0x0000_0040 | 0x0000_0100 | 0x0001_0000 | 0x0004_0000 | 0x0008_0000 | 0x1000_0000 | 0x4000_0000;
/// For a folder, adding a file or subfolder does not let anyone change a file an administrator
/// put there; deleting or replacing it does.
const FOLDER_WRITE_RIGHTS: u32 = WRITE_RIGHTS & !(0x0000_0002 | 0x0000_0004 | 0x0000_0010 | 0x0000_0100 | 0x4000_0000);

pub fn only_trusted_can_write(owner: &str, aces: &[(String, u32, bool)], is_dir: bool) -> Result<(), String> {
    if !TRUSTED_SIDS.contains(&owner) {
        return Err(format!("it is owned by {owner}, not by an administrator"));
    }
    let rights = if is_dir { FOLDER_WRITE_RIGHTS } else { WRITE_RIGHTS };
    for (sid, mask, inherit_only) in aces {
        if !inherit_only && mask & rights != 0 && !TRUSTED_SIDS.contains(&sid.as_str()) {
            return Err(format!("{sid} can change it"));
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn admin_only(path: &Path) -> Result<(), String> {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows::Win32::Security::{GetAce, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};
    unsafe fn sid_string(sid: PSID) -> String {
        let mut out = PWSTR::null();
        if unsafe { ConvertSidToStringSidW(sid, &mut out) }.is_err() {
            return "an unknown account".into();
        }
        let s = unsafe { out.to_string() }.unwrap_or_default();
        unsafe {
            let _ = LocalFree(Some(HLOCAL(out.0 as _)));
        }
        s
    }
    let mut owner = PSID::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let err = unsafe { GetNamedSecurityInfoW(&HSTRING::from(path.as_os_str()), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, Some(&mut owner), None, Some(&mut dacl), None, &mut sd) };
    if err.0 != 0 {
        return Err(format!("its permissions could not be read (error {})", err.0));
    }
    let result = (|| {
        if dacl.is_null() {
            return Err("it has no access list, so everyone can change it".to_string());
        }
        let owner_s = unsafe { sid_string(owner) };
        let mut aces = vec![];
        for i in 0..unsafe { (*dacl).AceCount } as u32 {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            if unsafe { GetAce(dacl, i, &mut ace) }.is_err() {
                return Err("its access list could not be read".to_string());
            }
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            if header.AceType != 0 {
                continue; // only access-allowed entries grant rights
            }
            let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
            let sid = PSID(&allowed.SidStart as *const u32 as *mut _);
            aces.push((unsafe { sid_string(sid) }, allowed.Mask, header.AceFlags & 0x08 != 0));
        }
        only_trusted_can_write(&owner_s, &aces, path.is_dir())
    })();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sd.0 as _)));
    }
    result
}

#[cfg(not(windows))]
pub fn admin_only(_path: &Path) -> Result<(), String> {
    Err("policy files are read only on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_users_can_change_is_not_trusted() {
        let admins = "S-1-5-32-544".to_string();
        assert!(only_trusted_can_write("S-1-5-32-544", &[(admins.clone(), 0x1F01FF, false)], false).is_ok());
        assert!(only_trusted_can_write("S-1-5-21-1-2-3-1001", &[], false).is_err());
        assert!(only_trusted_can_write("S-1-5-32-544", &[("S-1-5-32-545".into(), 0x0000_0002, false)], false).is_err());
        // Users may add files to the folder, but not change the one an administrator put there.
        assert!(only_trusted_can_write("S-1-5-32-544", &[("S-1-5-32-545".into(), 0x0000_0002 | 0x0000_0004, false)], true).is_ok());
    }
}

//! Explicit private-root provisioning. Existing directories are never repaired.
use super::*;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::{addr_of, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_NO_TOKEN, HANDLE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::{CreateDirectoryW, FILE_ALL_ACCESS};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
};

struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: each allocation is owned and returned by a LocalFree API.
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: token owns a successful OpenThreadToken/OpenProcessToken handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct UserSid {
    buffer: Vec<usize>,
}
impl UserSid {
    fn current() -> Result<Self, WasiDiagnostic> {
        let mut handle = null_mut();
        // SAFETY: pseudohandles are borrowed; Win32 returns one owned token.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut handle) } == 0
            && (unsafe { GetLastError() } != ERROR_NO_TOKEN
                || unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0)
        {
            return Err(denied());
        }
        let token = Token(handle);
        let mut needed = 0;
        // SAFETY: zero-length query writes only the required byte count.
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
        }
        if needed < std::mem::size_of::<TOKEN_USER>() as u32 || needed > 4096 {
            return Err(denied());
        }
        let mut buffer = vec![0_usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let capacity = buffer.len() * std::mem::size_of::<usize>();
        // SAFETY: usize storage aligns TOKEN_USER and remains stable thereafter.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                capacity as u32,
                &mut needed,
            )
        } == 0
            || needed as usize > capacity
        {
            return Err(denied());
        }
        let result = Self { buffer };
        let offset = (result.as_ptr() as usize)
            .checked_sub(result.buffer.as_ptr() as usize)
            .ok_or_else(denied)?;
        if offset > needed as usize {
            return Err(denied());
        }
        bounded_sid(result.as_ptr(), needed as usize - offset)?;
        Ok(result)
    }
    fn as_ptr(&self) -> PSID {
        // SAFETY: successful bounded GetTokenInformation returned TOKEN_USER;
        // its SID storage is inside this retained aligned allocation.
        unsafe { (*self.buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
    fn text(&self) -> Result<String, WasiDiagnostic> {
        let mut raw = null_mut();
        // SAFETY: bounded valid SID; the API returns an owned terminated string.
        if unsafe { ConvertSidToStringSidW(self.as_ptr(), &mut raw) } == 0 || raw.is_null() {
            return Err(denied());
        }
        let allocation = Allocation(raw.cast());
        let mut length = 0;
        // SAFETY: this OS API returns a terminated string for a valid bounded SID.
        unsafe {
            while length < 256 && *raw.add(length) != 0 {
                length += 1;
            }
        }
        if length == 256 {
            return Err(denied());
        }
        let text = String::from_utf16(unsafe { std::slice::from_raw_parts(raw, length) })
            .map_err(|_| denied())?;
        drop(allocation);
        Ok(text)
    }
}

impl WindowsProtectedApprovalCheckpoint {
    /// Creates only a new single child directory, with its restrictive descriptor
    /// applied by the OS at creation. Never changes parent or existing-root ACLs.
    pub fn create_private_root(
        parent: &Path,
        name: &str,
        owner: &str,
        persistence: WindowsApprovalPersistence,
    ) -> Result<Self, WasiDiagnostic> {
        if owner.is_empty()
            || owner.len() > 4096
            || owner.chars().any(char::is_control)
            || name.is_empty()
            || name.len() > 128
            || name.chars().any(char::is_control)
            || name.ends_with('.')
            || name.ends_with(' ')
            || name.contains(['/', '\\', ':'])
            || !matches!(
                Path::new(name).components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err(denied());
        }
        let parent = parent.canonicalize().map_err(|_| denied())?;
        if !parent.is_dir() {
            return Err(denied());
        }
        let path = parent.join(name);
        let sid = UserSid::current()?;
        let text = sid.text()?;
        let descriptor = descriptor(&format!(
            "O:{text}D:P(A;OICI;FA;;;{text})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
        ))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: descriptor/path remain live. CreateDirectoryW refuses an
        // existing target and applies the descriptor before any child payload.
        if unsafe { CreateDirectoryW(path_wide.as_ptr(), &attributes) } == 0 {
            return Err(denied());
        }
        // On later failure keep this newly created directory for inspection;
        // never delete a possibly externally changed path or silently repair it.
        Self::open_verified_private_root(&path, owner, persistence)
    }

    /// Read-only validation of an existing root; no ACL rewrite or migration.
    pub fn open_verified_private_root(
        root: &Path,
        owner: &str,
        persistence: WindowsApprovalPersistence,
    ) -> Result<Self, WasiDiagnostic> {
        let mut result = Self::open(root, owner, persistence)?;
        result.require_private_dacl = true;
        result.check_private_dacl()?;
        Ok(result)
    }

    pub(super) fn check_private_dacl(&self) -> Result<(), WasiDiagnostic> {
        if !self.require_private_dacl {
            return Ok(());
        }
        verify(&self.directory)
    }

    pub(super) fn check_private_file_dacl(&self, handle: HANDLE) -> Result<(), WasiDiagnostic> {
        if self.require_private_dacl {
            verify_security(handle, false)
        } else {
            Ok(())
        }
    }
}

fn descriptor(value: &str) -> Result<Allocation, WasiDiagnostic> {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let mut raw = null_mut();
    let mut bytes = 0;
    // SAFETY: terminated source and writable output pointers remain live.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), 1, &mut raw, &mut bytes)
    } == 0
        || raw.is_null()
    {
        return Err(denied());
    }
    let allocation = Allocation(raw);
    if bytes == 0 || bytes > 64 * 1024 {
        return Err(denied());
    }
    Ok(allocation)
}

fn bounded_sid(sid: PSID, available: usize) -> Result<(), WasiDiagnostic> {
    if sid.is_null() || available < 8 {
        return Err(denied());
    }
    // SAFETY: caller bounds this pointer in an OS-owned allocation. A SID has
    // an 8-byte header and at most 15 four-byte subauthorities.
    let header = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), 8) };
    if header[1] > 15 || 8 + usize::from(header[1]) * 4 > available {
        return Err(denied());
    }
    // SAFETY: the complete variable-length SID is now bounded.
    if unsafe { IsValidSid(sid) } == 0 {
        return Err(denied());
    }
    Ok(())
}

fn verify(directory: &Dir) -> Result<(), WasiDiagnostic> {
    verify_security(directory.as_raw_handle(), true)
}

fn accepted_owner(owner: PSID, current_user: PSID, directory: bool) -> bool {
    // SAFETY: callers retain valid SIDs from live OS descriptor/token allocations.
    // Elevated tokens may default file ownership to an OS principal already
    // granted full access by our exact DACL. Root ownership must still bind user.
    unsafe {
        EqualSid(owner, current_user) != 0
            || (!directory
                && (IsWellKnownSid(owner, WinLocalSystemSid) != 0
                    || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0))
    }
}

fn verify_security(handle: HANDLE, directory: bool) -> Result<(), WasiDiagnostic> {
    let user = UserSid::current()?;
    let mut owner = null_mut();
    let mut acl = null_mut();
    let mut raw = null_mut();
    // SAFETY: live borrowed handle; outputs belong to one OS allocation.
    if unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut raw,
        )
    } != 0
        || raw.is_null()
    {
        return Err(denied());
    }
    let allocation = Allocation(raw);
    let length = unsafe { GetSecurityDescriptorLength(raw) } as usize;
    if length == 0 || length > 64 * 1024 || owner.is_null() || acl.is_null() {
        return Err(denied());
    }
    if !accepted_owner(owner, user.as_ptr(), directory) {
        return Err(denied());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(raw, &mut control, &mut revision) } == 0
        || (directory && control & SE_DACL_PROTECTED == 0)
    {
        return Err(denied());
    }
    let mut info = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            acl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
        || info.AceCount != 3
        || info.AclBytesInUse as usize > length
    {
        return Err(denied());
    }
    let mut found = [false; 3];
    let inherit_flags = if directory {
        OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
    } else {
        0
    };
    for index in 0..info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(acl, index, &mut ace) } == 0 || ace.is_null() {
            return Err(denied());
        }
        let offset = (ace as usize)
            .checked_sub(raw as usize)
            .ok_or_else(denied)?;
        if offset + std::mem::size_of::<ACCESS_ALLOWED_ACE>() > length {
            return Err(denied());
        }
        // SAFETY: OS-validated ACE is bounded above before any field is read.
        let value = unsafe { std::ptr::read_unaligned(ace.cast::<ACCESS_ALLOWED_ACE>()) };
        if value.Header.AceType != 0
            || value.Mask != FILE_ALL_ACCESS
            || offset + value.Header.AceSize as usize > length
            || value.Header.AceSize < 16
            || (u32::from(value.Header.AceFlags) & !INHERITED_ACE) != inherit_flags
        {
            return Err(denied());
        }
        let sid = unsafe { addr_of!((*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart) }
            .cast_mut()
            .cast();
        bounded_sid(sid, usize::from(value.Header.AceSize) - 8)?;
        if unsafe { EqualSid(sid, user.as_ptr()) } != 0 {
            found[0] = true;
        } else if unsafe { IsWellKnownSid(sid, WinLocalSystemSid) } != 0 {
            found[1] = true;
        } else if unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) } != 0 {
            found[2] = true;
        } else {
            return Err(denied());
        }
    }
    // SYSTEM may itself be the effective user in a service logon.
    if unsafe { IsWellKnownSid(user.as_ptr(), WinLocalSystemSid) } != 0 {
        found[1] = found[0];
    }
    drop(allocation);
    if found.iter().all(|present| *present) {
        Ok(())
    } else {
        Err(denied())
    }
}

fn denied() -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiPermissionDenied,
        "private approval directory security is unsupported or not restricted to the current user and trusted OS principals")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ApprovalStorageIsolation;
    use windows_sys::Win32::Security::Authorization::SetNamedSecurityInfoW;

    // Only called for fresh fixture-owned children/files, never the parent.
    fn fixture_dacl(root: &Path, sddl: &str, protected: bool) {
        let security = descriptor(sddl).unwrap();
        let mut acl = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        // SAFETY: descriptor is a live OS allocation and outputs are writable.
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(security.0, &mut present, &mut acl, &mut defaulted)
            },
            0
        );
        assert_ne!(present, 0);
        let mut wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        let mode = if protected {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        // SAFETY: exact test-owned path and bounded live descriptor; this fixture
        // deliberately changes only its new child's DACL to exercise rejection.
        assert_eq!(
            unsafe {
                SetNamedSecurityInfoW(
                    wide.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | mode,
                    null_mut(),
                    null_mut(),
                    acl,
                    null_mut(),
                )
            },
            0
        );
    }

    fn restricted() -> String {
        let sid = UserSid::current().unwrap().text().unwrap();
        format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")
    }

    #[test]
    fn elevated_file_owner_compatibility_retains_current_user_root_identity() {
        let user = UserSid::current().unwrap();
        for (owner_name, trusted_file) in [
            ("SY", true),
            ("BA", true),
            ("WD", false),
            ("AU", false),
            ("BU", false),
        ] {
            let security = descriptor(&format!("O:{owner_name}")).unwrap();
            let mut owner = null_mut();
            let mut defaulted = 0;
            // SAFETY: OS-parsed descriptor with an explicit live owner SID.
            assert_ne!(
                unsafe { GetSecurityDescriptorOwner(security.0, &mut owner, &mut defaulted) },
                0
            );
            let is_user = unsafe { EqualSid(owner, user.as_ptr()) } != 0;
            assert_eq!(
                accepted_owner(owner, user.as_ptr(), false),
                trusted_file || is_user
            );
            assert_eq!(accepted_owner(owner, user.as_ptr(), true), is_user);
        }
        let temporary = tempfile::tempdir().unwrap();
        let backend = WindowsProtectedApprovalCheckpoint::create_private_root(
            temporary.path(),
            "private",
            "fixture-default-file-owner",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let file = backend
            .directory
            .open_with("fixture-default-owner.bin", &options)
            .unwrap();
        backend
            .check_private_file_dacl(file.as_raw_handle())
            .unwrap();
        backend.check_private_dacl().unwrap();
        drop(file);
        drop(backend);
        temporary.close().unwrap();
    }

    #[test]
    fn private_root_is_explicit_non_mutating_and_rechecked_after_recovery() {
        let temporary = tempfile::tempdir().unwrap();
        for name in [
            "",
            ".",
            "..",
            "nested/child",
            "nested\\child",
            "disk:child",
            "trailing.",
        ] {
            assert!(WindowsProtectedApprovalCheckpoint::create_private_root(
                temporary.path(),
                name,
                "fixture-owner",
                WindowsApprovalPersistence::LogonSession
            )
            .is_err());
        }
        for owner in ["", "\n", &"x".repeat(4097)] {
            assert!(WindowsProtectedApprovalCheckpoint::create_private_root(
                temporary.path(),
                "invalid-owner",
                owner,
                WindowsApprovalPersistence::LogonSession
            )
            .is_err());
            assert!(!temporary.path().join("invalid-owner").exists());
        }
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
        let backend = WindowsProtectedApprovalCheckpoint::create_private_root(
            temporary.path(),
            "private",
            "fixture-owner",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let root = temporary.path().join("private");
        assert!(backend.require_private_dacl);
        verify(&backend.directory).unwrap();
        assert_eq!(backend.directory.entries().unwrap().count(), 0);
        assert!(WindowsProtectedApprovalCheckpoint::create_private_root(
            temporary.path(),
            "private",
            "other-owner",
            WindowsApprovalPersistence::LogonSession
        )
        .is_err());
        verify(&backend.directory).unwrap();
        let retained = backend.duplicate().unwrap();
        assert!(retained.require_private_dacl);
        let permissions = crate::permissions::ComponentPermissions::deny_all();
        backend
            .check_permissions(&permissions, Instant::now() + Duration::from_secs(2))
            .unwrap();
        let restricted = restricted();
        let broad = format!("{restricted}(A;OICI;GR;;;WD)");
        fixture_dacl(&root, &broad, true);
        assert!(backend.check_private_dacl().is_err());
        assert!(retained.check_private_dacl().is_err());
        assert!(backend
            .check_permissions(&permissions, Instant::now() + Duration::from_secs(2))
            .is_err());
        assert!(
            WindowsProtectedApprovalCheckpoint::open_verified_private_root(
                &root,
                "fixture-owner",
                WindowsApprovalPersistence::LogonSession
            )
            .is_err()
        );
        // Opening a bad root never fixes it or writes a credential/lock file.
        assert!(backend.check_private_dacl().is_err());
        assert!(backend.load().is_err());
        assert!(backend
            .lock(Instant::now() + Duration::from_secs(2))
            .is_err());
        assert_eq!(backend.directory.entries().unwrap().count(), 0);
        fixture_dacl(&root, &restricted, false);
        assert!(backend.check_private_dacl().is_err());
        fixture_dacl(&root, &restricted.replace(";;;BA)", ";;;WD)"), true);
        assert!(backend.check_private_dacl().is_err());
        fixture_dacl(&root, &restricted, true);
        backend.check_private_dacl().unwrap();
        retained.check_private_dacl().unwrap();
        backend
            .check_permissions(&permissions, Instant::now() + Duration::from_secs(2))
            .unwrap();
        drop(retained);
        drop(backend);
        temporary.close().unwrap();
    }

    #[test]
    #[ignore = "explicit owned private-root fixture and exact current-user credential; cleaned on exit"]
    fn native_private_root_recovery_keeps_consent_and_selected_checkpoint() {
        struct OwnedCredential<'a>(&'a WindowsProtectedApprovalCheckpoint);
        impl Drop for OwnedCredential<'_> {
            fn drop(&mut self) {
                let _ = self.0.remove_test_credential();
            }
        }
        let temporary = tempfile::tempdir().unwrap();
        let backend = WindowsProtectedApprovalCheckpoint::create_private_root(
            temporary.path(),
            "private",
            "fixture-private-recovery",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let root = temporary.path().join("private");
        let owned = OwnedCredential(&backend);
        assert!(backend.load().unwrap().is_none());
        let key = ApprovalPolicyKey::generate().unwrap();
        let policies = RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(
            *key.secret_bytes(),
        ))
        .unwrap();
        let request = crate::ApprovalRequest {
            schema_version: 1,
            id: "a".repeat(64),
            plugin_id: "fixture.component".into(),
            plugin_version: "1.0.0".into(),
            component_sha256: "b".repeat(64),
            host_context_sha256: "c".repeat(64),
            remembered_binding_sha256: Some("d".repeat(64)),
            action: crate::SensitiveAction::FileWrite {
                grant_id: "path-0".into(),
                root: "fixture-root".into(),
                relative_path: "one.txt".into(),
                target_sha256: None,
                payload_sha256: "e".repeat(64),
                payload_bytes: 1,
            },
        };
        policies
            .remember_request(
                &request,
                crate::ApprovalScope::Exact,
                Duration::from_secs(3600),
            )
            .unwrap();
        let candidate = policies.seal().unwrap();
        backend
            .initialize(&key, &candidate, Instant::now() + Duration::from_secs(3))
            .unwrap();
        let restored = backend
            .restore_policies(Instant::now() + Duration::from_secs(3))
            .unwrap()
            .unwrap();
        let permit = restored.authorize(&request).unwrap().unwrap();
        for _ in 0..3 {
            restored
                .authorize(&request)
                .unwrap()
                .unwrap()
                .check()
                .unwrap();
        }
        let before = backend.load().unwrap().unwrap().checkpoint;
        let restricted = restricted();
        fixture_dacl(&root, &format!("{restricted}(A;OICI;GR;;;WD)"), true);
        assert!(backend.load().is_err());
        assert!(permit.check().is_err());
        assert!(backend
            .restore_policies(Instant::now() + Duration::from_secs(3))
            .is_err());
        assert!(backend
            .observe_time(Instant::now() + Duration::from_secs(3))
            .is_err());
        fixture_dacl(&root, &restricted, true);
        permit.check().unwrap();
        restored
            .authorize(&request)
            .unwrap()
            .unwrap()
            .check()
            .unwrap();
        let after = backend.load().unwrap().unwrap().checkpoint;
        assert!(same_snapshot(&before, &after));
        let snapshot = root.join(backend.snapshot_name(&after));
        let file_restricted = restricted.replace("OICI", "");
        fixture_dacl(&snapshot, &format!("{file_restricted}(A;;GR;;;WD)"), true);
        // A safe root cannot hide an explicitly overbroad selected file.
        backend.check_private_dacl().unwrap();
        assert!(backend
            .read_snapshot(&after, Instant::now() + Duration::from_secs(3))
            .is_err());
        assert!(permit.check().is_err());
        fixture_dacl(&snapshot, &file_restricted, true);
        permit.check().unwrap();
        restored
            .authorize(&request)
            .unwrap()
            .unwrap()
            .check()
            .unwrap();
        backend
            .restore_policies(Instant::now() + Duration::from_secs(3))
            .unwrap()
            .unwrap();
        drop(restored);
        drop(permit);
        backend.remove_test_credential().unwrap();
        assert!(backend.load().unwrap().is_none());
        drop(owned);
        drop(backend);
        temporary.close().unwrap();
    }
}

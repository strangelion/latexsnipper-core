//! Physical directory exclusion for the opt-in protected Windows owner.
use super::*;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_GUID};

impl crate::ApprovalStorageIsolation for WindowsProtectedApprovalCheckpoint {
    fn check_permissions(
        &self,
        permissions: &crate::permissions::ComponentPermissions,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        check_deadline(deadline)?;
        self.check_private_dacl()?;
        if permissions.filesystem.is_empty() {
            return check_deadline(deadline);
        }
        if permissions.filesystem.len() > 128 {
            return Err(denied());
        }
        let private = identity(&self.directory)?;
        let parents = ancestors(&self.directory, deadline)?;
        for grant in permissions.filesystem.values() {
            check_deadline(deadline)?;
            let granted = identity(grant.directory())?;
            if granted.0 != private.0 {
                continue;
            }
            if parents.contains(&granted)
                || ancestors(grant.directory(), deadline)?.contains(&private)
            {
                return Err(denied());
            }
        }
        check_deadline(deadline)
    }
}

fn identity(directory: &Dir) -> Result<(u64, u64), WasiDiagnostic> {
    let metadata = directory.dir_metadata().map_err(|_| denied())?;
    if !metadata.is_dir() || metadata.ino() == 0 {
        return Err(denied());
    }
    Ok((metadata.dev(), metadata.ino()))
}

fn physical_path(directory: &Dir) -> Result<PathBuf, WasiDiagnostic> {
    // SAFETY: the borrowed live directory handle belongs to this capability;
    // the first call queries size, the second writes only within our buffer.
    let needed = unsafe {
        GetFinalPathNameByHandleW(
            directory.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            VOLUME_NAME_GUID,
        )
    };
    if needed == 0 || needed > 32768 {
        return Err(denied());
    }
    let mut buffer = vec![0_u16; needed as usize];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            directory.as_raw_handle(),
            buffer.as_mut_ptr(),
            needed,
            VOLUME_NAME_GUID,
        )
    };
    if count == 0 || count >= needed {
        return Err(denied());
    }
    buffer.truncate(count as usize);
    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

fn ancestors(directory: &Dir, deadline: Instant) -> Result<Vec<(u64, u64)>, WasiDiagnostic> {
    let path = physical_path(directory)?;
    let mut result = Vec::new();
    for ancestor in path.ancestors().filter(|path| {
        path.components()
            .any(|part| matches!(part, Component::RootDir))
    }) {
        check_deadline(deadline)?;
        if result.len() >= 128 {
            return Err(denied());
        }
        let opened =
            Dir::open_ambient_dir(ancestor, cap_std::ambient_authority()).map_err(|_| denied())?;
        result.push(identity(&opened)?);
    }
    check_deadline(deadline)?;
    if result.first() != Some(&identity(directory)?) {
        return Err(denied());
    }
    Ok(result)
}

fn denied() -> WasiDiagnostic {
    WasiDiagnostic::new(
        WasiDiagnosticCode::PluginWasiPermissionDenied,
        "private approval storage overlaps a guest grant or isolation cannot be verified",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::ComponentPermissions;
    use crate::ApprovalStorageIsolation;

    fn permissions(root: &Path, relative: &str) -> ComponentPermissions {
        let manifest: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": relative, "access": "read"}],
            "temporaryDirectory": false, "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        ComponentPermissions::from_manifest(&manifest, root).unwrap()
    }

    #[test]
    fn physical_private_root_rejects_ancestors_descendants_and_spoofed_labels() {
        let temporary = tempfile::tempdir().unwrap();
        for directory in ["private/nested", "workspace", "private-other"] {
            std::fs::create_dir_all(temporary.path().join(directory)).unwrap();
        }
        let owner = WindowsProtectedApprovalCheckpoint::open(
            &temporary.path().join("private"),
            "fixture-isolation",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        for relative in [".", "private", "PRIVATE", "private/nested"] {
            assert_eq!(
                owner
                    .check_permissions(
                        &permissions(temporary.path(), relative),
                        Instant::now() + Duration::from_secs(5)
                    )
                    .unwrap_err()
                    .code,
                WasiDiagnosticCode::PluginWasiPermissionDenied
            );
        }
        for relative in ["workspace", "private-other"] {
            owner
                .check_permissions(
                    &permissions(temporary.path(), relative),
                    Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
        }
        let mut spoofed = permissions(temporary.path(), "private");
        spoofed.filesystem.get_mut("path-0").unwrap().root = temporary.path().join("workspace");
        assert!(owner
            .check_permissions(&spoofed, Instant::now() + Duration::from_secs(5))
            .is_err());
        assert!(owner
            .check_permissions(&ComponentPermissions::deny_all(), Instant::now())
            .is_err());
        drop(spoofed);
        drop(owner);
        temporary.close().unwrap();
    }

    #[test]
    fn protected_reads_do_not_traverse_a_junction_to_private_storage() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("private")).unwrap();
        std::fs::create_dir(temporary.path().join("workspace")).unwrap();
        std::fs::write(
            temporary.path().join("private/secret.txt"),
            b"synthetic private fixture",
        )
        .unwrap();
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:CORE_TEST_JUNCTION -Target $env:CORE_TEST_JUNCTION_TARGET | Out-Null"])
            .env("CORE_TEST_JUNCTION", temporary.path().join("workspace/alias"))
            .env("CORE_TEST_JUNCTION_TARGET", temporary.path().join("private"))
            .creation_flags(0x0800_0000).stdout(Stdio::null()).stderr(Stdio::piped()).output().unwrap();
        assert!(
            result.status.success(),
            "owned junction fixture creation failed"
        );
        let owner = WindowsProtectedApprovalCheckpoint::open(
            &temporary.path().join("private"),
            "fixture-junction",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let grants = permissions(temporary.path(), "workspace");
        owner
            .check_permissions(&grants, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert!(grants
            .read_file_no_follow_parents("path-0", "alias/secret.txt", 1024)
            .is_err());
        assert_eq!(
            std::fs::read(temporary.path().join("private/secret.txt")).unwrap(),
            b"synthetic private fixture"
        );
        drop(grants);
        drop(owner);
        temporary.close().unwrap();
    }
}

//! Security metadata for owned staging files, never for ambient user targets.
use crate::permissions::FilesystemOperationError;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File, OpenOptions};
use std::path::Path;

pub(crate) struct WriteSecurity {
    #[cfg(windows)]
    snapshot: Option<windows::SecuritySnapshot>,
}

impl WriteSecurity {
    pub(crate) fn read(parent: &Dir, target: &Path) -> Result<Self, FilesystemOperationError> {
        #[cfg(windows)]
        {
            Ok(Self {
                snapshot: windows::read(parent, target)?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (parent, target);
            Ok(Self {})
        }
    }

    pub(crate) fn create(&self, parent: &Dir, name: &str) -> std::io::Result<File> {
        #[cfg(windows)]
        if let Some(snapshot) = &self.snapshot {
            return snapshot.create(parent, name);
        }
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        parent.open_with(name, &options)
    }

    pub(crate) fn recheck(
        &self,
        parent: &Dir,
        target: &Path,
    ) -> Result<(), FilesystemOperationError> {
        #[cfg(windows)]
        {
            let current = windows::read(parent, target)?;
            if self.snapshot.as_ref().map(|snapshot| &snapshot.fingerprint)
                != current.as_ref().map(|snapshot| &snapshot.fingerprint)
            {
                return Err(FilesystemOperationError::Conflict);
            }
        }
        #[cfg(not(windows))]
        let _ = (parent, target);
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::permissions::map_io_error;
    use sha2::{Digest, Sha256};
    use std::ffi::c_void;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{LocalFree, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SetSecurityInfo,
        SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorControl, GetSecurityDescriptorLength, ACL, DACL_SECURITY_INFORMATION,
        GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
        SE_DACL_PROTECTED, UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFinalPathNameByHandleW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
    };
    const INFO: u32 =
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    const MAX_SECURITY_BYTES: u32 = 64 * 1024;

    struct LocalAllocation(*mut c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            // SAFETY: Only successful Win32 LocalAlloc-returning APIs construct this guard.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    pub(super) struct SecuritySnapshot {
        descriptor: LocalAllocation,
        dacl: *mut ACL,
        protected: bool,
        pub(super) fingerprint: String,
    }

    pub(super) fn read(
        parent: &Dir,
        target: &Path,
    ) -> Result<Option<SecuritySnapshot>, FilesystemOperationError> {
        use cap_std::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .access_mode(FILE_GENERIC_READ | READ_CONTROL)
            .follow(FollowSymlinks::No);
        match parent.open_with(target, &options) {
            Ok(file) => SecuritySnapshot::from_file(&file)
                .map(Some)
                .map_err(map_io_error),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(map_io_error(error)),
        }
    }
    impl SecuritySnapshot {
        fn from_file(file: &File) -> std::io::Result<Self> {
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            let mut dacl: *mut ACL = null_mut();
            // SAFETY: The file handle is live. Win32 initializes the output pointers;
            // ACL/SID pointers remain owned by the returned security descriptor.
            let status = unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    INFO,
                    null_mut::<PSID>(),
                    null_mut::<PSID>(),
                    &mut dacl,
                    null_mut(),
                    &mut descriptor,
                )
            };
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32));
            }
            let descriptor = LocalAllocation(descriptor);
            if descriptor.0.is_null() {
                return Err(std::io::Error::other("empty security descriptor"));
            }
            let mut control = 0_u16;
            let mut revision = 0_u32;
            // SAFETY: GetSecurityInfo produced this live descriptor.
            if unsafe { GetSecurityDescriptorLength(descriptor.0) } > MAX_SECURITY_BYTES {
                return Err(std::io::Error::other("security metadata exceeds budget"));
            }
            // SAFETY: Descriptor and output references have valid lifetimes.
            if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) }
                == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            let fingerprint = fingerprint(descriptor.0)?;
            Ok(Self {
                descriptor,
                dacl,
                protected: control & SE_DACL_PROTECTED != 0,
                fingerprint,
            })
        }
        pub(super) fn create(&self, parent: &Dir, name: &str) -> std::io::Result<File> {
            let path = staging_path(parent, name)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.descriptor.0,
                bInheritHandle: 0,
            };
            // SAFETY: The bounded, terminated path names a random staging file.
            // CREATE_NEW prevents overwriting any pre-existing file. The complete
            // kernel-owned descriptor stays live throughout creation; no payload
            // is written under a broader inherited DACL first.
            let handle = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    &attributes,
                    CREATE_NEW,
                    FILE_ATTRIBUTE_NORMAL,
                    null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: Successful CreateFileW transfers one unique owned handle.
            let file = File::from_std(unsafe { std::fs::File::from_raw_handle(handle) });
            let checked = (|| {
                let protection = if self.protected {
                    PROTECTED_DACL_SECURITY_INFORMATION
                } else {
                    UNPROTECTED_DACL_SECURITY_INFORMATION
                };
                // SAFETY: The exclusively created staging handle and descriptor's
                // DACL are live. This never modifies the original target's ACL.
                let status = unsafe {
                    SetSecurityInfo(
                        file.as_raw_handle(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION | protection,
                        null_mut(),
                        null_mut(),
                        self.dacl,
                        null(),
                    )
                };
                if status != 0 {
                    return Err(std::io::Error::from_raw_os_error(status as i32));
                }
                if Self::from_file(&file)?.fingerprint != self.fingerprint {
                    return Err(std::io::Error::other(
                        "staging security metadata was not preserved",
                    ));
                }
                Ok(())
            })();
            if let Err(error) = checked {
                drop(file);
                let _ = parent.remove_file(name);
                return Err(error);
            }
            Ok(file)
        }
    }
    fn fingerprint(descriptor: PSECURITY_DESCRIPTOR) -> std::io::Result<String> {
        let mut text = null_mut();
        let mut length = 0_u32;
        // SAFETY: Win32 serializes the live descriptor into a LocalAlloc buffer.
        // Raw SIDs/SDDL are never logged or persisted.
        if unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                1,
                INFO,
                &mut text,
                &mut length,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let allocation = LocalAllocation(text.cast());
        if allocation.0.is_null() || length == 0 || length > MAX_SECURITY_BYTES / 2 {
            return Err(std::io::Error::other("security identity exceeds budget"));
        }
        // SAFETY: Win32 returned length UTF-16 units including the terminator.
        let units = unsafe { std::slice::from_raw_parts(text, length as usize) };
        let mut digest = Sha256::new();
        for unit in units {
            digest.update(unit.to_le_bytes());
        }
        Ok(hex::encode(digest.finalize()))
    }
    fn staging_path(parent: &Dir, name: &str) -> std::io::Result<Vec<u16>> {
        if name.is_empty()
            || name.contains(['/', '\\', ':', '\0'])
            || !matches!(
                Path::new(name).components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "staging name is not a single safe component",
            ));
        }
        let mut buffer = vec![0_u16; 32768];
        // SAFETY: The directory handle and initialized buffer are valid. Win32
        // returns the bounded current native path for this pinned directory.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                parent.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if length as usize >= buffer.len() {
            return Err(std::io::Error::other("staging parent path exceeds budget"));
        }
        buffer.truncate(length as usize);
        if buffer.last() != Some(&(b'\\' as u16)) {
            buffer.push(b'\\' as u16);
        }
        buffer.extend(name.encode_utf16());
        buffer.push(0);
        if buffer.len() > 32768 {
            return Err(std::io::Error::other("staging path exceeds budget"));
        }
        Ok(buffer)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use cap_std::ambient_authority;
        use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
        use windows_sys::Win32::Security::GetSecurityDescriptorDacl;

        fn apply_owner_only_dacl(parent: &Dir, target: &str) {
            use cap_std::fs::OpenOptionsExt;
            let mut options = OpenOptions::new();
            options
                .read(true)
                .write(true)
                .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
                .follow(FollowSymlinks::No);
            let file = parent.open_with(target, &options).unwrap();
            let text: Vec<_> = "D:P(A;;FA;;;OW)".encode_utf16().chain(Some(0)).collect();
            let mut descriptor = null_mut();
            // SAFETY: Test-only constant SDDL, terminated input and valid output.
            assert_ne!(
                unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        text.as_ptr(),
                        1,
                        &mut descriptor,
                        null_mut(),
                    )
                },
                0
            );
            let descriptor = LocalAllocation(descriptor);
            let mut dacl = null_mut();
            let mut present = 0;
            let mut defaulted = 0;
            // SAFETY: The parser returned a live descriptor; outputs are valid.
            assert_ne!(
                unsafe {
                    GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted)
                },
                0
            );
            // SAFETY: This changes only a fixture file in its owned TempDir.
            assert_eq!(
                unsafe {
                    SetSecurityInfo(
                        file.as_raw_handle(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                        null_mut(),
                        null_mut(),
                        dacl,
                        null(),
                    )
                },
                0
            );
        }

        #[test]
        fn staging_security_preserves_owner_group_and_inheritance_protection() {
            let temporary = tempfile::tempdir().unwrap();
            std::fs::write(temporary.path().join("target"), b"original").unwrap();
            let parent = Dir::open_ambient_dir(temporary.path(), ambient_authority()).unwrap();
            for protected in [false, true] {
                if protected {
                    apply_owner_only_dacl(&parent, "target");
                }
                let original = read(&parent, Path::new("target")).unwrap().unwrap();
                assert_eq!(original.protected, protected);
                let staged = original.create(&parent, "candidate").unwrap();
                let copied = SecuritySnapshot::from_file(&staged).unwrap();
                assert!(copied.fingerprint == original.fingerprint);
                assert_eq!(copied.protected, original.protected);
                drop(staged);
                parent.remove_file("candidate").unwrap();
            }
            drop(parent);
            temporary.close().unwrap();
        }

        #[test]
        fn changed_security_is_a_conflict_and_unsafe_staging_names_never_create_files() {
            let temporary = tempfile::tempdir().unwrap();
            std::fs::write(temporary.path().join("target"), b"original").unwrap();
            let parent = Dir::open_ambient_dir(temporary.path(), ambient_authority()).unwrap();
            let snapshot = WriteSecurity::read(&parent, Path::new("target")).unwrap();
            std::fs::write(temporary.path().join("occupied"), b"keep").unwrap();
            assert!(snapshot.create(&parent, "occupied").is_err());
            assert_eq!(
                std::fs::read(temporary.path().join("occupied")).unwrap(),
                b"keep"
            );
            for name in ["../escape", "..\\escape", "target:stream", "", "."] {
                assert!(snapshot.create(&parent, name).is_err());
            }
            apply_owner_only_dacl(&parent, "target");
            assert_eq!(
                snapshot.recheck(&parent, Path::new("target")),
                Err(FilesystemOperationError::Conflict)
            );
            assert_eq!(
                std::fs::read(temporary.path().join("target")).unwrap(),
                b"original"
            );
            assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 2);
            drop(parent);
            temporary.close().unwrap();
        }

        #[test]
        fn actual_write_retains_security_and_acl_only_concurrent_change_prevents_publication() {
            use crate::permissions::CheckedWriteError;
            let temporary = tempfile::tempdir().unwrap();
            std::fs::create_dir(temporary.path().join("write")).unwrap();
            let root = temporary.path().join("write");
            std::fs::write(root.join("target"), b"original").unwrap();
            let manifest_permissions: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
                "paths": [{"path": "write", "access": "write"}], "temporaryDirectory": false, "clocks": false, "randomness": false,
                "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
                "limits": {"maxConcurrentExecutions": 1}
            })).unwrap();
            let permissions =
                crate::ComponentPermissions::from_manifest(&manifest_permissions, temporary.path())
                    .unwrap();
            let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
            let original = read(&parent, Path::new("target")).unwrap().unwrap();
            permissions
                .write_file("path-0", "target", b"new", 1024)
                .unwrap();
            assert!(
                read(&parent, Path::new("target"))
                    .unwrap()
                    .unwrap()
                    .fingerprint
                    == original.fingerprint
            );
            let expected = hex::encode(Sha256::digest(b"new"));
            let payload = vec![b'x'; 128 * 1024];
            let mut checkpoints = 0;
            let result = permissions.write_file_checked(
                "path-0",
                "target",
                &payload,
                payload.len(),
                Some(&expected),
                || {
                    checkpoints += 1;
                    if checkpoints == 3 {
                        apply_owner_only_dacl(&parent, "target");
                    }
                    Ok::<_, ()>(())
                },
            );
            assert!(matches!(
                result,
                Err(CheckedWriteError::Filesystem(
                    FilesystemOperationError::Conflict
                ))
            ));
            assert_eq!(std::fs::read(root.join("target")).unwrap(), b"new");
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
            let protected = read(&parent, Path::new("target")).unwrap().unwrap();
            permissions
                .write_file("path-0", "target", b"protected", 1024)
                .unwrap();
            assert!(
                read(&parent, Path::new("target"))
                    .unwrap()
                    .unwrap()
                    .fingerprint
                    == protected.fingerprint
            );
            assert_eq!(std::fs::read(root.join("target")).unwrap(), b"protected");
            drop(permissions);
            drop(parent);
            temporary.close().unwrap();
        }
    }
}

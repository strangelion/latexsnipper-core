//! Finite Windows basic-metadata preservation for owned replacement staging.
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::MetadataExt as WindowsMetadataExt;
use cap_std::fs::{Dir, File, OpenOptions};
use windows_sys::Win32::Storage::FileSystem::{
    FileBasicInfo, GetFileInformationByHandleEx, SetFileInformationByHandle,
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TEMPORARY, FILE_BASIC_INFO,
};

use crate::permissions::{map_io_error, FilesystemOperationError};

const SUPPORTED: u32 = FILE_ATTRIBUTE_READONLY
    | FILE_ATTRIBUTE_HIDDEN
    | FILE_ATTRIBUTE_SYSTEM
    | FILE_ATTRIBUTE_ARCHIVE
    | FILE_ATTRIBUTE_NORMAL
    | FILE_ATTRIBUTE_TEMPORARY
    | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
    | FILE_ATTRIBUTE_COMPRESSED;

#[derive(Debug, PartialEq, Eq)]
struct BasicIdentity {
    creation: i64,
    last_write: i64,
    attributes: u32,
}

#[derive(PartialEq, Eq)]
pub(crate) struct WriteBasicMetadata(Option<BasicIdentity>);

impl WriteBasicMetadata {
    pub(crate) fn read(parent: &Dir, target: &Path) -> Result<Self, FilesystemOperationError> {
        let metadata = match parent.symlink_metadata(target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self(None)),
            Err(error) => return Err(map_io_error(error)),
        };
        if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() != 1 {
            return Err(FilesystemOperationError::InvalidPath);
        }
        // Inspect finite attributes before opening for data read. This is not a
        // guarantee against native races or every cloud/minifilter behavior.
        supported_attributes(metadata.file_attributes())?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        match parent.open_with(target, &options) {
            Ok(file) => Ok(Self(Some(identity(&file)?))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self(None)),
            Err(error) => Err(map_io_error(error)),
        }
    }

    pub(crate) fn prepare(&self, file: &File) -> Result<(), FilesystemOperationError> {
        // Reject an unsupported inherited staging format before writing payload.
        identity(file).map(|_| ())
    }

    pub(crate) fn apply(&self, file: &File) -> Result<(), FilesystemOperationError> {
        if let Some(expected) = &self.0 {
            identity(file)?;
            let value = FILE_BASIC_INFO {
                CreationTime: expected.creation,
                // Do not restore stale write/access/change times on new content.
                // Mark the replacement for backup; compression has its own API.
                FileAttributes: (expected.attributes & !FILE_ATTRIBUTE_COMPRESSED)
                    | FILE_ATTRIBUTE_ARCHIVE,
                ..FILE_BASIC_INFO::default()
            };
            set_basic(file, &value)?;
        }
        self.verify_staging(file)
    }

    fn verify_staging(&self, file: &File) -> Result<(), FilesystemOperationError> {
        let current = identity(file)?;
        if let Some(expected) = &self.0 {
            if current.creation != expected.creation
                || current.attributes != (expected.attributes | FILE_ATTRIBUTE_ARCHIVE)
            {
                return Err(FilesystemOperationError::Conflict);
            }
        }
        Ok(())
    }

    pub(crate) fn recheck_staging(
        &self,
        parent: &Dir,
        target: &Path,
    ) -> Result<(), FilesystemOperationError> {
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = parent.open_with(target, &options).map_err(map_io_error)?;
        self.verify_staging(&file)
    }

    pub(crate) fn recheck(
        &self,
        parent: &Dir,
        target: &Path,
    ) -> Result<(), FilesystemOperationError> {
        match Self::read(parent, target) {
            Ok(current) if current == *self => Ok(()),
            Ok(_)
            | Err(FilesystemOperationError::MetadataUnsupported)
            | Err(FilesystemOperationError::PermissionDenied) => {
                Err(FilesystemOperationError::Conflict)
            }
            Err(error) => Err(error),
        }
    }
}

fn supported_attributes(flags: u32) -> Result<u32, FilesystemOperationError> {
    if flags & !SUPPORTED != 0 {
        return Err(FilesystemOperationError::MetadataUnsupported);
    }
    if flags & FILE_ATTRIBUTE_READONLY != 0 {
        return Err(FilesystemOperationError::PermissionDenied);
    }
    Ok(flags & !FILE_ATTRIBUTE_NORMAL)
}

fn identity(file: &File) -> Result<BasicIdentity, FilesystemOperationError> {
    let metadata = file.metadata().map_err(map_io_error)?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(FilesystemOperationError::InvalidPath);
    }
    let value = basic(file)?;
    // A zero creation value means "unchanged" in the setter, not epoch zero.
    // Refuse values that cannot be faithfully applied through this finite API.
    if value.CreationTime <= 0 {
        return Err(FilesystemOperationError::MetadataUnsupported);
    }
    Ok(BasicIdentity {
        creation: value.CreationTime,
        last_write: value.LastWriteTime,
        attributes: supported_attributes(value.FileAttributes)?,
    })
}

fn basic(file: &File) -> Result<FILE_BASIC_INFO, FilesystemOperationError> {
    let mut value = FILE_BASIC_INFO::default();
    // SAFETY: borrowed live handle and one aligned, fixed-size writable buffer.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&mut value as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(map_io_error(std::io::Error::last_os_error()));
    }
    Ok(value)
}

fn set_basic(file: &File, value: &FILE_BASIC_INFO) -> Result<(), FilesystemOperationError> {
    // SAFETY: owned staging handle; fixed-size input stays live. Never called on
    // an ambient original by production code and never enables privileges.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (value as *const FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(map_io_error(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::CheckedWriteError;
    use cap_std::ambient_authority;
    use sha2::{Digest, Sha256};

    fn permissions(root: &Path) -> crate::ComponentPermissions {
        let manifest: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": "write", "access": "write"}], "temporaryDirectory": false,
            "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        crate::ComponentPermissions::from_manifest(&manifest, root).unwrap()
    }

    fn open_rw(parent: &Dir, name: &str) -> File {
        let mut options = OpenOptions::new();
        options.read(true).write(true).follow(FollowSymlinks::No);
        parent.open_with(name, &options).unwrap()
    }

    fn no_staging(root: &Path) {
        assert!(std::fs::read_dir(root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".snipper-write-")));
    }

    #[test]
    fn finite_attribute_policy_rejects_special_storage_without_changing_it() {
        use windows_sys::Win32::Storage::FileSystem::*;
        for flag in [
            FILE_ATTRIBUTE_ENCRYPTED,
            FILE_ATTRIBUTE_SPARSE_FILE,
            FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_ATTRIBUTE_INTEGRITY_STREAM,
            FILE_ATTRIBUTE_OFFLINE,
            FILE_ATTRIBUTE_NO_SCRUB_DATA,
            FILE_ATTRIBUTE_PINNED,
            FILE_ATTRIBUTE_UNPINNED,
            FILE_ATTRIBUTE_RECALL_ON_OPEN,
            FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
            FILE_ATTRIBUTE_DIRECTORY,
            FILE_ATTRIBUTE_DEVICE,
            FILE_ATTRIBUTE_VIRTUAL,
            0x8000_0000,
        ] {
            assert_eq!(
                supported_attributes(flag | FILE_ATTRIBUTE_ARCHIVE),
                Err(FilesystemOperationError::MetadataUnsupported)
            );
        }
        assert_eq!(
            supported_attributes(FILE_ATTRIBUTE_READONLY),
            Err(FilesystemOperationError::PermissionDenied)
        );
        assert_eq!(supported_attributes(FILE_ATTRIBUTE_NORMAL), Ok(0));
        assert_eq!(
            supported_attributes(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_COMPRESSED),
            Ok(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_COMPRESSED)
        );
    }

    #[test]
    fn actual_replacement_keeps_creation_and_attributes_but_marks_new_content() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("write");
        std::fs::create_dir(&root).unwrap();
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let permissions = permissions(temporary.path());
        for flags in [
            FILE_ATTRIBUTE_NORMAL,
            FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
            FILE_ATTRIBUTE_TEMPORARY,
        ] {
            let file = open_rw(&parent, "target");
            set_basic(
                &file,
                &FILE_BASIC_INFO {
                    CreationTime: 132_537_600_000_000_000,
                    LastWriteTime: 132_537_600_100_000_000,
                    FileAttributes: flags,
                    ..FILE_BASIC_INFO::default()
                },
            )
            .unwrap();
            drop(file);
            let before = identity(&open_rw(&parent, "target")).unwrap();
            permissions
                .write_file("path-0", "target", b"replacement", 1024)
                .unwrap();
            let after = identity(&open_rw(&parent, "target")).unwrap();
            assert_eq!(after.creation, before.creation);
            assert_eq!(after.attributes, before.attributes | FILE_ATTRIBUTE_ARCHIVE);
            assert!(after.last_write > before.last_write);
            assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
            no_staging(&root);
        }
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }

    #[test]
    fn metadata_only_concurrent_edits_preserve_original_and_report_conflict() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("write");
        std::fs::create_dir(&root).unwrap();
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let permissions = permissions(temporary.path());
        let digest = hex::encode(Sha256::digest(b"original"));
        for variation in 0..4 {
            let original = open_rw(&parent, "target");
            let before = basic(&original).unwrap();
            let mut changed = false;
            let result = permissions.write_file_checked(
                "path-0",
                "target",
                b"replacement",
                1024,
                Some(&digest),
                || {
                    if !changed && std::fs::read_dir(&root).unwrap().count() > 1 {
                        let mut edit = FILE_BASIC_INFO::default();
                        match variation {
                            0 => edit.CreationTime = before.CreationTime + 10_000_000,
                            1 => edit.LastWriteTime = before.LastWriteTime + 10_000_000,
                            2 => {
                                edit.FileAttributes = before.FileAttributes ^ FILE_ATTRIBUTE_HIDDEN
                            }
                            _ => {
                                edit.FileAttributes =
                                    before.FileAttributes | FILE_ATTRIBUTE_READONLY
                            }
                        }
                        set_basic(&open_rw(&parent, "target"), &edit).unwrap();
                        changed = true;
                    }
                    Ok::<_, ()>(())
                },
            );
            let after = basic(&original).unwrap();
            if variation == 3 {
                // Restore only this fixture's external readonly edit for cleanup.
                set_basic(
                    &original,
                    &FILE_BASIC_INFO {
                        FileAttributes: before.FileAttributes,
                        ..FILE_BASIC_INFO::default()
                    },
                )
                .unwrap();
            }
            assert!(changed);
            assert!(matches!(
                result,
                Err(CheckedWriteError::Filesystem(
                    FilesystemOperationError::Conflict
                ))
            ));
            assert_eq!(std::fs::read(&target).unwrap(), b"original");
            if variation == 3 {
                assert_ne!(after.FileAttributes & FILE_ATTRIBUTE_READONLY, 0);
            }
            no_staging(&root);
            drop(original);
            // A conflict is not a persistent refusal: a fresh observed operation works.
            permissions
                .write_file("path-0", "target", b"original", 1024)
                .unwrap();
        }
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }

    #[test]
    fn readonly_preflight_does_not_change_original_or_create_staging() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("write");
        std::fs::create_dir(&root).unwrap();
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let permissions = permissions(temporary.path());
        let file = open_rw(&parent, "target");
        set_basic(
            &file,
            &FILE_BASIC_INFO {
                FileAttributes: FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_ARCHIVE,
                ..FILE_BASIC_INFO::default()
            },
        )
        .unwrap();
        let validation = permissions.validate_write_target("path-0", "target").err();
        let result = permissions.write_file("path-0", "target", b"replacement", 1024);
        let after = basic(&file).unwrap();
        let bytes = std::fs::read(&target).unwrap();
        // Clear only this fixture's readonly bit before asserting/owned cleanup.
        set_basic(
            &file,
            &FILE_BASIC_INFO {
                FileAttributes: FILE_ATTRIBUTE_ARCHIVE,
                ..FILE_BASIC_INFO::default()
            },
        )
        .unwrap();
        assert_eq!(validation, Some(FilesystemOperationError::PermissionDenied));
        assert_eq!(result, Err(FilesystemOperationError::PermissionDenied));
        assert_ne!(after.FileAttributes & FILE_ATTRIBUTE_READONLY, 0);
        assert_eq!(bytes, b"original");
        no_staging(&root);
        drop(file);
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }
}

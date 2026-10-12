//! NTFS compression preservation; unsupported storage semantics are not erased.
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::{null, null_mut};

use crate::permissions::{map_io_error, FilesystemOperationError};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File, OpenOptions};
use windows_sys::Win32::Storage::FileSystem::{
    FileBasicInfo, GetFileInformationByHandleEx, COMPRESSION_FORMAT_LZNT1, COMPRESSION_FORMAT_NONE,
    FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_ENCRYPTED, FILE_ATTRIBUTE_INTEGRITY_STREAM,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_BASIC_INFO,
};
use windows_sys::Win32::System::Ioctl::{FSCTL_GET_COMPRESSION, FSCTL_SET_COMPRESSION};
use windows_sys::Win32::System::IO::DeviceIoControl;

#[derive(PartialEq, Eq)]
pub(crate) struct WriteCompression(Option<u16>);

impl WriteCompression {
    pub(crate) fn read(parent: &Dir, target: &Path) -> Result<Self, FilesystemOperationError> {
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        match parent.open_with(target, &options) {
            Ok(file) => {
                let flags = attributes(&file)?;
                if unsupported(flags) {
                    return Err(FilesystemOperationError::MetadataUnsupported);
                }
                let format = if flags & FILE_ATTRIBUTE_COMPRESSED == 0 {
                    COMPRESSION_FORMAT_NONE
                } else {
                    format(&file)?
                };
                Ok(Self(Some(format)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self(None)),
            Err(error) => Err(map_io_error(error)),
        }
    }

    pub(crate) fn apply(&self, file: &File) -> Result<(), FilesystemOperationError> {
        if let Some(expected) = self.0 {
            let actual = if attributes(file)? & FILE_ATTRIBUTE_COMPRESSED == 0 {
                COMPRESSION_FORMAT_NONE
            } else {
                format(file)?
            };
            if actual != expected {
                set_format(file, expected)?;
            }
        }
        Ok(())
    }

    pub(crate) fn verify_staging(&self, file: &File) -> Result<(), FilesystemOperationError> {
        if let Some(expected) = self.0 {
            if unsupported(attributes(file)?) || format(file)? != expected {
                return Err(FilesystemOperationError::Conflict);
            }
        }
        Ok(())
    }

    pub(crate) fn recheck(
        &self,
        parent: &Dir,
        target: &Path,
    ) -> Result<(), FilesystemOperationError> {
        match Self::read(parent, target) {
            Ok(current) if current == *self => Ok(()),
            Ok(_) | Err(FilesystemOperationError::MetadataUnsupported) => {
                Err(FilesystemOperationError::Conflict)
            }
            Err(error) => Err(error),
        }
    }
}

fn unsupported(flags: u32) -> bool {
    flags
        & (FILE_ATTRIBUTE_ENCRYPTED
            | FILE_ATTRIBUTE_SPARSE_FILE
            | FILE_ATTRIBUTE_REPARSE_POINT
            | FILE_ATTRIBUTE_INTEGRITY_STREAM)
        != 0
}

fn attributes(file: &File) -> Result<u32, FilesystemOperationError> {
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: Live regular-file handle and correctly aligned fixed-size output.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(map_io_error(std::io::Error::last_os_error()));
    }
    Ok(basic.FileAttributes)
}

pub(crate) fn format(file: &File) -> Result<u16, FilesystemOperationError> {
    let mut compression = 0_u16;
    let mut returned = 0_u32;
    // SAFETY: Synchronous call, live handle, initialized two-byte output; no
    // overlapped pointer or callback outlives these stack references.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_GET_COMPRESSION,
            null(),
            0,
            (&mut compression as *mut u16).cast(),
            2,
            &mut returned,
            null_mut(),
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(1 | 50))
            && attributes(file)? & FILE_ATTRIBUTE_COMPRESSED == 0
        {
            return Ok(COMPRESSION_FORMAT_NONE);
        }
        return Err(map_io_error(error));
    }
    if returned != 2
        || !matches!(
            compression,
            COMPRESSION_FORMAT_NONE | COMPRESSION_FORMAT_LZNT1
        )
    {
        return Err(FilesystemOperationError::MetadataUnsupported);
    }
    Ok(compression)
}

pub(crate) fn set_format(file: &File, compression: u16) -> Result<(), FilesystemOperationError> {
    if !matches!(
        compression,
        COMPRESSION_FORMAT_NONE | COMPRESSION_FORMAT_LZNT1
    ) {
        return Err(FilesystemOperationError::MetadataUnsupported);
    }
    let mut returned = 0_u32;
    // SAFETY: Synchronous control code modifies only the owned staging handle;
    // input is one supported fixed-size format value with a valid lifetime.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_COMPRESSION,
            (&compression as *const u16).cast(),
            2,
            null_mut(),
            0,
            &mut returned,
            null_mut(),
        )
    } == 0
    {
        return Err(map_io_error(std::io::Error::last_os_error()));
    }
    if format(file)? != compression {
        return Err(FilesystemOperationError::MetadataUnsupported);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::CheckedWriteError;
    use cap_std::ambient_authority;
    use sha2::{Digest, Sha256};
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;

    fn permissions(root: &Path) -> crate::ComponentPermissions {
        let value: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": "write", "access": "write"}], "temporaryDirectory": false, "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        crate::ComponentPermissions::from_manifest(&value, root).unwrap()
    }

    fn open_rw(parent: &Dir, name: &str) -> File {
        let mut options = OpenOptions::new();
        options.read(true).write(true).follow(FollowSymlinks::No);
        parent.open_with(name, &options).unwrap()
    }

    #[test]
    fn actual_primary_and_named_stream_compression_survive_replacement() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("target");
        std::fs::write(&target, vec![b'x'; 64 * 1024]).unwrap();
        let mut stream = target.as_os_str().to_os_string();
        stream.push(":metadata");
        std::fs::write(&stream, b"stream").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let permissions = permissions(temporary.path());
        for (main, named) in [
            (COMPRESSION_FORMAT_NONE, COMPRESSION_FORMAT_LZNT1),
            (COMPRESSION_FORMAT_LZNT1, COMPRESSION_FORMAT_NONE),
            (COMPRESSION_FORMAT_LZNT1, COMPRESSION_FORMAT_LZNT1),
        ] {
            let file = open_rw(&parent, "target");
            set_format(&file, main).unwrap();
            drop(file);
            let file = File::from_std(
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&stream)
                    .unwrap(),
            );
            set_format(&file, named).unwrap();
            drop(file);
            permissions
                .write_file("path-0", "target", b"replacement", 128 * 1024)
                .unwrap();
            assert_eq!(format(&open_rw(&parent, "target")).unwrap(), main);
            let file = crate::write_security::open_named_stream(
                &parent,
                Path::new("target"),
                ":metadata:$DATA",
                false,
            )
            .unwrap();
            assert_eq!(format(&file).unwrap(), named);
            assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
            assert_eq!(std::fs::read(&stream).unwrap(), b"stream");
        }
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }

    #[test]
    fn compression_only_concurrent_change_conflicts_without_replacing_data() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let permissions = permissions(temporary.path());
        let expected = hex::encode(Sha256::digest(b"original"));
        let mut changed = false;
        let result = permissions.write_file_checked(
            "path-0",
            "target",
            b"replacement",
            1024,
            Some(&expected),
            || {
                if !changed && std::fs::read_dir(&root).unwrap().count() > 1 {
                    set_format(&open_rw(&parent, "target"), COMPRESSION_FORMAT_LZNT1).unwrap();
                    changed = true;
                }
                Ok::<_, ()>(())
            },
        );
        assert!(changed);
        assert!(matches!(
            result,
            Err(CheckedWriteError::Filesystem(
                FilesystemOperationError::Conflict
            ))
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }

    #[test]
    fn named_stream_compression_only_change_refuses_publication() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let mut stream = target.as_os_str().to_os_string();
        stream.push(":metadata");
        std::fs::write(&stream, b"stream").unwrap();
        let permissions = permissions(temporary.path());
        let expected = hex::encode(Sha256::digest(b"original"));
        let mut changed = false;
        let result = permissions.write_file_checked(
            "path-0",
            "target",
            b"replacement",
            1024,
            Some(&expected),
            || {
                if !changed && std::fs::read_dir(&root).unwrap().count() > 1 {
                    let file = File::from_std(
                        std::fs::OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(&stream)
                            .unwrap(),
                    );
                    set_format(&file, COMPRESSION_FORMAT_LZNT1).unwrap();
                    changed = true;
                }
                Ok::<_, ()>(())
            },
        );
        assert!(changed);
        assert!(matches!(
            result,
            Err(CheckedWriteError::Filesystem(
                FilesystemOperationError::Conflict
            ))
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read(&stream).unwrap(), b"stream");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn special_storage_is_refused_without_enabling_privileges_or_creating_staging() {
        for flag in [
            FILE_ATTRIBUTE_ENCRYPTED,
            FILE_ATTRIBUTE_SPARSE_FILE,
            FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_ATTRIBUTE_INTEGRITY_STREAM,
        ] {
            assert!(unsupported(flag));
        }
        assert!(!unsupported(FILE_ATTRIBUTE_COMPRESSED));
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("target");
        std::fs::write(&target, b"original").unwrap();
        let parent = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let file = open_rw(&parent, "target");
        let mut returned = 0;
        // SAFETY: Test-only sparse marking of an owned fixture; no global policy,
        // certificate creation or process-privilege enabling is performed.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    FSCTL_SET_SPARSE,
                    null(),
                    0,
                    null_mut(),
                    0,
                    &mut returned,
                    null_mut(),
                )
            },
            0
        );
        drop(file);
        let permissions = permissions(temporary.path());
        assert_eq!(
            permissions.write_file("path-0", "target", b"new", 1024),
            Err(FilesystemOperationError::MetadataUnsupported)
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        drop(parent);
        temporary.close().unwrap();
    }
}

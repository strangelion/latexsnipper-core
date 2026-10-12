//! Bounded Windows named-$DATA preservation for owned staging replacements.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use windows_sys::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_HANDLE_EOF, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA,
};
use windows_sys::Win32::Storage::FileSystem::{FileStreamInfo, GetFileInformationByHandleEx};

use crate::permissions::{map_io_error, CheckedWriteError, FilesystemOperationError};

const ENUM_BYTES: usize = 64 * 1024;
const MAX_STREAMS: usize = 128;
const MAX_STREAM_NAME_BYTES: usize = 1024;
const MAX_STREAM_DATA_BYTES: usize = 4 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

#[derive(Default, PartialEq, Eq)]
pub(crate) struct WriteStreams {
    streams: BTreeMap<String, NamedStream>,
}

#[derive(PartialEq, Eq)]
struct NamedStream {
    content: Vec<u8>,
    compression: u16,
}

impl WriteStreams {
    pub(crate) fn read<E>(
        parent: &Dir,
        target: &Path,
        limit: usize,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Self, CheckedWriteError<E>> {
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = match parent.open_with(target, &options) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(map_io_error(error).into()),
        };
        let metadata = file.metadata().map_err(map_io_error)?;
        if !metadata.is_file() || metadata.nlink() > 1 {
            return Err(FilesystemOperationError::InvalidPath.into());
        }
        // Vec<u64> provides the Win32-required eight-byte alignment. It is fully
        // initialized; parsing never assumes that all requested bytes were used.
        let mut storage = vec![0_u64; ENUM_BYTES / 8];
        // SAFETY: Live regular-file handle and initialized, aligned bounded
        // buffer. LastError is thread-local and reset to disambiguate empty lists.
        let (success, error) = unsafe {
            SetLastError(0);
            let success = GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileStreamInfo,
                storage.as_mut_ptr().cast(),
                ENUM_BYTES as u32,
            );
            (success, GetLastError())
        };
        if error == ERROR_HANDLE_EOF {
            return Ok(Self::default());
        }
        if success == 0 {
            return Err(
                if matches!(error, ERROR_INSUFFICIENT_BUFFER | ERROR_MORE_DATA) {
                    FilesystemOperationError::SizeLimit
                } else {
                    map_io_error(std::io::Error::from_raw_os_error(error as i32))
                }
                .into(),
            );
        }
        // SAFETY: The initialized allocation contains exactly ENUM_BYTES bytes;
        // the native call has returned and retains no pointer into this buffer.
        let bytes =
            unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), ENUM_BYTES) };
        let names = parse_streams(bytes)?;
        let budget = limit.min(MAX_STREAM_DATA_BYTES);
        let mut streams = BTreeMap::new();
        let mut remaining = budget;
        for (name, declared_length) in names {
            if declared_length > remaining {
                return Err(FilesystemOperationError::SizeLimit.into());
            }
            checkpoint().map_err(CheckedWriteError::Interrupted)?;
            let stream = crate::write_security::open_named_stream(parent, target, &name, false)
                .map_err(map_io_error)?;
            let metadata = stream.metadata().map_err(map_io_error)?;
            if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() > 1 {
                return Err(FilesystemOperationError::InvalidPath.into());
            }
            let compression = crate::write_compression::format(&stream)?;
            let mut reader = stream.take(remaining as u64 + 1);
            let mut content = Vec::with_capacity(declared_length);
            let mut buffer = [0_u8; CHUNK];
            loop {
                checkpoint().map_err(CheckedWriteError::Interrupted)?;
                let length = reader.read(&mut buffer).map_err(map_io_error)?;
                if length == 0 {
                    break;
                }
                if content.len().saturating_add(length) > remaining {
                    return Err(FilesystemOperationError::SizeLimit.into());
                }
                content.extend_from_slice(&buffer[..length]);
            }
            if content.len() != declared_length {
                return Err(FilesystemOperationError::Conflict.into());
            }
            remaining -= content.len();
            streams.insert(
                name,
                NamedStream {
                    content,
                    compression,
                },
            );
        }
        Ok(Self { streams })
    }

    pub(crate) fn apply<E>(
        &self,
        parent: &Dir,
        staged_name: &str,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), CheckedWriteError<E>> {
        for (name, stream) in &self.streams {
            checkpoint().map_err(CheckedWriteError::Interrupted)?;
            let mut file = crate::write_security::open_named_stream(
                parent,
                Path::new(staged_name),
                name,
                true,
            )
            .map_err(map_io_error)?;
            if crate::write_compression::format(&file)? != stream.compression {
                crate::write_compression::set_format(&file, stream.compression)?;
            }
            for chunk in stream.content.chunks(CHUNK) {
                checkpoint().map_err(CheckedWriteError::Interrupted)?;
                file.write_all(chunk).map_err(map_io_error)?;
            }
            file.sync_all().map_err(map_io_error)?;
        }
        Ok(())
    }

    pub(crate) fn recheck<E>(
        &self,
        parent: &Dir,
        target: &Path,
        limit: usize,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<(), CheckedWriteError<E>> {
        if Self::read(parent, target, limit, checkpoint)? != *self {
            return Err(FilesystemOperationError::Conflict.into());
        }
        Ok(())
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), FilesystemOperationError> {
    if name.encode_utf16().count() * 2 > MAX_STREAM_NAME_BYTES {
        return Err(FilesystemOperationError::InvalidPath);
    }
    let inner = name
        .strip_prefix(':')
        .and_then(|name| name.strip_suffix(":$DATA"))
        .ok_or(FilesystemOperationError::InvalidPath)?;
    if inner.is_empty() || inner.contains([':', '/', '\\']) || inner.chars().any(char::is_control) {
        return Err(FilesystemOperationError::InvalidPath);
    }
    Ok(())
}

fn parse_streams(bytes: &[u8]) -> Result<BTreeMap<String, usize>, FilesystemOperationError> {
    let mut streams = BTreeMap::new();
    let mut offset = 0_usize;
    let mut unnamed_seen = false;
    for _ in 0..MAX_STREAMS + 1 {
        let header = bytes
            .get(
                offset
                    ..offset
                        .checked_add(24)
                        .ok_or(FilesystemOperationError::InvalidPath)?,
            )
            .ok_or(FilesystemOperationError::InvalidPath)?;
        let next = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let name_length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let size = i64::from_le_bytes(header[8..16].try_into().unwrap());
        if name_length == 0
            || !name_length.is_multiple_of(2)
            || name_length > MAX_STREAM_NAME_BYTES
            || size < 0
        {
            return Err(FilesystemOperationError::InvalidPath);
        }
        let end = offset
            .checked_add(24)
            .and_then(|value| value.checked_add(name_length))
            .ok_or(FilesystemOperationError::InvalidPath)?;
        let raw_name = bytes
            .get(offset + 24..end)
            .ok_or(FilesystemOperationError::InvalidPath)?;
        let units: Vec<_> = raw_name
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let name = String::from_utf16(&units).map_err(|_| FilesystemOperationError::InvalidPath)?;
        if name == "::$DATA" {
            if unnamed_seen {
                return Err(FilesystemOperationError::InvalidPath);
            }
            unnamed_seen = true;
        } else {
            validate_name(&name)?;
            let size = usize::try_from(size).map_err(|_| FilesystemOperationError::SizeLimit)?;
            if streams.insert(name, size).is_some() {
                return Err(FilesystemOperationError::InvalidPath);
            }
            if streams.len() > MAX_STREAMS {
                return Err(FilesystemOperationError::SizeLimit);
            }
        }
        if next == 0 {
            return Ok(streams);
        }
        if next < 24 + name_length || !next.is_multiple_of(8) {
            return Err(FilesystemOperationError::InvalidPath);
        }
        offset = offset
            .checked_add(next)
            .filter(|offset| *offset < bytes.len())
            .ok_or(FilesystemOperationError::InvalidPath)?;
    }
    Err(FilesystemOperationError::SizeLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::ambient_authority;
    use sha2::Digest;

    fn permissions(root: &Path) -> crate::ComponentPermissions {
        let value: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": "write", "access": "write"}], "temporaryDirectory": false, "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        crate::ComponentPermissions::from_manifest(&value, root).unwrap()
    }

    fn encoded_streams(entries: &[(&str, i64)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (index, (name, size)) in entries.iter().enumerate() {
            let units: Vec<_> = name.encode_utf16().collect();
            let length = (24 + units.len() * 2 + 7) & !7;
            let next = if index + 1 == entries.len() {
                0
            } else {
                length as u32
            };
            bytes.extend(next.to_le_bytes());
            bytes.extend((units.len() as u32 * 2).to_le_bytes());
            bytes.extend(size.to_le_bytes());
            bytes.extend(size.to_le_bytes());
            for unit in units {
                bytes.extend(unit.to_le_bytes());
            }
            bytes.resize(
                bytes.len() + (length - 24 - name.encode_utf16().count() * 2),
                0,
            );
        }
        bytes
    }

    #[test]
    fn stream_records_reject_malformed_lengths_offsets_names_types_duplicates_and_counts() {
        assert_eq!(
            parse_streams(&encoded_streams(&[
                ("::$DATA", 3),
                (":Zone.Identifier:$DATA", 8)
            ]))
            .unwrap()
            .len(),
            1
        );
        for name in [
            ":../escape:$DATA",
            ":bad\\name:$DATA",
            ":bad:more:$DATA",
            ":x:$INDEX_ALLOCATION",
            ":nul\0:$DATA",
        ] {
            assert!(parse_streams(&encoded_streams(&[(name, 0)])).is_err());
        }
        assert!(parse_streams(&encoded_streams(&[(":x:$DATA", -1)])).is_err());
        assert!(parse_streams(&encoded_streams(&[(":x:$DATA", 0), (":x:$DATA", 0)])).is_err());
        assert!(parse_streams(&encoded_streams(&[("::$DATA", 0), ("::$DATA", 0)])).is_err());
        let valid = encoded_streams(&[(":x:$DATA", 0), (":y:$DATA", 0)]);
        for (field, value) in [(0, 1_u32), (0, u32::MAX), (4, 3), (4, 1026)] {
            let mut malformed = valid.clone();
            malformed[field..field + 4].copy_from_slice(&value.to_le_bytes());
            assert!(parse_streams(&malformed).is_err());
        }
        assert!(parse_streams(&valid[..25]).is_err());
        let mut invalid_utf16 = encoded_streams(&[(":x:$DATA", 0)]);
        invalid_utf16[24..26].copy_from_slice(&0xD800_u16.to_le_bytes());
        assert!(parse_streams(&invalid_utf16).is_err());
        let names: Vec<_> = (0..129)
            .map(|index| format!(":stream{index}:$DATA"))
            .collect();
        let entries: Vec<_> = names.iter().map(|name| (name.as_str(), 0)).collect();
        assert_eq!(
            parse_streams(&encoded_streams(&entries)).unwrap_err(),
            FilesystemOperationError::SizeLimit
        );
    }

    #[test]
    fn actual_replacement_preserves_zone_binary_unicode_and_empty_streams() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("a");
        std::fs::write(&target, b"old").unwrap();
        for (name, data) in [
            (":Zone.Identifier", b"[ZoneTransfer]\nZoneId=3\n".as_slice()),
            (":\u{03bb}", &[0_u8, 255, 1]),
            (":empty", b""),
        ] {
            std::fs::write(fixture_stream(&target, name), data).unwrap();
        }
        let permissions = permissions(temporary.path());
        permissions
            .write_file("path-0", "a", b"replacement", 1024)
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
        assert_eq!(
            std::fs::read(fixture_stream(&target, ":Zone.Identifier")).unwrap(),
            b"[ZoneTransfer]\nZoneId=3\n"
        );
        assert_eq!(
            std::fs::read(fixture_stream(&target, ":\u{03bb}")).unwrap(),
            [0_u8, 255, 1]
        );
        assert!(std::fs::read(fixture_stream(&target, ":empty"))
            .unwrap()
            .is_empty());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn named_stream_only_change_is_a_conflict_and_preserves_the_external_edit() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("a");
        let zone = fixture_stream(&target, ":Zone.Identifier");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&zone, b"old-zone").unwrap();
        let permissions = permissions(temporary.path());
        let expected = hex::encode(sha2::Sha256::digest(b"old"));
        let mut changed = false;
        let result = permissions.write_file_checked(
            "path-0",
            "a",
            b"replacement",
            1024,
            Some(&expected),
            || {
                if !changed && std::fs::read_dir(&root).unwrap().count() > 1 {
                    std::fs::write(&zone, b"new-zone").unwrap();
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
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(std::fs::read(&zone).unwrap(), b"new-zone");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn oversized_metadata_and_cancelled_partial_stream_copy_leave_original_untouched() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("a");
        let data_path = fixture_stream(&target, ":data");
        std::fs::write(&target, b"old").unwrap();
        let data = vec![b'x'; 2 * CHUNK];
        std::fs::write(&data_path, &data).unwrap();
        let permissions = permissions(temporary.path());
        assert_eq!(
            permissions.write_file("path-0", "a", b"new", 1024),
            Err(FilesystemOperationError::SizeLimit)
        );
        let expected = hex::encode(sha2::Sha256::digest(b"old"));
        let mut stopped_after_partial_copy = false;
        let result = permissions.write_file_checked(
            "path-0",
            "a",
            b"new",
            data.len(),
            Some(&expected),
            || {
                for entry in std::fs::read_dir(&root).unwrap() {
                    let path = entry.unwrap().path();
                    if path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".snipper-write-")
                        && std::fs::metadata(fixture_stream(&path, ":data"))
                            .is_ok_and(|metadata| metadata.len() > 0)
                    {
                        stopped_after_partial_copy = true;
                        return Err("cancelled during metadata copy");
                    }
                }
                Ok(())
            },
        );
        assert!(stopped_after_partial_copy);
        assert!(matches!(result, Err(CheckedWriteError::Interrupted(_))));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(std::fs::read(&data_path).unwrap(), data);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::write(&data_path, vec![b'x'; MAX_STREAM_DATA_BYTES + 1]).unwrap();
        assert_eq!(
            permissions.write_file("path-0", "a", b"new", MAX_STREAM_DATA_BYTES + 1),
            Err(FilesystemOperationError::SizeLimit)
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        drop(permissions);
        temporary.close().unwrap();
    }

    fn fixture_stream(base: &Path, suffix: &str) -> PathBuf {
        let mut path = base.as_os_str().to_os_string();
        path.push(suffix);
        PathBuf::from(path)
    }

    #[test]
    fn real_stream_enumeration_and_capability_relative_copy_preserve_unicode_and_empty_data() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("a");
        std::fs::write(&path, b"main").unwrap();
        std::fs::write(
            fixture_stream(&path, ":Zone.Identifier"),
            b"[ZoneTransfer]\nZoneId=3\n",
        )
        .unwrap();
        std::fs::write(fixture_stream(&path, ":\u{03bb}"), [0_u8, 255, 1]).unwrap();
        std::fs::write(fixture_stream(&path, ":empty"), b"").unwrap();
        let parent = Dir::open_ambient_dir(temporary.path(), ambient_authority()).unwrap();
        let original =
            WriteStreams::read(&parent, Path::new("a"), 1024, &mut || Ok::<_, ()>(())).unwrap();
        assert_eq!(original.streams.len(), 3);
        assert!(original
            .apply(&parent, "missing", &mut || Ok::<_, ()>(()))
            .is_err());
        assert!(!temporary.path().join("missing").exists());
        std::fs::write(temporary.path().join("candidate"), b"new").unwrap();
        original
            .apply(&parent, "candidate", &mut || Ok::<_, ()>(()))
            .unwrap();
        original
            .recheck(&parent, Path::new("candidate"), 1024, &mut || {
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(
            std::fs::read(temporary.path().join("candidate:Zone.Identifier")).unwrap(),
            b"[ZoneTransfer]\nZoneId=3\n"
        );
        assert_eq!(
            std::fs::read(temporary.path().join("candidate")).unwrap(),
            b"new"
        );
        assert!(original
            .apply(&parent, "candidate", &mut || Ok::<_, ()>(()))
            .is_err());
        drop(parent);
        temporary.close().unwrap();
    }
}

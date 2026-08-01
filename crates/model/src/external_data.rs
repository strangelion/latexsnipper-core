//! Fail-closed validation for ONNX external-data model packages.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ExternalDataEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalDataLimits {
    pub maximum_files: usize,
    pub maximum_total_bytes: u64,
}

impl Default for ExternalDataLimits {
    fn default() -> Self {
        Self {
            maximum_files: 128,
            maximum_total_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDataValidation {
    pub files: Vec<ValidatedExternalData>,
    pub total_bytes: u64,
}

/// Validate every staged external-data file before atomically publishing the
/// manifest that makes the generation visible. The replacement is performed
/// in the destination directory and uses write-through replacement on Windows.
pub fn publish_external_data_generation(
    model_root: &Path,
    entries: &[ExternalDataEntry],
    limits: ExternalDataLimits,
    manifest_bytes: &[u8],
    live_manifest: &Path,
) -> Result<ExternalDataValidation, ExternalDataError> {
    let validation = validate_external_data(model_root, entries, limits)?;
    let parent = live_manifest.parent().ok_or_else(|| {
        ExternalDataError::PathEscape("live manifest has no parent directory".to_owned())
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        ExternalDataError::PathEscape(format!("cannot create manifest directory: {error}"))
    })?;
    let staged = parent.join(format!(
        ".{}.{}.staging",
        live_manifest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("manifest"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| {
                ExternalDataError::PathEscape(format!("cannot stage manifest: {error}"))
            })?;
        file.write_all(manifest_bytes).map_err(|error| {
            ExternalDataError::PathEscape(format!("cannot write staged manifest: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            ExternalDataError::PathEscape(format!("cannot sync staged manifest: {error}"))
        })?;
        drop(file);
        atomic_replace(&staged, live_manifest).map_err(|error| {
            ExternalDataError::PathEscape(format!("cannot activate staged manifest: {error}"))
        })?;
        Ok(validation)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: Both UTF-16 buffers are NUL-terminated and live for the call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedExternalData {
    pub relative_path: String,
    pub canonical_path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExternalDataError {
    #[error("MODEL_EXTERNAL_DATA_MISSING: {0}")]
    Missing(String),
    #[error("MODEL_EXTERNAL_DATA_HASH_MISMATCH: {0}")]
    HashMismatch(String),
    #[error("MODEL_EXTERNAL_DATA_PATH_ESCAPE: {0}")]
    PathEscape(String),
    #[error("MODEL_EXTERNAL_DATA_SIZE_LIMIT: {0}")]
    SizeLimit(String),
    #[error("MODEL_EXTERNAL_DATA_COUNT_LIMIT: {0}")]
    CountLimit(String),
}

impl ExternalDataError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Missing(_) => "MODEL_EXTERNAL_DATA_MISSING",
            Self::HashMismatch(_) => "MODEL_EXTERNAL_DATA_HASH_MISMATCH",
            Self::PathEscape(_) => "MODEL_EXTERNAL_DATA_PATH_ESCAPE",
            Self::SizeLimit(_) => "MODEL_EXTERNAL_DATA_SIZE_LIMIT",
            Self::CountLimit(_) => "MODEL_EXTERNAL_DATA_COUNT_LIMIT",
        }
    }
}

pub fn validate_external_data(
    model_root: &Path,
    entries: &[ExternalDataEntry],
    limits: ExternalDataLimits,
) -> Result<ExternalDataValidation, ExternalDataError> {
    if entries.len() > limits.maximum_files {
        return Err(ExternalDataError::CountLimit(format!(
            "{} files exceed the configured maximum of {}",
            entries.len(),
            limits.maximum_files
        )));
    }
    let canonical_root = model_root.canonicalize().map_err(|error| {
        ExternalDataError::Missing(format!("model root '{}': {error}", model_root.display()))
    })?;
    let mut names = BTreeSet::new();
    let mut total_bytes = 0u64;
    let mut files = Vec::with_capacity(entries.len());

    for entry in entries {
        let relative = safe_relative_path(&entry.path)?;
        let duplicate_key = entry.path.replace('\\', "/").to_ascii_lowercase();
        if !names.insert(duplicate_key) {
            return Err(ExternalDataError::PathEscape(format!(
                "duplicate external-data name '{}'",
                entry.path
            )));
        }
        if !is_sha256(&entry.sha256) {
            return Err(ExternalDataError::HashMismatch(format!(
                "'{}' does not declare a valid SHA-256",
                entry.path
            )));
        }
        let canonical = canonical_root
            .join(relative)
            .canonicalize()
            .map_err(|error| ExternalDataError::Missing(format!("'{}': {error}", entry.path)))?;
        if !canonical.starts_with(&canonical_root) {
            return Err(ExternalDataError::PathEscape(format!(
                "'{}' resolves outside '{}'",
                entry.path,
                canonical_root.display()
            )));
        }
        let metadata = fs::metadata(&canonical)
            .map_err(|error| ExternalDataError::Missing(format!("'{}': {error}", entry.path)))?;
        if !metadata.is_file() {
            return Err(ExternalDataError::Missing(format!(
                "'{}' is not a regular file",
                entry.path
            )));
        }
        if metadata.len() != entry.size_bytes {
            return Err(ExternalDataError::SizeLimit(format!(
                "'{}' has {} bytes but the manifest declares {}",
                entry.path,
                metadata.len(),
                entry.size_bytes
            )));
        }
        total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
            ExternalDataError::SizeLimit("external-data size overflow".to_owned())
        })?;
        if total_bytes > limits.maximum_total_bytes {
            return Err(ExternalDataError::SizeLimit(format!(
                "external data totals {total_bytes} bytes, exceeding {}",
                limits.maximum_total_bytes
            )));
        }
        let bytes = fs::read(&canonical)
            .map_err(|error| ExternalDataError::Missing(format!("'{}': {error}", entry.path)))?;
        let observed = format!("{:x}", Sha256::digest(&bytes));
        if !observed.eq_ignore_ascii_case(&entry.sha256) {
            return Err(ExternalDataError::HashMismatch(format!(
                "'{}' expected {}, observed {observed}",
                entry.path, entry.sha256
            )));
        }
        files.push(ValidatedExternalData {
            relative_path: entry.path.replace('\\', "/"),
            canonical_path: canonical,
            sha256: observed,
            size_bytes: metadata.len(),
        });
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(ExternalDataValidation { files, total_bytes })
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ExternalDataError> {
    if value.trim().is_empty() {
        return Err(ExternalDataError::PathEscape(
            "external-data path is empty".to_owned(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return Err(ExternalDataError::PathEscape(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, bytes: &[u8]) -> ExternalDataEntry {
        ExternalDataEntry {
            path: path.to_owned(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        }
    }

    #[test]
    fn validates_single_and_multiple_external_data_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("weights.bin"), b"weights").unwrap();
        fs::write(root.path().join("bias.bin"), b"bias").unwrap();
        let report = validate_external_data(
            root.path(),
            &[entry("weights.bin", b"weights"), entry("bias.bin", b"bias")],
            ExternalDataLimits::default(),
        )
        .unwrap();
        assert_eq!(report.files.len(), 2);
        assert_eq!(report.total_bytes, 11);
    }

    #[test]
    fn missing_hash_size_count_and_duplicate_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("weights.bin"), b"weights").unwrap();
        assert_eq!(
            validate_external_data(
                root.path(),
                &[entry("missing.bin", b"missing")],
                ExternalDataLimits::default(),
            )
            .unwrap_err()
            .code(),
            "MODEL_EXTERNAL_DATA_MISSING"
        );
        let mut wrong_hash = entry("weights.bin", b"weights");
        wrong_hash.sha256 = "0".repeat(64);
        assert_eq!(
            validate_external_data(root.path(), &[wrong_hash], ExternalDataLimits::default())
                .unwrap_err()
                .code(),
            "MODEL_EXTERNAL_DATA_HASH_MISMATCH"
        );
        let mut wrong_size = entry("weights.bin", b"weights");
        wrong_size.size_bytes += 1;
        assert_eq!(
            validate_external_data(root.path(), &[wrong_size], ExternalDataLimits::default())
                .unwrap_err()
                .code(),
            "MODEL_EXTERNAL_DATA_SIZE_LIMIT"
        );
        let duplicate = entry("weights.bin", b"weights");
        assert_eq!(
            validate_external_data(
                root.path(),
                &[duplicate.clone(), duplicate],
                ExternalDataLimits::default(),
            )
            .unwrap_err()
            .code(),
            "MODEL_EXTERNAL_DATA_PATH_ESCAPE"
        );
        assert_eq!(
            validate_external_data(
                root.path(),
                &[entry("weights.bin", b"weights")],
                ExternalDataLimits {
                    maximum_files: 0,
                    maximum_total_bytes: 100,
                },
            )
            .unwrap_err()
            .code(),
            "MODEL_EXTERNAL_DATA_COUNT_LIMIT"
        );
    }

    #[test]
    fn absolute_and_parent_paths_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        for path in ["../escape.bin", "/absolute.bin", "sub/../escape.bin"] {
            assert_eq!(
                validate_external_data(
                    root.path(),
                    &[ExternalDataEntry {
                        path: path.to_owned(),
                        sha256: "0".repeat(64),
                        size_bytes: 0,
                    }],
                    ExternalDataLimits::default(),
                )
                .unwrap_err()
                .code(),
                "MODEL_EXTERNAL_DATA_PATH_ESCAPE"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_fails_closed() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), root.path().join("escape.bin")).unwrap();
        let bytes = fs::read(outside.path()).unwrap();
        assert_eq!(
            validate_external_data(
                root.path(),
                &[entry("escape.bin", &bytes)],
                ExternalDataLimits::default(),
            )
            .unwrap_err()
            .code(),
            "MODEL_EXTERNAL_DATA_PATH_ESCAPE"
        );
    }

    #[test]
    fn external_data_update_validates_before_atomic_manifest_switch() {
        let root = tempfile::tempdir().unwrap();
        let external = root.path().join("weights.bin");
        fs::write(&external, b"new-weights").unwrap();
        let live = root.path().join("manifest.json");
        fs::write(&live, b"old-manifest").unwrap();
        let entries = vec![entry("weights.bin", b"new-weights")];
        publish_external_data_generation(
            root.path(),
            &entries,
            ExternalDataLimits::default(),
            b"new-manifest",
            &live,
        )
        .unwrap();
        assert_eq!(fs::read(&live).unwrap(), b"new-manifest");

        let mut invalid = entries;
        invalid[0].sha256 = "0".repeat(64);
        let error = publish_external_data_generation(
            root.path(),
            &invalid,
            ExternalDataLimits::default(),
            b"invalid-manifest",
            &live,
        )
        .unwrap_err();
        assert!(matches!(error, ExternalDataError::HashMismatch(_)));
        assert_eq!(fs::read(&live).unwrap(), b"new-manifest");
    }
}

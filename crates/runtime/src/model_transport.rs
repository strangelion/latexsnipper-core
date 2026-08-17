//! Versioned `.lsmodel` ZIP transport for runtime model manifests.
//!
//! Transport v1 always stores `manifest.toml` at the ZIP root. A wrapper
//! directory is invalid so consumers can inspect a package without guessing a
//! layout or extracting untrusted entries first.

use crate::ModelManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Seek};
use std::path::Path;
use thiserror::Error;

pub const LSMODEL_TRANSPORT_VERSION: u32 = 1;
pub const LSMODEL_EXTENSION: &str = "lsmodel";
pub const LSMODEL_MANIFEST_PATH: &str = "manifest.toml";
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LsModelArchiveLayout {
    pub transport_version: u32,
    pub root_entries: Vec<String>,
    pub nested_manifest_paths: Vec<String>,
    pub entry_count: usize,
    pub total_uncompressed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LsModelArchiveInspection {
    pub manifest: ModelManifest,
    pub layout: LsModelArchiveLayout,
}

#[derive(Debug, Error)]
pub enum LsModelArchiveError {
    #[error("MODEL_PACKAGE_ARCHIVE_INVALID: {0}")]
    ArchiveInvalid(String),
    #[error("MODEL_PACKAGE_LIMIT_EXCEEDED: {0}")]
    LimitExceeded(String),
    #[error("MODEL_PACKAGE_ENTRY_INVALID: {0}")]
    EntryInvalid(String),
    #[error(
        "MODEL_PACKAGE_MANIFEST_MISSING: expected manifest.toml at ZIP root; root entries: {root_entries}{nested_hint}"
    )]
    ManifestMissing {
        root_entries: String,
        nested_hint: String,
    },
    #[error("MODEL_PACKAGE_MANIFEST_INVALID: {0}")]
    ManifestInvalid(String),
    #[error("MODEL_PACKAGE_SOURCE_INVALID: {0}")]
    SourceInvalid(String),
    #[error("MODEL_PACKAGE_OUTPUT_INVALID: {0}")]
    OutputInvalid(String),
    #[error("MODEL_PACKAGE_CREATE_FAILED: {0}")]
    CreateFailed(String),
}

pub fn inspect_lsmodel_archive<R: Read + Seek>(
    reader: R,
) -> Result<LsModelArchiveInspection, LsModelArchiveError> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|error| LsModelArchiveError::ArchiveInvalid(error.to_string()))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(LsModelArchiveError::LimitExceeded(format!(
            "entry count {} exceeds {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        )));
    }

    let mut root_entries = BTreeSet::new();
    let mut nested_manifest_paths = Vec::new();
    let mut normalized_paths = BTreeSet::new();
    let mut total_uncompressed_bytes = 0u64;
    let mut manifest_source = None;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| LsModelArchiveError::ArchiveInvalid(error.to_string()))?;
        let name = entry.name().to_owned();
        if name.contains('\\') || entry.enclosed_name().is_none() {
            return Err(LsModelArchiveError::EntryInvalid(format!(
                "entry {index} has an unsafe path: {name}"
            )));
        }
        let normalized = name.trim_matches('/').to_owned();
        if normalized.is_empty() {
            continue;
        }
        if !normalized_paths.insert(normalized.clone()) {
            return Err(LsModelArchiveError::EntryInvalid(format!(
                "duplicate entry: {normalized}"
            )));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(LsModelArchiveError::EntryInvalid(format!(
                "symbolic-link entry is not allowed: {normalized}"
            )));
        }
        if let Some(root) = normalized.split('/').next() {
            root_entries.insert(root.to_owned());
        }
        if normalized.ends_with(&format!("/{LSMODEL_MANIFEST_PATH}")) {
            nested_manifest_paths.push(normalized.clone());
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry.size())
            .ok_or_else(|| {
                LsModelArchiveError::LimitExceeded("uncompressed size overflow".to_owned())
            })?;
        if total_uncompressed_bytes > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(LsModelArchiveError::LimitExceeded(format!(
                "uncompressed size exceeds {MAX_TOTAL_UNCOMPRESSED_BYTES} bytes"
            )));
        }
        if normalized == LSMODEL_MANIFEST_PATH {
            if entry.is_dir() || entry.size() > MAX_MANIFEST_BYTES {
                return Err(LsModelArchiveError::ManifestInvalid(format!(
                    "manifest must be a file no larger than {MAX_MANIFEST_BYTES} bytes"
                )));
            }
            let mut source = String::new();
            entry
                .read_to_string(&mut source)
                .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
            manifest_source = Some(source);
        }
    }

    nested_manifest_paths.sort();
    let layout = LsModelArchiveLayout {
        transport_version: LSMODEL_TRANSPORT_VERSION,
        root_entries: root_entries.into_iter().collect(),
        nested_manifest_paths,
        entry_count: archive.len(),
        total_uncompressed_bytes,
    };
    let source = manifest_source.ok_or_else(|| {
        let roots = if layout.root_entries.is_empty() {
            "<empty archive>".to_owned()
        } else {
            layout.root_entries.iter().take(16).cloned().collect::<Vec<_>>().join(", ")
        };
        let nested_hint = if layout.nested_manifest_paths.is_empty() {
            String::new()
        } else {
            format!(
                "; nested manifest found at: {} (package the directory contents, not the wrapper directory)",
                layout
                    .nested_manifest_paths
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        LsModelArchiveError::ManifestMissing {
            root_entries: roots,
            nested_hint,
        }
    })?;
    let manifest: ModelManifest = toml::from_str(&source)
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
    manifest
        .validate()
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
    validate_declared_artifacts(&manifest, |path| normalized_paths.contains(path))?;
    validate_archive_checksums(&mut archive, &manifest)?;
    Ok(LsModelArchiveInspection { manifest, layout })
}

pub fn create_lsmodel_archive(
    source_directory: &Path,
    output_path: &Path,
) -> Result<(), LsModelArchiveError> {
    if !source_directory.is_dir() {
        return Err(LsModelArchiveError::SourceInvalid(format!(
            "directory does not exist: {}",
            source_directory.display()
        )));
    }
    if output_path.exists() {
        return Err(LsModelArchiveError::OutputInvalid(format!(
            "refusing to overwrite {}",
            output_path.display()
        )));
    }
    if output_path.extension().and_then(|value| value.to_str()) != Some(LSMODEL_EXTENSION) {
        return Err(LsModelArchiveError::OutputInvalid(
            "output must end with .lsmodel".to_owned(),
        ));
    }
    let source = source_directory
        .canonicalize()
        .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
    let manifest_source = std::fs::read_to_string(source.join(LSMODEL_MANIFEST_PATH))
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
    let manifest: ModelManifest = toml::from_str(&manifest_source)
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
    create_lsmodel_archive_with_manifest(source_directory, output_path, &manifest)
}

/// Create a transport-v1 archive while generating its root manifest from a
/// validated runtime manifest. Existing legacy package files are never moved or
/// rewritten, which makes this suitable for release migration jobs.
pub fn create_lsmodel_archive_with_manifest(
    source_directory: &Path,
    output_path: &Path,
    manifest: &ModelManifest,
) -> Result<(), LsModelArchiveError> {
    if !source_directory.is_dir() {
        return Err(LsModelArchiveError::SourceInvalid(format!(
            "directory does not exist: {}",
            source_directory.display()
        )));
    }
    if output_path.exists() {
        return Err(LsModelArchiveError::OutputInvalid(format!(
            "refusing to overwrite {}",
            output_path.display()
        )));
    }
    if output_path.extension().and_then(|value| value.to_str()) != Some(LSMODEL_EXTENSION) {
        return Err(LsModelArchiveError::OutputInvalid(
            "output must end with .lsmodel".to_owned(),
        ));
    }
    let source = source_directory
        .canonicalize()
        .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
    manifest
        .validate()
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;
    let manifest_source = toml::to_string_pretty(manifest)
        .map_err(|error| LsModelArchiveError::ManifestInvalid(error.to_string()))?;

    let parent = output_path
        .parent()
        .ok_or_else(|| LsModelArchiveError::OutputInvalid("missing parent directory".to_owned()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| LsModelArchiveError::OutputInvalid(error.to_string()))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| LsModelArchiveError::OutputInvalid(error.to_string()))?;
    let file_name = output_path
        .file_name()
        .ok_or_else(|| LsModelArchiveError::OutputInvalid("missing output file name".to_owned()))?;
    let output = parent.join(file_name);
    if output.starts_with(&source) {
        return Err(LsModelArchiveError::OutputInvalid(
            "output must be outside the source directory".to_owned(),
        ));
    }
    validate_declared_artifacts(manifest, |path| source.join(path).is_file())?;
    let temporary = output.with_extension(format!("lsmodel.tmp-{}", std::process::id()));
    if temporary.exists() {
        std::fs::remove_file(&temporary)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
    }
    let result = (|| {
        let file = std::fs::File::create(&temporary)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        writer
            .start_file(LSMODEL_MANIFEST_PATH, options)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        std::io::Write::write_all(&mut writer, manifest_source.as_bytes())
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        append_directory(&mut writer, &source, &source, true)?;
        let file = writer
            .finish()
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        file.sync_all()
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        std::fs::rename(&temporary, &output)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn append_directory(
    writer: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    directory: &Path,
    skip_root_manifest: bool,
) -> Result<(), LsModelArchiveError> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(LsModelArchiveError::SourceInvalid(format!(
                "symbolic links are not allowed: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            append_directory(writer, root, &path, false)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
        let archive_name = relative.to_string_lossy().replace('\\', "/");
        if skip_root_manifest && archive_name == LSMODEL_MANIFEST_PATH {
            continue;
        }
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        writer
            .start_file(archive_name, options)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
        let mut source = std::fs::File::open(&path)
            .map_err(|error| LsModelArchiveError::SourceInvalid(error.to_string()))?;
        std::io::copy(&mut source, writer)
            .map_err(|error| LsModelArchiveError::CreateFailed(error.to_string()))?;
    }
    Ok(())
}

fn validate_declared_artifacts(
    manifest: &ModelManifest,
    mut exists: impl FnMut(&str) -> bool,
) -> Result<(), LsModelArchiveError> {
    let mut artifacts = BTreeSet::new();
    for path in [
        manifest.files.primary.as_deref(),
        manifest.files.encoder.as_deref(),
        manifest.files.decoder.as_deref(),
        manifest.files.tokenizer.as_deref(),
        manifest.files.config.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        artifacts.insert(path);
    }
    for variant in &manifest.runtime_variants {
        artifacts.extend(variant.artifacts.values().map(String::as_str));
    }
    artifacts.extend(manifest.checksums.keys().map(String::as_str));
    for path in artifacts {
        let candidate = Path::new(path);
        if path.contains('\\')
            || candidate.is_absolute()
            || candidate
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(LsModelArchiveError::ManifestInvalid(format!(
                "declared artifact has an unsafe path: {path}"
            )));
        }
        if !exists(path) {
            return Err(LsModelArchiveError::ManifestInvalid(format!(
                "declared artifact is missing: {path}"
            )));
        }
    }
    Ok(())
}

fn validate_archive_checksums<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &ModelManifest,
) -> Result<(), LsModelArchiveError> {
    for (path, expected) in &manifest.checksums {
        let mut entry = archive.by_name(path).map_err(|error| {
            LsModelArchiveError::ManifestInvalid(format!(
                "failed to open checksummed artifact {path}: {error}"
            ))
        })?;
        let mut digest = Sha256::new();
        std::io::copy(&mut entry, &mut digest).map_err(|error| {
            LsModelArchiveError::ManifestInvalid(format!("failed to hash artifact {path}: {error}"))
        })?;
        let actual = format!("{:x}", digest.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(LsModelArchiveError::ManifestInvalid(format!(
                "checksum mismatch for {path}: expected {expected}, got {actual}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const MANIFEST: &str = r#"
id = "formula-recognition/test"
task = "FormulaRecognition"
version = "1.0.0"
adapter = "test-adapter"
[input]
name = "input"
shape = [1, 3, 32, 32]
dtype = "float32"
[[output]]
name = "output"
shape = [1, 8]
dtype = "float32"
[files]
primary = "model.onnx"
"#;

    fn archive(entries: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            for (name, content) in entries {
                writer
                    .start_file(*name, zip::write::FileOptions::default())
                    .unwrap();
                writer.write_all(content).unwrap();
            }
            writer.finish().unwrap();
        }
        bytes.set_position(0);
        bytes
    }

    #[test]
    fn transport_v1_accepts_only_a_root_manifest() {
        let inspection = inspect_lsmodel_archive(archive(&[
            (LSMODEL_MANIFEST_PATH, MANIFEST.as_bytes()),
            ("model.onnx", b"model"),
        ]))
        .unwrap();
        assert_eq!(inspection.layout.transport_version, 1);
        assert_eq!(inspection.manifest.id, "formula-recognition/test");
    }

    #[test]
    fn nested_manifest_error_explains_the_actual_layout() {
        let error =
            inspect_lsmodel_archive(archive(&[("model-x/manifest.toml", MANIFEST.as_bytes())]))
                .unwrap_err()
                .to_string();
        assert!(error.contains("MODEL_PACKAGE_MANIFEST_MISSING"));
        assert!(error.contains("root entries: model-x"));
        assert!(error.contains("model-x/manifest.toml"));
    }

    #[test]
    fn packager_does_not_add_the_source_wrapper_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("model-x");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join(LSMODEL_MANIFEST_PATH), MANIFEST).unwrap();
        std::fs::write(source.join("model.onnx"), b"model").unwrap();
        let output = temporary.path().join("model-x.lsmodel");
        create_lsmodel_archive(&source, &output).unwrap();
        let inspection = inspect_lsmodel_archive(std::fs::File::open(output).unwrap()).unwrap();
        assert_eq!(inspection.manifest.id, "formula-recognition/test");
        assert!(!inspection
            .layout
            .root_entries
            .contains(&"model-x".to_owned()));
    }

    #[test]
    fn generated_manifest_packager_keeps_legacy_source_unchanged() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("legacy");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("model.onnx"), b"model").unwrap();
        let manifest: ModelManifest = toml::from_str(MANIFEST).unwrap();
        let output = temporary.path().join("legacy.lsmodel");
        create_lsmodel_archive_with_manifest(&source, &output, &manifest).unwrap();
        assert!(!source.join(LSMODEL_MANIFEST_PATH).exists());
        let inspection = inspect_lsmodel_archive(std::fs::File::open(output).unwrap()).unwrap();
        assert_eq!(inspection.manifest.id, "formula-recognition/test");
    }

    #[test]
    fn inspection_rejects_missing_or_unsafe_declared_artifacts() {
        let missing =
            inspect_lsmodel_archive(archive(&[(LSMODEL_MANIFEST_PATH, MANIFEST.as_bytes())]))
                .unwrap_err()
                .to_string();
        assert!(missing.contains("declared artifact is missing: model.onnx"));

        let unsafe_manifest = MANIFEST.replace("model.onnx", "../model.onnx");
        let unsafe_error = inspect_lsmodel_archive(archive(&[
            (LSMODEL_MANIFEST_PATH, unsafe_manifest.as_bytes()),
            ("model.onnx", b"model"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(unsafe_error.contains("declared artifact has an unsafe path"));
    }

    #[test]
    fn inspection_rejects_a_declared_checksum_mismatch() {
        let manifest = MANIFEST.replace(
            "[files]",
            "[checksums]\n\"model.onnx\" = \"deadbeef\"\n[files]",
        );
        let error = inspect_lsmodel_archive(archive(&[
            (LSMODEL_MANIFEST_PATH, manifest.as_bytes()),
            ("model.onnx", b"model"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(error.contains("checksum mismatch for model.onnx"));
    }

    #[test]
    fn unsafe_and_duplicate_entries_fail_closed() {
        let unsafe_error =
            inspect_lsmodel_archive(archive(&[("../manifest.toml", MANIFEST.as_bytes())]))
                .unwrap_err()
                .to_string();
        assert!(unsafe_error.contains("MODEL_PACKAGE_ENTRY_INVALID"));

        let duplicate_error = inspect_lsmodel_archive(archive(&[
            (LSMODEL_MANIFEST_PATH, MANIFEST.as_bytes()),
            (LSMODEL_MANIFEST_PATH, MANIFEST.as_bytes()),
        ]))
        .unwrap_err()
        .to_string();
        assert!(duplicate_error.contains("duplicate entry"));
    }
}

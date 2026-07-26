use latexsnipper_foundation::{Result, SnipperError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Validates that a name contains no path traversal characters.
fn validate_name(name: &str) -> Result<()> {
    if name.contains('/') || name.contains('\\') || name.contains("..") || name.is_empty() {
        return Err(SnipperError::Model(format!(
            "Invalid name '{}' — contains path traversal characters",
            name
        )));
    }
    Ok(())
}

/// Validates that a resolved path stays within the base directory.
fn validate_path(base: &Path, resolved: &Path) -> Result<()> {
    match resolved.canonicalize() {
        Ok(canonical) => {
            if !canonical.starts_with(base.canonicalize().unwrap_or_default()) {
                return Err(SnipperError::Model(format!(
                    "Path escapes base directory: {}",
                    resolved.display()
                )));
            }
            Ok(())
        }
        Err(_) => Ok(()), // Path doesn't exist yet, which is fine for creation
    }
}

/// Progress callback for downloads.
pub type DownloadProgress = Box<dyn Fn(DownloadStatus) + Send>;

/// Download status updates.
pub enum DownloadStatus {
    /// Download starting.
    Starting {
        url: String,
        total_bytes: Option<u64>,
    },
    /// Download progress.
    Progress { downloaded: u64, total: Option<u64> },
    /// Extracting archive.
    Extracting { file: String },
    /// Download complete.
    Complete { path: PathBuf },
    /// Download failed.
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct ModelSecurityLimits {
    pub max_archive_bytes: u64,
    pub max_archive_entries: usize,
    pub max_extracted_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_compression_ratio: u64,
}

impl Default for ModelSecurityLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            max_archive_entries: 512,
            max_extracted_bytes: 4 * 1024 * 1024 * 1024,
            max_entry_bytes: 2 * 1024 * 1024 * 1024,
            max_compression_ratio: 200,
        }
    }
}

struct TemporaryDirectoryCleanup(PathBuf);

impl Drop for TemporaryDirectoryCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Manages model files on disk.
pub struct ModelManager {
    models_dir: PathBuf,
    _installed: HashMap<String, Vec<String>>,
    limits: ModelSecurityLimits,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            _installed: HashMap::new(),
            limits: ModelSecurityLimits::default(),
        }
    }

    pub fn with_security_limits(mut self, limits: ModelSecurityLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Get the directory for a model category.
    pub fn category_dir(&self, category: &str) -> PathBuf {
        if let Err(e) = validate_name(category) {
            log::warn!("{}", e);
        }
        self.models_dir.join(category)
    }

    /// Get the directory for a specific variant.
    pub fn variant_dir(&self, category: &str, variant_id: &str) -> PathBuf {
        validate_name(category).ok();
        validate_name(variant_id).ok();
        self.category_dir(category).join(variant_id)
    }

    /// Check if a variant is installed (all files exist).
    pub fn is_installed(&self, category: &str, variant_id: &str, files: &[String]) -> bool {
        let dir = self.variant_dir(category, variant_id);
        files.iter().all(|f| dir.join(f).exists())
    }

    /// List installed variants for a category.
    pub fn list_installed(&self, category: &str) -> Vec<String> {
        let cat_dir = self.category_dir(category);
        if !cat_dir.exists() {
            return Vec::new();
        }
        std::fs::read_dir(&cat_dir)
            .into_iter()
            .flat_map(|entries| entries.filter_map(|e| e.ok()))
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| validate_name(name).is_ok())
            .collect()
    }

    /// Delete a variant from disk.
    pub fn delete_variant(&self, category: &str, variant_id: &str) -> Result<()> {
        validate_name(category)?;
        validate_name(variant_id)?;

        let dir = self.variant_dir(category, variant_id);
        validate_path(&self.models_dir, &dir)?;

        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| {
                SnipperError::Model(format!("Failed to delete {}: {}", dir.display(), e))
            })?;
        }
        Ok(())
    }

    /// Get the models directory path.
    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// Download a model package from a URL and extract it.
    ///
    /// This is a blocking operation. For async usage, see `download_async`.
    /// Unverified downloads are intentionally disabled.
    pub fn download(&self, url: &str, category: &str, variant: &str) -> Result<PathBuf> {
        let _ = (url, category, variant);
        Err(SnipperError::Model(
            "Unverified model downloads are disabled; provide a manifest SHA-256".to_string(),
        ))
    }

    /// Download with progress reporting, SHA-256 integrity verification, and file-level validation.
    pub fn download_with_progress(
        &self,
        url: &str,
        category: &str,
        variant: &str,
        expected_sha256: Option<&str>,
        expected_files: &[String],
        progress: Option<DownloadProgress>,
    ) -> Result<PathBuf> {
        validate_name(category)?;
        validate_name(variant)?;
        let expected_sha256 = expected_sha256.ok_or_else(|| {
            SnipperError::Model(
                "Unverified model downloads are disabled; manifest checksum is missing".to_string(),
            )
        })?;

        let target_dir = self.variant_dir(category, variant);

        // Check if already installed with all expected files present
        if target_dir.exists()
            && std::fs::read_dir(&target_dir)
                .map(|mut e| e.next().is_some())
                .unwrap_or(false)
        {
            if !expected_files.is_empty() && self.all_files_present(&target_dir, expected_files) {
                if let Some(ref cb) = progress {
                    cb(DownloadStatus::Complete {
                        path: target_dir.clone(),
                    });
                }
                return Ok(target_dir);
            }
            // Partial or corrupted installation — re-download
            let _ = std::fs::remove_dir_all(&target_dir);
        }

        // Create temp directory for download
        let temp_dir = self
            .models_dir
            .join(format!(".download_{}_{}", category, variant));
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)
                .map_err(|e| SnipperError::Model(format!("Failed to clean temp dir: {}", e)))?;
        }
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| SnipperError::Model(format!("Failed to create temp dir: {}", e)))?;
        let _cleanup = TemporaryDirectoryCleanup(temp_dir.clone());

        // Determine filename from URL
        let filename = url.rsplit('/').next().unwrap_or("model.zip");
        let zip_path = temp_dir.join(filename);

        // Download
        if let Some(ref cb) = progress {
            cb(DownloadStatus::Starting {
                url: url.to_string(),
                total_bytes: None,
            });
        }

        let response = ureq::get(url)
            .call()
            .map_err(|e| SnipperError::Model(format!("Download failed: {}", e)))?;

        let total_bytes = response
            .header("content-length")
            .and_then(|v| v.parse::<u64>().ok());
        if total_bytes.is_some_and(|size| size > self.limits.max_archive_bytes) {
            return Err(SnipperError::LimitExceeded(format!(
                "model archive exceeds {} bytes",
                self.limits.max_archive_bytes
            )));
        }

        let mut reader = response.into_reader();
        let mut file = std::fs::File::create(&zip_path)
            .map_err(|e| SnipperError::Model(format!("Failed to create file: {}", e)))?;

        let mut downloaded = 0u64;
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer

        loop {
            let bytes_read = std::io::Read::read(&mut reader, &mut buffer)
                .map_err(|e| SnipperError::Model(format!("Read error: {}", e)))?;

            if bytes_read == 0 {
                break;
            }

            std::io::Write::write_all(&mut file, &buffer[..bytes_read])
                .map_err(|e| SnipperError::Model(format!("Write error: {}", e)))?;

            downloaded = downloaded
                .checked_add(bytes_read as u64)
                .ok_or_else(|| SnipperError::LimitExceeded("model size overflow".to_string()))?;
            if downloaded > self.limits.max_archive_bytes {
                return Err(SnipperError::LimitExceeded(format!(
                    "model archive exceeds {} bytes",
                    self.limits.max_archive_bytes
                )));
            }

            if let Some(ref cb) = progress {
                cb(DownloadStatus::Progress {
                    downloaded,
                    total: total_bytes,
                });
            }
        }

        drop(file);

        // Verify SHA-256 checksum if provided
        if let Some(ref cb) = progress {
            cb(DownloadStatus::Extracting {
                file: "verifying checksum".into(),
            });
        }
        let zip_bytes = std::fs::read(&zip_path).map_err(|e| {
            SnipperError::Model(format!(
                "Failed to read downloaded file for verification: {}",
                e
            ))
        })?;
        let actual = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(&zip_bytes))
        };
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            let _ = std::fs::remove_file(&zip_path);
            return Err(SnipperError::Model(format!(
                "SHA-256 checksum mismatch for {}.\n  Expected: {}\n  Actual:   {}\n  The download may be corrupted or tampered with.",
                filename, expected_sha256, actual
            )));
        }

        // Extract
        if let Some(ref cb) = progress {
            cb(DownloadStatus::Extracting {
                file: filename.to_string(),
            });
        }

        self.extract_zip(&zip_path, &temp_dir)?;

        // Move extracted contents to target directory
        // Find the extracted directory (usually has the same name as the zip without extension)
        let extracted_dir = self.find_extracted_dir(&temp_dir, filename)?;

        // Create target parent directory
        std::fs::create_dir_all(target_dir.parent().unwrap_or(&self.models_dir))
            .map_err(|e| SnipperError::Model(format!("Failed to create target dir: {}", e)))?;

        // Move to target
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir).map_err(|e| {
                SnipperError::Model(format!("Failed to remove existing dir: {}", e))
            })?;
        }

        std::fs::rename(&extracted_dir, &target_dir)
            .map_err(|e| SnipperError::Model(format!("Failed to move to target: {}", e)))?;

        // Validate installation: check that all expected files exist
        if !self.all_files_present(&target_dir, expected_files) {
            let _ = std::fs::remove_dir_all(&target_dir);
            let missing: Vec<&str> = expected_files
                .iter()
                .filter(|f| !target_dir.join(f).exists())
                .map(|s| s.as_str())
                .collect();
            return Err(SnipperError::Model(format!(
                "Installation validation failed: missing files in {}: {}\n\
                 The ZIP may have an unexpected directory layout.",
                target_dir.display(),
                missing.join(", ")
            )));
        }

        // Legacy fallback: at least one model file must exist
        if expected_files.is_empty() && !self.dir_contains_model_files(&target_dir) {
            let _ = std::fs::remove_dir_all(&target_dir);
            return Err(SnipperError::Model(format!(
                "Installation validation failed: no model files found in {}.\n\
                 The ZIP may have an unexpected directory layout. \
                 Check that the ZIP root contains the variant directory directly.",
                target_dir.display()
            )));
        }

        // Cleanup temp
        let _ = std::fs::remove_dir_all(&temp_dir);

        if let Some(ref cb) = progress {
            cb(DownloadStatus::Complete {
                path: target_dir.clone(),
            });
        }

        Ok(target_dir)
    }

    /// Extract a zip file.
    fn extract_zip(&self, zip_path: &Path, target_dir: &Path) -> Result<()> {
        let file = std::fs::File::open(zip_path)
            .map_err(|e| SnipperError::Model(format!("Failed to open zip: {}", e)))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| SnipperError::Model(format!("Failed to read zip: {}", e)))?;
        if archive.len() > self.limits.max_archive_entries {
            return Err(SnipperError::LimitExceeded(format!(
                "model archive has {} entries; limit is {}",
                archive.len(),
                self.limits.max_archive_entries
            )));
        }

        let mut total_extracted = 0u64;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| SnipperError::Model(format!("Failed to read zip entry: {}", e)))?;

            let enclosed = entry.enclosed_name().ok_or_else(|| {
                SnipperError::Model(format!("Unsafe model archive path: {}", entry.name()))
            })?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(SnipperError::Model(format!(
                    "Symbolic links are forbidden in model archives: {}",
                    entry.name()
                )));
            }
            if entry.size() > self.limits.max_entry_bytes {
                return Err(SnipperError::LimitExceeded(format!(
                    "model archive entry '{}' exceeds {} bytes",
                    entry.name(),
                    self.limits.max_entry_bytes
                )));
            }
            if entry.size() > 1024 * 1024
                && entry.size()
                    > entry
                        .compressed_size()
                        .max(1)
                        .saturating_mul(self.limits.max_compression_ratio)
            {
                return Err(SnipperError::LimitExceeded(format!(
                    "model archive entry '{}' exceeds compression ratio {}",
                    entry.name(),
                    self.limits.max_compression_ratio
                )));
            }
            total_extracted = total_extracted
                .checked_add(entry.size())
                .ok_or_else(|| SnipperError::LimitExceeded("model size overflow".to_string()))?;
            if total_extracted > self.limits.max_extracted_bytes {
                return Err(SnipperError::LimitExceeded(format!(
                    "model archive expands beyond {} bytes",
                    self.limits.max_extracted_bytes
                )));
            }

            let out_path = target_dir.join(enclosed);

            if entry.is_dir() {
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| SnipperError::Model(format!("Failed to create dir: {}", e)))?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        SnipperError::Model(format!("Failed to create parent dir: {}", e))
                    })?;
                }

                let mut out_file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&out_path)
                    .map_err(|e| SnipperError::Model(format!("Failed to create file: {}", e)))?;

                std::io::copy(&mut entry, &mut out_file)
                    .map_err(|e| SnipperError::Model(format!("Failed to extract file: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Find the extracted variant directory in temp folder.
    ///
    /// ZIP contents are `{variant}/files...`. This finds the single variant directory
    /// that contains model files (not metadata dirs like __MACOSX).
    fn find_extracted_dir(&self, temp_dir: &Path, _zip_filename: &str) -> Result<PathBuf> {
        let mut best: Option<PathBuf> = None;

        for entry in std::fs::read_dir(temp_dir)
            .map_err(|e| SnipperError::Model(format!("Failed to read temp dir: {}", e)))?
        {
            let entry =
                entry.map_err(|e| SnipperError::Model(format!("Failed to read entry: {}", e)))?;

            if !entry.path().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }

            // Prefer a directory that contains model files
            if self.dir_contains_model_files(&entry.path()) {
                return Ok(entry.path());
            }

            if best.is_none() {
                best = Some(entry.path());
            }
        }

        best.ok_or_else(|| SnipperError::Model("No extracted directory found".into()))
    }

    /// Check if a directory contains a supported model artifact or config.
    fn dir_contains_model_files(&self, dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    [
                        ".onnx",
                        ".ort",
                        ".pdmodel",
                        ".pdiparams",
                        ".pte",
                        ".engine",
                        ".plan",
                        ".mlmodel",
                        ".mlpackage",
                        ".mlmodelc",
                    ]
                    .iter()
                    .any(|extension| name.ends_with(extension))
                        || name == "config.json"
                })
            })
            .unwrap_or(false)
    }

    /// Check that all expected files exist in the target directory.
    fn all_files_present(&self, dir: &Path, expected: &[String]) -> bool {
        if expected.is_empty() {
            return true;
        }
        expected.iter().all(|f| dir.join(f).exists())
    }

    /// Download models from a manifest.
    ///
    /// If `all` is true, downloads all categories. Otherwise, downloads only `required` ones.
    /// SHA-256 checksums from the manifest are enforced on every download.
    pub fn download_all(
        &self,
        manifest: &super::manifest::ModelManifest,
        all: bool,
        _progress: Option<DownloadProgress>,
    ) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        for (category, info) in &manifest.categories {
            if !all && !info.required {
                continue;
            }

            let variant_id = info.default.as_deref().unwrap_or("default");
            let variant = info.variants.iter().find(|v| v.id == variant_id);

            if let Some(variant) = variant {
                if let Some(ref zip_file) = variant.zip_file {
                    let expected_files = variant.artifact_paths();
                    let url = format!("{}/{}", manifest.base_url, zip_file);
                    let expected_sha256 = manifest.checksums.get(zip_file).map(|s| s.as_str());
                    let path = self.download_with_progress(
                        &url,
                        category,
                        &variant.id,
                        expected_sha256,
                        &expected_files,
                        None,
                    )?;
                    paths.push(path);
                }
            }
        }

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn workspace() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "latexsnipper-model-security-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn archive(path: &Path, entries: &[(&str, &[u8], Option<u32>)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes, mode) in entries {
            let mut options = zip::write::FileOptions::default();
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn mark_first_entry_as_symlink(path: &Path) {
        let mut bytes = std::fs::read(path).unwrap();
        let offset = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .unwrap();
        bytes[offset + 5] = 3;
        bytes[offset + 38..offset + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn extraction_rejects_traversal_symlinks_and_entry_budgets() {
        let root = workspace();
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let manager =
            ModelManager::new(root.join("models")).with_security_limits(ModelSecurityLimits {
                max_archive_entries: 1,
                ..ModelSecurityLimits::default()
            });

        let crowded = root.join("crowded.zip");
        archive(
            &crowded,
            &[("a/file.onnx", b"a", None), ("b/file.onnx", b"b", None)],
        );
        assert!(manager.extract_zip(&crowded, &target).is_err());

        let manager = ModelManager::new(root.join("models"));
        let traversal = root.join("traversal.zip");
        archive(&traversal, &[("../outside.onnx", b"x", None)]);
        assert!(manager.extract_zip(&traversal, &target).is_err());
        assert!(!root.join("outside.onnx").exists());

        let symlink = root.join("symlink.zip");
        archive(&symlink, &[("variant/link", b"target", None)]);
        mark_first_entry_as_symlink(&symlink);
        assert!(manager.extract_zip(&symlink, &target).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unverified_downloads_are_rejected_before_network_access() {
        let manager = ModelManager::new(PathBuf::from("models"));
        let error = manager
            .download(
                "https://example.invalid/model.zip",
                "formula-det",
                "default",
            )
            .unwrap_err();
        assert!(error.to_string().contains("Unverified"));
        let error = manager
            .download_with_progress(
                "https://example.invalid/model.zip",
                "formula-det",
                "default",
                None,
                &[],
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("checksum"));
    }

    #[test]
    fn ort_format_counts_as_a_model_artifact() {
        let root = workspace();
        let variant = root.join("models").join("formula").join("ort");
        std::fs::create_dir_all(&variant).unwrap();
        std::fs::write(variant.join("model.ort"), b"ORTM").unwrap();
        let manager = ModelManager::new(root.join("models"));
        assert!(manager.dir_contains_model_files(&variant));
        std::fs::remove_dir_all(root).unwrap();
    }
}

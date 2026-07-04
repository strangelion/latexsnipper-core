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
    Starting { url: String, total_bytes: Option<u64> },
    /// Download progress.
    Progress { downloaded: u64, total: Option<u64> },
    /// Extracting archive.
    Extracting { file: String },
    /// Download complete.
    Complete { path: PathBuf },
    /// Download failed.
    Failed { error: String },
}

/// Manages model files on disk.
pub struct ModelManager {
    models_dir: PathBuf,
    _installed: HashMap<String, Vec<String>>,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            _installed: HashMap::new(),
        }
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
    pub fn download(
        &self,
        url: &str,
        category: &str,
        variant: &str,
    ) -> Result<PathBuf> {
        self.download_with_progress(url, category, variant, None)
    }

    /// Download with progress reporting.
    pub fn download_with_progress(
        &self,
        url: &str,
        category: &str,
        variant: &str,
        progress: Option<DownloadProgress>,
) -> Result<PathBuf> {
        validate_name(category)?;
        validate_name(variant)?;

        let target_dir = self.variant_dir(category, variant);

        // Check if already installed
        if target_dir.exists() && std::fs::read_dir(&target_dir).map(|mut e| e.next().is_some()).unwrap_or(false) {
            if let Some(ref cb) = progress {
                cb(DownloadStatus::Complete { path: target_dir.clone() });
            }
            return Ok(target_dir);
        }

        // Create temp directory for download
        let temp_dir = self.models_dir.join(format!(".download_{}_{}", category, variant));
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).map_err(|e| {
                SnipperError::Model(format!("Failed to clean temp dir: {}", e))
            })?;
        }
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            SnipperError::Model(format!("Failed to create temp dir: {}", e))
        })?;

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

        let mut reader = response.into_reader();
        let mut file = std::fs::File::create(&zip_path).map_err(|e| {
            SnipperError::Model(format!("Failed to create file: {}", e))
        })?;

        let mut downloaded = 0u64;
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer

        loop {
            let bytes_read = std::io::Read::read(&mut reader, &mut buffer).map_err(|e| {
                SnipperError::Model(format!("Read error: {}", e))
            })?;

            if bytes_read == 0 {
                break;
            }

            std::io::Write::write_all(&mut file, &buffer[..bytes_read]).map_err(|e| {
                SnipperError::Model(format!("Write error: {}", e))
            })?;

            downloaded += bytes_read as u64;

            if let Some(ref cb) = progress {
                cb(DownloadStatus::Progress {
                    downloaded,
                    total: total_bytes,
                });
            }
        }

        drop(file);

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
        std::fs::create_dir_all(target_dir.parent().unwrap_or(&self.models_dir)).map_err(|e| {
            SnipperError::Model(format!("Failed to create target dir: {}", e))
        })?;

        // Move to target
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir).map_err(|e| {
                SnipperError::Model(format!("Failed to remove existing dir: {}", e))
            })?;
        }

        std::fs::rename(&extracted_dir, &target_dir).map_err(|e| {
            SnipperError::Model(format!("Failed to move to target: {}", e))
        })?;

        // Cleanup temp
        let _ = std::fs::remove_dir_all(&temp_dir);

        if let Some(ref cb) = progress {
            cb(DownloadStatus::Complete { path: target_dir.clone() });
        }

        Ok(target_dir)
    }

    /// Extract a zip file.
    fn extract_zip(&self, zip_path: &Path, target_dir: &Path) -> Result<()> {
        let file = std::fs::File::open(zip_path).map_err(|e| {
            SnipperError::Model(format!("Failed to open zip: {}", e))
        })?;

        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            SnipperError::Model(format!("Failed to read zip: {}", e))
        })?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| {
                SnipperError::Model(format!("Failed to read zip entry: {}", e))
            })?;

            let out_path = target_dir.join(entry.mangled_name());

            if entry.is_dir() {
                std::fs::create_dir_all(&out_path).map_err(|e| {
                    SnipperError::Model(format!("Failed to create dir: {}", e))
                })?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        SnipperError::Model(format!("Failed to create parent dir: {}", e))
                    })?;
                }

                let mut out_file = std::fs::File::create(&out_path).map_err(|e| {
                    SnipperError::Model(format!("Failed to create file: {}", e))
                })?;

                std::io::copy(&mut entry, &mut out_file).map_err(|e| {
                    SnipperError::Model(format!("Failed to extract file: {}", e))
                })?;
            }
        }

        Ok(())
    }

    /// Find the extracted directory in temp folder.
    fn find_extracted_dir(&self, temp_dir: &Path, _zip_filename: &str) -> Result<PathBuf> {
        // Look for directories in temp_dir
        for entry in std::fs::read_dir(temp_dir).map_err(|e| {
            SnipperError::Model(format!("Failed to read temp dir: {}", e))
        })? {
            let entry = entry.map_err(|e| {
                SnipperError::Model(format!("Failed to read entry: {}", e))
            })?;

            if entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
                return Ok(entry.path());
            }
        }

        Err(SnipperError::Model("No extracted directory found".into()))
    }

    /// Download all required models from a manifest.
    pub fn download_all(
        &self,
        manifest: &super::manifest::ModelManifest,
        progress: Option<DownloadProgress>,
    ) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        for (category, info) in &manifest.categories {
            if !info.required {
                continue;
            }

            let variant_id = info.default.as_deref().unwrap_or("default");
            let variant = info.variants.iter().find(|v| v.id == variant_id);

            if let Some(variant) = variant {
                if let Some(ref zip_file) = variant.zip_file {
                    let url = format!("{}/{}", manifest.base_url, zip_file);
                    let path = self.download_with_progress(&url, category, &variant.id, progress.as_ref().map(|_p| {
                        // Create a new progress callback for this download
                        Box::new(move |_status| {
                            // In a real implementation, we'd prefix the status with category info
                        }) as DownloadProgress
                    }))?;
                    paths.push(path);
                }
            }
        }

        Ok(paths)
    }
}

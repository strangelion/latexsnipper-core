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
}

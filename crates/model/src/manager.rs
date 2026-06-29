use std::collections::HashMap;
use std::path::{Path, PathBuf};
use latexsnipper_foundation::{SnipperError, Result};

use crate::config::ModelConfig;
use crate::manifest::ModelManifest;

/// Manages model files on disk.
pub struct ModelManager {
    models_dir: PathBuf,
    installed: HashMap<String, Vec<String>>, // category -> variant IDs
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            installed: HashMap::new(),
        }
    }

    /// Get the directory for a model category.
    pub fn category_dir(&self, category: &str) -> PathBuf {
        self.models_dir.join(category)
    }

    /// Get the directory for a specific variant.
    pub fn variant_dir(&self, category: &str, variant_id: &str) -> PathBuf {
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
            .collect()
    }

    /// Delete a variant from disk.
    pub fn delete_variant(&self, category: &str, variant_id: &str) -> Result<()> {
        let dir = self.variant_dir(category, variant_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| SnipperError::Model(format!("Failed to delete {}: {}", dir.display(), e)))?;
        }
        Ok(())
    }

    /// Get the models directory path.
    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }
}

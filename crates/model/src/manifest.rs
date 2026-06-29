use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use latexsnipper_foundation::{SnipperError, Result};

/// Model manifest describing available models and their variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub source_id: String,
    pub source_label: String,
    pub version: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    #[serde(default)]
    pub checksums: HashMap<String, String>,
    pub categories: HashMap<String, CategoryInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    #[serde(default)]
    pub required: bool,
    pub default: Option<String>,
    pub variants: Vec<VariantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantInfo {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub files: Vec<String>,
    #[serde(default)]
    pub zip_file: Option<String>,
}

impl ModelManifest {
    /// Load manifest from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SnipperError::Model(format!("Failed to read manifest: {}", e)))?;
        Self::parse(&content)
    }

    /// Parse manifest from JSON string.
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| SnipperError::Model(format!("Invalid manifest: {}", e)))
    }

    /// Validate manifest structure.
    pub fn validate(&self) -> Result<()> {
        if self.source_id.is_empty() {
            return Err(SnipperError::Model("Missing source_id".into()));
        }
        if self.categories.is_empty() {
            return Err(SnipperError::Model("No categories defined".into()));
        }
        for (cat, info) in &self.categories {
            if info.variants.is_empty() {
                return Err(SnipperError::Model(format!("Category {} has no variants", cat)));
            }
            for v in &info.variants {
                if v.id.is_empty() {
                    return Err(SnipperError::Model(format!("Variant in {} has empty id", cat)));
                }
            }
        }
        Ok(())
    }

    /// Verify SHA256 checksum of a file.
    pub fn verify_checksum(&self, filename: &str, data: &[u8]) -> Result<bool> {
        if let Some(expected) = self.checksums.get(filename) {
            use sha2::{Sha256, Digest};
            let hash = Sha256::digest(data);
            let hex_hash = hex::encode(hash);
            Ok(hex_hash == *expected)
        } else {
            Ok(true) // No checksum to verify
        }
    }
}

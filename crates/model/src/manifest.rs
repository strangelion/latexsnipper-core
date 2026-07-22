use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "native")]
use std::path::Path;

use crate::{ModelManifestV3, MODEL_MANIFEST_SCHEMA_VERSION_V3};

/// Version-aware model manifest loader. Unknown future schemas are rejected
/// instead of being reinterpreted as the legacy shape.
#[derive(Debug, Clone)]
pub enum LoadedModelManifest {
    V2(ModelManifest),
    V3(ModelManifestV3),
}

impl LoadedModelManifest {
    pub fn parse(json: &str) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| SnipperError::Model(format!("Invalid manifest: {error}")))?;
        match raw
            .get("schemaVersion")
            .or_else(|| raw.get("schema_version"))
        {
            Some(version) => {
                let version = version.as_u64().ok_or_else(|| {
                    SnipperError::Model("Manifest schema version must be an integer".to_string())
                })?;
                if version == MODEL_MANIFEST_SCHEMA_VERSION_V3 as u64 {
                    let manifest: ModelManifestV3 =
                        serde_json::from_value(raw).map_err(|error| {
                            SnipperError::Model(format!("Invalid model manifest v3: {error}"))
                        })?;
                    manifest.validate_contract().map_err(|error| {
                        SnipperError::Model(format!("Invalid model manifest v3 contract: {error}"))
                    })?;
                    Ok(Self::V3(manifest))
                } else if version == 2 {
                    serde_json::from_value(raw)
                        .map(Self::V2)
                        .map_err(|error| SnipperError::Model(format!("Invalid manifest: {error}")))
                } else {
                    Err(SnipperError::Model(format!(
                        "Unsupported model manifest schema version {version}"
                    )))
                }
            }
            None => serde_json::from_value(raw)
                .map(Self::V2)
                .map_err(|error| SnipperError::Model(format!("Invalid manifest: {error}"))),
        }
    }

    #[cfg(feature = "native")]
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|error| SnipperError::Model(format!("Failed to read manifest: {error}")))?;
        Self::parse(&content)
    }

    #[cfg(feature = "native")]
    pub fn download(url: &str) -> Result<Self> {
        let response = ureq::get(url).call().map_err(|error| {
            SnipperError::Model(format!("Failed to download manifest: {error}"))
        })?;
        let body = response.into_string().map_err(|error| {
            SnipperError::Model(format!("Failed to read manifest response: {error}"))
        })?;
        Self::parse(&body)
    }

    #[cfg(feature = "native")]
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                SnipperError::Model(format!("Failed to create manifest directory: {error}"))
            })?;
        }
        let json = match self {
            Self::V2(manifest) => serde_json::to_string_pretty(manifest),
            Self::V3(manifest) => serde_json::to_string_pretty(manifest),
        }
        .map_err(|error| SnipperError::Model(format!("Failed to serialize manifest: {error}")))?;
        std::fs::write(path, json)
            .map_err(|error| SnipperError::Model(format!("Failed to write manifest: {error}")))
    }

    pub fn into_runtime_manifest(self) -> Result<ModelManifest> {
        match self {
            Self::V2(manifest) => {
                manifest.validate()?;
                Ok(manifest)
            }
            Self::V3(manifest) => manifest
                .to_runtime_adapter()
                .map_err(|error| SnipperError::Model(error.to_string())),
        }
    }

    pub const fn schema_version(&self) -> u32 {
        match self {
            Self::V2(_) => 2,
            Self::V3(_) => 3,
        }
    }
}

/// Model manifest describing available models and their variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    #[serde(default)]
    pub required: bool,
    pub default: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub variants: Vec<VariantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantInfo {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    /// Executable variants for different runtimes. Empty means derive an
    /// implicit ONNX variant from `files` for legacy catalogs.
    #[serde(default)]
    pub runtime_variants: Vec<crate::RuntimeVariant>,
    #[serde(default)]
    pub zip_file: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl VariantInfo {
    /// Complete deterministic package file list across legacy and runtime
    /// variants. Runtime artifacts may include directories such as `.mlpackage`.
    pub fn artifact_paths(&self) -> Vec<String> {
        let mut paths = std::collections::BTreeSet::new();
        paths.extend(self.files.iter().cloned());
        for variant in &self.runtime_variants {
            paths.extend(variant.artifacts.values().cloned());
        }
        paths.into_iter().collect()
    }
}

impl ModelManifest {
    /// Load manifest from a JSON file.
    #[cfg(feature = "native")]
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
                return Err(SnipperError::Model(format!(
                    "Category {} has no variants",
                    cat
                )));
            }
            for v in &info.variants {
                if v.id.is_empty() {
                    return Err(SnipperError::Model(format!(
                        "Variant in {} has empty id",
                        cat
                    )));
                }
            }
        }
        Ok(())
    }

    /// Verify SHA256 checksum of a file.
    pub fn verify_checksum(&self, filename: &str, data: &[u8]) -> Result<bool> {
        if let Some(expected) = self.checksums.get(filename) {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(data);
            let hex_hash = hex::encode(hash);
            Ok(hex_hash == *expected)
        } else {
            Ok(true) // No checksum to verify
        }
    }

    /// Download manifest from a remote URL.
    #[cfg(feature = "native")]
    pub fn download(url: &str) -> Result<Self> {
        let response = ureq::get(url)
            .call()
            .map_err(|e| SnipperError::Model(format!("Failed to download manifest: {}", e)))?;

        let body = response
            .into_string()
            .map_err(|e| SnipperError::Model(format!("Failed to read manifest response: {}", e)))?;

        Self::parse(&body)
    }

    /// Save manifest to a file, creating parent directories if needed.
    #[cfg(feature = "native")]
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SnipperError::Model(format!("Failed to create manifest directory: {}", e))
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SnipperError::Model(format!("Failed to serialize manifest: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| SnipperError::Model(format!("Failed to write manifest: {}", e)))?;
        Ok(())
    }
}

/// Default manifest URL for the official release.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/strangelion/latexsnipper-core/releases/download/models-v2.0.0/model-manifest.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_camelcase_manifest() {
        // Verify that the template manifest (camelCase) parses correctly into snake_case fields.
        let json = r#"{
            "sourceId": "official",
            "sourceLabel": "LaTeXSnipper Official",
            "version": "2.0.0",
            "baseUrl": "https://example.com",
            "mirrors": [],
            "checksums": { "model.zip": "abc123" },
            "categories": {
                "formula-det": {
                    "label": "Formula Detection",
                    "required": false,
                    "default": "yolov8-mfd",
                    "variants": [{
                        "id": "yolov8-mfd",
                        "label": "YOLOv8",
                        "adapter": "yolov8-detection-v1",
                        "modelType": "yolov8",
                        "files": ["model.onnx", "config.json"],
                        "zipFile": "latexsnipper-formula-det.zip",
                        "notes": "Main formula detector"
                    }]
                }
            }
        }"#;
        let manifest = ModelManifest::parse(json).expect("Failed to parse camelCase manifest");
        assert_eq!(manifest.source_id, "official");
        assert_eq!(manifest.source_label, "LaTeXSnipper Official");
        assert_eq!(manifest.version, "2.0.0");
        let fd = manifest.categories.get("formula-det").unwrap();
        assert_eq!(fd.default.as_deref(), Some("yolov8-mfd"));
        assert!(!fd.required);
        let v = &fd.variants[0];
        assert_eq!(v.id, "yolov8-mfd");
        assert_eq!(v.zip_file.as_deref(), Some("latexsnipper-formula-det.zip"));
        assert!(v.files.contains(&"model.onnx".to_string()));
    }

    #[test]
    fn versioned_loader_rejects_unknown_future_schema() {
        let error = LoadedModelManifest::parse(r#"{"schemaVersion":4}"#).unwrap_err();
        assert!(error
            .to_string()
            .contains("Unsupported model manifest schema version 4"));
    }

    #[test]
    fn versioned_loader_accepts_an_explicit_v2_schema_adapter() {
        let loaded = LoadedModelManifest::parse(
            r#"{
                "schemaVersion": 2,
                "sourceId": "fixture",
                "sourceLabel": "Fixture",
                "version": "2.0.0",
                "categories": {
                    "formula": {"default": null, "variants": [{"id": "v1", "files": ["model.onnx"]}]}
                }
            }"#,
        )
        .unwrap();
        assert!(matches!(loaded, LoadedModelManifest::V2(_)));
    }
}

use std::collections::BTreeMap;

use latexsnipper_foundation::{
    MigrationOutcome, MigrationReport, MigrationStatus, MigrationWarning,
};
use serde::{Deserialize, Serialize};

use crate::manifest::{CategoryInfo, ModelManifest, VariantInfo};

pub const MODEL_MANIFEST_SCHEMA_VERSION_V3: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelArtifactKindV3 {
    Model,
    Config,
    Tokenizer,
    Keys,
    Labels,
    Package,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArtifactV3 {
    pub path: String,
    pub kind: ModelArtifactKindV3,
    pub sha256: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelEvidenceStatusV3 {
    Unavailable,
    Experimental,
    Validated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEvidenceV3 {
    pub status: ModelEvidenceStatusV3,
    pub corpus_id: Option<String>,
    pub benchmark_id: Option<String>,
    #[serde(default)]
    pub report_path: Option<String>,
    #[serde(default)]
    pub report_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileV3 {
    pub id: String,
    pub label: Option<String>,
    pub adapter: String,
    pub model_type: String,
    pub source: String,
    pub license: String,
    pub artifacts: Vec<ModelArtifactV3>,
    #[serde(default)]
    pub supported_modes: Vec<String>,
    #[serde(default)]
    pub supported_languages: Vec<String>,
    #[serde(default)]
    pub runtime_compatibility: Vec<String>,
    pub memory_estimate_bytes: Option<u64>,
    pub preprocessing_schema: Option<serde_json::Value>,
    pub postprocessing_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub evidence: ModelEvidenceV3,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCategoryV3 {
    pub required: bool,
    pub default_profile: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub profiles: Vec<ModelProfileV3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifestV3 {
    pub schema_version: u32,
    pub source_id: String,
    pub source_label: String,
    pub version: String,
    pub base_url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    pub categories: BTreeMap<String, ModelCategoryV3>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelManifestV3Error {
    #[error("invalid v2 model manifest: {0}")]
    InvalidSource(String),
    #[error("invalid model manifest semantic version: {0}")]
    InvalidVersion(String),
    #[error("profile '{profile}' is missing a digest for '{path}'")]
    MissingDigest { profile: String, path: String },
    #[error("profile '{profile}' has an invalid SHA-256 digest for '{path}'")]
    InvalidDigest { profile: String, path: String },
    #[error("model manifest contract is internally inconsistent: {0}")]
    InvalidContract(String),
}

impl ModelManifestV3 {
    pub fn migrate_from_v2(
        source: ModelManifest,
    ) -> Result<MigrationOutcome<Self>, ModelManifestV3Error> {
        source
            .validate()
            .map_err(|error| ModelManifestV3Error::InvalidSource(error.to_string()))?;
        semver::Version::parse(&source.version)
            .map_err(|error| ModelManifestV3Error::InvalidVersion(error.to_string()))?;

        let mut report = MigrationReport::new(
            "model-manifest",
            "2-legacy",
            "model-manifest",
            MODEL_MANIFEST_SCHEMA_VERSION_V3.to_string(),
            MigrationStatus::Migrated,
        );
        let mut categories = BTreeMap::new();
        for (category_id, category) in source.categories {
            let mut profiles = Vec::with_capacity(category.variants.len());
            for variant in category.variants {
                let mut artifacts = Vec::with_capacity(variant.files.len() + 1);
                for path in &variant.files {
                    let digest = source.checksums.get(path).ok_or_else(|| {
                        ModelManifestV3Error::MissingDigest {
                            profile: variant.id.clone(),
                            path: path.clone(),
                        }
                    })?;
                    if !valid_sha256(digest) {
                        return Err(ModelManifestV3Error::InvalidDigest {
                            profile: variant.id.clone(),
                            path: path.clone(),
                        });
                    }
                    artifacts.push(ModelArtifactV3 {
                        path: path.clone(),
                        kind: artifact_kind(path),
                        sha256: digest.clone(),
                        size_bytes: None,
                    });
                }
                if let Some(package) = &variant.zip_file {
                    let digest = source.checksums.get(package).ok_or_else(|| {
                        ModelManifestV3Error::MissingDigest {
                            profile: variant.id.clone(),
                            path: package.clone(),
                        }
                    })?;
                    if !valid_sha256(digest) {
                        return Err(ModelManifestV3Error::InvalidDigest {
                            profile: variant.id.clone(),
                            path: package.clone(),
                        });
                    }
                    artifacts.push(ModelArtifactV3 {
                        path: package.clone(),
                        kind: ModelArtifactKindV3::Package,
                        sha256: digest.clone(),
                        size_bytes: None,
                    });
                }

                let adapter =
                    required_profile_field(&variant.id, "adapter", variant.adapter, &mut report);
                let model_type = required_profile_field(
                    &variant.id,
                    "modelType",
                    variant.model_type,
                    &mut report,
                );
                let profile_source =
                    required_profile_field(&variant.id, "source", variant.source, &mut report);
                let license =
                    required_profile_field(&variant.id, "license", variant.license, &mut report);
                report.require_manual_action(
                    MigrationWarning::new(
                        "MODEL_V3_PROFILE_EVIDENCE_REQUIRED",
                        format!(
                            "Profile '{}' requires explicit modes, languages, runtime compatibility, schemas, and validation evidence",
                            variant.id
                        ),
                    )
                    .with_field(format!("categories.{category_id}.profiles.{}", variant.id)),
                );

                profiles.push(ModelProfileV3 {
                    id: variant.id,
                    label: variant.label,
                    adapter,
                    model_type,
                    source: profile_source,
                    license,
                    artifacts,
                    supported_modes: Vec::new(),
                    supported_languages: Vec::new(),
                    runtime_compatibility: Vec::new(),
                    memory_estimate_bytes: None,
                    preprocessing_schema: None,
                    postprocessing_schema: None,
                    output_schema: None,
                    evidence: ModelEvidenceV3 {
                        status: ModelEvidenceStatusV3::Unavailable,
                        corpus_id: None,
                        benchmark_id: None,
                        report_path: None,
                        report_sha256: None,
                    },
                    notes: variant.notes,
                });
            }
            categories.insert(
                category_id,
                ModelCategoryV3 {
                    required: category.required,
                    default_profile: category.default,
                    label: category.label,
                    description: category.description,
                    profiles,
                },
            );
        }

        Ok(MigrationOutcome {
            value: Self {
                schema_version: MODEL_MANIFEST_SCHEMA_VERSION_V3,
                source_id: source.source_id,
                source_label: source.source_label,
                version: source.version,
                base_url: source.base_url,
                mirrors: source.mirrors,
                categories,
            },
            report,
        })
    }

    pub fn validate_contract(&self) -> Result<(), ModelManifestV3Error> {
        if self.schema_version != MODEL_MANIFEST_SCHEMA_VERSION_V3 {
            return Err(ModelManifestV3Error::InvalidContract(
                "unsupported schema version".to_string(),
            ));
        }
        semver::Version::parse(&self.version)
            .map_err(|error| ModelManifestV3Error::InvalidVersion(error.to_string()))?;
        if self.source_id.trim().is_empty()
            || self.source_label.trim().is_empty()
            || self.base_url.trim().is_empty()
            || self.categories.is_empty()
        {
            return Err(ModelManifestV3Error::InvalidContract(
                "source and categories are required".to_string(),
            ));
        }
        for (category_id, category) in &self.categories {
            if category_id.trim().is_empty() || category.profiles.is_empty() {
                return Err(ModelManifestV3Error::InvalidContract(format!(
                    "category '{category_id}' has no profiles"
                )));
            }
            let mut profile_ids = std::collections::BTreeSet::new();
            for profile in &category.profiles {
                if profile.id.trim().is_empty()
                    || !profile_ids.insert(profile.id.as_str())
                    || profile.adapter.trim().is_empty()
                    || profile.model_type.trim().is_empty()
                    || profile.source.trim().is_empty()
                    || profile.license.trim().is_empty()
                    || profile.artifacts.is_empty()
                {
                    return Err(ModelManifestV3Error::InvalidContract(format!(
                        "profile '{}' has incomplete identity or artifacts",
                        profile.id
                    )));
                }
                let mut artifact_paths = std::collections::BTreeSet::new();
                for artifact in &profile.artifacts {
                    if !valid_package_path(&artifact.path)
                        || !artifact_paths.insert(artifact.path.as_str())
                        || !valid_sha256(&artifact.sha256)
                    {
                        return Err(ModelManifestV3Error::InvalidDigest {
                            profile: profile.id.clone(),
                            path: artifact.path.clone(),
                        });
                    }
                }
                if profile.evidence.status == ModelEvidenceStatusV3::Validated
                    && (profile.supported_modes.is_empty()
                        || profile.runtime_compatibility.is_empty()
                        || profile.memory_estimate_bytes.is_none()
                        || profile.preprocessing_schema.is_none()
                        || profile.postprocessing_schema.is_none()
                        || profile.output_schema.is_none()
                        || profile
                            .evidence
                            .corpus_id
                            .as_deref()
                            .is_none_or(str::is_empty)
                        || profile
                            .evidence
                            .benchmark_id
                            .as_deref()
                            .is_none_or(str::is_empty)
                        || profile
                            .evidence
                            .report_path
                            .as_deref()
                            .is_none_or(|path| !valid_package_path(path))
                        || profile
                            .evidence
                            .report_sha256
                            .as_deref()
                            .is_none_or(|digest| !valid_sha256(digest)))
                {
                    return Err(ModelManifestV3Error::InvalidContract(format!(
                        "validated profile '{}' lacks executable evidence metadata",
                        profile.id
                    )));
                }
            }
            if category
                .default_profile
                .as_deref()
                .is_some_and(|profile| !profile_ids.contains(profile))
            {
                return Err(ModelManifestV3Error::InvalidContract(format!(
                    "category '{category_id}' names an unknown default profile"
                )));
            }
        }
        Ok(())
    }

    /// Build the legacy downloader view used by the current native model
    /// manager. Only explicitly evidenced v3 profiles are exposed.
    pub fn to_runtime_adapter(&self) -> Result<ModelManifest, ModelManifestV3Error> {
        self.validate_contract()?;
        let mut checksums = std::collections::HashMap::new();
        let mut categories = std::collections::HashMap::new();
        for (category_id, category) in &self.categories {
            let mut variants = Vec::new();
            for profile in &category.profiles {
                if profile.evidence.status == ModelEvidenceStatusV3::Unavailable {
                    continue;
                }
                let mut files = Vec::new();
                let mut package = None;
                for artifact in &profile.artifacts {
                    if checksums
                        .insert(artifact.path.clone(), artifact.sha256.clone())
                        .is_some_and(|existing| existing != artifact.sha256)
                    {
                        return Err(ModelManifestV3Error::InvalidContract(format!(
                            "artifact '{}' has conflicting digests",
                            artifact.path
                        )));
                    }
                    if artifact.kind == ModelArtifactKindV3::Package {
                        if package.replace(artifact.path.clone()).is_some() {
                            return Err(ModelManifestV3Error::InvalidContract(format!(
                                "profile '{}' declares multiple package artifacts",
                                profile.id
                            )));
                        }
                    } else {
                        files.push(artifact.path.clone());
                    }
                }
                if files.is_empty() {
                    return Err(ModelManifestV3Error::InvalidContract(format!(
                        "profile '{}' has no runtime files",
                        profile.id
                    )));
                }
                variants.push(VariantInfo {
                    id: profile.id.clone(),
                    label: profile.label.clone(),
                    adapter: Some(profile.adapter.clone()),
                    model_type: Some(profile.model_type.clone()),
                    source: Some(profile.source.clone()),
                    license: Some(profile.license.clone()),
                    files,
                    zip_file: package,
                    notes: profile.notes.clone(),
                });
            }
            if variants.is_empty() {
                return Err(ModelManifestV3Error::InvalidContract(format!(
                    "category '{category_id}' has no evidenced runtime profile"
                )));
            }
            if category
                .default_profile
                .as_ref()
                .is_some_and(|default| !variants.iter().any(|variant| &variant.id == default))
            {
                return Err(ModelManifestV3Error::InvalidContract(format!(
                    "category '{category_id}' default profile is unavailable"
                )));
            }
            categories.insert(
                category_id.clone(),
                CategoryInfo {
                    required: category.required,
                    default: category.default_profile.clone(),
                    label: category.label.clone(),
                    description: category.description.clone(),
                    variants,
                },
            );
        }
        Ok(ModelManifest {
            source_id: self.source_id.clone(),
            source_label: self.source_label.clone(),
            version: self.version.clone(),
            base_url: self.base_url.clone(),
            mirrors: self.mirrors.clone(),
            checksums,
            categories,
        })
    }
}

fn required_profile_field(
    profile: &str,
    field: &str,
    value: Option<String>,
    report: &mut MigrationReport,
) -> String {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        return value;
    }
    report.require_manual_action(
        MigrationWarning::new(
            "MODEL_V3_PROFILE_FIELD_REQUIRED",
            format!("Profile '{profile}' requires an explicit {field}"),
        )
        .with_field(format!("profiles.{profile}.{field}")),
    );
    String::new()
}

fn artifact_kind(path: &str) -> ModelArtifactKindV3 {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".onnx") || lower.ends_with(".ort") {
        ModelArtifactKindV3::Model
    } else if lower.contains("tokenizer") || lower.ends_with("tokenizer.json") {
        ModelArtifactKindV3::Tokenizer
    } else if lower.contains("keys") || lower.contains("dict") {
        ModelArtifactKindV3::Keys
    } else if lower.ends_with("config.json")
        || lower.ends_with("config.yaml")
        || lower.ends_with("config.yml")
    {
        ModelArtifactKindV3::Config
    } else if lower.contains("label") {
        ModelArtifactKindV3::Labels
    } else {
        ModelArtifactKindV3::Other
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_package_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::manifest::{CategoryInfo, VariantInfo};

    use super::*;

    fn source_manifest(with_digest: bool) -> ModelManifest {
        let mut checksums = HashMap::new();
        if with_digest {
            checksums.insert("model.onnx".to_string(), "b".repeat(64));
        }
        ModelManifest {
            source_id: "fixture".to_string(),
            source_label: "Fixture".to_string(),
            version: "2.0.0".to_string(),
            base_url: "https://example.invalid".to_string(),
            mirrors: Vec::new(),
            checksums,
            categories: HashMap::from([(
                "text-rec".to_string(),
                CategoryInfo {
                    required: true,
                    default: Some("fixture".to_string()),
                    label: None,
                    description: None,
                    variants: vec![VariantInfo {
                        id: "fixture".to_string(),
                        label: None,
                        adapter: Some("ctc-v1".to_string()),
                        model_type: Some("ctc".to_string()),
                        source: Some("fixture-source".to_string()),
                        license: Some("Apache-2.0".to_string()),
                        files: vec!["model.onnx".to_string()],
                        zip_file: None,
                        notes: None,
                    }],
                },
            )]),
        }
    }

    #[test]
    fn migration_requires_exact_per_artifact_digests() {
        assert_eq!(
            ModelManifestV3::migrate_from_v2(source_manifest(false)).unwrap_err(),
            ModelManifestV3Error::MissingDigest {
                profile: "fixture".to_string(),
                path: "model.onnx".to_string(),
            }
        );
    }

    #[test]
    fn migration_requires_package_digest() {
        let mut source = source_manifest(true);
        source.categories.get_mut("text-rec").unwrap().variants[0].zip_file =
            Some("models.zip".to_string());
        assert_eq!(
            ModelManifestV3::migrate_from_v2(source).unwrap_err(),
            ModelManifestV3Error::MissingDigest {
                profile: "fixture".to_string(),
                path: "models.zip".to_string(),
            }
        );
    }

    #[test]
    fn migrated_profiles_remain_unavailable_until_evidence_is_authored() {
        let migrated = ModelManifestV3::migrate_from_v2(source_manifest(true)).unwrap();
        assert_eq!(
            migrated.report.status,
            MigrationStatus::RequiresManualAction
        );
        let profile = &migrated.value.categories["text-rec"].profiles[0];
        assert_eq!(profile.evidence.status, ModelEvidenceStatusV3::Unavailable);
        assert!(profile.supported_modes.is_empty());
        migrated.value.validate_contract().unwrap();
    }

    #[test]
    fn validated_profiles_require_safe_paths_and_complete_evidence() {
        let mut migrated = ModelManifestV3::migrate_from_v2(source_manifest(true))
            .unwrap()
            .value;
        let profile = &mut migrated.categories.get_mut("text-rec").unwrap().profiles[0];
        profile.artifacts[0].path = "../model.onnx".to_string();
        assert!(matches!(
            migrated.validate_contract(),
            Err(ModelManifestV3Error::InvalidDigest { .. })
        ));

        let profile = &mut migrated.categories.get_mut("text-rec").unwrap().profiles[0];
        profile.artifacts[0].path = "model.onnx".to_string();
        profile.evidence.status = ModelEvidenceStatusV3::Validated;
        assert!(matches!(
            migrated.validate_contract(),
            Err(ModelManifestV3Error::InvalidContract(_))
        ));

        let profile = &mut migrated.categories.get_mut("text-rec").unwrap().profiles[0];
        profile.supported_modes = vec!["text".to_string()];
        profile.supported_languages = vec!["en".to_string()];
        profile.runtime_compatibility = vec!["onnxruntime-cpu".to_string()];
        profile.memory_estimate_bytes = Some(1024);
        profile.preprocessing_schema = Some(serde_json::json!({"version": 1}));
        profile.postprocessing_schema = Some(serde_json::json!({"version": 1}));
        profile.output_schema = Some(serde_json::json!({"version": 1}));
        profile.evidence.corpus_id = Some("repository-latin-text-v1".to_string());
        profile.evidence.benchmark_id = Some("ocr-evidence-v1".to_string());
        profile.evidence.report_path = Some("evidence/text-rec.json".to_string());
        profile.evidence.report_sha256 = Some("a".repeat(64));
        migrated.validate_contract().unwrap();
    }

    #[test]
    fn runtime_adapter_refuses_unavailable_profiles_and_accepts_evidenced_profiles() {
        let mut migrated = ModelManifestV3::migrate_from_v2(source_manifest(true))
            .unwrap()
            .value;
        assert!(matches!(
            migrated.to_runtime_adapter(),
            Err(ModelManifestV3Error::InvalidContract(_))
        ));

        migrated.categories.get_mut("text-rec").unwrap().profiles[0]
            .evidence
            .status = ModelEvidenceStatusV3::Experimental;
        let adapted = migrated.to_runtime_adapter().unwrap();
        assert_eq!(adapted.categories["text-rec"].variants[0].id, "fixture");
        assert_eq!(adapted.checksums["model.onnx"], "b".repeat(64));
    }
}

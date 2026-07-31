//! Trusted model-quality baseline registry.
//!
//! Quality is release evidence, not model-package metadata. A model manifest
//! therefore cannot promote itself to `Validated`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use latexsnipper_api_types::{CoreErrorCode, ModelQualityReadiness, ModelQualityStatus};
use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelQualityKey {
    pub model_id: String,
    pub model_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQualityMetrics {
    pub normalized_exact: f64,
    pub cer: f64,
    pub ter: f64,
    #[serde(default)]
    pub hard_negative_fpr: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQualityThresholds {
    pub minimum_normalized_exact: f64,
    pub maximum_cer: f64,
    #[serde(default)]
    pub maximum_hard_negative_fpr: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQualityRecord {
    pub schema_version: u32,
    pub model_id: String,
    pub model_version: String,
    pub model_sha256: String,
    pub dataset_version: String,
    pub runtime: String,
    pub provider: String,
    pub generated_by_commit: String,
    pub evidence_sha256: String,
    #[serde(default)]
    pub contains_synthetic: bool,
    #[serde(default)]
    pub contains_real: bool,
    #[serde(default)]
    pub contains_hard_negatives: bool,
    pub metrics: ModelQualityMetrics,
    pub thresholds: ModelQualityThresholds,
}

#[derive(Debug, Clone)]
struct TrustedRecord {
    record: ModelQualityRecord,
    baseline_sha256: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineTrustIndex {
    schema_version: u32,
    files: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelQualityRegistry {
    baselines: HashMap<ModelQualityKey, TrustedRecord>,
}

#[derive(Debug, Clone)]
pub struct ModelQualityValidation<'a> {
    pub model_id: &'a str,
    pub model_version: &'a str,
    pub model_sha256: &'a str,
    pub dataset_version: Option<&'a str>,
    pub runtime: Option<&'a str>,
    pub provider: Option<&'a str>,
}

impl ModelQualityRegistry {
    pub fn load(root: &Path) -> Result<Self> {
        if !root.exists() {
            return Ok(Self::default());
        }
        let index_path = root.join("index.json");
        let index_bytes = fs::read(&index_path).map_err(|error| {
            SnipperError::Model(format!(
                "trusted quality baseline index '{}' is required: {error}",
                index_path.display()
            ))
        })?;
        let trust_index: BaselineTrustIndex =
            serde_json::from_slice(&index_bytes).map_err(|error| {
                SnipperError::Model(format!(
                    "invalid quality baseline index '{}': {error}",
                    index_path.display()
                ))
            })?;
        if trust_index.schema_version != 1 {
            return Err(SnipperError::Model(format!(
                "unsupported quality baseline index schema {}",
                trust_index.schema_version
            )));
        }
        let mut files = Vec::new();
        collect_json_files(root, &mut files)?;
        files.sort();

        let mut registry = Self::default();
        for path in files {
            let bytes = fs::read(&path).map_err(|error| {
                SnipperError::Model(format!(
                    "failed to read quality baseline '{}': {error}",
                    path.display()
                ))
            })?;
            let relative = path
                .strip_prefix(root)
                .map_err(|error| SnipperError::Model(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let baseline_sha256 = sha256(&bytes);
            let expected_baseline_sha256 = trust_index.files.get(&relative).ok_or_else(|| {
                SnipperError::Model(format!(
                    "quality baseline '{relative}' is not present in the trusted index"
                ))
            })?;
            validate_sha256("baseline index hash", expected_baseline_sha256, &index_path)?;
            if !baseline_sha256.eq_ignore_ascii_case(expected_baseline_sha256) {
                return Err(SnipperError::Model(format!(
                    "quality baseline hash mismatch for '{relative}'"
                )));
            }
            let record: ModelQualityRecord = serde_json::from_slice(&bytes).map_err(|error| {
                SnipperError::Model(format!(
                    "invalid quality baseline '{}': {error}",
                    path.display()
                ))
            })?;
            if record.schema_version != 1 {
                return Err(SnipperError::Model(format!(
                    "unsupported quality baseline schema {} in '{}'",
                    record.schema_version,
                    path.display()
                )));
            }
            validate_sha256("modelSha256", &record.model_sha256, &path)?;
            validate_sha256("evidenceSha256", &record.evidence_sha256, &path)?;
            validate_commit(&record.generated_by_commit, &path)?;
            let key = ModelQualityKey {
                model_id: record.model_id.clone(),
                model_version: record.model_version.clone(),
            };
            if registry.baselines.contains_key(&key) {
                return Err(SnipperError::Model(format!(
                    "duplicate quality baseline for '{}@{}'",
                    key.model_id, key.model_version
                )));
            }
            registry.baselines.insert(
                key,
                TrustedRecord {
                    record,
                    baseline_sha256,
                    path,
                },
            );
        }
        Ok(registry)
    }

    pub fn validate(&self, expected: ModelQualityValidation<'_>) -> ModelQualityReadiness {
        if expected.runtime.is_none_or(is_unknown_identity)
            || expected.provider.is_none_or(is_unknown_identity)
        {
            return quality_failure(
                &expected,
                ModelQualityStatus::BaselineMissing,
                CoreErrorCode::ModelBaselineMissing,
                "runtime and effective provider must be known before quality evidence can match"
                    .to_owned(),
            );
        }
        let exact_key = ModelQualityKey {
            model_id: expected.model_id.to_owned(),
            model_version: expected.model_version.to_owned(),
        };
        let trusted = self.baselines.get(&exact_key).or_else(|| {
            let short_id = expected.model_id.rsplit('/').next()?;
            self.baselines.get(&ModelQualityKey {
                model_id: short_id.to_owned(),
                model_version: expected.model_version.to_owned(),
            })
        });
        let Some(trusted) = trusted else {
            return quality_failure(
                &expected,
                ModelQualityStatus::BaselineMissing,
                CoreErrorCode::ModelBaselineMissing,
                "no trusted quality baseline matches this model and version".to_owned(),
            );
        };

        let record = &trusted.record;
        let mismatch = [
            (
                "model SHA",
                !record
                    .model_sha256
                    .eq_ignore_ascii_case(expected.model_sha256),
            ),
            (
                "dataset version",
                expected
                    .dataset_version
                    .is_some_and(|value| value != record.dataset_version),
            ),
            (
                "runtime",
                expected.runtime.is_some_and(|value| {
                    canonical_runtime(value) != canonical_runtime(&record.runtime)
                }),
            ),
            (
                "provider",
                expected
                    .provider
                    .is_some_and(|value| !value.eq_ignore_ascii_case(&record.provider)),
            ),
        ]
        .into_iter()
        .find_map(|(name, differs)| differs.then_some(name));
        if let Some(name) = mismatch {
            return quality_failure(
                &expected,
                ModelQualityStatus::BaselineMissing,
                CoreErrorCode::ModelBaselineMissing,
                format!(
                    "trusted baseline '{}' does not match the current {name}",
                    trusted.path.display()
                ),
            );
        }

        let thresholds_pass = record.metrics.normalized_exact
            >= record.thresholds.minimum_normalized_exact
            && record.metrics.cer <= record.thresholds.maximum_cer
            && match (
                record.thresholds.maximum_hard_negative_fpr,
                record.metrics.hard_negative_fpr,
            ) {
                (Some(maximum), Some(actual)) => actual <= maximum,
                (Some(_), None) => false,
                (None, _) => true,
            };
        let status = if !thresholds_pass {
            ModelQualityStatus::BaselineFailed
        } else if record.contains_real && record.contains_hard_negatives {
            ModelQualityStatus::Validated
        } else {
            ModelQualityStatus::Experimental
        };
        ModelQualityReadiness {
            model_id: expected.model_id.to_owned(),
            model_version: expected.model_version.to_owned(),
            task: task_from_model_id(expected.model_id),
            status,
            dataset_version: Some(record.dataset_version.clone()),
            runtime: Some(record.runtime.clone()),
            provider: Some(record.provider.clone()),
            baseline_sha256: Some(trusted.baseline_sha256.clone()),
            code: (status == ModelQualityStatus::BaselineFailed)
                .then_some(CoreErrorCode::ModelBaselineFailed),
            message: (status == ModelQualityStatus::BaselineFailed).then_some(
                "trusted quality evidence does not meet the release thresholds".to_owned(),
            ),
        }
    }

    pub fn len(&self) -> usize {
        self.baselines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.baselines.is_empty()
    }
}

fn quality_failure(
    expected: &ModelQualityValidation<'_>,
    status: ModelQualityStatus,
    code: CoreErrorCode,
    message: String,
) -> ModelQualityReadiness {
    ModelQualityReadiness {
        model_id: expected.model_id.to_owned(),
        model_version: expected.model_version.to_owned(),
        task: task_from_model_id(expected.model_id),
        status,
        dataset_version: expected.dataset_version.map(str::to_owned),
        runtime: expected.runtime.map(str::to_owned),
        provider: expected.provider.map(str::to_owned),
        baseline_sha256: None,
        code: Some(code),
        message: Some(message),
    }
}

fn task_from_model_id(model_id: &str) -> String {
    model_id
        .split_once('/')
        .map_or_else(|| "unknown".to_owned(), |(task, _)| task.to_owned())
}

fn canonical_runtime(runtime: &str) -> String {
    runtime
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_unknown_identity(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "unknown" | "unavailable" | "unverified" | "auto"
    )
}

fn collect_json_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).map_err(|error| {
        SnipperError::Model(format!(
            "failed to enumerate quality baseline root '{}': {error}",
            root.display()
        ))
    })? {
        let path = entry
            .map_err(|error| SnipperError::Model(error.to_string()))?
            .path();
        if path.is_dir() {
            collect_json_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && path.file_name().is_some_and(|name| name != "index.json")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str, path: &Path) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(SnipperError::Model(format!(
            "{field} in '{}' is not a SHA-256 hex digest",
            path.display()
        )))
    }
}

fn validate_commit(value: &str, path: &Path) -> Result<()> {
    if (7..=40).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(SnipperError::Model(format!(
            "generatedByCommit in '{}' is not a Git commit id",
            path.display()
        )))
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn record() -> ModelQualityRecord {
        ModelQualityRecord {
            schema_version: 1,
            model_id: "formula-recognition/demo".to_owned(),
            model_version: "1.0.0".to_owned(),
            model_sha256: "a".repeat(64),
            dataset_version: "synthetic-v1".to_owned(),
            runtime: "onnxruntime".to_owned(),
            provider: "cpu".to_owned(),
            generated_by_commit: "b2267a0".to_owned(),
            evidence_sha256: "b".repeat(64),
            contains_synthetic: true,
            contains_real: false,
            contains_hard_negatives: false,
            metrics: ModelQualityMetrics {
                normalized_exact: 0.8,
                cer: 0.1,
                ter: 0.1,
                hard_negative_fpr: None,
            },
            thresholds: ModelQualityThresholds {
                minimum_normalized_exact: 0.7,
                maximum_cer: 0.2,
                maximum_hard_negative_fpr: None,
            },
        }
    }

    fn load_record(record: &ModelQualityRecord) -> (TempDir, ModelQualityRegistry) {
        let root = TempDir::new().unwrap();
        let bytes = serde_json::to_vec_pretty(record).unwrap();
        fs::write(root.path().join("baseline.json"), &bytes).unwrap();
        fs::write(
            root.path().join("index.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "files": {
                    "baseline.json": sha256(&bytes)
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let registry = ModelQualityRegistry::load(root.path()).unwrap();
        (root, registry)
    }

    fn validation<'a>() -> ModelQualityValidation<'a> {
        ModelQualityValidation {
            model_id: "formula-recognition/demo",
            model_version: "1.0.0",
            model_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            dataset_version: Some("synthetic-v1"),
            runtime: Some("onnxruntime"),
            provider: Some("cpu"),
        }
    }

    #[test]
    fn passing_synthetic_evidence_is_only_experimental() {
        let (_root, registry) = load_record(&record());
        assert_eq!(
            registry.validate(validation()).status,
            ModelQualityStatus::Experimental
        );
    }

    #[test]
    fn real_and_hard_negative_evidence_can_be_validated() {
        let mut record = record();
        record.contains_real = true;
        record.contains_hard_negatives = true;
        record.metrics.hard_negative_fpr = Some(0.01);
        record.thresholds.maximum_hard_negative_fpr = Some(0.02);
        let (_root, registry) = load_record(&record);
        assert_eq!(
            registry.validate(validation()).status,
            ModelQualityStatus::Validated
        );
    }

    #[test]
    fn threshold_failure_and_identity_mismatches_fail_closed() {
        let mut record = record();
        record.metrics.cer = 1.3;
        let (_root, registry) = load_record(&record);
        assert_eq!(
            registry.validate(validation()).status,
            ModelQualityStatus::BaselineFailed
        );

        let mut wrong = validation();
        wrong.model_sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        assert_eq!(
            registry.validate(wrong).status,
            ModelQualityStatus::BaselineMissing
        );
    }

    #[test]
    fn missing_or_unknown_provider_fails_closed() {
        let (_root, registry) = load_record(&record());
        let mut missing = validation();
        missing.provider = None;
        let result = registry.validate(missing);
        assert_eq!(result.status, ModelQualityStatus::BaselineMissing);
        assert!(result.message.unwrap().contains("effective provider"));

        let mut unknown = validation();
        unknown.provider = Some("unknown");
        assert_eq!(
            registry.validate(unknown).status,
            ModelQualityStatus::BaselineMissing
        );
    }

    #[test]
    fn baseline_file_hash_mismatch_is_rejected() {
        let (root, _registry) = load_record(&record());
        fs::write(root.path().join("baseline.json"), b"{}").unwrap();
        let error = ModelQualityRegistry::load(root.path()).unwrap_err();
        assert!(error.to_string().contains("baseline hash mismatch"));
    }
}

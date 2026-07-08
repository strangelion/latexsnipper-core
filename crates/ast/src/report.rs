use serde::{Deserialize, Serialize};

use crate::Diagnostic;

// ---------------------------------------------------------------------------
// JobRoot — standard job directory layout
// ---------------------------------------------------------------------------

/// Standard directory layout for a processing job.
/// All CLI, Tauri, Office, Server, and test pipelines can use this layout.
///
/// TODO(phase3): integrate with `crate::engine::Job` — replace `result: Option<String>`
///   with artifact/report tracking using these types.
/// TODO(phase3): add schema version migration tests for old Document JSON → new format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRoot {
    pub job_id: String,
    pub root_dir: String,
    pub source_dir: String,
    pub decoded_dir: String,
    pub regions_dir: String,
    pub ast_dir: String,
    pub converted_dir: String,
    pub exported_dir: String,
    pub artifacts_dir: String,
    pub logs_dir: String,
    pub specs_dir: String,
    pub reports_dir: String,
}

impl JobRoot {
    /// Create a JobRoot rooted at `base/jobs/<job_id>/`.
    pub fn new(job_id: impl Into<String>, base: impl Into<String>) -> Self {
        let job_id = job_id.into();
        let root = format!("{}/jobs/{}", base.into(), job_id);
        Self {
            job_id,
            root_dir: root.clone(),
            source_dir: format!("{}/source", root),
            decoded_dir: format!("{}/decoded", root),
            regions_dir: format!("{}/regions", root),
            ast_dir: format!("{}/ast", root),
            converted_dir: format!("{}/converted", root),
            exported_dir: format!("{}/exported", root),
            artifacts_dir: format!("{}/artifacts", root),
            logs_dir: format!("{}/logs", root),
            specs_dir: format!("{}/specs", root),
            reports_dir: format!("{}/reports", root),
        }
    }

    /// Create all job directories on the filesystem.
    pub fn ensure_dirs(&self) -> std::result::Result<(), String> {
        let dirs = [
            &self.root_dir,
            &self.source_dir,
            &self.decoded_dir,
            &self.regions_dir,
            &self.ast_dir,
            &self.converted_dir,
            &self.exported_dir,
            &self.artifacts_dir,
            &self.logs_dir,
            &self.specs_dir,
            &self.reports_dir,
        ];
        for dir in dirs {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("Failed to create job directory '{}': {}", dir, e))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StageKind / StageSpec
// ---------------------------------------------------------------------------

/// The type of a processing stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageKind {
    Decode,
    Extract,
    Layout,
    Detect,
    Recognize,
    Normalize,
    Resolve,
    Convert,
    Export,
    Enhance,
    InsertOffice,
}

/// Input specification for a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageInput {
    /// Artifact IDs used as input.
    pub artifacts: Vec<String>,
    /// Optional source descriptor path.
    pub source: Option<String>,
}

/// Output specification for a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutput {
    /// Expected artifact kind.
    pub artifact_kind: String,
    /// Subdirectory name within the job directory.
    pub subdir: String,
}

/// Specification for a single stage in a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSpec {
    pub schema_version: String,
    pub job_id: String,
    pub stage_id: String,
    pub kind: StageKind,
    pub input: StageInput,
    pub output: StageOutput,
    /// Free-form stage-specific options.
    pub options: serde_json::Value,
    pub provider: Option<String>,
    pub credentials: Vec<CredentialRef>,
    pub retry: RetryPolicy,
}

// ---------------------------------------------------------------------------
// Credential handling
// ---------------------------------------------------------------------------

/// Reference to a credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    pub name: String,
    pub source: CredentialSource,
}

/// Where a credential is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialSource {
    Env { var: String },
    SystemKeychain { key: String },
    None,
}

// ---------------------------------------------------------------------------
// RetryPolicy
// ---------------------------------------------------------------------------

/// Retry policy for a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub backoff_factor: f32,
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_factor: 2.0,
            max_delay_ms: 30_000,
        }
    }
}

// ---------------------------------------------------------------------------
// ArtifactManifest
// ---------------------------------------------------------------------------

/// A manifest of all artifacts produced during a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub schema_version: String,
    pub job_id: String,
    pub artifacts: Vec<ArtifactEntry>,
}

/// A single artifact entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub id: String,
    pub kind: ArtifactKind,
    pub path: String,
    pub mime_type: Option<String>,
    pub format: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub producer_stage_id: Option<String>,
    #[serde(default)]
    pub source_artifact_ids: Vec<String>,
}

/// The kind/type of an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    Source,
    DecodedPageImage,
    NativeAsset,
    RegionGraph,
    ModelRawOutput,
    DocumentAst,
    DocumentAssets,
    ConvertedText,
    ExportedFile,
    ClipboardBundle,
    Report,
    Log,
}

// ---------------------------------------------------------------------------
// EventRecord — structured event log entry
// ---------------------------------------------------------------------------

/// A structured log entry for observability and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: String,
    pub level: String,
    pub job_id: Option<String>,
    pub stage_id: Option<String>,
    pub event: String,
    pub code: Option<String>,
    pub message: String,
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// StageReport
// ---------------------------------------------------------------------------

/// Status of a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

/// Report for a single stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub stage_id: String,
    pub kind: StageKind,
    pub status: StageStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub input_artifacts: Vec<String>,
    #[serde(default)]
    pub output_artifacts: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// DocumentReport
// ---------------------------------------------------------------------------

/// Summary of the input document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSummary {
    pub format: Option<String>,
    pub filename: Option<String>,
    pub page_count: usize,
    pub file_size_bytes: Option<u64>,
}

/// Summary of blocks in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSummary {
    pub total: usize,
    pub paragraphs: usize,
    pub formulas: usize,
    pub tables: usize,
    pub figures: usize,
    pub other: usize,
}

/// Summary of assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSummary {
    pub total: usize,
    pub embedded: usize,
    pub referenced: usize,
    pub total_size_bytes: Option<u64>,
}

/// Confidence summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceSummary {
    pub mean: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

/// An unsupported feature encountered during processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedFeature {
    pub feature: String,
    pub context: Option<String>,
    pub diagnostic_code: Option<String>,
}

/// Overall report for a processed document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentReport {
    pub schema_version: String,
    pub document_id: Option<String>,
    pub input_summary: InputSummary,
    pub page_count: usize,
    pub block_summary: BlockSummary,
    pub asset_summary: AssetSummary,
    pub confidence_summary: ConfidenceSummary,
    #[serde(default)]
    pub unsupported_features: Vec<UnsupportedFeature>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub timings_ms: std::collections::HashMap<String, u64>,
}

impl DocumentReport {
    /// Generate a report from a Document AST by walking all blocks and assets.
    ///
    /// Computes:
    /// - `page_count` from Document.pages.len()
    /// - `block_summary` by counting each Block variant
    /// - `asset_summary` by counting assets by storage/format
    /// - `confidence_summary` from formulas, tables, and detected regions
    /// - `diagnostics` copied directly from Document.diagnostics
    pub fn from_document(doc: &crate::Document) -> Self {
        let page_count = doc.pages.len();

        // Block summary
        let mut total_blocks = 0usize;
        let mut paragraphs = 0usize;
        let mut formulas = 0usize;
        let mut tables = 0usize;
        let mut figures = 0usize;
        let mut other = 0usize;
        for block in doc.all_blocks() {
            total_blocks += 1;
            match block {
                crate::Block::Paragraph(_) => paragraphs += 1,
                crate::Block::Formula(_) => formulas += 1,
                crate::Block::Table(_) => tables += 1,
                crate::Block::Figure(_) => figures += 1,
                _ => other += 1,
            }
        }

        // Confidence summary
        let mut confidences: Vec<f32> = Vec::new();
        for block in doc.all_blocks() {
            if let crate::Block::Formula(f) = block {
                confidences.push(f.formula.confidence);
            }
            if let crate::Block::Table(t) = block {
                for row in &t.rows {
                    for cell in row {
                        for inline in &cell.inlines {
                            if let crate::Inline::Formula(f) = inline {
                                confidences.push(f.confidence);
                            }
                        }
                    }
                }
            }
        }
        let confidence_summary = if confidences.is_empty() {
            ConfidenceSummary {
                mean: None,
                min: None,
                max: None,
            }
        } else {
            let sum: f32 = confidences.iter().sum();
            let min = confidences.iter().cloned().fold(f32::MAX, f32::min);
            let max = confidences.iter().cloned().fold(f32::MIN, f32::max);
            ConfidenceSummary {
                mean: Some(sum / confidences.len() as f32),
                min: Some(min),
                max: Some(max),
            }
        };

        // Asset summary
        let total_assets = doc.assets.len();
        let embedded = doc
            .assets
            .iter()
            .filter(|a| matches!(a.storage, crate::AssetStorage::InlineBase64 { .. }))
            .count();
        let referenced = total_assets.saturating_sub(embedded);

        Self {
            schema_version: doc.schema_version.clone(),
            document_id: doc.metadata.language.clone(),
            input_summary: InputSummary {
                format: doc.metadata.ocr_model.clone(),
                filename: None,
                page_count,
                file_size_bytes: None,
            },
            page_count,
            block_summary: BlockSummary {
                total: total_blocks,
                paragraphs,
                formulas,
                tables,
                figures,
                other,
            },
            asset_summary: AssetSummary {
                total: total_assets,
                embedded,
                referenced,
                total_size_bytes: None,
            },
            confidence_summary,
            unsupported_features: Vec::new(),
            diagnostics: doc.diagnostics.clone(),
            timings_ms: std::collections::HashMap::new(),
        }
    }

    /// Attach stage reports to this document report.
    pub fn with_stage_reports(mut self, stages: &[StageReport]) -> Self {
        for stage in stages {
            if stage.status == StageStatus::Failed {
                self.diagnostics.extend(stage.diagnostics.clone());
            }
        }
        self
    }

    /// Attach provider reports to this document report.
    pub fn with_provider_reports(mut self, providers: &[ProviderReport]) -> Self {
        for provider in providers {
            self.timings_ms.insert(
                format!("provider.{}", provider.provider_id),
                provider.total_elapsed_ms,
            );
        }
        self
    }
}

// ---------------------------------------------------------------------------
// ProviderReport
// ---------------------------------------------------------------------------

/// A single API call record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCallReport {
    pub call_id: String,
    pub model: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub elapsed_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Report for a provider (local or remote).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReport {
    pub provider_id: String,
    /// "LocalOnnx", "RemoteApi", "RemoteVlm", etc.
    pub provider_kind: String,
    pub model: Option<String>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub calls: Vec<ProviderCallReport>,
    pub fallback_used: bool,
    pub total_elapsed_ms: u64,
}

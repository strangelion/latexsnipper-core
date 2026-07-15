use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CORPUS_SCHEMA_VERSION: u32 = 1;
pub const PREDICTION_SCHEMA_VERSION: u32 = 1;
pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;
pub const GATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusTask {
    PrintedFormula,
    HandwrittenFormula,
    LatinText,
    SimplifiedChineseText,
    MixedCjkLatinText,
    MixedFormulaText,
    DocumentLayout,
    TableStructure,
    Orientation,
}

impl CorpusTask {
    pub const ALL: [Self; 9] = [
        Self::PrintedFormula,
        Self::HandwrittenFormula,
        Self::LatinText,
        Self::SimplifiedChineseText,
        Self::MixedCjkLatinText,
        Self::MixedFormulaText,
        Self::DocumentLayout,
        Self::TableStructure,
        Self::Orientation,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionPolicy {
    Allowed,
    OptInOnly,
    Prohibited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusSource {
    pub name: String,
    pub uri: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusLicense {
    pub spdx: String,
    pub attribution: String,
    pub redistribution: RedistributionPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub class: String,
    pub bbox: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCellValue {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBlockValue {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Annotation {
    Text { text: String },
    Formula { latex: String },
    Layout { regions: Vec<Region> },
    Table { cells: Vec<TableCellValue> },
    Orientation { degrees: u16 },
    Document { blocks: Vec<DocumentBlockValue> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusSample {
    pub id: String,
    pub asset: String,
    pub sha256: String,
    pub annotation: Annotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusManifest {
    pub schema_version: u32,
    pub id: String,
    pub description: String,
    pub task: CorpusTask,
    pub source: CorpusSource,
    pub license: CorpusLicense,
    pub annotation_format: String,
    pub preprocessing_assumptions: Vec<String>,
    pub content_sha256: String,
    pub samples: Vec<CorpusSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusIndexEntry {
    pub manifest: String,
    pub tiers: Vec<ValidationTier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusIndex {
    pub schema_version: u32,
    pub corpora: Vec<CorpusIndexEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationTier {
    PullRequest,
    Scheduled,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelIdentity {
    pub id: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionIdentity {
    pub runtime: String,
    pub provider: String,
    pub platform: String,
    pub preprocessing_version: String,
    pub postprocessing_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplePrediction {
    pub sample_id: String,
    pub prediction: Annotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionSet {
    pub schema_version: u32,
    pub corpus_id: String,
    pub corpus_sha256: String,
    pub model: ModelIdentity,
    pub execution: ExecutionIdentity,
    pub predictions: Vec<SamplePrediction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionBundle {
    pub schema_version: u32,
    pub tier: ValidationTier,
    pub runs: Vec<PredictionSet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricValue {
    pub value: f64,
    pub unit: String,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdDirection {
    Minimum,
    Maximum,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricThreshold {
    pub direction: ThresholdDirection,
    pub value: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateConfig {
    pub schema_version: u32,
    pub tier: ValidationTier,
    pub thresholds: BTreeMap<CorpusTask, BTreeMap<String, MetricThreshold>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub metric: String,
    pub actual: f64,
    pub threshold: f64,
    pub direction: ThresholdDirection,
    pub passed: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusEvidence {
    pub corpus_id: String,
    pub corpus_sha256: String,
    pub task: CorpusTask,
    pub model: ModelIdentity,
    pub execution: ExecutionIdentity,
    pub metrics: BTreeMap<String, MetricValue>,
    pub gates: Vec<GateResult>,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReport {
    pub schema_version: u32,
    pub metric_schema_version: u32,
    pub tier: ValidationTier,
    pub source_commit: String,
    pub generated_at_utc: String,
    pub corpora: Vec<CorpusEvidence>,
    pub passed: bool,
}

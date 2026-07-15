use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::schema::{
    Annotation, CorpusIndex, CorpusLicense, CorpusManifest, CorpusTask, EvidenceReport, GateConfig,
    PredictionBundle, RedistributionPolicy, ValidationTier, CORPUS_SCHEMA_VERSION,
    EVIDENCE_SCHEMA_VERSION, GATE_SCHEMA_VERSION, PREDICTION_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("failed to read '{path}': {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse JSON '{path}': {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid evaluation data: {0}")]
    Invalid(String),
}

#[derive(Debug)]
pub struct LoadedCorpus {
    pub manifest_path: PathBuf,
    pub manifest: CorpusManifest,
}

#[derive(Debug)]
pub struct LoadedCorpusIndex {
    pub index: CorpusIndex,
    pub corpora: Vec<LoadedCorpus>,
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, ValidationError> {
    let bytes = fs::read(path).map_err(|source| ValidationError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ValidationError::Json {
        path: path.to_path_buf(),
        source,
    })
}

pub fn load_and_validate_index(path: &Path) -> Result<LoadedCorpusIndex, ValidationError> {
    let index: CorpusIndex = read_json(path)?;
    if index.schema_version != CORPUS_SCHEMA_VERSION {
        return invalid(format!(
            "corpus index schemaVersion must be {CORPUS_SCHEMA_VERSION}, got {}",
            index.schema_version
        ));
    }
    if index.corpora.is_empty() {
        return invalid("corpus index must contain at least one corpus");
    }

    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let mut corpora = Vec::with_capacity(index.corpora.len());
    let mut seen_manifests = BTreeSet::new();
    let mut seen_tasks = BTreeSet::new();
    for entry in &index.corpora {
        validate_relative_path(&entry.manifest, "manifest")?;
        if !seen_manifests.insert(entry.manifest.as_str()) {
            return invalid(format!("duplicate corpus manifest '{}'", entry.manifest));
        }
        if entry.tiers.is_empty() {
            return invalid(format!(
                "corpus '{}' has no validation tiers",
                entry.manifest
            ));
        }
        let unique_tiers: BTreeSet<_> = entry.tiers.iter().copied().collect();
        if unique_tiers.len() != entry.tiers.len() {
            return invalid(format!(
                "corpus '{}' contains duplicate validation tiers",
                entry.manifest
            ));
        }

        let manifest_path = resolve_confined_file(root, &entry.manifest, "manifest")?;
        let manifest: CorpusManifest = read_json(&manifest_path)?;
        validate_manifest(&manifest_path, &manifest)?;
        if !seen_tasks.insert(manifest.task) {
            return invalid(format!(
                "corpus index contains more than one corpus for task {:?}",
                manifest.task
            ));
        }
        corpora.push(LoadedCorpus {
            manifest_path,
            manifest,
        });
    }

    let missing: Vec<_> = CorpusTask::ALL
        .into_iter()
        .filter(|task| !seen_tasks.contains(task))
        .collect();
    if !missing.is_empty() {
        return invalid(format!(
            "corpus index is missing required tasks: {missing:?}"
        ));
    }

    Ok(LoadedCorpusIndex { index, corpora })
}

pub fn compute_manifest_digest(
    manifest_path: &Path,
    manifest: &CorpusManifest,
) -> Result<String, ValidationError> {
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut hasher = Sha256::new();
    for sample in &manifest.samples {
        let asset = resolve_confined_file(root, &sample.asset, "sample asset")?;
        let bytes = fs::read(&asset).map_err(|source| ValidationError::Read {
            path: asset,
            source,
        })?;
        hash_field(&mut hasher, sample.id.as_bytes());
        hash_field(&mut hasher, sample.asset.as_bytes());
        hash_field(&mut hasher, &bytes);
        let annotation =
            serde_json::to_vec(&sample.annotation).map_err(|source| ValidationError::Json {
                path: manifest_path.to_path_buf(),
                source,
            })?;
        hash_field(&mut hasher, &annotation);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn validate_prediction_bundle(
    loaded: &LoadedCorpusIndex,
    bundle: &PredictionBundle,
    tier: ValidationTier,
) -> Result<(), ValidationError> {
    if bundle.schema_version != PREDICTION_SCHEMA_VERSION {
        return invalid(format!(
            "prediction schemaVersion must be {PREDICTION_SCHEMA_VERSION}, got {}",
            bundle.schema_version
        ));
    }
    if bundle.tier != tier {
        return invalid(format!(
            "prediction tier {:?} does not match requested tier {tier:?}",
            bundle.tier
        ));
    }

    let selected = selected_corpora(loaded, tier);
    let selected_ids: BTreeSet<_> = selected
        .iter()
        .map(|corpus| corpus.manifest.id.as_str())
        .collect();
    let mut run_ids = BTreeSet::new();
    for run in &bundle.runs {
        if run.schema_version != PREDICTION_SCHEMA_VERSION {
            return invalid(format!(
                "prediction run schemaVersion must be {PREDICTION_SCHEMA_VERSION}, got {}",
                run.schema_version
            ));
        }
        validate_nonempty(&run.corpus_id, "prediction corpusId")?;
        validate_sha256(&run.corpus_sha256, "prediction corpusSha256")?;
        validate_nonempty(&run.model.id, "model id")?;
        validate_sha256(&run.model.sha256, "model sha256")?;
        validate_execution(run.execution.runtime.as_str(), "runtime")?;
        validate_execution(run.execution.provider.as_str(), "provider")?;
        validate_execution(run.execution.platform.as_str(), "platform")?;
        validate_execution(
            run.execution.preprocessing_version.as_str(),
            "preprocessingVersion",
        )?;
        validate_execution(
            run.execution.postprocessing_version.as_str(),
            "postprocessingVersion",
        )?;
        if !run_ids.insert(run.corpus_id.as_str()) {
            return invalid(format!("duplicate prediction run for '{}'", run.corpus_id));
        }
        if !selected_ids.contains(run.corpus_id.as_str()) {
            return invalid(format!(
                "prediction run '{}' is not selected for tier {tier:?}",
                run.corpus_id
            ));
        }

        let corpus = selected
            .iter()
            .find(|corpus| corpus.manifest.id == run.corpus_id)
            .expect("selected corpus ID was checked");
        if run.corpus_sha256 != corpus.manifest.content_sha256 {
            return invalid(format!(
                "prediction corpus digest for '{}' does not match the manifest",
                run.corpus_id
            ));
        }
        let expected_ids: BTreeSet<_> = corpus
            .manifest
            .samples
            .iter()
            .map(|sample| sample.id.as_str())
            .collect();
        let actual_ids: BTreeSet<_> = run
            .predictions
            .iter()
            .map(|prediction| prediction.sample_id.as_str())
            .collect();
        if actual_ids.len() != run.predictions.len() {
            return invalid(format!(
                "prediction run '{}' contains duplicate sample IDs",
                run.corpus_id
            ));
        }
        if expected_ids != actual_ids {
            return invalid(format!(
                "prediction run '{}' must contain exactly the corpus sample IDs",
                run.corpus_id
            ));
        }
    }
    if run_ids != selected_ids {
        return invalid(format!(
            "predictions must contain exactly one run for every corpus in tier {tier:?}"
        ));
    }
    Ok(())
}

pub fn validate_gate_config(
    loaded: &LoadedCorpusIndex,
    gates: &GateConfig,
    tier: ValidationTier,
) -> Result<(), ValidationError> {
    if gates.schema_version != GATE_SCHEMA_VERSION {
        return invalid(format!(
            "gate schemaVersion must be {GATE_SCHEMA_VERSION}, got {}",
            gates.schema_version
        ));
    }
    if gates.tier != tier {
        return invalid(format!(
            "gate tier {:?} does not match requested tier {tier:?}",
            gates.tier
        ));
    }
    let metric_contract = expected_metrics();
    for corpus in selected_corpora(loaded, tier) {
        let thresholds = gates.thresholds.get(&corpus.manifest.task).ok_or_else(|| {
            ValidationError::Invalid(format!(
                "missing thresholds for task {:?}",
                corpus.manifest.task
            ))
        })?;
        let threshold_names: BTreeSet<_> = thresholds.keys().map(String::as_str).collect();
        let expected_names = metric_contract
            .get(&corpus.manifest.task)
            .expect("every corpus task has a metric contract");
        if &threshold_names != expected_names {
            return invalid(format!(
                "task {:?} thresholds must exactly match its metric contract",
                corpus.manifest.task
            ));
        }
        for (metric, threshold) in thresholds {
            validate_nonempty(metric, "metric name")?;
            if !threshold.value.is_finite() || threshold.value < 0.0 {
                return invalid(format!(
                    "threshold '{metric}' for task {:?} must be finite and non-negative",
                    corpus.manifest.task
                ));
            }
            validate_nonempty(&threshold.rationale, "threshold rationale")?;
        }
    }
    Ok(())
}

pub fn validate_evidence(report: &EvidenceReport) -> Result<(), ValidationError> {
    if report.schema_version != EVIDENCE_SCHEMA_VERSION {
        return invalid(format!(
            "evidence schemaVersion must be {EVIDENCE_SCHEMA_VERSION}, got {}",
            report.schema_version
        ));
    }
    if report.metric_schema_version != 1 {
        return invalid(format!(
            "metricSchemaVersion must be 1, got {}",
            report.metric_schema_version
        ));
    }
    validate_nonempty(&report.source_commit, "sourceCommit")?;
    validate_nonempty(&report.generated_at_utc, "generatedAtUtc")?;
    if !matches!(report.source_commit.len(), 40 | 64)
        || !report
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid("sourceCommit must be a lowercase 40- or 64-character object ID");
    }
    if report.generated_at_utc.len() < 20
        || !report.generated_at_utc.ends_with('Z')
        || report.generated_at_utc.as_bytes().get(4) != Some(&b'-')
        || report.generated_at_utc.as_bytes().get(7) != Some(&b'-')
        || report.generated_at_utc.as_bytes().get(10) != Some(&b'T')
        || report.generated_at_utc.as_bytes().get(13) != Some(&b':')
        || report.generated_at_utc.as_bytes().get(16) != Some(&b':')
    {
        return invalid("generatedAtUtc must be an RFC 3339 UTC timestamp ending in Z");
    }
    if report.corpora.is_empty() {
        return invalid("evidence report must contain at least one corpus");
    }
    let mut corpus_ids = BTreeSet::new();
    for corpus in &report.corpora {
        if !corpus_ids.insert(corpus.corpus_id.as_str()) {
            return invalid(format!("duplicate evidence corpus '{}'", corpus.corpus_id));
        }
        validate_nonempty(&corpus.corpus_id, "evidence corpusId")?;
        validate_sha256(&corpus.corpus_sha256, "evidence corpusSha256")?;
        validate_nonempty(&corpus.model.id, "evidence model id")?;
        validate_sha256(&corpus.model.sha256, "evidence model sha256")?;
        validate_execution(&corpus.execution.runtime, "evidence runtime")?;
        validate_execution(&corpus.execution.provider, "evidence provider")?;
        validate_execution(&corpus.execution.platform, "evidence platform")?;
        validate_execution(
            &corpus.execution.preprocessing_version,
            "evidence preprocessingVersion",
        )?;
        validate_execution(
            &corpus.execution.postprocessing_version,
            "evidence postprocessingVersion",
        )?;
        if corpus.metrics.is_empty() {
            return invalid(format!("corpus '{}' has no metrics", corpus.corpus_id));
        }
        let metric_contract = expected_metrics();
        let metric_names: BTreeSet<_> = corpus.metrics.keys().map(String::as_str).collect();
        if metric_contract.get(&corpus.task) != Some(&metric_names) {
            return invalid(format!(
                "corpus '{}' metrics do not match task {:?}",
                corpus.corpus_id, corpus.task
            ));
        }
        for (name, metric) in &corpus.metrics {
            validate_nonempty(name, "metric name")?;
            let valid_value = match metric.unit.as_str() {
                "score" => metric.value.is_finite() && (0.0..=1.0).contains(&metric.value),
                "ratio" => metric.value.is_finite() && metric.value >= 0.0,
                _ => false,
            };
            if !valid_value {
                return invalid(format!(
                    "metric '{name}' for corpus '{}' has an invalid value or unit",
                    corpus.corpus_id
                ));
            }
            if metric.sample_count == 0 {
                return invalid(format!(
                    "metric '{name}' for corpus '{}' has no samples",
                    corpus.corpus_id
                ));
            }
        }
        let mut gated_metrics = BTreeSet::new();
        for gate in &corpus.gates {
            if !gated_metrics.insert(gate.metric.as_str()) {
                return invalid(format!(
                    "corpus '{}' contains a duplicate gate for '{}'",
                    corpus.corpus_id, gate.metric
                ));
            }
            let metric = corpus.metrics.get(&gate.metric).ok_or_else(|| {
                ValidationError::Invalid(format!(
                    "corpus '{}' gate references unknown metric '{}'",
                    corpus.corpus_id, gate.metric
                ))
            })?;
            if gate.actual != metric.value
                || !gate.threshold.is_finite()
                || gate.threshold < 0.0
                || gate.rationale.trim().is_empty()
            {
                return invalid(format!(
                    "corpus '{}' gate '{}' contains inconsistent values",
                    corpus.corpus_id, gate.metric
                ));
            }
            let expected_pass = match gate.direction {
                crate::schema::ThresholdDirection::Minimum => gate.actual >= gate.threshold,
                crate::schema::ThresholdDirection::Maximum => gate.actual <= gate.threshold,
            };
            if gate.passed != expected_pass {
                return invalid(format!(
                    "corpus '{}' gate '{}' pass state is incorrect",
                    corpus.corpus_id, gate.metric
                ));
            }
        }
        if gated_metrics != metric_names {
            return invalid(format!(
                "corpus '{}' gates must cover every emitted metric exactly once",
                corpus.corpus_id
            ));
        }
        if corpus.passed != corpus.gates.iter().all(|gate| gate.passed) {
            return invalid(format!(
                "corpus '{}' pass state does not match its gates",
                corpus.corpus_id
            ));
        }
    }
    if report.passed != report.corpora.iter().all(|corpus| corpus.passed) {
        return invalid("report pass state does not match corpus results");
    }
    Ok(())
}

pub fn selected_corpora(loaded: &LoadedCorpusIndex, tier: ValidationTier) -> Vec<&LoadedCorpus> {
    loaded
        .index
        .corpora
        .iter()
        .zip(&loaded.corpora)
        .filter_map(|(entry, corpus)| entry.tiers.contains(&tier).then_some(corpus))
        .collect()
}

fn validate_manifest(path: &Path, manifest: &CorpusManifest) -> Result<(), ValidationError> {
    if manifest.schema_version != CORPUS_SCHEMA_VERSION {
        return invalid(format!(
            "manifest '{}' schemaVersion must be {CORPUS_SCHEMA_VERSION}",
            path.display()
        ));
    }
    validate_nonempty(&manifest.id, "corpus id")?;
    validate_nonempty(&manifest.description, "corpus description")?;
    validate_nonempty(&manifest.source.name, "corpus source name")?;
    validate_nonempty(&manifest.source.uri, "corpus source URI")?;
    validate_nonempty(&manifest.source.revision, "corpus source revision")?;
    validate_license(&manifest.license)?;
    validate_nonempty(&manifest.annotation_format, "annotation format")?;
    if manifest.preprocessing_assumptions.is_empty()
        || manifest
            .preprocessing_assumptions
            .iter()
            .any(|value| value.trim().is_empty())
    {
        return invalid(format!(
            "corpus '{}' must document preprocessing assumptions",
            manifest.id
        ));
    }
    validate_sha256(&manifest.content_sha256, "corpus contentSha256")?;
    if manifest.samples.is_empty() {
        return invalid(format!("corpus '{}' must contain samples", manifest.id));
    }

    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let mut sample_ids = BTreeSet::new();
    for sample in &manifest.samples {
        validate_nonempty(&sample.id, "sample id")?;
        if !sample_ids.insert(sample.id.as_str()) {
            return invalid(format!(
                "corpus '{}' contains duplicate sample ID '{}'",
                manifest.id, sample.id
            ));
        }
        validate_relative_path(&sample.asset, "sample asset")?;
        validate_sha256(&sample.sha256, "sample sha256")?;
        validate_annotation(manifest.task, &sample.id, &sample.annotation)?;
        let asset = resolve_confined_file(root, &sample.asset, "sample asset")?;
        let bytes = fs::read(&asset).map_err(|source| ValidationError::Read {
            path: asset,
            source,
        })?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if digest != sample.sha256 {
            return invalid(format!(
                "sample '{}' asset digest mismatch: expected {}, got {digest}",
                sample.id, sample.sha256
            ));
        }
    }
    let digest = compute_manifest_digest(path, manifest)?;
    if digest != manifest.content_sha256 {
        return invalid(format!(
            "corpus '{}' content digest mismatch: expected {}, got {digest}",
            manifest.id, manifest.content_sha256
        ));
    }
    Ok(())
}

fn validate_annotation(
    task: CorpusTask,
    sample_id: &str,
    annotation: &Annotation,
) -> Result<(), ValidationError> {
    let matches = matches!(
        (task, annotation),
        (
            CorpusTask::PrintedFormula | CorpusTask::HandwrittenFormula,
            Annotation::Formula { .. }
        ) | (
            CorpusTask::LatinText
                | CorpusTask::SimplifiedChineseText
                | CorpusTask::MixedCjkLatinText,
            Annotation::Text { .. }
        ) | (CorpusTask::MixedFormulaText, Annotation::Document { .. })
            | (CorpusTask::DocumentLayout, Annotation::Layout { .. })
            | (CorpusTask::TableStructure, Annotation::Table { .. })
            | (CorpusTask::Orientation, Annotation::Orientation { .. })
    );
    if !matches {
        return invalid(format!(
            "sample '{sample_id}' annotation kind does not match task {task:?}"
        ));
    }
    match annotation {
        Annotation::Text { text } if text.is_empty() => {
            return invalid(format!("sample '{sample_id}' text must not be empty"));
        }
        Annotation::Formula { latex } if latex.is_empty() => {
            return invalid(format!("sample '{sample_id}' formula must not be empty"));
        }
        Annotation::Layout { regions } if regions.is_empty() => {
            return invalid(format!("sample '{sample_id}' layout must not be empty"));
        }
        Annotation::Table { cells } if cells.is_empty() => {
            return invalid(format!("sample '{sample_id}' table must not be empty"));
        }
        Annotation::Document { blocks } if blocks.is_empty() => {
            return invalid(format!("sample '{sample_id}' document must not be empty"));
        }
        _ => {}
    }
    if let Annotation::Orientation { degrees } = annotation {
        if !matches!(*degrees, 0 | 90 | 180 | 270) {
            return invalid(format!(
                "sample '{sample_id}' orientation must be 0, 90, 180, or 270"
            ));
        }
    }
    if let Annotation::Layout { regions } = annotation {
        for region in regions {
            if region.class.trim().is_empty()
                || region.bbox.iter().any(|value| !value.is_finite())
                || region.bbox[0] < 0.0
                || region.bbox[1] < 0.0
                || region.bbox[2] <= 0.0
                || region.bbox[3] <= 0.0
            {
                return invalid(format!("sample '{sample_id}' has an invalid layout region"));
            }
        }
    }
    if let Annotation::Table { cells } = annotation {
        let origins: BTreeSet<_> = cells.iter().map(|cell| (cell.row, cell.col)).collect();
        if cells
            .iter()
            .any(|cell| cell.rowspan == 0 || cell.colspan == 0)
            || origins.len() != cells.len()
        {
            return invalid(format!(
                "sample '{sample_id}' has an invalid or duplicate table cell"
            ));
        }
    }
    if let Annotation::Document { blocks } = annotation {
        let ids: BTreeSet<_> = blocks.iter().map(|block| block.id.as_str()).collect();
        if ids.len() != blocks.len()
            || blocks
                .iter()
                .any(|block| block.id.trim().is_empty() || block.kind.trim().is_empty())
        {
            return invalid(format!(
                "sample '{sample_id}' has invalid or duplicate document blocks"
            ));
        }
    }
    Ok(())
}

fn validate_license(license: &CorpusLicense) -> Result<(), ValidationError> {
    validate_nonempty(&license.spdx, "corpus SPDX license")?;
    validate_nonempty(&license.attribution, "corpus attribution")?;
    if license.redistribution == RedistributionPolicy::Prohibited {
        return invalid("tracked corpus assets cannot prohibit redistribution");
    }
    Ok(())
}

fn validate_relative_path(value: &str, label: &str) -> Result<(), ValidationError> {
    validate_nonempty(value, label)?;
    if value.contains('\\') || value.contains(':') {
        return invalid(format!(
            "{label} must use portable forward-slash paths: '{value}'"
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return invalid(format!("{label} must be a safe relative path: '{value}'"));
    }
    Ok(())
}

fn resolve_confined_file(
    root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, ValidationError> {
    validate_relative_path(relative, label)?;
    let canonical_root = fs::canonicalize(root).map_err(|source| ValidationError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    let candidate = root.join(relative);
    let canonical_candidate =
        fs::canonicalize(&candidate).map_err(|source| ValidationError::Read {
            path: candidate,
            source,
        })?;
    if !canonical_candidate.starts_with(&canonical_root) || !canonical_candidate.is_file() {
        return invalid(format!(
            "{label} must resolve to a regular file inside '{}': '{relative}'",
            canonical_root.display()
        ));
    }
    Ok(canonical_candidate)
}

fn validate_sha256(value: &str, label: &str) -> Result<(), ValidationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(format!("{label} must be a lowercase SHA-256 digest"));
    }
    Ok(())
}

fn validate_execution(value: &str, label: &str) -> Result<(), ValidationError> {
    validate_nonempty(value, label)
}

fn validate_nonempty(value: &str, label: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        invalid(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::Invalid(message.into()))
}

pub fn expected_metrics() -> BTreeMap<CorpusTask, BTreeSet<&'static str>> {
    BTreeMap::from([
        (
            CorpusTask::PrintedFormula,
            BTreeSet::from([
                "formula_normalized_exact_match",
                "formula_structural_similarity",
            ]),
        ),
        (
            CorpusTask::HandwrittenFormula,
            BTreeSet::from([
                "formula_normalized_exact_match",
                "formula_structural_similarity",
            ]),
        ),
        (CorpusTask::LatinText, BTreeSet::from(["cer", "wer"])),
        (
            CorpusTask::SimplifiedChineseText,
            BTreeSet::from(["cer", "wer"]),
        ),
        (
            CorpusTask::MixedCjkLatinText,
            BTreeSet::from(["cer", "wer"]),
        ),
        (
            CorpusTask::MixedFormulaText,
            BTreeSet::from([
                "document_block_semantics_f1",
                "document_reading_order_similarity",
            ]),
        ),
        (
            CorpusTask::DocumentLayout,
            BTreeSet::from(["layout_macro_f1"]),
        ),
        (
            CorpusTask::TableStructure,
            BTreeSet::from(["table_structure_f1", "table_tree_similarity"]),
        ),
        (
            CorpusTask::Orientation,
            BTreeSet::from(["orientation_accuracy"]),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_paths_and_uppercase_digests() {
        assert!(validate_relative_path("../secret", "asset").is_err());
        assert!(validate_relative_path("C:/secret", "asset").is_err());
        assert!(validate_relative_path("assets\\sample.png", "asset").is_err());
        assert!(validate_sha256(&"A".repeat(64), "digest").is_err());
        assert!(validate_sha256(&"a".repeat(64), "digest").is_ok());
    }

    #[test]
    fn exposes_metrics_for_every_required_task() {
        let metrics = expected_metrics();
        assert_eq!(metrics.len(), CorpusTask::ALL.len());
        assert!(CorpusTask::ALL
            .iter()
            .all(|task| metrics.contains_key(task)));
    }
}

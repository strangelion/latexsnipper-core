use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::schema::{CorpusLicense, CorpusSource};

pub const FORMULA_CORPUS_PLAN_SCHEMA_VERSION: u32 = 1;
pub const FORMULA_CORPUS_SCHEMA_VERSION: u32 = 1;
pub const FORMULA_PULL_REQUEST_RECORD_COUNT: usize = 360;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaCategory {
    BasicStructure,
    Matrix,
    Multiline,
    Cases,
    NestedCalculus,
    ProbabilityStatistics,
    Chemistry,
    CustomStyles,
    Drawing,
    Malformed,
    OfficeCrossReference,
}

impl FormulaCategory {
    pub const ALL: [Self; 11] = [
        Self::BasicStructure,
        Self::Matrix,
        Self::Multiline,
        Self::Cases,
        Self::NestedCalculus,
        Self::ProbabilityStatistics,
        Self::Chemistry,
        Self::CustomStyles,
        Self::Drawing,
        Self::Malformed,
        Self::OfficeCrossReference,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaWrapper {
    DisplayDollar,
    InlineParentheses,
    DisplayBrackets,
    Bare,
}

impl FormulaWrapper {
    pub const ALL: [Self; 4] = [
        Self::DisplayDollar,
        Self::InlineParentheses,
        Self::DisplayBrackets,
        Self::Bare,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaContext {
    FormulaOnly,
    ChineseProse,
    Heading,
    CodeBlock,
    List,
    Table,
    MixedMarkdown,
}

impl FormulaContext {
    pub const ALL: [Self; 7] = [
        Self::FormulaOnly,
        Self::ChineseProse,
        Self::Heading,
        Self::CodeBlock,
        Self::List,
        Self::Table,
        Self::MixedMarkdown,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaComplexity {
    L1,
    L2,
    L3,
    L4,
    L5,
}

impl FormulaComplexity {
    pub const ALL: [Self; 5] = [Self::L1, Self::L2, Self::L3, Self::L4, Self::L5];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedFormulaOutcome {
    Valid,
    RecoverableError,
    Reject,
}

impl ExpectedFormulaOutcome {
    pub const ALL: [Self; 3] = [Self::Valid, Self::RecoverableError, Self::Reject];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaOutputTarget {
    Latex,
    Mathml,
    Omml,
    Typst,
    Svg,
    Png,
    Docx,
    Pptx,
    Xlsx,
}

impl FormulaOutputTarget {
    pub const ALL: [Self; 9] = [
        Self::Latex,
        Self::Mathml,
        Self::Omml,
        Self::Typst,
        Self::Svg,
        Self::Png,
        Self::Docx,
        Self::Pptx,
        Self::Xlsx,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaInsertionMode {
    NativeFormula,
    Svg,
    Png,
    Ole,
    Clipboard,
    Batch,
    Inline,
    Display,
}

impl FormulaInsertionMode {
    pub const ALL: [Self; 8] = [
        Self::NativeFormula,
        Self::Svg,
        Self::Png,
        Self::Ole,
        Self::Clipboard,
        Self::Batch,
        Self::Inline,
        Self::Display,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaCorpusTier {
    Pilot,
    PullRequest,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryQuota {
    pub category: FormulaCategory,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WrapperMinimum {
    pub wrapper: FormulaWrapper,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextMinimum {
    pub context: FormulaContext,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComplexityMinimum {
    pub complexity: FormulaComplexity,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeMinimum {
    pub outcome: ExpectedFormulaOutcome,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputMinimum {
    pub target: FormulaOutputTarget,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertionMinimum {
    pub mode: FormulaInsertionMode,
    pub minimum_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCoveragePlan {
    pub wrapper_minimums: Vec<WrapperMinimum>,
    pub context_minimums: Vec<ContextMinimum>,
    pub complexity_minimums: Vec<ComplexityMinimum>,
    pub outcome_minimums: Vec<OutcomeMinimum>,
    pub output_minimums: Vec<OutputMinimum>,
    pub insertion_minimums: Vec<InsertionMinimum>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCorpusPlan {
    pub schema_version: u32,
    pub id: String,
    pub description: String,
    pub target_formula_count: usize,
    pub compound_document_count: usize,
    pub seed: u64,
    pub source: CorpusSource,
    pub license: CorpusLicense,
    pub category_quotas: Vec<CategoryQuota>,
    pub coverage: FormulaCoveragePlan,
    pub pilot_content_sha256: String,
    pub pull_request_content_sha256: String,
    pub full_content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCorpusRecord {
    pub id: String,
    pub category: FormulaCategory,
    pub source: String,
    pub normalized_source: Option<String>,
    pub wrapper: FormulaWrapper,
    pub context: FormulaContext,
    pub complexity: FormulaComplexity,
    pub expected_outcome: ExpectedFormulaOutcome,
    pub output_targets: Vec<FormulaOutputTarget>,
    pub insertion_modes: Vec<FormulaInsertionMode>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCorpus {
    pub schema_version: u32,
    pub plan_id: String,
    pub seed: u64,
    pub tier: FormulaCorpusTier,
    pub content_sha256: String,
    pub records: Vec<FormulaCorpusRecord>,
}

#[derive(Debug, Error)]
pub enum FormulaCorpusError {
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
    #[error("failed to serialize formula corpus data: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("invalid formula corpus data: {0}")]
    Invalid(String),
}

pub fn read_formula_plan(path: &Path) -> Result<FormulaCorpusPlan, FormulaCorpusError> {
    read_json(path)
}

pub fn read_formula_corpus(path: &Path) -> Result<FormulaCorpus, FormulaCorpusError> {
    read_json(path)
}

pub fn validate_formula_plan(plan: &FormulaCorpusPlan) -> Result<(), FormulaCorpusError> {
    if plan.schema_version != FORMULA_CORPUS_PLAN_SCHEMA_VERSION {
        return invalid(format!(
            "plan schemaVersion must be {FORMULA_CORPUS_PLAN_SCHEMA_VERSION}, got {}",
            plan.schema_version
        ));
    }
    validate_nonempty(&plan.id, "plan id")?;
    validate_nonempty(&plan.description, "plan description")?;
    validate_nonempty(&plan.source.name, "source name")?;
    validate_nonempty(&plan.source.uri, "source uri")?;
    validate_nonempty(&plan.source.revision, "source revision")?;
    validate_nonempty(&plan.license.spdx, "license SPDX identifier")?;
    validate_nonempty(&plan.license.attribution, "license attribution")?;
    validate_sha256(&plan.pilot_content_sha256, "pilotContentSha256")?;
    validate_sha256(
        &plan.pull_request_content_sha256,
        "pullRequestContentSha256",
    )?;
    validate_sha256(&plan.full_content_sha256, "fullContentSha256")?;
    if plan.target_formula_count == 0 {
        return invalid("targetFormulaCount must be greater than zero");
    }
    if plan.compound_document_count == 0 {
        return invalid("compoundDocumentCount must be greater than zero");
    }

    let category_counts = collect_complete_minimums(
        plan.category_quotas
            .iter()
            .map(|entry| (entry.category, entry.count)),
        &FormulaCategory::ALL,
        plan.target_formula_count,
        "category quota",
    )?;
    let quota_total: usize = category_counts.values().sum();
    if quota_total != plan.target_formula_count {
        return invalid(format!(
            "category quotas must sum to targetFormulaCount {} but sum to {quota_total}",
            plan.target_formula_count
        ));
    }

    collect_complete_minimums(
        plan.coverage
            .wrapper_minimums
            .iter()
            .map(|entry| (entry.wrapper, entry.minimum_count)),
        &FormulaWrapper::ALL,
        plan.target_formula_count,
        "wrapper minimum",
    )?;
    collect_complete_minimums(
        plan.coverage
            .context_minimums
            .iter()
            .map(|entry| (entry.context, entry.minimum_count)),
        &FormulaContext::ALL,
        plan.target_formula_count,
        "context minimum",
    )?;
    collect_complete_minimums(
        plan.coverage
            .complexity_minimums
            .iter()
            .map(|entry| (entry.complexity, entry.minimum_count)),
        &FormulaComplexity::ALL,
        plan.target_formula_count,
        "complexity minimum",
    )?;
    let outcome_counts = collect_complete_minimums(
        plan.coverage
            .outcome_minimums
            .iter()
            .map(|entry| (entry.outcome, entry.minimum_count)),
        &ExpectedFormulaOutcome::ALL,
        plan.target_formula_count,
        "outcome minimum",
    )?;
    if outcome_counts.values().sum::<usize>() > plan.target_formula_count {
        return invalid("outcome minimums exceed targetFormulaCount");
    }
    collect_complete_minimums(
        plan.coverage
            .output_minimums
            .iter()
            .map(|entry| (entry.target, entry.minimum_count)),
        &FormulaOutputTarget::ALL,
        plan.target_formula_count,
        "output minimum",
    )?;
    collect_complete_minimums(
        plan.coverage
            .insertion_minimums
            .iter()
            .map(|entry| (entry.mode, entry.minimum_count)),
        &FormulaInsertionMode::ALL,
        plan.target_formula_count,
        "insertion minimum",
    )?;
    Ok(())
}

pub fn compute_formula_records_digest(
    records: &[FormulaCorpusRecord],
) -> Result<String, FormulaCorpusError> {
    let bytes = serde_json::to_vec(records)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn validate_formula_corpus(
    plan: &FormulaCorpusPlan,
    corpus: &FormulaCorpus,
) -> Result<(), FormulaCorpusError> {
    validate_formula_plan(plan)?;
    if corpus.schema_version != FORMULA_CORPUS_SCHEMA_VERSION {
        return invalid(format!(
            "corpus schemaVersion must be {FORMULA_CORPUS_SCHEMA_VERSION}, got {}",
            corpus.schema_version
        ));
    }
    if corpus.plan_id != plan.id {
        return invalid(format!(
            "corpus planId '{}' does not match plan '{}'",
            corpus.plan_id, plan.id
        ));
    }
    if corpus.seed != plan.seed {
        return invalid(format!(
            "corpus seed {} does not match plan seed {}",
            corpus.seed, plan.seed
        ));
    }
    validate_sha256(&corpus.content_sha256, "contentSha256")?;
    let digest = compute_formula_records_digest(&corpus.records)?;
    if digest != corpus.content_sha256 {
        return invalid(format!(
            "corpus contentSha256 does not match records: expected {}, got {}",
            corpus.content_sha256, digest
        ));
    }
    match corpus.tier {
        FormulaCorpusTier::Pilot => {
            if !(FormulaCategory::ALL.len()..=100).contains(&corpus.records.len()) {
                return invalid("pilot corpus must contain 11 to 100 records");
            }
        }
        FormulaCorpusTier::PullRequest => {
            if !(300..=500).contains(&corpus.records.len()) {
                return invalid("pull-request corpus must contain 300 to 500 records");
            }
        }
        FormulaCorpusTier::Full => {
            if corpus.records.len() != plan.target_formula_count {
                return invalid(format!(
                    "full corpus must contain exactly {} records",
                    plan.target_formula_count
                ));
            }
        }
    }

    let mut ids = BTreeSet::new();
    let mut category_counts = BTreeMap::new();
    let mut wrapper_counts = BTreeMap::new();
    let mut context_counts = BTreeMap::new();
    let mut complexity_counts = BTreeMap::new();
    let mut outcome_counts = BTreeMap::new();
    let mut output_counts = BTreeMap::new();
    let mut insertion_counts = BTreeMap::new();

    for record in &corpus.records {
        validate_nonempty(&record.id, "record id")?;
        if !ids.insert(record.id.as_str()) {
            return invalid(format!("duplicate formula record id '{}'", record.id));
        }
        validate_nonempty(&record.source, "record source")?;
        if record.expected_outcome == ExpectedFormulaOutcome::Valid {
            let normalized = record.normalized_source.as_deref().unwrap_or_default();
            validate_nonempty(normalized, "valid record normalizedSource")?;
        }
        if record.category == FormulaCategory::Malformed
            && record.expected_outcome == ExpectedFormulaOutcome::Valid
        {
            return invalid(format!(
                "malformed record '{}' cannot expect a valid outcome",
                record.id
            ));
        }
        if record.category != FormulaCategory::Malformed
            && record.expected_outcome != ExpectedFormulaOutcome::Valid
        {
            return invalid(format!(
                "non-malformed record '{}' must expect a valid outcome",
                record.id
            ));
        }
        if record.output_targets.is_empty() {
            return invalid(format!(
                "record '{}' must declare at least one output target",
                record.id
            ));
        }
        validate_unique_values(&record.output_targets, "output target", &record.id)?;
        validate_unique_values(&record.insertion_modes, "insertion mode", &record.id)?;
        if record.tags.is_empty() {
            return invalid(format!(
                "record '{}' must declare at least one tag",
                record.id
            ));
        }
        let mut tags = BTreeSet::new();
        for tag in &record.tags {
            validate_nonempty(tag, "record tag")?;
            if !tags.insert(tag.as_str()) {
                return invalid(format!("record '{}' contains duplicate tags", record.id));
            }
        }

        increment(&mut category_counts, record.category);
        increment(&mut wrapper_counts, record.wrapper);
        increment(&mut context_counts, record.context);
        increment(&mut complexity_counts, record.complexity);
        increment(&mut outcome_counts, record.expected_outcome);
        for target in &record.output_targets {
            increment(&mut output_counts, *target);
        }
        for mode in &record.insertion_modes {
            increment(&mut insertion_counts, *mode);
        }
    }

    if corpus.tier == FormulaCorpusTier::Full {
        let expected: BTreeMap<_, _> = plan
            .category_quotas
            .iter()
            .map(|entry| (entry.category, entry.count))
            .collect();
        if category_counts != expected {
            return invalid("full corpus category counts do not match the frozen plan");
        }
        enforce_minimums(
            &wrapper_counts,
            plan.coverage
                .wrapper_minimums
                .iter()
                .map(|entry| (entry.wrapper, entry.minimum_count)),
            "wrapper",
        )?;
        enforce_minimums(
            &context_counts,
            plan.coverage
                .context_minimums
                .iter()
                .map(|entry| (entry.context, entry.minimum_count)),
            "context",
        )?;
        enforce_minimums(
            &complexity_counts,
            plan.coverage
                .complexity_minimums
                .iter()
                .map(|entry| (entry.complexity, entry.minimum_count)),
            "complexity",
        )?;
        enforce_minimums(
            &outcome_counts,
            plan.coverage
                .outcome_minimums
                .iter()
                .map(|entry| (entry.outcome, entry.minimum_count)),
            "outcome",
        )?;
        enforce_minimums(
            &output_counts,
            plan.coverage
                .output_minimums
                .iter()
                .map(|entry| (entry.target, entry.minimum_count)),
            "output target",
        )?;
        enforce_minimums(
            &insertion_counts,
            plan.coverage
                .insertion_minimums
                .iter()
                .map(|entry| (entry.mode, entry.minimum_count)),
            "insertion mode",
        )?;
    } else {
        require_coverage(&category_counts, &FormulaCategory::ALL, "category")?;
        require_coverage(&wrapper_counts, &FormulaWrapper::ALL, "wrapper")?;
        require_coverage(&context_counts, &FormulaContext::ALL, "context")?;
        require_coverage(&complexity_counts, &FormulaComplexity::ALL, "complexity")?;
        require_coverage(
            &outcome_counts,
            &ExpectedFormulaOutcome::ALL,
            "expected outcome",
        )?;
        require_coverage(&output_counts, &FormulaOutputTarget::ALL, "output target")?;
        require_coverage(
            &insertion_counts,
            &FormulaInsertionMode::ALL,
            "insertion mode",
        )?;
    }
    let expected_digest = match corpus.tier {
        FormulaCorpusTier::Pilot => &plan.pilot_content_sha256,
        FormulaCorpusTier::PullRequest => &plan.pull_request_content_sha256,
        FormulaCorpusTier::Full => &plan.full_content_sha256,
    };
    if &corpus.content_sha256 != expected_digest {
        return invalid(format!(
            "{:?} digest drifted: plan requires {}, corpus has {}",
            corpus.tier, expected_digest, corpus.content_sha256
        ));
    }
    Ok(())
}

pub fn generate_formula_pilot(
    plan: &FormulaCorpusPlan,
) -> Result<FormulaCorpus, FormulaCorpusError> {
    validate_formula_plan(plan)?;
    let contexts = FormulaContext::ALL;
    let context_offset = (plan.seed % contexts.len() as u64) as usize;
    let records = pilot_templates()
        .into_iter()
        .enumerate()
        .map(|(index, template)| {
            let context = contexts[(index + context_offset) % contexts.len()];
            let wrapped = apply_wrapper(template.body, template.wrapper);
            FormulaCorpusRecord {
                id: format!("pilot-{index:03}-{}", template.id_suffix),
                category: template.category,
                source: apply_context(&wrapped, context),
                normalized_source: (template.expected_outcome == ExpectedFormulaOutcome::Valid)
                    .then(|| template.body.to_string()),
                wrapper: template.wrapper,
                context,
                complexity: template.complexity,
                expected_outcome: template.expected_outcome,
                output_targets: template.output_targets,
                insertion_modes: template.insertion_modes,
                tags: template.tags.into_iter().map(str::to_string).collect(),
            }
        })
        .collect::<Vec<_>>();
    let content_sha256 = compute_formula_records_digest(&records)?;
    Ok(FormulaCorpus {
        schema_version: FORMULA_CORPUS_SCHEMA_VERSION,
        plan_id: plan.id.clone(),
        seed: plan.seed,
        tier: FormulaCorpusTier::Pilot,
        content_sha256,
        records,
    })
}

/// Generate the deterministic pull-request smoke corpus.
///
/// The 360 records cover every category, wrapper, context, complexity, output
/// target, insertion mode, and expected outcome without storing a large JSON
/// fixture in the repository.
pub fn generate_formula_pull_request(
    plan: &FormulaCorpusPlan,
) -> Result<FormulaCorpus, FormulaCorpusError> {
    validate_formula_plan(plan)?;
    let templates = pilot_templates();
    let context_offset = (plan.seed % FormulaContext::ALL.len() as u64) as usize;
    let wrapper_offset = (plan.seed % FormulaWrapper::ALL.len() as u64) as usize;
    let complexity_offset = (plan.seed % FormulaComplexity::ALL.len() as u64) as usize;
    let mut records = Vec::with_capacity(FORMULA_PULL_REQUEST_RECORD_COUNT);

    for index in 0..FORMULA_PULL_REQUEST_RECORD_COUNT {
        let template = &templates[index % templates.len()];
        let round = index / templates.len();
        let context = FormulaContext::ALL[(index + context_offset) % FormulaContext::ALL.len()];
        let wrapper = FormulaWrapper::ALL[(index + wrapper_offset) % FormulaWrapper::ALL.len()];
        let complexity =
            FormulaComplexity::ALL[(index + complexity_offset) % FormulaComplexity::ALL.len()];
        let body = if template.expected_outcome == ExpectedFormulaOutcome::Valid {
            format!(r"{}\qquad v_{{{round}}}={}", template.body, round + 1)
        } else {
            format!("{} % deterministic variant {round}", template.body)
        };
        let wrapped = apply_wrapper(&body, wrapper);
        let mut tags = template
            .tags
            .iter()
            .map(|tag| (*tag).to_string())
            .collect::<Vec<_>>();
        tags.push("pr-smoke".to_string());
        tags.push(format!("variant-{round:02}"));
        records.push(FormulaCorpusRecord {
            id: format!("pr-{index:03}-{}-v{round:02}", template.id_suffix),
            category: template.category,
            source: apply_context(&wrapped, context),
            normalized_source: (template.expected_outcome == ExpectedFormulaOutcome::Valid)
                .then_some(body),
            wrapper,
            context,
            complexity,
            expected_outcome: template.expected_outcome,
            output_targets: template.output_targets.clone(),
            insertion_modes: template.insertion_modes.clone(),
            tags,
        });
    }

    let content_sha256 = compute_formula_records_digest(&records)?;
    let corpus = FormulaCorpus {
        schema_version: FORMULA_CORPUS_SCHEMA_VERSION,
        plan_id: plan.id.clone(),
        seed: plan.seed,
        tier: FormulaCorpusTier::PullRequest,
        content_sha256,
        records,
    };
    validate_formula_corpus(plan, &corpus)?;
    Ok(corpus)
}

/// Generate the complete deterministic 10,000-record corpus described by the
/// frozen plan. Full generation is intended for nightly and release evidence,
/// not the pull-request critical path.
pub fn generate_formula_full(
    plan: &FormulaCorpusPlan,
) -> Result<FormulaCorpus, FormulaCorpusError> {
    validate_formula_plan(plan)?;
    let templates = pilot_templates();
    let mut records = Vec::with_capacity(plan.target_formula_count);
    let wrapper_offset = (plan.seed % FormulaWrapper::ALL.len() as u64) as usize;
    let complexity_offset = (plan.seed % FormulaComplexity::ALL.len() as u64) as usize;

    for quota in &plan.category_quotas {
        let category_templates = templates
            .iter()
            .filter(|template| template.category == quota.category)
            .collect::<Vec<_>>();
        if category_templates.is_empty() {
            return invalid(format!(
                "no deterministic template exists for category {:?}",
                quota.category
            ));
        }
        for category_index in 0..quota.count {
            let index = records.len();
            let template = if quota.category == FormulaCategory::Malformed {
                let template_index = usize::from(category_index >= quota.count / 2);
                category_templates[template_index.min(category_templates.len() - 1)]
            } else {
                category_templates[category_index % category_templates.len()]
            };
            let wrapper = FormulaWrapper::ALL[(index + wrapper_offset) % FormulaWrapper::ALL.len()];
            let context = full_context(plan.seed, index, plan.target_formula_count);
            let complexity =
                FormulaComplexity::ALL[(index + complexity_offset) % FormulaComplexity::ALL.len()];
            let body = if template.expected_outcome == ExpectedFormulaOutcome::Valid {
                format!(
                    r"{}\qquad c_{{{}}}={}",
                    template.body,
                    category_index,
                    category_index + 1
                )
            } else {
                format!(
                    "{} % full deterministic variant {category_index}",
                    template.body
                )
            };
            let wrapped = apply_wrapper(&body, wrapper);
            let mut tags = template
                .tags
                .iter()
                .map(|tag| (*tag).to_string())
                .collect::<Vec<_>>();
            tags.push("nightly-full".to_string());
            tags.push(format!("variant-{category_index:04}"));
            let valid = template.expected_outcome == ExpectedFormulaOutcome::Valid;
            records.push(FormulaCorpusRecord {
                id: format!(
                    "full-{index:05}-{}-{category_index:04}",
                    category_slug(quota.category)
                ),
                category: quota.category,
                source: apply_context(&wrapped, context),
                normalized_source: valid.then_some(body),
                wrapper,
                context,
                complexity,
                expected_outcome: template.expected_outcome,
                output_targets: if valid {
                    FormulaOutputTarget::ALL.to_vec()
                } else {
                    vec![FormulaOutputTarget::Latex]
                },
                insertion_modes: if valid {
                    FormulaInsertionMode::ALL.to_vec()
                } else {
                    Vec::new()
                },
                tags,
            });
        }
    }

    let content_sha256 = compute_formula_records_digest(&records)?;
    let corpus = FormulaCorpus {
        schema_version: FORMULA_CORPUS_SCHEMA_VERSION,
        plan_id: plan.id.clone(),
        seed: plan.seed,
        tier: FormulaCorpusTier::Full,
        content_sha256,
        records,
    };
    validate_formula_corpus(plan, &corpus)?;
    Ok(corpus)
}

fn full_context(seed: u64, index: usize, target_count: usize) -> FormulaContext {
    let permuted = ((index as u64 * 7_919 + seed) % target_count as u64) as usize;
    let scaled = permuted * 10_000 / target_count;
    match scaled {
        0..=2_999 => FormulaContext::FormulaOnly,
        3_000..=3_999 => FormulaContext::ChineseProse,
        4_000..=4_999 => FormulaContext::Heading,
        5_000..=5_999 => FormulaContext::CodeBlock,
        6_000..=6_999 => FormulaContext::List,
        7_000..=7_999 => FormulaContext::Table,
        _ => FormulaContext::MixedMarkdown,
    }
}

fn category_slug(category: FormulaCategory) -> &'static str {
    match category {
        FormulaCategory::BasicStructure => "basic-structure",
        FormulaCategory::Matrix => "matrix",
        FormulaCategory::Multiline => "multiline",
        FormulaCategory::Cases => "cases",
        FormulaCategory::NestedCalculus => "nested-calculus",
        FormulaCategory::ProbabilityStatistics => "probability-statistics",
        FormulaCategory::Chemistry => "chemistry",
        FormulaCategory::CustomStyles => "custom-styles",
        FormulaCategory::Drawing => "drawing",
        FormulaCategory::Malformed => "malformed",
        FormulaCategory::OfficeCrossReference => "office-cross-reference",
    }
}

struct PilotTemplate {
    id_suffix: &'static str,
    category: FormulaCategory,
    body: &'static str,
    wrapper: FormulaWrapper,
    complexity: FormulaComplexity,
    expected_outcome: ExpectedFormulaOutcome,
    output_targets: Vec<FormulaOutputTarget>,
    insertion_modes: Vec<FormulaInsertionMode>,
    tags: Vec<&'static str>,
}

fn pilot_templates() -> Vec<PilotTemplate> {
    use FormulaComplexity::{L1, L2, L3, L4, L5};
    use FormulaInsertionMode::{
        Batch, Clipboard, Display, Inline, NativeFormula, Ole, Png as InsertPng, Svg as InsertSvg,
    };
    use FormulaOutputTarget::{Docx, Latex, Mathml, Omml, Png, Pptx, Svg, Typst, Xlsx};
    use FormulaWrapper::{Bare, DisplayBrackets, DisplayDollar, InlineParentheses};

    let semantic_outputs = || vec![Latex, Mathml, Omml, Typst, Svg, Png];
    vec![
        PilotTemplate {
            id_suffix: "basic-structure",
            category: FormulaCategory::BasicStructure,
            body: r"\frac{a_1+b^2}{\sqrt{x}}",
            wrapper: InlineParentheses,
            complexity: L1,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Clipboard, Inline],
            tags: vec!["fraction", "root", "subscript", "superscript"],
        },
        PilotTemplate {
            id_suffix: "matrix",
            category: FormulaCategory::Matrix,
            body: r"\begin{bmatrix}a&b\\c&d\end{bmatrix}",
            wrapper: DisplayBrackets,
            complexity: L2,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Display],
            tags: vec!["matrix", "array"],
        },
        PilotTemplate {
            id_suffix: "multiline-align",
            category: FormulaCategory::Multiline,
            body: r"\begin{align}a+b&=c\\d-e&=f\end{align}",
            wrapper: DisplayDollar,
            complexity: L3,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Display, Batch],
            tags: vec!["align", "multiline"],
        },
        PilotTemplate {
            id_suffix: "cases",
            category: FormulaCategory::Cases,
            body: r"f(x)=\begin{cases}x^2,&x\ge0\\-x,&x<0\end{cases}",
            wrapper: DisplayBrackets,
            complexity: L3,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Display],
            tags: vec!["cases", "piecewise"],
        },
        PilotTemplate {
            id_suffix: "nested-calculus",
            category: FormulaCategory::NestedCalculus,
            body: r"\int_0^\infty\!\left(\int_{-\infty}^{\infty}e^{-x^2-y^2}\,dx\right)dy",
            wrapper: DisplayDollar,
            complexity: L5,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Display],
            tags: vec!["integral", "nested", "limits"],
        },
        PilotTemplate {
            id_suffix: "probability-statistics",
            category: FormulaCategory::ProbabilityStatistics,
            body: r"\Pr(A\mid B)=\frac{\Pr(B\mid A)\Pr(A)}{\Pr(B)}",
            wrapper: InlineParentheses,
            complexity: L3,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, Inline],
            tags: vec!["probability", "bayes"],
        },
        PilotTemplate {
            id_suffix: "chemistry",
            category: FormulaCategory::Chemistry,
            body: r"\ce{2H2 + O2 -> 2H2O}",
            wrapper: InlineParentheses,
            complexity: L2,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: vec![Latex, Mathml, Svg, Png, Docx],
            insertion_modes: vec![NativeFormula, InsertSvg, InsertPng, Inline],
            tags: vec!["chemistry", "reaction"],
        },
        PilotTemplate {
            id_suffix: "custom-style",
            category: FormulaCategory::CustomStyles,
            body: r"\overset{\star}{\longrightarrow}_{\mathrm{custom}}",
            wrapper: Bare,
            complexity: L2,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: semantic_outputs(),
            insertion_modes: vec![NativeFormula, InsertSvg, InsertPng],
            tags: vec!["custom-symbol", "style", "overset"],
        },
        PilotTemplate {
            id_suffix: "tikz-pgfplots",
            category: FormulaCategory::Drawing,
            body: r"\begin{tikzpicture}\draw[->](0,0)--(1,1);\end{tikzpicture}",
            wrapper: Bare,
            complexity: L4,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: vec![Latex, Svg, Png, Docx, Pptx],
            insertion_modes: vec![InsertSvg, InsertPng, Clipboard, Display],
            tags: vec!["tikz", "drawing", "safe-preview"],
        },
        PilotTemplate {
            id_suffix: "malformed-recoverable",
            category: FormulaCategory::Malformed,
            body: r"\frac{1}{",
            wrapper: DisplayDollar,
            complexity: L2,
            expected_outcome: ExpectedFormulaOutcome::RecoverableError,
            output_targets: vec![Latex],
            insertion_modes: vec![],
            tags: vec!["malformed", "recovery", "missing-brace"],
        },
        PilotTemplate {
            id_suffix: "malformed-reject",
            category: FormulaCategory::Malformed,
            body: r"\begin{matrix}1&2",
            wrapper: Bare,
            complexity: L3,
            expected_outcome: ExpectedFormulaOutcome::Reject,
            output_targets: vec![Latex],
            insertion_modes: vec![],
            tags: vec!["malformed", "reject", "missing-end"],
        },
        PilotTemplate {
            id_suffix: "office-cross-reference",
            category: FormulaCategory::OfficeCrossReference,
            body: r"E=mc^2",
            wrapper: DisplayBrackets,
            complexity: L1,
            expected_outcome: ExpectedFormulaOutcome::Valid,
            output_targets: vec![Latex, Mathml, Omml, Svg, Png, Docx, Pptx, Xlsx],
            insertion_modes: vec![
                NativeFormula,
                InsertSvg,
                InsertPng,
                Ole,
                Clipboard,
                Batch,
                Inline,
                Display,
            ],
            tags: vec!["office", "equation-number", "seq-ref", "read-back"],
        },
    ]
}

fn apply_wrapper(body: &str, wrapper: FormulaWrapper) -> String {
    match wrapper {
        FormulaWrapper::DisplayDollar => format!("$${body}$$"),
        FormulaWrapper::InlineParentheses => format!(r"\({body}\)"),
        FormulaWrapper::DisplayBrackets => format!(r"\[{body}\]"),
        FormulaWrapper::Bare => body.to_string(),
    }
}

fn apply_context(formula: &str, context: FormulaContext) -> String {
    match context {
        FormulaContext::FormulaOnly => formula.to_string(),
        FormulaContext::ChineseProse => format!("计算结果如下：{formula}，请核对。"),
        FormulaContext::Heading => format!("## 公式示例\n\n{formula}"),
        FormulaContext::CodeBlock => format!("```latex\n{formula}\n```"),
        FormulaContext::List => format!("- 条件\n- 结果：{formula}"),
        FormulaContext::Table => format!("| 名称 | 公式 |\n| --- | --- |\n| 示例 | {formula} |"),
        FormulaContext::MixedMarkdown => {
            format!("# 混合文档\n\n正文包含公式 {formula}\n\n> 生成于固定语料")
        }
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, FormulaCorpusError> {
    let bytes = fs::read(path).map_err(|source| FormulaCorpusError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| FormulaCorpusError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn collect_complete_minimums<T>(
    entries: impl IntoIterator<Item = (T, usize)>,
    required: &[T],
    target_count: usize,
    label: &str,
) -> Result<BTreeMap<T, usize>, FormulaCorpusError>
where
    T: Copy + Ord + std::fmt::Debug,
{
    let mut values = BTreeMap::new();
    for (key, count) in entries {
        if count == 0 || count > target_count {
            return invalid(format!(
                "{label} for {key:?} must be between 1 and {target_count}"
            ));
        }
        if values.insert(key, count).is_some() {
            return invalid(format!("duplicate {label} for {key:?}"));
        }
    }
    let expected: BTreeSet<_> = required.iter().copied().collect();
    let actual: BTreeSet<_> = values.keys().copied().collect();
    if actual != expected {
        return invalid(format!("{label} entries must cover every declared value"));
    }
    Ok(values)
}

fn enforce_minimums<T>(
    actual: &BTreeMap<T, usize>,
    expected: impl IntoIterator<Item = (T, usize)>,
    label: &str,
) -> Result<(), FormulaCorpusError>
where
    T: Copy + Ord + std::fmt::Debug,
{
    for (key, minimum) in expected {
        let count = actual.get(&key).copied().unwrap_or_default();
        if count < minimum {
            return invalid(format!(
                "{label} {key:?} requires at least {minimum} records, got {count}"
            ));
        }
    }
    Ok(())
}

fn require_coverage<T>(
    actual: &BTreeMap<T, usize>,
    expected: &[T],
    label: &str,
) -> Result<(), FormulaCorpusError>
where
    T: Copy + Ord + std::fmt::Debug,
{
    for value in expected {
        if actual.get(value).copied().unwrap_or_default() == 0 {
            return invalid(format!("corpus does not cover {label} {value:?}"));
        }
    }
    Ok(())
}

fn validate_unique_values<T: Copy + Ord + std::fmt::Debug>(
    values: &[T],
    label: &str,
    record_id: &str,
) -> Result<(), FormulaCorpusError> {
    let unique: BTreeSet<_> = values.iter().copied().collect();
    if unique.len() != values.len() {
        invalid(format!(
            "record '{record_id}' contains duplicate {label} values"
        ))
    } else {
        Ok(())
    }
}

fn increment<T: Copy + Ord>(counts: &mut BTreeMap<T, usize>, key: T) {
    *counts.entry(key).or_default() += 1;
}

fn validate_nonempty(value: &str, label: &str) -> Result<(), FormulaCorpusError> {
    if value.trim().is_empty() {
        invalid(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn validate_sha256(value: &str, label: &str) -> Result<(), FormulaCorpusError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        invalid(format!(
            "{label} must be a lowercase 64-character SHA-256 digest"
        ))
    } else {
        Ok(())
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, FormulaCorpusError> {
    Err(FormulaCorpusError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked_in_plan() -> FormulaCorpusPlan {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evaluation/formula-corpus/plan.json");
        read_formula_plan(&path).expect("checked-in formula corpus plan must parse")
    }

    #[test]
    fn checked_in_plan_freezes_the_requested_scale() {
        let plan = checked_in_plan();
        validate_formula_plan(&plan).expect("checked-in formula corpus plan must validate");
        assert_eq!(plan.target_formula_count, 10_000);
        assert_eq!(plan.compound_document_count, 500);
        assert_eq!(plan.category_quotas.len(), FormulaCategory::ALL.len());
        assert_eq!(plan.pilot_content_sha256.len(), 64);
        assert_eq!(plan.pull_request_content_sha256.len(), 64);
        assert_eq!(plan.full_content_sha256.len(), 64);
    }

    #[test]
    fn pilot_generation_is_deterministic_and_matches_the_frozen_digest() {
        let plan = checked_in_plan();
        let first = generate_formula_pilot(&plan).expect("pilot generation should succeed");
        let second = generate_formula_pilot(&plan).expect("pilot generation should be repeatable");
        assert_eq!(first, second);
        assert_eq!(first.content_sha256, plan.pilot_content_sha256);
        validate_formula_corpus(&plan, &first).expect("generated pilot should validate");
    }

    #[test]
    fn pull_request_generation_is_deterministic_and_covers_the_smoke_contract() {
        let plan = checked_in_plan();
        let first =
            generate_formula_pull_request(&plan).expect("pull-request generation should succeed");
        let second =
            generate_formula_pull_request(&plan).expect("pull-request generation should repeat");
        assert_eq!(first, second);
        assert_eq!(first.records.len(), FORMULA_PULL_REQUEST_RECORD_COUNT);
        assert_eq!(first.tier, FormulaCorpusTier::PullRequest);
        validate_formula_corpus(&plan, &first).expect("pull-request corpus should validate");
        assert!(first.records.iter().any(|record| {
            record.context == FormulaContext::MixedMarkdown
                && record.source.starts_with("# 混合文档")
        }));
    }

    #[test]
    fn full_generation_matches_every_frozen_quota_and_minimum() {
        let plan = checked_in_plan();
        let first = generate_formula_full(&plan).expect("full generation should succeed");
        let second = generate_formula_full(&plan).expect("full generation should repeat");
        assert_eq!(first.content_sha256, second.content_sha256);
        assert_eq!(first.records, second.records);
        assert_eq!(first.records.len(), plan.target_formula_count);
        assert_eq!(first.tier, FormulaCorpusTier::Full);
        validate_formula_corpus(&plan, &first).expect("full corpus should validate");
    }

    #[test]
    fn plan_rejects_category_quota_drift() {
        let mut plan = checked_in_plan();
        plan.category_quotas[0].count += 1;
        let error = validate_formula_plan(&plan).expect_err("quota drift must be rejected");
        assert!(error.to_string().contains("must sum"));
    }

    #[test]
    fn corpus_rejects_duplicate_record_ids() {
        let plan = checked_in_plan();
        let mut corpus = generate_formula_pilot(&plan).expect("pilot generation should succeed");
        corpus.records[1].id = corpus.records[0].id.clone();
        corpus.content_sha256 =
            compute_formula_records_digest(&corpus.records).expect("digest should succeed");
        let error = validate_formula_corpus(&plan, &corpus)
            .expect_err("duplicate record IDs must be rejected");
        assert!(error.to_string().contains("duplicate formula record id"));
    }
}

//! Explicitly supplied local fragments, never a history or storage provider.
//!
//! The trusted host authorizes retrieval before supplying data. Policy flags and
//! opaque scope hashes bind an editing snapshot; they are not credentials. Core
//! does no IO, macro expansion, learning, or execution of the returned source.
use super::scoped::{
    self, FormulaDefinitionCatalog, FormulaDefinitionReference, ScopedFormulaCompletionError,
};
use super::*;
use std::collections::BTreeSet;

pub const FRAGMENT_FORMULA_COMPLETION_VERSION: u16 = 3;
pub const FORMULA_COMPLETION_MAX_FRAGMENTS: usize = 64;
pub const FORMULA_COMPLETION_MAX_FRAGMENT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentPolicy {
    pub retrieval_enabled: bool,
    pub sensitive_document: bool,
    pub allow_in_sensitive_document: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentDefinition {
    pub id: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub user_sha256: String,
    pub project_sha256: Option<String>,
    pub document_sha256: Option<String>,
    /// ASCII letters matched against a control-word prefix, without backslash.
    pub trigger: String,
    pub label: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentCatalog {
    pub commands: FormulaDefinitionCatalog,
    pub policy: FormulaFragmentPolicy,
    pub fragments: Vec<FormulaFragmentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaFragmentReference {
    pub id: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaFragmentCandidate {
    pub edit: FormulaCompletionCandidate,
    pub definition: Option<FormulaDefinitionReference>,
    pub fragment: Option<FormulaFragmentReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentSnapshot {
    pub schema_version: u16,
    pub source_sha256: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub cursor: usize,
    pub catalog_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaFragmentCompletion {
    pub snapshot: FormulaFragmentSnapshot,
    pub context: FormulaCompletionContext,
    pub candidates: Vec<FormulaFragmentCandidate>,
}

type FragmentResult<T> = Result<T, ScopedFormulaCompletionError>;

struct PreparedFragments<'a> {
    commands: scoped::PreparedCatalog<'a>,
    fragments: Vec<&'a FormulaFragmentDefinition>,
    digest: String,
}

fn prepare(catalog: &FormulaFragmentCatalog) -> FragmentResult<PreparedFragments<'_>> {
    if catalog.fragments.len() > FORMULA_COMPLETION_MAX_FRAGMENTS {
        return Err(ScopedFormulaCompletionError::LimitExceeded);
    }
    let commands = scoped::prepare(&catalog.commands)?;
    let scope = &catalog.commands.scope;
    let allowed = catalog.policy.retrieval_enabled
        && (!catalog.policy.sensitive_document || catalog.policy.allow_in_sensitive_document);
    // Filter before validating or hashing private labels, triggers and bodies.
    let mut fragments: Vec<_> = catalog
        .fragments
        .iter()
        .filter(|entry| {
            allowed
                && entry.user_sha256 == scope.user_sha256
                && (entry.project_sha256.is_none() || entry.project_sha256 == scope.project_sha256)
                && (entry.document_sha256.is_none()
                    || entry.document_sha256 == scope.document_sha256)
        })
        .collect();
    let mut ids = BTreeSet::new();
    for entry in &fragments {
        if entry.source.len() > FORMULA_COMPLETION_MAX_FRAGMENT_BYTES {
            return Err(ScopedFormulaCompletionError::LimitExceeded);
        }
        if entry.id.is_empty()
            || entry.id.len() > 64
            || !entry
                .id
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.'))
            || entry.trigger.is_empty()
            || entry.trigger.len() > 64
            || !entry.trigger.bytes().all(|ch| ch.is_ascii_alphabetic())
            || entry.label.trim().is_empty()
            || entry.label.len() > 128
            || entry.label.chars().any(char::is_control)
            || entry.source.trim().is_empty()
            || entry
                .source
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\r' | '\n'))
            || !scoped::valid_optional_hash(&entry.project_sha256)
            || !scoped::valid_optional_hash(&entry.document_sha256)
            || !ids.insert(&entry.id)
        {
            return Err(ScopedFormulaCompletionError::InvalidCatalog);
        }
        crate::formula_fragment::validate_latex_fragment_source(&entry.source)
            .map_err(|_| ScopedFormulaCompletionError::InvalidCatalog)?;
        let context = analyze_with_signatures(&entry.source, 0, 0, &commands.signatures)?;
        if !context.diagnostics.is_empty() {
            return Err(ScopedFormulaCompletionError::InvalidCatalog);
        }
    }
    fragments.sort_by(|left, right| left.id.cmp(&right.id));
    let bytes = serde_json::to_vec(&(&commands.digest, &catalog.policy, &fragments))
        .map_err(|_| ScopedFormulaCompletionError::InvalidCatalog)?;
    Ok(PreparedFragments {
        commands,
        fragments,
        digest: format!("{:x}", Sha256::digest(bytes)),
    })
}

/// Retrieve bounded local templates after the host has authorized the catalog.
pub fn complete_formula_fragments(
    source: &str,
    cursor: usize,
    revision: u64,
    catalog: &FormulaFragmentCatalog,
) -> FragmentResult<FormulaFragmentCompletion> {
    let prepared = prepare(catalog)?;
    complete_prepared(source, cursor, revision, &prepared)
}

fn complete_prepared(
    source: &str,
    cursor: usize,
    revision: u64,
    prepared: &PreparedFragments<'_>,
) -> FragmentResult<FormulaFragmentCompletion> {
    let base = scoped::complete_prepared(source, cursor, revision, &prepared.commands)?;
    let mut context = base.context;
    context.schema_version = FRAGMENT_FORMULA_COMPLETION_VERSION;
    let mut candidates = Vec::new();
    if !context.in_comment
        && !context.in_text
        && !matches!(
            context.slot,
            FormulaSlot::EnvironmentName
                | FormulaSlot::ArrayColumnSpec
                | FormulaSlot::FractionAlignment
        )
    {
        if let Some(prefix) = &context.command_prefix {
            for fragment in prepared
                .fragments
                .iter()
                .filter(|entry| entry.trigger.starts_with(&prefix.prefix))
            {
                match candidates::candidate(
                    source,
                    &context,
                    &format!("fragment:{}", fragment.id),
                    &fragment.label,
                    prefix.replace,
                    &fragment.source,
                    &prepared.commands.signatures,
                ) {
                    Ok(mut edit) => {
                        edit.source = "core:fragment".into();
                        candidates.push(FormulaFragmentCandidate {
                            edit,
                            definition: None,
                            fragment: Some(FormulaFragmentReference {
                                id: fragment.id.clone(),
                                revision: fragment.revision,
                            }),
                        });
                    }
                    Err(FormulaCompletionError::LimitExceeded) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    candidates.extend(
        base.candidates
            .into_iter()
            .map(|entry| FormulaFragmentCandidate {
                edit: entry.edit,
                definition: entry.definition,
                fragment: None,
            })
            .take(FORMULA_COMPLETION_MAX_CANDIDATES - candidates.len()),
    );
    Ok(FormulaFragmentCompletion {
        snapshot: FormulaFragmentSnapshot {
            schema_version: FRAGMENT_FORMULA_COMPLETION_VERSION,
            source_sha256: context.source_sha256.clone(),
            revision,
            cursor,
            catalog_sha256: prepared.digest.clone(),
        },
        context,
        candidates,
    })
}

/// Accept only a regenerated candidate bound to the CURRENT host state/policy.
pub fn apply_formula_candidate_fragments(
    source: &str,
    cursor: usize,
    revision: u64,
    catalog: &FormulaFragmentCatalog,
    snapshot: &FormulaFragmentSnapshot,
    candidate_id: &str,
) -> FragmentResult<AppliedFormulaCompletion> {
    if source.len() > MAX_BYTES {
        return Err(ScopedFormulaCompletionError::LimitExceeded);
    }
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return Err(ScopedFormulaCompletionError::InvalidCursor);
    }
    let prepared = prepare(catalog)?;
    if snapshot.schema_version != FRAGMENT_FORMULA_COMPLETION_VERSION
        || snapshot.source_sha256 != format!("{:x}", Sha256::digest(source.as_bytes()))
        || snapshot.revision != revision
        || snapshot.cursor != cursor
        || snapshot.catalog_sha256 != prepared.digest
    {
        return Err(ScopedFormulaCompletionError::StaleContext);
    }
    let current = complete_prepared(source, cursor, revision, &prepared)?;
    let selected = current
        .candidates
        .iter()
        .find(|entry| entry.edit.id == candidate_id)
        .ok_or(ScopedFormulaCompletionError::UnknownCandidate)?;
    Ok(candidates::apply_selected(source, &selected.edit)?)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentCompletionRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    pub cursor: usize,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub catalog: FormulaFragmentCatalog,
}
impl FormulaFragmentCompletionRequest {
    pub fn execute(&self) -> FragmentResult<FormulaFragmentCompletion> {
        complete_formula_fragments(&self.source, self.cursor, self.revision, &self.catalog)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaFragmentCompletionApplyRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    pub cursor: usize,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub catalog: FormulaFragmentCatalog,
    pub snapshot: FormulaFragmentSnapshot,
    pub candidate_id: String,
}
impl FormulaFragmentCompletionApplyRequest {
    pub fn execute(&self) -> FragmentResult<AppliedFormulaCompletion> {
        apply_formula_candidate_fragments(
            &self.source,
            self.cursor,
            self.revision,
            &self.catalog,
            &self.snapshot,
            &self.candidate_id,
        )
    }
}

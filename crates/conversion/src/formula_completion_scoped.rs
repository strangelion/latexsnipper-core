//! Host-authorized, scoped command signatures. No macro bodies, IO or learning.
//!
//! Scope hashes are opaque host identities, not credentials. The host must filter
//! authorized declarations and advance generation after clear/reset. All state is
//! passed explicitly; Core never stores names, histories or document contents.
use super::*;
use std::collections::BTreeSet;

pub const SCOPED_FORMULA_COMPLETION_VERSION: u16 = 2;
pub const FORMULA_COMPLETION_MAX_DEFINITIONS: usize = 64;
pub const FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaDefinitionScope {
    pub user_sha256: String,
    pub project_sha256: Option<String>,
    pub document_sha256: Option<String>,
    #[serde(with = "wire::revision_decimal")]
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormulaDefinitionArgument {
    Math,
    Text,
}

/// A TeX control word without a leading backslash. Bodies/expansion are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCommandDefinition {
    pub id: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub user_sha256: String,
    pub project_sha256: Option<String>,
    pub command: String,
    pub arguments: Vec<FormulaDefinitionArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaDefinitionCatalog {
    pub scope: FormulaDefinitionScope,
    pub definitions: Vec<FormulaCommandDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaDefinitionReference {
    pub id: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedFormulaCandidate {
    pub edit: FormulaCompletionCandidate,
    pub definition: Option<FormulaDefinitionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedFormulaSnapshot {
    pub schema_version: u16,
    pub source_sha256: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub cursor: usize,
    pub definitions_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedFormulaCompletion {
    pub snapshot: ScopedFormulaSnapshot,
    /// Version 2 prevents accidentally accepting this context with the v1 API.
    pub context: FormulaCompletionContext,
    pub candidates: Vec<ScopedFormulaCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScopedFormulaCompletionError {
    InvalidCatalog,
    LimitExceeded,
    InvalidCursor,
    StaleContext,
    UnknownCandidate,
}

impl std::fmt::Display for ScopedFormulaCompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Scoped formula completion {self:?}")
    }
}
impl std::error::Error for ScopedFormulaCompletionError {}
impl From<FormulaCompletionError> for ScopedFormulaCompletionError {
    fn from(value: FormulaCompletionError) -> Self {
        match value {
            FormulaCompletionError::InvalidCursor => Self::InvalidCursor,
            FormulaCompletionError::LimitExceeded => Self::LimitExceeded,
            FormulaCompletionError::StaleContext => Self::StaleContext,
            FormulaCompletionError::UnknownCandidate => Self::UnknownCandidate,
        }
    }
}
type ScopedResult<T> = Result<T, ScopedFormulaCompletionError>;

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_optional_hash(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(valid_hash)
}

// Keep parser-owned syntax and built-in completion names unambiguous. This is
// not a compiler allowlist; a valid declaration never authorizes TeX execution.
fn reserved(command: &str) -> bool {
    candidates::builtin_command(command)
        || delimiters::builtin_command(command)
        || matches!(
            command,
            "dbinom"
                | "tbinom"
                | "textrm"
                | "textbf"
                | "textit"
                | "textsf"
                | "texttt"
                | "underline"
                | "mathsf"
                | "mathtt"
                | "mathfrak"
                | "begin"
                | "end"
                | "verb"
                | "left"
                | "right"
                | "middle"
                | "newcommand"
                | "renewcommand"
                | "providecommand"
                | "def"
                | "gdef"
                | "edef"
                | "xdef"
                | "let"
                | "csname"
                | "endcsname"
                | "input"
                | "include"
                | "catcode"
        )
}

struct PreparedCatalog<'a> {
    definitions: Vec<&'a FormulaCommandDefinition>,
    signatures: Vec<CommandSignature>,
    digest: String,
}

fn prepare(catalog: &FormulaDefinitionCatalog) -> ScopedResult<PreparedCatalog<'_>> {
    if catalog.definitions.len() > FORMULA_COMPLETION_MAX_DEFINITIONS {
        return Err(ScopedFormulaCompletionError::LimitExceeded);
    }
    let scope = &catalog.scope;
    if !valid_hash(&scope.user_sha256)
        || !valid_optional_hash(&scope.project_sha256)
        || !valid_optional_hash(&scope.document_sha256)
    {
        return Err(ScopedFormulaCompletionError::InvalidCatalog);
    }
    // Filter before inspecting private names/signatures or exposing diagnostics.
    let mut definitions: Vec<_> = catalog
        .definitions
        .iter()
        .filter(|definition| {
            definition.user_sha256 == scope.user_sha256
                && (definition.project_sha256.is_none()
                    || definition.project_sha256 == scope.project_sha256)
        })
        .collect();
    let mut ids = BTreeSet::new();
    let mut commands = BTreeSet::new();
    for definition in &definitions {
        if definition.arguments.len() > FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS {
            return Err(ScopedFormulaCompletionError::LimitExceeded);
        }
        if definition.id.is_empty()
            || definition.id.len() > 64
            || !definition
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || definition.command.is_empty()
            || definition.command.len() > 64
            || !definition
                .command
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic())
            || !valid_optional_hash(&definition.project_sha256)
            || reserved(&definition.command)
            || !ids.insert(&definition.id)
            || !commands.insert(&definition.command)
        {
            // Refuse collisions, including project-vs-user shadowing. Do not
            // guess precedence or include private definition values in errors.
            return Err(ScopedFormulaCompletionError::InvalidCatalog);
        }
    }
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    let signatures = definitions
        .iter()
        .map(|definition| CommandSignature {
            command: definition.command.clone(),
            slots: definition
                .arguments
                .iter()
                .map(|arg| match arg {
                    FormulaDefinitionArgument::Math => FormulaSlot::MathArgument,
                    FormulaDefinitionArgument::Text => FormulaSlot::Text,
                })
                .collect(),
        })
        .collect();
    // Bind actual content as well as claimed revisions, scope and reset epoch.
    // Fixed struct serialization plus sorted unique IDs makes order irrelevant.
    let bytes = serde_json::to_vec(&(scope, &definitions))
        .map_err(|_| ScopedFormulaCompletionError::InvalidCatalog)?;
    let digest = format!("{:x}", Sha256::digest(bytes));
    Ok(PreparedCatalog {
        definitions,
        signatures,
        digest,
    })
}

/// Deterministic, bounded completion with explicitly authorized definitions.
pub fn complete_formula_scoped(
    source: &str,
    cursor: usize,
    revision: u64,
    catalog: &FormulaDefinitionCatalog,
) -> ScopedResult<ScopedFormulaCompletion> {
    let prepared = prepare(catalog)?;
    complete_prepared(source, cursor, revision, &prepared)
}

fn complete_prepared(
    source: &str,
    cursor: usize,
    revision: u64,
    prepared: &PreparedCatalog<'_>,
) -> ScopedResult<ScopedFormulaCompletion> {
    let mut base =
        candidates::complete_with_signatures(source, cursor, revision, &prepared.signatures)?;
    base.context.schema_version = SCOPED_FORMULA_COMPLETION_VERSION;
    let context = base.context;
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
            for definition in prepared
                .definitions
                .iter()
                .filter(|entry| entry.command.starts_with(&prefix.prefix))
            {
                let mut insert = format!("\\{}", definition.command);
                // An existing argument list is preserved as a whole. Partial
                // lists remain incomplete instead of guessing/duplicating args.
                if !candidates::existing_group(source, prefix.replace.end, false) {
                    for _ in &definition.arguments {
                        insert.push_str("{}");
                    }
                }
                let edit = candidates::candidate(
                    source,
                    &context,
                    &format!("definition:{}", definition.id),
                    &format!("\\{}", definition.command),
                    prefix.replace,
                    &insert,
                    &prepared.signatures,
                );
                match edit {
                    Ok(mut edit) => {
                        edit.source = "core:definition".into();
                        candidates.push(ScopedFormulaCandidate {
                            edit,
                            definition: Some(FormulaDefinitionReference {
                                id: definition.id.clone(),
                                revision: definition.revision,
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
            .map(|edit| ScopedFormulaCandidate {
                edit,
                definition: None,
            })
            .take(FORMULA_COMPLETION_MAX_CANDIDATES - candidates.len()),
    );
    Ok(ScopedFormulaCompletion {
        snapshot: ScopedFormulaSnapshot {
            schema_version: SCOPED_FORMULA_COMPLETION_VERSION,
            source_sha256: context.source_sha256.clone(),
            revision,
            cursor,
            definitions_sha256: prepared.digest.clone(),
        },
        context,
        candidates,
    })
}

/// Recompute a selected edit; no client-provided replacement payload is trusted.
pub fn apply_formula_candidate_scoped(
    source: &str,
    cursor: usize,
    revision: u64,
    catalog: &FormulaDefinitionCatalog,
    snapshot: &ScopedFormulaSnapshot,
    candidate_id: &str,
) -> ScopedResult<AppliedFormulaCompletion> {
    if source.len() > MAX_BYTES {
        return Err(ScopedFormulaCompletionError::LimitExceeded);
    }
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return Err(ScopedFormulaCompletionError::InvalidCursor);
    }
    let prepared = prepare(catalog)?;
    if snapshot.schema_version != SCOPED_FORMULA_COMPLETION_VERSION
        || snapshot.source_sha256 != format!("{:x}", Sha256::digest(source.as_bytes()))
        || snapshot.revision != revision
        || snapshot.cursor != cursor
        || snapshot.definitions_sha256 != prepared.digest
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
pub struct ScopedFormulaCompletionRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    pub cursor: usize,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub catalog: FormulaDefinitionCatalog,
}
impl ScopedFormulaCompletionRequest {
    pub fn execute(&self) -> ScopedResult<ScopedFormulaCompletion> {
        complete_formula_scoped(&self.source, self.cursor, self.revision, &self.catalog)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedFormulaCompletionApplyRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    pub cursor: usize,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub catalog: FormulaDefinitionCatalog,
    pub snapshot: ScopedFormulaSnapshot,
    pub candidate_id: String,
}
impl ScopedFormulaCompletionApplyRequest {
    pub fn execute(&self) -> ScopedResult<AppliedFormulaCompletion> {
        apply_formula_candidate_scoped(
            &self.source,
            self.cursor,
            self.revision,
            &self.catalog,
            &self.snapshot,
            &self.candidate_id,
        )
    }
}

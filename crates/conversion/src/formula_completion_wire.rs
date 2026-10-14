//! Transport-neutral, precision-preserving requests for bounded LaTeX completion.
use super::*;

/// Explicit finite grammar, independent of conversion format capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormulaCompletionInputFormat {
    Latex,
}

/// Clients echo this token with their current source, cursor and revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCompletionSnapshot {
    pub schema_version: u16,
    pub source_sha256: String,
    #[serde(with = "revision_decimal")]
    pub revision: u64,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCompletionRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    /// UTF-8 byte offset, not a UTF-16 editor index.
    pub cursor: usize,
    #[serde(with = "revision_decimal")]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCompletionReply {
    pub snapshot: FormulaCompletionSnapshot,
    pub completion: FormulaCompletionResult,
}

impl FormulaCompletionRequest {
    pub fn execute(&self) -> EditResult<FormulaCompletionReply> {
        let completion = complete_formula(&self.source, self.cursor, self.revision)?;
        Ok(FormulaCompletionReply {
            snapshot: completion.context.snapshot(),
            completion,
        })
    }
}

/// Only a candidate ID is accepted; clients cannot supply replacement edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCompletionApplyRequest {
    pub source: String,
    pub input_format: FormulaCompletionInputFormat,
    pub cursor: usize,
    #[serde(with = "revision_decimal")]
    pub revision: u64,
    pub snapshot: FormulaCompletionSnapshot,
    pub candidate_id: String,
}

impl FormulaCompletionApplyRequest {
    /// Returns data only. The host owns acceptance, revision increments and undo.
    pub fn execute(&self) -> EditResult<AppliedFormulaCompletion> {
        apply_formula_candidate(
            &self.source,
            self.cursor,
            self.revision,
            &self.snapshot,
            &self.candidate_id,
        )
    }
}

/// JSON numbers cannot losslessly transport all u64 editor revisions to JS.
pub(super) mod revision_decimal {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 20
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(D::Error::custom(
                "revision must be a canonical decimal u64 string",
            ));
        }
        value.parse().map_err(D::Error::custom)
    }
}

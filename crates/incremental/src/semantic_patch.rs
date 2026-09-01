use latexsnipper_ast::{Block, Inline, Span};
use serde::{Deserialize, Serialize};

use crate::{DocumentSession, InvalidationState, SessionEdit, SessionError};

/// Wire schema for source-aware semantic patches.
pub const SEMANTIC_PATCH_SCHEMA_VERSION: u16 = 1;

/// A revision-guarded, ordered set of semantic edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticPatch {
    pub schema_version: u16,
    pub base_revision: u64,
    pub operations: Vec<SemanticPatchOperation>,
}

impl SemanticPatch {
    pub fn new(base_revision: u64) -> Self {
        Self {
            schema_version: SEMANTIC_PATCH_SCHEMA_VERSION,
            base_revision,
            operations: Vec::new(),
        }
    }

    pub fn push(mut self, operation: SemanticPatchOperation) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

/// Stable-ID edits are preferred; source ranges are the lossless structural fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum SemanticPatchOperation {
    ReplaceFormulaSource { stable_id: String, latex: String },
    ReplaceParagraphSource { stable_id: String, text: String },
    ReplaceSourceRange { span: Span, replacement: String },
}

impl SemanticPatchOperation {
    fn as_session_edit(&self, expected_revision: u64) -> SessionEdit {
        match self {
            Self::ReplaceFormulaSource { stable_id, latex } => SessionEdit::ReplaceFormulaSource {
                expected_revision,
                stable_id: stable_id.clone(),
                latex: latex.clone(),
            },
            Self::ReplaceParagraphSource { stable_id, text } => {
                SessionEdit::ReplaceParagraphSource {
                    expected_revision,
                    stable_id: stable_id.clone(),
                    text: text.clone(),
                }
            }
            Self::ReplaceSourceRange { span, replacement } => SessionEdit::ReplaceSourceRange {
                expected_revision,
                span: *span,
                replacement: replacement.clone(),
            },
        }
    }
}

/// Aggregate result for an atomically applied patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOutcome {
    pub base_revision: u64,
    pub revision: u64,
    pub applied_operations: usize,
    pub invalidation: InvalidationState,
}

impl DocumentSession {
    /// Build a minimal stable-ID patch when the document structure is unchanged.
    /// Structural edits fall back to one complete source replacement so the parser remains
    /// the canonical authority for the resulting document.
    pub fn diff_source(
        &self,
        target_source: impl AsRef<str>,
    ) -> Result<SemanticPatch, SessionError> {
        let target_source = target_source.as_ref();
        if target_source == self.source() {
            return Ok(SemanticPatch::new(self.revision));
        }

        let target = DocumentSession::from_latex(self.session_id.clone(), target_source)?;
        let current_blocks = self.document().all_blocks();
        let target_blocks = target.document().all_blocks();
        if current_blocks.len() != target_blocks.len() {
            return Ok(self.full_source_patch(target_source));
        }

        let mut patch = SemanticPatch::new(self.revision);
        for (current, target) in current_blocks.into_iter().zip(target_blocks) {
            if current.type_name() != target.type_name() {
                return Ok(self.full_source_patch(target_source));
            }
            let current_fragment = source_fragment(self.source(), current)?;
            let target_fragment = source_fragment(target_source, target)?;
            if current_fragment == target_fragment {
                continue;
            }

            let Some(stable_id) = current
                .source()
                .and_then(|source| source.stable_id.as_ref())
                .cloned()
            else {
                return Ok(self.full_source_patch(target_source));
            };

            match (current, target) {
                (Block::Formula(_), Block::Formula(formula)) => {
                    patch
                        .operations
                        .push(SemanticPatchOperation::ReplaceFormulaSource {
                            stable_id,
                            latex: formula.formula.as_latex().to_string(),
                        });
                }
                (Block::Paragraph(current), Block::Paragraph(target))
                    if text_only(&current.inlines) && text_only(&target.inlines) =>
                {
                    patch
                        .operations
                        .push(SemanticPatchOperation::ReplaceParagraphSource {
                            stable_id,
                            text: inline_text(&target.inlines),
                        });
                }
                _ => return Ok(self.full_source_patch(target_source)),
            }
        }
        Ok(patch)
    }

    /// Apply all operations atomically. Any failed operation restores the entire session,
    /// including revision, caches, provenance and stable identities.
    pub fn apply_semantic_patch(
        &mut self,
        patch: &SemanticPatch,
    ) -> Result<PatchOutcome, SessionError> {
        if patch.schema_version != SEMANTIC_PATCH_SCHEMA_VERSION {
            return Err(SessionError::UnsupportedPatchSchema(patch.schema_version));
        }
        if patch.base_revision != self.revision {
            return Err(SessionError::RevisionConflict {
                expected: patch.base_revision,
                actual: self.revision,
            });
        }

        let snapshot = self.clone();
        let base_revision = self.revision;
        let mut invalidation = InvalidationState::default();
        for (index, operation) in patch.operations.iter().enumerate() {
            match self.apply_edit(operation.as_session_edit(self.revision)) {
                Ok(outcome) => merge_invalidation(&mut invalidation, outcome.invalidation),
                Err(error) => {
                    *self = snapshot;
                    return Err(SessionError::PatchOperationFailed {
                        operation: index,
                        message: error.to_string(),
                    });
                }
            }
        }

        Ok(PatchOutcome {
            base_revision,
            revision: self.revision,
            applied_operations: patch.operations.len(),
            invalidation,
        })
    }

    fn full_source_patch(&self, target_source: &str) -> SemanticPatch {
        SemanticPatch::new(self.revision).push(SemanticPatchOperation::ReplaceSourceRange {
            span: Span::new(0, self.source().len()),
            replacement: target_source.to_string(),
        })
    }
}

fn source_fragment<'a>(source: &'a str, block: &Block) -> Result<&'a str, SessionError> {
    let span = block
        .source()
        .and_then(|source| source.span)
        .ok_or(SessionError::MissingSource)?;
    source
        .get(span.start..span.end)
        .ok_or(SessionError::InvalidRange)
}

fn text_only(inlines: &[Inline]) -> bool {
    inlines
        .iter()
        .all(|inline| matches!(inline, Inline::Text(_)))
}

fn inline_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

fn merge_invalidation(target: &mut InvalidationState, source: InvalidationState) {
    target.dirty_nodes.extend(source.dirty_nodes);
    target
        .semantic_invalidated
        .extend(source.semantic_invalidated);
    target.render_invalidated.extend(source.render_invalidated);
    target.dependent_outputs.extend(source.dependent_outputs);
    target.full_reconcile_required |= source.full_reconcile_required;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_uses_stable_formula_and_paragraph_operations() {
        let mut session = DocumentSession::from_latex("semantic", "Before $x$ after.").unwrap();
        let patch = session.diff_source("Changed $y^2$ after.").unwrap();
        assert_eq!(patch.operations.len(), 2);
        assert!(matches!(
            patch.operations[0],
            SemanticPatchOperation::ReplaceParagraphSource { .. }
        ));
        assert!(matches!(
            patch.operations[1],
            SemanticPatchOperation::ReplaceFormulaSource { .. }
        ));

        let outcome = session.apply_semantic_patch(&patch).unwrap();
        assert_eq!(outcome.applied_operations, 2);
        assert_eq!(outcome.revision, 2);
        assert_eq!(session.source(), "Changed $y^2$ after.");
        assert!(session.verify_full_equivalence().unwrap());
    }

    #[test]
    fn structural_change_falls_back_to_complete_source_replacement() {
        let mut session = DocumentSession::from_latex("semantic", "$x$").unwrap();
        let patch = session.diff_source("$x$\n\n$y$").unwrap();
        assert_eq!(patch.operations.len(), 1);
        assert!(matches!(
            patch.operations[0],
            SemanticPatchOperation::ReplaceSourceRange { .. }
        ));
        session.apply_semantic_patch(&patch).unwrap();
        assert_eq!(session.source(), "$x$\n\n$y$");
        assert!(session.verify_full_equivalence().unwrap());
    }

    #[test]
    fn failed_operation_rolls_back_all_session_state() {
        let mut session = DocumentSession::from_latex("semantic", "$x$").unwrap();
        let formula_id = session.document().all_blocks()[0]
            .source()
            .and_then(|source| source.stable_id.clone())
            .unwrap();
        let patch = SemanticPatch::new(0)
            .push(SemanticPatchOperation::ReplaceFormulaSource {
                stable_id: formula_id,
                latex: "y".to_string(),
            })
            .push(SemanticPatchOperation::ReplaceFormulaSource {
                stable_id: "missing".to_string(),
                latex: "z".to_string(),
            });

        assert!(matches!(
            session.apply_semantic_patch(&patch),
            Err(SessionError::PatchOperationFailed { operation: 1, .. })
        ));
        assert_eq!(session.revision, 0);
        assert_eq!(session.source(), "$x$");
        assert!(session.invalidation().dirty_nodes.is_empty());
    }

    #[test]
    fn patch_checks_schema_and_base_revision() {
        let mut session = DocumentSession::from_latex("semantic", "$x$").unwrap();
        let mut patch = SemanticPatch::new(1);
        assert!(matches!(
            session.apply_semantic_patch(&patch),
            Err(SessionError::RevisionConflict { .. })
        ));
        patch.base_revision = 0;
        patch.schema_version = 99;
        assert!(matches!(
            session.apply_semantic_patch(&patch),
            Err(SessionError::UnsupportedPatchSchema(99))
        ));
    }

    #[test]
    fn patch_json_is_tagged_and_deterministic() {
        let patch = SemanticPatch::new(4).push(SemanticPatchOperation::ReplaceSourceRange {
            span: Span::new(2, 4),
            replacement: "x".to_string(),
        });
        let first = serde_json::to_string(&patch).unwrap();
        let second = serde_json::to_string(&patch).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\"schemaVersion\":1"));
        assert!(first.contains("\"operation\":\"replaceSourceRange\""));
    }
}

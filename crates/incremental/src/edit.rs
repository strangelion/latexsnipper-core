use latexsnipper_ast::Span;

/// Supported source-aware edits for the experimental session API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEdit {
    ReplaceSourceRange {
        expected_revision: u64,
        span: Span,
        replacement: String,
    },
    ReplaceFormulaSource {
        expected_revision: u64,
        stable_id: String,
        latex: String,
    },
    ReplaceParagraphSource {
        expected_revision: u64,
        stable_id: String,
        text: String,
    },
    /// Insert one source-backed block after `after_stable_id`, or at the start.
    InsertBlockSource {
        expected_revision: u64,
        after_stable_id: Option<String>,
        source: String,
    },
    /// Delete one source-backed block by stable identity.
    DeleteBlock {
        expected_revision: u64,
        stable_id: String,
    },
    /// Move one source-backed block after another stable block, or to the start.
    MoveBlock {
        expected_revision: u64,
        stable_id: String,
        after_stable_id: Option<String>,
    },
}

impl SessionEdit {
    pub fn expected_revision(&self) -> u64 {
        match self {
            Self::ReplaceSourceRange {
                expected_revision, ..
            }
            | Self::ReplaceFormulaSource {
                expected_revision, ..
            }
            | Self::ReplaceParagraphSource {
                expected_revision, ..
            }
            | Self::InsertBlockSource {
                expected_revision, ..
            }
            | Self::DeleteBlock {
                expected_revision, ..
            }
            | Self::MoveBlock {
                expected_revision, ..
            } => *expected_revision,
        }
    }
}

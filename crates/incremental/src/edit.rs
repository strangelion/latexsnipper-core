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
            } => *expected_revision,
        }
    }
}

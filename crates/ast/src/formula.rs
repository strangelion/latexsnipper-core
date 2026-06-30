use serde::{Deserialize, Serialize};

use crate::SourceInfo;

/// A mathematical formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    pub source: FormulaSource,
    pub display_mode: bool,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_info: Option<SourceInfo>,
}

/// The source format of a formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", content = "content")]
pub enum FormulaSource {
    Latex(String),
    Omml(String),
    Typst(String),
    MathML(String),
}

impl Formula {
    pub fn latex(latex: impl Into<String>) -> Self {
        Self {
            source: FormulaSource::Latex(latex.into()),
            display_mode: true,
            confidence: 1.0,
            source_info: None,
        }
    }

    pub fn with_source_info(mut self, info: SourceInfo) -> Self {
        self.source_info = Some(info);
        self
    }

    /// Get the formula as a LaTeX string, regardless of source format.
    pub fn as_latex(&self) -> &str {
        match &self.source {
            FormulaSource::Latex(s) => s,
            _ => "",
        }
    }
}

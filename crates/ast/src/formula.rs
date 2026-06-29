use serde::{Deserialize, Serialize};

/// A mathematical formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    pub source: FormulaSource,
    pub display_mode: bool,
    pub confidence: f32,
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
        }
    }

    /// Get the formula as a LaTeX string, regardless of source format.
    pub fn as_latex(&self) -> &str {
        match &self.source {
            FormulaSource::Latex(s) => s,
            _ => "",
        }
    }
}

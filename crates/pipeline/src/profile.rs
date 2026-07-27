/// Controlled vocabulary for built-in pipeline profiles.
///
/// Profiles deliberately describe supported behavior rather than allowing
/// untrusted manifests to instantiate arbitrary Rust node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineProfile {
    Formula,
    CroppedFormula,
    Text,
    Mixed,
    Handwriting,
    Table,
    FormulaLayout,
}

impl PipelineProfile {
    pub const fn pipeline_name(self) -> &'static str {
        match self {
            Self::Formula => "Formula_pipeline",
            Self::CroppedFormula => "CroppedFormula_pipeline",
            Self::Text => "Text_pipeline",
            Self::Mixed => "Mixed_pipeline",
            Self::Handwriting => "Handwriting_pipeline",
            Self::Table => "Table_pipeline",
            Self::FormulaLayout => "FormulaLayout_pipeline",
        }
    }
}

//! Adapters that normalize supported formula sources into `FormulaLayout`.

use latexsnipper_ast::{FormulaLayout, FormulaSource};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_inference::parse_formula_latex;

/// Convert any supported formula source to the existing structural layout tree.
///
/// XML and Typst inputs reuse their established source-to-LaTeX parsers before
/// entering the shared LaTeX layout parser. The original source remains owned
/// by `FormulaSource` for fidelity fallback.
pub fn parse_formula_source_to_layout(source: &FormulaSource) -> Result<FormulaLayout> {
    let latex = match source {
        FormulaSource::Latex(value) => value.clone(),
        FormulaSource::Omml(value) => {
            crate::parse_omml_to_latex(value).map_err(SnipperError::Conversion)?
        }
        FormulaSource::MathML(value) => {
            crate::parse_mathml_to_latex(value).map_err(SnipperError::Conversion)?
        }
        FormulaSource::Typst(value) => crate::parse_typst_to_latex(value),
    };
    parse_formula_latex(&latex).map_err(|error| SnipperError::Conversion(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_sources_share_the_latex_layout_parser() {
        let sources = [
            FormulaSource::Latex("\\frac{a}{b}".to_string()),
            FormulaSource::MathML("<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>".to_string()),
            FormulaSource::Omml(r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:fPr/><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"#.to_string()),
            FormulaSource::Typst("frac(a, b)".to_string()),
        ];

        for source in sources {
            let layout = parse_formula_source_to_layout(&source).unwrap();
            assert_eq!(layout.canonical_latex(), "\\frac{a}{b}");
        }
    }
}

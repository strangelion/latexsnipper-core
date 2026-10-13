//! Formula-only targets; legacy document OutputFormat stays unchanged.
use latexsnipper_foundation::{Result, SnipperError};

use crate::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaOutputTarget {
    Semantic(OutputFormat),
    UnicodeMath,
    AsciiMath,
}

impl FormulaOutputTarget {
    pub fn all() -> Vec<Self> {
        OutputFormat::all()
            .iter()
            .copied()
            .map(Self::Semantic)
            .chain([Self::UnicodeMath, Self::AsciiMath])
            .collect()
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Semantic(format) => format.name(),
            Self::UnicodeMath => "unicode-math",
            Self::AsciiMath => "ascii-math",
        }
    }

    pub fn parse(label: &str) -> Option<Self> {
        let normalized = label.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "unicode_math" => Some(Self::UnicodeMath),
            "ascii_math" => Some(Self::AsciiMath),
            _ => OutputFormat::all()
                .iter()
                .copied()
                .find(|format| format.name() == normalized)
                .map(Self::Semantic),
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Semantic(format) => crate::semantic_mime_type(format),
            Self::UnicodeMath | Self::AsciiMath => "text/plain; charset=utf-8",
        }
    }
}

pub(crate) fn linear_ast(
    source: &str,
    input: FormulaInputFormat,
) -> Result<crate::latex_ast::LatexNode> {
    match input {
        FormulaInputFormat::AsciiMath => {
            crate::asciimath_pilot::parse_asciimath(source).map_err(|error| match error.kind {
                crate::asciimath_pilot::AsciiMathErrorKind::LimitExceeded => {
                    SnipperError::LimitExceeded(error.to_string())
                }
                _ => SnipperError::Conversion(error.to_string()),
            })
        }
        FormulaInputFormat::UnicodeMath => crate::unicodemath_pilot::parse_unicodemath(source)
            .map_err(|error| match error.kind {
                crate::unicodemath_pilot::UnicodeMathErrorKind::LimitExceeded => {
                    SnipperError::LimitExceeded(error.to_string())
                }
                _ => SnipperError::Conversion(error.to_string()),
            }),
        _ => Err(SnipperError::Conversion(
            "Expected a declared linear formula grammar".into(),
        )),
    }
}

impl DocumentConverter {
    /// Bounded single-formula target dispatch. Does not add document converters.
    pub fn convert_formula_to_target(
        source: &str,
        input: FormulaInputFormat,
        output: FormulaOutputTarget,
        mode: FormulaConversionMode,
    ) -> Result<String> {
        if let FormulaOutputTarget::Semantic(format) = output {
            return Self::convert_formula_string(source, input, format, mode);
        }
        let target = if cfg!(target_arch = "wasm32") {
            CapabilityTarget::Wasm32UnknownUnknown
        } else {
            CapabilityTarget::Native
        };
        let capability = CapabilityRegistry::formula_conversion_target(input, output, mode, target);
        if !capability.available {
            return Err(SnipperError::Conversion(
                capability
                    .unavailable_reason
                    .unwrap_or("Formula route unavailable")
                    .into(),
            ));
        }
        if source.len() > 64 * 1024 {
            return Err(SnipperError::LimitExceeded(
                "Formula source exceeds 64 KiB".into(),
            ));
        }
        let node = if matches!(
            input,
            FormulaInputFormat::AsciiMath | FormulaInputFormat::UnicodeMath
        ) {
            linear_ast(source, input)?
        } else if input == FormulaInputFormat::Omml {
            crate::document_converter::guard_formula_input(source, true)?;
            crate::omml_parser::parse_omml_to_linear_ast(source)
                .map_err(SnipperError::Conversion)?
        } else {
            let latex = if input == FormulaInputFormat::Markdown {
                crate::extrema::standalone_math(source)
                    .map(|(formula, _)| formula)
                    .ok_or_else(|| {
                        SnipperError::Conversion(
                            "Linear targets require exactly one wrapped Markdown formula, without surrounding prose".into(),
                        )
                    })?
            } else {
                Self::convert_formula_fragment(source, input, mode)?
            };
            // The general parser is permissive; do not silently drop unknown source
            // syntax while projecting it into a finite linear grammar.
            crate::omml::validate_omml_latex(&latex).map_err(SnipperError::Conversion)?;
            crate::latex_parser::parse_latex(&latex)
        };
        match output {
            FormulaOutputTarget::AsciiMath => crate::asciimath_pilot::write_asciimath(&node)
                .map_err(|error| match error.kind {
                    crate::asciimath_pilot::AsciiMathErrorKind::LimitExceeded => {
                        SnipperError::LimitExceeded(error.to_string())
                    }
                    _ => SnipperError::Conversion(error.to_string()),
                }),
            FormulaOutputTarget::UnicodeMath => crate::unicodemath_pilot::write_unicodemath(&node)
                .map_err(|error| match error.kind {
                    crate::unicodemath_pilot::UnicodeMathErrorKind::LimitExceeded => {
                        SnipperError::LimitExceeded(error.to_string())
                    }
                    _ => SnipperError::Conversion(error.to_string()),
                }),
            FormulaOutputTarget::Semantic(_) => unreachable!("semantic target dispatched above"),
        }
    }
}

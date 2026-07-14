use latexsnipper_ast::{ExportFormat, FidelityLevel};
use serde::Serialize;

use crate::OutputFormat;

pub const REGISTERED_EXPORT_FORMATS: &[ExportFormat] = &[
    ExportFormat::AstJson,
    ExportFormat::PlainText,
    ExportFormat::Markdown,
    ExportFormat::Latex,
    ExportFormat::Typst,
    ExportFormat::Html,
    ExportFormat::MathML,
    ExportFormat::OMML,
    ExportFormat::Svg,
    ExportFormat::Pdf,
    ExportFormat::Png,
    ExportFormat::Docx,
    ExportFormat::Pptx,
    ExportFormat::Xlsx,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityTarget {
    Native,
    Wasm32UnknownUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetFormatCapability {
    pub format: &'static str,
    pub aliases: &'static [&'static str],
    pub kind: &'static str,
    pub compiled: bool,
    pub available: bool,
    pub target: CapabilityTarget,
    pub feature_requirements: &'static [&'static str],
    pub runtime_requirements: &'static [&'static str],
    pub binary: bool,
    pub mime_type: &'static str,
    pub fidelity: FidelityLevel,
    pub unavailable_reason: Option<&'static str>,
}

pub struct CapabilityRegistry;

impl CapabilityRegistry {
    pub fn for_target(target: CapabilityTarget) -> Vec<TargetFormatCapability> {
        let mut entries = OutputFormat::all()
            .iter()
            .map(|format| TargetFormatCapability {
                format: format.name(),
                aliases: semantic_aliases(*format),
                kind: "semantic",
                compiled: true,
                available: true,
                target,
                feature_requirements: &[],
                runtime_requirements: &[],
                binary: false,
                mime_type: semantic_mime_type(*format),
                fidelity: FidelityLevel::SemanticOnly,
                unavailable_reason: None,
            })
            .collect::<Vec<_>>();

        for &format in REGISTERED_EXPORT_FORMATS {
            let label = export_format_label(format);
            if entries.iter().any(|entry| entry.format == label) {
                continue;
            }
            let native_target = matches!(target, CapabilityTarget::Native);
            let compiled = cfg!(feature = "native") && native_target;
            entries.push(TargetFormatCapability {
                format: label,
                aliases: export_format_aliases(format),
                kind: "visual-or-package",
                compiled,
                available: compiled,
                target,
                feature_requirements: &["native"],
                runtime_requirements: export_runtime_requirements(format),
                binary: export_format_is_binary(format),
                mime_type: export_mime_type(format),
                fidelity: export_fidelity(format),
                unavailable_reason: (!compiled)
                    .then_some("native exporter is not compiled for wasm32-unknown-unknown"),
            });
        }
        entries
    }

    pub fn resolve_export(label: &str) -> Option<ExportFormat> {
        let normalized = label.trim().to_ascii_lowercase();
        REGISTERED_EXPORT_FORMATS.iter().copied().find(|format| {
            export_format_label(*format) == normalized
                || export_format_aliases(*format).contains(&normalized.as_str())
        })
    }
}

pub const fn semantic_aliases(format: OutputFormat) -> &'static [&'static str] {
    match format {
        OutputFormat::Latex => &["tex"],
        OutputFormat::LatexDisplay => &["display"],
        OutputFormat::LatexEquation => &["equation", "eqn"],
        OutputFormat::Typst => &["typ"],
        OutputFormat::MarkdownInline => &["md_inline"],
        OutputFormat::MarkdownBlock => &["markdown", "md"],
        OutputFormat::MathML => &["mml"],
        OutputFormat::OMML | OutputFormat::Html => &[],
    }
}

pub const fn semantic_mime_type(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Latex | OutputFormat::LatexDisplay | OutputFormat::LatexEquation => {
            "application/x-tex"
        }
        OutputFormat::Typst => "text/x-typst",
        OutputFormat::MarkdownInline | OutputFormat::MarkdownBlock => "text/markdown",
        OutputFormat::MathML => "application/mathml+xml",
        OutputFormat::OMML => "application/xml",
        OutputFormat::Html => "text/html",
    }
}

pub const fn export_format_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::AstJson => "json",
        ExportFormat::PlainText => "text",
        ExportFormat::Markdown => "markdown",
        ExportFormat::Latex => "latex",
        ExportFormat::Typst => "typst",
        ExportFormat::Html => "html",
        ExportFormat::MathML => "mathml",
        ExportFormat::OMML => "omml",
        ExportFormat::Svg => "svg",
        ExportFormat::Pdf => "pdf",
        ExportFormat::Png => "png",
        ExportFormat::Docx => "docx",
        ExportFormat::Pptx => "pptx",
        ExportFormat::Xlsx => "xlsx",
        _ => "unregistered",
    }
}

pub const fn export_format_aliases(format: ExportFormat) -> &'static [&'static str] {
    match format {
        ExportFormat::AstJson => &["ast"],
        ExportFormat::PlainText => &["txt"],
        ExportFormat::Markdown => &["md"],
        ExportFormat::Latex => &["tex"],
        ExportFormat::Typst => &["typ"],
        ExportFormat::Html => &["htm"],
        ExportFormat::MathML => &["mml"],
        _ => &[],
    }
}

pub const fn export_format_is_binary(format: ExportFormat) -> bool {
    matches!(
        format,
        ExportFormat::Pdf
            | ExportFormat::Png
            | ExportFormat::Docx
            | ExportFormat::Pptx
            | ExportFormat::Xlsx
    )
}

pub const fn export_mime_type(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::AstJson => "application/json",
        ExportFormat::PlainText => "text/plain",
        ExportFormat::Markdown => "text/markdown",
        ExportFormat::Latex => "application/x-tex",
        ExportFormat::Typst => "text/x-typst",
        ExportFormat::Html => "text/html",
        ExportFormat::MathML => "application/mathml+xml",
        ExportFormat::OMML => "application/xml",
        ExportFormat::Svg => "image/svg+xml",
        ExportFormat::Pdf => "application/pdf",
        ExportFormat::Png => "image/png",
        ExportFormat::Docx => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        ExportFormat::Pptx => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        }
        ExportFormat::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

const fn export_runtime_requirements(format: ExportFormat) -> &'static [&'static str] {
    match format {
        ExportFormat::Svg | ExportFormat::Pdf | ExportFormat::Png => &["native-renderer"],
        ExportFormat::Docx | ExportFormat::Pptx | ExportFormat::Xlsx => &["zip-package-writer"],
        _ => &[],
    }
}

const fn export_fidelity(format: ExportFormat) -> FidelityLevel {
    match format {
        ExportFormat::AstJson => FidelityLevel::Lossless,
        ExportFormat::Svg | ExportFormat::Pdf | ExportFormat::Png => FidelityLevel::VisualOnly,
        ExportFormat::Docx | ExportFormat::Pptx | ExportFormat::Xlsx => FidelityLevel::BestEffort,
        _ => FidelityLevel::SemanticOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_projection_is_complete_and_native_formats_are_unavailable() {
        let entries = CapabilityRegistry::for_target(CapabilityTarget::Wasm32UnknownUnknown);
        for format in OutputFormat::all() {
            let entry = entries
                .iter()
                .find(|entry| entry.format == format.name())
                .unwrap();
            assert!(entry.compiled && entry.available);
            assert_eq!(entry.mime_type, semantic_mime_type(*format));
        }
        for label in ["svg", "png", "pdf", "docx", "pptx", "xlsx"] {
            let entry = entries.iter().find(|entry| entry.format == label).unwrap();
            assert!(!entry.compiled && !entry.available);
            assert!(entry.unavailable_reason.is_some());
        }
    }

    #[test]
    fn every_export_alias_resolves_to_a_registered_format() {
        for &format in REGISTERED_EXPORT_FORMATS {
            assert_eq!(
                CapabilityRegistry::resolve_export(export_format_label(format)),
                Some(format)
            );
            for alias in export_format_aliases(format) {
                assert_eq!(CapabilityRegistry::resolve_export(alias), Some(format));
            }
        }
    }
}

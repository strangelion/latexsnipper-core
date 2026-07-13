use latexsnipper_conversion::OutputFormat;
use latexsnipper_runtime::MemoryModelResolver;
use serde::Serialize;

use crate::api::ApiInfo;
use crate::profiles::{validate_profile, ProfileValidation};
use crate::state::{MemoryLimits, MemoryUsage};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    pub format: String,
    pub kind: &'static str,
    pub mime_type: &'static str,
    pub available: bool,
    pub binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDocument {
    pub api: ApiInfo,
    pub recognition: Vec<ProfileValidation>,
    pub exports: Vec<FormatCapability>,
    pub memory_limits: MemoryLimits,
    pub memory_usage: MemoryUsage,
    pub async_recognition: bool,
    pub progress_callbacks: bool,
    pub cancellation: &'static str,
    pub indexed_db_cache: bool,
    pub incremental_downloads: bool,
}

pub fn collect(
    resolver: &MemoryModelResolver,
    limits: MemoryLimits,
    usage: MemoryUsage,
) -> CapabilityDocument {
    let recognition = [
        "formula",
        "text",
        "mixed",
        "formula_layout",
        "table",
        "handwriting",
    ]
    .into_iter()
    .filter_map(|profile| validate_profile(resolver, profile).ok())
    .collect();

    let mut exports: Vec<_> = OutputFormat::all()
        .iter()
        .map(|format| FormatCapability {
            format: format.name().to_string(),
            kind: "semantic",
            mime_type: mime_type(format.name()),
            available: true,
            binary: false,
            reason: None,
        })
        .collect();

    exports.extend(
        [
            ("svg", "image/svg+xml"),
            ("png", "image/png"),
            ("pdf", "application/pdf"),
            (
                "docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            (
                "pptx",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            ),
            (
                "xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
        ]
        .into_iter()
        .map(|(format, mime_type)| FormatCapability {
            format: format.to_string(),
            kind: "visual-or-package",
            mime_type,
            available: false,
            binary: format != "svg",
            reason: Some("native exporter is intentionally excluded from the WASM build"),
        }),
    );

    CapabilityDocument {
        api: ApiInfo::current(),
        recognition,
        exports,
        memory_limits: limits,
        memory_usage: usage,
        async_recognition: true,
        progress_callbacks: true,
        cancellation: "cooperative-stage-boundary",
        indexed_db_cache: false,
        incremental_downloads: false,
    }
}

pub fn mime_type(format: &str) -> &'static str {
    match format {
        "latex" | "latex_display" | "latex_equation" => "application/x-latex",
        "typst" => "text/x-typst",
        "markdown_inline" | "markdown_block" => "text/markdown",
        "mathml" => "application/mathml+xml",
        "omml" => "application/xml",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}

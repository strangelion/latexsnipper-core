use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{DrawingPackageProfile, DrawingSourceLanguage};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableIdentity {
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
}

impl ExecutableIdentity {
    pub fn is_strong(&self) -> bool {
        self.path.is_absolute()
            && !self.version.trim().is_empty()
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingSecurityPolicy {
    pub allow_shell_escape: bool,
    pub allow_network: bool,
    pub allow_absolute_paths: bool,
    pub allow_parent_path: bool,
    pub allow_external_processes: bool,
    pub allowed_source_languages: BTreeSet<DrawingSourceLanguage>,
    pub allowed_package_profiles: BTreeSet<DrawingPackageProfile>,
    pub allowed_resource_roots: Vec<PathBuf>,
    pub allowed_executables: BTreeMap<String, ExecutableIdentity>,
    #[serde(default)]
    pub available_graphviz_outputs: BTreeSet<String>,
    pub max_source_bytes: u64,
    pub max_resource_bytes: u64,
    pub max_resource_count: usize,
    pub max_ast_nodes: usize,
    pub max_svg_elements: usize,
    pub max_svg_path_commands: usize,
    pub max_output_bytes: u64,
    pub max_generated_files: usize,
    pub timeout_ms: u64,
    pub memory_limit_bytes: u64,
}

impl Default for DrawingSecurityPolicy {
    fn default() -> Self {
        Self {
            allow_shell_escape: false,
            allow_network: false,
            allow_absolute_paths: false,
            allow_parent_path: false,
            allow_external_processes: false,
            allowed_source_languages: BTreeSet::from([
                DrawingSourceLanguage::Tikz,
                DrawingSourceLanguage::Mermaid,
                DrawingSourceLanguage::GraphvizDot,
                DrawingSourceLanguage::SvgSource,
                DrawingSourceLanguage::DrawingJson,
            ]),
            allowed_package_profiles: BTreeSet::from([
                DrawingPackageProfile::BaseTikz,
                DrawingPackageProfile::PgfPlots,
                DrawingPackageProfile::CircuitTikz,
                DrawingPackageProfile::TikzCd,
                DrawingPackageProfile::Forest,
            ]),
            allowed_resource_roots: Vec::new(),
            allowed_executables: BTreeMap::new(),
            available_graphviz_outputs: BTreeSet::new(),
            max_source_bytes: 1024 * 1024,
            max_resource_bytes: 64 * 1024 * 1024,
            max_resource_count: 128,
            max_ast_nodes: 100_000,
            max_svg_elements: 100_000,
            max_svg_path_commands: 1_000_000,
            max_output_bytes: 128 * 1024 * 1024,
            max_generated_files: 128,
            timeout_ms: 30_000,
            memory_limit_bytes: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DrawingSecurityError {
    #[error("DRAWING_LANGUAGE_NOT_ALLOWED: {0}")]
    LanguageNotAllowed(String),
    #[error("DRAWING_ADAPTER_UNAVAILABLE: {0}")]
    AdapterUnavailable(String),
    #[error("DRAWING_EXECUTABLE_HASH_MISMATCH: {0}")]
    ExecutableHashMismatch(String),
    #[error("DRAWING_PACKAGE_LOCK_MISSING: {0}")]
    PackageLockMissing(String),
    #[error("DRAWING_REMOTE_INCLUDE_FORBIDDEN: {0}")]
    RemoteIncludeForbidden(String),
    #[error("DRAWING_LOCAL_INCLUDE_FORBIDDEN: {0}")]
    LocalIncludeForbidden(String),
    #[error("DRAWING_SVG_SCRIPT_FORBIDDEN: {0}")]
    SvgScriptForbidden(String),
    #[error("DRAWING_SVG_EXTERNAL_REFERENCE_FORBIDDEN: {0}")]
    SvgExternalReferenceForbidden(String),
    #[error("DRAWING_SVG_COMPLEXITY_LIMIT: {0}")]
    SvgComplexityLimit(String),
    #[error("DRAWING_GRAPHVIZ_PLUGIN_UNAVAILABLE: {0}")]
    GraphvizPluginUnavailable(String),
    #[error("DRAWING_PLANTUML_SECURITY_PROFILE_INVALID: {0}")]
    PlantUmlSecurityProfileInvalid(String),
    #[error("DRAWING_ASYMPTOTE_REMOTE_RENDER_FORBIDDEN: {0}")]
    AsymptoteRemoteRenderForbidden(String),
    #[error("DRAWING_PSTRICKS_DISABLED: {0}")]
    PstricksDisabled(String),
    #[error("DRAWING_SOURCE_LIMIT: {0}")]
    SourceLimit(String),
}

impl DrawingSecurityError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::LanguageNotAllowed(_) => "DRAWING_LANGUAGE_NOT_ALLOWED",
            Self::AdapterUnavailable(_) => "DRAWING_ADAPTER_UNAVAILABLE",
            Self::ExecutableHashMismatch(_) => "DRAWING_EXECUTABLE_HASH_MISMATCH",
            Self::PackageLockMissing(_) => "DRAWING_PACKAGE_LOCK_MISSING",
            Self::RemoteIncludeForbidden(_) => "DRAWING_REMOTE_INCLUDE_FORBIDDEN",
            Self::LocalIncludeForbidden(_) => "DRAWING_LOCAL_INCLUDE_FORBIDDEN",
            Self::SvgScriptForbidden(_) => "DRAWING_SVG_SCRIPT_FORBIDDEN",
            Self::SvgExternalReferenceForbidden(_) => "DRAWING_SVG_EXTERNAL_REFERENCE_FORBIDDEN",
            Self::SvgComplexityLimit(_) => "DRAWING_SVG_COMPLEXITY_LIMIT",
            Self::GraphvizPluginUnavailable(_) => "DRAWING_GRAPHVIZ_PLUGIN_UNAVAILABLE",
            Self::PlantUmlSecurityProfileInvalid(_) => "DRAWING_PLANTUML_SECURITY_PROFILE_INVALID",
            Self::AsymptoteRemoteRenderForbidden(_) => "DRAWING_ASYMPTOTE_REMOTE_RENDER_FORBIDDEN",
            Self::PstricksDisabled(_) => "DRAWING_PSTRICKS_DISABLED",
            Self::SourceLimit(_) => "DRAWING_SOURCE_LIMIT",
        }
    }
}

pub(crate) fn validate_language_and_source(
    language: DrawingSourceLanguage,
    source: &str,
    policy: &DrawingSecurityPolicy,
) -> Result<(), DrawingSecurityError> {
    if !policy.allowed_source_languages.contains(&language) {
        return Err(if language == DrawingSourceLanguage::Pstricks {
            DrawingSecurityError::PstricksDisabled("PSTricks is blocked by default".to_owned())
        } else {
            DrawingSecurityError::LanguageNotAllowed(format!("{language:?}"))
        });
    }
    if source.len() as u64 > policy.max_source_bytes {
        return Err(DrawingSecurityError::SourceLimit(format!(
            "{} bytes exceed {}",
            source.len(),
            policy.max_source_bytes
        )));
    }
    let lower = source.to_ascii_lowercase();
    if !policy.allow_network
        && ["http://", "https://", "ftp://", "!includeurl"]
            .iter()
            .any(|token| lower.contains(token))
    {
        return Err(DrawingSecurityError::RemoteIncludeForbidden(
            "network references are disabled".to_owned(),
        ));
    }
    if !policy.allow_shell_escape
        && ["\\write18", "\\immediate\\write18", "runsystem("]
            .iter()
            .any(|token| lower.contains(token))
    {
        return Err(DrawingSecurityError::LocalIncludeForbidden(
            "shell escape is disabled".to_owned(),
        ));
    }
    if language == DrawingSourceLanguage::PlantUml
        && [
            "!include ",
            "!include_once",
            "!import",
            "%getenv",
            "%file_exists",
        ]
        .iter()
        .any(|token| lower.contains(token))
    {
        return Err(DrawingSecurityError::PlantUmlSecurityProfileInvalid(
            "PlantUML include/environment access is forbidden".to_owned(),
        ));
    }
    if language == DrawingSourceLanguage::Mermaid
        && ["click ", "<script", "<iframe", "javascript:"]
            .iter()
            .any(|token| lower.contains(token))
    {
        return Err(DrawingSecurityError::RemoteIncludeForbidden(
            "Mermaid strict mode forbids click callbacks and arbitrary HTML".to_owned(),
        ));
    }
    if language == DrawingSourceLanguage::Asymptote
        && !policy.allow_network
        && lower.contains("remoterender")
    {
        return Err(DrawingSecurityError::AsymptoteRemoteRenderForbidden(
            "remote Asymptote rendering is disabled".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_defaults_block_shell_network_and_experimental_languages() {
        let policy = DrawingSecurityPolicy::default();
        assert_eq!(
            validate_language_and_source(
                DrawingSourceLanguage::Tikz,
                "\\immediate\\write18{whoami}",
                &policy,
            )
            .unwrap_err()
            .code(),
            "DRAWING_LOCAL_INCLUDE_FORBIDDEN"
        );
        assert_eq!(
            validate_language_and_source(
                DrawingSourceLanguage::Mermaid,
                "click A https://example.invalid",
                &policy,
            )
            .unwrap_err()
            .code(),
            "DRAWING_REMOTE_INCLUDE_FORBIDDEN"
        );
        assert_eq!(
            validate_language_and_source(DrawingSourceLanguage::PlantUml, "@startuml", &policy)
                .unwrap_err()
                .code(),
            "DRAWING_LANGUAGE_NOT_ALLOWED"
        );
        assert_eq!(
            validate_language_and_source(DrawingSourceLanguage::Pstricks, "x", &policy)
                .unwrap_err()
                .code(),
            "DRAWING_PSTRICKS_DISABLED"
        );

        let mut experimental = policy;
        experimental
            .allowed_source_languages
            .insert(DrawingSourceLanguage::PlantUml);
        experimental
            .allowed_source_languages
            .insert(DrawingSourceLanguage::Asymptote);
        assert_eq!(
            validate_language_and_source(
                DrawingSourceLanguage::PlantUml,
                "!include /private/diagram.puml",
                &experimental,
            )
            .unwrap_err()
            .code(),
            "DRAWING_PLANTUML_SECURITY_PROFILE_INVALID"
        );
        assert_eq!(
            validate_language_and_source(
                DrawingSourceLanguage::Asymptote,
                "remoteRender(\"scene\")",
                &experimental,
            )
            .unwrap_err()
            .code(),
            "DRAWING_ASYMPTOTE_REMOTE_RENDER_FORBIDDEN"
        );
    }
}

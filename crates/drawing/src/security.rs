use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DrawingCompileRequest, DrawingDocument, DrawingPackageProfile, DrawingSourceLanguage};

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

    pub fn verify_file_hash(&self) -> Result<(), DrawingSecurityError> {
        if !self.is_strong() {
            return Err(DrawingSecurityError::ExecutableHashMismatch(
                self.path.display().to_string(),
            ));
        }
        let actual = sha256_file(&self.path).map_err(|error| {
            DrawingSecurityError::ExecutableHashMismatch(format!(
                "{} cannot be read: {error}",
                self.path.display()
            ))
        })?;
        if !actual.eq_ignore_ascii_case(&self.sha256) {
            return Err(DrawingSecurityError::ExecutableHashMismatch(format!(
                "{} expected {}, got {actual}",
                self.path.display(),
                self.sha256
            )));
        }
        Ok(())
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
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
    #[error("DRAWING_PACKAGE_PROFILE_NOT_ALLOWED: {0}")]
    PackageProfileNotAllowed(String),
    #[error("DRAWING_RESOURCE_LIMIT: {0}")]
    ResourceLimit(String),
    #[error("DRAWING_RESOURCE_PATH_FORBIDDEN: {0}")]
    ResourcePathForbidden(String),
    #[error("DRAWING_RESOURCE_HASH_MISMATCH: {0}")]
    ResourceHashMismatch(String),
    #[error("DRAWING_AST_LIMIT: {0}")]
    AstLimit(String),
    #[error("DRAWING_GENERATED_FILE_LIMIT: {0}")]
    GeneratedFileLimit(String),
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
            Self::PackageProfileNotAllowed(_) => "DRAWING_PACKAGE_PROFILE_NOT_ALLOWED",
            Self::ResourceLimit(_) => "DRAWING_RESOURCE_LIMIT",
            Self::ResourcePathForbidden(_) => "DRAWING_RESOURCE_PATH_FORBIDDEN",
            Self::ResourceHashMismatch(_) => "DRAWING_RESOURCE_HASH_MISMATCH",
            Self::AstLimit(_) => "DRAWING_AST_LIMIT",
            Self::GeneratedFileLimit(_) => "DRAWING_GENERATED_FILE_LIMIT",
        }
    }
}

pub(crate) fn validate_document_security(
    document: &DrawingDocument,
    request: &DrawingCompileRequest,
    policy: &DrawingSecurityPolicy,
) -> Result<(), DrawingSecurityError> {
    if document
        .package_profiles
        .iter()
        .any(|profile| !policy.allowed_package_profiles.contains(profile))
    {
        return Err(DrawingSecurityError::PackageProfileNotAllowed(format!(
            "{:?}",
            document.package_profiles
        )));
    }
    let ast_nodes = document
        .objects
        .len()
        .saturating_add(document.raw_nodes.len())
        .saturating_add(document.layers.len())
        .saturating_add(document.datasets.len());
    if ast_nodes > policy.max_ast_nodes {
        return Err(DrawingSecurityError::AstLimit(format!(
            "{ast_nodes} nodes exceed {}",
            policy.max_ast_nodes
        )));
    }
    if policy.max_generated_files == 0 {
        return Err(DrawingSecurityError::GeneratedFileLimit(
            "at least one output file must be permitted".to_owned(),
        ));
    }
    if document.resources.len() > policy.max_resource_count {
        return Err(DrawingSecurityError::ResourceLimit(format!(
            "{} resources exceed {}",
            document.resources.len(),
            policy.max_resource_count
        )));
    }
    let declared_bytes = document
        .resources
        .iter()
        .try_fold(0u64, |total, resource| {
            total.checked_add(resource.size_bytes).ok_or_else(|| {
                DrawingSecurityError::ResourceLimit("resource byte count overflow".to_owned())
            })
        })?;
    if declared_bytes > policy.max_resource_bytes {
        return Err(DrawingSecurityError::ResourceLimit(format!(
            "{declared_bytes} bytes exceed {}",
            policy.max_resource_bytes
        )));
    }
    let mut expected_hashes = document
        .resources
        .iter()
        .map(|resource| resource.sha256.to_ascii_lowercase())
        .collect::<Vec<_>>();
    expected_hashes.sort();
    let mut requested_hashes = request
        .resource_sha256
        .iter()
        .map(|digest| digest.to_ascii_lowercase())
        .collect::<Vec<_>>();
    requested_hashes.sort();
    if expected_hashes != requested_hashes {
        return Err(DrawingSecurityError::ResourceHashMismatch(
            "compile request resource hashes differ from the document".to_owned(),
        ));
    }
    for resource in &document.resources {
        validate_resource(resource, policy)?;
    }
    Ok(())
}

fn validate_resource(
    resource: &crate::DrawingResource,
    policy: &DrawingSecurityPolicy,
) -> Result<(), DrawingSecurityError> {
    resolve_resource(resource, policy).map(|_| ())
}

pub(crate) fn resolve_resource(
    resource: &crate::DrawingResource,
    policy: &DrawingSecurityPolicy,
) -> Result<PathBuf, DrawingSecurityError> {
    let relative = Path::new(&resource.relative_path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(DrawingSecurityError::ResourcePathForbidden(
            resource.relative_path.clone(),
        ));
    }
    let mut found = None;
    for root in &policy.allowed_resource_roots {
        let Ok(canonical_root) = root.canonicalize() else {
            continue;
        };
        let Ok(candidate) = canonical_root.join(relative).canonicalize() else {
            continue;
        };
        if candidate.starts_with(&canonical_root) && candidate.is_file() {
            found = Some(candidate);
            break;
        }
    }
    let path = found.ok_or_else(|| {
        DrawingSecurityError::ResourcePathForbidden(resource.relative_path.clone())
    })?;
    let metadata = path.metadata().map_err(|error| {
        DrawingSecurityError::ResourcePathForbidden(format!("{}: {error}", path.display()))
    })?;
    if metadata.len() != resource.size_bytes || metadata.len() > policy.max_resource_bytes {
        return Err(DrawingSecurityError::ResourceLimit(format!(
            "{} has unexpected size {}",
            resource.relative_path,
            metadata.len()
        )));
    }
    let actual = sha256_file(&path).map_err(|error| {
        DrawingSecurityError::ResourceHashMismatch(format!("{}: {error}", path.display()))
    })?;
    if !actual.eq_ignore_ascii_case(&resource.sha256) {
        return Err(DrawingSecurityError::ResourceHashMismatch(
            resource.relative_path.clone(),
        ));
    }
    Ok(path)
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
    // XML namespace identifiers are stable vocabulary identifiers, not fetched
    // resources. Keep them available to standards-compliant SVG while the SVG
    // sanitizer separately rejects external href/src/style URLs.
    let network_scan = if language == DrawingSourceLanguage::SvgSource {
        lower
            .replace("http://www.w3.org/2000/svg", "")
            .replace("http://www.w3.org/1999/xlink", "")
    } else {
        lower.clone()
    };
    if !policy.allow_network
        && ["http://", "https://", "ftp://", "!includeurl"]
            .iter()
            .any(|token| network_scan.contains(token))
    {
        return Err(DrawingSecurityError::RemoteIncludeForbidden(
            "network references are disabled".to_owned(),
        ));
    }
    if !policy.allow_shell_escape
        && [
            "\\write18",
            "\\immediate\\write18",
            "runsystem(",
            "\\input",
            "\\include",
            "\\openin",
            "\\read",
            "file://",
        ]
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

    #[test]
    fn svg_namespace_identifiers_are_not_misclassified_as_network_access() {
        let policy = DrawingSecurityPolicy::default();
        validate_language_and_source(
            DrawingSourceLanguage::SvgSource,
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"#,
            &policy,
        )
        .unwrap();
        assert_eq!(
            validate_language_and_source(
                DrawingSourceLanguage::SvgSource,
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><image href="https://example.invalid/a.png"/></svg>"#,
                &policy,
            )
            .unwrap_err()
            .code(),
            "DRAWING_REMOTE_INCLUDE_FORBIDDEN"
        );
    }

    #[test]
    fn executable_identity_hashes_the_actual_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("renderer.bin");
        std::fs::write(&path, b"renderer-v1").unwrap();
        let identity = ExecutableIdentity {
            path: path.canonicalize().unwrap(),
            version: "1".to_owned(),
            sha256: format!("{:x}", Sha256::digest(b"renderer-v1")),
        };
        identity.verify_file_hash().unwrap();
        std::fs::write(&path, b"tampered").unwrap();
        assert_eq!(
            identity.verify_file_hash().unwrap_err().code(),
            "DRAWING_EXECUTABLE_HASH_MISMATCH"
        );
    }

    #[test]
    fn file_access_primitives_are_blocked_before_compilation() {
        let policy = DrawingSecurityPolicy::default();
        for source in [
            "\\input{secret}",
            "\\include{secret}",
            "\\openin1=secret",
            "\\read1 to \\value",
            "file:///private/data",
        ] {
            assert_eq!(
                validate_language_and_source(DrawingSourceLanguage::Tikz, source, &policy)
                    .unwrap_err()
                    .code(),
                "DRAWING_LOCAL_INCLUDE_FORBIDDEN"
            );
        }
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    validate_document_security, validate_language_and_source, DrawingCompatibility,
    DrawingDocument, DrawingOutputFormat, DrawingPackageProfile, DrawingSecurityError,
    DrawingSecurityPolicy, DrawingSource, DrawingSourceLanguage,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "contract-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DrawingAdapterCapabilities {
    pub source_editing: bool,
    pub structured_parse: bool,
    pub structured_emit: bool,
    pub lossless_round_trip: bool,
    pub native_compile: bool,
    pub wasm_compile: bool,
    pub svg_output: bool,
    pub pdf_output: bool,
    pub png_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingInspection {
    pub language: DrawingSourceLanguage,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub compatibility: DrawingCompatibility,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingParseResult {
    pub document: DrawingDocument,
    pub preserved_source: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingCompileRequest {
    pub output: DrawingOutputFormat,
    pub renderer_id: String,
    pub package_lock_sha256: Option<String>,
    #[serde(default)]
    pub resource_sha256: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingCompilePlan {
    pub adapter: String,
    pub executable: Option<String>,
    pub arguments: Vec<String>,
    pub output: DrawingOutputFormat,
    pub network_allowed: bool,
    pub timeout_ms: u64,
    pub memory_limit_bytes: u64,
    pub cache_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DrawingAdapterError {
    #[error(transparent)]
    Security(#[from] DrawingSecurityError),
    #[error("DRAWING_ADAPTER_UNAVAILABLE: {0}")]
    Unavailable(String),
    #[error("DRAWING_OUTPUT_UNSUPPORTED: {0}")]
    UnsupportedOutput(String),
    #[error("DRAWING_SERIALIZATION_FAILED: {0}")]
    Serialization(String),
}

pub trait DrawingSourceAdapter: Send + Sync {
    fn language(&self) -> DrawingSourceLanguage;
    fn capabilities(&self) -> DrawingAdapterCapabilities;
    fn inspect(
        &self,
        source: &str,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingInspection, DrawingAdapterError>;
    fn parse(
        &self,
        source: &str,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingParseResult, DrawingAdapterError>;
    fn emit(&self, document: &DrawingDocument) -> Result<DrawingSource, DrawingAdapterError>;
    fn compile_plan(
        &self,
        document: &DrawingDocument,
        request: &DrawingCompileRequest,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingCompilePlan, DrawingAdapterError>;
}

#[derive(Debug, Clone)]
pub struct SourcePreservingAdapter {
    language: DrawingSourceLanguage,
    capabilities: DrawingAdapterCapabilities,
    executable_key: Option<&'static str>,
}

impl SourcePreservingAdapter {
    pub fn for_language(language: DrawingSourceLanguage) -> Self {
        let (capabilities, executable_key) = match language {
            DrawingSourceLanguage::Tikz => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    structured_parse: false,
                    structured_emit: false,
                    lossless_round_trip: false,
                    native_compile: true,
                    svg_output: true,
                    pdf_output: true,
                    png_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("tectonic"),
            ),
            DrawingSourceLanguage::Mermaid => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    native_compile: true,
                    wasm_compile: true,
                    svg_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("mermaid"),
            ),
            DrawingSourceLanguage::GraphvizDot => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    structured_parse: false,
                    native_compile: true,
                    wasm_compile: true,
                    svg_output: true,
                    pdf_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("graphviz"),
            ),
            DrawingSourceLanguage::PlantUml => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    native_compile: true,
                    svg_output: true,
                    png_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("plantuml"),
            ),
            DrawingSourceLanguage::Asymptote => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    native_compile: true,
                    svg_output: true,
                    pdf_output: true,
                    png_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("asymptote"),
            ),
            DrawingSourceLanguage::MetaPost | DrawingSourceLanguage::Pstricks => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    native_compile: true,
                    pdf_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                Some("system-tex"),
            ),
            DrawingSourceLanguage::SvgSource => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    structured_parse: true,
                    structured_emit: true,
                    lossless_round_trip: false,
                    svg_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                None,
            ),
            DrawingSourceLanguage::DrawingJson => (
                DrawingAdapterCapabilities {
                    source_editing: true,
                    structured_parse: true,
                    structured_emit: true,
                    lossless_round_trip: true,
                    svg_output: true,
                    ..DrawingAdapterCapabilities::default()
                },
                None,
            ),
        };
        Self {
            language,
            capabilities,
            executable_key,
        }
    }

    fn supports_output(&self, output: DrawingOutputFormat) -> bool {
        match output {
            DrawingOutputFormat::Svg => self.capabilities.svg_output,
            DrawingOutputFormat::Pdf => self.capabilities.pdf_output,
            DrawingOutputFormat::Png => self.capabilities.png_output,
            DrawingOutputFormat::WebP | DrawingOutputFormat::Eps => false,
        }
    }
}

impl DrawingSourceAdapter for SourcePreservingAdapter {
    fn language(&self) -> DrawingSourceLanguage {
        self.language
    }

    fn capabilities(&self) -> DrawingAdapterCapabilities {
        self.capabilities
    }

    fn inspect(
        &self,
        source: &str,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingInspection, DrawingAdapterError> {
        validate_language_and_source(self.language, source, policy)?;
        Ok(DrawingInspection {
            language: self.language,
            source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
            source_bytes: source.len() as u64,
            compatibility: if self.language == DrawingSourceLanguage::DrawingJson {
                DrawingCompatibility::VisualCompatible
            } else {
                DrawingCompatibility::SourceOnly
            },
            diagnostics: Vec::new(),
        })
    }

    fn parse(
        &self,
        source: &str,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingParseResult, DrawingAdapterError> {
        self.inspect(source, policy)?;
        if self.language == DrawingSourceLanguage::DrawingJson {
            let document: DrawingDocument = serde_json::from_str(source)
                .map_err(|error| DrawingAdapterError::Serialization(error.to_string()))?;
            return Ok(DrawingParseResult {
                document,
                preserved_source: true,
                diagnostics: Vec::new(),
            });
        }
        Ok(DrawingParseResult {
            document: DrawingDocument::source_only(
                format!("{}-source", format!("{:?}", self.language).to_ascii_lowercase()),
                self.language,
                source,
            ),
            preserved_source: true,
            diagnostics: vec![
                "advanced syntax is preserved as RawDrawingNode; no lossless structured emit is claimed"
                    .to_owned(),
            ],
        })
    }

    fn emit(&self, document: &DrawingDocument) -> Result<DrawingSource, DrawingAdapterError> {
        if self.language == DrawingSourceLanguage::DrawingJson {
            return serde_json::to_string(document)
                .map(|text| DrawingSource { text })
                .map_err(|error| DrawingAdapterError::Serialization(error.to_string()));
        }
        if document.source_language != self.language {
            return Err(DrawingAdapterError::Serialization(
                "adapter cannot emit a different source language".to_owned(),
            ));
        }
        Ok(document.source.clone())
    }

    fn compile_plan(
        &self,
        document: &DrawingDocument,
        request: &DrawingCompileRequest,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingCompilePlan, DrawingAdapterError> {
        self.inspect(&document.source.text, policy)?;
        validate_document_security(document, request, policy)?;
        if self.language == DrawingSourceLanguage::Tikz
            && request.package_lock_sha256.as_deref().is_none_or(|digest| {
                digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(DrawingSecurityError::PackageLockMissing(
                "TikZ-family compilation requires a versioned package-lock SHA-256".to_owned(),
            )
            .into());
        }
        if !self.supports_output(request.output) {
            return Err(DrawingAdapterError::UnsupportedOutput(format!(
                "{:?} cannot produce {:?}",
                self.language, request.output
            )));
        }
        let executable = if let Some(key) = self.executable_key {
            if !policy.allow_external_processes {
                return Err(DrawingAdapterError::Unavailable(format!(
                    "'{key}' requires an explicitly enabled local sidecar"
                )));
            }
            let identity = policy.allowed_executables.get(key).ok_or_else(|| {
                DrawingAdapterError::Unavailable(format!(
                    "'{key}' has no pinned executable identity"
                ))
            })?;
            identity.verify_file_hash()?;
            Some(identity.path.to_string_lossy().into_owned())
        } else {
            None
        };
        if self.language == DrawingSourceLanguage::GraphvizDot
            && !policy
                .available_graphviz_outputs
                .contains(output_name(request.output))
        {
            return Err(DrawingSecurityError::GraphvizPluginUnavailable(format!(
                "the probed Graphviz build does not advertise '{}' output",
                output_name(request.output)
            ))
            .into());
        }
        let cache_key = drawing_cache_key(document, request)?;
        Ok(DrawingCompilePlan {
            adapter: format!("{:?}", self.language),
            executable,
            arguments: controlled_arguments(self.language, request.output),
            output: request.output,
            network_allowed: false,
            timeout_ms: policy.timeout_ms,
            memory_limit_bytes: policy.memory_limit_bytes,
            cache_key,
        })
    }
}

fn controlled_arguments(
    language: DrawingSourceLanguage,
    output: DrawingOutputFormat,
) -> Vec<String> {
    match language {
        DrawingSourceLanguage::Mermaid => vec![
            "--securityLevel=strict".to_owned(),
            "--no-sandbox-network".to_owned(),
            format!("--format={}", output_name(output)),
        ],
        DrawingSourceLanguage::PlantUml => vec![
            "-DPLANTUML_SECURITY_PROFILE=SANDBOX".to_owned(),
            format!("-t{}", output_name(output)),
        ],
        DrawingSourceLanguage::GraphvizDot => vec![format!("-T{}", output_name(output))],
        _ => vec![format!("--format={}", output_name(output))],
    }
}

fn output_name(output: DrawingOutputFormat) -> &'static str {
    match output {
        DrawingOutputFormat::Svg => "svg",
        DrawingOutputFormat::Pdf => "pdf",
        DrawingOutputFormat::Png => "png",
        DrawingOutputFormat::WebP => "webp",
        DrawingOutputFormat::Eps => "eps",
    }
}

pub fn drawing_cache_key(
    document: &DrawingDocument,
    request: &DrawingCompileRequest,
) -> Result<String, DrawingAdapterError> {
    let mut resources = request.resource_sha256.clone();
    resources.sort();
    let canonical = serde_json::to_vec(&BTreeMap::from([
        (
            "document",
            serde_json::to_value(document)
                .map_err(|e| DrawingAdapterError::Serialization(e.to_string()))?,
        ),
        ("renderer", serde_json::json!(request.renderer_id)),
        ("output", serde_json::json!(request.output)),
        (
            "packageLock",
            serde_json::json!(request.package_lock_sha256),
        ),
        ("resources", serde_json::json!(resources)),
    ]))
    .map_err(|error| DrawingAdapterError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

pub fn required_profiles_allowed(
    profiles: &[DrawingPackageProfile],
    policy: &DrawingSecurityPolicy,
) -> bool {
    profiles
        .iter()
        .all(|profile| policy.allowed_package_profiles.contains(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_renderer_package_and_resources() {
        let document = DrawingDocument::source_only("d", DrawingSourceLanguage::Tikz, "x");
        let base = DrawingCompileRequest {
            output: DrawingOutputFormat::Svg,
            renderer_id: "tectonic@1".to_owned(),
            package_lock_sha256: Some("a".repeat(64)),
            resource_sha256: vec!["b".repeat(64)],
        };
        let first = drawing_cache_key(&document, &base).unwrap();
        let mut changed = base.clone();
        changed.renderer_id = "tectonic@2".to_owned();
        assert_ne!(first, drawing_cache_key(&document, &changed).unwrap());
        changed = base.clone();
        changed.package_lock_sha256 = Some("c".repeat(64));
        assert_ne!(first, drawing_cache_key(&document, &changed).unwrap());
        changed = base;
        changed.resource_sha256.push("d".repeat(64));
        assert_ne!(first, drawing_cache_key(&document, &changed).unwrap());
    }

    #[test]
    fn absent_sidecar_is_blocked_instead_of_claimed_available() {
        let adapter = SourcePreservingAdapter::for_language(DrawingSourceLanguage::Tikz);
        let document = DrawingDocument::source_only("d", DrawingSourceLanguage::Tikz, "x");
        let error = adapter
            .compile_plan(
                &document,
                &DrawingCompileRequest {
                    output: DrawingOutputFormat::Svg,
                    renderer_id: "tectonic".to_owned(),
                    package_lock_sha256: Some("a".repeat(64)),
                    resource_sha256: Vec::new(),
                },
                &DrawingSecurityPolicy::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("DRAWING_ADAPTER_UNAVAILABLE"));
    }

    #[test]
    fn source_adapters_preserve_advanced_languages_without_lossless_claims() {
        let policy = DrawingSecurityPolicy::default();
        for (language, source) in [
            (DrawingSourceLanguage::Tikz, "\\draw (0,0)--(1,1);"),
            (DrawingSourceLanguage::Mermaid, "graph TD; A-->B"),
            (DrawingSourceLanguage::GraphvizDot, "digraph { A -> B }"),
        ] {
            let adapter = SourcePreservingAdapter::for_language(language);
            let parsed = adapter.parse(source, &policy).unwrap();
            assert!(parsed.preserved_source);
            assert_eq!(parsed.document.source.text, source);
            assert!(!adapter.capabilities().lossless_round_trip);
        }
    }

    #[test]
    fn tikz_and_pgfplots_require_a_versioned_package_lock() {
        let adapter = SourcePreservingAdapter::for_language(DrawingSourceLanguage::Tikz);
        let mut document = DrawingDocument::source_only(
            "plot",
            DrawingSourceLanguage::Tikz,
            "\\begin{axis}\\addplot coordinates {(0,0) (1,1)};\\end{axis}",
        );
        document.package_profiles = vec![DrawingPackageProfile::PgfPlots];
        let policy = DrawingSecurityPolicy {
            allow_external_processes: true,
            ..DrawingSecurityPolicy::default()
        };
        let error = adapter
            .compile_plan(
                &document,
                &DrawingCompileRequest {
                    output: DrawingOutputFormat::Svg,
                    renderer_id: "tectonic@pinned".to_owned(),
                    package_lock_sha256: None,
                    resource_sha256: Vec::new(),
                },
                &policy,
            )
            .unwrap_err();
        assert!(error.to_string().contains("DRAWING_PACKAGE_LOCK_MISSING"));
    }

    #[test]
    fn graphviz_output_requires_a_probed_plugin() {
        let adapter = SourcePreservingAdapter::for_language(DrawingSourceLanguage::GraphvizDot);
        let document = DrawingDocument::source_only(
            "graph",
            DrawingSourceLanguage::GraphvizDot,
            "digraph { A -> B }",
        );
        let mut policy = DrawingSecurityPolicy {
            allow_external_processes: true,
            ..DrawingSecurityPolicy::default()
        };
        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join(if cfg!(windows) { "dot.exe" } else { "dot" });
        std::fs::write(&executable, b"pinned graphviz fixture").unwrap();
        policy.allowed_executables.insert(
            "graphviz".to_owned(),
            crate::ExecutableIdentity {
                path: executable.canonicalize().unwrap(),
                version: "pinned".to_owned(),
                sha256: format!("{:x}", Sha256::digest(b"pinned graphviz fixture")),
            },
        );
        let error = adapter
            .compile_plan(
                &document,
                &DrawingCompileRequest {
                    output: DrawingOutputFormat::Svg,
                    renderer_id: "graphviz@pinned".to_owned(),
                    package_lock_sha256: None,
                    resource_sha256: Vec::new(),
                },
                &policy,
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("DRAWING_GRAPHVIZ_PLUGIN_UNAVAILABLE"));
    }

    #[test]
    fn compile_plan_enforces_package_profile_allowlist() {
        let adapter = SourcePreservingAdapter::for_language(DrawingSourceLanguage::Tikz);
        let mut document = DrawingDocument::source_only("d", DrawingSourceLanguage::Tikz, "x");
        document.package_profiles = vec![DrawingPackageProfile::ChemFig];
        let error = adapter
            .compile_plan(
                &document,
                &DrawingCompileRequest {
                    output: DrawingOutputFormat::Svg,
                    renderer_id: "tectonic".to_owned(),
                    package_lock_sha256: Some("a".repeat(64)),
                    resource_sha256: Vec::new(),
                },
                &DrawingSecurityPolicy::default(),
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("DRAWING_PACKAGE_PROFILE_NOT_ALLOWED"));
    }
}

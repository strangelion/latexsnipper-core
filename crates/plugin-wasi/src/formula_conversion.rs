//! Registered formula routes backed by the existing verified WIT v1 host.
use latexsnipper_ast::{Block, Document, DocumentBuilder, FormulaSource, DOCUMENT_SCHEMA_VERSION};
use latexsnipper_conversion::{
    conversion_registry::{
        ConversionBackendDescriptor, ConversionBackendRoute, ConversionCandidate, ConversionLoss,
        ConversionRegistryError, ConversionRegistryErrorCode, RegisteredConversionRequest,
        RegisteredFormulaBackend, SemanticConversionRegistry, CONVERSION_REGISTRY_VERSION,
    },
    CapabilityTarget, DocumentConverter, FormulaConversionMode, FormulaInputFormat, OutputFormat,
};
use latexsnipper_plugin::{CancellationToken, PluginHook};
use sha2::{Digest, Sha256};

use crate::{
    wit_types, ActivatedRemoteWasiPlugin, ComponentInvocation, ComponentInvocationResult,
    WasiDiagnostic,
};

pub const WASI_DOCUMENT_JSON_MEDIA_TYPE: &str = "application/vnd.latexsnipper.document+json";
const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

enum Operation {
    Import {
        transport: String,
        output: OutputFormat,
    },
    Export {
        transport: String,
        input: FormulaInputFormat,
    },
}

/// Owns an already verified, explicitly enabled activation; never loads arbitrary code.
/// Registry registration itself remains disabled until explicitly enabled separately.
pub struct WasiFormulaBackend {
    plugin: ActivatedRemoteWasiPlugin,
    descriptor: ConversionBackendDescriptor,
    operation: Operation,
    cancellation: CancellationToken,
}

impl WasiFormulaBackend {
    /// Import a declared custom UTF-8 format as a single-formula Document JSON,
    /// then use Core conversion for the selected semantic output.
    pub fn importer(
        plugin: ActivatedRemoteWasiPlugin,
        backend_id: &str,
        input_id: &str,
        transport: &str,
        output: OutputFormat,
    ) -> Result<Self, ConversionRegistryError> {
        validate_declaration(
            &plugin,
            PluginHook::RegisterImporter,
            "importer",
            transport,
            "AST",
        )?;
        Self::create(
            plugin,
            backend_id,
            input_id,
            output.name(),
            Operation::Import {
                transport: transport.into(),
                output,
            },
        )
    }

    /// Supply a real single-formula AST to a declared exporter. No opaque input
    /// string is relabelled as AST and no exporter output is relabelled as OMML.
    pub fn exporter(
        plugin: ActivatedRemoteWasiPlugin,
        backend_id: &str,
        input: FormulaInputFormat,
        output_id: &str,
        transport: &str,
    ) -> Result<Self, ConversionRegistryError> {
        if !matches!(
            input,
            FormulaInputFormat::Latex
                | FormulaInputFormat::Mathml
                | FormulaInputFormat::Omml
                | FormulaInputFormat::Typst
        ) {
            return Err(rejected(
                "exporter input must have an implemented formula parser",
            ));
        }
        validate_declaration(
            &plugin,
            PluginHook::RegisterExporter,
            "exporter",
            "AST",
            transport,
        )?;
        Self::create(
            plugin,
            backend_id,
            input.name(),
            output_id,
            Operation::Export {
                transport: transport.into(),
                input,
            },
        )
    }

    fn create(
        plugin: ActivatedRemoteWasiPlugin,
        id: &str,
        input: &str,
        output: &str,
        operation: Operation,
    ) -> Result<Self, ConversionRegistryError> {
        plugin.ensure_still_enabled().map_err(wasi_failure)?;
        let manifest = plugin.manifest();
        let transport = match &operation {
            Operation::Import { transport, .. } | Operation::Export { transport, .. } => transport,
        };
        if transport.len() > 128 || transport.is_empty() || transport.chars().any(char::is_control)
        {
            return Err(rejected("invalid WIT transport format"));
        }
        let limits = plugin.resource_limits();
        let configuration = serde_json::to_vec(&(
            manifest,
            plugin.component_sha256(),
            limits,
            input,
            output,
            transport,
            CONVERSION_REGISTRY_VERSION,
        ))
        .map_err(|error| rejected(error.to_string()))?;
        let mut limitations = vec![
            "WIT v1 standalone single-formula AST only; no assets or document write authority"
                .into(),
            "best-effort output is not a visual, semantic or editable fidelity guarantee".into(),
        ];
        for capability in &manifest.format_capabilities {
            let pair = match &operation {
                Operation::Import { .. } => (transport.as_str(), "AST"),
                Operation::Export { .. } => ("AST", transport.as_str()),
            };
            if capability.available
                && capability.supports_formula
                && capability.input.as_deref() == Some(pair.0)
                && capability.output.as_deref() == Some(pair.1)
            {
                limitations.extend(capability.notes.iter().cloned());
                limitations.extend(
                    capability
                        .known_loss
                        .iter()
                        .map(|loss| format!("declared-loss:{loss:?}")),
                );
            }
        }
        let descriptor = ConversionBackendDescriptor {
            schema_version: CONVERSION_REGISTRY_VERSION,
            id: id.into(),
            version: manifest.version.clone(),
            configuration_sha256: format!("{:x}", Sha256::digest(configuration)),
            license: manifest
                .license
                .clone()
                .ok_or_else(|| rejected("verified backend has no license"))?,
            limitations,
            routes: vec![ConversionBackendRoute {
                input: input.into(),
                output: output.into(),
                mode: FormulaConversionMode::BestEffort,
                target: CapabilityTarget::Native,
            }],
            max_input_bytes: limits.input_bytes.min(64 * 1024),
            max_output_bytes: limits.output_bytes.min(MAX_DOCUMENT_BYTES),
        };
        Ok(Self {
            plugin,
            descriptor,
            operation,
            cancellation: CancellationToken::default(),
        })
    }

    pub fn descriptor(&self) -> &ConversionBackendDescriptor {
        &self.descriptor
    }
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }
    pub fn register(
        self,
        registry: &mut SemanticConversionRegistry,
    ) -> Result<(), ConversionRegistryError> {
        registry.register_trusted_backend(self.descriptor.clone(), self)
    }
}

impl RegisteredFormulaBackend for WasiFormulaBackend {
    fn check_ready(&self) -> Result<(), ConversionRegistryError> {
        if self.cancellation.is_cancelled() {
            return Err(ConversionRegistryError::new(
                ConversionRegistryErrorCode::BackendFailed,
                "PLUGIN_WASI_CANCELLED: cancellation requested",
            ));
        }
        self.plugin.ensure_still_enabled().map_err(wasi_failure)
    }

    fn convert(
        &self,
        request: &RegisteredConversionRequest,
    ) -> Result<ConversionCandidate, ConversionRegistryError> {
        self.check_ready()?;
        let route = &self.descriptor.routes[0];
        if request.schema_version != CONVERSION_REGISTRY_VERSION
            || request.input != route.input
            || request.output != route.output
            || request.mode != route.mode
            || request.backend.as_deref() != Some(self.descriptor.id.as_str())
        {
            return Err(rejected(
                "request does not match the verified WASI formula route",
            ));
        }
        let mut losses = vec![ConversionLoss { code: "WASI_FORMULA_RECONSTRUCTION".into(), message: "Single-formula AST projection; original spelling, document metadata and full layout are not preserved".into() }];
        let content = match &self.operation {
            Operation::Import { transport, output } => {
                let result = self
                    .plugin
                    .execute(
                        ComponentInvocation::Import(wit_types::ImportRequest {
                            format: transport.clone(),
                            payload: request.content.as_bytes().to_vec(),
                        }),
                        self.cancellation.clone(),
                    )
                    .map_err(wasi_failure)?;
                let ComponentInvocationResult::Document(document) = result else {
                    return Err(rejected("WASI importer did not return a document"));
                };
                if document.media_type != WASI_DOCUMENT_JSON_MEDIA_TYPE
                    || document.schema_version != DOCUMENT_SCHEMA_VERSION
                {
                    return Err(rejected(
                        "WASI importer result is not the declared Document JSON representation",
                    ));
                }
                let (source, input) = import_formula_source(&document.payload)?;
                DocumentConverter::convert_formula_string(
                    &source,
                    input,
                    *output,
                    FormulaConversionMode::BestEffort,
                )
                .map_err(|error| rejected(error.to_string()))?
            }
            Operation::Export { transport, input } => {
                let source = DocumentConverter::convert_formula_fragment(
                    &request.content,
                    *input,
                    FormulaConversionMode::BestEffort,
                )
                .map_err(|error| rejected(error.to_string()))?;
                let document = DocumentBuilder::new()
                    .page(0.0, 0.0, |page| {
                        page.display_formula(source);
                    })
                    .build();
                let payload =
                    serde_json::to_vec(&document).map_err(|error| rejected(error.to_string()))?;
                if payload.len() > MAX_DOCUMENT_BYTES {
                    return Err(rejected("export Document JSON exceeds bridge budget"));
                }
                let result = self
                    .plugin
                    .execute(
                        ComponentInvocation::Export(wit_types::ExportRequest {
                            format: transport.clone(),
                            document: wit_types::Document {
                                schema_version: DOCUMENT_SCHEMA_VERSION.into(),
                                media_type: WASI_DOCUMENT_JSON_MEDIA_TYPE.into(),
                                payload,
                            },
                        }),
                        self.cancellation.clone(),
                    )
                    .map_err(wasi_failure)?;
                let ComponentInvocationResult::Export(result) = result else {
                    return Err(rejected("WASI exporter did not return an export result"));
                };
                if result.media_type != *transport {
                    return Err(rejected(
                        "WASI exporter returned a mismatched transport format",
                    ));
                }
                if result.diagnostics.iter().any(|diagnostic| {
                    matches!(diagnostic.severity, wit_types::DiagnosticSeverity::Error)
                }) {
                    return Err(rejected(
                        "WASI exporter returned error diagnostics with its candidate",
                    ));
                }
                losses.extend(result.diagnostics.iter().map(|diagnostic| ConversionLoss {
                    code: format!("WASI_GUEST_{:?}", diagnostic.code),
                    message: diagnostic.message.clone(),
                }));
                String::from_utf8(result.payload)
                    .map_err(|_| rejected("WASI formula exporter returned non-UTF-8 bytes"))?
            }
        };
        Ok(ConversionCandidate { content, losses })
    }
}

fn validate_declaration(
    plugin: &ActivatedRemoteWasiPlugin,
    hook: PluginHook,
    capability: &str,
    input: &str,
    output: &str,
) -> Result<(), ConversionRegistryError> {
    let manifest = plugin.manifest();
    let grant = match hook {
        PluginHook::RegisterImporter => manifest.permissions.registrations.importers,
        _ => manifest.permissions.registrations.exporters,
    };
    if !grant
        || !manifest.hooks.contains(&hook)
        || !manifest
            .capabilities
            .iter()
            .any(|entry| entry == capability)
        || !manifest.format_capabilities.iter().any(|entry| {
            entry.available
                && entry.supports_formula
                && entry.input.as_deref() == Some(input)
                && entry.output.as_deref() == Some(output)
        })
    {
        return Err(rejected(
            "WASI formula route is not declared by the verified hooks, grants and format pair",
        ));
    }
    Ok(())
}

fn import_formula_source(
    payload: &[u8],
) -> Result<(String, FormulaInputFormat), ConversionRegistryError> {
    if payload.len() > MAX_DOCUMENT_BYTES {
        return Err(rejected("import Document JSON exceeds bridge budget"));
    }
    let document: Document =
        serde_json::from_slice(payload).map_err(|error| rejected(error.to_string()))?;
    let shape: serde_json::Value =
        serde_json::from_slice(payload).map_err(|error| rejected(error.to_string()))?;
    validate_fields(
        &shape,
        &[
            "metadata",
            "pages",
            "assets",
            "diagnostics",
            "schema_version",
            "notes",
            "outline",
        ],
    )?;
    if shape
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        != Some(DOCUMENT_SCHEMA_VERSION)
    {
        return Err(rejected(
            "import payload must declare the current Document schema explicitly",
        ));
    }
    if document.schema_version != DOCUMENT_SCHEMA_VERSION
        || document.pages.len() != 1
        || !document.assets.is_empty()
        || !document.notes.is_empty()
        || document.outline.is_some()
        || !document.diagnostics.is_empty()
        || document.pages[0].blocks.len() != 1
        || document.pages[0].layout.is_some()
        || document.pages[0].background_asset_id.is_some()
    {
        return Err(rejected(
            "WASI formula importer returned an unsupported document shape",
        ));
    }
    let Block::Formula(block) = &document.pages[0].blocks[0] else {
        return Err(rejected(
            "WASI formula importer did not return one standalone formula",
        ));
    };
    let page = &shape["pages"][0];
    validate_fields(
        page,
        &[
            "width",
            "height",
            "blocks",
            "page_number",
            "layout",
            "background_asset_id",
        ],
    )?;
    let raw_block = &page["blocks"][0];
    validate_fields(
        raw_block,
        &[
            "type",
            "formula",
            "label",
            "number",
            "environment",
            "geometry",
            "source",
        ],
    )?;
    let formula = &raw_block["formula"];
    validate_fields(
        formula,
        &[
            "source",
            "display_mode",
            "confidence",
            "source_info",
            "layout",
            "recognition_provenance",
            "recognition_evidence",
        ],
    )?;
    validate_fields(&formula["source"], &["format", "content"])?;
    if block.formula.layout.is_some()
        || block.environment.is_some()
        || block.number.is_some()
        || block.label.is_some()
    {
        return Err(rejected(
            "WASI formula importer returned unprojected layout or numbering semantics",
        ));
    }
    Ok(match &block.formula.source {
        FormulaSource::Latex(source) => (source.clone(), FormulaInputFormat::Latex),
        FormulaSource::MathML(source) => (source.clone(), FormulaInputFormat::Mathml),
        FormulaSource::Omml(source) => (source.clone(), FormulaInputFormat::Omml),
        FormulaSource::Typst(source) => (source.clone(), FormulaInputFormat::Typst),
    })
}

fn rejected(message: impl Into<String>) -> ConversionRegistryError {
    ConversionRegistryError::new(ConversionRegistryErrorCode::InputRejected, message)
}

fn validate_fields(
    value: &serde_json::Value,
    allowed: &[&str],
) -> Result<(), ConversionRegistryError> {
    if value
        .as_object()
        .is_none_or(|fields| fields.keys().any(|key| !allowed.contains(&key.as_str())))
    {
        return Err(rejected(
            "import payload contains unknown or non-object formula fields",
        ));
    }
    Ok(())
}

fn wasi_failure(error: WasiDiagnostic) -> ConversionRegistryError {
    ConversionRegistryError::new(
        ConversionRegistryErrorCode::BackendFailed,
        format!("{}: {}", error.code, error.message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_conversion::formula_fragment::latex_display_to_fragment as standalone_display_source;
    #[test]
    fn standalone_projection_preserves_formula_content_and_rejects_document_shapes() {
        for (input, expected) in [
            ("\\[\nx^2\n\\]", "x^2"),
            (" \\[ \\frac{a}{b} \\] ", "\\frac{a}{b}"),
            ("\\[\\$+\\%+\\\\[x]\\]", "\\$+\\%+\\\\[x]"),
            ("\\[x % \\] ignored\n+y\\]", "x % \\] ignored\n+y"),
            ("\\[x % final comment\n\\]", "x % final comment\n"),
            (
                "\\[\\begin{aligned}x&=y\\end{aligned}\\]",
                "\\begin{aligned}x&=y\\end{aligned}",
            ),
        ] {
            assert_eq!(standalone_display_source(input).unwrap(), expected);
        }
        for input in [
            "x^2",
            "\\[\\]",
            "prefix \\[x\\]",
            "\\[x\\] tail",
            "\\[x\\]\\[y\\]",
            "\\[$x$\\]",
            "\\[\\(x\\)\\]",
            "\\[\\documentclass{article}x\\]",
            "\\[\\begin{document}x\\end{document}\\]",
            "\\[\\usepackage{amsmath}x\\]",
            "\\[\\verb|x|\\]",
            "\\[\\catcode1=2\\]",
        ] {
            assert!(standalone_display_source(input).is_err(), "{input}");
        }
        for (input, source) in [
            (FormulaInputFormat::Latex, "x^2"),
            (FormulaInputFormat::Typst, "x^2"),
            (FormulaInputFormat::Mathml, "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><msup><mi>x</mi><mn>2</mn></msup></math>"),
            (FormulaInputFormat::Omml, "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>"),
        ] {
            let display = DocumentConverter::convert_formula_string(source, input, OutputFormat::LatexDisplay, FormulaConversionMode::BestEffort).unwrap();
            let projected = standalone_display_source(&display).unwrap();
            assert!(!projected.contains("document"));
            assert!(!projected.contains("\\["));
            assert!(projected.contains('x') && projected.contains('2'));
        }
    }
    #[test]
    fn imported_json_does_not_silently_drop_unknown_formula_fields_or_future_schema() {
        let document = DocumentBuilder::new()
            .page(0.0, 0.0, |page| {
                page.display_formula("x^2");
            })
            .build();
        let valid = serde_json::to_vec(&document).unwrap();
        assert_eq!(
            import_formula_source(&valid).unwrap(),
            ("x^2".into(), FormulaInputFormat::Latex)
        );
        for path in [0, 1, 2, 3, 4] {
            let mut value = serde_json::to_value(&document).unwrap();
            match path {
                0 => value["unknown_formula"] = serde_json::json!("hidden"),
                1 => value["pages"][0]["future"] = serde_json::json!(true),
                2 => value["pages"][0]["blocks"][0]["future"] = serde_json::json!(true),
                3 => value["pages"][0]["blocks"][0]["formula"]["future"] = serde_json::json!(true),
                _ => {
                    value["pages"][0]["blocks"][0]["formula"]["source"]["future"] =
                        serde_json::json!(true)
                }
            }
            assert!(import_formula_source(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let mut value = serde_json::to_value(document).unwrap();
        value["schema_version"] = serde_json::json!("2.0.0");
        assert!(import_formula_source(&serde_json::to_vec(&value).unwrap()).is_err());
        value.as_object_mut().unwrap().remove("schema_version");
        assert!(import_formula_source(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

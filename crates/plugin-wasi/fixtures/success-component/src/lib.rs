wit_bindgen::generate!({
    path: "../../../../wit/plugin-v1",
    world: "plugin",
});

use core::sync::atomic::{AtomicBool, Ordering};

use exports::latexsnipper::plugin::{document_transformer, exporter, importer, lifecycle};
use latexsnipper::plugin::types::{
    Capability, Diagnostic, DiagnosticCode, DiagnosticSeverity, Document, DocumentPatch,
    ExportRequest, ExportResult, ImportRequest, InitContext, PatchOperation, PluginError,
    PluginErrorCode, PluginMetadata, ReplaceDocument,
};
use latexsnipper::plugin::{
    environment_broker, filesystem_broker, model_artifact_broker, network_broker, system_broker,
    temporary_storage_broker,
};

struct Fixture;

static FAIL_SHUTDOWN: AtomicBool = AtomicBool::new(false);

impl lifecycle::Guest for Fixture {
    fn metadata() -> PluginMetadata {
        PluginMetadata {
            id: "fixture.component".to_string(),
            name: "Fixture Component".to_string(),
            version: "1.0.0".to_string(),
            plugin_api_version: 2,
            wit_version: 1,
        }
    }

    fn declared_capabilities() -> Vec<Capability> {
        vec![
            Capability::DocumentTransform,
            Capability::Importer,
            Capability::Exporter,
            Capability::FilesystemRead,
            Capability::FilesystemWrite,
            Capability::EnvironmentRead,
            Capability::NetworkRequest,
            Capability::ModelArtifactRead,
            Capability::TemporaryStorage,
            Capability::ClockRead,
            Capability::RandomRead,
        ]
    }

    fn initialize(context: InitContext) -> Result<(), PluginError> {
        if context.configuration == b"control:init-error" {
            return Err(fixture_error("fixture initialize failed"));
        }
        Ok(())
    }

    fn shutdown() -> Result<(), PluginError> {
        if FAIL_SHUTDOWN.swap(false, Ordering::SeqCst) {
            return Err(fixture_error("fixture shutdown failed"));
        }
        Ok(())
    }
}

impl document_transformer::Guest for Fixture {
    fn transform(document: Document) -> Result<DocumentPatch, PluginError> {
        if document.payload == b"control:infinite" {
            loop {
                core::hint::spin_loop();
            }
        }
        if document.payload == b"control:guest-error" {
            return Err(fixture_error("fixture invocation failed"));
        }
        if document.payload == b"control:shutdown-error" {
            FAIL_SHUTDOWN.store(true, Ordering::SeqCst);
        }
        let invalid_patch = document.payload == b"control:invalid-patch";
        let payload = match document.payload.as_slice() {
            b"control:oversize-output" => vec![0; 2 * 1024 * 1024],
            b"broker:environment" => environment_broker::get("FIXTURE_ENV")?
                .unwrap_or_default()
                .into_bytes(),
            b"broker:filesystem-read" => filesystem_broker::read("path-0", "input.txt")
                .map_err(|_| fixture_error("filesystem read failed"))?,
            b"broker:filesystem-write" => {
                filesystem_broker::write("path-1", "output.txt", b"written")
                    .map_err(|_| fixture_error("filesystem write failed"))?;
                b"written".to_vec()
            }
            b"broker:model" => {
                let artifact = model_artifact_broker::open("fixture-model")?;
                artifact.read(0, 1024)?
            }
            b"broker:temporary" => {
                let file = temporary_storage_broker::create()?;
                file.write(0, b"temporary")?;
                file.read(0, 1024)?
            }
            b"broker:network" => {
                network_broker::send(&network_broker::Request {
                    destination: network_broker::Destination {
                        scheme: network_broker::Scheme::Https,
                        host: "models.example.invalid".to_string(),
                        port: 443,
                    },
                    method: "GET".to_string(),
                    path_and_query: "/fixture".to_string(),
                    body: Vec::new(),
                })?
                .body
            }
            b"broker:system" => {
                let _ = system_broker::monotonic_millis()?;
                system_broker::random_bytes(8)?
            }
            _ => document.payload,
        };
        Ok(DocumentPatch {
            base_schema_version: if invalid_patch {
                "wrong-schema".to_string()
            } else {
                document.schema_version
            },
            operations: vec![PatchOperation::ReplaceDocument(ReplaceDocument {
                media_type: document.media_type,
                payload,
            })],
            diagnostics: Vec::new(),
        })
    }
}

fn fixture_error(message: &str) -> PluginError {
    PluginError {
        code: PluginErrorCode::Internal,
        message: message.to_string(),
        diagnostics: vec![Diagnostic {
            code: DiagnosticCode::PluginWasiHostFailure,
            severity: DiagnosticSeverity::Error,
            message: "fixture detail".to_string(),
            field: Some("fixture.field".to_string()),
        }],
    }
}

impl importer::Guest for Fixture {
    fn import_document(request: ImportRequest) -> Result<Document, PluginError> {
        if request.format == "application/vnd.fixture.power" {
            if request.payload == b"control:infinite" {
                loop {
                    core::hint::spin_loop();
                }
            }
            if request.payload == b"control:wrong-media" {
                return Ok(Document {
                    schema_version: "1.0.0".into(),
                    media_type: "text/plain".into(),
                    payload: b"x^2".to_vec(),
                });
            }
            if request.payload == b"control:invalid-json" {
                return Ok(Document {
                    schema_version: "1.0.0".into(),
                    media_type: "application/vnd.latexsnipper.document+json".into(),
                    payload: b"not JSON".to_vec(),
                });
            }
            let source = if request.payload == b"control:extra-block" {
                "x^2".into()
            } else {
                let [variable, b':', exponent] = request.payload.as_slice() else {
                    return Err(fixture_error("invalid power syntax"));
                };
                if !variable.is_ascii_lowercase() || !exponent.is_ascii_digit() {
                    return Err(fixture_error("invalid power operands"));
                }
                format!("{}^{}", *variable as char, *exponent as char)
            };
            let ast = latexsnipper_ast::DocumentBuilder::new()
                .page(0.0, 0.0, |page| {
                    page.display_formula(source);
                    if request.payload == b"control:extra-block" {
                        page.text_paragraph("unprojected content");
                    }
                })
                .build();
            return Ok(Document {
                schema_version: "1.0.0".into(),
                media_type: "application/vnd.latexsnipper.document+json".into(),
                payload: serde_json::to_vec(&ast)
                    .map_err(|_| fixture_error("serialization failed"))?,
            });
        }
        Ok(Document {
            schema_version: "1.0.0".to_string(),
            media_type: request.format,
            payload: request.payload,
        })
    }
}

impl exporter::Guest for Fixture {
    fn export_document(request: ExportRequest) -> Result<ExportResult, PluginError> {
        if request.format == "application/vnd.fixture.power" {
            if request.document.media_type != "application/vnd.latexsnipper.document+json" {
                return Err(fixture_error("real AST required"));
            }
            let ast: latexsnipper_ast::Document = serde_json::from_slice(&request.document.payload)
                .map_err(|_| fixture_error("invalid AST"))?;
            let [latexsnipper_ast::Block::Formula(block)] = ast
                .pages
                .first()
                .ok_or_else(|| fixture_error("no page"))?
                .blocks
                .as_slice()
            else {
                return Err(fixture_error("one formula required"));
            };
            let latexsnipper_ast::FormulaSource::Latex(source) = &block.formula.source else {
                return Err(fixture_error("LaTeX projection required"));
            };
            if source == "control:invalid-utf8" {
                return Ok(ExportResult {
                    media_type: request.format,
                    payload: vec![255],
                    diagnostics: Vec::new(),
                });
            }
            let [variable, b'^', exponent] = source.as_bytes() else {
                return Err(fixture_error("unsupported power"));
            };
            if !variable.is_ascii_lowercase() || !exponent.is_ascii_digit() {
                return Err(fixture_error("unsupported power operands"));
            }
            return Ok(ExportResult {
                media_type: request.format,
                payload: vec![*variable, b':', *exponent],
                diagnostics: Vec::new(),
            });
        }
        Ok(ExportResult {
            media_type: request.format,
            payload: request.document.payload,
            diagnostics: Vec::new(),
        })
    }
}

export!(Fixture);

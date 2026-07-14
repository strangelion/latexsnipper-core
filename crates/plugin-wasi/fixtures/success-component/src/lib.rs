wit_bindgen::generate!({
    path: "../../../../wit/plugin-v1",
    world: "plugin",
});

use exports::latexsnipper::plugin::{document_transformer, exporter, importer, lifecycle};
use latexsnipper::plugin::types::{
    Capability, Document, DocumentPatch, ExportRequest, ExportResult, ImportRequest, InitContext,
    PatchOperation, PluginError, PluginErrorCode, PluginMetadata, ReplaceDocument,
};
use latexsnipper::plugin::{
    environment_broker, filesystem_broker, model_artifact_broker, network_broker, system_broker,
    temporary_storage_broker,
};

struct Fixture;

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
        ]
    }

    fn initialize(_context: InitContext) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown() -> Result<(), PluginError> {
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
        diagnostics: Vec::new(),
    }
}

impl importer::Guest for Fixture {
    fn import_document(request: ImportRequest) -> Result<Document, PluginError> {
        Ok(Document {
            schema_version: "1.0.0".to_string(),
            media_type: request.format,
            payload: request.payload,
        })
    }
}

impl exporter::Guest for Fixture {
    fn export_document(request: ExportRequest) -> Result<ExportResult, PluginError> {
        Ok(ExportResult {
            media_type: request.format,
            payload: request.document.payload,
            diagnostics: Vec::new(),
        })
    }
}

export!(Fixture);

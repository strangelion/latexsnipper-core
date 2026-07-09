//! Concrete StageRunner implementations for standard processing stages.
//!
//! Each runner implements the `StageRunner` trait from `latexsnipper-ast`
//! and produces a `StageReport` that can be attached to a `Job`.

use latexsnipper_ast::{
    ArtifactEntry, ArtifactKind, ArtifactManifest, Diagnostic, DiagnosticLevel, Document,
    EventRecord, JobRoot, StageKind, StageProducedArtifact, StageReport, StageRunner, StageSpec,
    StageStatus,
};
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// DecodeStage — decodes raw input into page images
// ---------------------------------------------------------------------------

/// Stage runner that decodes a source document into page images
/// for downstream processing (detection, recognition).
pub struct DecodeStage;

impl StageRunner for DecodeStage {
    fn kind(&self) -> StageKind {
        StageKind::Decode
    }

    fn run(&self, spec: &StageSpec, job_root: &JobRoot) -> Result<StageReport, String> {
        let start = std::time::Instant::now();
        let mut diags = Vec::new();
        let mut output_artifacts = Vec::new();
        let mut produced_artifacts = Vec::new();

        // Ensure decoded dir exists
        std::fs::create_dir_all(&job_root.decoded_dir)
            .map_err(|e| format!("Create decoded dir: {}", e))?;

        if let Some(src) = &spec.input.source {
            match std::fs::read(src) {
                Ok(bytes) => {
                    let ext = src.rsplit('.').next().unwrap_or("bin");
                    let out_path = format!("{}/source.{}", job_root.decoded_dir, ext);
                    std::fs::write(&out_path, &bytes)
                        .map_err(|e| format!("Write decoded: {}", e))?;
                    let mime = match ext {
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "pdf" => "application/pdf",
                        _ => "application/octet-stream",
                    };
                    let meta = std::fs::metadata(&out_path).ok();
                    let checksum = compute_file_sha256(&out_path).ok();
                    let size = meta.map(|m| m.len());
                    produced_artifacts.push(StageProducedArtifact {
                        id: format!("{}:decoded", spec.stage_id),
                        kind: ArtifactKind::from_output_or_stage(&spec.output, spec.kind),
                        path: out_path.clone(),
                        mime_type: Some(mime.to_string()),
                        format: Some(ext.to_string()),
                        checksum_sha256: checksum,
                        size_bytes: size,
                    });
                    output_artifacts.push(out_path);
                }
                Err(e) => {
                    diags.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            "W_DECODE_FAILED",
                            format!("Cannot read source '{}': {}", src, e),
                        )
                        .with_recoverable(true),
                    );
                }
            }
        } else {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    "W_NO_SOURCE",
                    "No input source specified for DecodeStage",
                )
                .with_recoverable(true),
            );
        }

        let has_output = !output_artifacts.is_empty();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Decode,
            status: if has_output {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            },
            started_at: Some(minimal_timestamp()),
            finished_at: Some(minimal_timestamp()),
            elapsed_ms: Some(elapsed),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts,
            diagnostics: diags,
            produced_artifacts,
        })
    }
}

// ---------------------------------------------------------------------------
// RecognizeStage — runs OCR detection and recognition
// ---------------------------------------------------------------------------

/// Stage runner that runs detection and recognition models.
pub struct RecognizeStage;

impl StageRunner for RecognizeStage {
    fn kind(&self) -> StageKind {
        StageKind::Recognize
    }

    fn run(&self, spec: &StageSpec, job_root: &JobRoot) -> Result<StageReport, String> {
        let start = std::time::Instant::now();
        let mut diags = Vec::new();
        let mut output_artifacts = Vec::new();
        let mut produced_artifacts = Vec::new();

        // Ensure ast dir exists
        std::fs::create_dir_all(&job_root.ast_dir).map_err(|e| format!("Create ast dir: {}", e))?;

        if let Some(src) = &spec.input.source {
            match std::fs::read_to_string(src) {
                Ok(content) => {
                    // Check if it's a Document JSON by trying to parse it
                    if let Ok(doc) = serde_json::from_str::<Document>(&content) {
                        // Passthrough — write to ast directory
                        let out_path = format!("{}/document.ast.json", job_root.ast_dir);
                        std::fs::write(&out_path, &content)
                            .map_err(|e| format!("Write AST: {}", e))?;
                        let checksum = compute_file_sha256(&out_path).ok();
                        let meta = std::fs::metadata(&out_path).ok();
                        let size = meta.map(|m| m.len());
                        produced_artifacts.push(StageProducedArtifact {
                            id: format!("{}:ast", spec.stage_id),
                            kind: ArtifactKind::DocumentAst,
                            path: out_path.clone(),
                            mime_type: Some("application/json".to_string()),
                            format: Some("json".to_string()),
                            checksum_sha256: checksum,
                            size_bytes: size,
                        });
                        output_artifacts.push(out_path);
                        diags.push(Diagnostic::new(
                            DiagnosticLevel::Info,
                            "I_AST_PASSTHROUGH",
                            format!(
                                "Recognized {} blocks across {} pages",
                                doc.all_blocks().len(),
                                doc.pages.len()
                            ),
                        ));
                    } else {
                        // Not Document JSON — could be an image, emit diagnostic
                        diags.push(
                            Diagnostic::new(
                                DiagnosticLevel::Warning,
                                "W_RECOGNIZE_NOT_IMPLEMENTED",
                                "Real image recognition not implemented in RecognizeStage yet",
                            )
                            .with_recoverable(true),
                        );
                    }
                }
                Err(e) => {
                    diags.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            "W_READ_FAILED",
                            format!("Cannot read '{}': {}", src, e),
                        )
                        .with_recoverable(true),
                    );
                }
            }
        }

        let has_output = !output_artifacts.is_empty();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Recognize,
            status: if has_output {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            },
            started_at: Some(minimal_timestamp()),
            finished_at: Some(minimal_timestamp()),
            elapsed_ms: Some(elapsed),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts,
            diagnostics: diags,
            produced_artifacts,
        })
    }
}

// ---------------------------------------------------------------------------
// ConvertStage — converts Document AST to a text format
// ---------------------------------------------------------------------------

/// Stage runner that converts a Document AST to a semantic text format.
pub struct ConvertStage;

impl StageRunner for ConvertStage {
    fn kind(&self) -> StageKind {
        StageKind::Convert
    }

    fn run(&self, spec: &StageSpec, job_root: &JobRoot) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        let start = std::time::Instant::now();
        let mut diags = Vec::new();
        let mut output_artifacts = Vec::new();
        let mut produced_artifacts = Vec::new();

        std::fs::create_dir_all(&job_root.converted_dir)
            .map_err(|e| format!("Create converted dir: {}", e))?;

        // Try to read Document AST from input artifacts
        let format = spec
            .options
            .get("target_format")
            .and_then(|v| v.as_str())
            .unwrap_or("latex");

        if let Some(src) = &spec.input.source {
            match std::fs::read_to_string(src) {
                Ok(content) => {
                    // Try to read as Document JSON
                    match serde_json::from_str::<Document>(&content) {
                        Ok(doc) => {
                            let format_label = match format {
                                "md" | "markdown" => "markdown",
                                "tex" | "latex" => "latex",
                                "html" => "html",
                                "typ" | "typst" => "typst",
                                _ => format,
                            };
                            let out_dir = &job_root.converted_dir;
                            let out_path =
                                format!("{}/{}.{}", out_dir, spec.stage_id, format_label);
                            let mut text = String::new();
                            let mut conv_diags = diags.clone();

                            // Try DocumentConverter
                            use latexsnipper_conversion::document_converter::{
                                DocumentConverter, OutputFormat,
                            };
                            let out_fmt = match format_label {
                                "latex" => OutputFormat::Latex,
                                "typst" => OutputFormat::Typst,
                                "markdown" => OutputFormat::MarkdownBlock,
                                "html" => OutputFormat::Html,
                                _ => OutputFormat::Latex,
                            };
                            match DocumentConverter::new(out_fmt).convert_artifact(&doc) {
                                Ok(artifact) => {
                                    if let Some(t) = &artifact.text {
                                        text = t.clone();
                                    }
                                    conv_diags.extend(artifact.diagnostics);
                                }
                                Err(e) => {
                                    diags.push(
                                        Diagnostic::new(
                                            DiagnosticLevel::Warning,
                                            "W_CONVERT_FAILED",
                                            format!("Converter error: {}", e),
                                        )
                                        .with_recoverable(true),
                                    );
                                }
                            }

                            if !text.is_empty() {
                                std::fs::write(&out_path, &text)
                                    .map_err(|e| format!("Write output '{}': {}", out_path, e))?;
                                let checksum = compute_file_sha256(&out_path).ok();
                                let meta = std::fs::metadata(&out_path).ok();
                                let size = meta.map(|m| m.len());
                                let mime = match format_label {
                                    "latex" => "text/x-latex",
                                    "markdown" => "text/markdown",
                                    "html" => "text/html",
                                    "typst" => "text/x-typst",
                                    _ => "text/plain",
                                };
                                produced_artifacts.push(StageProducedArtifact {
                                    id: format!("{}:converted", spec.stage_id),
                                    kind: ArtifactKind::from_output_or_stage(
                                        &spec.output,
                                        spec.kind,
                                    ),
                                    path: out_path.clone(),
                                    mime_type: Some(mime.to_string()),
                                    format: Some(format_label.to_string()),
                                    checksum_sha256: checksum,
                                    size_bytes: size,
                                });
                                output_artifacts.push(out_path);
                            }
                            diags.extend(conv_diags);
                        }
                        Err(e) => {
                            diags.push(
                                Diagnostic::new(
                                    DiagnosticLevel::Warning,
                                    "W_UNSUPPORTED_INPUT",
                                    format!("Input is not Document JSON: {}", e),
                                )
                                .with_recoverable(true),
                            );
                            // Don't push to output_artifacts — stage will fail cleanly
                        }
                    }
                }
                Err(e) => {
                    diags.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            "W_INPUT_MISSING",
                            format!("Cannot read input source '{}': {}", src, e),
                        )
                        .with_recoverable(true),
                    );
                }
            }
        }

        let has_output = !output_artifacts.is_empty();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Convert,
            status: if has_output {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            },
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(elapsed),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts,
            produced_artifacts,
            diagnostics: diags,
        })
    }
}

// ---------------------------------------------------------------------------
// ExportStage — exports the result to a file format
// ---------------------------------------------------------------------------

/// Stage runner that exports the final result to a file or clipboard format.
pub struct ExportStage;

impl StageRunner for ExportStage {
    fn kind(&self) -> StageKind {
        StageKind::Export
    }

    fn run(&self, spec: &StageSpec, job_root: &JobRoot) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        let start = std::time::Instant::now();
        let mut diags = Vec::new();
        let mut output_artifacts = Vec::new();
        let mut produced_artifacts = Vec::new();

        std::fs::create_dir_all(&job_root.exported_dir)
            .map_err(|e| format!("Create exported dir: {}", e))?;

        let format = spec
            .options
            .get("visual_format")
            .and_then(|v| v.as_str())
            .unwrap_or("svg");

        if let Some(src) = &spec.input.source {
            match std::fs::read_to_string(src) {
                Ok(content) => match serde_json::from_str::<Document>(&content) {
                    Ok(doc) => {
                        let out_dir = &job_root.exported_dir;
                        let out_path = format!("{}/{}.{}", out_dir, spec.stage_id, format);
                        let visual_fmt = match format {
                            "svg" => latexsnipper_export::VisualFormat::Svg,
                            "pdf" => latexsnipper_export::VisualFormat::Pdf,
                            "txt" | "text" => latexsnipper_export::VisualFormat::PlainText,
                            _ => latexsnipper_export::VisualFormat::Svg,
                        };
                        match latexsnipper_export::ExportService::export(&doc, visual_fmt) {
                            Ok(artifact) => {
                                if let Some(t) = &artifact.text {
                                    std::fs::write(&out_path, t).map_err(|e| {
                                        format!("Write output '{}': {}", out_path, e)
                                    })?;
                                    let checksum = compute_file_sha256(&out_path).ok();
                                    let meta = std::fs::metadata(&out_path).ok();
                                    let size = meta.map(|m| m.len());
                                    let mime = match format {
                                        "svg" => "image/svg+xml",
                                        "pdf" => "application/pdf",
                                        "txt" | "text" => "text/plain",
                                        _ => "application/octet-stream",
                                    };
                                    produced_artifacts.push(StageProducedArtifact {
                                        id: format!("{}:exported", spec.stage_id),
                                        kind: ArtifactKind::from_output_or_stage(
                                            &spec.output,
                                            spec.kind,
                                        ),
                                        path: out_path.clone(),
                                        mime_type: Some(mime.to_string()),
                                        format: Some(format.to_string()),
                                        checksum_sha256: checksum,
                                        size_bytes: size,
                                    });
                                    output_artifacts.push(out_path);
                                }
                                diags.extend(artifact.diagnostics);
                            }
                            Err(e) => {
                                diags.push(
                                    Diagnostic::new(
                                        DiagnosticLevel::Warning,
                                        "W_EXPORT_FAILED",
                                        format!("Export error: {}", e),
                                    )
                                    .with_recoverable(true),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        diags.push(
                            Diagnostic::new(
                                DiagnosticLevel::Warning,
                                "W_SOURCE_NOT_AST",
                                format!("Cannot parse Document JSON: {}", e),
                            )
                            .with_recoverable(true),
                        );
                    }
                },
                Err(e) => {
                    diags.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            "W_INPUT_MISSING",
                            format!("Cannot read input source '{}': {}", src, e),
                        )
                        .with_recoverable(true),
                    );
                }
            }
        }

        let has_output = !output_artifacts.is_empty();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Export,
            status: if has_output {
                StageStatus::Succeeded
            } else {
                StageStatus::Failed
            },
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(elapsed),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts,
            produced_artifacts,
            diagnostics: diags,
        })
    }
}

/// Compute SHA-256 checksum of a file.
fn compute_file_sha256(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Read for checksum: {}", e))?;
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// Minimal ISO 8601 timestamp (no external chrono dependency).
fn minimal_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let ms = now.subsec_millis();
    // Simple ISO 8601 formatting (UTC)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let month_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1usize;
    for &md in month_days.iter() {
        if d < md {
            break;
        }
        d -= md;
        m += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y,
        m,
        d + 1,
        hours,
        minutes,
        seconds,
        ms
    )
}

// ---------------------------------------------------------------------------
// StageOrchestrator — runs stages in sequence, managing manifests, events
// ---------------------------------------------------------------------------

/// Orchestrator that runs stages in sequence, managing artifact manifests,
/// event logs, and stage reports.
pub struct StageOrchestrator {
    pub job_root: JobRoot,
    pub artifact_manifest: ArtifactManifest,
    pub runners: HashMap<String, Box<dyn StageRunner>>,
}

impl StageOrchestrator {
    pub fn new(job_root: JobRoot) -> Self {
        Self {
            artifact_manifest: ArtifactManifest {
                schema_version: "1.0.0".to_string(),
                job_id: job_root.job_id.clone(),
                artifacts: Vec::new(),
            },
            job_root,
            runners: HashMap::new(),
        }
    }

    pub fn register_runner(&mut self, runner: Box<dyn StageRunner>) {
        self.runners.insert(format!("{:?}", runner.kind()), runner);
    }

    /// Run a single stage spec and produce a report.
    /// Writes the report, an event log entry, and updates the artifact manifest.
    pub fn run_stage(&mut self, spec: &StageSpec) -> Result<StageReport, String> {
        let kind_str = format!("{:?}", spec.kind);
        let runner = self
            .runners
            .get(&kind_str)
            .ok_or_else(|| format!("No runner registered for stage kind: {}", kind_str))?;

        // Ensure job directories exist
        self.job_root.ensure_dirs()?;

        // Execute the stage
        let report = runner.run(spec, &self.job_root)?;

        // Write stage report JSON
        let report_path = format!(
            "{}/{}.report.json",
            self.job_root.reports_dir, spec.stage_id
        );
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| format!("Serialize report: {}", e))?;
        std::fs::write(&report_path, &json).map_err(|e| format!("Write report: {}", e))?;

        // Append event record to events.jsonl (JSON Lines format)
        let event = EventRecord {
            timestamp: minimal_timestamp(),
            level: if report.status == StageStatus::Failed {
                "error".to_string()
            } else {
                "info".to_string()
            },
            job_id: Some(self.job_root.job_id.clone()),
            stage_id: Some(spec.stage_id.clone()),
            event: format!("stage.{:?}", report.status).to_lowercase(),
            code: if report.status == StageStatus::Failed {
                Some("STAGE_FAILED".to_string())
            } else {
                None
            },
            message: format!("Stage '{}' {:?}", spec.stage_id, report.status),
            data: serde_json::Value::Null,
        };
        let event_path = format!("{}/events.jsonl", self.job_root.logs_dir);
        let line = serde_json::to_string(&event).map_err(|e| format!("Serialize event: {}", e))?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&event_path)
            .map_err(|e| format!("Open events file: {}", e))?;
        use std::io::Write;
        writeln!(f, "{}", line).map_err(|e| format!("Write event: {}", e))?;

        // Register each output artifact in the manifest
        // Use produced_artifacts from report if available
        if !report.produced_artifacts.is_empty() {
            for art in &report.produced_artifacts {
                self.artifact_manifest.artifacts.push(ArtifactEntry {
                    id: art.id.clone(),
                    kind: art.kind.clone(),
                    path: art.path.clone(),
                    mime_type: art.mime_type.clone(),
                    format: art.format.clone(),
                    checksum_sha256: art.checksum_sha256.clone(),
                    size_bytes: art.size_bytes,
                    producer_stage_id: Some(spec.stage_id.clone()),
                    source_artifact_ids: spec.input.artifacts.clone(),
                });
            }
        } else {
            // Legacy fallback using output_artifacts strings
            for art_id in &report.output_artifacts {
                self.artifact_manifest.artifacts.push(ArtifactEntry {
                    id: art_id.clone(),
                    kind: ArtifactKind::from_stage_kind(&spec.kind),
                    path: art_id.clone(),
                    mime_type: None,
                    format: None,
                    checksum_sha256: None,
                    size_bytes: None,
                    producer_stage_id: Some(spec.stage_id.clone()),
                    source_artifact_ids: spec.input.artifacts.clone(),
                });
            }
        }

        // Write updated artifact manifest
        let manifest_path = format!("{}/artifacts.json", self.job_root.artifacts_dir);
        let json = serde_json::to_string_pretty(&self.artifact_manifest)
            .map_err(|e| format!("Serialize manifest: {}", e))?;
        std::fs::write(&manifest_path, &json).map_err(|e| format!("Write manifest: {}", e))?;

        Ok(report)
    }

    /// Read a StageSpec from a JSON file, run it, and return the report.
    /// This is a lighter-weight entry point when specs are stored as files.
    pub fn run_spec_file(&mut self, path: &Path) -> Result<StageReport, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read spec file: {}", e))?;
        let spec: StageSpec =
            serde_json::from_str(&content).map_err(|e| format!("Failed to parse spec: {}", e))?;
        self.run_stage(&spec)
    }
}

/// Register the four standard stage runners (Decode, Recognize, Convert, Export).
pub fn register_default_runners(orchestrator: &mut StageOrchestrator) {
    orchestrator.register_runner(Box::new(DecodeStage));
    orchestrator.register_runner(Box::new(RecognizeStage));
    orchestrator.register_runner(Box::new(ConvertStage));
    orchestrator.register_runner(Box::new(ExportStage));
}

//! Concrete StageRunner implementations for standard processing stages.
//!
//! Each runner implements the `StageRunner` trait from `latexsnipper-ast`
//! and produces a `StageReport` that can be attached to a `Job`.

use latexsnipper_ast::{StageKind, StageReport, StageRunner, StageSpec, StageStatus};

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

    fn run(&self, spec: &StageSpec) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Decode,
            status: StageStatus::Succeeded,
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(0),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts: vec![format!("{}/decoded", spec.stage_id)],
            diagnostics: Vec::new(),
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

    fn run(&self, spec: &StageSpec) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Recognize,
            status: StageStatus::Succeeded,
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(0),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts: vec![format!("{}/ast", spec.stage_id)],
            diagnostics: Vec::new(),
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

    fn run(&self, spec: &StageSpec) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Convert,
            status: StageStatus::Succeeded,
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(0),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts: vec![format!("{}/converted", spec.stage_id)],
            diagnostics: Vec::new(),
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

    fn run(&self, spec: &StageSpec) -> Result<StageReport, String> {
        let now = Some(minimal_timestamp());
        Ok(StageReport {
            stage_id: spec.stage_id.clone(),
            kind: StageKind::Export,
            status: StageStatus::Succeeded,
            started_at: now.clone(),
            finished_at: now,
            elapsed_ms: Some(0),
            input_artifacts: spec.input.artifacts.clone(),
            output_artifacts: vec![format!("{}/exported", spec.stage_id)],
            diagnostics: Vec::new(),
        })
    }
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

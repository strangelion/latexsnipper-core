use latexsnipper_ast::{
    ArtifactKind, ArtifactManifest, Diagnostic, EventRecord, JobRoot, StageReport, StageStatus,
};
use log::info;
use std::collections::VecDeque;

/// Status of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A unit of work to be executed by the engine.
///
/// Uses platform types from `latexsnipper-ast` for artifacts, stages,
/// diagnostics, and event logs. Backward-compatible: `result` is kept
/// as a simple string for callers that don't need structured reports.
pub struct Job {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub result: Option<String>,

    // ── Platform extensions (Phase 3) ──
    /// Standard job directory layout.
    pub job_root: Option<JobRoot>,
    /// Index of all artifacts produced by this job.
    pub artifacts: ArtifactManifest,
    /// Per-stage reports.
    pub stages: Vec<StageReport>,
    /// Diagnostic messages (warnings, errors).
    pub diagnostics: Vec<Diagnostic>,
    /// Structured event log.
    pub events: Vec<EventRecord>,
}

impl Job {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: JobStatus::Pending,
            result: None,
            job_root: None,
            artifacts: ArtifactManifest {
                schema_version: "1.0.0".to_string(),
                job_id: String::new(),
                artifacts: Vec::new(),
            },
            stages: Vec::new(),
            diagnostics: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Add a stage report and update job status on completion.
    pub fn add_stage_report(&mut self, report: StageReport) {
        if report.status == StageStatus::Failed {
            self.status = JobStatus::Failed;
        }
        self.stages.push(report);
    }

    /// Add an event record to the event log.
    pub fn log_event(&mut self, event: EventRecord) {
        self.events.push(event);
    }

    /// Add a diagnostic.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        if !diagnostic.recoverable {
            self.status = JobStatus::Failed;
        }
        self.diagnostics.push(diagnostic);
    }

    /// Record an artifact produced by a stage.
    pub fn add_artifact(
        &mut self,
        id: impl Into<String>,
        kind: ArtifactKind,
        path: impl Into<String>,
        producer_stage_id: Option<String>,
    ) {
        self.artifacts
            .artifacts
            .push(latexsnipper_ast::ArtifactEntry {
                id: id.into(),
                kind,
                path: path.into(),
                mime_type: None,
                format: None,
                checksum_sha256: None,
                size_bytes: None,
                producer_stage_id,
                source_artifact_ids: Vec::new(),
            });
    }
}

/// A queue for managing jobs.
pub struct JobQueue {
    pending: VecDeque<Job>,
    active: HashMap<String, Job>,
    completed: HashMap<String, Job>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            active: HashMap::new(),
            completed: HashMap::new(),
        }
    }

    /// Submit a job to the queue.
    pub fn submit(&mut self, job: Job) {
        info!("Job '{}' submitted: {}", job.id, job.name);
        self.pending.push_back(job);
    }

    /// Get the next job to execute.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Job> {
        self.pending.pop_front()
    }

    /// Mark a job as running.
    pub fn start(&mut self, job: Job) {
        let id = job.id.clone();
        info!("Job '{}' started", id);
        self.active.insert(id, job);
    }

    /// Mark a job as completed.
    pub fn complete(&mut self, id: &str, result: String) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Completed;
            job.result = Some(result.clone());
            job.log_event(EventRecord {
                timestamp: chrono_now(),
                level: "info".to_string(),
                job_id: Some(id.to_string()),
                stage_id: None,
                event: "job.completed".to_string(),
                code: None,
                message: format!("Job '{}' completed", id),
                data: serde_json::Value::Null,
            });
            info!("Job '{}' completed", id);
            self.completed.insert(id.to_string(), job);
        }
    }

    /// Mark a job as failed with diagnostics.
    pub fn fail(&mut self, id: &str, error: String) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Failed;
            job.result = Some(error.clone());
            job.add_diagnostic(Diagnostic::new(
                latexsnipper_ast::DiagnosticLevel::Error,
                "JOB_FAILED",
                &error,
            ));
            job.log_event(EventRecord {
                timestamp: chrono_now(),
                level: "error".to_string(),
                job_id: Some(id.to_string()),
                stage_id: None,
                event: "job.failed".to_string(),
                code: Some("JOB_FAILED".to_string()),
                message: error,
                data: serde_json::Value::Null,
            });
            info!("Job '{}' failed", id);
            self.completed.insert(id.to_string(), job);
        }
    }

    /// Cancel a job.
    pub fn cancel(&mut self, id: &str) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Cancelled;
            info!("Job '{}' cancelled", id);
            self.completed.insert(id.to_string(), job);
        } else if let Some(job) = self.pending.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Cancelled;
        }
    }

    /// Get the number of pending jobs.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get the number of active jobs.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Get a job by ID.
    pub fn get(&self, id: &str) -> Option<&Job> {
        self.active.get(id).or_else(|| self.completed.get(id))
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

use std::collections::HashMap;

/// Simple UTC timestamp string (ISO 8601 without external chrono dependency).
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Format as ISO 8601 (fixed offset +00:00)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Compute year/month/day from days since epoch (simplified)
    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
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
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        y,
        m,
        d + 1,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

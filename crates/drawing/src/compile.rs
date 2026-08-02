use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::{
    sanitize_svg, DrawingArtifactRef, DrawingCompileRequest, DrawingDocument,
    DrawingFailureCandidate, DrawingOutputFormat, DrawingPackageProfile, DrawingSecurityPolicy,
    DrawingSourceAdapter, DrawingSourceLanguage, SourcePreservingAdapter, SvgSanitizerReport,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawingCompileArtifact {
    pub artifact: DrawingArtifactRef,
    pub bytes: Vec<u8>,
    pub sanitizer_report: SvgSanitizerReport,
}

#[derive(Debug, thiserror::Error)]
pub enum DrawingCompileError {
    #[error(transparent)]
    Adapter(#[from] crate::DrawingAdapterError),
    #[error(transparent)]
    Security(#[from] crate::DrawingSecurityError),
    #[error("DRAWING_COMPILE_IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("DRAWING_COMPILE_UNSUPPORTED: {0}")]
    Unsupported(String),
    #[error("DRAWING_COMPILE_TIMEOUT: {0}")]
    Timeout(String),
    #[error("DRAWING_COMPILE_FAILED: {0}")]
    Process(String),
    #[error("DRAWING_COMPILE_OUTPUT_INVALID: {0}")]
    Output(String),
}

pub trait DrawingFailureSink {
    fn record(&mut self, candidate: DrawingFailureCandidate);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DrawingCompileService;

impl DrawingCompileService {
    pub fn compile_svg(
        &self,
        document: &DrawingDocument,
        renderer_id: impl Into<String>,
        package_lock_sha256: Option<String>,
        policy: &DrawingSecurityPolicy,
    ) -> Result<DrawingCompileArtifact, DrawingCompileError> {
        let request = DrawingCompileRequest {
            output: DrawingOutputFormat::Svg,
            renderer_id: renderer_id.into(),
            package_lock_sha256,
            resource_sha256: document
                .resources
                .iter()
                .map(|resource| resource.sha256.clone())
                .collect(),
        };
        let adapter = SourcePreservingAdapter::for_language(document.source_language);
        let plan = adapter.compile_plan(document, &request, policy)?;
        let raw_svg = match document.source_language {
            DrawingSourceLanguage::SvgSource => document.source.text.as_bytes().to_vec(),
            DrawingSourceLanguage::GraphvizDot => {
                compile_graphviz(document, policy, &plan.executable)?
            }
            DrawingSourceLanguage::Tikz => compile_tikz(document, policy, &plan.executable)?,
            language => {
                return Err(DrawingCompileError::Unsupported(format!(
                    "{language:?} has no supervised SVG compiler"
                )))
            }
        };
        if raw_svg.len() as u64 > policy.max_output_bytes {
            return Err(DrawingCompileError::Output(format!(
                "{} bytes exceed {}",
                raw_svg.len(),
                policy.max_output_bytes
            )));
        }
        let raw_svg = String::from_utf8(raw_svg)
            .map_err(|error| DrawingCompileError::Output(error.to_string()))?;
        let report = sanitize_svg(&raw_svg, policy)?;
        let bytes = report.canonical_svg.as_bytes().to_vec();
        Ok(DrawingCompileArtifact {
            artifact: DrawingArtifactRef {
                format: DrawingOutputFormat::Svg,
                content_ref: format!("sha256:{}.svg", report.canonical_sha256),
                sha256: report.canonical_sha256.clone(),
                sanitizer_report_sha256: Some(format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&report)
                            .map_err(|error| { DrawingCompileError::Output(error.to_string()) })?
                    )
                )),
            },
            bytes,
            sanitizer_report: report,
        })
    }

    pub fn compile_svg_with_failure_sink(
        &self,
        document: &DrawingDocument,
        renderer_id: impl Into<String>,
        package_lock_sha256: Option<String>,
        policy: &DrawingSecurityPolicy,
        sink: &mut impl DrawingFailureSink,
    ) -> Result<DrawingCompileArtifact, DrawingCompileError> {
        let result = self.compile_svg(document, renderer_id, package_lock_sha256, policy);
        if let Err(error) = &result {
            sink.record(DrawingFailureCandidate::sanitized(
                format!("drawing-{}", document.id),
                document.source_language,
                &document.source.text,
                error.to_string(),
            ));
        }
        result
    }
}

fn compile_graphviz(
    document: &DrawingDocument,
    policy: &DrawingSecurityPolicy,
    executable: &Option<String>,
) -> Result<Vec<u8>, DrawingCompileError> {
    let executable = required_executable(executable, "graphviz")?;
    let directory = tempfile::tempdir()?;
    run_supervised(
        executable,
        &["-Tsvg".to_owned()],
        Some(document.source.text.as_bytes()),
        directory.path(),
        policy,
    )
}

fn compile_tikz(
    document: &DrawingDocument,
    policy: &DrawingSecurityPolicy,
    executable: &Option<String>,
) -> Result<Vec<u8>, DrawingCompileError> {
    let tectonic = required_executable(executable, "tectonic")?;
    let dvisvgm = policy.allowed_executables.get("dvisvgm").ok_or_else(|| {
        DrawingCompileError::Unsupported(
            "TikZ SVG requires a pinned and hash-verified dvisvgm executable".to_owned(),
        )
    })?;
    dvisvgm.verify_file_hash()?;
    let directory = tempfile::tempdir()?;
    stage_resources(document, policy, directory.path())?;
    let source = standalone_tikz_source(document);
    fs::write(directory.path().join("drawing.tex"), source)?;
    run_supervised(
        tectonic,
        &[
            "--only-cached".to_owned(),
            "--keep-logs".to_owned(),
            "--outdir".to_owned(),
            ".".to_owned(),
            "drawing.tex".to_owned(),
        ],
        None,
        directory.path(),
        policy,
    )?;
    let pdf = directory.path().join("drawing.pdf");
    if !pdf.is_file() {
        return Err(DrawingCompileError::Output(
            "Tectonic did not produce drawing.pdf".to_owned(),
        ));
    }
    run_supervised(
        &dvisvgm.path,
        &[
            "--pdf".to_owned(),
            "--no-fonts".to_owned(),
            "--exact".to_owned(),
            "--output=drawing.svg".to_owned(),
            "drawing.pdf".to_owned(),
        ],
        None,
        directory.path(),
        policy,
    )?;
    let svg = directory.path().join("drawing.svg");
    let bytes = fs::read(svg)?;
    validate_generated_files(directory.path(), policy)?;
    Ok(bytes)
}

fn required_executable<'a>(
    executable: &'a Option<String>,
    name: &str,
) -> Result<&'a Path, DrawingCompileError> {
    executable
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| DrawingCompileError::Unsupported(format!("{name} is not configured")))
}

fn standalone_tikz_source(document: &DrawingDocument) -> String {
    if document.source.text.contains("\\documentclass") {
        return document.source.text.clone();
    }
    let packages = document
        .package_profiles
        .iter()
        .filter_map(|profile| match profile {
            DrawingPackageProfile::BaseTikz => None,
            DrawingPackageProfile::PgfPlots => Some("\\usepackage{pgfplots}"),
            DrawingPackageProfile::CircuitTikz => Some("\\usepackage{circuitikz}"),
            DrawingPackageProfile::TikzCd => Some("\\usepackage{tikz-cd}"),
            DrawingPackageProfile::Forest => Some("\\usepackage{forest}"),
            DrawingPackageProfile::ChemFig => Some("\\usepackage{chemfig}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\\documentclass[tikz,border=2pt]{{standalone}}\n\\usepackage{{tikz}}\n{packages}\n\\begin{{document}}\n\\begin{{tikzpicture}}\n{}\n\\end{{tikzpicture}}\n\\end{{document}}\n",
        document.source.text
    )
}

fn stage_resources(
    document: &DrawingDocument,
    policy: &DrawingSecurityPolicy,
    destination: &Path,
) -> Result<(), DrawingCompileError> {
    for resource in &document.resources {
        let source = crate::resolve_resource(resource, policy)?;
        let target = destination.join(&resource.relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, target)?;
    }
    Ok(())
}

fn run_supervised(
    executable: &Path,
    arguments: &[String],
    stdin: Option<&[u8]>,
    working_directory: &Path,
    policy: &DrawingSecurityPolicy,
) -> Result<Vec<u8>, DrawingCompileError> {
    if policy.timeout_ms == 0 || policy.memory_limit_bytes == 0 {
        return Err(DrawingCompileError::Unsupported(
            "non-zero process limits are required".to_owned(),
        ));
    }
    let stdout_path = working_directory.join("process.stdout");
    let stderr_path = working_directory.join("process.stderr");
    let stdout = File::create(&stdout_path)?;
    let stderr = File::create(&stderr_path)?;
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(working_directory)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .env("LATEXSNIPPER_NETWORK_ALLOWED", "0")
        .env(
            "LATEXSNIPPER_MEMORY_LIMIT_BYTES",
            policy.memory_limit_bytes.to_string(),
        );
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn()?;
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .ok_or_else(|| DrawingCompileError::Process("stdin pipe unavailable".to_owned()))?
            .write_all(input)?;
    }
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= Duration::from_millis(policy.timeout_ms) {
            child.kill()?;
            let _ = child.wait();
            return Err(DrawingCompileError::Timeout(format!(
                "{} exceeded {} ms",
                executable.display(),
                policy.timeout_ms
            )));
        }
        thread::sleep(Duration::from_millis(10));
    };
    validate_generated_files(working_directory, policy)?;
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    if !status.success() {
        return Err(DrawingCompileError::Process(format!(
            "{} exited with {status}: {}",
            executable.display(),
            stderr.trim()
        )));
    }
    fs::read(stdout_path).map_err(Into::into)
}

fn validate_generated_files(
    directory: &Path,
    policy: &DrawingSecurityPolicy,
) -> Result<(), DrawingCompileError> {
    let mut pending = vec![PathBuf::from(directory)];
    let mut files = 0usize;
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                files = files.saturating_add(1);
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    }
    if files > policy.max_generated_files {
        return Err(DrawingCompileError::Output(format!(
            "{files} generated files exceed {}",
            policy.max_generated_files
        )));
    }
    if bytes > policy.max_output_bytes {
        return Err(DrawingCompileError::Output(format!(
            "{bytes} generated bytes exceed {}",
            policy.max_output_bytes
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Candidates(Vec<DrawingFailureCandidate>);

    impl DrawingFailureSink for Candidates {
        fn record(&mut self, candidate: DrawingFailureCandidate) {
            self.0.push(candidate);
        }
    }

    #[test]
    fn svg_source_compiles_in_process_and_is_sanitized() {
        let document = DrawingDocument::source_only(
            "svg",
            DrawingSourceLanguage::SvgSource,
            r#"<svg viewBox="0 0 10 10"><path d="M0 0L10 10"/></svg>"#,
        );
        let artifact = DrawingCompileService
            .compile_svg(
                &document,
                "svg-sanitizer@1",
                None,
                &DrawingSecurityPolicy::default(),
            )
            .unwrap();
        assert_eq!(artifact.artifact.format, DrawingOutputFormat::Svg);
        assert_eq!(artifact.artifact.sha256.len(), 64);
    }

    #[test]
    fn compile_failures_automatically_enter_the_privacy_safe_sink() {
        let document = DrawingDocument::source_only(
            "private",
            DrawingSourceLanguage::Tikz,
            "private drawing source",
        );
        let mut candidates = Candidates::default();
        DrawingCompileService
            .compile_svg_with_failure_sink(
                &document,
                "missing",
                Some("a".repeat(64)),
                &DrawingSecurityPolicy::default(),
                &mut candidates,
            )
            .unwrap_err();
        assert_eq!(candidates.0.len(), 1);
        let serialized = serde_json::to_string(&candidates.0[0]).unwrap();
        assert!(!serialized.contains("private drawing source"));
    }
}

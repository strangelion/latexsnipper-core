//! Bounded JSONL transport for long-lived application recognition sessions.
//!
//! This crate deliberately owns protocol framing only. Every live worker
//! session contains the same [`RecognitionSession`] used by the C and Python
//! adapters, so model/runtime lifetime and error semantics remain in Core.

use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Duration;

use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3, RecognitionProfile};
use latexsnipper_ast::{Diagnostic, ImportOptions};
use latexsnipper_conversion::OutputFormat;
use latexsnipper_engine::application::{ApplicationError, RecognitionOptions, RuntimePreference};
use latexsnipper_engine::{
    DocumentParseMode, RecognitionIntegrationApi, RecognitionRequest, RecognitionResult,
    RecognitionSession,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const JSONL_PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_MAX_SESSIONS: usize = 32;
pub const MAX_REQUEST_LINE_BYTES: usize = 1024 * 1024;
pub const MAX_INPUT_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const MAX_THREADS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerRequest {
    version: u32,
    id: Value,
    action: String,
    #[serde(default = "empty_object")]
    params: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerResponse {
    protocol_version: u32,
    id: Value,
    #[serde(flatten)]
    envelope: ApiEnvelopeV3<Value>,
}

impl WorkerResponse {
    fn success(id: Value, data: Value, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            protocol_version: JSONL_PROTOCOL_VERSION,
            id,
            envelope: ApiEnvelopeV3::success(data, diagnostics),
        }
    }

    fn failure(id: Value, error: WorkerError) -> Self {
        Self {
            protocol_version: JSONL_PROTOCOL_VERSION,
            id,
            envelope: ApiEnvelopeV3::failure(error.into(), Vec::new()),
        }
    }
}

#[derive(Debug)]
struct WorkerError {
    code: String,
    message: String,
    recoverable: bool,
    details: Option<Value>,
}

impl WorkerError {
    fn protocol(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            recoverable: false,
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl From<ApplicationError> for WorkerError {
    fn from(error: ApplicationError) -> Self {
        let code = serde_json::to_value(error.code)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "INTERNAL".to_string());
        Self {
            code,
            message: error.message,
            recoverable: error.retryable,
            details: error.detail.map(|detail| json!({ "detail": detail })),
        }
    }
}

impl From<WorkerError> for ApiErrorV3 {
    fn from(error: WorkerError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            recoverable: error.recoverable,
            details: error.details,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionCreateParams {
    models_dir: PathBuf,
    #[serde(default)]
    quality_baselines_dir: Option<PathBuf>,
    #[serde(default)]
    provider_smoke_fixture: Option<PathBuf>,
    #[serde(default = "default_runtime_preference")]
    runtime_preference: String,
    #[serde(default = "default_parse_mode")]
    parse_mode: String,
    #[serde(default = "default_max_threads")]
    max_threads: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionParams {
    session_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WarmupParams {
    session_id: u64,
    #[serde(default = "default_profile")]
    profile: RecognitionProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecognizePathParams {
    session_id: u64,
    path: PathBuf,
    #[serde(default = "default_profile")]
    profile: RecognitionProfile,
    #[serde(default)]
    parse_mode: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    include_source_asset: bool,
    #[serde(default)]
    formats: Vec<String>,
}

#[derive(Debug)]
struct DispatchOutcome {
    data: Value,
    diagnostics: Vec<Diagnostic>,
    shutdown: bool,
}

impl DispatchOutcome {
    fn data(data: Value) -> Self {
        Self {
            data,
            diagnostics: Vec::new(),
            shutdown: false,
        }
    }
}

/// Process-local session registry. Requests on one stream are deliberately
/// serialized; callers that require parallel inference should run multiple
/// workers or open multiple independently supervised streams.
pub struct Worker {
    sessions: HashMap<u64, RecognitionSession>,
    next_session_id: u64,
    max_sessions: usize,
}

impl Default for Worker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS)
    }
}

impl Worker {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            next_session_id: 1,
            max_sessions: max_sessions.max(1),
        }
    }

    fn process_line(&mut self, line: &str) -> (WorkerResponse, bool) {
        let id = best_effort_id(line);
        let request = match parse_request(line) {
            Ok(request) => request,
            Err(error) => return (WorkerResponse::failure(id, error), false),
        };
        let id = request.id.clone();
        if request.version != JSONL_PROTOCOL_VERSION {
            let error = WorkerError::protocol(
                "UNSUPPORTED_PROTOCOL_VERSION",
                format!(
                    "Protocol version {} is not supported; expected {JSONL_PROTOCOL_VERSION}.",
                    request.version
                ),
            );
            return (WorkerResponse::failure(id, error), false);
        }
        match self.dispatch(request) {
            Ok(outcome) => {
                let shutdown = outcome.shutdown;
                (
                    WorkerResponse::success(id, outcome.data, outcome.diagnostics),
                    shutdown,
                )
            }
            Err(error) => (WorkerResponse::failure(id, error), false),
        }
    }

    fn dispatch(&mut self, request: WorkerRequest) -> Result<DispatchOutcome, WorkerError> {
        match request.action.as_str() {
            "session.create" => self.create_session(request.params),
            "session.health" => self.session_health(request.params),
            "session.capabilities" => self.session_capabilities(request.params),
            "session.warmup" => self.session_warmup(request.params),
            "session.recognizePath" => self.session_recognize_path(request.params),
            "session.reloadModels" => self.session_reload_models(request.params),
            "session.close" => self.close_session(request.params),
            "worker.status" => {
                require_empty_params(request.params)?;
                Ok(DispatchOutcome::data(json!({
                    "liveSessions": self.sessions.len(),
                    "maxSessions": self.max_sessions
                })))
            }
            "worker.shutdown" => {
                require_empty_params(request.params)?;
                self.close_all();
                Ok(DispatchOutcome {
                    data: json!({ "closed": true }),
                    diagnostics: Vec::new(),
                    shutdown: true,
                })
            }
            _ => Err(WorkerError::protocol(
                "UNKNOWN_ACTION",
                format!("Unknown worker action: {}", request.action),
            )),
        }
    }

    fn create_session(&mut self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        if self.sessions.len() >= self.max_sessions {
            return Err(WorkerError::protocol(
                "SESSION_LIMIT_REACHED",
                format!(
                    "The worker allows at most {} live sessions.",
                    self.max_sessions
                ),
            ));
        }
        let params: SessionCreateParams = decode_params(params)?;
        validate_non_empty_path(&params.models_dir, "modelsDir")?;
        if let Some(path) = &params.quality_baselines_dir {
            validate_non_empty_path(path, "qualityBaselinesDir")?;
        }
        if let Some(path) = &params.provider_smoke_fixture {
            validate_non_empty_path(path, "providerSmokeFixture")?;
        }
        if !(1..=MAX_THREADS).contains(&params.max_threads) {
            return Err(WorkerError::protocol(
                "INVALID_ARGUMENT",
                format!("maxThreads must be between 1 and {MAX_THREADS}."),
            ));
        }
        let runtime_preference = parse_runtime_preference(&params.runtime_preference)?;
        let parse_mode = parse_parse_mode(&params.parse_mode)?;
        let import_options = ImportOptions {
            max_input_size: MAX_INPUT_BYTES,
            ..ImportOptions::default()
        };
        let mut builder = RecognitionSession::builder()
            .models_dir(params.models_dir)
            .runtime_preference(runtime_preference)
            .parse_mode(parse_mode)
            .max_threads(params.max_threads)
            .import_options(import_options);
        if let Some(path) = params.quality_baselines_dir {
            builder = builder.quality_baselines_dir(path);
        }
        if let Some(path) = params.provider_smoke_fixture {
            builder = builder.provider_smoke_fixture(path);
        }
        let session = builder.build().map_err(WorkerError::from)?;
        let session_id = self.allocate_session_id()?;
        self.sessions.insert(session_id, session);
        Ok(DispatchOutcome::data(json!({ "sessionId": session_id })))
    }

    fn session_health(&self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: SessionParams = decode_params(params)?;
        let session = self.session(params.session_id)?;
        let report = session.health_check().map_err(WorkerError::from)?;
        Ok(DispatchOutcome::data(to_value(report)?))
    }

    fn session_capabilities(&self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: SessionParams = decode_params(params)?;
        let session = self.session(params.session_id)?;
        Ok(DispatchOutcome::data(to_value(session.capabilities())?))
    }

    fn session_warmup(&mut self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: WarmupParams = decode_params(params)?;
        let session = self.session_mut(params.session_id)?;
        let report = session.warmup(params.profile).map_err(WorkerError::from)?;
        Ok(DispatchOutcome::data(to_value(report)?))
    }

    fn session_reload_models(&mut self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: SessionParams = decode_params(params)?;
        let session = self.session_mut(params.session_id)?;
        let report =
            RecognitionIntegrationApi::reload_models(session).map_err(WorkerError::from)?;
        Ok(DispatchOutcome::data(to_value(report)?))
    }

    fn session_recognize_path(&mut self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: RecognizePathParams = decode_params(params)?;
        validate_non_empty_path(&params.path, "path")?;
        if params
            .timeout_ms
            .is_some_and(|value| value > MAX_TIMEOUT_MS)
        {
            return Err(WorkerError::protocol(
                "INVALID_ARGUMENT",
                format!("timeoutMs must not exceed {MAX_TIMEOUT_MS}."),
            ));
        }
        let formats = parse_output_formats(params.formats)?;
        let options = RecognitionOptions {
            parse_mode: params
                .parse_mode
                .as_deref()
                .map(parse_parse_mode)
                .transpose()?,
            include_source_asset: params.include_source_asset,
            timeout: params.timeout_ms.map(Duration::from_millis),
            strict: params.strict,
        };
        let request = RecognitionRequest::from_path(params.path)
            .with_profile(params.profile)
            .with_options(options);
        let session = self.session_mut(params.session_id)?;
        let result =
            RecognitionIntegrationApi::recognize(session, request).map_err(WorkerError::from)?;
        let diagnostics = result.diagnostics.clone();
        let data = recognition_result_value(&result, formats)?;
        Ok(DispatchOutcome {
            data,
            diagnostics,
            shutdown: false,
        })
    }

    fn close_session(&mut self, params: Value) -> Result<DispatchOutcome, WorkerError> {
        let params: SessionParams = decode_params(params)?;
        let already_closed = match self.sessions.remove(&params.session_id) {
            Some(mut session) => {
                session.close();
                false
            }
            None if params.session_id != 0 && params.session_id < self.next_session_id => true,
            None => return Err(session_not_found(params.session_id)),
        };
        Ok(DispatchOutcome::data(json!({
            "closed": true,
            "alreadyClosed": already_closed
        })))
    }

    fn session(&self, session_id: u64) -> Result<&RecognitionSession, WorkerError> {
        self.sessions
            .get(&session_id)
            .ok_or_else(|| session_not_found(session_id))
    }

    fn session_mut(&mut self, session_id: u64) -> Result<&mut RecognitionSession, WorkerError> {
        self.sessions
            .get_mut(&session_id)
            .ok_or_else(|| session_not_found(session_id))
    }

    fn allocate_session_id(&mut self) -> Result<u64, WorkerError> {
        let start = self.next_session_id;
        loop {
            let candidate = self.next_session_id;
            self.next_session_id = self.next_session_id.checked_add(1).unwrap_or(1);
            if candidate != 0 && !self.sessions.contains_key(&candidate) {
                return Ok(candidate);
            }
            if self.next_session_id == start {
                return Err(WorkerError::protocol(
                    "SESSION_ID_EXHAUSTED",
                    "No unused session identifier is available.",
                ));
            }
        }
    }

    fn close_all(&mut self) {
        for (_, mut session) in self.sessions.drain() {
            session.close();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Run the worker until EOF or a successful `worker.shutdown` request.
pub fn run<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> io::Result<()> {
    let mut worker = Worker::default();
    loop {
        let line = match read_bounded_line(&mut reader)? {
            None => break,
            Some(Ok(line)) => line,
            Some(Err(error)) => {
                write_response(&mut writer, &WorkerResponse::failure(Value::Null, error))?;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| worker.process_line(&line)));
        let (response, shutdown) = match outcome {
            Ok(outcome) => outcome,
            Err(_) => {
                worker.close_all();
                (
                    WorkerResponse::failure(
                        best_effort_id(&line),
                        WorkerError::protocol(
                            "INTERNAL",
                            "The request handler panicked; all worker sessions were closed.",
                        ),
                    ),
                    false,
                )
            }
        };
        write_response(&mut writer, &response)?;
        if shutdown {
            break;
        }
    }
    worker.close_all();
    Ok(())
}

fn write_response(writer: &mut impl Write, response: &WorkerResponse) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, response).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn read_bounded_line(reader: &mut impl BufRead) -> io::Result<Option<Result<String, WorkerError>>> {
    let mut bytes = Vec::new();
    let mut too_large = false;
    let mut observed_any = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if !observed_any {
                return Ok(None);
            }
            break;
        }
        observed_any = true;
        let chunk_len = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if !too_large {
            let remaining = MAX_REQUEST_LINE_BYTES.saturating_sub(bytes.len());
            if chunk_len <= remaining {
                bytes.extend_from_slice(&available[..chunk_len]);
            } else {
                bytes.extend_from_slice(&available[..remaining]);
                too_large = true;
            }
        }
        let found_newline = available[..chunk_len].last() == Some(&b'\n');
        reader.consume(chunk_len);
        if found_newline {
            break;
        }
    }
    if too_large {
        return Ok(Some(Err(WorkerError::protocol(
            "REQUEST_TOO_LARGE",
            format!("A JSONL request must not exceed {MAX_REQUEST_LINE_BYTES} bytes."),
        ))));
    }
    while bytes
        .last()
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
    {
        bytes.pop();
    }
    let line = match String::from_utf8(bytes) {
        Ok(line) => line,
        Err(error) => {
            return Ok(Some(Err(WorkerError::protocol(
                "INVALID_ENCODING",
                format!(
                    "The JSONL request was not valid UTF-8 near byte {}.",
                    error.utf8_error().valid_up_to()
                ),
            ))));
        }
    };
    Ok(Some(Ok(line)))
}

fn parse_request(line: &str) -> Result<WorkerRequest, WorkerError> {
    let request: WorkerRequest = serde_json::from_str(line).map_err(|error| {
        WorkerError::protocol("INVALID_REQUEST", "The request is not valid worker JSON.")
            .with_details(json!({ "detail": error.to_string() }))
    })?;
    if !matches!(request.id, Value::String(_) | Value::Number(_)) {
        return Err(WorkerError::protocol(
            "INVALID_REQUEST_ID",
            "id must be a JSON string or number.",
        ));
    }
    if request.action.trim().is_empty() {
        return Err(WorkerError::protocol(
            "INVALID_ACTION",
            "action must not be empty.",
        ));
    }
    if !request.params.is_object() {
        return Err(WorkerError::protocol(
            "INVALID_PARAMS",
            "params must be a JSON object.",
        ));
    }
    Ok(request)
}

fn best_effort_id(line: &str) -> Value {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| value.get("id").cloned())
        .filter(|value| matches!(value, Value::String(_) | Value::Number(_)))
        .unwrap_or(Value::Null)
}

fn decode_params<T: DeserializeOwned>(params: Value) -> Result<T, WorkerError> {
    serde_json::from_value(params).map_err(|error| {
        WorkerError::protocol("INVALID_PARAMS", "The action parameters are invalid.")
            .with_details(json!({ "detail": error.to_string() }))
    })
}

fn require_empty_params(params: Value) -> Result<(), WorkerError> {
    let object = params
        .as_object()
        .ok_or_else(|| WorkerError::protocol("INVALID_PARAMS", "params must be a JSON object."))?;
    if object.is_empty() {
        Ok(())
    } else {
        Err(WorkerError::protocol(
            "INVALID_PARAMS",
            "This action does not accept parameters.",
        ))
    }
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

fn default_runtime_preference() -> String {
    "auto".to_string()
}

fn default_parse_mode() -> String {
    "specialized".to_string()
}

const fn default_max_threads() -> usize {
    4
}

const fn default_profile() -> RecognitionProfile {
    RecognitionProfile::Formula
}

fn validate_non_empty_path(path: &Path, field: &str) -> Result<(), WorkerError> {
    if path.as_os_str().is_empty() {
        Err(WorkerError::protocol(
            "INVALID_ARGUMENT",
            format!("{field} must not be empty."),
        ))
    } else {
        Ok(())
    }
}

fn parse_runtime_preference(value: &str) -> Result<RuntimePreference, WorkerError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(RuntimePreference::Auto),
        "cpu" => Ok(RuntimePreference::Cpu),
        "gpu" => Ok(RuntimePreference::Gpu),
        _ => Err(WorkerError::protocol(
            "INVALID_ARGUMENT",
            "runtimePreference must be one of: auto, cpu, gpu.",
        )),
    }
}

fn parse_parse_mode(value: &str) -> Result<DocumentParseMode, WorkerError> {
    DocumentParseMode::from_label(value).ok_or_else(|| {
        WorkerError::protocol(
            "INVALID_ARGUMENT",
            "parseMode must be one of: specialized, openocr-text, opendoc-hybrid.",
        )
    })
}

fn parse_output_formats(formats: Vec<String>) -> Result<Vec<(String, OutputFormat)>, WorkerError> {
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::new();
    for value in formats {
        let key = value.trim().to_ascii_lowercase().replace('-', "_");
        let format = match key.as_str() {
            "latex" => OutputFormat::Latex,
            "latex_display" | "display_latex" => OutputFormat::LatexDisplay,
            "latex_equation" | "equation_latex" => OutputFormat::LatexEquation,
            "typst" => OutputFormat::Typst,
            "markdown_inline" => OutputFormat::MarkdownInline,
            "markdown" | "markdown_block" => OutputFormat::MarkdownBlock,
            "mathml" => OutputFormat::MathML,
            "omml" => OutputFormat::OMML,
            "html" => OutputFormat::Html,
            _ => {
                return Err(WorkerError::protocol(
                    "INVALID_ARGUMENT",
                    format!("Unsupported output format: {value}"),
                ));
            }
        };
        if seen.insert(key.clone()) {
            parsed.push((key, format));
        }
    }
    Ok(parsed)
}

fn recognition_result_value(
    result: &RecognitionResult,
    formats: Vec<(String, OutputFormat)>,
) -> Result<Value, WorkerError> {
    let mut value = to_value(result)?;
    let outputs = formats
        .into_iter()
        .map(|(name, format)| {
            result
                .to_format(format)
                .map(|output| (name, Value::String(output)))
                .map_err(WorkerError::from)
        })
        .collect::<Result<Map<String, Value>, WorkerError>>()?;
    value
        .as_object_mut()
        .ok_or_else(|| {
            WorkerError::protocol("INTERNAL", "The recognition result is not a JSON object.")
        })?
        .insert("outputs".to_string(), Value::Object(outputs));
    Ok(value)
}

fn to_value(value: impl Serialize) -> Result<Value, WorkerError> {
    serde_json::to_value(value).map_err(|error| {
        WorkerError::protocol("INTERNAL", "A Core result could not be serialized.")
            .with_details(json!({ "detail": error.to_string() }))
    })
}

fn session_not_found(session_id: u64) -> WorkerError {
    WorkerError::protocol(
        "SESSION_NOT_FOUND",
        format!("Recognition session {session_id} does not exist or is closed."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn request(id: impl Into<Value>, action: &str, params: Value) -> String {
        json!({
            "version": JSONL_PROTOCOL_VERSION,
            "id": id.into(),
            "action": action,
            "params": params
        })
        .to_string()
    }

    fn response_data(response: &WorkerResponse) -> &Value {
        assert!(response.envelope.ok, "{:?}", response.envelope.error);
        response.envelope.data.as_ref().unwrap()
    }

    #[test]
    fn session_lifecycle_reuses_warmup_and_close_is_idempotent() {
        let models = tempfile::tempdir().unwrap();
        let mut worker = Worker::new(2);
        let (create, shutdown) = worker.process_line(&request(
            "create",
            "session.create",
            json!({ "modelsDir": models.path() }),
        ));
        assert!(!shutdown);
        let session_id = response_data(&create)["sessionId"].as_u64().unwrap();

        let (first, _) = worker.process_line(&request(
            1,
            "session.warmup",
            json!({ "sessionId": session_id, "profile": "formula" }),
        ));
        let (second, _) = worker.process_line(&request(
            2,
            "session.warmup",
            json!({ "sessionId": session_id, "profile": "formula" }),
        ));
        assert_eq!(response_data(&first)["alreadyWarm"], false);
        assert_eq!(response_data(&second)["alreadyWarm"], true);

        let (close, _) = worker.process_line(&request(
            3,
            "session.close",
            json!({ "sessionId": session_id }),
        ));
        let (close_again, _) = worker.process_line(&request(
            4,
            "session.close",
            json!({ "sessionId": session_id }),
        ));
        assert_eq!(response_data(&close)["alreadyClosed"], false);
        assert_eq!(response_data(&close_again)["alreadyClosed"], true);

        let (unknown, _) = worker.process_line(&request(
            5,
            "session.close",
            json!({ "sessionId": session_id + 1000 }),
        ));
        assert_eq!(
            unknown.envelope.error.as_ref().unwrap().code,
            "SESSION_NOT_FOUND"
        );
    }

    #[test]
    fn independent_sessions_and_capacity_are_bounded() {
        let first_models = tempfile::tempdir().unwrap();
        let second_models = tempfile::tempdir().unwrap();
        let third_models = tempfile::tempdir().unwrap();
        let mut worker = Worker::new(2);
        let (first, _) = worker.process_line(&request(
            1,
            "session.create",
            json!({ "modelsDir": first_models.path() }),
        ));
        assert!(first.envelope.ok);
        let (second, _) = worker.process_line(&request(
            2,
            "session.create",
            json!({ "modelsDir": second_models.path() }),
        ));
        assert!(second.envelope.ok);
        assert_ne!(
            response_data(&first)["sessionId"],
            response_data(&second)["sessionId"]
        );
        let (third, _) = worker.process_line(&request(
            3,
            "session.create",
            json!({ "modelsDir": third_models.path() }),
        ));
        assert_eq!(
            third.envelope.error.as_ref().unwrap().code,
            "SESSION_LIMIT_REACHED"
        );
    }

    #[test]
    fn invalid_recognition_does_not_poison_the_session() {
        let models = tempfile::tempdir().unwrap();
        let missing = models.path().join("missing.png");
        let mut worker = Worker::default();
        let (create, _) = worker.process_line(&request(
            1,
            "session.create",
            json!({ "modelsDir": models.path() }),
        ));
        let session_id = response_data(&create)["sessionId"].as_u64().unwrap();
        let (failed, _) = worker.process_line(&request(
            2,
            "session.recognizePath",
            json!({ "sessionId": session_id, "path": missing }),
        ));
        assert_eq!(
            failed.envelope.error.as_ref().unwrap().code,
            "INVALID_INPUT"
        );

        let (health, _) = worker.process_line(&request(
            3,
            "session.health",
            json!({ "sessionId": session_id }),
        ));
        assert!(health.envelope.ok);
    }

    #[test]
    fn repeated_create_and_close_does_not_leak_registry_entries() {
        let models = tempfile::tempdir().unwrap();
        let mut worker = Worker::new(2);
        for request_id in 1..=100 {
            let (create, _) = worker.process_line(&request(
                request_id,
                "session.create",
                json!({ "modelsDir": models.path() }),
            ));
            let session_id = response_data(&create)["sessionId"].as_u64().unwrap();
            let (close, _) = worker.process_line(&request(
                request_id + 100,
                "session.close",
                json!({ "sessionId": session_id }),
            ));
            assert!(close.envelope.ok);
        }
        assert!(worker.sessions.is_empty());
    }

    #[test]
    fn protocol_errors_preserve_request_id() {
        let mut worker = Worker::default();
        let (wrong_version, _) = worker
            .process_line(r#"{"version":2,"id":"request-7","action":"worker.status","params":{}}"#);
        assert_eq!(wrong_version.id, "request-7");
        assert_eq!(
            wrong_version.envelope.error.as_ref().unwrap().code,
            "UNSUPPORTED_PROTOCOL_VERSION"
        );

        let (unknown, _) = worker.process_line(&request(8, "missing.action", json!({})));
        assert_eq!(unknown.id, 8);
        assert_eq!(
            unknown.envelope.error.as_ref().unwrap().code,
            "UNKNOWN_ACTION"
        );
    }

    #[test]
    fn jsonl_run_flushes_one_response_per_request_and_stops_on_shutdown() {
        let input = format!(
            "{}\n{}\n{}\n",
            request(1, "worker.status", json!({})),
            request(2, "worker.shutdown", json!({})),
            request(3, "worker.status", json!({}))
        );
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output).unwrap();
        let lines = String::from_utf8(output).unwrap();
        let responses = lines
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[0]["data"]["liveSessions"], 0);
        assert_eq!(responses[1]["id"], 2);
        assert_eq!(responses[1]["data"]["closed"], true);
    }

    #[test]
    fn oversized_line_is_drained_before_the_next_request() {
        let oversized = "x".repeat(MAX_REQUEST_LINE_BYTES + 1);
        let input = format!(
            "{oversized}\n{}\n{}\n",
            request(2, "worker.status", json!({})),
            request(3, "worker.shutdown", json!({}))
        );
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output).unwrap();
        let responses = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["error"]["code"], "REQUEST_TOO_LARGE");
        assert_eq!(responses[1]["id"], 2);
        assert_eq!(responses[1]["ok"], true);
    }

    #[test]
    fn invalid_utf8_is_reported_without_stopping_the_worker() {
        let mut input = vec![0xff, b'\n'];
        input.extend_from_slice(request(2, "worker.status", json!({})).as_bytes());
        input.push(b'\n');
        input.extend_from_slice(request(3, "worker.shutdown", json!({})).as_bytes());
        input.push(b'\n');
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output).unwrap();
        let responses = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["error"]["code"], "INVALID_ENCODING");
        assert_eq!(responses[1]["id"], 2);
        assert_eq!(responses[1]["ok"], true);
    }
}

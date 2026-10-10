//! Explicit, instance-local routing for trusted formula string converters.
//!
//! This is not a plugin loader or an OS sandbox. Untrusted handlers must be
//! adapted through the existing verified WASI host, not registered as Rust code.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

pub const CONVERSION_REGISTRY_VERSION: u16 = 1;
pub const BUILTIN_CONVERSION_BACKEND: &str = "core:builtin";
const MAX_INPUT: usize = 64 * 1024;
const MAX_OUTPUT: usize = 256 * 1024;
const MAX_METADATA: usize = 16 * 1024;
const MAX_ENTRIES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConversionRegistryErrorCode {
    InvalidDeclaration,
    VersionMismatch,
    DuplicateRegistration,
    UnknownFormat,
    UnknownBackend,
    BackendDisabled,
    RouteUnavailable,
    InputRejected,
    OutputRejected,
    LimitExceeded,
    BackendFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRegistryError {
    pub code: ConversionRegistryErrorCode,
    pub message: String,
}

impl ConversionRegistryError {
    pub fn new(code: ConversionRegistryErrorCode, message: impl Into<String>) -> Self {
        let message = message.into();
        let mut boundary = message.len().min(1024);
        while !message.is_char_boundary(boundary) {
            boundary -= 1;
        }
        Self {
            code,
            message: message[..boundary].to_string(),
        }
    }
}

impl std::fmt::Display for ConversionRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for ConversionRegistryError {}
type RegistryResult<T> = Result<T, ConversionRegistryError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionFormatDescriptor {
    pub schema_version: u16,
    pub id: String,
    pub mime_type: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionBackendRoute {
    pub input: String,
    pub output: String,
    pub mode: FormulaConversionMode,
    pub target: CapabilityTarget,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionBackendDescriptor {
    pub schema_version: u16,
    pub id: String,
    pub version: String,
    /// Must change when backend configuration or executable identity changes.
    pub configuration_sha256: String,
    pub license: String,
    pub limitations: Vec<String>,
    pub routes: Vec<ConversionBackendRoute>,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredConversionRequest {
    pub schema_version: u16,
    pub input: String,
    pub output: String,
    pub mode: FormulaConversionMode,
    /// None selects only the built-in backend, never an extension implicitly.
    pub backend: Option<String>,
    pub content: String,
    /// Caller fingerprint of Core build, definitions, fonts, styles and host context.
    pub context_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionLoss {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionCandidate {
    pub content: String,
    pub losses: Vec<ConversionLoss>,
}

/// A candidate is not a host write permit or proof of mathematical fidelity.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredConversionResult {
    pub schema_version: u16,
    pub core_version: String,
    pub input: String,
    pub output: String,
    pub mode: FormulaConversionMode,
    pub backend: String,
    pub backend_version: String,
    pub configuration_sha256: String,
    pub context_sha256: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub route_sha256: String,
    pub mime_type: String,
    pub limitations: Vec<String>,
    pub candidate: ConversionCandidate,
}

/// Trusted synchronous handler. This trait does not impose a hard deadline.
pub trait RegisteredFormulaBackend: Send + Sync {
    /// Adapters must recheck activation/trust and dependencies here on every call.
    fn check_ready(&self) -> RegistryResult<()>;
    fn convert(&self, request: &RegisteredConversionRequest)
        -> RegistryResult<ConversionCandidate>;
}

type FormatValidator = Arc<dyn Fn(&str) -> RegistryResult<()> + Send + Sync>;
struct RegisteredFormat {
    descriptor: ConversionFormatDescriptor,
    validator: FormatValidator,
}
struct RegisteredBackend {
    descriptor: ConversionBackendDescriptor,
    handler: Arc<dyn RegisteredFormulaBackend>,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredConversionCapability {
    pub route: ConversionBackendRoute,
    pub backend: String,
    pub backend_version: String,
    pub available: bool,
    pub limitations: Vec<String>,
    pub unavailable_reason: Option<ConversionRegistryErrorCode>,
}

/// Additive dispatch; existing OutputFormat and legacy entry points are unchanged.
pub struct SemanticConversionRegistry {
    target: CapabilityTarget,
    formats: BTreeMap<String, RegisteredFormat>,
    backends: BTreeMap<String, RegisteredBackend>,
}

impl Default for SemanticConversionRegistry {
    fn default() -> Self {
        Self::new(if cfg!(target_arch = "wasm32") {
            CapabilityTarget::Wasm32UnknownUnknown
        } else {
            CapabilityTarget::Native
        })
    }
}

impl SemanticConversionRegistry {
    pub fn new(target: CapabilityTarget) -> Self {
        Self {
            target,
            formats: BTreeMap::new(),
            backends: BTreeMap::new(),
        }
    }

    /// Register a namespaced UTF-8 format and a callable validation policy.
    /// Binary/container formats are outside this formula-string API.
    pub fn register_trusted_format(
        &mut self,
        descriptor: ConversionFormatDescriptor,
        validator: impl Fn(&str) -> RegistryResult<()> + Send + Sync + 'static,
    ) -> RegistryResult<()> {
        check_version(descriptor.schema_version)?;
        check_metadata(&descriptor)?;
        if !extension_id(&descriptor.id)
            || descriptor.mime_type.is_empty()
            || descriptor.mime_type.len() > 128
            || descriptor.mime_type.chars().any(char::is_control)
            || !descriptor.mime_type.contains('/')
            || descriptor.description.trim().is_empty()
        {
            return Err(error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "invalid namespaced format or MIME description",
            ));
        }
        if self.formats.contains_key(&descriptor.id) {
            return Err(error(
                ConversionRegistryErrorCode::DuplicateRegistration,
                "format is already registered",
            ));
        }
        if self.formats.len() >= MAX_ENTRIES {
            return Err(error(
                ConversionRegistryErrorCode::LimitExceeded,
                "format registry is full",
            ));
        }
        self.formats.insert(
            descriptor.id.clone(),
            RegisteredFormat {
                descriptor,
                validator: Arc::new(validator),
            },
        );
        Ok(())
    }

    /// No loader, network access or implicit trust grant occurs here.
    pub fn register_trusted_backend(
        &mut self,
        descriptor: ConversionBackendDescriptor,
        handler: impl RegisteredFormulaBackend + 'static,
    ) -> RegistryResult<()> {
        check_version(descriptor.schema_version)?;
        check_metadata(&descriptor)?;
        if !extension_id(&descriptor.id)
            || descriptor.version.trim().is_empty()
            || descriptor.version.len() > 128
            || descriptor.version.chars().any(char::is_control)
            || !sha256_id(&descriptor.configuration_sha256)
            || descriptor.license.trim().is_empty()
            || descriptor.routes.is_empty()
            || descriptor.routes.len() > MAX_ENTRIES
            || descriptor.max_input_bytes == 0
            || descriptor.max_input_bytes > MAX_INPUT
            || descriptor.max_output_bytes == 0
            || descriptor.max_output_bytes > MAX_OUTPUT
            || descriptor
                .limitations
                .iter()
                .any(|item| item.trim().is_empty())
        {
            return Err(error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "invalid backend identity, configuration, routes or budgets",
            ));
        }
        for (index, route) in descriptor.routes.iter().enumerate() {
            self.require_format(&route.input, true)?;
            self.require_format(&route.output, false)?;
            if descriptor.routes[..index].contains(route) {
                return Err(error(
                    ConversionRegistryErrorCode::DuplicateRegistration,
                    "duplicate backend route",
                ));
            }
            // Strict still means the existing validated LaTeX -> OMML subset.
            if route.mode == FormulaConversionMode::Strict
                && (route.input != "latex" || route.output != "omml")
            {
                return Err(error(
                    ConversionRegistryErrorCode::InvalidDeclaration,
                    "strict extension routes require LaTeX to OMML",
                ));
            }
        }
        if self.backends.contains_key(&descriptor.id) {
            return Err(error(
                ConversionRegistryErrorCode::DuplicateRegistration,
                "backend is already registered",
            ));
        }
        if self.backends.len() >= MAX_ENTRIES {
            return Err(error(
                ConversionRegistryErrorCode::LimitExceeded,
                "backend registry is full",
            ));
        }
        self.backends.insert(
            descriptor.id.clone(),
            RegisteredBackend {
                descriptor,
                handler: Arc::new(handler),
                enabled: false,
            },
        );
        Ok(())
    }

    pub fn set_backend_enabled(&mut self, id: &str, enabled: bool) -> RegistryResult<()> {
        let backend = self.backends.get_mut(id).ok_or_else(|| {
            error(
                ConversionRegistryErrorCode::UnknownBackend,
                "unknown extension backend",
            )
        })?;
        if enabled {
            backend.handler.check_ready().map_err(bound_failure)?;
        }
        backend.enabled = enabled;
        Ok(())
    }

    pub fn extension_formats(&self) -> Vec<ConversionFormatDescriptor> {
        self.formats
            .values()
            .map(|entry| entry.descriptor.clone())
            .collect()
    }

    pub fn capabilities(&self) -> Vec<RegisteredConversionCapability> {
        let mut capabilities: Vec<_> = CapabilityRegistry::formula_conversions(self.target)
            .into_iter()
            .map(|entry| RegisteredConversionCapability {
                route: ConversionBackendRoute {
                    input: entry.input.name().into(),
                    output: entry.output.into(),
                    mode: entry.mode,
                    target: entry.target,
                },
                backend: BUILTIN_CONVERSION_BACKEND.into(),
                backend_version: env!("CARGO_PKG_VERSION").into(),
                available: entry.available,
                limitations: entry
                    .limitations
                    .iter()
                    .map(|item| (*item).into())
                    .collect(),
                unavailable_reason: (!entry.available)
                    .then_some(ConversionRegistryErrorCode::RouteUnavailable),
            })
            .collect();
        for entry in self.backends.values() {
            let readiness = if entry.enabled {
                entry
                    .handler
                    .check_ready()
                    .err()
                    .map(|failure| failure.code)
            } else {
                Some(ConversionRegistryErrorCode::BackendDisabled)
            };
            for route in &entry.descriptor.routes {
                let reason = readiness.or_else(|| {
                    (route.target != self.target)
                        .then_some(ConversionRegistryErrorCode::RouteUnavailable)
                });
                capabilities.push(RegisteredConversionCapability {
                    route: route.clone(),
                    backend: entry.descriptor.id.clone(),
                    backend_version: entry.descriptor.version.clone(),
                    available: reason.is_none(),
                    limitations: entry.descriptor.limitations.clone(),
                    unavailable_reason: reason,
                });
            }
        }
        capabilities
    }

    pub fn convert(
        &self,
        request: &RegisteredConversionRequest,
    ) -> RegistryResult<RegisteredConversionResult> {
        check_version(request.schema_version)?;
        if !sha256_id(&request.context_sha256)
            || request.content.trim().is_empty()
            || request.input.len() > 96
            || request.output.len() > 96
            || request.backend.as_ref().is_some_and(|id| id.len() > 96)
        {
            return Err(error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "context fingerprint or content is missing",
            ));
        }
        self.require_format(&request.input, true)?;
        self.require_format(&request.output, false)?;
        if request.content.len() > MAX_INPUT {
            return Err(error(
                ConversionRegistryErrorCode::LimitExceeded,
                "formula input exceeds registry budget",
            ));
        }
        self.validate_format(&request.input, &request.content, true)?;
        let id = request
            .backend
            .as_deref()
            .unwrap_or(BUILTIN_CONVERSION_BACKEND);
        let route = ConversionBackendRoute {
            input: request.input.clone(),
            output: request.output.clone(),
            mode: request.mode,
            target: self.target,
        };
        let (candidate, version, configuration, limitations, output_budget) = if id
            == BUILTIN_CONVERSION_BACKEND
        {
            let input = builtin_input(&request.input).ok_or_else(|| {
                error(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    "built-in backend does not handle extension inputs",
                )
            })?;
            let output = builtin_output(&request.output).ok_or_else(|| {
                error(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    "built-in backend does not handle extension outputs",
                )
            })?;
            let capability =
                CapabilityRegistry::formula_conversion(input, output, request.mode, self.target);
            if !capability.available {
                return Err(error(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    capability
                        .unavailable_reason
                        .unwrap_or("built-in route unavailable"),
                ));
            }
            let content = DocumentConverter::convert_formula_string(
                &request.content,
                input,
                output,
                request.mode,
            )
            .map_err(|failure| {
                error(
                    ConversionRegistryErrorCode::InputRejected,
                    failure.to_string(),
                )
            })?;
            let losses = if request.mode == FormulaConversionMode::BestEffort {
                vec![ConversionLoss { code: "UNVERIFIED_BEST_EFFORT".into(), message: "Unsupported syntax/style may be lost; acceptance is not a fidelity guarantee".into() }]
            } else {
                Vec::new()
            };
            (
                ConversionCandidate { content, losses },
                env!("CARGO_PKG_VERSION").to_string(),
                digest(b"builtin-default"),
                capability
                    .limitations
                    .iter()
                    .map(|item| (*item).into())
                    .collect(),
                MAX_OUTPUT,
            )
        } else {
            let backend = self.backends.get(id).ok_or_else(|| {
                error(
                    ConversionRegistryErrorCode::UnknownBackend,
                    "unknown extension backend",
                )
            })?;
            if !backend.enabled {
                return Err(error(
                    ConversionRegistryErrorCode::BackendDisabled,
                    "extension backend is disabled",
                ));
            }
            if !backend.descriptor.routes.contains(&route) {
                return Err(error(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    "backend does not declare this route/platform/mode",
                ));
            }
            if request.content.len() > backend.descriptor.max_input_bytes {
                return Err(error(
                    ConversionRegistryErrorCode::LimitExceeded,
                    "backend input budget exceeded",
                ));
            }
            if request.mode == FormulaConversionMode::Strict {
                crate::omml::validate_omml_latex(&request.content).map_err(|failure| {
                    error(ConversionRegistryErrorCode::InputRejected, failure)
                })?;
            }
            backend.handler.check_ready().map_err(bound_failure)?;
            let candidate = backend.handler.convert(request).map_err(bound_failure)?;
            // No fallback or retry: a failing selected backend preserves the caller's source.
            (
                candidate,
                backend.descriptor.version.clone(),
                backend.descriptor.configuration_sha256.clone(),
                backend.descriptor.limitations.clone(),
                backend.descriptor.max_output_bytes,
            )
        };
        if candidate.content.trim().is_empty() || candidate.content.len() > output_budget {
            return Err(error(
                ConversionRegistryErrorCode::LimitExceeded,
                "empty or oversized conversion output",
            ));
        }
        check_metadata(&candidate.losses)?;
        if candidate.losses.len() > 32
            || candidate
                .losses
                .iter()
                .any(|loss| loss.code.trim().is_empty() || loss.message.trim().is_empty())
        {
            return Err(error(
                ConversionRegistryErrorCode::OutputRejected,
                "invalid loss diagnostics",
            ));
        }
        self.validate_format(&request.output, &candidate.content, false)?;
        let backend_declaration = self.backends.get(id).map(|backend| &backend.descriptor);
        let input_declaration = self
            .formats
            .get(&request.input)
            .map(|format| &format.descriptor);
        let output_declaration = self
            .formats
            .get(&request.output)
            .map(|format| &format.descriptor);
        let route_bytes = serde_json::to_vec(&(
            CONVERSION_REGISTRY_VERSION,
            env!("CARGO_PKG_VERSION"),
            &route,
            id,
            &version,
            &configuration,
            &request.context_sha256,
            digest(request.content.as_bytes()),
            backend_declaration,
            input_declaration,
            output_declaration,
        ))
        .map_err(|failure| {
            error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                failure.to_string(),
            )
        })?;
        Ok(RegisteredConversionResult {
            schema_version: CONVERSION_REGISTRY_VERSION,
            core_version: env!("CARGO_PKG_VERSION").into(),
            input: request.input.clone(),
            output: request.output.clone(),
            mode: request.mode,
            backend: id.into(),
            backend_version: version,
            configuration_sha256: configuration,
            context_sha256: request.context_sha256.clone(),
            input_sha256: digest(request.content.as_bytes()),
            output_sha256: digest(candidate.content.as_bytes()),
            route_sha256: digest(&route_bytes),
            mime_type: self
                .formats
                .get(&request.output)
                .map(|format| format.descriptor.mime_type.clone())
                .unwrap_or_else(|| {
                    crate::semantic_mime_type(
                        builtin_output(&request.output).expect("validated built-in output"),
                    )
                    .into()
                }),
            limitations,
            candidate,
        })
    }

    fn require_format(&self, id: &str, input: bool) -> RegistryResult<()> {
        if self.formats.contains_key(id)
            || if input {
                builtin_input(id).is_some()
            } else {
                builtin_output(id).is_some()
            }
        {
            Ok(())
        } else {
            Err(error(
                ConversionRegistryErrorCode::UnknownFormat,
                "format ID is not registered in this direction",
            ))
        }
    }

    fn validate_format(&self, id: &str, content: &str, input: bool) -> RegistryResult<()> {
        let failure_code = if input {
            ConversionRegistryErrorCode::InputRejected
        } else {
            ConversionRegistryErrorCode::OutputRejected
        };
        if let Some(format) = self.formats.get(id) {
            return (format.validator)(content)
                .map_err(|failure| error(failure_code, failure.message));
        }
        if matches!(id, "mathml" | "omml") {
            validate_math_xml(content, id)
                .map_err(|failure| error(failure_code, failure.message))?;
        }
        Ok(())
    }
}

/// Route through an explicitly supplied registry without changing legacy dispatch.
impl DocumentConverter {
    pub fn convert_registered_formula(
        registry: &SemanticConversionRegistry,
        request: &RegisteredConversionRequest,
    ) -> RegistryResult<RegisteredConversionResult> {
        registry.convert(request)
    }
}

fn builtin_input(id: &str) -> Option<FormulaInputFormat> {
    FormulaInputFormat::all()
        .iter()
        .copied()
        .find(|format| format.name() == id)
}
fn builtin_output(id: &str) -> Option<OutputFormat> {
    OutputFormat::all()
        .iter()
        .copied()
        .find(|format| format.name() == id)
}
fn extension_id(id: &str) -> bool {
    let Some((namespace, name)) = id.split_once(':') else {
        return false;
    };
    id.len() <= 96
        && namespace != "core"
        && [namespace, name].iter().all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}
fn sha256_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn error(code: ConversionRegistryErrorCode, message: impl Into<String>) -> ConversionRegistryError {
    ConversionRegistryError::new(code, message)
}
fn bound_failure(failure: ConversionRegistryError) -> ConversionRegistryError {
    error(failure.code, failure.message)
}
fn check_version(version: u16) -> RegistryResult<()> {
    if version == CONVERSION_REGISTRY_VERSION {
        Ok(())
    } else {
        Err(error(
            ConversionRegistryErrorCode::VersionMismatch,
            "unsupported conversion registry version",
        ))
    }
}
fn check_metadata(value: &impl Serialize) -> RegistryResult<()> {
    let bytes = serde_json::to_vec(value).map_err(|failure| {
        error(
            ConversionRegistryErrorCode::InvalidDeclaration,
            failure.to_string(),
        )
    })?;
    if bytes.len() > MAX_METADATA {
        Err(error(
            ConversionRegistryErrorCode::LimitExceeded,
            "registry metadata exceeds budget",
        ))
    } else {
        Ok(())
    }
}

fn validate_math_xml(content: &str, format: &str) -> RegistryResult<()> {
    use quick_xml::events::Event;
    use quick_xml::name::ResolveResult;
    let mut reader = quick_xml::NsReader::from_str(content);
    let namespace = if format == "omml" {
        b"http://schemas.openxmlformats.org/officeDocument/2006/math".as_slice()
    } else {
        b"http://www.w3.org/1998/Math/MathML".as_slice()
    };
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut nodes = 0usize;
    let mut owners: Vec<(Vec<u8>, bool)> = Vec::new();
    loop {
        let (resolved, event) = reader.read_resolved_event().map_err(|failure| {
            error(
                ConversionRegistryErrorCode::OutputRejected,
                failure.to_string(),
            )
        })?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                let word_property = format == "omml"
                    && matches!(resolved, ResolveResult::Bound(ref value) if value.as_ref() == b"http://schemas.openxmlformats.org/wordprocessingml/2006/main");
                if !word_property
                    && !matches!(resolved, ResolveResult::Bound(ref value) if value.as_ref() == namespace)
                {
                    return Err(error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "math XML element has an invalid or missing namespace",
                    ));
                }
                let local = element.local_name();
                let local = local.as_ref();
                if depth == 0 {
                    roots += 1;
                    if roots != 1
                        || if format == "omml" {
                            !matches!(local, b"oMath" | b"oMathPara")
                        } else {
                            local != b"math"
                        }
                    {
                        return Err(error(
                            ConversionRegistryErrorCode::OutputRejected,
                            "math XML root does not match the declared format",
                        ));
                    }
                }
                if !allowed_math_element(local, format, word_property) {
                    return Err(error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "active or external math XML content is not accepted",
                    ));
                }
                if word_property {
                    let valid_parent = owners.last().is_some_and(|(parent, word)| {
                        if local == b"rPr" {
                            !*word && matches!(parent.as_slice(), b"r" | b"ctrlPr")
                        } else {
                            *word && parent == b"rPr"
                        }
                    });
                    if !valid_parent {
                        return Err(error(
                            ConversionRegistryErrorCode::OutputRejected,
                            "Word run property has an invalid math owner",
                        ));
                    }
                }
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|failure| {
                        error(
                            ConversionRegistryErrorCode::OutputRejected,
                            failure.to_string(),
                        )
                    })?;
                    let name = attribute.key.local_name();
                    let normalized_name = name.as_ref().to_ascii_lowercase();
                    let name = normalized_name.as_slice();
                    if name.starts_with(b"on")
                        || matches!(
                            name,
                            b"href" | b"src" | b"style" | b"altimg" | b"definitionurl" | b"base"
                        )
                    {
                        return Err(error(
                            ConversionRegistryErrorCode::OutputRejected,
                            "active or external math XML attribute is not accepted",
                        ));
                    }
                    attribute
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(|failure| {
                            error(
                                ConversionRegistryErrorCode::OutputRejected,
                                failure.to_string(),
                            )
                        })?;
                }
                nodes += 1;
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    owners.push((local.to_vec(), word_property));
                }
                if depth > 64 || nodes > 4096 {
                    return Err(error(
                        ConversionRegistryErrorCode::LimitExceeded,
                        "math XML structural budget exceeded",
                    ));
                }
            }
            Event::End(_) => {
                owners.pop();
                depth = depth.checked_sub(1).ok_or_else(|| {
                    error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "unexpected XML end",
                    )
                })?;
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err(error(
                    ConversionRegistryErrorCode::OutputRejected,
                    "XML DTD or processing instruction is forbidden",
                ))
            }
            Event::GeneralRef(reference) => {
                if depth == 0 {
                    return Err(error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "XML reference outside math root",
                    ));
                }
                crate::xml_util::decode_xml_reference(&reference).map_err(|failure| {
                    error(ConversionRegistryErrorCode::OutputRejected, failure)
                })?;
            }
            Event::Text(text) => {
                let value = text.decode().map_err(|failure| {
                    error(
                        ConversionRegistryErrorCode::OutputRejected,
                        failure.to_string(),
                    )
                })?;
                if value.chars().any(|ch| !valid_xml_char(ch))
                    || (depth == 0 && !value.trim().is_empty())
                {
                    return Err(error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "text outside math XML root",
                    ));
                }
            }
            Event::CData(text) => {
                if depth == 0
                    || text
                        .decode()
                        .map_err(|failure| {
                            error(
                                ConversionRegistryErrorCode::OutputRejected,
                                failure.to_string(),
                            )
                        })?
                        .chars()
                        .any(|ch| !valid_xml_char(ch))
                {
                    return Err(error(
                        ConversionRegistryErrorCode::OutputRejected,
                        "invalid math XML CDATA",
                    ));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || roots != 1 {
        return Err(error(
            ConversionRegistryErrorCode::OutputRejected,
            "incomplete math XML document",
        ));
    }
    Ok(())
}

fn valid_xml_char(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r')
        || matches!(ch as u32, 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn allowed_math_element(name: &[u8], format: &str, word: bool) -> bool {
    if word {
        return matches!(
            name,
            b"rPr"
                | b"color"
                | b"sz"
                | b"rFonts"
                | b"b"
                | b"i"
                | b"bCs"
                | b"iCs"
                | b"szCs"
                | b"lang"
        );
    }
    if format == "mathml" {
        matches!(
            name,
            b"math"
                | b"mrow"
                | b"mi"
                | b"mn"
                | b"mo"
                | b"mtext"
                | b"ms"
                | b"mspace"
                | b"mfrac"
                | b"msqrt"
                | b"mroot"
                | b"msup"
                | b"msub"
                | b"msubsup"
                | b"munder"
                | b"mover"
                | b"munderover"
                | b"mfenced"
                | b"mtable"
                | b"mtr"
                | b"mtd"
                | b"mlabeledtr"
                | b"menclose"
                | b"mstyle"
                | b"mpadded"
                | b"mphantom"
                | b"mmultiscripts"
                | b"mprescripts"
                | b"none"
                | b"semantics"
                | b"annotation"
        )
    } else {
        matches!(
            name,
            b"oMath"
                | b"oMathPara"
                | b"oMathParaPr"
                | b"jc"
                | b"r"
                | b"rPr"
                | b"t"
                | b"e"
                | b"f"
                | b"fPr"
                | b"num"
                | b"den"
                | b"type"
                | b"d"
                | b"dPr"
                | b"begChr"
                | b"endChr"
                | b"sepChr"
                | b"grow"
                | b"shp"
                | b"m"
                | b"mPr"
                | b"mr"
                | b"mcs"
                | b"mc"
                | b"mcPr"
                | b"mcJc"
                | b"count"
                | b"sSub"
                | b"sSubPr"
                | b"sSup"
                | b"sSupPr"
                | b"sSubSup"
                | b"sSubSupPr"
                | b"sub"
                | b"sup"
                | b"rad"
                | b"radPr"
                | b"deg"
                | b"degHide"
                | b"nary"
                | b"naryPr"
                | b"chr"
                | b"limLoc"
                | b"subHide"
                | b"supHide"
                | b"limLow"
                | b"limLowPr"
                | b"limUpp"
                | b"limUppPr"
                | b"lim"
                | b"func"
                | b"funcPr"
                | b"fName"
                | b"acc"
                | b"accPr"
                | b"bar"
                | b"barPr"
                | b"pos"
                | b"borderBox"
                | b"borderBoxPr"
                | b"box"
                | b"boxPr"
                | b"groupChr"
                | b"groupChrPr"
                | b"eqArr"
                | b"eqArrPr"
                | b"sty"
                | b"scr"
                | b"nor"
                | b"ctrlPr"
                | b"brk"
                | b"aln"
                | b"lit"
                | b"spacing"
                | b"eqAr"
                | b"eqNum"
                | b"mRow"
        )
    }
}

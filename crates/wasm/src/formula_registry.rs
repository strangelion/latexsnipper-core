//! Trusted synchronous JavaScript adapters, not a browser WASI sandbox/loader.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use js_sys::Function;
use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_conversion::conversion_registry::{
    ConversionBackendDescriptor, ConversionCandidate, ConversionFallbackPolicy,
    ConversionFormatDescriptor, ConversionLoss, ConversionRegistryError,
    ConversionRegistryErrorCode, RegisteredConversionRequest, RegisteredFormulaBackend,
    SemanticConversionRegistry,
};
use latexsnipper_conversion::CapabilityTarget;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::formula_api::serialize_v3_to_js;
type Result<T> = std::result::Result<T, ConversionRegistryError>;
const MAX_CALLBACKS: usize = 64 * 384;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;

thread_local! {
    static CALLBACKS: RefCell<BTreeMap<u32, Function>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_CALLBACK: Cell<u32> = const { Cell::new(0) };
    static INSTANCES: Cell<u32> = const { Cell::new(0) };
}

fn failure(code: ConversionRegistryErrorCode, message: &str) -> ConversionRegistryError {
    ConversionRegistryError::new(code, message)
}

fn parse<T: DeserializeOwned>(text: &str, limit: usize) -> Result<T> {
    if text.len() > limit {
        return Err(failure(
            ConversionRegistryErrorCode::LimitExceeded,
            "JSON transfer budget exceeded",
        ));
    }
    serde_json::from_str(text).map_err(|_| {
        failure(
            ConversionRegistryErrorCode::InvalidDeclaration,
            "Expected a closed registered-conversion JSON declaration",
        )
    })
}

fn envelope<T: Serialize>(result: Result<T>) -> JsValue {
    let result = result.and_then(|value| {
        if serde_json::to_vec(&value)
            .map_err(|_| {
                failure(
                    ConversionRegistryErrorCode::BackendFailed,
                    "Result serialization failed",
                )
            })?
            .len()
            > MAX_RESPONSE
        {
            Err(failure(
                ConversionRegistryErrorCode::LimitExceeded,
                "Serialized result exceeds 2 MiB",
            ))
        } else {
            Ok(value)
        }
    });
    match result {
        Ok(data) => serialize_v3_to_js(&ApiEnvelopeV3::success(data, Vec::new())),
        Err(error) => {
            let code = serde_json::to_value(error.code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "BACKEND_FAILED".into());
            serialize_v3_to_js(&ApiEnvelopeV3::<Value>::failure(
                ApiErrorV3 {
                    code,
                    message: error.message,
                    recoverable: false,
                    details: None,
                },
                Vec::new(),
            ))
        }
    }
}

fn install(function: Function) -> Result<u32> {
    CALLBACKS.with(|callbacks| {
        let mut callbacks = callbacks.borrow_mut();
        if callbacks.len() >= MAX_CALLBACKS {
            return Err(failure(
                ConversionRegistryErrorCode::LimitExceeded,
                "Callback capacity reached",
            ));
        }
        let id = NEXT_CALLBACK.with(|next| {
            let id = next.get().checked_add(1).ok_or_else(|| {
                failure(
                    ConversionRegistryErrorCode::LimitExceeded,
                    "Callback identifier space exhausted",
                )
            })?;
            next.set(id);
            Ok(id)
        })?;
        callbacks.insert(id, function);
        Ok(id)
    })
}

fn remove(ids: &[u32]) {
    CALLBACKS.with(|callbacks| {
        let mut callbacks = callbacks.borrow_mut();
        for id in ids {
            callbacks.remove(id);
        }
    });
}

fn invoke(id: u32, argument: &JsValue) -> Result<JsValue> {
    let function = CALLBACKS
        .with(|callbacks| callbacks.borrow().get(&id).cloned())
        .ok_or_else(|| {
            failure(
                ConversionRegistryErrorCode::BackendFailed,
                "Trusted callback was released",
            )
        })?;
    // Never hold the callback-map borrow across user code or read exception objects.
    function.call1(&JsValue::NULL, argument).map_err(|_| {
        failure(
            ConversionRegistryErrorCode::BackendFailed,
            "Trusted synchronous callback threw an exception",
        )
    })
}

fn validate_callback(id: u32, content: &str) -> Result<()> {
    if invoke(id, &JsValue::from_str(content))?.as_bool() == Some(true) {
        Ok(())
    } else {
        Err(failure(
            ConversionRegistryErrorCode::InputRejected,
            "Trusted validator must synchronously return true",
        ))
    }
}

struct JsBackend {
    ready: u32,
    convert: u32,
}
impl RegisteredFormulaBackend for JsBackend {
    fn check_ready(&self) -> Result<()> {
        if invoke(self.ready, &JsValue::UNDEFINED)?.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(failure(
                ConversionRegistryErrorCode::BackendFailed,
                "Trusted backend readiness is not synchronously true",
            ))
        }
    }
    fn convert(&self, request: &RegisteredConversionRequest) -> Result<ConversionCandidate> {
        let request = serde_json::to_string(request).map_err(|_| {
            failure(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "Request serialization failed",
            )
        })?;
        let text = invoke(self.convert, &JsValue::from_str(&request))?
            .as_string()
            .ok_or_else(|| {
                failure(
                    ConversionRegistryErrorCode::OutputRejected,
                    "Trusted converter must synchronously return a candidate JSON string",
                )
            })?;
        parse_candidate(&text)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    content: String,
    losses: Vec<Loss>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Loss {
    code: String,
    message: String,
}

fn parse_candidate(text: &str) -> Result<ConversionCandidate> {
    let candidate: Candidate = parse(text, 512 * 1024).map_err(|error| {
        if error.code == ConversionRegistryErrorCode::InvalidDeclaration {
            failure(
                ConversionRegistryErrorCode::OutputRejected,
                "Trusted converter returned an invalid closed candidate JSON string",
            )
        } else {
            error
        }
    })?;
    Ok(ConversionCandidate {
        content: candidate.content,
        losses: candidate
            .losses
            .into_iter()
            .map(|loss| ConversionLoss {
                code: loss.code,
                message: loss.message,
            })
            .collect(),
    })
}

struct Active<'a>(&'a Cell<bool>);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Browser/Node instance-local trusted JS route registry. No code is fetched.
#[wasm_bindgen]
pub struct FormulaRegistryWasm {
    registry: RefCell<Option<SemanticConversionRegistry>>,
    callbacks: RefCell<Vec<u32>>,
    busy: Cell<bool>,
    closed: Cell<bool>,
    // Callback identifiers belong to one JS agent; prohibit moving the instance
    // across Rust threads where an unrelated thread-local callback may share an ID.
    agent: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl FormulaRegistryWasm {
    fn enter(&self) -> Result<Active<'_>> {
        if self.closed.get() {
            return Err(failure(
                ConversionRegistryErrorCode::BackendFailed,
                "Formula registry is closed",
            ));
        }
        if self.busy.replace(true) {
            return Err(failure(
                ConversionRegistryErrorCode::BackendFailed,
                "Formula registry is busy; recursive calls are refused",
            ));
        }
        Ok(Active(&self.busy))
    }

    fn cleanup(&self) {
        if self.closed.get() && !self.busy.get() {
            self.registry.borrow_mut().take();
            remove(&self.callbacks.take());
        }
    }

    fn run<T: Serialize>(
        &self,
        operation: impl FnOnce(&mut SemanticConversionRegistry) -> Result<T>,
    ) -> JsValue {
        let result = (|| {
            let active = self.enter()?;
            let result = {
                let mut registry = self.registry.borrow_mut();
                operation(registry.as_mut().ok_or_else(|| {
                    failure(
                        ConversionRegistryErrorCode::BackendFailed,
                        "Formula registry is closed",
                    )
                })?)
            };
            drop(active);
            if self.closed.get() {
                return Err(failure(
                    ConversionRegistryErrorCode::BackendFailed,
                    "Registry closed during the operation; candidate discarded",
                ));
            }
            result
        })();
        self.cleanup();
        envelope(result)
    }
}

#[wasm_bindgen]
impl FormulaRegistryWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> std::result::Result<FormulaRegistryWasm, JsValue> {
        let allowed = INSTANCES.with(|instances| {
            if instances.get() >= 64 {
                false
            } else {
                instances.set(instances.get() + 1);
                true
            }
        });
        if !allowed {
            return Err(JsValue::from_str(
                "At most 64 formula registries are allowed",
            ));
        }
        Ok(Self {
            registry: RefCell::new(Some(SemanticConversionRegistry::new(
                CapabilityTarget::Wasm32UnknownUnknown,
            ))),
            callbacks: RefCell::new(Vec::new()),
            busy: Cell::new(false),
            closed: Cell::new(false),
            agent: std::marker::PhantomData,
        })
    }

    pub fn register_format(&self, declaration_json: &str, validator: Function) -> JsValue {
        self.run(|registry| {
            let declaration: ConversionFormatDescriptor = parse(declaration_json, 16 * 1024)?;
            let id = install(validator)?;
            if let Err(error) = registry
                .register_trusted_format(declaration, move |content| validate_callback(id, content))
            {
                remove(&[id]);
                return Err(error);
            }
            self.callbacks.borrow_mut().push(id);
            Ok(true)
        })
    }

    pub fn register_backend(
        &self,
        declaration_json: &str,
        ready: Function,
        convert: Function,
    ) -> JsValue {
        self.run(|registry| {
            let declaration: ConversionBackendDescriptor = parse(declaration_json, 16 * 1024)?;
            let ready = install(ready)?;
            let convert = match install(convert) {
                Ok(id) => id,
                Err(error) => {
                    remove(&[ready]);
                    return Err(error);
                }
            };
            if let Err(error) =
                registry.register_trusted_backend(declaration, JsBackend { ready, convert })
            {
                remove(&[ready, convert]);
                return Err(error);
            }
            self.callbacks.borrow_mut().extend([ready, convert]);
            Ok(true)
        })
    }

    pub fn set_enabled(&self, backend: &str, enabled: bool) -> JsValue {
        self.run(|registry| {
            registry.set_backend_enabled(backend, enabled)?;
            Ok(true)
        })
    }

    pub fn capabilities(&self) -> JsValue {
        self.run(|registry| {
            Ok(
                json!({"schemaVersion":1,"coreVersion":env!("CARGO_PKG_VERSION"),
            "formats":registry.extension_formats(),"routes":registry.capabilities(),
            "executionClass":"trusted-synchronous-js","browserWasiSandbox":false}),
            )
        })
    }

    pub fn convert(&self, request_json: &str, policy_json: &str) -> JsValue {
        self.run(|registry| {
            let request: RegisteredConversionRequest = parse(request_json, 1024 * 1024)?;
            let policy: ConversionFallbackPolicy = parse(policy_json, 4096)?;
            Ok(registry.convert_with_fallback(&request, &policy))
        })
    }

    pub fn close(&self) -> JsValue {
        self.closed.set(true);
        self.cleanup();
        envelope(Ok(true))
    }
}

impl Drop for FormulaRegistryWasm {
    fn drop(&mut self) {
        remove(self.callbacks.get_mut());
        INSTANCES.with(|instances| instances.set(instances.get().saturating_sub(1)));
    }
}

/// Additive factory; app-owned Worker bridges may explicitly override it.
#[wasm_bindgen]
pub fn create_formula_registry_v1(
    configuration_key: Option<String>,
) -> std::result::Result<FormulaRegistryWasm, JsValue> {
    if configuration_key.is_some() {
        return Err(JsValue::from_str("Core has no built-in trusted extension configuration; use an explicitly trusted host bridge"));
    }
    FormulaRegistryWasm::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_candidate_and_request_json_do_not_discard_future_fields() {
        assert_eq!(
            parse_candidate(r#"{"content":"x","losses":[],"ignored":true}"#)
                .err()
                .unwrap()
                .code,
            ConversionRegistryErrorCode::OutputRejected
        );
        assert!(parse_candidate(
            r#"{"content":"x","losses":[{"code":"x","message":"x","ignored":true}]}"#
        )
        .is_err());
        assert_eq!(
            parse_candidate(r#"{"content":"x","losses":[]}"#)
                .unwrap()
                .content,
            "x"
        );
        assert!(parse::<RegisteredConversionRequest>(
            r#"{"schemaVersion":1,"schemaVersion":2}"#,
            4096
        )
        .is_err());
    }
}

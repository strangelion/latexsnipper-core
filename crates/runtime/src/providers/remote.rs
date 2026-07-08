//! Remote API Provider — optional HTTP-based model execution.
//!
//! Enabled via the `remote-api` Cargo feature. Provides an OpenAI-compatible
//! API client for vision-language and prompt-based model calls.
//!
//! All remote outputs are validated against the configured JSON schema before
//! being returned; schema violations produce diagnostics instead of panicking.

use latexsnipper_ast::{Diagnostic, DiagnosticLevel, ProviderCallReport, ProviderReport};
use std::time::Instant;

use crate::api_provider::{ApiProviderConfig, PromptProfile, UploadPolicy};

/// Result of a single remote API call.
#[derive(Debug, Clone)]
pub struct RemoteApiResult {
    /// Raw text response from the API.
    pub text: String,
    /// Parsed JSON (if the response was JSON).
    pub parsed_json: Option<serde_json::Value>,
    /// Whether the response passed schema validation.
    pub schema_valid: bool,
    /// Elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Token usage if reported by the API.
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

impl RemoteApiResult {
    /// Check if the response is usable (non-empty, schema-valid if schema required).
    pub fn is_usable(&self) -> bool {
        !self.text.is_empty() && self.schema_valid
    }

    /// Check if the response is usable for a specific profile.
    /// Allows responses without schema validation when the profile doesn't require one.
    pub fn is_usable_for_profile(&self, profile: &PromptProfile) -> bool {
        !self.text.is_empty() && (profile.output_schema.is_none() || self.schema_valid)
    }
}

/// A remote API provider that executes prompts via HTTP.
///
/// Supports OpenAI-compatible chat completion endpoints with image support.
/// Use `RemoteApiProvider::new(config)` to create, then call `execute()`.
pub struct RemoteApiProvider {
    pub config: ApiProviderConfig,
}

impl RemoteApiProvider {
    pub fn new(config: ApiProviderConfig) -> Self {
        Self { config }
    }

    /// Execute a prompt with an optional base64-encoded image.
    ///
    /// Returns the API response along with diagnostics and timing information.
    pub async fn execute(
        &self,
        profile: &PromptProfile,
        image_base64: Option<&str>,
    ) -> (RemoteApiResult, Vec<Diagnostic>, ProviderReport) {
        let start = Instant::now();
        let mut diagnostics = Vec::new();
        let report_id = format!("remote_{}", self.config.provider);

        // 1. Check upload policy
        if let Some(_img) = image_base64 {
            if !self.may_upload(UploadScope::PageImage) {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticLevel::Error,
                        "E_UPLOAD_BLOCKED",
                        "Upload policy prevented image transmission",
                    )
                    .with_recoverable(true),
                );
                let elapsed = start.elapsed().as_millis() as u64;
                return (
                    RemoteApiResult {
                        text: String::new(),
                        parsed_json: None,
                        schema_valid: false,
                        elapsed_ms: elapsed,
                        input_tokens: None,
                        output_tokens: None,
                    },
                    diagnostics,
                    ProviderReport {
                        provider_id: report_id,
                        provider_kind: "RemoteApi".to_string(),
                        model: Some(self.config.model.clone()),
                        tasks: vec![format!("{:?}", profile.task)],
                        calls: vec![ProviderCallReport {
                            call_id: "call_1".to_string(),
                            model: Some(self.config.model.clone()),
                            input_tokens: None,
                            output_tokens: None,
                            elapsed_ms: elapsed,
                            success: false,
                            error: Some("E_UPLOAD_BLOCKED: Upload policy prevented image transmission".to_string()),
                        }],
                        fallback_used: false,
                        total_elapsed_ms: elapsed,
                    },
                );
            }
        }

        // 2. Build the request payload
        let payload = match self.build_payload(profile, image_base64) {
            Ok(p) => p,
            Err(e) => {
                diagnostics.push(Diagnostic::new(DiagnosticLevel::Error, "E_PAYLOAD", &e));
                let elapsed = start.elapsed().as_millis() as u64;
                return (
                    RemoteApiResult {
                        text: String::new(),
                        parsed_json: None,
                        schema_valid: false,
                        elapsed_ms: elapsed,
                        input_tokens: None,
                        output_tokens: None,
                    },
                    diagnostics,
                    ProviderReport {
                        provider_id: report_id,
                        provider_kind: "RemoteApi".to_string(),
                        model: Some(self.config.model.clone()),
                        tasks: vec![format!("{:?}", profile.task)],
                        calls: vec![ProviderCallReport {
                            call_id: "call_1".to_string(),
                            model: Some(self.config.model.clone()),
                            input_tokens: None,
                            output_tokens: None,
                            elapsed_ms: elapsed,
                            success: false,
                            error: Some(format!("E_PAYLOAD: {}", e)),
                        }],
                        fallback_used: false,
                        total_elapsed_ms: elapsed,
                    },
                );
            }
        };

        // 3. Send HTTP request
        let response = self.send_request(&payload).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let (text, parsed_json, input_tokens, output_tokens) = match response {
            Ok(raw) => {
                let parsed = serde_json::from_str::<serde_json::Value>(&raw.content).ok();
                (raw.content, parsed, raw.input_tokens, raw.output_tokens)
            }
            Err(e) => {
                diagnostics.push(Diagnostic::new(DiagnosticLevel::Error, "E_API_CALL", &e));
                let report = ProviderReport {
                    provider_id: report_id,
                    provider_kind: "RemoteApi".to_string(),
                    model: Some(self.config.model.clone()),
                    tasks: vec![format!("{:?}", profile.task)],
                    calls: vec![ProviderCallReport {
                        call_id: "call_1".to_string(),
                        model: Some(self.config.model.clone()),
                        input_tokens: None,
                        output_tokens: None,
                        elapsed_ms,
                        success: false,
                        error: Some(e),
                    }],
                    fallback_used: false,
                    total_elapsed_ms: elapsed_ms,
                };
                return (
                    RemoteApiResult {
                        text: String::new(),
                        parsed_json: None,
                        schema_valid: false,
                        elapsed_ms,
                        input_tokens: None,
                        output_tokens: None,
                    },
                    diagnostics,
                    report,
                );
            }
        };

        // 4. Schema validation
        let schema_valid =
            if let (Some(schema), Some(json)) = (&profile.output_schema, &parsed_json) {
                validate_json_against_schema(json, schema)
            } else {
                true
            };

        if !schema_valid {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    "E_SCHEMA_OUTPUT",
                    "API response did not match the expected output schema",
                )
                .with_recoverable(true),
            );
        }

        // 5. Build report
        let report = ProviderReport {
            provider_id: report_id,
            provider_kind: "RemoteApi".to_string(),
            model: Some(self.config.model.clone()),
            tasks: vec![format!("{:?}", profile.task)],
            calls: vec![ProviderCallReport {
                call_id: "call_1".to_string(),
                model: Some(self.config.model.clone()),
                input_tokens,
                output_tokens,
                elapsed_ms,
                success: true,
                error: None,
            }],
            fallback_used: false,
            total_elapsed_ms: elapsed_ms,
        };

        (
            RemoteApiResult {
                text,
                parsed_json,
                schema_valid,
                elapsed_ms,
                input_tokens,
                output_tokens,
            },
            diagnostics,
            report,
        )
    }

    /// Check whether the upload policy allows sending the given scope of data.
    fn may_upload(&self, scope: UploadScope) -> bool {
        self.config.upload_policy.allows(scope)
    }

    /// Build the JSON request payload for an OpenAI-compatible chat endpoint.
    fn build_payload(
        &self,
        profile: &PromptProfile,
        image_base64: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let mut messages = Vec::new();

        if let Some(system) = &profile.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system
            }));
        }

        let mut content = Vec::new();
        content.push(serde_json::json!({
            "type": "text",
            "text": &profile.instruction
        }));

        if let Some(b64) = image_base64 {
            content.push(serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/png;base64,{}", b64)
                }
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": content
        }));

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": profile.max_tokens.unwrap_or(1024),
        });

        if let Some(temp) = profile.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        if profile.output_schema.is_some() {
            body["response_format"] = serde_json::json!({
                "type": "json_object"
            });
        }

        Ok(body)
    }

    /// Send the HTTP request to the configured endpoint.
    /// Returns a structured response with content and token usage from the full response body.
    struct ApiRawResponse {
        content: String,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
    }

    /// Send the request and return a structured response with token usage.
    async fn send_request(&self, payload: &serde_json::Value) -> Result<ApiRawResponse, String> {
        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/chat/completions");

        let api_key = self
            .config
            .api_key_env
            .as_ref()
            .and_then(|env_var| std::env::var(env_var).ok());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(self.config.timeout_ms))
            .build()
            .map_err(|e| format!("E_API_HTTP: Failed to create HTTP client: {}", e))?;

        let mut req = client.post(endpoint).json(payload);

        if let Some(key) = &api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                format!(
                    "E_API_TIMEOUT: Request timed out after {}ms",
                    self.config.timeout_ms
                )
            } else if e.is_connect() {
                format!("E_API_HTTP: Connection failed: {}", e)
            } else {
                format!("E_API_HTTP: Request failed: {}", e)
            }
        })?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("E_API_HTTP: Failed to read response body: {}", e))?;

        if status.as_u16() == 401 {
            return Err("E_API_AUTH: Authentication failed (status 401)".to_string());
        }
        if status.as_u16() == 429 {
            return Err("E_API_RATE_LIMIT: Rate limited (status 429)".to_string());
        }
        if !status.is_success() {
            return Err(format!("E_API_HTTP: API error {}: {}", status.as_u16(), body));
        }

        let content = extract_content_from_openai_response(&body);
        let (input_tokens, output_tokens) = extract_token_usage(&body);
        Ok(ApiRawResponse {
            content,
            input_tokens,
            output_tokens,
        })
    }
}

/// Extract the text content from an OpenAI-compatible chat completion response.
fn extract_content_from_openai_response(body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(choice) = parsed["choices"].get(0) {
            if let Some(content) = choice["message"]["content"].as_str() {
                return content.to_string();
            }
        }
    }
    body.to_string()
}

/// Extract token usage from an OpenAI-compatible response JSON.
fn extract_token_usage(body: &str) -> (Option<u32>, Option<u32>) {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) {
        let usage = &parsed["usage"];
        let input = usage["prompt_tokens"].as_u64().map(|v| v as u32);
        let output = usage["completion_tokens"].as_u64().map(|v| v as u32);
        (input, output)
    } else {
        (None, None)
    }
}

/// Simple JSON schema validation.
///
/// Checks that the response JSON has all top-level keys that the schema specifies
/// as `required` or `properties`. This is a lightweight check; a full schema
/// validator would require a JSON Schema library.
fn validate_json_against_schema(json: &serde_json::Value, schema: &serde_json::Value) -> bool {
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        for key in required {
            if let Some(key_str) = key.as_str() {
                if json.get(key_str).is_none() {
                    return false;
                }
            }
        }
    }

    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        for (key, prop_schema) in properties {
            if json.get(key).is_none() && prop_schema.get("default").is_none() {
                return false;
            }
        }
    }

    true
}

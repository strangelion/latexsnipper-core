use serde::{Deserialize, Serialize};

// Re-export platform-level provider kind from ast to avoid duplicate definitions.
pub use latexsnipper_ast::ModelProviderKind;

/// Configuration for a remote API provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProviderConfig {
    /// Provider name (e.g., "openai", "azure", "anthropic", "custom").
    pub provider: String,
    /// Optional custom endpoint URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Model identifier.
    pub model: String,
    /// Environment variable name for the API key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum number of retries.
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    /// Upload policy for images/data.
    pub upload_policy: UploadPolicy,
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_max_retries() -> u8 {
    3
}

/// Policy for uploading images/data to remote providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadPolicy {
    /// Never upload images.
    Never,
    /// Only upload cropped regions, not whole pages.
    CroppedRegionsOnly,
    /// Upload whole pages.
    WholePage,
    /// Upload entire documents.
    WholeDocument,
}

/// The scope of data to be uploaded for a single API call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadScope {
    /// A single cropped region (e.g., a detected formula or text block).
    CroppedRegion,
    /// A full page image.
    PageImage,
    /// The entire document (all pages).
    WholeDocument,
}

impl UploadPolicy {
    /// Check whether this policy allows uploading the given scope.
    pub fn allows(&self, scope: UploadScope) -> bool {
        match self {
            UploadPolicy::Never => false,
            UploadPolicy::CroppedRegionsOnly => matches!(scope, UploadScope::CroppedRegion),
            UploadPolicy::WholePage => {
                matches!(scope, UploadScope::CroppedRegion | UploadScope::PageImage)
            }
            UploadPolicy::WholeDocument => true,
        }
    }
}

/// Prompt profile for structured API interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptProfile {
    /// Unique profile identifier.
    pub id: String,
    /// The task this profile is designed for.
    pub task: PromptTask,
    /// Human-readable label.
    pub label: String,
    /// Optional system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// User instruction template.
    pub instruction: String,
    /// Optional JSON schema for structured output validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Few-shot examples.
    #[serde(default)]
    pub examples: Vec<PromptExample>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens in response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Tasks that prompt profiles can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptTask {
    FormulaRecognition,
    FormulaCorrection,
    TextRecognition,
    TableToMarkdown,
    TableToLatex,
    DiagramDescription,
    ChartExtraction,
    DocumentCleanup,
}

/// A few-shot example for a prompt profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptExample {
    /// Description of the input.
    pub input_description: String,
    /// Expected output (JSON value).
    pub expected_output: serde_json::Value,
}

/// Extension trait for providing API key resolution.
pub trait ApiKeyResolver {
    /// Resolve an API key environment variable name to the actual key.
    fn resolve_api_key(&self, env_name: &str) -> Option<String>;
}

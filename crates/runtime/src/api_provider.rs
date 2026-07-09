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
    ExamPaperGeneration,
}

/// Named presets for well-known prompt profiles.
///
/// Each preset comes with built-in system prompts, instructions,
/// output schemas, and sensible defaults. Use `PromptProfile::from_preset()`
/// to create a profile, then override fields with builder methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptPreset {
    /// Extract structured chart data (bar/line/pie/scatter) from a chart image.
    ChartExtraction,
    /// Describe a diagram or flowchart as structured shapes and connections.
    DiagramDescription,
    /// Correct a formula LaTeX string that may contain OCR errors.
    FormulaCorrection,
    /// Convert a table image to Markdown format.
    TableToMarkdown,
    /// Convert a table image to LaTeX format.
    TableToLatex,
    /// Clean up and normalize a recognized document.
    DocumentCleanup,
    /// Generate an exam paper from a specification.
    ExamPaperGeneration,
    /// Start from an empty profile, override everything.
    Custom,
}

impl PromptProfile {
    /// Create a `PromptProfile` from a named preset.
    ///
    /// Returns a fully populated profile with system prompt, instruction,
    /// output schema, and defaults tuned for the preset task.
    /// Use builder methods (`with_system`, `with_instruction`, etc.)
    /// to override any field.
    pub fn from_preset(preset: PromptPreset) -> Self {
        match preset {
            PromptPreset::ChartExtraction => Self {
                id: "chart-extraction-v1".to_string(),
                task: PromptTask::ChartExtraction,
                label: "Chart Data Extraction".to_string(),
                system: Some(
                    "You are a chart analysis assistant. Extract the chart data \
                     precisely from the provided chart image. Return ONLY valid JSON."
                        .to_string(),
                ),
                instruction: [
                    "Analyze this chart image and extract its data as JSON with this exact structure:",
                    "{",
                    r#"  "chart_type": "bar|line|pie|scatter|area|unknown","#,
                    r#"  "title": "chart title or empty string","#,
                    r#"  "x_axis": { "label": "...", "min": null, "max": null },"#,
                    r#"  "y_axis": { "label": "...", "min": null, "max": null },"#,
                    "  \"series\": [",
                    r#"    { "name": "Series1", "values": [1.0, 2.0, 3.0] }"#,
                    "  ],",
                    r#"  "labels": ["Cat1", "Cat2", "Cat3"],"#,
                    r#"  "legend": { "visible": true, "position": "top" }"#,
                    "}",
                    "",
                    "Use numeric values (not strings) for data points.",
                    "If a field is not visible in the image, use null or empty array.",
                ]
                .join("\n"),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "required": ["chart_type", "title", "series", "labels"],
                    "properties": {
                        "chart_type": { "type": "string" },
                        "title": { "type": "string" },
                        "x_axis": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "min": { "type": "number" },
                                "max": { "type": "number" }
                            }
                        },
                        "y_axis": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "min": { "type": "number" },
                                "max": { "type": "number" }
                            }
                        },
                        "series": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "values": { "type": "array", "items": { "type": "number" } }
                                }
                            }
                        },
                        "labels": { "type": "array", "items": { "type": "string" } },
                        "legend": {
                            "type": "object",
                            "properties": {
                                "visible": { "type": "boolean" },
                                "position": { "type": "string" }
                            }
                        }
                    }
                })),
                examples: Vec::new(),
                temperature: Some(0.1),
                max_tokens: Some(2048),
            },

            PromptPreset::DiagramDescription => Self {
                id: "diagram-extraction-v1".to_string(),
                task: PromptTask::DiagramDescription,
                label: "Diagram Description".to_string(),
                system: Some(
                    "You are a diagram analysis assistant. Extract shapes, text, \
                     and connections from the provided diagram image. Return ONLY valid JSON."
                        .to_string(),
                ),
                instruction: [
                    "Analyze this diagram image and extract its structure as JSON:",
                    "{",
                    r#"  "shapes": ["#,
                    "    {",
                    r#"      "shape_type": "rectangle|ellipse|diamond|arrow|line|text|custom","#,
                    r#"      "id": "shape-1","#,
                    r#"      "text": "label text","#,
                    r#"      "x": 100, "y": 200, "width": 80, "height": 40"#,
                    "    }",
                    "  ],",
                    "  \"connections\": [",
                    "    {",
                    r#"      "from_id": "shape-1", "to_id": "shape-2","#,
                    r#"      "label": "connects to","#,
                    r#"      "type": "arrow|line""#,
                    "    }",
                    "  ]",
                    "}",
                    "",
                    "Assign unique IDs. Use null for optional fields.",
                ]
                .join("\n"),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "required": ["shapes"],
                    "properties": {
                        "shapes": { "type": "array", "items": { "type": "object" } },
                        "connections": { "type": "array", "items": { "type": "object" } }
                    }
                })),
                examples: Vec::new(),
                temperature: Some(0.2),
                max_tokens: Some(4096),
            },

            PromptPreset::FormulaCorrection => Self {
                id: "formula-correction-v1".to_string(),
                task: PromptTask::FormulaCorrection,
                label: "Formula Correction".to_string(),
                system: Some(
                    "You are a LaTeX formula correction assistant. Fix OCR errors \
                     and malformed LaTeX while preserving the mathematical meaning."
                        .to_string(),
                ),
                instruction: "Correct the following LaTeX formula. Fix common OCR errors \
                              (e.g., '0' vs 'O', '1' vs 'l', missing braces). \
                              Return the corrected LaTeX only, no explanation."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(512),
            },

            PromptPreset::TableToMarkdown => Self {
                id: "table-to-markdown-v1".to_string(),
                task: PromptTask::TableToMarkdown,
                label: "Table to Markdown".to_string(),
                system: Some(
                    "You are a table extraction assistant. Convert table images \
                     to Markdown format accurately."
                        .to_string(),
                ),
                instruction: "Convert this table image to Markdown table format. \
                              Use proper alignment markers. Preserve all text, numbers, \
                              and formatting as much as possible."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(2048),
            },

            PromptPreset::TableToLatex => Self {
                id: "table-to-latex-v1".to_string(),
                task: PromptTask::TableToLatex,
                label: "Table to LaTeX".to_string(),
                system: Some(
                    "You are a table extraction assistant. Convert table images \
                     to LaTeX tabular environment accurately."
                        .to_string(),
                ),
                instruction: "Convert this table image to LaTeX tabular format. \
                              Use proper column alignment, horizontal lines, and escaping. \
                              Preserve all data."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(2048),
            },

            PromptPreset::DocumentCleanup => Self {
                id: "document-cleanup-v1".to_string(),
                task: PromptTask::DocumentCleanup,
                label: "Document Cleanup".to_string(),
                system: Some(
                    "You are a document cleanup assistant. Normalize and fix \
                     recognized document text."
                        .to_string(),
                ),
                instruction: "Clean up this recognized text: fix spacing, correct \
                              common OCR mistakes, normalize punctuation, and preserve \
                              formatting. Return the cleaned text."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.1),
                max_tokens: Some(4096),
            },

            PromptPreset::ExamPaperGeneration => Self {
                id: "exam-paper-v1".to_string(),
                task: PromptTask::ExamPaperGeneration,
                label: "Exam Paper Generation".to_string(),
                system: Some(
                    "You are an exam paper generation assistant. Create well-structured \
                     exam papers in LaTeX format with appropriate question types, \
                     difficulty levels, and clear instructions."
                        .to_string(),
                ),
                instruction: "Generate an exam paper based on the following specification. \
                              Include a mix of question types (multiple choice, short answer, \
                              problem solving). Use proper LaTeX formatting with \\begin{questions}, \
                              \\question, \\choice, etc. Add point values and total marks."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.7),
                max_tokens: Some(8192),
            },

            PromptPreset::Custom => Self {
                id: "custom".to_string(),
                task: PromptTask::DocumentCleanup,
                label: "Custom Profile".to_string(),
                system: None,
                instruction: String::new(),
                output_schema: None,
                examples: Vec::new(),
                temperature: None,
                max_tokens: None,
            },
        }
    }

    /// Override the system prompt.
    pub fn with_system(mut self, system: &str) -> Self {
        self.system = Some(system.to_string());
        self
    }

    /// Override the user instruction.
    pub fn with_instruction(mut self, instruction: &str) -> Self {
        self.instruction = instruction.to_string();
        self
    }

    /// Set or override the JSON Schema for structured output.
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Override the sampling temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Override the maximum response tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Add a few-shot example.
    pub fn with_example(mut self, input: &str, output: serde_json::Value) -> Self {
        self.examples.push(PromptExample {
            input_description: input.to_string(),
            expected_output: output,
        });
        self
    }
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

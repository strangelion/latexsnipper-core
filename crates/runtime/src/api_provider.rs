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
                id: "chart-extraction-v2".to_string(),
                task: PromptTask::ChartExtraction,
                label: "Structured Chart Data Extraction".to_string(),
                system: Some(
                    "You are an expert data extraction system specialized in chart analysis. \
                     Given a chart image, you must extract the underlying data with \
                     perfect numerical accuracy. Follow these rules:\n\
                     1. Identify the chart type first (bar, line, pie, scatter, area).\n\
                     2. Read axis labels, tick marks, and gridlines to determine scales.\n\
                     3. Extract every visible data point as precise numeric values.\n\
                     4. For bar/line charts: identify series names from the legend.\n\
                     5. For pie charts: extract each slice's label, value, and percentage.\n\
                     6. If axis values are ambiguous, use your best estimate and mark with ~.\n\
                     7. Return ONLY valid JSON — no preamble, no explanation."
                        .to_string(),
                ),
                instruction: [
                    "Analyze the chart image and respond with JSON following this exact schema:",
                    "{",
                    r#"  "chart_type": "bar" | "line" | "pie" | "scatter" | "area" | "unknown","#,
                    r#"  "title": "The chart title, or empty string if none","#,
                    r#"  "x_axis": { "label": "X-axis label", "min": 0.0, "max": 100.0 },"#,
                    r#"  "y_axis": { "label": "Y-axis label", "min": 0.0, "max": 50.0 },"#,
                    r#"  "series": [{ "name": "Series A", "values": [10.5, 20.3, 30.1] }], "#,
                    r#"  "labels": ["Q1", "Q2", "Q3"],"#,
                    r#"  "legend": { "visible": true, "position": "top" | "bottom" | "right" | "none" }"#,
                    "}",
                    "",
                    "Requirements:",
                    "- All numeric values must be f64 numbers, not strings.",
                    "- Use null for any unknown/unreadable field.",
                    "- If the chart has no legend, set legend.visible = false.",
                    "- Round decimal values to at most 2 decimal places.",
                    "- If the image is not a chart, set chart_type = \"unknown\" and leave other fields empty/null.",
                ]
                .join("\n"),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "required": ["chart_type", "title", "series"],
                    "properties": {
                        "chart_type": { "type": "string", "enum": ["bar", "line", "pie", "scatter", "area", "unknown"] },
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
                                "required": ["name", "values"],
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
                temperature: Some(0.05),
                max_tokens: Some(2048),
            },

            PromptPreset::DiagramDescription => Self {
                id: "diagram-extraction-v2".to_string(),
                task: PromptTask::DiagramDescription,
                label: "Structured Diagram Description".to_string(),
                system: Some(
                    "You are an expert diagram analysis system. Given a diagram or flowchart image, \
                     you must extract every visual element: shapes, text labels, connections, \
                     and their spatial relationships. Follow these rules:\n\
                     1. Identify each distinct shape: rectangle (process), diamond (decision), \
                        ellipse (start/end), parallelogram (I/O), arrow, line, text.\n\
                     2. Assign a unique ID to each shape.\n\
                     3. Extract visible text labels verbatim.\n\
                     4. Identify connections (arrows/lines) between shapes, including arrow direction.\n\
                     5. Use normalized coordinates (0-1000) if exact pixels are unclear.\n\
                     6. Return ONLY valid JSON — no preamble or explanation."
                        .to_string(),
                ),
                instruction: [
                    "Analyze this diagram image and respond with JSON following this exact schema:",
                    "{",
                    r#"  "shapes": ["#,
                    "    {",
                    r#"      "id": "shape-1","#,
                    r#"      "type": "rectangle" | "ellipse" | "diamond" | "parallelogram" | "arrow" | "line" | "text" | "custom","#,
                    r#"      "x": 100, "y": 200,"#,
                    r#"      "width": 80, "height": 40,"#,
                    r#"      "label": "Process Name","#,
                    r#"      "style": { "fill": "4CAF50", "stroke": "333333", "stroke_width": 2 }"#,
                    "    }",
                    "  ],",
                    r#"  "connections": ["#,
                    "    {",
                    r#"      "from_id": "shape-1", "to_id": "shape-2","#,
                    r#"      "label": "transition condition","#,
                    r#"      "type": "arrow" | "line""#,
                    "    }",
                    "  ]",
                    "}",
                    "",
                    "Rules:",
                    "- Every shape must have a unique id.",
                    "- Coordinates x, y refer to the top-left corner.",
                    "- Use null for any unknown attribute.",
                    "- If a shape type is ambiguous, use your best judgment.",
                    "- For connection type: \"arrow\" indicates direction, \"line\" indicates no direction.",
                ]
                .join("\n"),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "required": ["shapes"],
                    "properties": {
                        "shapes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["id", "type"],
                                "properties": {
                                    "id": { "type": "string" },
                                    "type": { "type": "string" },
                                    "x": { "type": "number" },
                                    "y": { "type": "number" },
                                    "width": { "type": "number" },
                                    "height": { "type": "number" },
                                    "label": { "type": "string" },
                                    "style": { "type": "object" }
                                }
                            }
                        },
                        "connections": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["from_id", "to_id"],
                                "properties": {
                                    "from_id": { "type": "string" },
                                    "to_id": { "type": "string" },
                                    "label": { "type": "string" },
                                    "type": { "type": "string" }
                                }
                            }
                        }
                    }
                })),
                examples: Vec::new(),
                temperature: Some(0.1),
                max_tokens: Some(4096),
            },

            PromptPreset::FormulaCorrection => Self {
                id: "formula-correction-v2".to_string(),
                task: PromptTask::FormulaCorrection,
                label: "LaTeX Formula OCR Correction".to_string(),
                system: Some(
                    "You are an expert LaTeX formula correction system. Your task is to \
                     fix OCR errors in LaTeX mathematical expressions.\n\n\
                     Common OCR errors you MUST correct:\n\
                     - '0' (zero) ↔ 'O' (letter)\n\
                     - '1' (one) ↔ 'l' (lowercase L) ↔ '|' (pipe)\n\
                     - '5' ↔ 'S', '8' ↔ 'B', '6' ↔ 'G'\n\
                     - Missing braces {} and brackets []\n\
                     - Wrong delimiters: ( vs [ vs \\{\n\
                     - Missing backslashes for commands: alpha → \\alpha\n\
                     - Space insertion/deletion: \\fraca b → \\frac{a}{b}\n\n\
                     Rules:\n\
                     1. Preserve the mathematical meaning exactly.\n\
                     2. Ensure all braces are balanced and correctly nested.\n\
                     3. Fix command names: e.g., \\sqrt, \\frac, \\sum, \\int.\n\
                     4. Return ONLY the corrected LaTeX code — no explanation, no wrapping."
                        .to_string(),
                ),
                instruction: "Correct the following LaTeX formula for OCR errors.\n\
                              Input: {input}\n\n\
                              Return the corrected LaTeX expression only."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(512),
            },

            PromptPreset::TableToMarkdown => Self {
                id: "table-to-markdown-v2".to_string(),
                task: PromptTask::TableToMarkdown,
                label: "Table Image to Markdown Conversion".to_string(),
                system: Some(
                    "You are an expert table extraction system. Convert table images to \
                     well-formed Markdown table format. Follow these rules:\n\
                     1. Reproduce every cell value exactly as it appears.\n\
                     2. Preserve numeric values, text, dates, and symbols.\n\
                     3. Determine column alignment from visual cues (left/center/right).\n\
                     4. Handle multi-line cells by using <br> within the cell.\n\
                     5. If the table has a header row, mark it distinctly.\n\
                     6. If a cell is empty, leave it blank.\n\
                     7. Return ONLY the Markdown table — no surrounding text."
                        .to_string(),
                ),
                instruction: "Convert this table image to a well-formed Markdown table.\n\
                              Use '|' for column separators and '---' for header separation.\n\
                              Preserve all text, numbers, and alignment exactly."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(2048),
            },

            PromptPreset::TableToLatex => Self {
                id: "table-to-latex-v2".to_string(),
                task: PromptTask::TableToLatex,
                label: "Table Image to LaTeX Conversion".to_string(),
                system: Some(
                    "You are an expert table-to-LaTeX conversion system. Convert table images \
                     into properly formatted LaTeX tabular environments. Follow these rules:\n\
                     1. Determine the number of columns and their alignment (l/c/r).\n\
                     2. Use \\hline for horizontal rules where appropriate.\n\
                     3. Escape special LaTeX characters: _, ^, %, $, #, &, {, }, ~, \\\n\
                     4. Preserve all text, numbers, and formatting.\n\
                     5. Wrap long cell content or use p{} columns as needed.\n\
                     6. Return ONLY the LaTeX tabular environment — no document wrapper."
                        .to_string(),
                ),
                instruction: "Convert this table image to LaTeX tabular format.\n\
                              Determine column count and alignment from the visual layout.\n\
                              Use \\hline for header separation. Preserve all data exactly."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.0),
                max_tokens: Some(2048),
            },

            PromptPreset::DocumentCleanup => Self {
                id: "document-cleanup-v2".to_string(),
                task: PromptTask::DocumentCleanup,
                label: "OCR Document Text Cleanup".to_string(),
                system: Some(
                    "You are an expert document cleanup system. Given OCR-recognized text, \
                     you must fix errors while preserving the original meaning and structure.\n\n\
                     Fix these common issues:\n\
                     1. Spacing: remove extra spaces, fix missing spaces between words.\n\
                     2. Punctuation: normalize quotes, dashes, ellipses.\n\
                     3. Capitalization: fix sentence-start capitalization if clearly wrong.\n\
                     4. Line breaks: preserve paragraph structure.\n\
                     5. Common OCR confusions: rn→m, cl→d, etc.\n\
                     6. Numbers: preserve digits, decimal points, and separators.\n\n\
                     Rules:\n\
                     - Do NOT rewrite content or change meaning.\n\
                     - Do NOT add or remove substantive text.\n\
                     - Preserve special formatting, indentation, and list markers.\n\
                     - Return the cleaned text only."
                        .to_string(),
                ),
                instruction: "Clean up this OCR-recognized text, fixing spacing, \
                              punctuation, and common OCR errors while preserving \
                              the original structure and meaning:\n\n{input}"
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.1),
                max_tokens: Some(4096),
            },

            PromptPreset::ExamPaperGeneration => Self {
                id: "exam-paper-v2".to_string(),
                task: PromptTask::ExamPaperGeneration,
                label: "LaTeX Exam Paper Generation".to_string(),
                system: Some(
                    "You are an expert exam paper generation assistant. Create professional, \
                     well-structured exam papers in LaTeX using the exam document class.\n\n\
                     Guidelines:\n\
                     1. Use the 'exam' document class for proper formatting.\n\
                     2. Structure with \\begin{questions}...\\end{questions}.\n\
                     3. Each question: \\question[points] text.\n\
                     4. Multiple choice: \\choice, \\CorrectChoice.\n\
                     5. Multi-part questions: \\begin{parts}...\\end{parts}.\n\
                     6. Include \\pointname, \\pointformat, \\bonuspointname as needed.\n\
                     7. Add \\gradetable at the end.\n\
                     8. Use appropriate math formatting: $...$ and $$...$$.\n\
                     9. Ensure the total marks are clearly stated.\n\
                     10. Vary difficulty: easy (30%), medium (50%), hard (20%)."
                        .to_string(),
                ),
                instruction: "Generate a LaTeX exam paper based on this specification:\n\
                              Subject: {subject}\n\
                              Grade Level: {grade}\n\
                              Total Marks: {marks}\n\
                              Duration: {duration}\n\
                              Topics: {topics}\n\n\
                              Include:\n\
                              - Section A: Multiple choice / fill-in-blank (30% of marks)\n\
                              - Section B: Short answer / problem solving (50% of marks)\n\
                              - Section C: Extended response / proof (20% of marks)\n\
                              - Answer key or marking scheme after \\end{questions}\n\n\
                              Return ONLY the complete LaTeX source code."
                    .to_string(),
                output_schema: None,
                examples: Vec::new(),
                temperature: Some(0.7),
                max_tokens: Some(16384),
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

use latexsnipper_ast::{
    Diagnostic, DiagnosticLevel, Inline, ProviderReport, Rect, ShapeBlock, ShapeStyle, ShapeType,
    SourceInfo, TextRun,
};
use latexsnipper_runtime::api_provider::{ApiProviderConfig, PromptPreset, PromptProfile};
use latexsnipper_runtime::RemoteApiProvider;

/// A recognized shape in a diagram.
#[derive(Debug, Clone)]
pub struct DiagramShape {
    pub shape_block: ShapeBlock,
    pub shape_id: Option<String>,
    pub connections: Vec<DiagramConnection>,
}

/// A connection (arrow/line) between two diagram shapes.
#[derive(Debug, Clone)]
pub struct DiagramConnection {
    pub from_id: String,
    pub to_id: String,
    pub label: Option<String>,
}

/// Result of diagram understanding.
#[derive(Debug, Clone)]
pub struct DiagramUnderstandingResult {
    pub shapes: Vec<DiagramShape>,
    pub diagnostics: Vec<Diagnostic>,
    pub provider_report: ProviderReport,
    pub raw_response: String,
}

/// Service that uses a VLM to analyze a diagram image (flowchart, architecture,
/// mind map, etc.) and extract shapes with their connections as a Graph AST.
///
/// # Example
/// ```no_run
/// # use latexsnipper_engine::diagram_understanding::DiagramUnderstandingService;
/// # use latexsnipper_runtime::api_provider::{ApiProviderConfig, UploadPolicy};
/// let config = ApiProviderConfig {
///     provider: "openai".into(),
///     endpoint: None,
///     model: "gpt-4o".into(),
///     api_key_env: Some("OPENAI_API_KEY".into()),
///     timeout_ms: 60000,
///     max_retries: 2,
///     upload_policy: UploadPolicy::CroppedRegionsOnly,
/// };
/// let service = DiagramUnderstandingService::new(config);
/// // let result = service.understand_diagram(image_base64).await;
/// ```
pub struct DiagramUnderstandingService {
    provider: RemoteApiProvider,
}

impl DiagramUnderstandingService {
    /// Create a new diagram understanding service.
    pub fn new(config: ApiProviderConfig) -> Self {
        Self {
            provider: RemoteApiProvider::new(config),
        }
    }

    /// Analyze a diagram image and extract shapes with connections.
    pub async fn understand_diagram(&self, image_base64: &str) -> DiagramUnderstandingResult {
        let profile = PromptProfile::from_preset(PromptPreset::DiagramDescription);

        let (result, mut diagnostics, report) =
            self.provider.execute(&profile, Some(image_base64)).await;

        if !result.is_usable() {
            return DiagramUnderstandingResult {
                shapes: Vec::new(),
                diagnostics,
                provider_report: report,
                raw_response: result.text,
            };
        }

        let shapes = match &result.parsed_json {
            Some(json) => Self::parse_diagram_json(json),
            None => {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticLevel::Warning,
                        "E_DIAGRAM_PARSE",
                        "VLM response was not valid JSON",
                    )
                    .with_recoverable(true),
                );
                Vec::new()
            }
        };

        DiagramUnderstandingResult {
            shapes,
            diagnostics,
            provider_report: report,
            raw_response: result.text,
        }
    }

    fn parse_diagram_json(json: &serde_json::Value) -> Vec<DiagramShape> {
        let mut shapes = Vec::new();

        let connections: Vec<DiagramConnection> = json["connections"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        Some(DiagramConnection {
                            from_id: c["from_id"].as_str()?.to_string(),
                            to_id: c["to_id"].as_str()?.to_string(),
                            label: c["label"].as_str().map(|s| s.to_string()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if let Some(arr) = json["shapes"].as_array() {
            for shape_val in arr {
                let id = shape_val["id"].as_str().map(|s| s.to_string());
                let shape_type = parse_shape_type(shape_val["type"].as_str().unwrap_or(""));
                let x = shape_val["x"].as_f64().unwrap_or(0.0) as f32;
                let y = shape_val["y"].as_f64().unwrap_or(0.0) as f32;
                let w = shape_val["width"].as_f64().unwrap_or(100.0) as f32;
                let h = shape_val["height"].as_f64().unwrap_or(50.0) as f32;
                let label = shape_val["label"].as_str().unwrap_or("");

                let mut text = Vec::new();
                if !label.is_empty() {
                    text.push(Inline::Text(TextRun::new(label)));
                }

                let style = parse_style(shape_val);

                let shape_connections: Vec<DiagramConnection> = connections
                    .iter()
                    .filter(|c| {
                        id.as_ref()
                            .is_some_and(|sid| c.from_id == *sid || c.to_id == *sid)
                    })
                    .cloned()
                    .collect();

                shapes.push(DiagramShape {
                    shape_block: ShapeBlock {
                        shape_type,
                        text,
                        geometry: Some(Rect::new(x, y, w, h)),
                        style,
                        source: Some(
                            SourceInfo::new()
                                .with_confidence(0.9)
                                .with_region(Rect::new(x, y, w, h)),
                        ),
                        transform: None,
                        layer: None,
                        accessibility: None,
                    },
                    shape_id: id,
                    connections: shape_connections,
                });
            }
        }

        shapes
    }
}

fn parse_shape_type(s: &str) -> ShapeType {
    match s.to_lowercase().as_str() {
        "rectangle" | "process" => ShapeType::Rectangle,
        "ellipse" | "start" | "end" | "oval" => ShapeType::Ellipse,
        "diamond" | "decision" => ShapeType::FlowchartDecision,
        "parallelogram" | "input" | "output" => ShapeType::FlowchartProcess,
        "arrow" | "line" => ShapeType::Arrow,
        _ => ShapeType::Custom,
    }
}

fn parse_style(val: &serde_json::Value) -> Option<ShapeStyle> {
    let style = &val["style"];
    if style.is_null() {
        return None;
    }
    Some(ShapeStyle {
        fill_color: style["fill"].as_str().map(|s| latexsnipper_ast::Color {
            value: s.to_string(),
            alpha: None,
        }),
        stroke_color: style["stroke"].as_str().map(|s| latexsnipper_ast::Color {
            value: s.to_string(),
            alpha: None,
        }),
        stroke_width: style["stroke_width"].as_f64().map(|v| v as f32),
        opacity: None,
    })
}

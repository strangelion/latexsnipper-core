use latexsnipper_ast::{
    ChartAxis, ChartBlock, ChartData, ChartLegend, ChartSeries, ChartType, Diagnostic,
    DiagnosticLevel, ProviderReport,
};
use latexsnipper_runtime::api_provider::{ApiProviderConfig, PromptPreset, PromptProfile};
use latexsnipper_runtime::RemoteApiProvider;

/// Result of chart understanding.
#[derive(Debug, Clone)]
pub struct ChartUnderstandingResult {
    pub chart_block: ChartBlock,
    pub diagnostics: Vec<Diagnostic>,
    pub provider_report: ProviderReport,
    pub raw_response: String,
}

/// Service that uses a VLM (Vision Language Model) to extract structured chart
/// data from a chart image, producing a populated `ChartBlock`.
///
/// # Example
/// ```no_run
/// # use latexsnipper_engine::chart_understanding::ChartUnderstandingService;
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
/// let service = ChartUnderstandingService::new(config);
/// // let result = service.understand_chart(image_base64).await;
/// ```
pub struct ChartUnderstandingService {
    provider: RemoteApiProvider,
}

impl ChartUnderstandingService {
    /// Create a new chart understanding service with the given API config.
    pub fn new(config: ApiProviderConfig) -> Self {
        Self {
            provider: RemoteApiProvider::new(config),
        }
    }

    /// Analyze a chart image (base64-encoded PNG) and extract structured data.
    ///
    /// Returns a `ChartBlock` populated with detected axes, series, legend,
    /// and chart type. If the VLM fails or returns invalid data, diagnostics
    /// are populated and a fallback `ChartBlock` is returned.
    pub async fn understand_chart(&self, image_base64: &str) -> ChartUnderstandingResult {
        let profile = PromptProfile::from_preset(PromptPreset::ChartExtraction);

        let (result, mut diagnostics, report) =
            self.provider.execute(&profile, Some(image_base64)).await;

        if !result.is_usable() {
            return ChartUnderstandingResult {
                chart_block: Self::fallback_chart_block(),
                diagnostics,
                provider_report: report,
                raw_response: result.text,
            };
        }

        // Parse the JSON response into a ChartBlock
        let chart_block = match &result.parsed_json {
            Some(json) => Self::parse_chart_json(json),
            None => {
                let mut diags = diagnostics;
                diags.push(
                    Diagnostic::new(
                        DiagnosticLevel::Warning,
                        "E_CHART_PARSE",
                        "VLM response was not valid JSON, using fallback",
                    )
                    .with_recoverable(true),
                );
                diagnostics = diags;
                Self::fallback_chart_block()
            }
        };

        ChartUnderstandingResult {
            chart_block,
            diagnostics,
            provider_report: report,
            raw_response: result.text,
        }
    }

    /// Parse validated JSON into a ChartBlock.
    fn parse_chart_json(json: &serde_json::Value) -> ChartBlock {
        let chart_type = json["chart_type"]
            .as_str()
            .and_then(|s| match s.to_lowercase().as_str() {
                "bar" => Some(ChartType::Bar),
                "line" => Some(ChartType::Line),
                "pie" => Some(ChartType::Pie),
                "scatter" => Some(ChartType::Scatter),
                "area" => Some(ChartType::Area),
                _ => None,
            })
            .unwrap_or(ChartType::Unknown);

        let title = json["title"].as_str().map(|s| {
            vec![latexsnipper_ast::Inline::Text(
                latexsnipper_ast::TextRun::new(s),
            )]
        });

        let x_axis = ChartAxis {
            label: json["x_axis"]["label"].as_str().map(|s| s.to_string()),
            min: json["x_axis"]["min"].as_f64(),
            max: json["x_axis"]["max"].as_f64(),
        };

        let y_axis = ChartAxis {
            label: json["y_axis"]["label"].as_str().map(|s| s.to_string()),
            min: json["y_axis"]["min"].as_f64(),
            max: json["y_axis"]["max"].as_f64(),
        };

        let labels: Vec<String> = json["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let series: Vec<ChartSeries> = json["series"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|s| ChartSeries {
                        name: s["name"].as_str().unwrap_or("").to_string(),
                        values: s["values"]
                            .as_array()
                            .map(|v| v.iter().filter_map(|n| n.as_f64()).collect())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let legend = json["legend"].as_object().map(|_| ChartLegend {
            visible: json["legend"]["visible"].as_bool().unwrap_or(true),
            position: json["legend"]["position"].as_str().map(|s| s.to_string()),
        });

        ChartBlock {
            chart_type,
            title,
            asset_id: None,
            data: Some(ChartData { labels, series }),
            axes: vec![x_axis, y_axis],
            legend,
            geometry: None,
            source: None,
            transform: None,
            layer: None,
            accessibility: None,
        }
    }

    fn fallback_chart_block() -> ChartBlock {
        ChartBlock {
            chart_type: ChartType::Unknown,
            title: None,
            asset_id: None,
            data: None,
            axes: Vec::new(),
            legend: None,
            geometry: None,
            source: None,
            transform: None,
            layer: None,
            accessibility: None,
        }
    }
}

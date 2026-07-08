use serde::{Deserialize, Serialize};

/// A color value with optional alpha.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    /// #RRGGBB or named color.
    pub value: String,
    pub alpha: Option<f32>,
}

/// Font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
    Justify,
}

/// Vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
    Baseline,
}

/// Text style for inline runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<FontWeight>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub vertical_align: Option<VerticalAlign>,
}

/// Paragraph style.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParagraphStyle {
    pub alignment: Option<TextAlignment>,
    pub indent_left: Option<f32>,
    pub indent_right: Option<f32>,
    pub first_line_indent: Option<f32>,
    pub line_spacing: Option<f32>,
    pub space_before: Option<f32>,
    pub space_after: Option<f32>,
}

/// Box style for text boxes and shapes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BoxStyle {
    pub fill_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub border_style: Option<crate::BorderStyle>,
    pub padding: Option<f32>,
    pub opacity: Option<f32>,
}

/// Shape style.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShapeStyle {
    pub fill_color: Option<Color>,
    pub stroke_color: Option<Color>,
    pub stroke_width: Option<f32>,
    pub opacity: Option<f32>,
}

/// Chart type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartType {
    Line,
    Bar,
    Column,
    Pie,
    Scatter,
    Area,
    Histogram,
    BoxPlot,
    Heatmap,
    Unknown,
}

/// Chart data (simplified).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub labels: Vec<String>,
    pub series: Vec<ChartSeries>,
}

/// A single chart data series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
}

/// Chart axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartAxis {
    pub label: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Chart legend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartLegend {
    pub visible: bool,
    pub position: Option<String>,
}

/// Embedded object kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddedObjectKind {
    OleObject,
    OfficeChart,
    SmartArt,
    DrawingCanvas,
    EquationObject,
    PdfObject,
    Unknown,
}

/// Annotation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationKind {
    Comment,
    Highlight,
    Ink,
    Strikeout,
    Underline,
    StickyNote,
    Unknown,
}

/// Shape type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Connector,
    Callout,
    FlowchartProcess,
    FlowchartDecision,
    Custom,
    Unknown,
}

/// Office application source info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeSourceInfo {
    pub app: OfficeApp,
    pub document_id: Option<String>,
    pub part_name: Option<String>,
    pub relationship_id: Option<String>,
    pub shape_id: Option<String>,
    pub slide_index: Option<u32>,
    pub sheet_name: Option<String>,
    pub cell_range: Option<String>,
}

/// Office application type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfficeApp {
    Word,
    PowerPoint,
    Excel,
}

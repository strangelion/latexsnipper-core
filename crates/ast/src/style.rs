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

/// Merge two TextStyles, with `overrides` taking priority over `base`.
///
/// Any `Some` value in `overrides` replaces the corresponding field in `base`.
/// `None` fields are inherited from `base`.
fn merge_style(base: &TextStyle, overrides: &TextStyle) -> TextStyle {
    TextStyle {
        font_family: overrides
            .font_family
            .clone()
            .or_else(|| base.font_family.clone()),
        font_size: overrides.font_size.or(base.font_size),
        font_weight: overrides.font_weight.or(base.font_weight),
        italic: overrides.italic.or(base.italic),
        underline: overrides.underline.or(base.underline),
        strikethrough: overrides.strikethrough.or(base.strikethrough),
        color: overrides.color.clone().or_else(|| base.color.clone()),
        background: overrides
            .background
            .clone()
            .or_else(|| base.background.clone()),
        vertical_align: overrides.vertical_align.or(base.vertical_align),
    }
}

/// Compute the effective TextStyle for a TextRun, considering:
///
/// 1. The run's individual `style: Option<TextStyle>` (when TextRun has migrated to use it)
/// 2. The run's legacy `bold`/`italic`/`underline`/`strikethrough` fields
/// 3. An optional inherited style from `SpanInline.style` or `ParagraphBlock.style`
///
/// Priority (highest wins): run.style > run.legacy > inherited > defaults
pub fn effective_text_style(
    run_bold: Option<bool>,
    run_italic: Option<bool>,
    run_underline: Option<bool>,
    run_strikethrough: Option<bool>,
    run_style: Option<&TextStyle>,
    inherited: Option<&TextStyle>,
) -> TextStyle {
    let mut result = inherited.cloned().unwrap_or_default();

    // Apply run-level style overlay
    if let Some(style) = run_style {
        result = merge_style(&result, style);
    }

    // Apply legacy format flags (they override style fields)
    if let Some(true) = run_bold {
        result.font_weight = Some(FontWeight::Bold);
    }
    if run_italic == Some(true) {
        result.italic = Some(true);
    }
    if run_underline == Some(true) {
        result.underline = Some(true);
    }
    if run_strikethrough == Some(true) {
        result.strikethrough = Some(true);
    }

    result
}

// ---------------------------------------------------------------------------
// TextDirection
// ---------------------------------------------------------------------------

/// Text direction (left-to-right, right-to-left, auto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDirection {
    Ltr,
    Rtl,
    Auto,
}

// ---------------------------------------------------------------------------
// UnderlineStyle
// ---------------------------------------------------------------------------

/// Style of underline decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnderlineStyle {
    Single,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

// ---------------------------------------------------------------------------
// Transform2D
// ---------------------------------------------------------------------------

/// 2D transformation (rotation, scale, translation, skew).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transform2D {
    pub rotation_deg: Option<f32>,
    pub scale_x: Option<f32>,
    pub scale_y: Option<f32>,
    pub translate_x: Option<f32>,
    pub translate_y: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
}

// ---------------------------------------------------------------------------
// LayerInfo
// ---------------------------------------------------------------------------

/// Z-order and visibility information for layering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayerInfo {
    pub z_index: Option<i32>,
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
    pub group_id: Option<String>,
}

// ---------------------------------------------------------------------------
// AccessibilityInfo
// ---------------------------------------------------------------------------

/// Accessibility information for content elements.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessibilityInfo {
    pub alt_text: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub decorative: Option<bool>,
    pub reading_order: Option<u32>,
    pub language: Option<String>,
}

// ---------------------------------------------------------------------------
// ListStyle / BulletStyle / NumberingStyle
// ---------------------------------------------------------------------------

/// Style of a bullet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulletStyle {
    Disc,
    Circle,
    Square,
    Dash,
    Custom(String),
}

/// Numbering style for ordered lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberingStyle {
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    Chinese,
    Custom(String),
}

/// Overall list styling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListStyle {
    Bullet(BulletStyle),
    Ordered(NumberingStyle),
    Task,
    Definition,
}

// ---------------------------------------------------------------------------
// VectorPath / PathCommand
// ---------------------------------------------------------------------------

/// A vector path composed of drawing commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPath {
    pub commands: Vec<PathCommand>,
}

/// A single path command in a vector path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathCommand {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    CurveTo { x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32 },
    ClosePath,
}

// ---------------------------------------------------------------------------
// ShapeGroup
// ---------------------------------------------------------------------------

/// A group of shapes with an optional transform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeGroup {
    pub shapes: Vec<super::block::ShapeBlock>,
    pub transform: Option<super::style::Transform2D>,
}

use serde::{Deserialize, Serialize};

use crate::{Inline, NodeId, Rect, SourceInfo};
use crate::style::{BoxStyle, ChartAxis, ChartData, ChartLegend, ChartType, EmbeddedObjectKind, ShapeStyle, ShapeType, AnnotationKind};
use crate::media::MediaRole;
use crate::span::BlockPolicy;

/// A layout block in the document.
///
/// This is the core enum for all block-level content.
/// All variants follow the same pattern: `{Name}Block` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Block {
    /// A heading/section title (H1-H6).
    Heading(HeadingBlock),
    /// A paragraph of inline content.
    Paragraph(ParagraphBlock),
    /// A standalone formula (display math).
    Formula(FormulaBlock),
    /// A table with rows and cells.
    Table(TableBlock),
    /// An image/figure with optional caption.
    Figure(FigureBlock),
    /// An ordered or unordered list.
    List(ListBlock),
    /// A blockquote with optional attribution.
    Quote(QuoteBlock),
    /// A code block with optional language.
    Code(CodeBlock),
    /// A horizontal rule/divider.
    HorizontalRule(HorizontalRuleBlock),
    /// A handwritten text block.
    Handwriting(HandwritingBlock),
    /// A description list (\begin{description}).
    DescriptionList(DescriptionListBlock),
    /// A table of contents (\tableofcontents).
    TableOfContents,
    /// A theorem-like environment (\begin{theorem}).
    Theorem(TheoremBlock),
    /// A proof environment (\begin{proof}).
    Proof(ProofBlock),
    /// A minipage environment (\begin{minipage}).
    Minipage(MinipageBlock),
    /// A float environment (\begin{figure} or \begin{table}).
    Float(FloatBlock),
    /// A text box (Office/PDF/PPT).
    TextBox(TextBoxBlock),
    /// A chart block (Excel/PPT/论文图片).
    Chart(ChartBlock),
    /// A shape block (Office arrow, rectangle, flowchart shape).
    Shape(ShapeBlock),
    /// An embedded object (OLE, Office Chart, SmartArt, etc.).
    EmbeddedObject(EmbeddedObjectBlock),
    /// An annotation/comment/highlight.
    Annotation(AnnotationBlock),
}

impl Block {
    /// Get the source info for this block.
    pub fn source(&self) -> Option<&SourceInfo> {
        match self {
            Block::Heading(h) => h.source.as_ref(),
            Block::Paragraph(p) => p.source.as_ref(),
            Block::Formula(f) => f.source.as_ref(),
            Block::Table(t) => t.source.as_ref(),
            Block::Figure(f) => f.source.as_ref(),
            Block::List(l) => l.source.as_ref(),
            Block::Quote(q) => q.source.as_ref(),
            Block::Code(c) => c.source.as_ref(),
            Block::HorizontalRule(h) => h.source.as_ref(),
            Block::Handwriting(hw) => hw.source.as_ref(),
            Block::DescriptionList(dl) => dl.source.as_ref(),
            Block::TableOfContents => None,
            Block::Theorem(t) => t.source.as_ref(),
            Block::Proof(p) => p.source.as_ref(),
            Block::Minipage(m) => m.source.as_ref(),
            Block::Float(f) => f.source.as_ref(),
            Block::TextBox(tb) => tb.source.as_ref(),
            Block::Chart(c) => c.source.as_ref(),
            Block::Shape(s) => s.source.as_ref(),
            Block::EmbeddedObject(e) => e.source.as_ref(),
            Block::Annotation(a) => a.source.as_ref(),
        }
    }

    /// Get the node ID for this block.
    pub fn node_id(&self) -> Option<NodeId> {
        self.source().and_then(|s| s.node_id)
    }

    /// Get mutable source info for this block.
    pub fn source_mut(&mut self) -> Option<&mut SourceInfo> {
        match self {
            Block::Heading(h) => h.source.as_mut(),
            Block::Paragraph(p) => p.source.as_mut(),
            Block::Formula(f) => f.source.as_mut(),
            Block::Table(t) => t.source.as_mut(),
            Block::Figure(f) => f.source.as_mut(),
            Block::List(l) => l.source.as_mut(),
            Block::Quote(q) => q.source.as_mut(),
            Block::Code(c) => c.source.as_mut(),
            Block::HorizontalRule(h) => h.source.as_mut(),
            Block::Handwriting(hw) => hw.source.as_mut(),
            Block::DescriptionList(dl) => dl.source.as_mut(),
            Block::TableOfContents => None,
            Block::Theorem(t) => t.source.as_mut(),
            Block::Proof(p) => p.source.as_mut(),
            Block::Minipage(m) => m.source.as_mut(),
            Block::Float(f) => f.source.as_mut(),
            Block::TextBox(tb) => tb.source.as_mut(),
            Block::Chart(c) => c.source.as_mut(),
            Block::Shape(s) => s.source.as_mut(),
            Block::EmbeddedObject(e) => e.source.as_mut(),
            Block::Annotation(a) => a.source.as_mut(),
        }
    }

    /// Get geometry for this block.
    pub fn geometry(&self) -> Option<&Rect> {
        match self {
            Block::Heading(h) => h.geometry.as_ref(),
            Block::Paragraph(p) => p.geometry.as_ref(),
            Block::Formula(f) => f.geometry.as_ref(),
            Block::Table(t) => t.geometry.as_ref(),
            Block::Figure(f) => f.geometry.as_ref(),
            Block::List(l) => l.geometry.as_ref(),
            Block::Quote(q) => q.geometry.as_ref(),
            Block::Code(c) => c.geometry.as_ref(),
            Block::HorizontalRule(h) => h.geometry.as_ref(),
            Block::Handwriting(hw) => hw.geometry.as_ref(),
            Block::DescriptionList(dl) => dl.geometry.as_ref(),
            Block::TableOfContents => None,
            Block::Theorem(t) => t.geometry.as_ref(),
            Block::Proof(p) => p.geometry.as_ref(),
            Block::Minipage(m) => m.geometry.as_ref(),
            Block::Float(f) => f.geometry.as_ref(),
            Block::TextBox(tb) => tb.geometry.as_ref(),
            Block::Chart(c) => c.geometry.as_ref(),
            Block::Shape(s) => s.geometry.as_ref(),
            Block::EmbeddedObject(e) => e.geometry.as_ref(),
            Block::Annotation(a) => a.geometry.as_ref(),
        }
    }

    /// Iterate over child inline elements.
    pub fn inlines(&self) -> Vec<&Inline> {
        match self {
            Block::Heading(h) => h.inlines.iter().collect(),
            Block::Paragraph(p) => p.inlines.iter().collect(),
            Block::Formula(_) => vec![],
            Block::Table(t) => t
                .rows
                .iter()
                .flat_map(|row| row.iter())
                .flat_map(|cell| cell.inlines.iter())
                .collect(),
            Block::Figure(_) => vec![],
            Block::List(l) => l
                .items
                .iter()
                .flat_map(|item| item.inlines.iter())
                .collect(),
            Block::Quote(q) => q.blocks.iter().flat_map(|b| b.inlines()).collect(),
            Block::Code(_) => vec![],
            Block::HorizontalRule(_) => vec![],
            Block::Handwriting(hw) => hw.inlines.iter().collect(),
            Block::DescriptionList(dl) => dl
                .items
                .iter()
                .filter_map(|item| item.label.as_ref())
                .flat_map(|label| label.iter())
                .collect(),
            Block::TableOfContents => vec![],
            Block::Theorem(t) => t.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::Proof(p) => p.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::Minipage(m) => m.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::Float(f) => f.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::TextBox(tb) => tb.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::Chart(c) => c.title.iter().flat_map(|t| t.iter()).collect(),
            Block::Shape(s) => s.text.iter().collect(),
            Block::EmbeddedObject(_) => vec![],
            Block::Annotation(a) => a.content.iter().collect(),
        }
    }

    /// Get a human-readable name for this block type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Block::Heading(_) => "heading",
            Block::Paragraph(_) => "paragraph",
            Block::Formula(_) => "formula",
            Block::Table(_) => "table",
            Block::Figure(_) => "figure",
            Block::List(_) => "list",
            Block::Quote(_) => "quote",
            Block::Code(_) => "code",
            Block::HorizontalRule(_) => "horizontal_rule",
            Block::Handwriting(_) => "handwriting",
            Block::DescriptionList(_) => "description_list",
            Block::TableOfContents => "table_of_contents",
            Block::Theorem(_) => "theorem",
            Block::Proof(_) => "proof",
            Block::Minipage(_) => "minipage",
            Block::Float(_) => "float",
            Block::TextBox(_) => "text_box",
            Block::Chart(_) => "chart",
            Block::Shape(_) => "shape",
            Block::EmbeddedObject(_) => "embedded_object",
            Block::Annotation(_) => "annotation",
        }
    }
}

/// A heading block (H1-H6).
///
/// Used for section titles, document structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadingBlock {
    /// Heading level (1-6).
    pub level: u8,
    /// Inline content of the heading.
    pub inlines: Vec<Inline>,
    /// Optional anchor ID for linking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A paragraph containing inline elements.
///
/// The most common block type for text content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParagraphBlock {
    /// Inline content of the paragraph.
    pub inlines: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A standalone formula block (display math).
///
/// Used for equations that appear on their own line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaBlock {
    /// The formula content.
    pub formula: crate::Formula,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A table block.
///
/// Contains rows of cells, each cell can have inline content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableBlock {
    /// Table rows, each row is a vector of cells.
    pub rows: Vec<Vec<TableCell>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// Border style for table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BorderStyle {
    None,
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

/// Text alignment for table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CellAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// A table cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    /// Inline content of the cell.
    pub inlines: Vec<Inline>,
    /// Number of columns this cell spans.
    pub colspan: u32,
    /// Number of rows this cell spans.
    pub rowspan: u32,
    /// Border style for this cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_style: Option<BorderStyle>,
    /// Border width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width: Option<u32>,
    /// Border color (hex or name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    /// Background color (hex or name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Text alignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<CellAlignment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// An image/figure block.
///
/// Used for standalone images with optional caption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureBlock {
    /// Reference to a media asset in the document's asset collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<crate::AssetId>,
    /// Image data (base64 encoded or file path).
    /// DEPRECATED: Use `asset_id` instead. This field is kept for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>,
    /// Optional caption text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Semantic role of the figure (photo, diagram, chart, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MediaRole>,
    /// Per-block processing policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<BlockPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A list block (ordered or unordered).
///
/// Contains list items, each with inline content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBlock {
    /// True for ordered (numbered), false for unordered (bulleted).
    pub ordered: bool,
    /// List items.
    pub items: Vec<ListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A single list item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    /// Inline content of the item.
    pub inlines: Vec<Inline>,
    /// For task lists: checked state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A blockquote.
///
/// Contains nested blocks with optional attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteBlock {
    /// Nested blocks inside the quote.
    pub blocks: Vec<Block>,
    /// Optional attribution (e.g., "— Shakespeare").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A code block.
///
/// Used for preformatted text, code snippets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Programming language (e.g., "rust", "python").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The code content.
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A horizontal rule/divider.
///
/// Used for visual separation between sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizontalRuleBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

impl HorizontalRuleBlock {
    pub fn new() -> Self {
        Self {
            geometry: None,
            source: None,
        }
    }
}

impl Default for HorizontalRuleBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// A handwritten text block.
///
/// Used for content detected as handwriting in the input image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandwritingBlock {
    /// Recognized inline content (text or formulas).
    pub inlines: Vec<Inline>,
    /// Confidence score for the handwriting detection/recognition.
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// An item in a description list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionItem {
    /// Optional label (e.g., term in a glossary).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<Vec<Inline>>,
    /// Content associated with the label.
    pub content: Vec<Block>,
}

/// A description list block (\begin{description}).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionListBlock {
    /// List items with optional labels.
    pub items: Vec<DescriptionItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A theorem-like block (\begin{theorem}, \begin{lemma}, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TheoremBlock {
    /// Theorem name (e.g., "Theorem", "Lemma", "Corollary").
    pub name: String,
    /// Optional theorem number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// Theorem content.
    pub content: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A proof block (\begin{proof}).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBlock {
    /// Proof content.
    pub content: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A minipage block (\begin{minipage}{width}).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinipageBlock {
    /// Width of the minipage (e.g., "0.5\textwidth").
    pub width: String,
    /// Content inside the minipage.
    pub content: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A float block (\begin{figure} or \begin{table}).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatBlock {
    /// Float environment name ("figure" or "table").
    pub env: String,
    /// Optional caption.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<Vec<Inline>>,
    /// Content inside the float.
    pub content: Vec<Block>,
    /// Optional placement specifier (e.g., "htbp").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A text box block (Office/PDF/PPT).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBoxBlock {
    /// Content blocks inside the text box.
    pub content: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    /// Rotation in degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_deg: Option<f32>,
    /// Z-index for layering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<BoxStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A chart block (Excel/PPT/paper figures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartBlock {
    pub chart_type: ChartType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Vec<Inline>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<crate::AssetId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ChartData>,
    #[serde(default)]
    pub axes: Vec<ChartAxis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<ChartLegend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A shape block (Office arrow, rectangle, flowchart shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeBlock {
    pub shape_type: ShapeType,
    #[serde(default)]
    pub text: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ShapeStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// An embedded object block (OLE, Office Chart, SmartArt, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedObjectBlock {
    pub kind: EmbeddedObjectKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<crate::AssetId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_asset_id: Option<crate::AssetId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// An annotation block (comment, highlight, ink, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationBlock {
    pub kind: AnnotationKind,
    #[serde(default)]
    pub content: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

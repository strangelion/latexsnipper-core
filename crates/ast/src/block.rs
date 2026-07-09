use serde::{Deserialize, Serialize};

use crate::media::MediaRole;
use crate::span::BlockPolicy;
use crate::style::{
    AnnotationKind, BoxStyle, ChartAxis, ChartData, ChartLegend, ChartType, EmbeddedObjectKind,
    ListStyle, ShapeStyle, ShapeType,
};
use crate::{Inline, NodeId, Rect, SourceInfo};

/// A layout block in the document.
///
/// This is the core enum for all block-level content.
/// All variants follow the same pattern: `{Name}Block` struct.
///
/// Some variants (FormulaBlock, ChartBlock, etc.) differ significantly in size.
/// Boxing individual variants would break exhaustive pattern matching across
/// the codebase, so the size difference is accepted by explicit allow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
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
    /// A page break (hard page boundary).
    PageBreak(PageBreakBlock),
    /// A section break with optional page layout change.
    SectionBreak(SectionBreakBlock),
    /// A header or footer block.
    HeaderFooter(HeaderFooterBlock),
    /// A bibliography/references block.
    Bibliography(BibliographyBlock),
    /// A form field (Office/PDF forms).
    FormField(FormFieldBlock),
    /// A tracked revision (inserted/deleted text etc.).
    Revision(Revision),
    /// A chemical formula (Office ChemDraw etc.).
    ChemicalFormula(ChemicalFormulaBlock),
    /// A QR code / barcode block.
    QrCode(QrCodeBlock),
    /// A data graph/plot block.
    Graph(GraphBlock),
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
            Block::PageBreak(pb) => pb.source.as_ref(),
            Block::SectionBreak(sb) => sb.source.as_ref(),
            Block::HeaderFooter(hf) => hf.source.as_ref(),
            Block::Bibliography(bb) => bb.source.as_ref(),
            Block::FormField(ff) => ff.source.as_ref(),
            Block::Revision(_) => None,
            Block::ChemicalFormula(cf) => cf.source.as_ref(),
            Block::QrCode(qr) => qr.source.as_ref(),
            Block::Graph(g) => g.source.as_ref(),
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
            Block::PageBreak(pb) => pb.source.as_mut(),
            Block::SectionBreak(sb) => sb.source.as_mut(),
            Block::HeaderFooter(hf) => hf.source.as_mut(),
            Block::Bibliography(bb) => bb.source.as_mut(),
            Block::FormField(ff) => ff.source.as_mut(),
            Block::Revision(_) => None,
            Block::ChemicalFormula(cf) => cf.source.as_mut(),
            Block::QrCode(qr) => qr.source.as_mut(),
            Block::Graph(g) => g.source.as_mut(),
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
            Block::PageBreak(_) => None,
            Block::SectionBreak(_) => None,
            Block::HeaderFooter(_) => None,
            Block::Bibliography(bb) => bb.geometry.as_ref(),
            Block::FormField(ff) => ff.geometry.as_ref(),
            Block::Revision(_) => None,
            Block::ChemicalFormula(cf) => cf.geometry.as_ref(),
            Block::QrCode(qr) => qr.geometry.as_ref(),
            Block::Graph(g) => g.geometry.as_ref(),
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
                .flat_map(|row| row.cells.iter())
                .flat_map(|cell| cell.content.iter())
                .flat_map(|b| b.inlines())
                .collect(),
            Block::Figure(_) => vec![],
            Block::List(l) => l
                .items
                .iter()
                .flat_map(|item| item.content.iter())
                .flat_map(|b| b.inlines())
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
            Block::PageBreak(_) => vec![],
            Block::SectionBreak(_) => vec![],
            Block::HeaderFooter(hf) => hf.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::Bibliography(_) => vec![],
            Block::FormField(_) => vec![],
            Block::Revision(r) => r.content.iter().flat_map(|b| b.inlines()).collect(),
            Block::ChemicalFormula(_) => vec![],
            Block::QrCode(_) => vec![],
            Block::Graph(_) => vec![],
        }
    }

    /// Get mutable access to the child inline elements of this block.
    ///
    /// Returns `None` for block types that do not have direct inline children
    /// (e.g., Formula, Code, Figure, nested container blocks).
    /// For container blocks (Quote, Theorem, etc.), this returns `None` since
    /// child blocks need separate recursive traversal.
    pub fn inlines_mut(&mut self) -> Option<Vec<&mut Inline>> {
        match self {
            Block::Heading(h) => Some(h.inlines.iter_mut().collect()),
            Block::Paragraph(p) => Some(p.inlines.iter_mut().collect()),
            Block::Formula(_) => None,
            Block::Table(_) => None,
            Block::Figure(_) => None,
            Block::List(_) => None,
            Block::Quote(_) => None,
            Block::Code(_) => None,
            Block::HorizontalRule(_) => None,
            Block::Handwriting(hw) => Some(hw.inlines.iter_mut().collect()),
            Block::DescriptionList(dl) => Some(
                dl.items
                    .iter_mut()
                    .filter_map(|item| item.label.as_mut())
                    .flat_map(|label| label.iter_mut())
                    .collect(),
            ),
            Block::TableOfContents => None,
            Block::Theorem(_) => None,
            Block::Proof(_) => None,
            Block::Minipage(_) => None,
            Block::Float(_) => None,
            Block::TextBox(_) => None,
            Block::Chart(c) => Some(c.title.iter_mut().flat_map(|t| t.iter_mut()).collect()),
            Block::Shape(s) => Some(s.text.iter_mut().collect()),
            Block::EmbeddedObject(_) => None,
            Block::Annotation(a) => Some(a.content.iter_mut().collect()),
            Block::PageBreak(_) => None,
            Block::SectionBreak(_) => None,
            Block::HeaderFooter(_) => None,
            Block::Bibliography(_) => None,
            Block::FormField(_) => None,
            Block::Revision(_) => None,
            Block::ChemicalFormula(_) => None,
            Block::QrCode(_) => None,
            Block::Graph(_) => None,
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
            Block::PageBreak(_) => "page_break",
            Block::SectionBreak(_) => "section_break",
            Block::HeaderFooter(_) => "header_footer",
            Block::Bibliography(_) => "bibliography",
            Block::FormField(_) => "form_field",
            Block::Revision(_) => "revision",
            Block::ChemicalFormula(_) => "chemical_formula",
            Block::QrCode(_) => "qr_code",
            Block::Graph(_) => "graph",
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
    /// Paragraph-level style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<crate::ParagraphStyle>,
}

/// A standalone formula block (display math).
///
/// Used for equations that appear on their own line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaBlock {
    /// The formula content.
    pub formula: crate::Formula,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<FormulaEnvironment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// The kind of formula environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormulaEnvironment {
    Equation,
    Align,
    Gather,
    Multline,
    Cases,
    Matrix,
    Inline,
    Display,
    Unknown,
}

/// A table block.
///
/// Contains rows and columns with rich cell content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableBlock {
    /// Table rows.
    pub rows: Vec<TableRow>,
    /// Table columns.
    #[serde(default)]
    pub columns: Vec<TableColumn>,
    /// Optional caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<Vec<Inline>>,
    /// Table-level style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TableStyle>,
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
    /// Block content of the cell (replaces `inlines` — more flexible).
    pub content: Vec<Block>,
    /// Number of columns this cell spans.
    pub colspan: u32,
    /// Number of rows this cell spans.
    pub rowspan: u32,
    /// The type of data in this cell (text, number, boolean, date, formula, empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<CellDataType>,
    /// Optional formula string (e.g., "SUM(A1:A10)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// Cell-level style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TableCellStyle>,
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

impl TableCell {
    /// Collect all inline elements from cell content blocks.
    pub fn collect_inlines(&self) -> Vec<Inline> {
        self.content
            .iter()
            .flat_map(|b| b.inlines().into_iter().cloned())
            .collect()
    }

    /// Build a TableCellStyle from legacy fields + style field.
    /// style field takes priority over legacy fields.
    pub fn effective_style(&self) -> crate::TableCellStyle {
        let mut s = self.legacy_style_as_table_cell_style();
        if let Some(ref style) = self.style {
            if style.background.is_some() {
                s.background = style.background.clone();
            }
            if style.vertical_align.is_some() {
                s.vertical_align = style.vertical_align;
            }
            if style.horizontal_align.is_some() {
                s.horizontal_align = style.horizontal_align;
            }
        }
        s
    }

    /// Convert legacy border/background/alignment fields to TableCellStyle.
    pub fn legacy_style_as_table_cell_style(&self) -> crate::TableCellStyle {
        let border = if self.border_style.is_some()
            || self.border_width.is_some()
            || self.border_color.is_some()
        {
            Some(crate::TableBorder {
                top: Some(crate::BorderSide {
                    style: self.border_style.unwrap_or(crate::BorderStyle::Solid),
                    width: self.border_width.map(|w| w as f32),
                    color: self.border_color.as_ref().map(|c| crate::Color {
                        value: c.clone(),
                        alpha: None,
                    }),
                }),
                right: None,
                bottom: None,
                left: None,
            })
        } else {
            None
        };
        crate::TableCellStyle {
            background: self.background.as_ref().map(|c| crate::Color {
                value: c.clone(),
                alpha: None,
            }),
            vertical_align: None,
            horizontal_align: self.alignment.map(|a| match a {
                crate::CellAlignment::Left => crate::TextAlignment::Left,
                crate::CellAlignment::Center => crate::TextAlignment::Center,
                crate::CellAlignment::Right => crate::TextAlignment::Right,
                crate::CellAlignment::Justify => crate::TextAlignment::Justify,
            }),
            border,
        }
    }
}

/// A table row with optional height and header flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRow {
    /// Cells in this row.
    pub cells: Vec<TableCell>,
    /// Row height in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Whether this is a header row.
    #[serde(default)]
    pub is_header: bool,
}

/// A table column with optional width.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    /// Column width in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Whether this is a header column.
    #[serde(default)]
    pub is_header: bool,
}

/// The type of data stored in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellDataType {
    Text,
    Number,
    Boolean,
    Date,
    Formula,
    Empty,
}

/// Table-level style.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_collapse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<crate::TextAlignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banded_rows: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banded_columns: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_row: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_row: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_column: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_column: Option<bool>,
}

/// Cell-level style.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableCellStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<crate::Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<crate::VerticalAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_align: Option<crate::TextAlignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<crate::TableBorder>,
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
    /// Optional caption text (plain text only).
    /// DEPRECATED: Use `caption_inlines` instead for structured content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Structured caption with full inline support (formulas, formatting, links).
    /// When set, takes priority over `caption`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_inlines: Option<Vec<crate::Inline>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
}

impl FigureBlock {
    /// Returns the caption inlines, falling back to legacy plain text caption.
    pub fn caption_inlines_or_legacy(&self) -> Vec<crate::Inline> {
        self.caption_inlines.clone().unwrap_or_else(|| {
            self.caption
                .as_ref()
                .map(|c| vec![crate::Inline::Text(crate::TextRun::new(c))])
                .unwrap_or_default()
        })
    }

    /// Returns the caption as plain text, preferring structured inlines.
    pub fn caption_plain_text(&self) -> String {
        if let Some(inlines) = &self.caption_inlines {
            let mut text = String::new();
            for inline in inlines {
                if let crate::Inline::Text(t) = inline {
                    text.push_str(&t.text);
                }
            }
            text
        } else {
            self.caption.clone().unwrap_or_default()
        }
    }
}

/// A list block (ordered or unordered).
///
/// Contains list items, each with inline content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBlock {
    /// List style (bullet/decimal/task/definition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ListStyle>,
    /// List items.
    pub items: Vec<ListItem>,
    /// Starting number for ordered lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

impl ListBlock {
    /// Returns `true` if the list is ordered (numbered).
    pub fn is_ordered(&self) -> bool {
        matches!(self.style, Some(ListStyle::Ordered(_)))
    }
}

/// A single list item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    /// Optional marker override (e.g., custom bullet character).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Block content of the item (paragraphs, formulas, nested lists, etc.).
    #[serde(default)]
    pub content: Vec<Block>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
}

impl TextBoxBlock {
    /// Returns the effective transform: prefers the explicit `transform` field,
    /// falls back to a basic rotation-only transform from `rotation_deg`.
    pub fn effective_transform(&self) -> Option<crate::Transform2D> {
        self.transform.clone().or_else(|| {
            self.rotation_deg.map(|deg| crate::Transform2D {
                rotation_deg: Some(deg),
                scale_x: None,
                scale_y: None,
                translate_x: None,
                translate_y: None,
                skew_x: None,
                skew_y: None,
            })
        })
    }

    /// Returns the effective layer info: prefers the explicit `layer` field,
    /// falls back to a basic layer from `z_index`.
    pub fn effective_layer(&self) -> Option<crate::LayerInfo> {
        self.layer.clone().or_else(|| {
            self.z_index.map(|z| crate::LayerInfo {
                z_index: Some(z),
                locked: None,
                hidden: None,
                group_id: None,
            })
        })
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_ref: Option<crate::AssetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_as_icon: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
}

// ---------------------------------------------------------------------------
// PageBreakBlock — a hard page boundary
// ---------------------------------------------------------------------------

/// A hard page break.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageBreakBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

// ---------------------------------------------------------------------------
// SectionBreakBlock — a section break with optional page layout change
// ---------------------------------------------------------------------------

/// Kind of section break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionBreakKind {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
}

/// A section break that may introduce a new page layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionBreakBlock {
    pub kind: SectionBreakKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_layout: Option<PageLayout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

// ---------------------------------------------------------------------------
// HeaderFooterBlock — a header or footer section
// ---------------------------------------------------------------------------

/// Whether this is a header or footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeaderFooterKind {
    Header,
    Footer,
}

/// Which pages this header/footer applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeaderFooterScope {
    AllPages,
    FirstPage,
    OddPages,
    EvenPages,
}

/// A header or footer block with content and scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderFooterBlock {
    pub kind: HeaderFooterKind,
    pub content: Vec<Block>,
    pub applies_to: HeaderFooterScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

// ---------------------------------------------------------------------------
// BibliographyBlock — a bibliography/references section
// ---------------------------------------------------------------------------

/// A bibliography with entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibliographyBlock {
    pub entries: Vec<BibliographyEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A single bibliography entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibliographyEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// FormFieldBlock — an interactive form field
// ---------------------------------------------------------------------------

/// An interactive form field (Office/PDF).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldBlock {
    pub id: String,
    pub kind: FormFieldKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<Vec<Inline>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
}

/// The kind of form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormFieldKind {
    TextInput,
    Checkbox,
    Radio,
    Dropdown,
    Date,
    Signature,
    Button,
    Unknown,
}

// ---------------------------------------------------------------------------
// Revision — a tracked change in the document
// ---------------------------------------------------------------------------

/// A tracked revision (insertion, deletion, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub kind: RevisionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub content: Vec<Block>,
}

/// The kind of revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionKind {
    Inserted,
    Deleted,
    MovedFrom,
    MovedTo,
    FormatChanged,
}

// ---------------------------------------------------------------------------
// ChemicalFormulaBlock — a chemical structure diagram
// ---------------------------------------------------------------------------

/// A chemical formula or structure diagram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChemicalFormulaBlock {
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
}

// ---------------------------------------------------------------------------
// QrCodeBlock — a QR code or barcode
// ---------------------------------------------------------------------------

/// A QR code or barcode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeBlock {
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
}

// ---------------------------------------------------------------------------
// GraphBlock — a data-driven graph/plot
// ---------------------------------------------------------------------------

/// A data graph or plot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphBlock {
    #[serde(default)]
    pub data_points: Vec<DataPoint>,
    pub graph_type: GraphType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::LayerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<crate::AccessibilityInfo>,
}

/// A single data point in a graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// The type of graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
    Unknown,
}

// ---------------------------------------------------------------------------
// PageLayout / PageMargin / PageOrientation / ColumnLayout
// ---------------------------------------------------------------------------

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// Page margins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMargin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Default for PageMargin {
    fn default() -> Self {
        Self {
            top: 72.0,
            right: 72.0,
            bottom: 72.0,
            left: 72.0,
        }
    }
}

/// Column layout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLayout {
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
}

/// Full page layout descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub margin: PageMargin,
    pub orientation: PageOrientation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<ColumnLayout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_asset_id: Option<crate::AssetId>,
}

use serde::{Deserialize, Serialize};

use crate::{Inline, NodeId, Rect, SourceInfo};

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

/// A table cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    /// Inline content of the cell.
    pub inlines: Vec<Inline>,
    /// Number of columns this cell spans.
    pub colspan: u32,
    /// Number of rows this cell spans.
    pub rowspan: u32,
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
    /// Image data (base64 encoded or file path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>,
    /// Optional caption text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
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

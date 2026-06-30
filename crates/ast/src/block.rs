use serde::{Deserialize, Serialize};

use crate::{Inline, Rect, SourceInfo};

/// A layout block in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Block {
    /// A paragraph of inline content.
    Paragraph(ParagraphBlock),
    /// A standalone formula (display math).
    Formula(FormulaBlock),
    /// A table.
    Table(TableBlock),
    /// An image/figure.
    Figure(FigureBlock),
}

/// A paragraph containing inline elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParagraphBlock {
    pub inlines: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A standalone formula block (display math).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaBlock {
    pub formula: crate::Formula,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A table block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableBlock {
    pub rows: Vec<Vec<TableCell>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A table cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    pub inlines: Vec<Inline>,
    pub colspan: u32,
    pub rowspan: u32,
}

/// An image/figure block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureBlock {
    pub image_data: Option<String>, // base64 or path
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

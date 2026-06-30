use serde::{Deserialize, Serialize};

use crate::{Inline, Rect, SourceInfo, NodeId};

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

impl Block {
    /// Get the source info for this block.
    pub fn source(&self) -> Option<&SourceInfo> {
        match self {
            Block::Paragraph(p) => p.source.as_ref(),
            Block::Formula(f) => f.source.as_ref(),
            Block::Table(t) => t.source.as_ref(),
            Block::Figure(f) => f.source.as_ref(),
        }
    }

    /// Get the node ID for this block.
    pub fn node_id(&self) -> Option<NodeId> {
        self.source().and_then(|s| s.node_id)
    }

    /// Get mutable source info for this block.
    pub fn source_mut(&mut self) -> Option<&mut SourceInfo> {
        match self {
            Block::Paragraph(p) => p.source.as_mut(),
            Block::Formula(f) => f.source.as_mut(),
            Block::Table(t) => t.source.as_mut(),
            Block::Figure(f) => f.source.as_mut(),
        }
    }

    /// Get geometry for this block.
    pub fn geometry(&self) -> Option<&Rect> {
        match self {
            Block::Paragraph(p) => p.geometry.as_ref(),
            Block::Formula(f) => f.geometry.as_ref(),
            Block::Table(t) => t.geometry.as_ref(),
            Block::Figure(f) => f.geometry.as_ref(),
        }
    }

    /// Iterate over child inline elements.
    pub fn inlines(&self) -> Vec<&Inline> {
        match self {
            Block::Paragraph(p) => p.inlines.iter().collect(),
            Block::Formula(_) => vec![],
            Block::Table(t) => t.rows.iter().flat_map(|row| row.iter()).flat_map(|cell| cell.inlines.iter()).collect(),
            Block::Figure(_) => vec![],
        }
    }

    /// Iterate over child blocks (tables have nested cells with inlines).
    pub fn child_blocks(&self) -> Vec<&Block> {
        vec![]
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
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

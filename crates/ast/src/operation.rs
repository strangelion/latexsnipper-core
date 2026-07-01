use serde::{Deserialize, Serialize};

use crate::{Block, Formula};

/// An operation that can be applied to a Document.
/// Supports undo/redo and collaborative editing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    /// Insert a block at a position.
    InsertBlock {
        page: usize,
        index: usize,
        block: Block,
    },
    /// Remove a block.
    RemoveBlock { page: usize, index: usize },
    /// Replace a formula's content.
    ReplaceFormula {
        page: usize,
        index: usize,
        formula: Formula,
    },
    /// Replace text in a paragraph.
    ReplaceText {
        page: usize,
        block_index: usize,
        inline_index: usize,
        text: String,
    },
}

impl Operation {
    /// Create the inverse operation for undo.
    pub fn inverse(&self) -> Option<Operation> {
        match self {
            Operation::InsertBlock { page, index, .. } => Some(Operation::RemoveBlock {
                page: *page,
                index: *index,
            }),
            Operation::RemoveBlock { .. } => None, // Need original block to undo
            Operation::ReplaceFormula { .. } => None, // Need original formula to undo
            Operation::ReplaceText { .. } => None, // Need original text to undo
        }
    }
}

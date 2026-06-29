use serde::{Deserialize, Serialize};

use crate::{Block, Metadata};

/// Top-level document — the single source of truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub metadata: Metadata,
    pub pages: Vec<Page>,
}

/// A page in the document (PDF page, single image, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub blocks: Vec<Block>,
    pub page_number: Option<u32>,
}

impl Document {
    pub fn new() -> Self {
        Self {
            metadata: Metadata::default(),
            pages: Vec::new(),
        }
    }

    /// Total number of blocks across all pages.
    pub fn block_count(&self) -> usize {
        self.pages.iter().map(|p| p.blocks.len()).sum()
    }

    /// Flatten all blocks from all pages.
    pub fn all_blocks(&self) -> Vec<&Block> {
        self.pages.iter().flat_map(|p| &p.blocks).collect()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

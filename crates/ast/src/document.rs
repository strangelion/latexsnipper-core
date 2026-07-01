use serde::{Deserialize, Serialize};

use crate::{Block, Metadata, NodeIdGenerator};

/// Top-level document — the single source of truth.
#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub metadata: Metadata,
    pub pages: Vec<Page>,
    #[serde(skip)]
    #[serde(default = "NodeIdGenerator::new")]
    pub id_gen: NodeIdGenerator,
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            pages: self.pages.clone(),
            id_gen: NodeIdGenerator::new(),
        }
    }
}

/// A page in the document (PDF page, single image, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub blocks: Vec<Block>,
    pub page_number: Option<u32>,
}

impl Page {
    /// Iterate over all blocks in this page.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Get the number of blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get a block by index.
    pub fn get_block(&self, index: usize) -> Option<&Block> {
        self.blocks.get(index)
    }
}

impl Document {
    pub fn new() -> Self {
        Self {
            metadata: Metadata::default(),
            pages: Vec::new(),
            id_gen: NodeIdGenerator::new(),
        }
    }

    /// Generate the next unique NodeId.
    pub fn next_node_id(&mut self) -> crate::NodeId {
        self.id_gen.generate()
    }

    /// Total number of blocks across all pages.
    pub fn block_count(&self) -> usize {
        self.pages.iter().map(|p| p.blocks.len()).sum()
    }

    /// Flatten all blocks from all pages.
    pub fn all_blocks(&self) -> Vec<&Block> {
        self.pages.iter().flat_map(|p| &p.blocks).collect()
    }

    /// Get a page by index.
    pub fn get_page(&self, index: usize) -> Option<&Page> {
        self.pages.get(index)
    }

    /// Get a mutable page by index.
    pub fn get_page_mut(&mut self, index: usize) -> Option<&mut Page> {
        self.pages.get_mut(index)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

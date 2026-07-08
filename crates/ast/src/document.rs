use serde::{Deserialize, Serialize};

use crate::{Block, Diagnostic, MediaAsset, Metadata, NodeIdGenerator};

/// Top-level document — the single source of truth.
///
/// TODO(phase1): add `normalize_assets()` to compute checksums, fill mime/role/size
/// TODO(phase4): integrate with unified Importer/Exporter trait dispatch
#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub metadata: Metadata,
    pub pages: Vec<Page>,

    #[serde(default)]
    pub assets: Vec<MediaAsset>,

    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,

    #[serde(skip)]
    #[serde(default = "NodeIdGenerator::new")]
    pub id_gen: NodeIdGenerator,

    /// Schema version for compatibility tracking.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
}

fn default_schema_version() -> String {
    "1.0.0".to_string()
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            pages: self.pages.clone(),
            assets: self.assets.clone(),
            diagnostics: self.diagnostics.clone(),
            id_gen: NodeIdGenerator::new(),
            schema_version: self.schema_version.clone(),
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
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: NodeIdGenerator::new(),
            schema_version: default_schema_version(),
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

    /// Filter pages by 0-based indices, returning a new Document.
    pub fn filter_pages(&self, indices: &[usize]) -> Self {
        let pages: Vec<Page> = indices
            .iter()
            .filter_map(|&i| self.pages.get(i).cloned())
            .collect();
        Self {
            metadata: self.metadata.clone(),
            pages,
            assets: self.assets.clone(),
            diagnostics: self.diagnostics.clone(),
            id_gen: NodeIdGenerator::new(),
            schema_version: self.schema_version.clone(),
        }
    }

    /// Filter pages by 1-based page numbers, returning a new Document.
    pub fn filter_page_numbers(&self, numbers: &[u32]) -> Self {
        let pages: Vec<Page> = self
            .pages
            .iter()
            .filter(|p| p.page_number.map(|n| numbers.contains(&n)).unwrap_or(false))
            .cloned()
            .collect();
        Self {
            metadata: self.metadata.clone(),
            pages,
            assets: self.assets.clone(),
            diagnostics: self.diagnostics.clone(),
            id_gen: NodeIdGenerator::new(),
            schema_version: self.schema_version.clone(),
        }
    }

    /// Parse a page range string like "1-3,5,8-10" into sorted 1-based page numbers.
    pub fn parse_page_range(range: &str) -> Vec<u32> {
        let mut result = Vec::new();
        for part in range.split(',') {
            let part = part.trim();
            if let Some((start_str, end_str)) = part.split_once('-') {
                if let (Ok(start), Ok(end)) = (
                    start_str.trim().parse::<u32>(),
                    end_str.trim().parse::<u32>(),
                ) {
                    for n in start..=end {
                        result.push(n);
                    }
                }
            } else if let Ok(n) = part.parse::<u32>() {
                result.push(n);
            }
        }
        result.sort();
        result.dedup();
        result
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

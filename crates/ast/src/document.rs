use serde::{Deserialize, Serialize};

use crate::{
    AssetId, Block, Diagnostic, DocumentOutline, Inline, MediaAsset, Metadata, NodeIdGenerator,
    NoteDefinition,
};

/// Top-level document — the single source of truth.
///
/// Provides asset management methods for working with media assets
/// referenced by blocks and inlines throughout the document.
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

    /// Footnotes and endnotes referenced by the document body.
    #[serde(default)]
    pub notes: Vec<NoteDefinition>,

    /// Document outline / table of contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<DocumentOutline>,
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
            notes: self.notes.clone(),
            outline: self.outline.clone(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::block::PageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_asset_id: Option<crate::AssetId>,
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
            notes: Vec::new(),
            outline: None,
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
            notes: self.notes.clone(),
            outline: self.outline.clone(),
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
            notes: self.notes.clone(),
            outline: self.outline.clone(),
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

    /// Add a media asset to the document's asset list, returning its ID.
    pub fn add_asset(&mut self, asset: MediaAsset) -> AssetId {
        let id = asset.id.clone();
        self.assets.push(asset);
        id
    }

    /// Look up a media asset by its ID.
    pub fn get_asset(&self, id: &AssetId) -> Option<&MediaAsset> {
        self.assets.iter().find(|a| a.id == *id)
    }

    /// Validate that every `asset_id` reference in blocks and inlines
    /// points to an existing entry in `self.assets`.
    ///
    /// Returns diagnostics for missing asset references.
    pub fn validate_asset_refs(&self) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let asset_ids: Vec<&AssetId> = self.assets.iter().map(|a| &a.id).collect();
        for page in &self.pages {
            for block in &page.blocks {
                Self::check_block_asset_refs(block, &asset_ids, &mut diags);
            }
        }
        diags
    }

    /// Walk all FigureBlock and ImageInline asset references in the document
    /// checking they exist in the assets list.
    fn check_block_asset_refs(block: &Block, asset_ids: &[&AssetId], diags: &mut Vec<Diagnostic>) {
        match block {
            Block::Figure(f) => {
                if let Some(ref aid) = f.asset_id {
                    if !asset_ids.contains(&aid) {
                        diags.push(
                            Diagnostic::warning(
                                "W_MISSING_ASSET_REF",
                                format!("FigureBlock references missing asset {}", aid.0),
                            )
                            .with_recoverable(true),
                        );
                    }
                }
            }
            Block::TextBox(tb) => {
                for child in &tb.content {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            Block::Quote(q) => {
                for child in &q.blocks {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            Block::Minipage(m) => {
                for child in &m.content {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            Block::Float(f) => {
                for child in &f.content {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            Block::Theorem(t) => {
                for child in &t.content {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            Block::Proof(p) => {
                for child in &p.content {
                    Self::check_block_asset_refs(child, asset_ids, diags);
                }
            }
            _ => {}
        }
        // Also check inlines within this block
        for inline in block.inlines() {
            if let Inline::Image(img) = inline {
                if let Some(ref aid) = img.asset_id {
                    if !asset_ids.contains(&aid) {
                        diags.push(
                            Diagnostic::warning(
                                "W_MISSING_ASSET_REF",
                                format!("ImageInline references missing asset {}", aid.0),
                            )
                            .with_recoverable(true),
                        );
                    }
                }
            }
        }
    }

    /// Migrate legacy `image_data` base64 strings to proper `MediaAsset` entries.
    ///
    /// Walks all blocks and inlines, and for every `FigureBlock` or `ImageInline` that
    /// has `image_data` but no `asset_id`, creates a new `MediaAsset` and sets the `asset_id`.
    /// This is used during deserialization of old JSON documents that predate the asset system.
    pub fn migrate_legacy_image_data(&mut self) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut next_id = 0;

        for page in &mut self.pages {
            for block in &mut page.blocks {
                Self::migrate_block_image_data(block, &mut next_id, &mut self.assets, &mut diags);
            }
        }

        diags
    }

    fn migrate_block_image_data(
        block: &mut Block,
        next_id: &mut usize,
        assets: &mut Vec<MediaAsset>,
        diags: &mut Vec<Diagnostic>,
    ) {
        match block {
            Block::Figure(f) => {
                if f.asset_id.is_none() {
                    if let Some(ref data) = f.image_data {
                        let id = AssetId(format!("migrated-figure-{}", *next_id));
                        *next_id += 1;
                        f.asset_id = Some(id.clone());
                        let format = guess_format_from_base64(data);
                        assets.push(MediaAsset {
                            id,
                            format,
                            mime_type: None,
                            role: crate::MediaRole::Photo,
                            storage: crate::AssetStorage::InlineBase64 { data: data.clone() },
                            width: None,
                            height: None,
                            dpi: None,
                            color_space: None,
                            checksum_sha256: None,
                            alt_text: f.caption.clone(),
                            metadata: Default::default(),
                        });
                        diags.push(
                            Diagnostic::info(
                                "I_LEGACY_IMAGE_MIGRATED",
                                "FigureBlock image_data migrated to MediaAsset",
                            )
                            .with_recoverable(true),
                        );
                    }
                }
            }
            Block::TextBox(tb) => {
                for child in &mut tb.content {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            Block::Quote(q) => {
                for child in &mut q.blocks {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            Block::Minipage(m) => {
                for child in &mut m.content {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            Block::Float(f) => {
                for child in &mut f.content {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            Block::Theorem(t) => {
                for child in &mut t.content {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            Block::Proof(p) => {
                for child in &mut p.content {
                    Self::migrate_block_image_data(child, next_id, assets, diags);
                }
            }
            _ => {}
        }
        // Also migrate inlines
        if let Some(inlines) = block.inlines_mut() {
            for inline in inlines {
                if let Inline::Image(img) = inline {
                    if img.asset_id.is_none() {
                        if let Some(ref data) = img.image_data {
                            let id = AssetId(format!("migrated-image-{}", *next_id));
                            *next_id += 1;
                            img.asset_id = Some(id.clone());
                            let format = guess_format_from_base64(data);
                            assets.push(MediaAsset {
                                id,
                                format,
                                mime_type: None,
                                role: crate::MediaRole::Photo,
                                storage: crate::AssetStorage::InlineBase64 { data: data.clone() },
                                width: None,
                                height: None,
                                dpi: None,
                                color_space: None,
                                checksum_sha256: None,
                                alt_text: img.alt_text.clone(),
                                metadata: Default::default(),
                            });
                            diags.push(
                                Diagnostic::info(
                                    "I_LEGACY_IMAGE_MIGRATED",
                                    "ImageInline image_data migrated to MediaAsset",
                                )
                                .with_recoverable(true),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Guess the asset format from a base64-encoded data prefix.
fn guess_format_from_base64(data: &str) -> crate::AssetFormat {
    if data.starts_with("/9j") || data.starts_with("/9k") {
        crate::AssetFormat::Jpeg
    } else if data.starts_with("iVBOR") {
        crate::AssetFormat::Png
    } else if data.starts_with("R0lG") {
        crate::AssetFormat::Gif
    } else if data.starts_with("UklGR") {
        crate::AssetFormat::Webp
    } else if data.starts_with("PHN2Zy") || data.starts_with("PD94bW") {
        crate::AssetFormat::Svg
    } else {
        crate::AssetFormat::Unknown
    }
}

impl Diagnostic {
    fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: crate::DiagnosticLevel::Info,
            code: code.into(),
            message: message.into(),
            source: None,
            recoverable: false,
            data: serde_json::Value::Null,
        }
    }

    fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: crate::DiagnosticLevel::Warning,
            code: code.into(),
            message: message.into(),
            source: None,
            recoverable: false,
            data: serde_json::Value::Null,
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

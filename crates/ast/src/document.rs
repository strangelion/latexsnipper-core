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

/// Options to control the `Document::normalize_assets()` behavior.
#[derive(Debug, Clone)]
pub struct NormalizeAssetOptions {
    pub compute_checksum: bool,
    pub infer_mime_type: bool,
    pub deduplicate: bool,
    pub fill_dimensions: bool,
    pub migrate_legacy: bool,
}

impl Default for NormalizeAssetOptions {
    fn default() -> Self {
        Self {
            compute_checksum: false,
            infer_mime_type: true,
            deduplicate: true,
            fill_dimensions: false,
            migrate_legacy: true,
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
        let asset_ids: std::collections::HashSet<&AssetId> =
            self.assets.iter().map(|a| &a.id).collect();
        self.visit_asset_refs(|id| {
            if !asset_ids.contains(id) {
                diags.push(
                    Diagnostic::warning(
                        "W_MISSING_ASSET_REF",
                        format!("Asset '{}' referenced but not in Document.assets", id.0),
                    )
                    .with_recoverable(true),
                );
            }
        });
        diags
    }

    /// Rewrite all asset references in blocks and inlines using the given remapping.
    pub fn rewrite_asset_refs(&mut self, remap: &std::collections::HashMap<AssetId, AssetId>) {
        self.visit_asset_refs_mut(|id| {
            if let Some(new_id) = remap.get(id).cloned() {
                *id = new_id;
            }
        });
    }

    /// Visit every asset reference in the document (blocks, inlines, page, source info).
    /// Useful for validation, collection, and reporting.
    pub fn visit_asset_refs<F: FnMut(&AssetId)>(&self, mut f: F) {
        for page in &self.pages {
            if let Some(ref id) = page.background_asset_id {
                f(id);
            }
            if let Some(ref layout) = page.layout {
                if let Some(ref id) = layout.background_asset_id {
                    f(id);
                }
            }
            for block in &page.blocks {
                Self::visit_block_asset_refs(block, &mut f);
            }
        }
    }

    fn visit_block_asset_refs<F: FnMut(&AssetId)>(block: &Block, f: &mut F) {
        match block {
            Block::Figure(fig) => {
                if let Some(ref id) = fig.asset_id {
                    f(id);
                }
                if let Some(ref cap) = fig.caption_inlines {
                    for inline in cap {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            Block::Chart(c) => {
                if let Some(ref id) = c.asset_id {
                    f(id);
                }
                if let Some(ref title) = c.title {
                    for inline in title {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            Block::EmbeddedObject(eo) => {
                if let Some(ref id) = eo.asset_id {
                    f(id);
                }
                if let Some(ref id) = eo.preview_asset_id {
                    f(id);
                }
                if let Some(ref id) = eo.storage_ref {
                    f(id);
                }
            }
            Block::TextBox(tb) => {
                for child in &tb.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Quote(q) => {
                for child in &q.blocks {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Minipage(m) => {
                for child in &m.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Float(fl) => {
                for child in &fl.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Theorem(t) => {
                for child in &t.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Proof(p) => {
                for child in &p.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for child in &cell.content {
                            Self::visit_block_asset_refs(child, f);
                        }
                    }
                }
            }
            Block::List(l) => {
                for item in &l.items {
                    for child in &item.content {
                        Self::visit_block_asset_refs(child, f);
                    }
                }
            }
            Block::HeaderFooter(hf) => {
                for child in &hf.content {
                    Self::visit_block_asset_refs(child, f);
                }
            }
            Block::Shape(s) => {
                for inline in &s.text {
                    if let crate::Inline::Image(img) = inline {
                        if let Some(ref id) = img.asset_id {
                            f(id);
                        }
                    }
                }
            }
            Block::Annotation(a) => {
                for inline in &a.content {
                    if let crate::Inline::Image(img) = inline {
                        if let Some(ref id) = img.asset_id {
                            f(id);
                        }
                    }
                }
            }
            Block::FormField(ff) => {
                if let Some(ref label) = ff.label {
                    for inline in label {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        // Check inlines for ImageInline.asset_id
        for inline in block.inlines() {
            if let crate::Inline::Image(img) = inline {
                if let Some(ref id) = img.asset_id {
                    f(id);
                }
            }
        }
        // Check source info
        if let Some(source) = block.source() {
            if let Some(ref id) = source.asset_id {
                f(id);
            }
        }
    }

    /// Mutable version — visit every &mut AssetId for rewriting.
    pub fn visit_asset_refs_mut<F: FnMut(&mut AssetId)>(&mut self, mut f: F) {
        for page in &mut self.pages {
            if let Some(ref mut id) = page.background_asset_id {
                f(id);
            }
            if let Some(ref mut layout) = page.layout {
                if let Some(ref mut id) = layout.background_asset_id {
                    f(id);
                }
            }
            for block in &mut page.blocks {
                Self::visit_block_asset_refs_mut(block, &mut f);
            }
        }
    }

    fn visit_block_asset_refs_mut<F: FnMut(&mut AssetId)>(block: &mut Block, f: &mut F) {
        match block {
            Block::Figure(fig) => {
                if let Some(ref mut id) = fig.asset_id {
                    f(id);
                }
                if let Some(ref mut cap) = fig.caption_inlines {
                    for inline in cap.iter_mut() {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref mut id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            Block::Chart(c) => {
                if let Some(ref mut id) = c.asset_id {
                    f(id);
                }
                if let Some(ref mut title) = c.title {
                    for inline in title.iter_mut() {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref mut id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            Block::EmbeddedObject(eo) => {
                if let Some(ref mut id) = eo.asset_id {
                    f(id);
                }
                if let Some(ref mut id) = eo.preview_asset_id {
                    f(id);
                }
                if let Some(ref mut id) = eo.storage_ref {
                    f(id);
                }
            }
            Block::TextBox(tb) => {
                for child in &mut tb.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Quote(q) => {
                for child in &mut q.blocks {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Minipage(m) => {
                for child in &mut m.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Float(fl) => {
                for child in &mut fl.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Theorem(t) => {
                for child in &mut t.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Proof(p) => {
                for child in &mut p.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        for child in &mut cell.content {
                            Self::visit_block_asset_refs_mut(child, f);
                        }
                    }
                }
            }
            Block::List(l) => {
                for item in &mut l.items {
                    for child in &mut item.content {
                        Self::visit_block_asset_refs_mut(child, f);
                    }
                }
            }
            Block::HeaderFooter(hf) => {
                for child in &mut hf.content {
                    Self::visit_block_asset_refs_mut(child, f);
                }
            }
            Block::Shape(s) => {
                for inline in &mut s.text {
                    if let crate::Inline::Image(img) = inline {
                        if let Some(ref mut id) = img.asset_id {
                            f(id);
                        }
                    }
                }
            }
            Block::Annotation(a) => {
                for inline in &mut a.content {
                    if let crate::Inline::Image(img) = inline {
                        if let Some(ref mut id) = img.asset_id {
                            f(id);
                        }
                    }
                }
            }
            Block::FormField(ff) => {
                if let Some(ref mut label) = ff.label {
                    for inline in label.iter_mut() {
                        if let crate::Inline::Image(img) = inline {
                            if let Some(ref mut id) = img.asset_id {
                                f(id);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        // Check inlines for ImageInline.asset_id
        if let Some(mut inlines) = block.inlines_mut() {
            for inline in inlines.iter_mut() {
                if let crate::Inline::Image(img) = inline {
                    if let Some(ref mut id) = img.asset_id {
                        f(id);
                    }
                }
            }
        }
        // Check source info
        if let Some(source) = block.source_mut() {
            if let Some(ref mut id) = source.asset_id {
                f(id);
            }
        }
    }

    /// Collect all asset IDs referenced by blocks, pages, and inlines.
    pub fn collect_asset_refs(&self) -> Vec<AssetId> {
        let mut ids = Vec::new();
        self.visit_asset_refs(|id| ids.push(id.clone()));
        ids
    }

    /// Walk all inlines, migrate old Inline::Footnote to Inline::NoteRef + Document.notes.
    /// Returns diagnostics for each migrated footnote.
    pub fn migrate_inline_footnotes_to_notes(&mut self) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut next_id = 0;
        for page in &mut self.pages {
            Self::migrate_page_footnotes(page, &mut next_id, &mut self.notes, &mut diags);
        }
        diags
    }

    fn migrate_page_footnotes(
        page: &mut crate::Page,
        next_id: &mut usize,
        notes: &mut Vec<NoteDefinition>,
        diags: &mut Vec<Diagnostic>,
    ) {
        for block in &mut page.blocks {
            Self::migrate_block_footnotes(block, next_id, notes, diags);
        }
    }

    fn migrate_block_footnotes(
        block: &mut Block,
        next_id: &mut usize,
        notes: &mut Vec<NoteDefinition>,
        diags: &mut Vec<Diagnostic>,
    ) {
        match block {
            Block::TextBox(tb) => {
                for child in &mut tb.content {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            Block::Quote(q) => {
                for child in &mut q.blocks {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            Block::Minipage(m) => {
                for child in &mut m.content {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            Block::Float(f) => {
                for child in &mut f.content {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            Block::Theorem(t) => {
                for child in &mut t.content {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            Block::Proof(p) => {
                for child in &mut p.content {
                    Self::migrate_block_footnotes(child, next_id, notes, diags);
                }
            }
            _ => {}
        }
        if let Some(mut inlines) = block.inlines_mut() {
            for inline in inlines.iter_mut() {
                // Phase 1: check for Footnote and clone content (borrows inline temporarily)
                let migration = match inline {
                    Inline::Footnote { content } => Some(content.clone()),
                    _ => None,
                };
                // Phase 2: perform replacement (borrow of inline from Phase 1 is dropped)
                if let Some(content) = migration {
                    let note_id = format!("migrated-fn-{}", *next_id);
                    *next_id += 1;
                    let note_content = if let Inline::Text(t) = content.as_ref() {
                        vec![Block::Paragraph(crate::ParagraphBlock {
                            inlines: vec![Inline::Text(t.clone())],
                            geometry: None,
                            source: None,
                            style: None,
                        })]
                    } else {
                        vec![Block::Paragraph(crate::ParagraphBlock {
                            inlines: vec![content.as_ref().clone()],
                            geometry: None,
                            source: None,
                            style: None,
                        })]
                    };
                    notes.push(NoteDefinition {
                        id: note_id.clone(),
                        kind: crate::inline::NoteKind::Footnote,
                        content: note_content,
                        source: None,
                    });
                    // Replace the old Footnote with a NoteRef inline
                    **inline = Inline::NoteRef(crate::inline::NoteRefInline {
                        note_id: note_id.clone(),
                        kind: crate::inline::NoteKind::Footnote,
                        source: None,
                    });
                    diags.push(
                        Diagnostic::info(
                            "I_FOOTNOTE_MIGRATED",
                            format!("Inline::Footnote migrated to NoteRef ({})", note_id),
                        )
                        .with_recoverable(true),
                    );
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
                            checksum: None,
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
                                checksum: None,
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

    /// Normalize all assets in the document according to the given options.
    ///
    /// 1. migrate legacy `image_data` → `MediaAsset` (when enabled)
    /// 2. infer missing format/mime_type from content (when enabled)
    /// 3. compute SHA256 checksums (when enabled)
    /// 4. deduplicate identical content (when enabled)
    /// 5. update AssetManifest
    pub fn normalize_assets(&mut self, options: NormalizeAssetOptions) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        // 1. Migrate legacy image_data
        if options.migrate_legacy {
            diags.extend(self.migrate_legacy_image_data());
        }

        // 2. Infer missing mime_type from format
        if options.infer_mime_type {
            for asset in &mut self.assets {
                if asset.mime_type.is_none() {
                    asset.mime_type = asset_format_to_mime(&asset.format);
                }
            }
        }

        // 3. Compute content hashes (for dedup — not cryptographic)
        if options.compute_checksum {
            for asset in &mut self.assets {
                if asset.checksum.is_none() {
                    if let Ok(bytes) = resolve_asset_bytes(asset) {
                        asset.checksum = Some(compute_sha256(&bytes));
                    }
                }
            }
        }

        // 4. Deduplicate with reference rewriting
        let mut remap: std::collections::HashMap<AssetId, AssetId> =
            std::collections::HashMap::new();
        if options.deduplicate && self.assets.len() > 1 {
            let mut keep: Vec<MediaAsset> = Vec::new();
            let mut dedup_map: std::collections::HashMap<String, AssetId> =
                std::collections::HashMap::new();
            for asset in self.assets.drain(..) {
                let key = asset.checksum.clone().unwrap_or_else(|| asset.id.0.clone());
                if let Some(existing) = dedup_map.get(&key) {
                    remap.insert(asset.id.clone(), existing.clone());
                    diags.push(
                        Diagnostic::warning(
                            "W_ASSET_DEDUP",
                            format!("Asset '{}' deduplicated to '{}'", asset.id.0, existing.0),
                        )
                        .with_recoverable(true),
                    );
                } else {
                    dedup_map.insert(key, asset.id.clone());
                    keep.push(asset);
                }
            }
            self.assets = keep;
        }

        // 4b. Rewrite references after dedup
        if !remap.is_empty() {
            self.rewrite_asset_refs(&remap);
        }

        // 5. Validate refs
        diags.extend(self.validate_asset_refs());

        diags
    }
}

fn asset_format_to_mime(format: &crate::AssetFormat) -> Option<String> {
    match format {
        crate::AssetFormat::Png => Some("image/png".to_string()),
        crate::AssetFormat::Jpeg => Some("image/jpeg".to_string()),
        crate::AssetFormat::Gif => Some("image/gif".to_string()),
        crate::AssetFormat::Webp => Some("image/webp".to_string()),
        crate::AssetFormat::Bmp => Some("image/bmp".to_string()),
        crate::AssetFormat::Tiff => Some("image/tiff".to_string()),
        crate::AssetFormat::Svg => Some("image/svg+xml".to_string()),
        crate::AssetFormat::Pdf => Some("application/pdf".to_string()),
        _ => None,
    }
}

/// Minimal base64 decode — handles standard base64 without padding.
/// Used to decode InlineBase64 asset data for checksum computation.
fn simple_base64_decode(data: &str) -> Vec<u8> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = data.trim_end_matches('=');
    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in data.as_bytes() {
        if let Some(pos) = CHARS.iter().position(|&c| c == b) {
            buf = (buf << 6) | pos as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                result.push((buf >> bits) as u8);
                buf &= (1u32 << bits) - 1;
            }
        }
    }
    result
}

/// Resolve asset bytes for checksum computation.
///
/// For `InlineBase64`, this decodes the base64 string to get the actual image bytes.
fn resolve_asset_bytes(asset: &crate::MediaAsset) -> Result<Vec<u8>, String> {
    match &asset.storage {
        crate::AssetStorage::InlineBase64 { data } => Ok(simple_base64_decode(data)),
        _ => Err("Cannot resolve bytes from this storage type".to_string()),
    }
}

/// Compute SHA-256 hex digest using the `sha2` crate.
/// Provides cryptographically secure checksums for asset dedup and manifest integrity.
fn compute_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
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

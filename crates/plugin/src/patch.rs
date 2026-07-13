use latexsnipper_ast::{AssetId, Block, Diagnostic, Document, MediaAsset, Page};
use latexsnipper_foundation::{Result, SnipperError};

/// A read-only document view for patch-producing plugins.
#[derive(Debug, Clone, Copy)]
pub struct DocumentView<'a> {
    document: &'a Document,
}

impl<'a> DocumentView<'a> {
    pub const fn new(document: &'a Document) -> Self {
        Self { document }
    }

    pub const fn document(&self) -> &'a Document {
        self.document
    }

    pub fn page(&self, index: usize) -> Option<&'a Page> {
        self.document.pages.get(index)
    }

    pub fn block(&self, page: usize, block: usize) -> Option<&'a Block> {
        self.page(page).and_then(|value| value.blocks.get(block))
    }

    pub fn asset(&self, id: &AssetId) -> Option<&'a MediaAsset> {
        self.document.assets.iter().find(|asset| asset.id == *id)
    }
}

/// One bounded mutation in an atomic document transaction.
#[derive(Debug, Clone)]
pub enum PatchOperation {
    InsertPage {
        index: usize,
        page: Page,
    },
    ReplacePage {
        index: usize,
        page: Page,
    },
    RemovePage {
        index: usize,
    },
    InsertBlock {
        page: usize,
        index: usize,
        block: Block,
    },
    ReplaceBlock {
        page: usize,
        index: usize,
        block: Block,
    },
    RemoveBlock {
        page: usize,
        index: usize,
    },
    AddAsset(MediaAsset),
    RemoveAsset(AssetId),
    AddDiagnostic(Diagnostic),
}

/// An ordered set of document mutations applied atomically.
#[derive(Debug, Clone, Default)]
pub struct DocumentPatch {
    operations: Vec<PatchOperation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    pub pages_changed: usize,
    pub blocks_changed: usize,
    pub assets_changed: usize,
    pub diagnostics_added: usize,
}

impl DocumentPatch {
    pub const fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn push(mut self, operation: PatchOperation) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn operations(&self) -> &[PatchOperation] {
        &self.operations
    }

    /// Apply all operations or restore the original document on the first error.
    pub fn apply(&self, document: &mut Document) -> Result<ChangeSummary> {
        let original = document.clone();
        let mut summary = ChangeSummary::default();
        for operation in &self.operations {
            if let Err(error) = apply_operation(document, operation, &mut summary) {
                *document = original;
                return Err(error);
            }
        }
        Ok(summary)
    }
}

fn apply_operation(
    document: &mut Document,
    operation: &PatchOperation,
    summary: &mut ChangeSummary,
) -> Result<()> {
    match operation {
        PatchOperation::InsertPage { index, page } => {
            if *index > document.pages.len() {
                return Err(patch_error(format!(
                    "page insertion index {index} is out of bounds"
                )));
            }
            document.pages.insert(*index, page.clone());
            summary.pages_changed += 1;
        }
        PatchOperation::ReplacePage { index, page } => {
            let target = document
                .pages
                .get_mut(*index)
                .ok_or_else(|| patch_error(format!("page index {index} is out of bounds")))?;
            *target = page.clone();
            summary.pages_changed += 1;
        }
        PatchOperation::RemovePage { index } => {
            if *index >= document.pages.len() {
                return Err(patch_error(format!("page index {index} is out of bounds")));
            }
            document.pages.remove(*index);
            summary.pages_changed += 1;
        }
        PatchOperation::InsertBlock { page, index, block } => {
            let blocks = page_blocks_mut(document, *page)?;
            if *index > blocks.len() {
                return Err(patch_error(format!(
                    "block insertion index {index} is out of bounds"
                )));
            }
            blocks.insert(*index, block.clone());
            summary.blocks_changed += 1;
        }
        PatchOperation::ReplaceBlock { page, index, block } => {
            let target = page_blocks_mut(document, *page)?
                .get_mut(*index)
                .ok_or_else(|| patch_error(format!("block index {index} is out of bounds")))?;
            *target = block.clone();
            summary.blocks_changed += 1;
        }
        PatchOperation::RemoveBlock { page, index } => {
            let blocks = page_blocks_mut(document, *page)?;
            if *index >= blocks.len() {
                return Err(patch_error(format!("block index {index} is out of bounds")));
            }
            blocks.remove(*index);
            summary.blocks_changed += 1;
        }
        PatchOperation::AddAsset(asset) => {
            if document.assets.iter().any(|value| value.id == asset.id) {
                return Err(patch_error(format!(
                    "asset '{}' already exists",
                    asset.id.0
                )));
            }
            document.assets.push(asset.clone());
            summary.assets_changed += 1;
        }
        PatchOperation::RemoveAsset(id) => {
            let mut referenced = false;
            document.visit_asset_refs(|value| referenced |= value == id);
            if referenced {
                return Err(patch_error(format!("asset '{}' is still referenced", id.0)));
            }
            let index = document
                .assets
                .iter()
                .position(|value| value.id == *id)
                .ok_or_else(|| patch_error(format!("asset '{}' does not exist", id.0)))?;
            document.assets.remove(index);
            summary.assets_changed += 1;
        }
        PatchOperation::AddDiagnostic(diagnostic) => {
            document.diagnostics.push(diagnostic.clone());
            summary.diagnostics_added += 1;
        }
    }
    Ok(())
}

fn page_blocks_mut(document: &mut Document, page: usize) -> Result<&mut Vec<Block>> {
    document
        .pages
        .get_mut(page)
        .map(|value| &mut value.blocks)
        .ok_or_else(|| patch_error(format!("page index {page} is out of bounds")))
}

fn patch_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(format!("Document patch failed: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_failure_rolls_back_prior_operations() {
        let mut document = Document::new();
        document.pages.push(Page::new(100.0, 100.0, 1));
        let patch = DocumentPatch::new()
            .push(PatchOperation::InsertPage {
                index: 1,
                page: Page::new(200.0, 200.0, 2),
            })
            .push(PatchOperation::RemovePage { index: 10 });

        assert!(patch.apply(&mut document).is_err());
        assert_eq!(document.pages.len(), 1);
        assert_eq!(document.pages[0].width, 100.0);
    }
}

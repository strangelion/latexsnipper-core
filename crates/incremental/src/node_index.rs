use std::collections::HashMap;

use latexsnipper_ast::Document;

/// Internal location of a source-addressable AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodePath {
    pub page: usize,
    pub block: usize,
}

/// Lookup table from the public stable identity to an internal location.
#[derive(Debug, Clone, Default)]
pub struct NodeIndex {
    by_stable_id: HashMap<String, NodePath>,
}

impl NodeIndex {
    pub fn build(document: &Document) -> Self {
        let mut by_stable_id = HashMap::new();
        for (page_index, page) in document.pages.iter().enumerate() {
            for (block_index, block) in page.blocks.iter().enumerate() {
                if let Some(stable_id) = block.source().and_then(|source| source.stable_id.as_ref())
                {
                    by_stable_id.insert(
                        stable_id.clone(),
                        NodePath {
                            page: page_index,
                            block: block_index,
                        },
                    );
                }
            }
        }
        Self { by_stable_id }
    }

    pub fn get(&self, stable_id: &str) -> Option<NodePath> {
        self.by_stable_id.get(stable_id).copied()
    }

    pub fn len(&self) -> usize {
        self.by_stable_id.len()
    }
}

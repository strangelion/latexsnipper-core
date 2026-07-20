use std::collections::BTreeSet;

/// Conservative invalidation result after a session edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvalidationState {
    pub dirty_nodes: BTreeSet<String>,
    pub semantic_invalidated: BTreeSet<String>,
    pub render_invalidated: BTreeSet<String>,
    /// Runtime dependency outputs affected by the edit (for example page layout).
    pub dependent_outputs: BTreeSet<String>,
    pub full_reconcile_required: bool,
}

impl InvalidationState {
    pub fn formula_changed(&mut self, stable_id: impl Into<String>) {
        self.block_changed(stable_id);
    }

    pub fn block_changed(&mut self, stable_id: impl Into<String>) {
        let stable_id = stable_id.into();
        self.dirty_nodes.insert(stable_id.clone());
        self.semantic_invalidated.insert(stable_id.clone());
        self.render_invalidated.insert(stable_id);
    }
}

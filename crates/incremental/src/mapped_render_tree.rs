use std::collections::BTreeMap;

use latexsnipper_ast::ExportArtifact;

/// Maps source stable IDs to their latest fragment render outputs.
#[derive(Debug, Clone, Default)]
pub struct MappedRenderTree {
    fragments: BTreeMap<String, ExportArtifact>,
}

impl MappedRenderTree {
    pub fn insert(&mut self, stable_id: impl Into<String>, artifact: ExportArtifact) {
        self.fragments.insert(stable_id.into(), artifact);
    }

    pub fn get(&self, stable_id: &str) -> Option<&ExportArtifact> {
        self.fragments.get(stable_id)
    }

    pub fn remove(&mut self, stable_id: &str) {
        self.fragments.remove(stable_id);
    }

    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }
}

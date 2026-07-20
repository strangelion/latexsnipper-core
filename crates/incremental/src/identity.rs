use std::collections::BTreeMap;

use latexsnipper_ast::Block;

/// Runtime-only provenance for `SourceInfo.stable_id` values in a session.
/// It deliberately does not alter the Document 1.0.0 wire schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityOrigin {
    External,
    Session,
    ParserGenerated,
}

#[derive(Debug, Clone)]
pub struct IdentityRegistry {
    session_id: String,
    next_session_id: u64,
    origins: BTreeMap<String, IdentityOrigin>,
}

impl IdentityRegistry {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            next_session_id: 0,
            origins: BTreeMap::new(),
        }
    }

    pub fn allocate_session_id(&mut self) -> String {
        let id = format!("session:{}:node:{}", self.session_id, self.next_session_id);
        self.next_session_id += 1;
        self.origins.insert(id.clone(), IdentityOrigin::Session);
        id
    }

    pub fn register_external(&mut self, stable_id: impl Into<String>) {
        self.origins
            .insert(stable_id.into(), IdentityOrigin::External);
    }

    pub fn origin(&self, stable_id: &str) -> Option<IdentityOrigin> {
        self.origins.get(stable_id).copied()
    }

    pub fn remove(&mut self, stable_id: &str) {
        self.origins.remove(stable_id);
    }
}

pub fn set_block_stable_id(block: &mut Block, stable_id: String) {
    if let Some(source) = block.source_mut() {
        source.stable_id = Some(stable_id.clone());
    }
    if let Block::Formula(formula) = block {
        if let Some(source) = formula.formula.source_info.as_mut() {
            source.stable_id = Some(stable_id);
        }
    }
}

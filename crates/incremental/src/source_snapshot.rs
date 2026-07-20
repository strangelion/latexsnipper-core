/// Immutable source text used to produce a session revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub text: String,
}

impl SourceSnapshot {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

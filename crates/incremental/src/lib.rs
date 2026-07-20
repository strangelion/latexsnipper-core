//! Experimental formula-first incremental document sessions.
//!
//! This crate is additive and does not alter the `Document` 1.0.0 wire schema.

mod dependency;
mod edit;
mod error;
mod invalidation;
mod mapped_render_tree;
mod metrics;
mod node_index;
mod session;
mod source_snapshot;

pub use dependency::DependencyGraph;
pub use edit::SessionEdit;
pub use error::SessionError;
pub use invalidation::InvalidationState;
pub use latexsnipper_artifact::{
    ArtifactEdge, ArtifactEdgeKind, ArtifactGraph, ArtifactKind, ArtifactRecord, ArtifactTrace,
};
pub use latexsnipper_syntax::{ParsedDocument, SourceMap};
pub use mapped_render_tree::MappedRenderTree;
pub use metrics::SessionMetrics;
pub use node_index::{NodeIndex, NodePath};
pub use session::{DocumentSession, EditOutcome};
pub use source_snapshot::SourceSnapshot;

#[cfg(test)]
mod tests {
    use latexsnipper_ast::Span;
    use latexsnipper_conversion::OutputFormat;
    use latexsnipper_export::VisualFormat;

    use crate::{DocumentSession, SessionEdit};

    #[test]
    fn formula_edit_preserves_incremental_equivalence() {
        let mut session = DocumentSession::from_latex("test", "Before $x^2$ after.").unwrap();
        let stable_id = "latex:formula:1";
        assert_eq!(
            session.source_map().span_for(stable_id),
            Some(Span::new(7, 12))
        );

        let outcome = session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: 0,
                stable_id: stable_id.to_string(),
                latex: "z^{10}".to_string(),
            })
            .unwrap();

        assert_eq!(outcome.revision, 1);
        assert!(outcome.invalidation.dirty_nodes.contains(stable_id));
        assert!(!outcome.invalidation.full_reconcile_required);
        assert_eq!(session.source(), "Before $z^{10}$ after.");
        assert!(session.full_reconcile().unwrap());
    }

    #[test]
    fn formula_fragment_outputs_are_cached() {
        let mut session = DocumentSession::from_latex("test", "$\\frac{a}{b}$").unwrap();
        let stable_id = "latex:formula:0";

        let first = session
            .convert_formula(stable_id, OutputFormat::OMML)
            .unwrap();
        let second = session
            .convert_formula(stable_id, OutputFormat::OMML)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(session.metrics().semantic_cache_misses, 1);
        assert_eq!(session.metrics().semantic_cache_hits, 1);

        let render = session
            .render_formula(stable_id, VisualFormat::Svg)
            .unwrap();
        assert_eq!(render.format, "svg");
        assert_eq!(session.mapped_renders().len(), 1);
        assert!(session.mapped_renders().get(stable_id).is_some());
        let cached = session
            .render_formula(stable_id, VisualFormat::Svg)
            .unwrap();
        assert_eq!(cached.checksum_sha256, render.checksum_sha256);
        assert_eq!(session.metrics().render_cache_hits, 1);
    }

    #[test]
    fn structural_range_edit_falls_back_to_full_reconcile() {
        let mut session = DocumentSession::from_latex("test", "$x$").unwrap();
        let outcome = session
            .apply_edit(SessionEdit::ReplaceSourceRange {
                expected_revision: 0,
                span: Span::new(0, 0),
                replacement: "Text ".to_string(),
            })
            .unwrap();
        assert!(outcome.invalidation.full_reconcile_required);
        assert!(session.full_reconcile().unwrap());
    }

    #[test]
    fn paragraph_edit_preserves_incremental_equivalence() {
        let mut session = DocumentSession::from_latex("test", "Before $x$ after.").unwrap();
        let stable_id = "latex:paragraph:0";
        let outcome = session
            .apply_edit(SessionEdit::ReplaceParagraphSource {
                expected_revision: 0,
                stable_id: stable_id.to_string(),
                text: "Updated".to_string(),
            })
            .unwrap();
        assert!(outcome.invalidation.dirty_nodes.contains(stable_id));
        assert_eq!(session.source(), "Updated $x$ after.");
        assert!(session.full_reconcile().unwrap());
    }
}

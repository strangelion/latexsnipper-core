//! Experimental formula-first incremental document sessions.
//!
//! This crate is additive and does not alter the `Document` 1.0.0 wire schema.

mod cache;
mod dependency;
mod edit;
mod error;
mod identity;
mod invalidation;
mod mapped_render_tree;
mod metrics;
mod node_index;
mod session;
mod source_snapshot;

pub use cache::CacheLimits;
pub use dependency::DependencyGraph;
pub use edit::SessionEdit;
pub use error::SessionError;
pub use identity::IdentityOrigin;
pub use invalidation::InvalidationState;
pub use latexsnipper_artifact::{
    ArtifactEdge, ArtifactEdgeKind, ArtifactGraph, ArtifactKind, ArtifactRecord, ArtifactTrace,
};
pub use latexsnipper_syntax::{ParsedDocument, SourceMap};
pub use mapped_render_tree::MappedRenderTree;
pub use metrics::SessionMetrics;
pub use node_index::{NodeIndex, NodePath};
pub use session::{DocumentSession, EditOutcome, ReconcileOutcome};
pub use source_snapshot::SourceSnapshot;

#[cfg(test)]
mod tests {
    use latexsnipper_ast::Span;
    use latexsnipper_conversion::OutputFormat;
    use latexsnipper_export::VisualFormat;

    use crate::{CacheLimits, DocumentSession, IdentityOrigin, SessionEdit, SessionError};

    fn formula_id(session: &DocumentSession, latex: &str) -> String {
        session
            .document()
            .all_blocks()
            .into_iter()
            .find_map(|block| match block {
                latexsnipper_ast::Block::Formula(formula)
                    if formula.formula.as_latex() == latex =>
                {
                    block.source().and_then(|source| source.stable_id.clone())
                }
                _ => None,
            })
            .expect("formula must exist")
    }

    fn paragraph_id(session: &DocumentSession, text: &str) -> String {
        session
            .document()
            .all_blocks()
            .into_iter()
            .find_map(|block| match block {
                latexsnipper_ast::Block::Paragraph(paragraph)
                    if paragraph.inlines.iter().any(|inline| matches!(inline, latexsnipper_ast::Inline::Text(run) if run.text == text)) =>
                {
                    block.source().and_then(|source| source.stable_id.clone())
                }
                _ => None,
            })
            .expect("paragraph must exist")
    }

    #[test]
    fn formula_edit_preserves_incremental_equivalence() {
        let mut session = DocumentSession::from_latex("test", "Before $x^2$ after.").unwrap();
        let stable_id = formula_id(&session, "x^2");
        assert_eq!(
            session.source_map().span_for(&stable_id),
            Some(Span::new(7, 12))
        );

        let outcome = session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: 0,
                stable_id: stable_id.clone(),
                latex: "z^{10}".to_string(),
            })
            .unwrap();

        assert_eq!(outcome.revision, 1);
        assert!(outcome.invalidation.dirty_nodes.contains(&stable_id));
        assert!(outcome
            .invalidation
            .dependent_outputs
            .contains(&format!("semantic:{stable_id}")));
        assert!(outcome
            .invalidation
            .dependent_outputs
            .contains(&format!("render:{stable_id}")));
        assert!(!outcome.invalidation.full_reconcile_required);
        assert_eq!(session.source(), "Before $z^{10}$ after.");
        assert!(session.verify_full_equivalence().unwrap());
    }

    #[test]
    fn formula_fragment_outputs_are_cached() {
        let mut session = DocumentSession::from_latex("test", "$\\frac{a}{b}$").unwrap();
        let stable_id = formula_id(&session, "\\frac{a}{b}");

        let first = session
            .convert_formula(&stable_id, OutputFormat::OMML)
            .unwrap();
        let second = session
            .convert_formula(&stable_id, OutputFormat::OMML)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(session.metrics().semantic_cache_misses, 1);
        assert_eq!(session.metrics().semantic_cache_hits, 1);

        let render = session
            .render_formula(&stable_id, VisualFormat::Svg)
            .unwrap();
        assert_eq!(render.format, "svg");
        assert_eq!(session.mapped_renders().len(), 1);
        assert!(session.mapped_renders().get(&stable_id).is_some());
        let cached = session
            .render_formula(&stable_id, VisualFormat::Svg)
            .unwrap();
        assert_eq!(cached.checksum_sha256, render.checksum_sha256);
        assert_eq!(session.metrics().render_cache_hits, 1);
    }

    #[test]
    fn structural_range_edit_reconciles_and_preserves_later_id() {
        let mut session = DocumentSession::from_latex("test", "A $x$ B $y$").unwrap();
        let y_id = formula_id(&session, "y");
        let outcome = session
            .apply_edit(SessionEdit::ReplaceSourceRange {
                expected_revision: 0,
                span: Span::new(0, 0),
                replacement: "$z$ ".to_string(),
            })
            .unwrap();
        assert!(!outcome.invalidation.full_reconcile_required);
        assert_eq!(formula_id(&session, "y"), y_id);
        assert!(session.verify_full_equivalence().unwrap());
    }

    #[test]
    fn paragraph_edit_preserves_incremental_equivalence() {
        let mut session = DocumentSession::from_latex("test", "Before $x$ after.").unwrap();
        let stable_id = paragraph_id(&session, "Before");
        let outcome = session
            .apply_edit(SessionEdit::ReplaceParagraphSource {
                expected_revision: 0,
                stable_id: stable_id.clone(),
                text: "Updated".to_string(),
            })
            .unwrap();
        assert!(outcome.invalidation.dirty_nodes.contains(&stable_id));
        assert!(outcome
            .invalidation
            .dependent_outputs
            .contains("page-layout:0"));
        assert_eq!(session.source(), "Updated $x$ after.");
        assert!(session.verify_full_equivalence().unwrap());
    }

    #[test]
    fn external_identity_binding_survives_reconcile() {
        let mut session =
            DocumentSession::from_formula_with_stable_id("office", "x", "office:formula:42")
                .unwrap();
        assert_eq!(
            session.identity_origin("office:formula:42"),
            Some(IdentityOrigin::External)
        );
        let outcome = session.reconcile_full().unwrap();
        assert!(outcome
            .preserved_stable_ids
            .contains(&"office:formula:42".to_string()));
    }

    #[test]
    fn stale_revision_is_atomic() {
        let mut session = DocumentSession::from_latex("test", "$x$").unwrap();
        let stable_id = formula_id(&session, "x");
        let document = serde_json::to_value(session.document()).unwrap();
        let source = session.source().to_string();
        let error = session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: 1,
                stable_id,
                latex: "y".to_string(),
            })
            .unwrap_err();
        assert!(matches!(error, SessionError::RevisionConflict { .. }));
        assert_eq!(session.source(), source);
        assert_eq!(serde_json::to_value(session.document()).unwrap(), document);
    }

    #[test]
    fn bounded_caches_evict_under_configured_limits() {
        let mut session = DocumentSession::from_latex_with_cache_limits(
            "cache",
            "$x$ $y$",
            CacheLimits {
                max_entries: 1,
                max_bytes: 1024 * 1024,
            },
        )
        .unwrap();
        let x = formula_id(&session, "x");
        let y = formula_id(&session, "y");
        session.convert_formula(&x, OutputFormat::OMML).unwrap();
        session.convert_formula(&y, OutputFormat::OMML).unwrap();
        assert_eq!(session.metrics().semantic_cache_evictions, 1);
        assert!(session.metrics().semantic_cache_bytes > 0);
    }

    #[test]
    fn mixed_edits_remain_semantically_equivalent_and_utf8_safe() {
        let mut session = DocumentSession::from_latex("mixed", "中文 $x^2$ 后文 $y$").unwrap();
        let x = formula_id(&session, "x^2");
        let paragraph = paragraph_id(&session, "中文");
        session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: 0,
                stable_id: x,
                latex: "z^3".to_string(),
            })
            .unwrap();
        session
            .apply_edit(SessionEdit::ReplaceParagraphSource {
                expected_revision: 1,
                stable_id: paragraph,
                text: "前言".to_string(),
            })
            .unwrap();
        session
            .apply_edit(SessionEdit::ReplaceSourceRange {
                expected_revision: 2,
                span: Span::new(0, 0),
                replacement: "$w$ ".to_string(),
            })
            .unwrap();
        assert!(session.verify_full_equivalence().unwrap());
        for block in session.document().all_blocks() {
            let span = block.source().and_then(|source| source.span).unwrap();
            assert!(session.source().is_char_boundary(span.start));
            assert!(session.source().is_char_boundary(span.end));
        }
    }
}

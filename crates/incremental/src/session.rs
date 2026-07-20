use std::collections::HashMap;

use latexsnipper_artifact::{
    ArtifactEdgeKind, ArtifactGraph, ArtifactId, ArtifactKind, ArtifactRecord, ArtifactTrace,
};
use latexsnipper_ast::{Block, Document, ExportArtifact, FormulaSource, Inline, Span, TextRun};
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_export::{ExportService, VisualFormat};
use latexsnipper_syntax::latex::parse_latex_with_source_map;
use sha2::{Digest, Sha256};

use crate::{
    DependencyGraph, InvalidationState, MappedRenderTree, NodeIndex, SessionEdit, SessionError,
    SessionMetrics, SourceMap, SourceSnapshot,
};

/// Result returned after a revision-changing edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    pub revision: u64,
    pub invalidation: InvalidationState,
}

/// Experimental, formula-first stateful document editor.
#[derive(Debug, Clone)]
pub struct DocumentSession {
    pub session_id: String,
    pub revision: u64,
    source: SourceSnapshot,
    document: Document,
    node_index: NodeIndex,
    source_map: SourceMap,
    artifact_graph: ArtifactGraph,
    semantic_cache: HashMap<String, String>,
    render_cache: HashMap<String, ExportArtifact>,
    mapped_renders: MappedRenderTree,
    dependencies: DependencyGraph,
    invalidation: InvalidationState,
    metrics: SessionMetrics,
}

impl DocumentSession {
    pub fn from_latex(
        session_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, SessionError> {
        let source = SourceSnapshot::new(source);
        let parsed = parse_latex_with_source_map(&source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        let mut session = Self {
            session_id: session_id.into(),
            revision: 0,
            node_index: NodeIndex::build(&parsed.document),
            source_map: parsed.source_map,
            source,
            document: parsed.document,
            artifact_graph: ArtifactGraph::default(),
            semantic_cache: HashMap::new(),
            render_cache: HashMap::new(),
            mapped_renders: MappedRenderTree::default(),
            dependencies: DependencyGraph::default(),
            invalidation: InvalidationState::default(),
            metrics: SessionMetrics::default(),
        };
        session.record_source_artifacts();
        Ok(session)
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn source(&self) -> &str {
        &self.source.text
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    pub fn node_index(&self) -> &NodeIndex {
        &self.node_index
    }

    pub fn invalidation(&self) -> &InvalidationState {
        &self.invalidation
    }

    pub fn metrics(&self) -> &SessionMetrics {
        &self.metrics
    }

    pub fn artifact_graph(&self) -> &ArtifactGraph {
        &self.artifact_graph
    }

    /// Deterministic runtime lineage for diagnostics and evaluation evidence.
    pub fn artifact_trace(&self) -> ArtifactTrace {
        self.artifact_graph.trace()
    }

    pub fn mapped_renders(&self) -> &MappedRenderTree {
        &self.mapped_renders
    }

    pub fn dependencies(&self) -> &DependencyGraph {
        &self.dependencies
    }

    pub fn apply_edit(&mut self, edit: SessionEdit) -> Result<EditOutcome, SessionError> {
        let expected_revision = edit.expected_revision();
        if expected_revision != self.revision {
            return Err(SessionError::RevisionConflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }

        let invalidation = match edit {
            SessionEdit::ReplaceFormulaSource {
                stable_id, latex, ..
            } => self.replace_formula_source(&stable_id, latex)?,
            SessionEdit::ReplaceParagraphSource {
                stable_id, text, ..
            } => self.replace_paragraph_source(&stable_id, text)?,
            SessionEdit::ReplaceSourceRange {
                span, replacement, ..
            } => self.replace_source_range(span, replacement)?,
        };
        self.revision += 1;
        self.record_source_artifacts();
        self.metrics.edits_applied += 1;
        self.invalidation = invalidation.clone();
        Ok(EditOutcome {
            revision: self.revision,
            invalidation,
        })
    }

    pub fn convert_formula(
        &mut self,
        stable_id: &str,
        format: OutputFormat,
    ) -> Result<String, SessionError> {
        let formula = self.formula(stable_id)?;
        let key = semantic_cache_key(stable_id, formula.as_latex(), format);
        if let Some(value) = self.semantic_cache.get(&key) {
            self.metrics.semantic_cache_hits += 1;
            return Ok(value.clone());
        }
        let output = DocumentConverter::convert_formula(formula, format)
            .map_err(|error| SessionError::Conversion(error.to_string()))?;
        self.metrics.semantic_cache_misses += 1;
        self.metrics.converted_nodes += 1;
        self.semantic_cache.insert(key, output.clone());
        self.record_artifact(
            stable_id,
            ArtifactKind::SemanticFragment,
            &output,
            ArtifactEdgeKind::ConvertedFrom,
        );
        Ok(output)
    }

    pub fn render_formula(
        &mut self,
        stable_id: &str,
        format: VisualFormat,
    ) -> Result<ExportArtifact, SessionError> {
        let formula = self.formula(stable_id)?;
        let key = render_cache_key(stable_id, formula.as_latex(), format);
        if let Some(value) = self.render_cache.get(&key) {
            self.metrics.render_cache_hits += 1;
            return Ok(value.clone());
        }
        let output = ExportService::export_formula(formula, format)
            .map_err(|error| SessionError::Render(error.to_string()))?;
        self.metrics.render_cache_misses += 1;
        self.metrics.rendered_nodes += 1;
        self.render_cache.insert(key, output.clone());
        self.mapped_renders.insert(stable_id, output.clone());
        let checksum = output
            .checksum_sha256
            .as_deref()
            .unwrap_or_default()
            .to_string();
        self.record_artifact_with_checksum(
            stable_id,
            ArtifactKind::RenderFragment,
            checksum,
            ArtifactEdgeKind::RenderedFrom,
        );
        Ok(output)
    }

    /// Reparse the complete source and compare it with the current canonical
    /// document. This is the correctness oracle for incremental sessions.
    pub fn full_reconcile(&self) -> Result<bool, SessionError> {
        let parsed = parse_latex_with_source_map(&self.source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        let current = serde_json::to_value(&self.document)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        let rebuilt = serde_json::to_value(parsed.document)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        Ok(current == rebuilt)
    }

    fn replace_formula_source(
        &mut self,
        stable_id: &str,
        latex: String,
    ) -> Result<InvalidationState, SessionError> {
        let replacement_len = latex.len();
        let path = self
            .node_index
            .get(stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        let source_span = self
            .source_map
            .span_for(stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        let source_fragment = self
            .source
            .text
            .get(source_span.start..source_span.end)
            .ok_or(SessionError::InvalidRange)?;
        let delimiter_len = if source_fragment.starts_with("$$") && source_fragment.ends_with("$$")
        {
            2
        } else if source_fragment.starts_with('$') && source_fragment.ends_with('$') {
            1
        } else {
            return Err(SessionError::UnsupportedEdit);
        };
        let content_span = Span::new(
            source_span.start + delimiter_len,
            source_span.end - delimiter_len,
        );
        self.replace_text(content_span, &latex)?;

        let block = self.document.pages[path.page]
            .blocks
            .get_mut(path.block)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        let formula = match block {
            Block::Formula(formula) => formula,
            _ => return Err(SessionError::UnsupportedEdit),
        };
        formula.formula.source = FormulaSource::Latex(latex);
        formula.formula.layout = None;
        Self::adjust_document_spans(&mut self.document, content_span, replacement_len);
        self.metrics.reparsed_nodes += 1;
        self.remove_cached(stable_id);
        let mut invalidation = InvalidationState::default();
        invalidation.formula_changed(stable_id);
        Ok(invalidation)
    }

    fn replace_source_range(
        &mut self,
        span: Span,
        replacement: String,
    ) -> Result<InvalidationState, SessionError> {
        self.replace_text(span, &replacement)?;
        let parsed = parse_latex_with_source_map(&self.source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        self.metrics.reparsed_nodes += parsed.document.block_count() as u64;
        self.document = parsed.document;
        self.source_map = parsed.source_map;
        self.node_index = NodeIndex::build(&self.document);
        self.semantic_cache.clear();
        self.render_cache.clear();
        Ok(InvalidationState {
            full_reconcile_required: true,
            ..InvalidationState::default()
        })
    }

    fn replace_paragraph_source(
        &mut self,
        stable_id: &str,
        text: String,
    ) -> Result<InvalidationState, SessionError> {
        let replacement_len = text.len();
        let path = self
            .node_index
            .get(stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        let source_span = self
            .source_map
            .span_for(stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        self.replace_text(source_span, &text)?;

        let block = self.document.pages[path.page]
            .blocks
            .get_mut(path.block)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        let Block::Paragraph(paragraph) = block else {
            return Err(SessionError::UnsupportedEdit);
        };
        paragraph.inlines = vec![Inline::Text(TextRun::new(text))];
        Self::adjust_document_spans(&mut self.document, source_span, replacement_len);
        self.metrics.reparsed_nodes += 1;
        let mut invalidation = InvalidationState::default();
        invalidation.block_changed(stable_id);
        Ok(invalidation)
    }

    fn replace_text(&mut self, span: Span, replacement: &str) -> Result<(), SessionError> {
        if span.start > span.end
            || span.end > self.source.text.len()
            || !self.source.text.is_char_boundary(span.start)
            || !self.source.text.is_char_boundary(span.end)
        {
            return Err(SessionError::InvalidRange);
        }
        self.source
            .text
            .replace_range(span.start..span.end, replacement);
        self.source_map
            .apply_text_replacement(span, replacement.len());
        Ok(())
    }

    fn formula(&self, stable_id: &str) -> Result<&latexsnipper_ast::Formula, SessionError> {
        let path = self
            .node_index
            .get(stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(stable_id.to_string()))?;
        match self
            .document
            .pages
            .get(path.page)
            .and_then(|page| page.blocks.get(path.block))
        {
            Some(Block::Formula(formula)) => Ok(&formula.formula),
            Some(_) => Err(SessionError::UnsupportedEdit),
            None => Err(SessionError::UnknownStableId(stable_id.to_string())),
        }
    }

    fn remove_cached(&mut self, stable_id: &str) {
        self.semantic_cache
            .retain(|key, _| !key.starts_with(&format!("{stable_id}:")));
        self.render_cache
            .retain(|key, _| !key.starts_with(&format!("{stable_id}:")));
        self.mapped_renders.remove(stable_id);
    }

    fn adjust_document_spans(document: &mut Document, replaced: Span, replacement_len: usize) {
        let delta = replacement_len as isize - replaced.len() as isize;
        for page in &mut document.pages {
            for block in &mut page.blocks {
                if let Some(source) = block.source_mut() {
                    adjust_span(source.span.as_mut(), replaced, delta);
                }
                if let Block::Formula(formula) = block {
                    adjust_span(
                        formula
                            .formula
                            .source_info
                            .as_mut()
                            .and_then(|source| source.span.as_mut()),
                        replaced,
                        delta,
                    );
                }
            }
        }
    }

    fn record_source_artifacts(&mut self) {
        let stable_ids: Vec<String> = self
            .document
            .all_blocks()
            .into_iter()
            .filter(|block| matches!(block, Block::Formula(_)))
            .filter_map(|block| block.source().and_then(|source| source.stable_id.clone()))
            .collect();
        for stable_id in stable_ids {
            self.record_source_artifact(&stable_id);
        }
    }

    fn record_source_artifact(&mut self, stable_id: &str) {
        let content = self
            .formula(stable_id)
            .map(|formula| formula.as_latex().to_string())
            .unwrap_or_default();
        self.record_artifact(
            stable_id,
            ArtifactKind::SourceFormula,
            &content,
            ArtifactEdgeKind::DerivedFrom,
        );
    }

    fn record_artifact(
        &mut self,
        stable_id: &str,
        kind: ArtifactKind,
        content: &str,
        edge_kind: ArtifactEdgeKind,
    ) {
        self.record_artifact_with_checksum(
            stable_id,
            kind,
            checksum(content.as_bytes()),
            edge_kind,
        );
    }

    fn record_artifact_with_checksum(
        &mut self,
        stable_id: &str,
        kind: ArtifactKind,
        checksum: String,
        edge_kind: ArtifactEdgeKind,
    ) {
        let kind_label = match kind {
            ArtifactKind::SourceFormula => "source",
            ArtifactKind::SemanticFragment => "semantic",
            ArtifactKind::RenderFragment | ArtifactKind::RenderSvg | ArtifactKind::RenderPng => {
                "render"
            }
            _ => "artifact",
        };
        let id = format!("{kind_label}:{stable_id}:{}", self.revision);
        let prior = self
            .artifact_graph
            .records()
            .filter(|record| record.stable_id.as_deref() == Some(stable_id) && record.kind == kind)
            .map(|record| record.id.clone())
            .max();
        self.artifact_graph.insert(ArtifactRecord {
            id: ArtifactId::from(id.clone()),
            kind,
            stable_id: Some(stable_id.to_string()),
            content_ref: None,
            checksum: Some(checksum),
            provenance: Vec::new(),
        });
        if let Some(prior) = prior {
            self.artifact_graph
                .link(prior, id.clone(), ArtifactEdgeKind::ReplacedBy);
        }
        let source_id = format!("source:{stable_id}:{}", self.revision);
        if kind_label != "source" {
            self.artifact_graph.link(source_id, id, edge_kind);
        }
    }
}

fn semantic_cache_key(stable_id: &str, latex: &str, format: OutputFormat) -> String {
    format!(
        "{stable_id}:{}:{}",
        checksum(latex.as_bytes()),
        format.name()
    )
}

fn render_cache_key(stable_id: &str, latex: &str, format: VisualFormat) -> String {
    format!(
        "{stable_id}:{}:{}",
        checksum(latex.as_bytes()),
        format.extension()
    )
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn adjust_span(span: Option<&mut Span>, replaced: Span, delta: isize) {
    let Some(span) = span else { return };
    if span.end <= replaced.start {
        return;
    }
    if span.start >= replaced.end {
        span.start = offset(span.start, delta);
        span.end = offset(span.end, delta);
    } else {
        span.end = offset(span.end, delta);
    }
}

fn offset(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value.saturating_add(delta as usize)
    } else {
        value.saturating_sub(delta.unsigned_abs())
    }
}

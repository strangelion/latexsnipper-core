use std::collections::{BTreeMap, BTreeSet};

use latexsnipper_artifact::{
    ArtifactEdgeKind, ArtifactGraph, ArtifactId, ArtifactKind, ArtifactRecord, ArtifactTrace,
};
use latexsnipper_ast::{
    Block, Diagnostic, Document, ExportArtifact, FormulaSource, Inline, Span, TextRun,
};
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_export::{ExportService, VisualFormat};
use latexsnipper_syntax::latex::parse_latex_with_source_map;
use sha2::{Digest, Sha256};

use crate::{
    cache::BoundedCache,
    identity::{set_block_stable_id, IdentityRegistry},
    CacheLimits, DependencyGraph, IdentityOrigin, InvalidationState, MappedRenderTree, NodeIndex,
    SessionEdit, SessionError, SessionMetrics, SourceMap, SourceSnapshot,
};

/// Result returned after a revision-changing edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    pub revision: u64,
    pub invalidation: InvalidationState,
}

/// Result of rebuilding canonical session state from its complete source snapshot.
#[derive(Debug, Clone)]
pub struct ReconcileOutcome {
    pub changed: bool,
    pub previous_revision: u64,
    pub revision: u64,
    pub preserved_stable_ids: Vec<String>,
    pub replaced_stable_ids: Vec<String>,
    pub invalidated_stable_ids: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
struct NodeDescriptor {
    stable_id: String,
    kind: &'static str,
    content_hash: String,
    left_hash: Option<String>,
    right_hash: Option<String>,
    span: Span,
    page: usize,
    block: usize,
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
    identities: IdentityRegistry,
    artifact_graph: ArtifactGraph,
    latest_artifact_ids: BTreeMap<String, ArtifactId>,
    semantic_cache: BoundedCache<String>,
    render_cache: BoundedCache<ExportArtifact>,
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
        Self::from_latex_with_cache_limits(session_id, source, CacheLimits::default())
    }

    pub fn from_latex_with_cache_limits(
        session_id: impl Into<String>,
        source: impl Into<String>,
        cache_limits: CacheLimits,
    ) -> Result<Self, SessionError> {
        let session_id = session_id.into();
        let source = SourceSnapshot::new(source);
        let mut parsed = parse_latex_with_source_map(&source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        let mut identities = IdentityRegistry::new(session_id.clone());
        for page in &mut parsed.document.pages {
            for block in &mut page.blocks {
                // Parser IDs are deterministic provisional identities only.
                set_block_stable_id(block, identities.allocate_session_id());
            }
        }
        let source_map = SourceMap::from_document(&parsed.document);
        let dependencies = build_dependencies(&parsed.document);
        let mut session = Self {
            session_id,
            revision: 0,
            node_index: NodeIndex::build(&parsed.document),
            source_map,
            source,
            document: parsed.document,
            identities,
            artifact_graph: ArtifactGraph::default(),
            latest_artifact_ids: BTreeMap::new(),
            semantic_cache: BoundedCache::new(cache_limits),
            render_cache: BoundedCache::new(cache_limits),
            mapped_renders: MappedRenderTree::default(),
            dependencies,
            invalidation: InvalidationState::default(),
            metrics: SessionMetrics::default(),
        };
        session.record_source_artifacts();
        Ok(session)
    }

    /// Construct an inline LaTex formula session while preserving a host-owned ID.
    pub fn from_formula_with_stable_id(
        session_id: impl Into<String>,
        latex: impl AsRef<str>,
        stable_id: impl Into<String>,
    ) -> Result<Self, SessionError> {
        let mut session = Self::from_latex(session_id, format!("${}$", latex.as_ref()))?;
        let generated_id = session
            .document
            .pages
            .first()
            .and_then(|page| page.blocks.first())
            .and_then(|block| block.source())
            .and_then(|source| source.stable_id.clone())
            .ok_or(SessionError::MissingSource)?;
        session.bind_external_stable_id(&generated_id, stable_id)?;
        session.artifact_graph = ArtifactGraph::default();
        session.latest_artifact_ids.clear();
        session.record_source_artifacts();
        Ok(session)
    }

    /// Replace a session identity with a durable host identity, such as an Office formulaId.
    pub fn bind_external_stable_id(
        &mut self,
        current_stable_id: &str,
        external_stable_id: impl Into<String>,
    ) -> Result<(), SessionError> {
        let external_stable_id = external_stable_id.into();
        if external_stable_id.is_empty()
            || (external_stable_id != current_stable_id
                && self.node_index.contains(&external_stable_id))
        {
            return Err(SessionError::DuplicateStableId(external_stable_id));
        }
        let path = self
            .node_index
            .get(current_stable_id)
            .ok_or_else(|| SessionError::UnknownStableId(current_stable_id.to_string()))?;
        let block = self.document.pages[path.page]
            .blocks
            .get_mut(path.block)
            .ok_or_else(|| SessionError::UnknownStableId(current_stable_id.to_string()))?;
        set_block_stable_id(block, external_stable_id.clone());
        self.identities.remove(current_stable_id);
        self.identities.register_external(external_stable_id);
        self.source_map = SourceMap::from_document(&self.document);
        self.node_index = NodeIndex::build(&self.document);
        self.dependencies = build_dependencies(&self.document);
        let bound_id = self.document.pages[path.page].blocks[path.block]
            .source()
            .and_then(|source| source.stable_id.clone())
            .expect("bound block has stable id");
        self.record_source_artifact(&bound_id);
        Ok(())
    }

    pub fn identity_origin(&self, stable_id: &str) -> Option<IdentityOrigin> {
        self.identities.origin(stable_id)
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
        self.record_source_artifacts_for(&invalidation.dirty_nodes);
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
        let formula = self.formula(stable_id)?.clone();
        let key = semantic_cache_key(stable_id, formula.as_latex(), format);
        if let Some(value) = self.semantic_cache.get(&key).cloned() {
            self.metrics.semantic_cache_hits += 1;
            return Ok(value);
        }
        let output = DocumentConverter::convert_formula(&formula, format)
            .map_err(|error| SessionError::Conversion(error.to_string()))?;
        self.metrics.semantic_cache_misses += 1;
        self.metrics.converted_nodes += 1;
        self.metrics.semantic_cache_evictions += self.semantic_cache.insert(
            key.clone(),
            stable_id.to_string(),
            output.clone(),
            key.len() + output.len(),
        );
        self.metrics.semantic_cache_bytes = self.semantic_cache.bytes() as u64;
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
        let formula = self.formula(stable_id)?.clone();
        let key = render_cache_key(stable_id, formula.as_latex(), format);
        if let Some(value) = self.render_cache.get(&key).cloned() {
            self.metrics.render_cache_hits += 1;
            return Ok(value);
        }
        let output = ExportService::export_formula(&formula, format)
            .map_err(|error| SessionError::Render(error.to_string()))?;
        self.metrics.render_cache_misses += 1;
        self.metrics.rendered_nodes += 1;
        let bytes = key.len() + serde_json::to_vec(&output).map_or(0, |bytes| bytes.len());
        self.metrics.render_cache_evictions +=
            self.render_cache
                .insert(key, stable_id.to_string(), output.clone(), bytes);
        self.metrics.render_cache_bytes = self.render_cache.bytes() as u64;
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

    /// Compare current canonical state with a clean parse, ignoring runtime identity values.
    pub fn verify_full_equivalence(&self) -> Result<bool, SessionError> {
        let parsed = parse_latex_with_source_map(&self.source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        document_equivalent(&self.document, &parsed.document)
    }

    /// Deprecated compatibility alias for the former verification-only API.
    #[deprecated(note = "use verify_full_equivalence() or reconcile_full()")]
    pub fn full_reconcile(&self) -> Result<bool, SessionError> {
        self.verify_full_equivalence()
    }

    /// Rebuild canonical state from complete source while retaining matched identities and caches.
    pub fn reconcile_full(&mut self) -> Result<ReconcileOutcome, SessionError> {
        let previous_revision = self.revision;
        let mut outcome = self.reconcile_current_source(previous_revision)?;
        if outcome.changed {
            self.revision += 1;
            self.record_source_artifacts_for(&outcome.invalidated_stable_ids);
        }
        outcome.revision = self.revision;
        Ok(outcome)
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
        let Block::Formula(formula) = block else {
            return Err(SessionError::UnsupportedEdit);
        };
        formula.formula.source = FormulaSource::Latex(latex);
        formula.formula.layout = None;
        Self::adjust_document_spans(&mut self.document, content_span, replacement_len);
        self.metrics.reparsed_nodes += 1;
        Ok(self.invalidate_stable_ids([stable_id]))
    }

    fn replace_source_range(
        &mut self,
        span: Span,
        replacement: String,
    ) -> Result<InvalidationState, SessionError> {
        self.replace_text(span, &replacement)?;
        let outcome = self.reconcile_current_source(self.revision)?;
        self.metrics.reparsed_nodes += self.document.block_count() as u64;
        Ok(self.invalidate_stable_ids(outcome.invalidated_stable_ids))
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
        let paragraph = match self
            .document
            .pages
            .get(path.page)
            .and_then(|page| page.blocks.get(path.block))
        {
            Some(Block::Paragraph(paragraph)) => paragraph,
            _ => return Err(SessionError::UnsupportedEdit),
        };
        if !paragraph
            .inlines
            .iter()
            .all(|inline| matches!(inline, Inline::Text(_)))
        {
            return Err(SessionError::UnsupportedEdit);
        }
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
        Ok(self.invalidate_stable_ids([stable_id]))
    }

    fn reconcile_current_source(
        &mut self,
        revision: u64,
    ) -> Result<ReconcileOutcome, SessionError> {
        let old_document = self.document.clone();
        let old_nodes = describe_document(&self.document, &self.source_map);
        let old_dependencies = self.dependencies.clone();
        let mut parsed = parse_latex_with_source_map(&self.source.text)
            .map_err(|error| SessionError::Parse(error.to_string()))?;
        let new_nodes = describe_document(&parsed.document, &parsed.source_map);
        let (assignments, preserved, replaced) = match_node_identities(&old_nodes, &new_nodes);
        let mut matched_content = BTreeMap::new();
        for new_node in &new_nodes {
            let stable_id = assignments
                .get(&new_node.stable_id)
                .cloned()
                .unwrap_or_else(|| self.identities.allocate_session_id());
            if let Some(old_id) = preserved.iter().find(|old_id| **old_id == stable_id) {
                if let Some(old_node) = old_nodes.iter().find(|node| &node.stable_id == old_id) {
                    matched_content.insert(
                        stable_id.clone(),
                        old_node.content_hash == new_node.content_hash,
                    );
                }
            }
            let page = parsed
                .document
                .pages
                .get_mut(new_node.page)
                .expect("descriptor page exists");
            let block = page
                .blocks
                .get_mut(new_node.block)
                .expect("descriptor block exists");
            set_block_stable_id(block, stable_id);
        }
        let valid_ids: BTreeSet<String> = describe_document(
            &parsed.document,
            &SourceMap::from_document(&parsed.document),
        )
        .into_iter()
        .map(|node| node.stable_id)
        .collect();
        let mut invalidated: BTreeSet<String> = replaced.iter().cloned().collect();
        for (stable_id, content_unchanged) in matched_content {
            if !content_unchanged {
                invalidated.insert(stable_id);
            }
        }
        for stable_id in &valid_ids {
            if !preserved.contains(stable_id) {
                invalidated.insert(stable_id.clone());
            }
        }
        for stable_id in &invalidated {
            self.remove_cached(stable_id);
            self.invalidate_artifact_descendants(stable_id);
        }
        for stable_id in &replaced {
            self.identities.remove(stable_id);
        }
        self.semantic_cache
            .retain_stable_ids(|stable_id| valid_ids.contains(stable_id));
        self.render_cache
            .retain_stable_ids(|stable_id| valid_ids.contains(stable_id));
        self.metrics.semantic_cache_bytes = self.semantic_cache.bytes() as u64;
        self.metrics.render_cache_bytes = self.render_cache.bytes() as u64;
        self.metrics.reconcile_matched_nodes += preserved.len() as u64;
        self.metrics.reconcile_replaced_nodes += (new_nodes.len() - preserved.len()) as u64;
        self.document = parsed.document;
        self.source_map = SourceMap::from_document(&self.document);
        self.node_index = NodeIndex::build(&self.document);
        self.dependencies = build_dependencies(&self.document);
        let changed = !document_equivalent(&old_document, &self.document)?;
        // Old dependency descendants are accounted for even when a source node disappeared.
        let mut all_invalidated = invalidated;
        for stable_id in &replaced {
            all_invalidated.extend(
                old_dependencies
                    .invalidate(stable_id)
                    .into_iter()
                    .filter(|id| !id.contains(':')),
            );
        }
        Ok(ReconcileOutcome {
            changed,
            previous_revision: revision,
            revision,
            preserved_stable_ids: preserved.into_iter().collect(),
            replaced_stable_ids: replaced.into_iter().collect(),
            invalidated_stable_ids: all_invalidated.into_iter().collect(),
            diagnostics: Vec::new(),
        })
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

    fn invalidate_stable_ids(
        &mut self,
        stable_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> InvalidationState {
        let mut invalidation = InvalidationState::default();
        for stable_id in stable_ids {
            let stable_id = stable_id.as_ref();
            invalidation.dirty_nodes.insert(stable_id.to_string());
            let affected = self.dependencies.invalidate(stable_id);
            invalidation
                .dependent_outputs
                .extend(affected.iter().filter(|id| *id != stable_id).cloned());
            if affected.contains(&format!("semantic:{stable_id}")) {
                invalidation
                    .semantic_invalidated
                    .insert(stable_id.to_string());
            }
            if affected.contains(&format!("render:{stable_id}")) {
                invalidation
                    .render_invalidated
                    .insert(stable_id.to_string());
            }
            if !invalidation.semantic_invalidated.contains(stable_id) {
                invalidation
                    .semantic_invalidated
                    .insert(stable_id.to_string());
            }
            if !invalidation.render_invalidated.contains(stable_id) {
                invalidation
                    .render_invalidated
                    .insert(stable_id.to_string());
            }
            self.remove_cached(stable_id);
            self.invalidate_artifact_descendants(stable_id);
        }
        invalidation
    }

    fn remove_cached(&mut self, stable_id: &str) {
        self.semantic_cache.remove_stable_id(stable_id);
        self.render_cache.remove_stable_id(stable_id);
        self.metrics.semantic_cache_bytes = self.semantic_cache.bytes() as u64;
        self.metrics.render_cache_bytes = self.render_cache.bytes() as u64;
        self.mapped_renders.remove(stable_id);
    }

    fn invalidate_artifact_descendants(&self, stable_id: &str) {
        for record in self
            .artifact_graph
            .records()
            .filter(|record| record.stable_id.as_deref() == Some(stable_id))
        {
            let _ = self.artifact_graph.descendants_of(&record.id);
        }
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

    fn record_source_artifacts_for(
        &mut self,
        stable_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let formula_ids: BTreeSet<String> = self
            .document
            .all_blocks()
            .into_iter()
            .filter(|block| matches!(block, Block::Formula(_)))
            .filter_map(|block| block.source().and_then(|source| source.stable_id.clone()))
            .collect();
        for stable_id in stable_ids {
            let stable_id = stable_id.as_ref();
            if formula_ids.contains(stable_id) {
                self.record_source_artifact(stable_id);
            }
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
        let artifact_key = format!("{kind_label}:{stable_id}");
        let prior = self
            .latest_artifact_ids
            .insert(artifact_key, ArtifactId::from(id.clone()));
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
        if kind_label != "source" {
            self.artifact_graph.link(
                format!("source:{stable_id}:{}", self.revision),
                id,
                edge_kind,
            );
        }
    }
}

fn describe_document(document: &Document, source_map: &SourceMap) -> Vec<NodeDescriptor> {
    let mut nodes = Vec::new();
    for (page_index, page) in document.pages.iter().enumerate() {
        for (block_index, block) in page.blocks.iter().enumerate() {
            let Some(stable_id) = block.source().and_then(|source| source.stable_id.clone()) else {
                continue;
            };
            let Some(span) = source_map.span_for(&stable_id) else {
                continue;
            };
            let Some((kind, content)) = block_identity_content(block) else {
                continue;
            };
            nodes.push(NodeDescriptor {
                stable_id,
                kind,
                content_hash: checksum(content.as_bytes()),
                left_hash: None,
                right_hash: None,
                span,
                page: page_index,
                block: block_index,
            });
        }
    }
    let hashes: Vec<String> = nodes.iter().map(|node| node.content_hash.clone()).collect();
    for (index, node) in nodes.iter_mut().enumerate() {
        node.left_hash = index
            .checked_sub(1)
            .and_then(|index| hashes.get(index))
            .cloned();
        node.right_hash = hashes.get(index + 1).cloned();
    }
    nodes
}

fn block_identity_content(block: &Block) -> Option<(&'static str, String)> {
    match block {
        Block::Formula(formula) => Some(("formula", normalize(formula.formula.as_latex()))),
        Block::Paragraph(paragraph) => Some((
            "paragraph",
            normalize(
                &paragraph
                    .inlines
                    .iter()
                    .map(inline_content)
                    .collect::<String>(),
            ),
        )),
        _ => None,
    }
}

fn inline_content(inline: &Inline) -> String {
    match inline {
        Inline::Text(text) => text.text.clone(),
        Inline::Formula(formula) => formula.as_latex().to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect()
}

fn match_node_identities(
    old_nodes: &[NodeDescriptor],
    new_nodes: &[NodeDescriptor],
) -> (BTreeMap<String, String>, BTreeSet<String>, BTreeSet<String>) {
    let mut assignments = BTreeMap::new();
    let mut preserved = BTreeSet::new();
    let mut unused: BTreeSet<usize> = (0..old_nodes.len()).collect();
    let mut exact_content = BTreeMap::<(&'static str, String), Vec<usize>>::new();
    for (index, old) in old_nodes.iter().enumerate() {
        exact_content
            .entry((old.kind, old.content_hash.clone()))
            .or_default()
            .push(index);
    }
    for new_node in new_nodes {
        let mut candidates: Vec<usize> = exact_content
            .get(&(new_node.kind, new_node.content_hash.clone()))
            .into_iter()
            .flatten()
            .copied()
            .filter(|index| unused.contains(index))
            .collect();
        if candidates.is_empty() {
            candidates.extend(
                unused
                    .iter()
                    .copied()
                    .filter(|index| old_nodes[*index].kind == new_node.kind),
            );
        }
        let candidate = candidates
            .into_iter()
            .map(|index| (index, match_score(&old_nodes[index], new_node)))
            .max_by_key(|(_, score)| *score);
        let Some((index, score)) = candidate else {
            continue;
        };
        let old = &old_nodes[index];
        let overlaps = old.span.start < new_node.span.end && new_node.span.start < old.span.end;
        if old.content_hash != new_node.content_hash && !overlaps && score < 2_000 {
            continue;
        }
        assignments.insert(new_node.stable_id.clone(), old.stable_id.clone());
        preserved.insert(old.stable_id.clone());
        unused.remove(&index);
    }
    let replaced = unused
        .into_iter()
        .map(|index| old_nodes[index].stable_id.clone())
        .collect();
    (assignments, preserved, replaced)
}

fn match_score(old: &NodeDescriptor, new: &NodeDescriptor) -> i64 {
    let mut score = 0;
    if old.content_hash == new.content_hash {
        score += 10_000;
    }
    if old.left_hash == new.left_hash {
        score += 300;
    }
    if old.right_hash == new.right_hash {
        score += 300;
    }
    if old.page == new.page {
        score += 100;
    }
    let distance = old.span.start.abs_diff(new.span.start).min(2_000) as i64;
    score -= distance;
    if old.span.start < new.span.end && new.span.start < old.span.end {
        score += 3_000;
    }
    score
}

fn build_dependencies(document: &Document) -> DependencyGraph {
    let mut graph = DependencyGraph::default();
    for (page_index, page) in document.pages.iter().enumerate() {
        for block in &page.blocks {
            let Some(stable_id) = block
                .source()
                .and_then(|source| source.stable_id.as_deref())
            else {
                continue;
            };
            match block {
                Block::Formula(_) => {
                    graph.link(stable_id, format!("semantic:{stable_id}"));
                    graph.link(
                        format!("semantic:{stable_id}"),
                        format!("render:{stable_id}"),
                    );
                }
                Block::Paragraph(_) => {
                    graph.link(stable_id, format!("paragraph-output:{stable_id}"));
                    graph.link(stable_id, format!("page-layout:{page_index}"));
                }
                _ => {}
            }
        }
    }
    graph
}

fn document_equivalent(left: &Document, right: &Document) -> Result<bool, SessionError> {
    let mut left =
        serde_json::to_value(left).map_err(|error| SessionError::Parse(error.to_string()))?;
    let mut right =
        serde_json::to_value(right).map_err(|error| SessionError::Parse(error.to_string()))?;
    remove_runtime_identity(&mut left);
    remove_runtime_identity(&mut right);
    Ok(left == right)
}

fn remove_runtime_identity(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("stable_id");
            for value in object.values_mut() {
                remove_runtime_identity(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                remove_runtime_identity(value);
            }
        }
        _ => {}
    }
}

fn semantic_cache_key(stable_id: &str, latex: &str, format: OutputFormat) -> String {
    format!(
        "semantic:v1:options-default:{stable_id}:{}:{}",
        checksum(latex.as_bytes()),
        format.name()
    )
}

fn render_cache_key(stable_id: &str, latex: &str, format: VisualFormat) -> String {
    format!(
        "render:v1:options-default:{stable_id}:{}:{}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_fast_path_rejects_non_text_inline_structure() {
        let mut session = DocumentSession::from_latex("paragraph-safety", "Before").unwrap();
        let stable_id = session.document.pages[0].blocks[0]
            .source()
            .and_then(|source| source.stable_id.clone())
            .unwrap();
        {
            let Block::Paragraph(paragraph) = &mut session.document.pages[0].blocks[0] else {
                panic!("expected paragraph")
            };
            paragraph
                .inlines
                .push(Inline::Formula(latexsnipper_ast::Formula::latex("x")));
            assert_eq!(paragraph.inlines.len(), 2);
        }
        let source_before = session.source().to_string();
        let error = session
            .apply_edit(SessionEdit::ReplaceParagraphSource {
                expected_revision: 0,
                stable_id,
                text: "Updated".to_string(),
            })
            .unwrap_err();
        assert!(matches!(error, SessionError::UnsupportedEdit));
        assert_eq!(session.source(), source_before);
    }

    #[test]
    fn unchanged_formula_cache_survives_structural_reconcile() {
        let mut session = DocumentSession::from_latex("cache-retention", "$x$ $y$").unwrap();
        let y = session
            .document
            .all_blocks()
            .into_iter()
            .find_map(|block| match block {
                Block::Formula(formula) if formula.formula.as_latex() == "y" => {
                    block.source().and_then(|source| source.stable_id.clone())
                }
                _ => None,
            })
            .unwrap();
        session.convert_formula(&y, OutputFormat::OMML).unwrap();
        session
            .apply_edit(SessionEdit::ReplaceSourceRange {
                expected_revision: 0,
                span: Span::new(0, 0),
                replacement: "$z$ ".to_string(),
            })
            .unwrap();
        session.convert_formula(&y, OutputFormat::OMML).unwrap();
        assert_eq!(session.metrics.semantic_cache_hits, 1);
        session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: 1,
                stable_id: y.clone(),
                latex: "q".to_string(),
            })
            .unwrap();
        session.convert_formula(&y, OutputFormat::OMML).unwrap();
        assert_eq!(session.metrics.semantic_cache_misses, 2);
    }
}

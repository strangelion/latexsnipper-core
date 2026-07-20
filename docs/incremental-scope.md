# Incremental Engine Scope

`latexsnipper-incremental` currently performs source-aware local edits for
formula and pure-text paragraph blocks. The lightweight parser emits only
parser-local provisional IDs (`latex:<kind>:<index>`). `DocumentSession`
immediately replaces those with session IDs, or with an externally bound ID
(for example an Office formula ID). `SourceInfo.stable_id` remains the single
wire field; identity origin is runtime-only session metadata.

`reconcile_full()` reparses complete source, matches FormulaBlock and
ParagraphBlock nodes by normalized content, neighbors, page parent, and source
distance/overlap, then rebuilds SourceMap, NodeIndex, and DependencyGraph.
`verify_full_equivalence()` is the non-mutating correctness oracle; the legacy
`full_reconcile()` name remains as a deprecated alias. Structural source-range
edits call reconciliation immediately rather than leaving stale canonical state.

Semantic and render fragment caches are bounded approximate-LRU caches keyed by
stable ID, content hash, output format, and cache format version. Reconcile
preserves entries only for unchanged matched nodes and invalidates changed nodes
plus their semantic/render dependency descendants. ArtifactGraph remains
immutable provenance/lineage, while `latest_artifact_ids` identifies currently
valid artifact versions and `stale_artifact_ids` records invalidated derived
artifacts without deleting history. Derived artifact edges always originate from
the real latest source artifact ID. DependencyGraph is the document dependency
authority; removed nodes retain their old dependency outputs in the reconcile
outcome so that page-layout invalidation is not lost.

External ID binding is accepted only at revision zero before semantic or render
artifacts exist. This prevents a public identity rebinding operation from
silently orphaning cache, render-tree, or provenance sidecars.

`ReplaceParagraphSource` only accepts a ParagraphBlock containing TextRun
inlines. Paragraphs with formula, formatting, links, annotations, or any other
inline structure return `UnsupportedEdit` and require source-range reconcile.
Table cells, headings, lists, quotes, and nested blocks are not advertised as
local-edit targets until their parsers provide stable source spans at that
granularity.

This boundary prevents a false incremental claim: every supported local edit
must be equivalent to a clean rebuild, and every unsupported edit must report
the conservative fallback.

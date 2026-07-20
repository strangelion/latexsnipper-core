/// Session-local metrics that distinguish correct output from incremental work.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetrics {
    pub edits_applied: u64,
    pub reparsed_nodes: u64,
    pub converted_nodes: u64,
    pub rendered_nodes: u64,
    pub semantic_cache_hits: u64,
    pub semantic_cache_misses: u64,
    pub render_cache_hits: u64,
    pub render_cache_misses: u64,
    pub semantic_cache_evictions: u64,
    pub render_cache_evictions: u64,
    pub semantic_cache_bytes: u64,
    pub render_cache_bytes: u64,
    pub reconcile_matched_nodes: u64,
    pub reconcile_replaced_nodes: u64,
}

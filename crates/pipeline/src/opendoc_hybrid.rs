//! OpenDoc Hybrid pipeline mode.
//!
//! Provides the `DocumentParseMode` enum and the hybrid pipeline logic
//! that combines layout analysis, text OCR, formula recognition, and
//! table structure recognition into a single document understanding flow.
//!
//! Fallback chain:
//!   OpenDocHybrid → layout + specialized regions → SpecializedStable → diagnostics

use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::{layout_node::LayoutNode, region_resolve_node::RegionResolveNode};

/// Document parsing mode — controls which models and heuristics are used.
///
/// - `SpecializedStable`: default mode, uses dedicated detection + recognition
///   per content type (PP-OCR for text, TrOCR for formula, TATR for table).
///   No layout analysis. Most stable, best for Office export.
///
/// - `OpenOcrText`: replaces text detection/recognition with OpenOCR DBNet+CTC.
///   Formula and table remain unchanged. Optional layout analysis.
///
/// - `OpenDocHybrid`: runs layout analysis as a frontend (PP-DocLayout),
///   routes regions to specialized recognizers based on layout label.
///   Falls back to SpecializedStable if layout model is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentParseMode {
    #[default]
    SpecializedStable,
    OpenOcrText,
    OpenDocHybrid,
}

impl DocumentParseMode {
    pub const fn all() -> &'static [Self] {
        &[
            Self::SpecializedStable,
            Self::OpenOcrText,
            Self::OpenDocHybrid,
        ]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SpecializedStable => "specialized",
            Self::OpenOcrText => "openocr-text",
            Self::OpenDocHybrid => "opendoc-hybrid",
        }
    }

    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::SpecializedStable => &["specialized-stable", "stable", "default"],
            Self::OpenOcrText => &["openocr", "openocr_text", "open-ocr-text"],
            Self::OpenDocHybrid => &["opendoc", "opendoc_hybrid", "hybrid"],
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        let normalized = label.trim().to_ascii_lowercase();
        Self::all().iter().copied().find(|mode| {
            mode.label() == normalized || mode.aliases().contains(&normalized.as_str())
        })
    }

    /// Run the OpenDocHybrid pipeline: layout → resolve → specialized recognizers.
    ///
    /// This is called in place of the standard detector/crop/recognizer chain.
    /// Falls back to the specialized stable pipeline if layout fails.
    pub async fn run_hybrid(ctx: &mut PipelineContext) -> Result<bool> {
        // Step 1: Run layout analysis (optional — skip if no model)
        let layout = LayoutNode::new();
        if let Err(e) = layout.process(ctx).await {
            log::warn!(
                "OpenDocHybrid: layout analysis failed (will fall back): {}",
                e
            );
            return Ok(false);
        }

        // Step 2: Run region resolution
        let resolver = RegionResolveNode::new();
        resolver.process(ctx).await?;

        let has_regions = !ctx.artifacts.resolved_regions.is_empty();
        if has_regions {
            log::info!(
                "OpenDocHybrid: resolved {} regions, delegating to specialized recognizers",
                ctx.artifacts.resolved_regions.len()
            );
        }

        Ok(has_regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mode_default() {
        assert_eq!(
            DocumentParseMode::default(),
            DocumentParseMode::SpecializedStable
        );
    }

    #[test]
    fn test_parse_mode_equality() {
        assert_eq!(
            DocumentParseMode::OpenDocHybrid,
            DocumentParseMode::OpenDocHybrid
        );
        assert_ne!(
            DocumentParseMode::SpecializedStable,
            DocumentParseMode::OpenOcrText
        );
    }
}

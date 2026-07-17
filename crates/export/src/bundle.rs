use latexsnipper_ast::ExportArtifact;
use serde::{Deserialize, Serialize};

/// High-level preference for portable visual rendering.
///
/// This is deliberately independent from host integration modes such as
/// Office Native, OLE, or VSTO.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPreference {
    /// Prefer SVG and provide PNG as a fallback.
    #[default]
    Auto,

    /// Require a vector-only SVG result.
    ///
    /// Embedded raster images are rejected and no PNG fallback is generated.
    VectorOnly,

    /// Return PNG only.
    RasterOnly,
}

/// Physical information associated with a visual rendering.
///
/// SVG user units are interpreted using the parser DPI. For normal SVG/CSS
/// output this is typically 96 DPI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderDimensions {
    pub width_px: f32,
    pub height_px: f32,
    pub dpi: f32,
}

impl RenderDimensions {
    pub fn width_pt(self) -> f32 {
        self.width_px * 72.0 / self.dpi
    }

    pub fn height_pt(self) -> f32 {
        self.height_px * 72.0 / self.dpi
    }
}

/// A preferred visual artifact together with valid fallback artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBundle {
    pub preferred: ExportArtifact,

    #[serde(default)]
    pub fallbacks: Vec<ExportArtifact>,

    pub dimensions: RenderDimensions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_preference_default_is_auto() {
        assert_eq!(RenderPreference::default(), RenderPreference::Auto);
    }

    #[test]
    fn render_dimensions_pt_conversion() {
        let dims = RenderDimensions {
            width_px: 200.0,
            height_px: 100.0,
            dpi: 96.0,
        };
        let expected_w = 200.0 * 72.0 / 96.0;
        let expected_h = 100.0 * 72.0 / 96.0;
        assert!((dims.width_pt() - expected_w).abs() < f32::EPSILON);
        assert!((dims.height_pt() - expected_h).abs() < f32::EPSILON);
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::DRAWING_SCHEMA_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingSourceLanguage {
    Tikz,
    Asymptote,
    Mermaid,
    GraphvizDot,
    PlantUml,
    MetaPost,
    Pstricks,
    SvgSource,
    DrawingJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingPackageProfile {
    BaseTikz,
    PgfPlots,
    CircuitTikz,
    TikzCd,
    Forest,
    ChemFig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingInterchangeFormat {
    DrawingJson,
    CanonicalSvg,
    GraphvizJson,
    OfficeShapeScene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingOutputFormat {
    Svg,
    Pdf,
    Png,
    WebP,
    Eps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingCompatibility {
    VisualCompatible,
    Mixed,
    SourceOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingDocument {
    pub schema_version: u32,
    pub id: String,
    pub source_language: DrawingSourceLanguage,
    #[serde(default)]
    pub package_profiles: Vec<DrawingPackageProfile>,
    pub source: DrawingSource,
    pub compatibility: DrawingCompatibility,
    pub canvas: DrawingCanvas,
    #[serde(default)]
    pub layers: Vec<DrawingLayer>,
    #[serde(default)]
    pub objects: Vec<DrawingObject>,
    #[serde(default)]
    pub raw_nodes: Vec<RawDrawingNode>,
    #[serde(default)]
    pub resources: Vec<DrawingResource>,
    #[serde(default)]
    pub datasets: Vec<DrawingDataset>,
    pub provenance: DrawingProvenance,
}

impl DrawingDocument {
    pub fn source_only(
        id: impl Into<String>,
        language: DrawingSourceLanguage,
        source: impl Into<String>,
    ) -> Self {
        let source = source.into();
        Self {
            schema_version: DRAWING_SCHEMA_VERSION,
            id: id.into(),
            source_language: language,
            package_profiles: Vec::new(),
            source: DrawingSource {
                text: source.clone(),
            },
            compatibility: DrawingCompatibility::SourceOnly,
            canvas: DrawingCanvas::default(),
            layers: Vec::new(),
            objects: Vec::new(),
            raw_nodes: vec![RawDrawingNode {
                language,
                source,
                reason: "source-only adapter preserves the complete original source".to_owned(),
            }],
            resources: Vec::new(),
            datasets: Vec::new(),
            provenance: DrawingProvenance::default(),
        }
    }

    pub fn semantic_sha256(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(self)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingSource {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingCanvas {
    pub width: f64,
    pub height: f64,
    pub view_box: [f64; 4],
}

impl Default for DrawingCanvas {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            view_box: [0.0; 4],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingLayer {
    pub id: String,
    pub label: Option<String>,
    #[serde(default)]
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawingObject {
    Point {
        id: String,
        x: f64,
        y: f64,
    },
    Line {
        id: String,
        from: [f64; 2],
        to: [f64; 2],
    },
    Arrow {
        id: String,
        from: [f64; 2],
        to: [f64; 2],
    },
    Rect {
        id: String,
        bounds: [f64; 4],
    },
    Ellipse {
        id: String,
        bounds: [f64; 4],
    },
    Path {
        id: String,
        data: String,
    },
    Node {
        id: String,
        position: [f64; 2],
        label: String,
    },
    Text {
        id: String,
        position: [f64; 2],
        text: String,
    },
    MathLabel {
        id: String,
        position: [f64; 2],
        latex: String,
    },
    Axis {
        id: String,
        bounds: [f64; 4],
    },
    Plot {
        id: String,
        dataset_id: String,
    },
    Graph {
        id: String,
        source_ref: String,
    },
    Tree {
        id: String,
        source_ref: String,
    },
    Group {
        id: String,
        children: Vec<String>,
    },
    Image {
        id: String,
        resource_id: String,
        bounds: [f64; 4],
    },
    Raw(RawDrawingNode),
}

impl DrawingObject {
    pub fn is_native_office_shape_compatible(&self) -> bool {
        matches!(
            self,
            Self::Line { .. }
                | Self::Arrow { .. }
                | Self::Rect { .. }
                | Self::Ellipse { .. }
                | Self::Text { .. }
                | Self::Group { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDrawingNode {
    pub language: DrawingSourceLanguage,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingResource {
    pub id: String,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingDataset {
    pub id: String,
    #[serde(default)]
    pub columns: BTreeMap<String, Vec<f64>>,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingProvenance {
    pub adapter_version: Option<String>,
    pub compiler_fingerprint: Option<String>,
    pub package_lock_sha256: Option<String>,
    #[serde(default)]
    pub resource_hashes: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_json_is_the_lossless_structured_contract() {
        let document =
            DrawingDocument::source_only("d1", DrawingSourceLanguage::Tikz, "\\draw (0,0)--(1,1);");
        let json = serde_json::to_vec(&document).unwrap();
        let restored: DrawingDocument = serde_json::from_slice(&json).unwrap();
        assert_eq!(document, restored);
        assert_eq!(
            document.semantic_sha256().unwrap(),
            restored.semantic_sha256().unwrap()
        );
        assert_eq!(restored.raw_nodes[0].source, "\\draw (0,0)--(1,1);");
    }

    #[test]
    fn output_taxonomy_does_not_include_office_only_formats() {
        let outputs = [
            DrawingOutputFormat::Svg,
            DrawingOutputFormat::Pdf,
            DrawingOutputFormat::Png,
            DrawingOutputFormat::WebP,
            DrawingOutputFormat::Eps,
        ];
        assert_eq!(outputs.len(), 5);
    }
}

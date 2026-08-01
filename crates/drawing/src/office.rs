use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    DrawingCompatibility, DrawingDocument, DrawingObject, DrawingOutputFormat,
    DrawingPackageProfile, DrawingSourceLanguage,
};

pub const DRAWING_OFFICE_PAYLOAD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingArtifactRef {
    pub format: DrawingOutputFormat,
    pub content_ref: String,
    pub sha256: String,
    pub sanitizer_report_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeShapeScene {
    pub schema_version: u32,
    pub objects: Vec<DrawingObject>,
}

impl OfficeShapeScene {
    pub fn from_document(document: &DrawingDocument) -> Option<Self> {
        document
            .objects
            .iter()
            .all(DrawingObject::is_native_office_shape_compatible)
            .then(|| Self {
                schema_version: 1,
                objects: document.objects.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingOfficePayload {
    pub schema_version: u32,
    pub drawing_id: String,
    pub source_language: DrawingSourceLanguage,
    pub package_profiles: Vec<DrawingPackageProfile>,
    pub source: String,
    pub drawing_document: Option<DrawingDocument>,
    pub compatibility: DrawingCompatibility,
    pub preferred_artifact: DrawingArtifactRef,
    pub fallback_artifacts: Vec<DrawingArtifactRef>,
    pub office_shape_scene: Option<OfficeShapeScene>,
    pub width_points: f64,
    pub height_points: f64,
    pub source_sha256: String,
    pub scene_sha256: Option<String>,
    pub render_sha256: String,
    pub compiler_fingerprint: String,
    pub package_lock_sha256: Option<String>,
    pub resources_sha256: String,
}

impl DrawingOfficePayload {
    pub fn new(
        document: DrawingDocument,
        preferred_artifact: DrawingArtifactRef,
        fallback_artifacts: Vec<DrawingArtifactRef>,
        width_points: f64,
        height_points: f64,
        compiler_fingerprint: impl Into<String>,
        package_lock_sha256: Option<String>,
    ) -> Result<Self, serde_json::Error> {
        let source_sha256 = format!("{:x}", Sha256::digest(document.source.text.as_bytes()));
        let scene_sha256 = Some(document.semantic_sha256()?);
        let render_sha256 = preferred_artifact.sha256.clone();
        let mut resource_hashes = document
            .resources
            .iter()
            .map(|resource| resource.sha256.clone())
            .collect::<Vec<_>>();
        resource_hashes.sort();
        let resources_sha256 = format!("{:x}", Sha256::digest(resource_hashes.join("\n")));
        let office_shape_scene = OfficeShapeScene::from_document(&document);
        Ok(Self {
            schema_version: DRAWING_OFFICE_PAYLOAD_SCHEMA_VERSION,
            drawing_id: document.id.clone(),
            source_language: document.source_language,
            package_profiles: document.package_profiles.clone(),
            source: document.source.text.clone(),
            compatibility: document.compatibility,
            drawing_document: Some(document),
            preferred_artifact,
            fallback_artifacts,
            office_shape_scene,
            width_points,
            height_points,
            source_sha256,
            scene_sha256,
            render_sha256,
            compiler_fingerprint: compiler_fingerprint.into(),
            package_lock_sha256,
            resources_sha256,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingOfficeRoute {
    NativeShapes,
    DrawingOle,
    Svg,
    Png,
    PdfExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawingOfficeCapabilities {
    pub native_shapes: bool,
    pub drawing_ole: bool,
    pub svg: bool,
    pub png: bool,
    pub pdf_export: bool,
}

pub fn select_office_route(
    payload: &DrawingOfficePayload,
    capabilities: DrawingOfficeCapabilities,
    request_native_editing: bool,
    request_double_click_editing: bool,
    export_or_print: bool,
) -> Option<DrawingOfficeRoute> {
    if export_or_print && capabilities.pdf_export {
        return Some(DrawingOfficeRoute::PdfExport);
    }
    if request_native_editing && capabilities.native_shapes && payload.office_shape_scene.is_some()
    {
        return Some(DrawingOfficeRoute::NativeShapes);
    }
    if request_double_click_editing && capabilities.drawing_ole {
        return Some(DrawingOfficeRoute::DrawingOle);
    }
    if capabilities.svg && artifact_available(payload, DrawingOutputFormat::Svg) {
        return Some(DrawingOfficeRoute::Svg);
    }
    if capabilities.png && artifact_available(payload, DrawingOutputFormat::Png) {
        return Some(DrawingOfficeRoute::Png);
    }
    None
}

fn artifact_available(payload: &DrawingOfficePayload, format: DrawingOutputFormat) -> bool {
    payload.preferred_artifact.format == format
        || payload
            .fallback_artifacts
            .iter()
            .any(|artifact| artifact.format == format)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingFailureCandidate {
    pub schema_version: u32,
    pub candidate_id: String,
    pub source_language: DrawingSourceLanguage,
    pub package_profiles: Vec<DrawingPackageProfile>,
    pub input_hash: String,
    pub adapter_version: Option<String>,
    pub compiler_fingerprint: Option<String>,
    pub package_lock_sha256: Option<String>,
    pub resource_hashes: Vec<String>,
    pub output_format: Option<DrawingOutputFormat>,
    pub first_diagnostic: String,
    pub scene_hash: Option<String>,
    pub render_hash: Option<String>,
    pub contains_raw_user_data: bool,
    pub redistributable: bool,
    pub status: String,
}

impl DrawingFailureCandidate {
    pub fn sanitized(
        candidate_id: impl Into<String>,
        language: DrawingSourceLanguage,
        source: &str,
        diagnostic: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 1,
            candidate_id: candidate_id.into(),
            source_language: language,
            package_profiles: Vec::new(),
            input_hash: format!("{:x}", Sha256::digest(source.as_bytes())),
            adapter_version: None,
            compiler_fingerprint: None,
            package_lock_sha256: None,
            resource_hashes: Vec::new(),
            output_format: None,
            first_diagnostic: diagnostic.into(),
            scene_hash: None,
            render_hash: None,
            contains_raw_user_data: false,
            redistributable: false,
            status: "new".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DrawingSource;

    fn payload(objects: Vec<DrawingObject>) -> DrawingOfficePayload {
        let document = DrawingDocument {
            schema_version: 1,
            id: "d".to_owned(),
            source_language: DrawingSourceLanguage::DrawingJson,
            package_profiles: Vec::new(),
            source: DrawingSource {
                text: "{}".to_owned(),
            },
            compatibility: DrawingCompatibility::VisualCompatible,
            canvas: Default::default(),
            layers: Vec::new(),
            objects,
            raw_nodes: Vec::new(),
            resources: Vec::new(),
            datasets: Vec::new(),
            provenance: Default::default(),
        };
        DrawingOfficePayload::new(
            document,
            DrawingArtifactRef {
                format: DrawingOutputFormat::Svg,
                content_ref: "drawing.svg".to_owned(),
                sha256: "a".repeat(64),
                sanitizer_report_sha256: Some("b".repeat(64)),
            },
            vec![DrawingArtifactRef {
                format: DrawingOutputFormat::Png,
                content_ref: "drawing.png".to_owned(),
                sha256: "c".repeat(64),
                sanitizer_report_sha256: None,
            }],
            100.0,
            50.0,
            "renderer-sha256:demo",
            None,
        )
        .unwrap()
    }

    #[test]
    fn native_shape_subset_falls_back_to_svg_for_complex_paths() {
        let capabilities = DrawingOfficeCapabilities {
            native_shapes: true,
            drawing_ole: false,
            svg: true,
            png: true,
            pdf_export: true,
        };
        let simple = payload(vec![DrawingObject::Rect {
            id: "r".to_owned(),
            bounds: [0.0, 0.0, 1.0, 1.0],
        }]);
        assert_eq!(
            select_office_route(&simple, capabilities, true, false, false),
            Some(DrawingOfficeRoute::NativeShapes)
        );
        let complex = payload(vec![DrawingObject::Path {
            id: "p".to_owned(),
            data: "M0 0C1 2 3 4 5 6".to_owned(),
        }]);
        assert_eq!(
            select_office_route(&complex, capabilities, true, false, false),
            Some(DrawingOfficeRoute::Svg)
        );
    }

    #[test]
    fn failure_candidate_contains_hash_not_raw_source() {
        let candidate = DrawingFailureCandidate::sanitized(
            "c1",
            DrawingSourceLanguage::Tikz,
            "private source",
            "compile failed",
        );
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(!json.contains("private source"));
        assert!(!candidate.contains_raw_user_data);
    }

    #[test]
    fn office_payload_is_forward_compatible() {
        let mut value = serde_json::to_value(payload(Vec::new())).unwrap();
        value["futureField"] = serde_json::json!({"v": 2});
        let parsed: DrawingOfficePayload = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.schema_version, 1);
    }
}

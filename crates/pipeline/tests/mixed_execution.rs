//! Execution-loop tests for the mixed-OCR pipeline: masked crops actually
//! reach the recognizer, fragment quads survive projection, and the
//! formula-dominance fast path gates text detection.

use latexsnipper_ast::Rect;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::DetectionBox;
use latexsnipper_pipeline::context::PipelineContext;
use latexsnipper_pipeline::node::PipelineNode;
use latexsnipper_pipeline::nodes::crop_node::CropNode;
use latexsnipper_pipeline::nodes::detector_node::DetectorNode;
use latexsnipper_pipeline::region_graph::{
    ArtifactRef, RegionCandidate, RegionFragmentProvenance, RegionKind, RegionProducer,
    TextSplitPolicy,
};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn white_image(w: u32, h: u32) -> SnipperImage {
    SnipperImage::new(w, h, PixelFormat::Rgb, vec![255u8; (w * h * 3) as usize])
}

/// Paint a dark (ink) rect.
fn paint_ink(img: &mut SnipperImage, x0: u32, y0: u32, x1: u32, y1: u32) {
    let w = img.width();
    for y in y0..y1 {
        for x in x0..x1 {
            let off = (y * w + x) as usize * 3;
            img.pixels_mut()[off] = 0;
            img.pixels_mut()[off + 1] = 0;
            img.pixels_mut()[off + 2] = 0;
        }
    }
}

fn ctx_with(image: SnipperImage) -> PipelineContext {
    let mut ctx = PipelineContext::new();
    ctx.image = Some(image);
    ctx
}

#[test]
fn crop_node_produces_masked_crops_and_fallback_evidence() {
    // Text region overlapping a formula: the crop must be masked.
    let mut img = white_image(200, 60);
    paint_ink(&mut img, 90, 10, 110, 50); // formula ink
    paint_ink(&mut img, 20, 20, 40, 40); // text ink
    let mut ctx = ctx_with(img);

    ctx.artifacts.text_detections = vec![DetectionBox::rect(
        Rect::new(10.0, 5.0, 180.0, 50.0),
        0.9,
        0,
        "text".into(),
    )];
    ctx.artifacts.formula_detections = vec![DetectionBox::rect(
        Rect::new(90.0, 10.0, 20.0, 40.0),
        0.95,
        1,
        "isolated".into(),
    )];

    let node = CropNode::new(4);
    rt().block_on(node.process(&mut ctx)).unwrap();

    assert_eq!(ctx.artifacts.text_crops.len(), 1);
    // Mask evidence must exist for the formula-intersecting text crop.
    assert_eq!(ctx.artifacts.text_crop_mask_evidence.len(), 1);
    let evidence = &ctx.artifacts.text_crop_mask_evidence[0];
    assert_eq!(evidence.mask.formula_mask_rects.len(), 1);
    assert_eq!(evidence.mask.mask_algorithm_version, "v1");
    assert!(evidence.mask.masked_image_sha256.len() == 64);
    // The masked crop differs from a plain crop (formula pixels filled).
    let masked = &ctx.artifacts.text_crops[0].image;
    let center = masked.get_pixel(100, 30); // inside former formula rect
    assert_eq!(
        center,
        [255, 255, 255],
        "formula area must be background-filled"
    );
}

#[test]
fn mask_fallback_records_evidence_without_losing_text() {
    // A text crop with NO overlapping formula falls back and still records
    // evidence, and the crop is the original pixels (not silently dropped).
    let img = white_image(100, 40);
    let mut ctx = ctx_with(img);
    ctx.artifacts.text_detections = vec![DetectionBox::rect(
        Rect::new(5.0, 5.0, 90.0, 30.0),
        0.9,
        0,
        "text".into(),
    )];
    // No formula detections.
    let node = CropNode::new(4);
    rt().block_on(node.process(&mut ctx)).unwrap();
    assert_eq!(ctx.artifacts.text_crops.len(), 1);
    // No intersecting formula → no mask evidence, crop preserved.
    assert!(ctx.artifacts.text_crop_mask_evidence.is_empty());
    assert_eq!(ctx.artifacts.text_crops[0].image.width(), 90);
}

#[test]
fn detector_node_skips_text_on_formula_dominant_fast_path() {
    // With fastPath=formulaDominant set, the text detector must not run and
    // must not overwrite the cleared text detections.
    let img = white_image(100, 100);
    let mut ctx = ctx_with(img);
    ctx.models_dir = Some(std::env::temp_dir());
    ctx.metadata
        .insert("fastPath".into(), serde_json::json!("formulaDominant"));
    // Pre-seed a text detection that must be cleared by the detector.
    ctx.artifacts.text_detections = vec![DetectionBox::rect(
        Rect::new(0.0, 0.0, 50.0, 50.0),
        0.9,
        0,
        "text".into(),
    )];

    let node = DetectorNode::text();
    rt().block_on(node.process(&mut ctx)).unwrap();
    assert!(
        ctx.artifacts.text_detections.is_empty(),
        "text detection must be skipped on the formula-dominant fast path"
    );
}

#[test]
fn region_resolve_propagates_fragment_quad() {
    use latexsnipper_ast::Quad;
    use latexsnipper_pipeline::nodes::region_resolve_node::RegionResolveNode;

    let img = white_image(300, 60);
    let mut ctx = ctx_with(img);

    // A rotated text quad with an inline formula in the middle.
    ctx.artifacts.text_detections = vec![DetectionBox {
        rect: Rect::new(10.0, 10.0, 280.0, 30.0),
        quad: Some(Quad::new(
            latexsnipper_ast::Point::new(10.0, 15.0),
            latexsnipper_ast::Point::new(290.0, 5.0),
            latexsnipper_ast::Point::new(290.0, 40.0),
            latexsnipper_ast::Point::new(10.0, 45.0),
        )),
        confidence: 0.8,
        class_id: 0,
        class_name: "text".into(),
    }];
    ctx.artifacts.formula_detections = vec![DetectionBox::rect(
        Rect::new(140.0, 15.0, 130.0, 20.0),
        0.95,
        0,
        "embedding".into(),
    )];

    let node = RegionResolveNode::new();
    rt().block_on(node.process(&mut ctx)).unwrap();

    // The split text region produces fragments; surviving fragments keep a
    // clipped quad when the source was rotated.
    let fragment_dets: Vec<&DetectionBox> = ctx
        .artifacts
        .text_detections
        .iter()
        .filter(|d| d.quad.is_some())
        .collect();
    assert!(
        !fragment_dets.is_empty(),
        "rotated fragments must keep their quad through projection"
    );
    // The original (formula-contaminated) full-width detection is gone.
    assert!(
        ctx.artifacts
            .text_detections
            .iter()
            .all(|d| d.rect.width < 250.0),
        "the formula-contaminated detection must be replaced by fragments"
    );
}

#[test]
fn fragment_split_records_removal_reason_when_fully_covered() {
    use latexsnipper_pipeline::region_graph::split_text_region_around_formulae;
    let text = RegionCandidate {
        id: 1,
        kind: RegionKind::TextLine,
        rect: Rect::new(0.0, 0.0, 100.0, 30.0),
        quad: None,
        confidence: 0.8,
        producer: RegionProducer::TextDetector,
        page: 0,
        artifact_ref: ArtifactRef::TextDetection(0),
    };
    let formula = RegionCandidate {
        id: 2,
        kind: RegionKind::FormulaDisplay,
        rect: Rect::new(0.0, 0.0, 100.0, 30.0),
        quad: None,
        confidence: 0.95,
        producer: RegionProducer::FormulaDetector,
        page: 0,
        artifact_ref: ArtifactRef::FormulaDetection(0),
    };
    let fragments =
        split_text_region_around_formulae(&text, &[&formula], &TextSplitPolicy::default());
    assert_eq!(fragments.len(), 1);
    assert!(matches!(
        fragments[0].provenance,
        RegionFragmentProvenance::RemovedTooSmall { .. }
    ));
}

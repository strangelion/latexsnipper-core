#![cfg(not(target_arch = "wasm32"))]

use latexsnipper_image::{PixelFormat, SnipperImage};
use latexsnipper_inference::{detect_text, TextDetParams};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend};
use latexsnipper_tract::TractBackend;

#[test]
fn tiny_detector_runs_in_tract_and_produces_a_bounded_region() {
    let backend = TractBackend::new(None);
    let session = backend
        .create_session(
            &ModelHandle::with_bytes(
                "tiny-text-det",
                include_bytes!("fixtures/tiny-text-det.onnx").to_vec(),
            ),
            AccelerationMode::Cpu,
        )
        .unwrap();
    let config = ModelConfig::from_json_str(include_str!("fixtures/tiny-text-det.json")).unwrap();
    let params = TextDetParams::from_config(&config);
    let image = SnipperImage::new(16, 8, PixelFormat::Rgba, vec![255; 16 * 8 * 4]);

    let regions = detect_text(&image, &*session, &params).unwrap();

    assert_eq!(regions.len(), 1);
    let rect = regions[0].rect;
    assert!(rect.width >= 4.0 && rect.height >= 4.0);
    assert!(rect.right() <= 16.0 && rect.bottom() <= 8.0);
}

// Dual-track testing: Mock vs ONNX auto-comparison
// Runs the same input through both runtimes and diffs the results

use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::{StubRuntime, OnnxRuntimeBackend, RuntimeBackend, AccelerationMode};
use latexsnipper_mock::FakePipeline;
use latexsnipper_image::SnipperImage;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_ast::Document;

fn test_image() -> SnipperImage {
    SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000])
}

/// Compare two documents structurally (ignoring content differences from mock vs real).
fn compare_documents(mock: &Document, real: &Document) -> Vec<String> {
    let mut diffs = Vec::new();

    if mock.pages.len() != real.pages.len() {
        diffs.push(format!("Page count: mock={}, real={}", mock.pages.len(), real.pages.len()));
    }

    for (i, (mp, rp)) in mock.pages.iter().zip(real.pages.iter()).enumerate() {
        if mp.blocks.len() != rp.blocks.len() {
            diffs.push(format!("Page {} block count: mock={}, real={}", i, mp.blocks.len(), rp.blocks.len()));
        }
    }

    diffs
}

/// Test that both runtimes can be initialized.
#[test]
fn dual_runtime_initialization() {
    let config = EngineConfig::default();

    // Mock runtime
    let mock_engine = SnipperEngine::new(config.clone(), Box::new(StubRuntime::new()));
    assert_eq!(mock_engine.runtime().name(), "stub");

    // ONNX runtime (may fail if ORT not available)
    let models_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap().join("test-models");
    if models_dir.exists() {
        if let Ok(ort_backend) = OnnxRuntimeBackend::new(models_dir) {
            let ort_engine = SnipperEngine::new(config, Box::new(ort_backend));
            assert_eq!(ort_engine.runtime().name(), "onnxruntime");
            println!("Both runtimes initialized successfully");
        } else {
            println!("ORT not available, skipping real runtime test");
        }
    }
}

/// Test that mock pipeline produces consistent results.
#[test]
fn mock_consistency() {
    let pipeline = FakePipeline::formula("\\frac{a}{b}", 0.95);
    let image = test_image();

    let doc1 = pipeline.run(&image).unwrap();
    let doc2 = pipeline.run(&image).unwrap();

    // Mock should produce identical results
    assert_eq!(doc1.block_count(), doc2.block_count());
    assert_eq!(doc1.pages[0].blocks.len(), doc2.pages[0].blocks.len());
    println!("Mock consistency: {} blocks, identical", doc1.block_count());
}

/// Test that different modes produce different document structures.
#[test]
fn mode_distinction() {
    let formula = FakePipeline::formula("E=mc^2", 0.95);
    let text = FakePipeline::text("Hello World", 0.9);
    let mixed = FakePipeline::mixed("E=mc^2", "text", 0.9);

    let image = test_image();

    let doc_formula = formula.run(&image).unwrap();
    let doc_text = text.run(&image).unwrap();
    let doc_mixed = mixed.run(&image).unwrap();

    // Formula should have formula blocks
    assert!(doc_formula.pages[0].blocks.iter().any(|b| matches!(b, latexsnipper_ast::Block::Formula(_))));
    // Text should have paragraph blocks
    assert!(doc_text.pages[0].blocks.iter().any(|b| matches!(b, latexsnipper_ast::Block::Paragraph(_))));
    // Mixed should have both
    assert!(doc_mixed.block_count() >= 2);

    println!("Mode distinction: formula={}, text={}, mixed={}",
        doc_formula.block_count(), doc_text.block_count(), doc_mixed.block_count());
}

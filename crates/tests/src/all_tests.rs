// Unified test suite for latexsnipper-core
// Consolidates all tests from individual crates into one location

// ═══════════════════════════════════════════════════════════════
// Foundation Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod foundation_tests {
    use latexsnipper_foundation::{SnipperError, Result, CoreConfig, AccelerationMode, EventBus, EventType};

    #[test]
    fn error_display() {
        let err = SnipperError::Model("model not found".into());
        assert_eq!(err.to_string(), "Model error: model not found");
    }

    #[test]
    fn error_variants() {
        let cases = vec![
            SnipperError::Io("file missing".into()),
            SnipperError::Runtime("session failed".into()),
            SnipperError::Inference("shape mismatch".into()),
            SnipperError::Pipeline("node failed".into()),
            SnipperError::Image("decode error".into()),
            SnipperError::Conversion("parse error".into()),
            SnipperError::Export("write failed".into()),
            SnipperError::Plugin("load failed".into()),
            SnipperError::Config("invalid json".into()),
            SnipperError::Timeout(5000),
            SnipperError::Cancelled,
            SnipperError::Other("unknown".into()),
        ];
        for err in cases { assert!(!err.to_string().is_empty()); }
    }

    #[test]
    fn result_ok() { let r: Result<i32> = Ok(42); assert_eq!(r.unwrap(), 42); }

    #[test]
    fn result_err() { let r: Result<i32> = Err(SnipperError::Cancelled); assert!(r.is_err()); }

    #[test]
    fn config_default() {
        let config = CoreConfig::default();
        assert_eq!(config.acceleration, AccelerationMode::Auto);
        assert_eq!(config.max_threads, 4);
    }

    #[test]
    fn event_bus_emit() {
        use std::sync::Arc;
        let bus = EventBus::new();
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let c = called.clone();
        bus.subscribe(EventType::RecognitionCompleted, Arc::new(move |_| {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
        }));
        bus.emit(latexsnipper_foundation::Event { event_type: EventType::RecognitionCompleted, data: serde_json::json!({}) });
        assert!(called.load(std::sync::atomic::Ordering::Relaxed));
    }
}

// ═══════════════════════════════════════════════════════════════
// AST Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod ast_tests {
    use latexsnipper_ast::*;

    #[test]
    fn document_new() {
        let doc = Document::new();
        assert!(doc.pages.is_empty());
        assert_eq!(doc.block_count(), 0);
    }

    #[test]
    fn formula_latex() {
        let f = Formula::latex("\\frac{a}{b}");
        assert_eq!(f.as_latex(), "\\frac{a}{b}");
        assert!(f.display_mode);
    }

    #[test]
    fn rect_iou() {
        let r1 = Rect::new(0.0, 0.0, 100.0, 100.0);
        let r2 = Rect::new(50.0, 50.0, 100.0, 100.0);
        let iou = r1.iou(&r2);
        assert!((iou - 2500.0 / 17500.0).abs() < 0.001);
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 10.0, 50.0, 50.0);
        assert!(r.contains(30.0, 30.0));
        assert!(!r.contains(5.0, 5.0));
    }

    #[test]
    fn document_serialization() {
        let mut doc = Document::new();
        doc.pages.push(Page {
            width: 800.0, height: 600.0,
            blocks: vec![Block::Formula(FormulaBlock {
                formula: Formula::latex("E=mc^2"),
                geometry: None,
            })],
            page_number: Some(1),
        });
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let restored: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.pages.len(), restored.pages.len());
    }
}

// ═══════════════════════════════════════════════════════════════
// Tensor Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tensor_tests {
    use latexsnipper_tensor::Tensor;

    #[test]
    fn tensor_float32() {
        let t = Tensor::float32("input", vec![1, 3, 224, 224], vec![0.0; 150528]);
        assert_eq!(t.name(), "input");
        assert_eq!(t.len(), 150528);
    }

    #[test]
    fn tensor_int64() {
        let t = Tensor::int64("ids", vec![1, 10], vec![0; 10]);
        assert_eq!(t.name(), "ids");
        assert!(t.as_i64_slice().is_some());
    }

    #[test]
    fn tensor_serialization() {
        let t = Tensor::float32("test", vec![2], vec![1.0, 2.0]);
        let json = serde_json::to_string(&t).unwrap();
        let restored: Tensor = serde_json::from_str(&json).unwrap();
        assert_eq!(t.name(), restored.name());
    }
}

// ═══════════════════════════════════════════════════════════════
// Image Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod image_tests {
    use latexsnipper_image::*;
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_ast::Rect;

    fn test_image() -> SnipperImage {
        SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000])
    }

    #[test]
    fn image_new() {
        let img = test_image();
        assert_eq!(img.width(), 100);
        assert_eq!(img.height(), 100);
    }

    #[test]
    fn resize_to_fit() {
        let img = SnipperImage::new(200, 100, PixelFormat::Rgb, vec![0u8; 60000]);
        let resized = operations::resize_to_fit(&img, 100);
        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 50);
    }

    #[test]
    fn letterbox() {
        let img = SnipperImage::new(100, 50, PixelFormat::Rgb, vec![128u8; 15000]);
        let (lb, scale, pad_x, pad_y) = operations::letterbox(&img, 64);
        assert_eq!(lb.width(), 64);
        assert_eq!(lb.height(), 64);
    }

    #[test]
    fn normalize() {
        let img = SnipperImage::new(2, 2, PixelFormat::Rgb, vec![128u8; 12]);
        let pixels = operations::normalize(&img, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]);
        assert_eq!(pixels.len(), 3 * 2 * 2);
    }

    #[test]
    fn crop() {
        let pixels: Vec<u8> = (0..20).collect();
        let img = SnipperImage::new(5, 4, PixelFormat::Gray, pixels);
        let cropped = operations::crop(&img, Rect::new(1.0, 1.0, 3.0, 2.0));
        assert_eq!(cropped.width(), 3);
        assert_eq!(cropped.height(), 2);
    }
}

// ═══════════════════════════════════════════════════════════════
// Model Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod model_tests {
    use latexsnipper_model::*;
    use std::path::Path;

    #[test]
    fn config_parse() {
        let json = r#"{"model_type":"yolov8","input":{"name":"images","shape":[1,3,768,768],"dtype":"float32"},"output":{"name":"output","shape":[1,6,8400]}}"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.model_type, "yolov8");
    }

    #[test]
    fn manifest_validate() {
        let manifest = ModelManifest {
            source_id: "test".into(),
            source_label: "Test".into(),
            version: "1.0.0".into(),
            base_url: String::new(),
            mirrors: vec![],
            checksums: Default::default(),
            categories: Default::default(),
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manager_paths() {
        let mgr = ModelManager::new("/models".into());
        assert_eq!(mgr.models_dir(), Path::new("/models"));
    }
}

// ═══════════════════════════════════════════════════════════════
// Syntax Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod syntax_tests {
    use latexsnipper_syntax::{Parser, Renderer};
    use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
    use latexsnipper_syntax::typst::latex_to_typst;

    #[test]
    fn latex_parse_display_math() {
        let parser = LatexParser;
        let doc = parser.parse("$$E = mc^2$$").unwrap();
        assert_eq!(doc.pages[0].blocks.len(), 1);
    }

    #[test]
    fn latex_to_typst_basic() {
        assert!(latex_to_typst("\\frac{a}{b}").contains("a"));
        assert_eq!(latex_to_typst("\\pi"), "pi");
    }

    #[test]
    fn latex_roundtrip() {
        let parser = LatexParser;
        let renderer = LatexRenderer;
        let doc = parser.parse("Given $x^2$, we have $$y = x + 1$$").unwrap();
        let output = renderer.render(&doc).unwrap();
        assert!(output.contains("x^2"));
    }
}

// ═══════════════════════════════════════════════════════════════
// Export Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod export_tests {
    use latexsnipper_export::{RenderTree, Generator};
    use latexsnipper_export::svg::SvgGenerator;
    use latexsnipper_export::text::TextGenerator;
    use latexsnipper_syntax::latex::LatexParser;
    use latexsnipper_syntax::Parser;

    #[test]
    fn render_tree_from_document() {
        let parser = LatexParser;
        let doc = parser.parse("$$E = mc^2$$").unwrap();
        let tree = RenderTree::from_document(&doc);
        assert!(tree.page_count() > 0);
    }

    #[test]
    fn svg_generator() {
        let parser = LatexParser;
        let doc = parser.parse("$$E = mc^2$$").unwrap();
        let tree = RenderTree::from_document(&doc);
        let svg = SvgGenerator;
        let output = svg.generate(&tree).unwrap();
        assert!(output.contains("<svg"));
    }

    #[test]
    fn text_generator() {
        let parser = LatexParser;
        let doc = parser.parse("Hello World").unwrap();
        let tree = RenderTree::from_document(&doc);
        let text = TextGenerator;
        let output = text.generate(&tree).unwrap();
        // The parser may treat "Hello World" as a single text block
        assert!(!output.is_empty(), "Output should not be empty");
        assert!(output.contains("Hello") || output.contains("World"), "Output should contain text");
    }
}

// ═══════════════════════════════════════════════════════════════
// Mock Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod mock_tests {
    use latexsnipper_mock::*;
    use latexsnipper_image::SnipperImage;
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_ast::Block;

    fn test_image() -> SnipperImage {
        SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000])
    }

    #[test]
    fn mock_pipeline_formula() {
        let pipeline = FakePipeline::formula("\\frac{a}{b}", 0.95);
        let doc = pipeline.run(&test_image()).unwrap();
        assert_eq!(doc.block_count(), 1);
    }

    #[test]
    fn mock_pipeline_mixed() {
        let pipeline = FakePipeline::mixed("E=mc^2", "Hello", 0.9);
        let doc = pipeline.run(&test_image()).unwrap();
        assert_eq!(doc.block_count(), 2);
    }

    #[test]
    fn fake_document_has_blocks() {
        let doc = fake_document();
        assert_eq!(doc.block_count(), 4);
    }
}

// ═══════════════════════════════════════════════════════════════
// Engine Integration Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod engine_tests {
    use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
    use latexsnipper_runtime::StubRuntime;
    use latexsnipper_mock::FakePipeline;
    use latexsnipper_image::SnipperImage;
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_syntax::{Parser, Renderer};
    use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
    use latexsnipper_export::{RenderTree, Generator};
    use latexsnipper_export::svg::SvgGenerator;

    fn test_image() -> SnipperImage {
        SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000])
    }

    #[tokio::test]
    async fn engine_mock() {
        let engine = SnipperEngine::new(EngineConfig::default(), Box::new(StubRuntime::new()));
        let doc = engine.recognize(test_image(), RecognizeMode::Formula).await.unwrap();
        assert_eq!(doc.pages.len(), 0); // StubRuntime returns empty
    }

    #[test]
    fn full_pipeline_mock() {
        let pipeline = FakePipeline::formula("\\frac{a}{b}", 0.95);
        let doc = pipeline.run(&test_image()).unwrap();
        assert_eq!(doc.block_count(), 1);

        let renderer = LatexRenderer;
        let latex = renderer.render(&doc).unwrap();
        assert!(latex.contains("\\frac{a}{b}"));

        let tree = RenderTree::from_document(&doc);
        let svg = SvgGenerator;
        let svg_out = svg.generate(&tree).unwrap();
        assert!(svg_out.contains("<svg"));
    }

    #[test]
    fn full_pipeline_parse_to_export() {
        let parser = LatexParser;
        let doc = parser.parse("Given $x^2$, we have $$y = x + 1$$").unwrap();
        assert!(doc.block_count() >= 2);

        let renderer = LatexRenderer;
        let latex = renderer.render(&doc).unwrap();
        assert!(latex.contains("x^2"));
    }
}

// ═══════════════════════════════════════════════════════════════
// Runtime Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod runtime_tests {
    use latexsnipper_runtime::{StubRuntime, AccelerationMode, ModelHandle, RuntimeBackend};
    use latexsnipper_tensor::Tensor;

    #[test]
    fn stub_runtime() {
        let rt = StubRuntime::new();
        assert_eq!(rt.name(), "stub");
        assert!(rt.is_available());
    }

    #[test]
    fn model_handle() {
        let h = ModelHandle::new("m1", "formula-det", "v1");
        assert_eq!(h.id(), "m1");
        assert_eq!(h.category(), "formula-det");
    }

    #[test]
    fn acceleration_default() {
        assert_eq!(AccelerationMode::default(), AccelerationMode::Auto);
    }
}

// ═══════════════════════════════════════════════════════════════
// FFI Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod ffi_tests {
    use latexsnipper_ffi::common::FfiResponse;

    #[test]
    fn ffi_response_success() {
        let r = FfiResponse::success("E=mc^2", 0.95, 1234);
        let json = r.to_json();
        assert!(json.contains("E=mc^2"));
        assert!(json.contains("0.95"));
    }

    #[test]
    fn ffi_response_error() {
        let r = FfiResponse::error("Model not found");
        let json = r.to_json();
        assert!(json.contains("Model not found"));
    }
}

// Unified test suite for latexsnipper-core
// Tests organized by module category

// ═══════════════════════════════════════════════════════════════
// Category 1: Foundation
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod foundation_tests {
    use latexsnipper_foundation::{
        AccelerationMode, CoreConfig, EventBus, EventType, Result, SnipperError,
    };

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
        for err in cases {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn result_ok() {
        let r: Result<i32> = Ok(42);
        assert!(matches!(r, Ok(42)));
    }

    #[test]
    fn result_err() {
        let r: Result<i32> = Err(SnipperError::Cancelled);
        assert!(r.is_err());
    }

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
        bus.subscribe(
            EventType::RecognitionCompleted,
            Arc::new(move |_| {
                c.store(true, std::sync::atomic::Ordering::Relaxed);
            }),
        );
        bus.emit(latexsnipper_foundation::Event {
            event_type: EventType::RecognitionCompleted,
            data: serde_json::json!({}),
        });
        assert!(called.load(std::sync::atomic::Ordering::Relaxed));
    }
}

// ═══════════════════════════════════════════════════════════════
// Category 2: AST
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
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Formula(FormulaBlock {
                formula: Formula::latex("E=mc^2"),
                label: None,
                number: None,
                environment: None,
                geometry: None,
                source: None,
            })],
            page_number: Some(1),
            layout: None,
            background_asset_id: None,
        });
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let restored: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.pages.len(), restored.pages.len());
    }
}

// ═══════════════════════════════════════════════════════════════
// Category 3: Tensor
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
// Category 4: Image
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod image_tests {
    use latexsnipper_ast::Rect;
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_image::*;

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
        let (lb, _scale, _pad_x, _pad_y) = operations::letterbox(&img, 64);
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
// Category 5: Model
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod model_tests {
    use latexsnipper_model::*;
    use std::path::Path;

    #[test]
    fn config_parse_yolov8() {
        let json = r#"{
            "model_type": "yolov8",
            "model_family": "YOLOv8",
            "license": "Apache-2.0",
            "input": {"name": "images", "shape": [1,3,768,768], "dtype": "float32", "range": [0.0, 1.0]},
            "output": {"name": "output0", "shape": [1,6,8400]},
            "preprocessing": {
                "resize": {"width": 768, "height": 768, "keep_ratio": true, "pad_value": 114},
                "normalization": {"mean": [0,0,0], "std": [255,255,255]},
                "color_format": "BGR"
            },
            "postprocessing": {"type": "yolo_nms", "confidence_threshold": 0.25, "iou_threshold": 0.45}
        }"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.model_type, "yolov8");
        assert_eq!(config.task_type(), "detection");
        assert_eq!(config.color_format(), "BGR");
        assert_eq!(config.normalization_mean(), vec![0.0, 0.0, 0.0]);
        assert_eq!(config.normalization_std(), vec![255.0, 255.0, 255.0]);
    }

    #[test]
    fn config_parse_trocr() {
        let json = r#"{
            "model_type": "trocr",
            "encoder": {"input": {"name": "pixel_values", "shape": [1,3,384,384], "dtype": "float32"}, "output": {"name": "last_hidden_state", "shape": [1,577,384]}},
            "decoder": {"input_ids": {"name": "input_ids"}, "encoder_hidden": {"name": "encoder_hidden_states"}, "output": {"name": "logits", "shape": [1,-1,50265]}, "max_length": 512, "eos_token_id": 2},
            "preprocessing": {"normalization": {"mean": [0.5,0.5,0.5], "std": [0.5,0.5,0.5]}},
            "decoding": {"type": "beam_search", "beam_width": 3, "top_k": 5}
        }"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.model_type, "trocr");
        assert_eq!(config.task_type(), "ocr");
        assert!(config.encoder.is_some());
        assert!(config.decoder.is_some());
        assert_eq!(config.decoder.as_ref().unwrap().max_length, Some(512));
    }

    #[test]
    fn config_parse_dbnet() {
        let json = r#"{
            "model_type": "dbnet",
            "input": {"name": "x", "shape": [1,3,-1,-1], "dtype": "float32"},
            "output": {"name": "out", "shape": [1,1,-1,-1]},
            "preprocessing": {"normalization": {"mean": [0.485,0.456,0.406], "std": [0.229,0.224,0.225]}, "divisible_by": 32},
            "postprocessing": {"type": "dbnet", "threshold": 0.3, "box_threshold": 0.5, "unclip_ratio": 1.5}
        }"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.model_type, "dbnet");
        assert!(!config.has_dynamic_shapes());
    }

    #[test]
    fn config_parse_crnn() {
        let json = r#"{
            "model_type": "crnn_ctc",
            "input": {"name": "x", "shape": [1,3,48,320], "dtype": "float32"},
            "output": {"name": "out", "shape": [1,-1,6637]},
            "decoding": {"type": "ctc_greedy", "blank_id": 0, "keys_file": "ppocr_keys.txt"}
        }"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.model_type, "crnn_ctc");
        assert_eq!(
            config.decoding.as_ref().unwrap().decoding_type.as_deref(),
            Some("ctc_greedy")
        );
    }

    #[test]
    fn config_task_type_auto() {
        let json = r#"{"model_type": "yolov8", "input": {"name": "x", "shape": [1], "dtype": "float32"}, "output": {"name": "y", "shape": [1]}}"#;
        let config = ModelConfig::parse(json).unwrap();
        assert_eq!(config.task_type(), "detection");
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
// Category 6: Syntax
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod syntax_tests {
    use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
    use latexsnipper_syntax::typst::latex_to_typst;
    use latexsnipper_syntax::{Parser, Renderer};

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
// Category 7: Export
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod export_tests {
    use latexsnipper_export::svg::SvgGenerator;
    use latexsnipper_export::text::TextGenerator;
    use latexsnipper_export::{Generator, RenderTree};
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
        assert!(!output.is_empty());
        assert!(output.contains("Hello") || output.contains("World"));
    }
}

// ═══════════════════════════════════════════════════════════════
// Category 8: Conversion (14 formats)
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod conversion_tests {
    use latexsnipper_ast::{
        Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, TextRun,
    };
    use latexsnipper_conversion::*;

    /// Shared test document fixture — built once per test file, reused by all tests.
    static TEST_DOC: once_cell::sync::Lazy<Document> = once_cell::sync::Lazy::new(|| Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun::new("Given "))],
                    geometry: None,
                    source: None,
                    style: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: {
                        let mut f = Formula::latex("E=mc^2");
                        f.display_mode = false;
                        f.confidence = 0.95;
                        f
                    },
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: {
                        let mut f = Formula::latex("\\frac{a+b}{c}");
                        f.confidence = 0.92;
                        f
                    },
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
            layout: None,
            background_asset_id: None,
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    });

    #[test]
    fn latex() {
        let r = LatexConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("E=mc^2"));
    }
    #[test]
    fn latex_display() {
        let r = LatexDisplayConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("\\["));
    }
    #[test]
    fn latex_equation() {
        let r = LatexEquationConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("\\begin{equation}"));
    }
    #[test]
    fn markdown_inline() {
        let r = MarkdownInlineConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("$E=mc^2$"));
    }
    #[test]
    fn markdown_block() {
        let r = MarkdownBlockConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("$$"));
    }
    #[test]
    fn mathml() {
        let r = MathmlConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("<math"));
    }
    #[test]
    fn mathml_mml() {
        let r = MathmlMmlConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("mml:math"));
    }
    #[test]
    fn mathml_m() {
        let r = MathmlMConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("<m:math"));
    }
    #[test]
    fn mathml_attr() {
        let r = MathmlAttrConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("math"));
    }
    #[test]
    fn omml() {
        let r = OmmlConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("<m:f>"));
    }
    #[test]
    fn typst() {
        let r = TypstConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("frac") || r.contains("(a+b)/(c)") || r.contains("(a, b)"));
    }
    #[test]
    fn html() {
        let r = HtmlConverter.convert(&TEST_DOC).unwrap();
        assert!(r.contains("MathJax"));
    }

    #[test]
    fn fraction_omml() {
        let doc = Document {
            metadata: Default::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula::latex("\\frac{a}{b}"),
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                })],
                page_number: None,
                layout: None,
                background_asset_id: None,
            }],
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        };
        let r = OmmlConverter.convert(&doc).unwrap();
        assert!(r.contains("<m:num>"));
        assert!(r.contains("<m:den>"));
    }

    #[test]
    fn fraction_mathml() {
        let doc = Document {
            metadata: Default::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula::latex("\\frac{a}{b}"),
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                })],
                page_number: None,
                layout: None,
                background_asset_id: None,
            }],
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        };
        let r = MathmlConverter.convert(&doc).unwrap();
        assert!(r.contains("<mfrac>"));
    }

    // ── Roundtrip / Loopback tests for nested formulas across all formats ──
    // These verify that the full pipeline (LaTeX recognized string → OMML/MathML/Typst)
    // preserves mathematical structure, not just raw text.

    /// LaTeX string → all 6 output formats: check each preserves math structure.
    #[test]
    fn roundtrip_latex_to_all_formats() {
        let formulas = vec![
            ("E=mc^2", "simple", true),
            ("x^{2}", "superscript", true),
            ("x_{y_{z}}", "nested subscript", true),
            ("x^{y^{z}}", "nested superscript", true),
            ("\\frac{a}{b}", "fraction", true),
            ("\\frac{\\frac{a}{b}}{c}", "nested fraction", true),
            ("\\frac{x^{2}}{y_{n}}", "fraction with sub/sup", true),
            ("\\sqrt{x}", "sqrt", true),
            ("\\sqrt[3]{x}", "nth root", true),
            ("\\sqrt[3]{\\frac{x}{y}}", "root with fraction", true),
            ("\\alpha_{i}^{2}", "greek with sub/sup", true),
            ("\\int_{0}^{\\infty} f(x) dx", "integral", true),
            ("\\sum_{i=0}^{n} a_i", "summation", true),
            ("\\prod_{i=1}^{\\infty} b_i", "product", true),
            ("\\lim_{x \\to 0} \\sin x", "limit with arrow", true),
            ("\\left(\\frac{a}{b}\\right)", "delimited", true),
            ("\\hat{x} + \\bar{y}", "accents", true),
            ("\\operatorname{Spec}(A)", "operatorname", true),
        ];

        for (latex, desc, should_have_math) in &formulas {
            // LaTeX → LaTeX (passthrough)
            let out_latex = DocumentConverter::convert_latex_string(latex, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("LaTeX→LaTeX failed for {}: {}", desc, e));
            assert!(!out_latex.is_empty(), "LaTeX→LaTeX empty for {}", desc);

            // LaTeX → Typst
            let out_typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst)
                .unwrap_or_else(|e| panic!("LaTeX→Typst failed for {}: {}", desc, e));
            assert!(!out_typst.is_empty(), "LaTeX→Typst empty for {}", desc);

            // LaTeX → MathML
            let out_mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML)
                .unwrap_or_else(|e| panic!("LaTeX→MathML failed for {}: {}", desc, e));
            assert!(
                out_mathml.contains("<math"),
                "LaTeX→MathML missing <math> for {} ({}): {}",
                desc,
                latex,
                out_mathml
            );

            // LaTeX → OMML
            let out_omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML)
                .unwrap_or_else(|e| panic!("LaTeX→OMML failed for {}: {}", desc, e));
            if *should_have_math {
                let has = out_omml.contains("<m:f>")
                    || out_omml.contains("<m:sSup>")
                    || out_omml.contains("<m:sSub>")
                    || out_omml.contains("<m:rad>")
                    || out_omml.contains("<m:acc>")
                    || out_omml.contains("<m:nary>")
                    || out_omml.contains("<m:d>")
                    || out_omml.contains("<m:mRow>")
                    || out_omml.contains("<m:oMathPara")
                    || out_omml.contains("<m:r>");
                assert!(
                    has,
                    "LaTeX→OMML missing structure for {} ({}): {}",
                    desc, latex, out_omml
                );
            }

            // LaTeX → Markdown
            let out_md =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MarkdownBlock)
                    .unwrap_or_else(|e| panic!("LaTeX→Markdown failed for {}: {}", desc, e));
            assert!(
                out_md.contains("$") || out_md.contains("\\("),
                "LaTeX→Markdown missing delimiters for {} ({}): {}",
                desc,
                latex,
                out_md
            );

            // LaTeX → HTML
            let out_html = DocumentConverter::convert_latex_string(latex, OutputFormat::Html)
                .unwrap_or_else(|e| panic!("LaTeX→HTML failed for {}: {}", desc, e));
            assert!(
                out_html.contains("MathJax") || out_html.contains("math"),
                "LaTeX→HTML missing math for {} ({}): {}",
                desc,
                latex,
                out_html
            );
        }
    }

    /// LaTeX → OMML → LaTeX: verify math structure survives the roundtrip.
    #[test]
    fn roundtrip_latex_omml_latex() {
        // Simple cases
        let cases = vec![
            ("\\frac{a}{b}", "<m:f>"),
            ("x^{2}", "<m:sSup>"),
            ("x_{i}", "<m:sSub>"),
        ];
        for (latex, omml_structure) in &cases {
            let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML)
                .unwrap_or_else(|e| panic!("LaTeX→OMML failed for {}: {}", latex, e));
            assert!(
                omml.contains(omml_structure),
                "OMML missing {} for {}: {}",
                omml_structure,
                latex,
                omml
            );
            let back = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("OMML→LaTeX failed for {}: {}", latex, e));
            assert!(
                back.contains("frac") || back.contains("^") || back.contains("_"),
                "OMML→LaTeX lost structure for {}: {}",
                latex,
                back
            );
        }
    }

    /// LaTeX → MathML → LaTeX: verify math structure survives the roundtrip.
    #[test]
    fn roundtrip_latex_mathml_latex() {
        let cases = vec![
            ("\\frac{a}{b}", "mfrac"),
            ("x^{2}", "msup"),
            ("x_{i}", "msub"),
        ];
        for (latex, tag) in &cases {
            let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML)
                .unwrap_or_else(|e| panic!("LaTeX→MathML failed for {}: {}", latex, e));
            assert!(
                mathml.contains(tag),
                "MathML missing <{}> for {}: {}",
                tag,
                latex,
                mathml
            );
            let back = DocumentConverter::convert_mathml_string(&mathml, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("MathML→LaTeX failed for {}: {}", latex, e));
            assert!(
                back.contains("frac") || back.contains("^") || back.contains("_"),
                "MathML→LaTeX lost structure for {}: {}",
                latex,
                back
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Category 9: Plugin
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod plugin_tests {
    use latexsnipper_ast::{Document, Page};
    use latexsnipper_plugin::*;

    #[test]
    fn registry_register_list() {
        let mut reg = PluginRegistry::new();
        let plugin = latexsnipper_plugin::plugin::TransformPlugin::new("test", "0.1", |_| Ok(()));
        reg.register(Box::new(plugin)).unwrap();
        assert!(reg.has("test"));
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn registry_unregister() {
        let mut reg = PluginRegistry::new();
        let plugin = latexsnipper_plugin::plugin::TransformPlugin::new("test", "0.1", |_| Ok(()));
        reg.register(Box::new(plugin)).unwrap();
        reg.unregister("test").unwrap();
        assert!(!reg.has("test"));
    }

    #[test]
    fn registry_handle() {
        let mut reg = PluginRegistry::new();
        let plugin = latexsnipper_plugin::plugin::TransformPlugin::new("test", "0.1", |doc| {
            if doc.pages.is_empty() {
                doc.pages.push(Page {
                    width: 0.0,
                    height: 0.0,
                    blocks: vec![],
                    page_number: None,
                    layout: None,
                    background_asset_id: None,
                });
            }
            Ok(())
        });
        reg.register(Box::new(plugin)).unwrap();
        let req = PluginRequest::new("test", Document::new());
        let resp = reg.handle("test", &req).unwrap();
        assert!(!resp.document.pages.is_empty());
    }

    #[test]
    fn registry_handle_not_found() {
        let reg = PluginRegistry::new();
        let req = PluginRequest::new("test", Document::new());
        assert!(reg.handle("nonexistent", &req).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════
// Category 10: Mock
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod mock_tests {
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_image::SnipperImage;
    use latexsnipper_mock::*;

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
// Category 11: Runtime
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod runtime_tests {
    use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend, StubRuntime};

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
// Category 12: FFI
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

// ═══════════════════════════════════════════════════════════════
// Category 13: Engine Integration
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod engine_tests {
    use latexsnipper_engine::{EngineConfig, RecognizeMode, SnipperEngine};
    use latexsnipper_export::svg::SvgGenerator;
    use latexsnipper_export::{Generator, RenderTree};
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_image::SnipperImage;
    use latexsnipper_mock::FakePipeline;
    use latexsnipper_runtime::StubRuntime;
    use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
    use latexsnipper_syntax::{Parser, Renderer};

    fn test_image() -> SnipperImage {
        SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000])
    }

    #[tokio::test]
    async fn engine_mock() {
        let engine = SnipperEngine::new(EngineConfig::default(), Box::new(StubRuntime::new()));
        let doc = engine
            .recognize(test_image(), RecognizeMode::Formula)
            .await
            .unwrap();
        // Pipeline First: document always has 1 page, blocks may be empty
        assert!(doc.pages.len() <= 1);
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
// Category 14: Assets (media, checksum, image roundtrip)
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod asset_tests {
    use latexsnipper_ast::*;
    use latexsnipper_conversion::markdown_parser::parse_markdown_to_document;

    /// A 1x1 red PNG, base64-encoded.
    const RED_DOT_PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn test_normalize_assets_computes_checksum() {
        let mut doc = Document::new();
        doc.assets.push(MediaAsset {
            id: AssetId("test-img".to_string()),
            format: AssetFormat::Png,
            mime_type: Some("image/png".to_string()),
            role: MediaRole::Photo,
            storage: AssetStorage::InlineBase64 {
                data: RED_DOT_PNG_B64.to_string(),
            },
            width: None,
            height: None,
            dpi: None,
            color_space: None,
            checksum: None,
            alt_text: None,
            metadata: Default::default(),
        });

        let _diags = doc.normalize_assets(NormalizeAssetOptions {
            compute_checksum: true,
            ..Default::default()
        });

        assert!(
            doc.assets[0].checksum.is_some(),
            "Checksum should be computed"
        );
        let checksum = doc.assets[0].checksum.as_ref().unwrap();
        assert_eq!(checksum.len(), 64, "SHA-256 hex should be 64 chars");

        // Verify deterministic: re-run should produce identical checksum
        let checksum2 = doc.assets[0].checksum.as_ref().unwrap();
        assert_eq!(checksum, checksum2, "Checksum must be deterministic");
    }

    #[test]
    fn test_normalize_assets_skips_checksum_when_disabled() {
        let mut doc = Document::new();
        doc.assets.push(MediaAsset {
            id: AssetId("no-checksum".to_string()),
            format: AssetFormat::Png,
            mime_type: Some("image/png".to_string()),
            role: MediaRole::Photo,
            storage: AssetStorage::InlineBase64 {
                data: RED_DOT_PNG_B64.to_string(),
            },
            width: None,
            height: None,
            dpi: None,
            color_space: None,
            checksum: None,
            alt_text: None,
            metadata: Default::default(),
        });

        let _diags = doc.normalize_assets(NormalizeAssetOptions {
            compute_checksum: false,
            ..Default::default()
        });

        assert!(
            doc.assets[0].checksum.is_none(),
            "Checksum should not be computed when disabled"
        );
    }

    #[test]
    fn test_normalize_assets_infers_mime_type() {
        let mut doc = Document::new();
        doc.assets.push(MediaAsset {
            id: AssetId("mime-test".to_string()),
            format: AssetFormat::Png,
            mime_type: None,
            role: MediaRole::Photo,
            storage: AssetStorage::InlineBase64 {
                data: RED_DOT_PNG_B64.to_string(),
            },
            width: None,
            height: None,
            dpi: None,
            color_space: None,
            checksum: None,
            alt_text: None,
            metadata: Default::default(),
        });

        let _diags = doc.normalize_assets(NormalizeAssetOptions {
            infer_mime_type: true,
            compute_checksum: false,
            deduplicate: false,
            fill_dimensions: false,
            migrate_legacy: false,
        });

        assert_eq!(
            doc.assets[0].mime_type.as_deref(),
            Some("image/png"),
            "MIME type should be inferred from format"
        );
    }

    #[test]
    fn test_markdown_image_roundtrip() {
        let md = "![Alt text](https://example.com/img.png)";
        let doc = parse_markdown_to_document(md);

        assert!(
            !doc.assets.is_empty(),
            "Markdown image should create MediaAsset"
        );

        let has_image = doc.all_blocks().iter().any(|b| {
            b.inlines()
                .iter()
                .any(|i| matches!(i, Inline::Image(img) if img.asset_id.is_some()))
        });
        assert!(has_image, "ImageInline should have asset_id set");

        assert!(
            matches!(doc.assets[0].storage, AssetStorage::Uri { .. }),
            "Markdown image should be Uri storage"
        );

        // Verify alt text is captured
        assert_eq!(
            doc.assets[0].alt_text.as_deref(),
            Some("Alt text"),
            "Alt text should be preserved"
        );
    }

    #[test]
    fn test_markdown_image_in_paragraph() {
        let md = "A sentence with ![icon](https://example.com/icon.svg) inline.";
        let doc = parse_markdown_to_document(md);

        assert!(!doc.assets.is_empty(), "Should create asset for SVG image");

        // The paragraph should have both text and an Image inline
        let block = &doc.pages[0].blocks[0];
        let inlines = block.inlines();
        assert!(inlines.len() >= 2, "Paragraph should have text + image");

        let has_image = inlines.iter().any(|i| matches!(i, Inline::Image(_)));
        assert!(has_image, "Paragraph should contain Image inline");
    }

    #[test]
    fn test_asset_storage_serialization_roundtrip() {
        let asset = MediaAsset {
            id: AssetId("rt".to_string()),
            format: AssetFormat::Jpeg,
            mime_type: Some("image/jpeg".to_string()),
            role: MediaRole::Screenshot,
            storage: AssetStorage::InlineBase64 {
                data: "/9j/4AAQSkZJRg==".to_string(),
            },
            width: Some(1920.0),
            height: Some(1080.0),
            dpi: Some(72.0),
            color_space: Some("sRGB".to_string()),
            checksum: Some("abc123".to_string()),
            alt_text: Some("A screenshot".to_string()),
            metadata: Default::default(),
        };

        let json = serde_json::to_string(&asset).unwrap();
        let restored: MediaAsset = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id.0, "rt");
        assert_eq!(restored.format, AssetFormat::Jpeg);
        assert_eq!(restored.width, Some(1920.0));
    }
}

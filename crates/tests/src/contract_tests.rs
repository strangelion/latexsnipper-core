use latexsnipper_ast::{
    ArtifactKind, ArtifactManifest, Block, Document, DocumentVisitor, Formula, FormulaBlock,
    Inline, JobRoot, Page, ParagraphBlock, Rect, RetryPolicy, StageInput, StageKind, StageOutput,
    StageRunner, StageSpec, StageStatus, TextCollector, TextRun,
};
use latexsnipper_conversion::{Converter, MathmlConverter, TypstConverter};
use latexsnipper_engine::stage_runners::{
    register_default_runners, ConvertStage, ExportStage, RecognizeStage, StageOrchestrator,
};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_pipeline::{PipelineContext, PipelineGraph, TransformNode};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend, StubRuntime};
use latexsnipper_tensor::Tensor;
use std::sync::{Arc, Mutex};

fn formula_doc(latex: &str) -> Document {
    Document {
        metadata: Default::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Formula(FormulaBlock {
                formula: Formula::latex(latex),
                label: None,
                number: None,
                environment: None,
                geometry: Some(Rect::new(1.0, 2.0, 3.0, 4.0)),
                source: None,
            })],
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
    }
}

#[test]
fn ast_document_roundtrip_preserves_public_shape() {
    let mut doc = Document::new();
    doc.pages.push(Page {
        width: 320.0,
        height: 240.0,
        page_number: Some(7),
        layout: None,
        background_asset_id: None,
        blocks: vec![Block::Paragraph(ParagraphBlock {
            inlines: vec![
                Inline::Text(TextRun::new("Area: ")),
                Inline::Formula(Formula::latex("x^2")),
            ],
            geometry: Some(Rect::new(10.0, 20.0, 30.0, 40.0)),
            source: None,
            style: None,
        })],
    });

    let json = serde_json::to_string(&doc).unwrap();
    let restored: Document = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.pages.len(), 1);
    assert_eq!(restored.block_count(), 1);
    assert_eq!(restored.pages[0].page_number, Some(7));
    assert_eq!(restored.pages[0].blocks[0].geometry().unwrap().width, 30.0);
}

#[test]
fn ast_text_collector_visits_nested_inline_formula() {
    let doc = Document {
        metadata: Default::default(),
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            page_number: None,
            layout: None,
            background_asset_id: None,
            blocks: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![
                    Inline::Text(TextRun::new("f=")),
                    Inline::Formula(Formula::latex("x+1")),
                ],
                geometry: None,
                source: None,
                style: None,
            })],
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    };

    let mut collector = TextCollector::new();
    collector.visit_document(&doc);
    assert_eq!(collector.text.trim(), "f=x+1");
}

#[test]
fn conversion_handles_complex_latex_environments() {
    let cases = formula_doc(r"\begin{cases}x&x>0\\-x&x<0\end{cases}");
    let matrix = formula_doc(r"\begin{pmatrix}a&b\\c&d\end{pmatrix}");
    let aligned = formula_doc(r"\begin{aligned}a&=b+c\\d&=e+f\end{aligned}");
    let phantom = formula_doc(r"\phantom{x}");

    assert!(MathmlConverter
        .convert(&cases)
        .unwrap()
        .contains("<mtable>"));
    assert!(MathmlConverter
        .convert(&matrix)
        .unwrap()
        .contains("<mtable>"));
    assert!(MathmlConverter
        .convert(&aligned)
        .unwrap()
        .contains("<mtable>"));
    assert!(MathmlConverter
        .convert(&phantom)
        .unwrap()
        .contains("<mpadded"));
    assert!(TypstConverter.convert(&matrix).unwrap().contains("mat("));
}

#[tokio::test]
async fn pipeline_graph_executes_dependencies_before_dependents() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut graph = PipelineGraph::new("contract");

    for name in ["b", "a", "c"] {
        let order = Arc::clone(&order);
        graph.add_node_with_deps(
            Box::new(TransformNode::new(name, move |_| {
                order.lock().unwrap().push(name.to_string());
                Ok(())
            })),
            match name {
                "b" => vec!["a".to_string()],
                "c" => vec!["b".to_string()],
                _ => vec![],
            },
        );
    }

    graph.run(&mut PipelineContext::new()).await.unwrap();
    assert_eq!(*order.lock().unwrap(), vec!["a", "b", "c"]);
}

#[tokio::test]
async fn pipeline_graph_reports_missing_dependency() {
    let mut graph = PipelineGraph::new("broken");
    graph.add_node_with_deps(
        Box::new(TransformNode::new("node", |_| Ok(()))),
        vec!["missing".to_string()],
    );

    let err = graph.run(&mut PipelineContext::new()).await.unwrap_err();
    assert!(err.to_string().contains("unknown node"));
}

#[tokio::test]
async fn pipeline_graph_stops_after_node_error() {
    let mut graph = PipelineGraph::new("error");
    graph.add_node(Box::new(TransformNode::new("fail", |_| {
        Err(SnipperError::Pipeline("expected failure".into()))
    })));

    let err: Result<()> = graph.run(&mut PipelineContext::new()).await;
    assert!(err.unwrap_err().to_string().contains("expected failure"));
}

#[test]
fn runtime_stub_preserves_input_shape_for_mock_output() {
    let runtime = StubRuntime::new();
    let handle = ModelHandle::new("mock", "text-rec", "test");
    let session = runtime
        .create_session(&handle, AccelerationMode::Cpu)
        .unwrap();
    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.0; 3 * 48 * 320]);

    let outputs = session.run(&[input]).unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].name(), "x_output");
    assert_eq!(outputs[0].shape(), &[1, 3, 48, 320]);
}

#[test]
fn stage_orchestrator_produces_manifest_and_report() {
    let tmp = std::env::temp_dir().join(format!("test-stage-manifest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let job_root = JobRoot::new("manifest-test", tmp.to_string_lossy().to_string());
    let mut orchestrator = StageOrchestrator::new(job_root.clone());
    register_default_runners(&mut orchestrator);

    // Decode stage with no source — will fail but produce a valid report & manifest
    let spec = StageSpec {
        schema_version: "1.0.0".to_string(),
        job_id: "manifest-test".to_string(),
        stage_id: "decode-1".to_string(),
        kind: StageKind::Decode,
        input: StageInput {
            artifacts: vec![],
            source: None,
        },
        output: StageOutput {
            artifact_kind: "decoded_image".to_string(),
            subdir: "decoded".to_string(),
        },
        options: serde_json::json!({}),
        provider: None,
        credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };

    let report = orchestrator
        .run_stage(&spec)
        .expect("run_stage should not error even when stage fails");
    assert_eq!(report.stage_id, "decode-1");
    assert_eq!(report.kind, StageKind::Decode);
    assert_eq!(
        report.status,
        StageStatus::Failed,
        "Decode with no source should fail"
    );

    // Manifest JSON file should have been written
    let manifest_path = std::path::Path::new(&job_root.artifacts_dir).join("artifacts.json");
    assert!(
        manifest_path.exists(),
        "Artifact manifest should be written to disk"
    );

    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: ArtifactManifest = serde_json::from_str(&content).unwrap();
    assert_eq!(manifest.job_id, "manifest-test");
    assert_eq!(manifest.schema_version, "1.0.0");

    // Event log should have been written
    let event_path = std::path::Path::new(&job_root.logs_dir).join("events.jsonl");
    assert!(event_path.exists(), "Event log should be written to disk");

    // Stage report JSON should have been written
    let report_path = std::path::Path::new(&job_root.reports_dir).join("decode-1.report.json");
    assert!(
        report_path.exists(),
        "Stage report JSON should be written to disk"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn stage_orchestrator_runs_convert_stage_with_document_json() {
    let tmp = std::env::temp_dir().join(format!("test-stage-convert-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let job_root = JobRoot::new("convert-test", tmp.to_string_lossy().to_string());
    job_root.ensure_dirs().unwrap();

    // Write a minimal Document JSON as the input source
    let doc = Document {
        metadata: Default::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Formula(latexsnipper_ast::FormulaBlock {
                formula: latexsnipper_ast::Formula::latex("E=mc^2"),
                label: None,
                number: None,
                environment: None,
                geometry: None,
                source: None,
            })],
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
    };
    let source_path = format!("{}/doc.json", job_root.source_dir);
    let json = serde_json::to_string(&doc).unwrap();
    std::fs::write(&source_path, &json).unwrap();

    let mut orchestrator = StageOrchestrator::new(job_root.clone());
    register_default_runners(&mut orchestrator);

    let spec = StageSpec {
        schema_version: "1.0.0".to_string(),
        job_id: "convert-test".to_string(),
        stage_id: "convert-1".to_string(),
        kind: StageKind::Convert,
        input: StageInput {
            artifacts: vec!["convert:input".to_string()],
            source: Some(source_path.clone()),
        },
        output: StageOutput {
            artifact_kind: "converted_text".to_string(),
            subdir: "converted".to_string(),
        },
        options: serde_json::json!({"target_format": "latex"}),
        provider: None,
        credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };

    let report = orchestrator
        .run_stage(&spec)
        .expect("ConvertStage should succeed");
    assert_eq!(report.stage_id, "convert-1");
    assert_eq!(report.kind, StageKind::Convert);
    assert_eq!(
        report.status,
        StageStatus::Succeeded,
        "Convert stage should succeed with valid Document JSON"
    );

    // Verify the converted output was written
    assert!(
        !report.output_artifacts.is_empty(),
        "Should have at least one output artifact"
    );
    let output_path = &report.output_artifacts[0];
    assert!(
        std::path::Path::new(output_path).exists(),
        "Converted output file should exist"
    );
    let content = std::fs::read_to_string(output_path).unwrap();
    assert!(
        content.contains("E=mc^2"),
        "Converted output should contain the formula"
    );

    // Verify the artifact manifest was updated
    let manifest_path = std::path::Path::new(&job_root.artifacts_dir).join("artifacts.json");
    let manifest_content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: ArtifactManifest = serde_json::from_str(&manifest_content).unwrap();
    assert!(
        !manifest.artifacts.is_empty(),
        "Manifest should contain the produced artifact"
    );
    let entry = &manifest.artifacts[0];
    assert_eq!(
        entry.producer_stage_id.as_deref(),
        Some("convert-1"),
        "Artifact should record its producer stage"
    );
    assert!(
        entry.checksum_sha256.is_some(),
        "Artifact should have a checksum"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_full_stage_pipeline_e2e() {
    // Create a temporary job root
    let tmp = std::env::temp_dir().join(format!("stage_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let job_root = JobRoot::new("e2e-test", tmp.to_string_lossy().to_string());

    // Create a minimal Document and write as input AST
    std::fs::create_dir_all(&job_root.source_dir).unwrap();
    let mut doc = Document::new();
    doc.pages.push(Page {
        width: 800.0,
        height: 600.0,
        blocks: vec![
            Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new("Hello from E2E test"))],
                geometry: None,
                source: None,
                style: None,
            }),
            Block::Formula(FormulaBlock {
                formula: Formula::latex("E=mc^2"),
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
    });
    let doc_json = serde_json::to_string_pretty(&doc).unwrap();
    let source_path = format!("{}/document.ast.json", job_root.source_dir);
    std::fs::write(&source_path, &doc_json).unwrap();

    // ── RecognizeStage: passthrough ──
    let recognize_spec = StageSpec {
        schema_version: "2.0.0".to_string(),
        job_id: "e2e-test".to_string(),
        stage_id: "recognize-1".to_string(),
        kind: StageKind::Recognize,
        input: StageInput {
            artifacts: vec![],
            source: Some(source_path.clone()),
        },
        output: StageOutput {
            artifact_kind: "document_ast".to_string(),
            subdir: "ast".to_string(),
        },
        options: serde_json::json!({}),
        provider: None,
        credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };
    let report1 = RecognizeStage
        .run(&recognize_spec, &job_root)
        .expect("RecognizeStage should succeed");
    assert_eq!(report1.status, StageStatus::Succeeded);
    assert!(
        !report1.produced_artifacts.is_empty(),
        "RecognizeStage should produce artifacts"
    );
    assert!(report1.produced_artifacts[0]
        .path
        .contains("document.ast.json"));
    assert!(
        std::path::Path::new(&report1.produced_artifacts[0].path).exists(),
        "Path should exist"
    );

    let ast_path = report1.produced_artifacts[0].path.clone();

    // ── ConvertStage: AST → Markdown ──
    let convert_spec = StageSpec {
        schema_version: "2.0.0".to_string(),
        job_id: "e2e-test".to_string(),
        stage_id: "convert-1".to_string(),
        kind: StageKind::Convert,
        input: StageInput {
            artifacts: vec!["e2e-test:ast".to_string()],
            source: Some(ast_path.clone()),
        },
        output: StageOutput {
            artifact_kind: "converted_text".to_string(),
            subdir: "converted".to_string(),
        },
        options: serde_json::json!({"target_format": "markdown"}),
        provider: None,
        credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };
    let report2 = ConvertStage
        .run(&convert_spec, &job_root)
        .expect("ConvertStage should succeed");
    assert_eq!(report2.status, StageStatus::Succeeded);
    assert!(
        !report2.produced_artifacts.is_empty(),
        "ConvertStage should produce artifacts"
    );
    let converted_path = &report2.produced_artifacts[0].path;
    assert!(std::path::Path::new(converted_path).exists());

    // ── ExportStage: AST → PlainText ──
    let export_spec = StageSpec {
        schema_version: "2.0.0".to_string(),
        job_id: "e2e-test".to_string(),
        stage_id: "export-1".to_string(),
        kind: StageKind::Export,
        input: StageInput {
            artifacts: vec!["e2e-test:ast".to_string()],
            source: Some(ast_path),
        },
        output: StageOutput {
            artifact_kind: "exported_file".to_string(),
            subdir: "exported".to_string(),
        },
        options: serde_json::json!({"visual_format": "txt"}),
        provider: None,
        credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };
    let report3 = ExportStage
        .run(&export_spec, &job_root)
        .expect("ExportStage should succeed");
    assert_eq!(report3.status, StageStatus::Succeeded);
    assert!(
        !report3.produced_artifacts.is_empty(),
        "ExportStage should produce artifacts"
    );
    let export_path = &report3.produced_artifacts[0].path;
    assert!(std::path::Path::new(export_path).exists());

    // ── Verify content ──
    let converted_text = std::fs::read_to_string(converted_path).unwrap_or_default();
    assert!(
        converted_text.contains("Hello from E2E test"),
        "Converted text should contain original content"
    );

    // ── Cleanup ──
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_produced_artifact_metadata() {
    // Run a ConvertStage and verify produced_artifact metadata
    let tmp = std::env::temp_dir().join(format!("artifact_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let job_root = JobRoot::new("artifact-test", tmp.to_string_lossy().to_string());

    let mut doc = Document::new();
    doc.pages.push(Page {
        width: 800.0, height: 600.0,
        blocks: vec![Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun::new("Artifact test"))],
            geometry: None, source: None, style: None,
        })],
        page_number: Some(1), layout: None, background_asset_id: None,
    });
    let json = serde_json::to_string_pretty(&doc).unwrap();
    std::fs::create_dir_all(&job_root.source_dir).unwrap();
    let src = format!("{}/doc.json", job_root.source_dir);
    std::fs::write(&src, &json).unwrap();

    let spec = StageSpec {
        schema_version: "2.0.0".to_string(),
        job_id: "artifact-test".to_string(),
        stage_id: "convert-art".to_string(),
        kind: StageKind::Convert,
        input: StageInput { artifacts: vec![], source: Some(src) },
        output: StageOutput { artifact_kind: "converted_text".to_string(), subdir: "converted".to_string() },
        options: serde_json::json!({"target_format": "latex"}),
        provider: None, credentials: Vec::new(),
        retry: RetryPolicy::default(),
    };

    let report = ConvertStage.run(&spec, &job_root).expect("ConvertStage");
    assert!(!report.produced_artifacts.is_empty());

    let art = &report.produced_artifacts[0];
    assert!(std::path::Path::new(&art.path).exists(), "path must exist");
    assert!(art.size_bytes.unwrap_or(0) > 0, "size_bytes must be > 0");
    assert!(art.checksum_sha256.as_ref().map_or(false, |c| c.len() == 64), "checksum must be 64-char SHA-256 hex");
    assert!(art.mime_type.is_some(), "mime_type should be set");
    assert_eq!(art.kind, ArtifactKind::ConvertedText, "ArtifactKind should match");

    let _ = std::fs::remove_dir_all(&tmp);
}

use latexsnipper_ast::{
    Block, Document, DocumentVisitor, Formula, FormulaBlock, Inline, Page, ParagraphBlock, Rect,
    TextCollector, TextRun,
};
use latexsnipper_conversion::{Converter, MathmlConverter, TypstConverter};
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
                geometry: Some(Rect::new(1.0, 2.0, 3.0, 4.0)),
                source: None,
            })],
            page_number: Some(1),
        }],
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
    }
}

#[test]
fn ast_document_roundtrip_preserves_public_shape() {
    let mut doc = Document::new();
    doc.pages.push(Page {
        width: 320.0,
        height: 240.0,
        page_number: Some(7),
        blocks: vec![Block::Paragraph(ParagraphBlock {
            inlines: vec![
                Inline::Text(TextRun::new("Area: ")),
                Inline::Formula(Formula::latex("x^2")),
            ],
            geometry: Some(Rect::new(10.0, 20.0, 30.0, 40.0)),
            source: None,
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
            blocks: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![
                    Inline::Text(TextRun::new("f=")),
                    Inline::Formula(Formula::latex("x+1")),
                ],
                geometry: None,
                source: None,
            })],
        }],
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
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
    assert!(TypstConverter.convert(&matrix).unwrap().contains("matrix"));
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
    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.0; 1 * 3 * 48 * 320]);

    let outputs = session.run(&[input]).unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].name(), "x_output");
    assert_eq!(outputs[0].shape(), &[1, 3, 48, 320]);
}

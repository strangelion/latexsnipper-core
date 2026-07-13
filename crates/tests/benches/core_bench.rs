use latexsnipper_ast::{
    Block, Document, DocumentVisitor, ExportFormat, Formula, FormulaBlock, Page, TextCollector,
};
use latexsnipper_conversion::{
    read_docx_bytes, read_pptx_bytes, read_xlsx_bytes, Converter, DocumentExportService,
    MathmlConverter, OmmlConverter, TypstConverter,
};
use latexsnipper_pipeline::{PipelineContext, PipelineGraph, TransformNode};
use latexsnipper_plugin::{PluginRegistry, PluginRequest, TransformPlugin};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn formulas_doc(count: usize, latex: &str) -> Document {
    let blocks = (0..count)
        .map(|_| {
            Block::Formula(FormulaBlock {
                formula: Formula::latex(latex),
                label: None,
                number: None,
                environment: None,
                geometry: None,
                source: None,
            })
        })
        .collect();

    let mut page = Page::new(800.0, 600.0, 1);
    page.blocks = blocks;

    let mut document = Document::new();
    document.add_page(page);
    document
}

fn run_bench(name: &str, iterations: usize, mut f: impl FnMut()) {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    report(name, iterations, start.elapsed());
}

fn report(name: &str, iterations: usize, elapsed: Duration) {
    let per_iter = elapsed.as_nanos() / iterations as u128;
    println!("bench {name}: {iterations} iterations, {elapsed:?}, {per_iter} ns/iter");
    println!(
        "benchmark_json={}",
        serde_json::json!({
            "name": name,
            "iterations": iterations,
            "elapsedNanos": elapsed.as_nanos(),
            "nanosPerIteration": per_iter,
        })
    );
}

fn bench_ast_visitor() {
    let doc = formulas_doc(256, r"\frac{a+b}{c+d}");
    run_bench("ast_text_collector_256_formula_blocks", 2_000, || {
        let mut collector = TextCollector::new();
        collector.visit_document(black_box(&doc));
        black_box(collector.text.len());
    });
}

fn bench_conversion() {
    let doc = formulas_doc(64, r"\begin{cases}x&x>0\\-x&x<0\end{cases}");
    run_bench("conversion_mathml_cases_64", 500, || {
        black_box(MathmlConverter.convert(black_box(&doc)).unwrap());
    });
    run_bench("conversion_omml_cases_64", 500, || {
        black_box(OmmlConverter.convert(black_box(&doc)).unwrap());
    });
    run_bench("conversion_typst_cases_64", 500, || {
        black_box(TypstConverter.convert(black_box(&doc)).unwrap());
    });
}

fn bench_pipeline() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut graph = PipelineGraph::new("bench");
    for index in 0..8 {
        let name = format!("node_{index}");
        let deps = if index == 0 {
            vec![]
        } else {
            vec![format!("node_{}", index - 1)]
        };
        graph.add_node_with_deps(
            Box::new(TransformNode::new(name, move |ctx| {
                ctx.set(format!("step_{index}"), serde_json::json!(index));
                Ok(())
            })),
            deps,
        );
    }

    run_bench("pipeline_graph_8_transform_nodes", 1_000, || {
        runtime.block_on(async {
            let mut ctx = PipelineContext::new();
            graph.run(black_box(&mut ctx)).await.unwrap();
            black_box(ctx.metadata.len());
        })
    });
}

fn bench_visual_and_package_exports() {
    let document = formulas_doc(4, r"\frac{a+b}{c+d}");
    for (name, format, iterations) in [
        ("export_svg", ExportFormat::Svg, 20),
        ("export_png", ExportFormat::Png, 5),
        ("export_pdf", ExportFormat::Pdf, 20),
        ("export_docx", ExportFormat::Docx, 20),
        ("export_pptx", ExportFormat::Pptx, 20),
        ("export_xlsx", ExportFormat::Xlsx, 20),
    ] {
        run_bench(name, iterations, || {
            let artifact = DocumentExportService::export(black_box(&document), format).unwrap();
            black_box(artifact.as_bytes().map_or(0, <[u8]>::len));
        });
    }
}

fn bench_office_imports() {
    let document = formulas_doc(4, r"x^2+y^2");
    let docx = DocumentExportService::export(&document, ExportFormat::Docx).unwrap();
    let pptx = DocumentExportService::export(&document, ExportFormat::Pptx).unwrap();
    let xlsx = DocumentExportService::export(&document, ExportFormat::Xlsx).unwrap();
    run_bench("import_docx", 50, || {
        black_box(read_docx_bytes(docx.as_bytes().unwrap()).unwrap());
    });
    run_bench("import_pptx", 50, || {
        black_box(read_pptx_bytes(pptx.as_bytes().unwrap()).unwrap());
    });
    run_bench("import_xlsx", 50, || {
        black_box(read_xlsx_bytes(xlsx.as_bytes().unwrap()).unwrap());
    });
}

fn bench_plugin_chain() {
    let mut registry = PluginRegistry::new();
    for index in 0..8 {
        registry
            .register(Box::new(TransformPlugin::new(
                format!("plugin-{index}"),
                "1.0.0",
                |_| Ok(()),
            )))
            .unwrap();
    }
    let request = PluginRequest::new("after_export", formulas_doc(16, "a+b"));
    run_bench("plugin_chain_8", 1_000, || {
        black_box(registry.handle_all(black_box(&request)).unwrap());
    });
}

fn main() {
    bench_ast_visitor();
    bench_conversion();
    bench_pipeline();
    bench_visual_and_package_exports();
    bench_office_imports();
    bench_plugin_chain();
}

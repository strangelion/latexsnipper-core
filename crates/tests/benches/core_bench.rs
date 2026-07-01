use latexsnipper_ast::{
    Block, Document, DocumentVisitor, Formula, FormulaBlock, Page, TextCollector,
};
use latexsnipper_conversion::{Converter, MathmlConverter, OmmlConverter, TypstConverter};
use latexsnipper_pipeline::{PipelineContext, PipelineGraph, TransformNode};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn formulas_doc(count: usize, latex: &str) -> Document {
    let blocks = (0..count)
        .map(|_| {
            Block::Formula(FormulaBlock {
                formula: Formula::latex(latex),
                geometry: None,
                source: None,
            })
        })
        .collect();

    Document {
        metadata: Default::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks,
            page_number: Some(1),
        }],
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
    }
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

fn main() {
    bench_ast_visitor();
    bench_conversion();
    bench_pipeline();
}

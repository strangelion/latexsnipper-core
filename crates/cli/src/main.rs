use clap::{Parser, Subcommand};
use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::StubRuntime;
use latexsnipper_mock::FakePipeline;
use latexsnipper_image::SnipperImage;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_export::{RenderTree, Generator};
use latexsnipper_export::svg::SvgGenerator;

#[derive(Parser)]
#[command(name = "snipper")]
#[command(about = "LaTeXSnipper Core CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Recognize content in an image
    Recognize {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "formula")]
        mode: String,
        #[arg(long, default_value_t = true)]
        mock: bool,
    },

    /// Parse LaTeX string to AST
    Parse {
        #[arg(short, long)]
        latex: String,
    },

    /// Render AST to LaTeX
    Render {
        #[arg(short, long)]
        latex: String,
    },

    /// Show version info
    Version,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Recognize { input, mode, mock } => {
            println!("Input: {}", input);
            println!("Mode: {}", mode);
            println!("Mock: {}", mock);

            if mock {
                let pipeline = match mode.as_str() {
                    "formula" => FakePipeline::formula("\\frac{a+b}{c}", 0.95),
                    "text" => FakePipeline::text("Hello World", 0.92),
                    "mixed" => FakePipeline::mixed("E=mc^2", "Given the equation", 0.9),
                    _ => { eprintln!("Unknown mode: {}", mode); std::process::exit(1); }
                };

                let image = SnipperImage::new(100, 100, PixelFormat::Rgb, vec![128u8; 30000]);
                let doc = pipeline.run(&image).expect("Pipeline failed");

                println!("\nResult:");
                for (i, page) in doc.pages.iter().enumerate() {
                    println!("Page {}: {} blocks", i + 1, page.blocks.len());
                    for (j, block) in page.blocks.iter().enumerate() {
                        match block {
                            latexsnipper_ast::Block::Formula(f) => {
                                println!("  Block {}: Formula: {}", j + 1, f.formula.as_latex());
                            }
                            latexsnipper_ast::Block::Paragraph(p) => {
                                let text: String = p.inlines.iter().filter_map(|l| {
                                    if let latexsnipper_ast::Inline::Text(t) = l { Some(t.text.as_str()) } else { None }
                                }).collect();
                                println!("  Block {}: Text: {}", j + 1, text);
                            }
                            _ => println!("  Block {}: Other", j + 1),
                        }
                    }
                }

                let json = serde_json::to_string_pretty(&doc).expect("JSON failed");
                println!("\nJSON ({} bytes):", json.len());
            } else {
                eprintln!("Real model mode not implemented yet");
            }
        }

        Commands::Parse { latex } => {
            let parser = LatexParser;
            let doc = parser.parse(&latex).expect("Parse failed");
            println!("Parsed: {} blocks", doc.block_count());
            let json = serde_json::to_string_pretty(&doc).expect("JSON failed");
            println!("{}", json);
        }

        Commands::Render { latex } => {
            let parser = LatexParser;
            let renderer = LatexRenderer;
            let doc = parser.parse(&latex).expect("Parse failed");
            let output = renderer.render(&doc).expect("Render failed");
            println!("{}", output);
        }

        Commands::Version => {
            println!("snipper {}", env!("CARGO_PKG_VERSION"));
            println!("LaTeXSnipper Core — Mock Runtime Mode");
        }
    }
}

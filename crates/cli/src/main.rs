use clap::{Parser, Subcommand};
use latexsnipper_pipeline::sdk::Snipper;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};

#[derive(Parser)]
#[command(name = "snipper")]
#[command(about = "LaTeXSnipper Core CLI — Image to LaTeX/Markdown/Typst")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Recognize formulas in an image and export to format
    Recognize {
        /// Input image path
        #[arg(short, long)]
        input: String,

        /// Output format: latex, markdown, typst, html, json
        #[arg(short, long, default_value = "latex")]
        format: String,
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

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Recognize { input, format } => {
            eprintln!("Processing: {}", input);

            let snipper = match Snipper::from_file(&input) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            eprintln!("Detected {} formulas", snipper.document().block_count());

            let output = match format.as_str() {
                "latex" | "tex" => snipper.to_latex(),
                "markdown" | "md" => snipper.to_markdown(),
                "typst" => snipper.to_typst(),
                "html" => snipper.to_html(),
                "mathml" => snipper.to_mathml(),
                "omml" => snipper.to_omml(),
                "json" => snipper.to_json(),
                _ => {
                    eprintln!("Unknown format: {}. Use: latex, markdown, typst, html, json", format);
                    std::process::exit(1);
                }
            };

            match output {
                Ok(text) => println!("{}", text),
                Err(e) => {
                    eprintln!("Export error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Parse { latex } => {
            let parser = LatexParser;
            match parser.parse(&latex) {
                Ok(doc) => {
                    println!("Parsed: {} blocks", doc.block_count());
                    let json = serde_json::to_string_pretty(&doc).expect("JSON failed");
                    println!("{}", json);
                }
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Render { latex } => {
            let parser = LatexParser;
            let renderer = LatexRenderer;
            match parser.parse(&latex) {
                Ok(doc) => match renderer.render(&doc) {
                    Ok(output) => println!("{}", output),
                    Err(e) => {
                        eprintln!("Render error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Version => {
            println!("snipper {}", env!("CARGO_PKG_VERSION"));
            println!("LaTeXSnipper Core — Real ONNX Runtime Mode");
        }
    }
}

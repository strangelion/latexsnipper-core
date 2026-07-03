use clap::{Parser, Subcommand};
use latexsnipper_pipeline::sdk::Snipper;
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::{Parser as _, Renderer as _};

const SUPPORTED_FORMATS: &str = "latex, markdown, typst, html, mathml, omml, json";

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

        /// Output file path (e.g., output.tex, output.typ). If omitted, prints to stdout.
        #[arg(short, long)]
        output: Option<String>,
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

fn resolve_format(format: &str, output: Option<&str>) -> String {
    if let Some(path) = output {
        if let Some(ext) = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
        {
            return match ext {
                "tex" | "latex" => "latex".to_string(),
                "typ" => "typst".to_string(),
                "md" | "markdown" => "markdown".to_string(),
                "html" | "htm" => "html".to_string(),
                "json" => "json".to_string(),
                "mathml" | "xml" => "mathml".to_string(),
                "omml" => "omml".to_string(),
                _ => format.to_string(),
            };
        }
    }
    format.to_string()
}

fn suggest_format(input: &str) -> Option<&'static str> {
    let lower = input.to_lowercase();
    let suggestions: Vec<(&str, Vec<&str>)> = vec![
        ("latex", vec!["latex", "tex", "late", "ltx"]),
        ("markdown", vec!["markdown", "md", "mark", "mard"]),
        ("typst", vec!["typst", "typ", "typs"]),
        ("html", vec!["html", "htm"]),
        ("json", vec!["json", "jsn"]),
        ("mathml", vec!["mathml", "math", "mml"]),
        ("omml", vec!["omml", "omm"]),
    ];

    for (correct, hints) in &suggestions {
        for hint in hints {
            if lower.contains(hint) || levenshtein_distance(&lower, hint) <= 2 {
                return Some(correct);
            }
        }
    }
    None
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(a_len + 1) {
        row[0] = i;
    }
    if let Some(first_row) = matrix.first_mut() {
        for (j, cell) in first_row.iter_mut().enumerate().take(b_len + 1) {
            *cell = j;
        }
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Recognize {
            input,
            format,
            output,
        } => {
            eprintln!("Processing: {}", input);

            let snipper = match Snipper::from_file(&input) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            eprintln!("Detected {} formulas", snipper.document().block_count());

            let resolved_format = resolve_format(&format, output.as_deref());

            let output_result = match resolved_format.as_str() {
                "latex" | "tex" => snipper.to_latex(),
                "markdown" | "md" => snipper.to_markdown(),
                "typst" => snipper.to_typst(),
                "html" => snipper.to_html(),
                "mathml" => snipper.to_mathml(),
                "omml" => snipper.to_omml(),
                "json" => snipper.to_json(),
                _ => {
                    eprintln!("Unknown format: '{}'", resolved_format);
                    if let Some(suggestion) = suggest_format(&resolved_format) {
                        eprintln!("  Did you mean '{}'?", suggestion);
                    }
                    eprintln!("  Supported formats: {}", SUPPORTED_FORMATS);
                    eprintln!("  Hint: use -h to see all options");
                    std::process::exit(1);
                }
            };

            match output_result {
                Ok(text) => {
                    if let Some(path) = output {
                        std::fs::write(&path, &text).unwrap_or_else(|e| {
                            eprintln!("Failed to write {}: {}", path, e);
                            std::process::exit(1);
                        });
                        eprintln!("Exported to {} ({})", path, resolved_format);
                    } else {
                        println!("{}", text);
                    }
                }
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

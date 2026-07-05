use clap::{Parser, Subcommand};
use latexsnipper_engine::sdk::Snipper;
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::{Parser as _, Renderer as _};
use std::io::{self, Write};

const SUPPORTED_FORMATS: &str = "latex, markdown, typst, html, mathml, omml, json";

#[derive(Parser)]
#[command(name = "snipper")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "LaTeXSnipper Core -- Image to LaTeX/Markdown/Typst converter")]
#[command(long_about = "LaTeXSnipper Core CLI\n\n\
    A command-line tool for recognizing mathematical formulas in images\n\
    and converting them to various formats (LaTeX, Markdown, Typst, HTML, MathML, OMML).\n\n\
    USAGE:\n    \
    snipper recognize -i image.png -f latex\n    \
    snipper recognize -i image.png -f markdown -o output.md\n    \
    snipper parse -l '\\frac{a}{b}'\n    \
    snipper render -l '\\frac{a}{b}'\n\n\
    For more information, run 'snipper <command> --help'.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Recognize formulas in an image and export to a format
    #[command(long_about = "Recognize mathematical formulas in an image.\n\n\
        Detects formulas (and optionally text) in the input image,\n\
        then exports the recognized content to the specified format.\n\n\
        EXAMPLES:\n    \
        snipper rec -i scan.png -f latex\n    \
        snipper recognize -i photo.jpg -f mathml -o output.xml\n    \
        snipper rec -i page.png -f markdown -o notes.md\n\n\
        SUPPORTED FORMATS:\n    \
        latex      - LaTeX source code (default)\n    \
        markdown   - Markdown with inline math\n    \
        typst      - Typst markup\n    \
        html       - HTML with MathJax\n    \
        mathml     - MathML XML\n    \
        omml       - Office MathML (Word)\n    \
        json       - Full AST as JSON")]
    Recognize {
        /// Input image path (png, jpg, pdf, bmp, tiff)
        #[arg(short = 'i', long)]
        input: String,

        /// Output format (default: latex)
        #[arg(short = 'f', long, default_value = "latex")]
        format: String,

        /// Output file path. If omitted, prints to stdout.
        #[arg(short = 'o', long)]
        output: Option<String>,
    },

    /// Shorthand for 'recognize'
    #[command(visible_alias = "rec")]
    Rec {
        /// Input image path
        #[arg(short = 'i', long)]
        input: String,

        /// Output format (default: latex)
        #[arg(short = 'f', long, default_value = "latex")]
        format: String,

        /// Output file path. If omitted, prints to stdout.
        #[arg(short = 'o', long)]
        output: Option<String>,
    },

    /// Parse a LaTeX string to AST (JSON)
    #[command(long_about = "Parse a LaTeX string into an Abstract Syntax Tree.\n\n\
        Outputs the full AST as formatted JSON, useful for debugging\n\
        and understanding how LaTeXSnipper parses formulas.\n\n\
        EXAMPLES:\n    \
        snipper parse -l '\\frac{a}{b}'\n    \
        snipper parse -l 'E = mc^2'")]
    Parse {
        /// LaTeX string to parse
        #[arg(short = 'l', long)]
        latex: String,
    },

    /// Render a LaTeX string back to LaTeX (roundtrip test)
    #[command(
        long_about = "Render a LaTeX string by parsing it to AST and back.\n\n\
        Useful for testing roundtrip fidelity -- the output should be\n\
        semantically equivalent to the input, though formatting may differ.\n\n\
        EXAMPLES:\n    \
        snipper render -l '\\frac{a}{b}'\n    \
        snipper render -l 'x^2 + y^2 = r^2'"
    )]
    Render {
        /// LaTeX string to render
        #[arg(short = 'l', long)]
        latex: String,
    },

    /// Show version, build info, and system details
    #[command(long_about = "Show detailed version and build information.\n\n\
        Includes version number, build target, and runtime mode.\n\
        Use '-v' or '--version' as a flag on the root command for brief output.")]
    Version,

    /// Play the LaTeX Math Rendering Challenge (minigame)
    #[command(long_about = "Launch the LaTeX Math Rendering Challenge.\n\n\
        A terminal mini-game where you see ASCII math art and must type\n\
        the correct LaTeX code. Features 3 difficulty levels, streak\n\
        bonuses, speed bonuses, and a hint system.\n\n\
        GAME COMMANDS:\n    \
        quit   - exit the game\n    \
        hint   - reveal next character (-50 pts)\n    \
        skip   - skip current question (-100 pts)\n    \
        answer - reveal the answer (0 pts)\n\n\
        SCORING:\n    \
        Base: 100 pts per correct answer\n    \
        Streak: +50 pts per consecutive correct answer\n    \
        Speed: up to +50 pts for fast answers\n    \
        Hint penalty: -50 pts per hint\n    \
        Skip penalty: -100 pts")]
    Play,

    /// Manage models (download, list, verify)
    #[command(subcommand)]
    Models(ModelsCommand),
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Download models from release
    #[command(long_about = "Download model packages from GitHub releases.\n\n\
        Downloads and extracts model packages to the models directory.\n\
        By default, downloads all required models from the official manifest.\n\n\
        EXAMPLES:\n    \
        snipper models download\n    \
        snipper models download --category formula-det\n    \
        snipper models download --all")]
    Download {
        /// Model category to download (e.g., formula-det, text-rec)
        #[arg(short = 'c', long)]
        category: Option<String>,

        /// Download all models (not just required ones)
        #[arg(short = 'a', long)]
        all: bool,

        /// Custom manifest URL
        #[arg(long)]
        manifest_url: Option<String>,
    },

    /// List installed models
    #[command(long_about = "List all installed model packages.\n\n\
        Shows which models are installed and their variants.\n\n\
        EXAMPLES:\n    \
        snipper models list\n    \
        snipper models list --category formula-det")]
    List {
        /// Filter by category
        #[arg(short = 'c', long)]
        category: Option<String>,
    },

    /// Verify model file existence against manifest
    #[command(
        long_about = "Verify that all model files listed in the manifest exist.\n\n\
SHA-256 integrity is checked at download time. This command only confirms\n\
that expected files are present after extraction.\n\n\
Examples:\n    \
snipper models verify\n    \
snipper models verify --category formula-det"
    )]
    Verify {
        /// Filter by category
        #[arg(short = 'c', long)]
        category: Option<String>,
    },
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
        }
        | Commands::Rec {
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
            println!("LaTeXSnipper Core -- Real ONNX Runtime Mode");
            println!();
            println!("Try 'snipper play' for a hidden mini-game!");
        }

        Commands::Play => play_game(),

        Commands::Models(cmd) => match cmd {
            ModelsCommand::Download {
                category,
                all,
                manifest_url,
            } => {
                handle_models_download(category, all, manifest_url);
            }
            ModelsCommand::List { category } => {
                handle_models_list(category);
            }
            ModelsCommand::Verify { category } => {
                handle_models_verify(category);
            }
        },
    }
}

fn handle_models_download(category: Option<String>, all: bool, manifest_url: Option<String>) {
    let models_dir = std::path::PathBuf::from("models");
    let manager = latexsnipper_model::ModelManager::new(models_dir);

    // Load manifest
    use latexsnipper_model::manifest::DEFAULT_MANIFEST_URL;
    let manifest_path = std::path::PathBuf::from("models/model-manifest.json");

    // --manifest-url overrides local manifest: always download fresh
    let manifest = if let Some(url) = manifest_url {
        eprintln!("Downloading manifest from {}", url);
        match latexsnipper_model::ModelManifest::download(&url) {
            Ok(m) => {
                if let Err(e) = m.save(&manifest_path) {
                    eprintln!("Warning: could not save manifest locally: {}", e);
                }
                eprintln!("Manifest downloaded successfully.");
                m
            }
            Err(e) => {
                eprintln!("Failed to download manifest: {}", e);
                std::process::exit(1);
            }
        }
    } else if manifest_path.exists() {
        match latexsnipper_model::ModelManifest::load(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to load local manifest: {}", e);
                eprintln!("Try --manifest-url to download a fresh manifest.");
                std::process::exit(1);
            }
        }
    } else {
        let url = DEFAULT_MANIFEST_URL.to_string();
        eprintln!("Local manifest not found, downloading from {}", url);

        match latexsnipper_model::ModelManifest::download(&url) {
            Ok(m) => {
                if let Err(e) = m.save(&manifest_path) {
                    eprintln!("Warning: could not save manifest locally: {}", e);
                }
                eprintln!("Manifest downloaded successfully.");
                m
            }
            Err(e) => {
                eprintln!("Failed to download manifest: {}", e);
                eprintln!(
                    "Ensure you have a network connection, or provide a local manifest at {}",
                    manifest_path.display()
                );
                std::process::exit(1);
            }
        }
    };

    if let Some(cat) = category {
        // Download specific category
        if let Some(info) = manifest.categories.get(&cat) {
            let variant_id = info.default.as_deref().unwrap_or("default");
            if let Some(variant) = info.variants.iter().find(|v| v.id == variant_id) {
                if let Some(ref zip_file) = variant.zip_file {
                    let url = format!("{}/{}", manifest.base_url, zip_file);
                    eprintln!("Downloading {} from {}", cat, url);

                    let progress =
                        Box::new(|status: latexsnipper_model::DownloadStatus| match status {
                            latexsnipper_model::DownloadStatus::Starting { url, total_bytes } => {
                                eprintln!("Starting download: {}", url);
                                if let Some(total) = total_bytes {
                                    eprintln!("  Size: {:.1} MB", total as f64 / 1024.0 / 1024.0);
                                }
                            }
                            latexsnipper_model::DownloadStatus::Progress { downloaded, total } => {
                                if let Some(total) = total {
                                    let percent = downloaded as f64 / total as f64 * 100.0;
                                    eprint!("\r  Progress: {:.1}%", percent);
                                }
                            }
                            latexsnipper_model::DownloadStatus::Extracting { file } => {
                                eprintln!("\n  Extracting: {}", file);
                            }
                            latexsnipper_model::DownloadStatus::Complete { path } => {
                                eprintln!("  Installed to: {}", path.display());
                            }
                            latexsnipper_model::DownloadStatus::Failed { error } => {
                                eprintln!("  Failed: {}", error);
                            }
                        });

                    let expected_sha256 = manifest.checksums.get(zip_file).map(|s| s.as_str());

                    match manager.download_with_progress(
                        &url,
                        &cat,
                        &variant.id,
                        expected_sha256,
                        &variant.files,
                        Some(progress),
                    ) {
                        Ok(path) => {
                            eprintln!("Successfully downloaded {} to {}", cat, path.display());
                        }
                        Err(e) => {
                            eprintln!("Download failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        } else {
            eprintln!("Unknown category: {}", cat);
            eprintln!(
                "Available categories: {:?}",
                manifest.categories.keys().collect::<Vec<_>>()
            );
            std::process::exit(1);
        }
    } else if all {
        // Download all models
        eprintln!("Downloading all models...");
        match manager.download_all(&manifest, true, None) {
            Ok(paths) => {
                eprintln!("Downloaded {} model packages", paths.len());
                for path in &paths {
                    eprintln!("  - {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("Download failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Download only required models
        eprintln!("Downloading required models...");
        match manager.download_all(&manifest, false, None) {
            Ok(paths) => {
                eprintln!("Downloaded {} model packages", paths.len());
                for path in &paths {
                    eprintln!("  - {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("Download failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn handle_models_list(category: Option<String>) {
    let models_dir = std::path::PathBuf::from("models");
    let manager = latexsnipper_model::ModelManager::new(models_dir);

    if let Some(cat) = category {
        let variants = manager.list_installed(&cat);
        if variants.is_empty() {
            eprintln!("No models installed for category: {}", cat);
        } else {
            eprintln!("Installed models for {}:", cat);
            for variant in &variants {
                eprintln!("  - {}", variant);
            }
        }
    } else {
        // List all categories
        let manifest_path = std::path::PathBuf::from("models/model-manifest.json");
        if manifest_path.exists() {
            if let Ok(manifest) = latexsnipper_model::ModelManifest::load(&manifest_path) {
                eprintln!("Installed models:");
                for (cat, info) in &manifest.categories {
                    let variants = manager.list_installed(cat);
                    if variants.is_empty() {
                        eprintln!("  {} (not installed)", cat);
                    } else {
                        eprintln!("  {}:", cat);
                        for variant in &variants {
                            let status =
                                if let Some(v) = info.variants.iter().find(|v| &v.id == variant) {
                                    if let Some(ref zip) = v.zip_file {
                                        format!(" ({})", zip)
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };
                            eprintln!("    - {}{}", variant, status);
                        }
                    }
                }
            }
        } else {
            eprintln!("No manifest found. Run 'snipper models download' first.");
        }
    }
}

fn handle_models_verify(category: Option<String>) {
    let models_dir = std::path::PathBuf::from("models");
    let manager = latexsnipper_model::ModelManager::new(models_dir);

    let manifest_path = std::path::PathBuf::from("models/model-manifest.json");
    if !manifest_path.exists() {
        eprintln!("No manifest found. Run 'snipper models download' first.");
        std::process::exit(1);
    }

    let manifest = match latexsnipper_model::ModelManifest::load(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to load manifest: {}", e);
            std::process::exit(1);
        }
    };

    let categories = if let Some(cat) = category {
        vec![cat]
    } else {
        manifest.categories.keys().cloned().collect()
    };

    let mut all_valid = true;
    let mut total_files = 0;
    let mut verified_files = 0;

    for cat in &categories {
        if let Some(info) = manifest.categories.get(cat) {
            let variants = manager.list_installed(cat);
            if variants.is_empty() {
                eprintln!("  {} (not installed)", cat);
                continue;
            }

            for variant in &variants {
                if let Some(v) = info.variants.iter().find(|v| &v.id == variant) {
                    let dir = manager.variant_dir(cat, variant);
                    let mut missing = Vec::new();
                    let mut present = Vec::new();

                    for file in &v.files {
                        total_files += 1;
                        let file_path = dir.join(file);
                        if !file_path.exists() {
                            missing.push(file.as_str());
                            all_valid = false;
                        } else {
                            present.push(file.as_str());
                            verified_files += 1;
                        }
                    }

                    if missing.is_empty() {
                        eprintln!(
                            "  {}/{} - VERIFY_OK ({} files)",
                            cat,
                            variant,
                            present.len()
                        );
                    } else {
                        for m in &missing {
                            eprintln!("  {}/{} - MISSING: {}", cat, variant, m);
                        }
                    }
                }
            }
        }
    }

    eprintln!(
        "\nVerified {}/{} files across installed models.",
        verified_files, total_files
    );
    if all_valid {
        eprintln!("All model files present.");
        eprintln!(
            "Note: SHA-256 integrity is verified at download time. Re-download to re-verify."
        );
    } else {
        eprintln!("Some model files are missing. Run 'snipper models download' to re-download.");
        std::process::exit(1);
    }
}

fn play_game() {
    use std::time::Instant;

    // ========================================================================
    // Puzzle Pool: (ascii_art, answer, hint, difficulty)
    // difficulty: 1=beginner, 2=intermediate, 3=advanced
    // ========================================================================
    struct Puzzle {
        art: &'static str,
        answer: &'static str,
        hint: &'static str,
        difficulty: u8,
    }

    let all_puzzles: Vec<Puzzle> = vec![
        // --- Difficulty 1: Beginner ---
        Puzzle {
            art: "  x",
            answer: "x",
            hint: "just the letter x",
            difficulty: 1,
        },
        Puzzle {
            art: "  a + b",
            answer: "a+b",
            hint: "a plus b",
            difficulty: 1,
        },
        Puzzle {
            art: "  42",
            answer: "42",
            hint: "the number 42",
            difficulty: 1,
        },
        Puzzle {
            art: "  E = mc^2",
            answer: "E=mc^2",
            hint: "E equals mc squared",
            difficulty: 1,
        },
        Puzzle {
            art: "  a^2",
            answer: "a^2",
            hint: "a superscript 2",
            difficulty: 1,
        },
        Puzzle {
            art: "  x_i",
            answer: "x_i",
            hint: "x subscript i",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\pi",
            answer: "\\pi",
            hint: "Greek letter pi",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\alpha",
            answer: "\\alpha",
            hint: "first Greek letter",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\beta",
            answer: "\\beta",
            hint: "second Greek letter",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\gamma",
            answer: "\\gamma",
            hint: "third Greek letter",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\infty",
            answer: "\\infty",
            hint: "infinity symbol",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\pm",
            answer: "\\pm",
            hint: "plus-minus sign",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\times",
            answer: "\\times",
            hint: "multiplication sign",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\leq",
            answer: "\\leq",
            hint: "less than or equal",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\geq",
            answer: "\\geq",
            hint: "greater than or equal",
            difficulty: 1,
        },
        Puzzle {
            art: "  \\neq",
            answer: "\\neq",
            hint: "not equal sign",
            difficulty: 1,
        },
        // --- Difficulty 2: Intermediate ---
        Puzzle {
            art: "  a / b\n  ---",
            answer: "\\frac{a}{b}",
            hint: "fraction a over b",
            difficulty: 2,
        },
        Puzzle {
            art: "  ___\n / 25",
            answer: "\\sqrt{25}",
            hint: "square root of 25",
            difficulty: 2,
        },
        Puzzle {
            art: "  ___\n / a+b",
            answer: "\\sqrt{a+b}",
            hint: "square root of sum",
            difficulty: 2,
        },
        Puzzle {
            art: "  n\n  \\ Sigma\n  i=1",
            answer: "\\sum_{i=1}^{n}",
            hint: "sum from i=1 to n",
            difficulty: 2,
        },
        Puzzle {
            art: "  b\n  \\ Integral\n  a",
            answer: "\\int_{a}^{b}",
            hint: "definite integral a to b",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\rightarrow",
            answer: "\\rightarrow",
            hint: "right arrow",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\leftarrow",
            answer: "\\leftarrow",
            hint: "left arrow",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\theta",
            answer: "\\theta",
            hint: "Greek letter theta",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\lambda",
            answer: "\\lambda",
            hint: "wavelength symbol",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\sigma",
            answer: "\\sigma",
            hint: "standard deviation",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\omega",
            answer: "\\omega",
            hint: "angular frequency",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\nabla",
            answer: "\\nabla",
            hint: "del operator",
            difficulty: 2,
        },
        Puzzle {
            art: "  \\partial",
            answer: "\\partial",
            hint: "partial derivative",
            difficulty: 2,
        },
        Puzzle {
            art: "  a \\cdot b",
            answer: "a \\cdot b",
            hint: "dot product notation",
            difficulty: 2,
        },
        Puzzle {
            art: "  x \\in R",
            answer: "x \\in \\mathbb{R}",
            hint: "x is in reals (use mathbb)",
            difficulty: 2,
        },
        Puzzle {
            art: "  A \\cup B",
            answer: "A \\cup B",
            hint: "A union B",
            difficulty: 2,
        },
        // --- Difficulty 3: Advanced ---
        Puzzle {
            art: "    n\n    \\ Sigma   k^2\n    k=1",
            answer: "\\sum_{k=1}^{n} k^2",
            hint: "sum of squares",
            difficulty: 3,
        },
        Puzzle {
            art: "       ___\n      / a\n     / ---\n    /  b",
            answer: "\\sqrt{\\frac{a}{b}}",
            hint: "square root of fraction",
            difficulty: 3,
        },
        Puzzle {
            art: "  d/dx [ f(x) ]",
            answer: "\\frac{d}{dx} [ f(x) ]",
            hint: "derivative notation",
            difficulty: 3,
        },
        Puzzle {
            art: "  lim\n  x->0  sin(x)/x",
            answer: "\\lim_{x \\to 0} \\frac{\\sin(x)}{x}",
            hint: "famous limit",
            difficulty: 3,
        },
        Puzzle {
            art: "    n\n    \\ Prod   i\n    i=1",
            answer: "\\prod_{i=1}^{n} i",
            hint: "product notation",
            difficulty: 3,
        },
        Puzzle {
            art: "  e^{i\\pi} + 1 = 0",
            answer: "e^{i\\pi} + 1 = 0",
            hint: "Euler's identity",
            difficulty: 3,
        },
        Puzzle {
            art: "  | \\psi > ",
            answer: "|\\psi\\rangle",
            hint: "ket notation (bra-ket)",
            difficulty: 3,
        },
        Puzzle {
            art: "  < \\phi | \\psi >",
            answer: "\\langle \\phi | \\psi \\rangle",
            hint: "inner product (bra-ket)",
            difficulty: 3,
        },
        Puzzle {
            art: "  A^T",
            answer: "A^T",
            hint: "matrix transpose",
            difficulty: 3,
        },
        Puzzle {
            art: "  det(A)",
            answer: "\\det(A)",
            hint: "matrix determinant",
            difficulty: 3,
        },
        Puzzle {
            art: "  \\forall x \\in X",
            answer: "\\forall x \\in X",
            hint: "for all x in X",
            difficulty: 3,
        },
        Puzzle {
            art: "  \\exists x",
            answer: "\\exists x",
            hint: "there exists x",
            difficulty: 3,
        },
        Puzzle {
            art: "  \\oint E \\cdot dl",
            answer: "\\oint \\mathbf{E} \\cdot d\\mathbf{l}",
            hint: "line integral (closed)",
            difficulty: 3,
        },
        Puzzle {
            art: "  \\binom{n}{k}",
            answer: "\\binom{n}{k}",
            hint: "binomial coefficient",
            difficulty: 3,
        },
        Puzzle {
            art: "  a \\equiv b (mod n)",
            answer: "a \\equiv b \\pmod{n}",
            hint: "modular congruence",
            difficulty: 3,
        },
        Puzzle {
            art: "  \\int_0^\\infty e^{-x} dx",
            answer: "\\int_0^{\\infty} e^{-x} dx",
            hint: "improper integral",
            difficulty: 3,
        },
    ];

    // ========================================================================
    // Title Screen
    // ========================================================================
    println!();
    println!("  ╔══════════════════════════════════════════════════╗");
    println!("  ║       LaTeX Math Rendering Challenge             ║");
    println!("  ╠══════════════════════════════════════════════════╣");
    println!("  ║  See the math art below. Type the LaTeX code!    ║");
    println!("  ║                                                  ║");
    println!("  ║  Commands:                                       ║");
    println!("  ║    quit   - exit game                            ║");
    println!("  ║    hint   - reveal next character (-50 pts)      ║");
    println!("  ║    skip   - skip question (-100 pts)             ║");
    println!("  ║    answer - reveal answer (0 pts)                ║");
    println!("  ║                                                  ║");
    println!("  ║  Scoring:                                        ║");
    println!("  ║    Base: 100 pts    Streak bonus: +50 per combo  ║");
    println!("  ║    Speed bonus: up to +50 pts for fast answers   ║");
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();

    // ========================================================================
    // Difficulty Selection
    // ========================================================================
    println!("  Select difficulty:");
    println!("    1) Beginner   - basic symbols and simple expressions");
    println!("    2) Intermediate - fractions, roots, sums");
    println!("    3) Advanced   - calculus, linear algebra, logic");
    println!("    4) Mixed      - all difficulties shuffled");
    print!("  Choice [1/2/3/4] > ");
    io::stdout().flush().unwrap();

    let mut diff_input = String::new();
    io::stdin().read_line(&mut diff_input).unwrap();
    let difficulty = match diff_input.trim() {
        "1" => 1u8,
        "2" => 2,
        "3" => 3,
        _ => 4, // mixed
    };

    // Filter and shuffle puzzles
    let mut puzzles: Vec<&Puzzle> = if difficulty == 4 {
        all_puzzles.iter().collect()
    } else {
        all_puzzles
            .iter()
            .filter(|p| p.difficulty == difficulty)
            .collect()
    };

    // Simple seeded shuffle using Fisher-Yates with a basic PRNG
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut rng_state = seed;
    for i in (1..puzzles.len()).rev() {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (rng_state >> 33) as usize % (i + 1);
        puzzles.swap(i, j);
    }

    let total_rounds = puzzles.len().min(15); // Cap at 15 rounds
    let puzzles = &puzzles[..total_rounds];

    // ========================================================================
    // Game Loop
    // ========================================================================
    let mut score: i64 = 0;
    let mut streak: u32 = 0;
    let mut correct: u32 = 0;
    let mut hints_used: u32 = 0;
    let game_start = Instant::now();

    println!();
    println!("  Starting {} rounds. Good luck!", total_rounds);
    println!();

    for (round, puzzle) in puzzles.iter().enumerate() {
        let round_num = round + 1;
        let diff_label = match puzzle.difficulty {
            1 => "Beginner",
            2 => "Intermediate",
            _ => "Advanced",
        };

        // Header
        println!(
            "  ┌─ Round {}/{} ─────────────────────────────────",
            round_num, total_rounds
        );
        println!("  │ Difficulty: {}", diff_label);
        if streak >= 3 {
            println!("  │ Streak: {}x (+{} bonus)", streak, streak * 50);
        }
        println!("  │");
        println!("  │  What is the LaTeX for this?");

        // Render ASCII art
        for line in puzzle.art.lines() {
            println!("  │    {}", line);
        }
        println!("  │");

        // Hint tracking
        let mut revealed_hint = false;
        let mut hint_chars = 0usize;

        // Input loop for this round
        let round_start = Instant::now();
        let mut answered = false;

        while !answered {
            print!("  └─> ");
            io::stdout().flush().unwrap();

            let mut guess = String::new();
            io::stdin().read_line(&mut guess).unwrap();
            let trimmed = guess.trim();

            match trimmed {
                "quit" | "q" | "exit" => {
                    println!();
                    print_final_stats(score, correct, total_rounds as u32, hints_used, &game_start);
                    return;
                }
                "hint" | "h" => {
                    if hint_chars < puzzle.answer.len() {
                        hint_chars += 1;
                        revealed_hint = true;
                        let partial: String = puzzle.answer.chars().take(hint_chars).collect();
                        let penalty = 50;
                        score -= penalty;
                        hints_used += 1;
                        println!("  [HINT] (cost: -{} pts): {}", penalty, partial);
                        println!("  [HINT] Full hint: {}", puzzle.hint);
                    } else {
                        println!(
                            "  [HINT] No more characters to reveal. Full hint: {}",
                            puzzle.hint
                        );
                    }
                    continue;
                }
                "skip" | "s" => {
                    score -= 100;
                    streak = 0;
                    println!("  [SKIP] Skipped (-100 pts). Answer was: {}", puzzle.answer);
                    println!();
                    break;
                }
                "answer" | "a" => {
                    println!("  [ANSWER] {}", puzzle.answer);
                    streak = 0;
                    println!();
                    break;
                }
                _ => {}
            }

            // Normalize: remove backslash for comparison flexibility
            let normalize = |s: &str| -> String {
                s.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            };

            let guess_norm = normalize(trimmed);
            let answer_norm = normalize(puzzle.answer);

            if guess_norm == answer_norm {
                // Correct!
                let elapsed = round_start.elapsed().as_secs();
                let speed_bonus: i64 = if elapsed <= 3 {
                    50
                } else if elapsed <= 8 {
                    30
                } else if elapsed <= 15 {
                    10
                } else {
                    0
                };
                streak += 1;
                let streak_bonus = (streak.saturating_sub(1)) as i64 * 50;
                let base = 100i64;
                let total_pts = base + streak_bonus + speed_bonus;
                score += total_pts;
                correct += 1;

                println!();
                println!(
                    "  [CORRECT] +{} pts (base:{} streak:{} speed:{})",
                    total_pts, base, streak_bonus, speed_bonus
                );
                if streak >= 3 {
                    println!("  [STREAK] {}x streak! Keep it going!", streak);
                }
                println!();
                answered = true;
            } else {
                // Check Levenshtein for near-miss
                let dist = levenshtein_distance(&guess_norm, &answer_norm);
                if dist <= 2 && !guess_norm.is_empty() {
                    println!("  [CLOSE] But not quite. (edit distance: {})", dist);
                } else if revealed_hint {
                    println!("  [WRONG] Try again or type 'skip'.");
                } else {
                    println!("  [WRONG] Try again, or type 'hint' for help.");
                }
            }
        }
    }

    // ========================================================================
    // Final Stats
    // ========================================================================
    print_final_stats(score, correct, total_rounds as u32, hints_used, &game_start);
}

fn print_final_stats(
    score: i64,
    correct: u32,
    total: u32,
    hints_used: u32,
    start: &std::time::Instant,
) {
    let elapsed = start.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;
    let accuracy = if total > 0 {
        (correct as f64 / total as f64 * 100.0) as u32
    } else {
        0
    };

    let rank = match score {
        s if s >= 2000 => "Grandmaster",
        s if s >= 1500 => "LaTeX Wizard",
        s if s >= 1000 => "Math Artist",
        s if s >= 500 => "Formula Apprentice",
        s if s >= 200 => "Symbol Explorer",
        _ => "Keep Practicing!",
    };

    println!("  ╔══════════════════════════════════════════════════╗");
    println!("  ║              Game Over!                          ║");
    println!("  ╠══════════════════════════════════════════════════╣");
    println!(
        "  ║  Score:       {:>6} pts                          ║",
        score
    );
    println!(
        "  ║  Correct:     {}/{} ({:>3}%)                      ║",
        correct, total, accuracy
    );
    println!(
        "  ║  Hints used:  {:>6}                               ║",
        hints_used
    );
    println!(
        "  ║  Time:        {:>2}:{:02}                               ║",
        mins, secs
    );
    println!("  ║  Rank:        {:<34} ║", rank);
    println!("  ╚══════════════════════════════════════════════════╝");
    println!();

    // ASCII art based on score
    if score >= 1500 {
        println!("       *  *  *  *  *");
        println!("      *  LA TEX  *");
        println!("       *  MASTER *");
        println!("        * * * * *");
    } else if score >= 500 {
        println!("      \\frac{{success}}{{practice}} = \\infty");
    } else {
        println!("      Keep going! \\int practice \\, dx = mastery");
    }
    println!();
}

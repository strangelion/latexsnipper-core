use std::fs;
use std::path::{Path, PathBuf};

use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use serde_json::Value;

#[derive(Debug)]
struct CorpusFormula {
    category: String,
    name: String,
    latex: String,
}

fn corpus_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OBSIDIAN_FORMULA_LIBRARY_DIR") {
        let formulas = PathBuf::from(path).join("formulas");
        if formulas.is_dir() {
            return Some(formulas);
        }
    }

    let sibling = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("obsidian-formula-library").join("formulas"));
    sibling.filter(|p| p.is_dir())
}

fn load_corpus() -> Vec<CorpusFormula> {
    let Some(dir) = corpus_dir() else {
        eprintln!("obsidian-formula-library corpus not found; skipping external corpus test");
        return Vec::new();
    };

    let mut formulas = Vec::new();
    let entries = fs::read_dir(&dir).expect("read corpus formulas directory");
    for entry in entries {
        let entry = entry.expect("read corpus entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_name.starts_with('_') {
            continue;
        }

        let data = fs::read_to_string(&path).expect("read corpus json");
        let value: Value = serde_json::from_str(&data).expect("parse corpus json");
        let category = value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(file_name)
            .to_string();
        let Some(items) = value.get("items").and_then(Value::as_array) else {
            continue;
        };

        for item in items {
            let Some(parts) = item.as_array() else {
                continue;
            };
            if parts.len() < 2 {
                continue;
            }
            let Some(name) = parts.first().and_then(Value::as_str) else {
                continue;
            };
            let Some(latex) = parts.get(1).and_then(Value::as_str) else {
                continue;
            };
            if latex.trim().is_empty() {
                continue;
            }
            formulas.push(CorpusFormula {
                category: category.clone(),
                name: name.to_string(),
                latex: latex.to_string(),
            });
        }
    }

    formulas.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.latex.cmp(&b.latex))
    });
    formulas
}

fn assert_no_empty_or_error_like_output(
    failures: &mut Vec<String>,
    formula: &CorpusFormula,
    path: &str,
    result: Result<String, String>,
) {
    match result {
        Ok(output) => {
            if output.trim().is_empty() {
                failures.push(format!(
                    "{} / {} [{}] produced empty output for {}",
                    formula.category, formula.name, formula.latex, path
                ));
            }
        }
        Err(error) => failures.push(format!(
            "{} / {} [{}] failed for {}: {}",
            formula.category, formula.name, formula.latex, path, error
        )),
    }
}

#[test]
fn obsidian_formula_library_latex_to_six_outputs() {
    let formulas = load_corpus();
    if formulas.is_empty() {
        return;
    }

    let outputs = [
        ("latex", OutputFormat::Latex),
        ("typst", OutputFormat::Typst),
        ("mathml", OutputFormat::MathML),
        ("omml", OutputFormat::OMML),
        ("markdown", OutputFormat::MarkdownBlock),
        ("html", OutputFormat::Html),
    ];

    let mut failures = Vec::new();
    for formula in &formulas {
        for (name, format) in outputs {
            let result = DocumentConverter::convert_latex_string(&formula.latex, format)
                .map_err(|e| e.to_string());
            assert_no_empty_or_error_like_output(&mut failures, formula, name, result);
        }
    }

    assert!(
        failures.is_empty(),
        "obsidian-formula-library latex-to-output failures ({} formulas, {} failures):\n{}",
        formulas.len(),
        failures.len(),
        failures
            .iter()
            .take(80)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn obsidian_formula_library_intermediate_roundtrips() {
    let formulas = load_corpus();
    if formulas.is_empty() {
        return;
    }

    let targets = [
        ("latex", OutputFormat::Latex),
        ("typst", OutputFormat::Typst),
        ("mathml", OutputFormat::MathML),
        ("omml", OutputFormat::OMML),
    ];

    let mut failures = Vec::new();
    for formula in &formulas {
        let mathml = DocumentConverter::convert_latex_string(&formula.latex, OutputFormat::MathML)
            .map_err(|e| e.to_string());
        let omml = DocumentConverter::convert_latex_string(&formula.latex, OutputFormat::OMML)
            .map_err(|e| e.to_string());
        let typst = DocumentConverter::convert_latex_string(&formula.latex, OutputFormat::Typst)
            .map_err(|e| e.to_string());
        let markdown = Ok(format!("$$ {} $$", formula.latex));

        let intermediates = [
            ("mathml", mathml),
            ("omml", omml),
            ("typst", typst),
            ("markdown", markdown),
        ];

        for (source, value) in intermediates {
            let Ok(value) = value else {
                failures.push(format!(
                    "{} / {} [{}] failed to build {} intermediate: {}",
                    formula.category,
                    formula.name,
                    formula.latex,
                    source,
                    value.unwrap_err()
                ));
                continue;
            };

            for (target_name, target_format) in targets {
                let result = match source {
                    "mathml" => DocumentConverter::convert_mathml_string(&value, target_format),
                    "omml" => DocumentConverter::convert_omml_string(&value, target_format),
                    "typst" => DocumentConverter::convert_typst_string(&value, target_format),
                    "markdown" => DocumentConverter::convert_markdown_string(&value, target_format),
                    _ => unreachable!(),
                }
                .map_err(|e| e.to_string());
                assert_no_empty_or_error_like_output(
                    &mut failures,
                    formula,
                    &format!("{}->{}", source, target_name),
                    result,
                );
            }
        }
    }

    assert!(
        failures.is_empty(),
        "obsidian-formula-library intermediate roundtrip failures ({} formulas, {} failures):\n{}",
        formulas.len(),
        failures.len(),
        failures
            .iter()
            .take(80)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

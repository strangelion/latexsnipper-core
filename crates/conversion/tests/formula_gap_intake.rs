use std::{collections::BTreeSet, path::Path};

use latexsnipper_conversion::{
    DocumentConverter, FormulaConversionMode, FormulaInputFormat, OutputFormat,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[test]
fn minimized_style_gaps_are_hash_pinned_and_fail_closed_until_implemented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let record: Value = serde_json::from_slice(
        &std::fs::read(root.join("quality/failure-corpus/inbox/formula-style-command-gaps.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(record["status"], "minimized");
    assert_eq!(record["reproducible"], true);
    assert_eq!(record["sanitized"], true);
    assert_eq!(record["redistributable"], true);
    assert_eq!(record["license"], "AGPL-3.0-only");
    let input = std::fs::read_to_string(root.join(record["sanitizedInputRef"].as_str().unwrap()))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        format!("{:x}", Sha256::digest(input.as_bytes())),
        record["inputHash"].as_str().unwrap()
    );
    assert!(root.join(record["expectedRef"].as_str().unwrap()).is_file());
    let fixture: Value = serde_json::from_str(&input).unwrap();
    let mut ids = BTreeSet::new();
    for case in fixture["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        assert_eq!(case["status"], "missing");
        assert!(case["expected"].is_object());
        let source = case["latex"].as_str().unwrap();
        let error = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::Latex,
            OutputFormat::OMML,
            FormulaConversionMode::Strict,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(&format!(
                "Unsupported OMML command \\{}",
                case["command"].as_str().unwrap()
            )),
            "{}: {error}",
            case["id"]
        );
        assert!(error.contains("original source must be retained"));
        assert!(
            DocumentConverter::convert_latex_string(source, OutputFormat::Latex)
                .unwrap()
                .contains(source)
        );
    }
    assert_eq!(ids.len(), 4);
}

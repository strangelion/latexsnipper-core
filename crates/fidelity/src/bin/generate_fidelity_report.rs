use std::fmt::Write as _;
use std::path::PathBuf;

use latexsnipper_ast::{FidelityClaim, FidelityMeasurement};
use latexsnipper_fidelity::{
    load_and_validate_index, run, FidelityReport, LayerKind, LayerStatus, RunOptions,
};

fn main() {
    if let Err(error) = execute() {
        eprintln!("generate-fidelity-report: {error}");
        std::process::exit(2);
    }
}

fn execute() -> latexsnipper_fidelity::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut index = None;
    let mut repository_root = PathBuf::from(".");
    let mut output = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| {
            latexsnipper_fidelity::FidelityError::InvalidCorpus(format!("missing value for {flag}"))
        })?;
        match flag.as_str() {
            "--index" => index = Some(PathBuf::from(value)),
            "--repository-root" => repository_root = PathBuf::from(value),
            "--output" => output = Some(PathBuf::from(value)),
            _ => {
                return Err(latexsnipper_fidelity::FidelityError::InvalidCorpus(
                    format!("unknown option {flag}"),
                ));
            }
        }
    }
    let index_path = index.ok_or_else(|| {
        latexsnipper_fidelity::FidelityError::InvalidCorpus("--index is required".to_string())
    })?;
    let output = output.ok_or_else(|| {
        latexsnipper_fidelity::FidelityError::InvalidCorpus("--output is required".to_string())
    })?;
    let index = load_and_validate_index(&index_path, &repository_root)?;
    let report = run(
        &index,
        &RunOptions::ci(
            repository_root,
            "repository-fixtures".to_string(),
            "deterministic".to_string(),
        ),
    )?;
    let markdown = render_report(&report);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, markdown)?;
    Ok(())
}

fn render_report(report: &FidelityReport) -> String {
    let mut markdown = String::from(
        "# Generated Office package fidelity evidence\n\n\
This file is generated from the checksum-pinned fixtures in `fidelity/corpora/index.json`. Do not edit it by hand.\n\n\
The measurements below execute Core import, same-format export, package validation, and read-back. They do **not** run Microsoft Word, Excel, or PowerPoint. Skipped application and visual layers remain unverified.\n\n\
| Case | Format pair | structuralValidity | semanticPreservation | layoutPreservation | visualFidelity | editability | roundTripFidelity |\n\
|---|---|---|---|---|---|---|---|\n",
    );
    for case in &report.cases {
        let dimensions = &case.dimensions;
        writeln!(
            markdown,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            case.id,
            case.format_pair,
            measurement(&dimensions.structural_validity),
            measurement(&dimensions.semantic_preservation),
            measurement(&dimensions.layout_preservation),
            measurement(&dimensions.visual_fidelity),
            measurement(&dimensions.editability),
            measurement(&dimensions.round_trip_fidelity),
        )
        .expect("writing to a String cannot fail");
    }

    markdown.push_str("\n## Declared package capability expectations\n\n");
    for case in &report.cases {
        let layer = case
            .layers
            .iter()
            .find(|layer| layer.layer == LayerKind::CapabilityExpectationComparison);
        let Some(layer) = layer else {
            continue;
        };
        writeln!(
            markdown,
            "### {}\n\nStatus: `{}` — {}\n",
            case.id,
            layer_status(layer.status),
            layer.summary
        )
        .expect("writing to a String cannot fail");
        for evidence in &layer.evidence {
            writeln!(markdown, "- `{evidence}`").expect("writing to a String cannot fail");
        }
        if layer.evidence.is_empty() {
            markdown.push_str("- No package capability expectations declared.\n");
        }
        markdown.push('\n');
    }

    markdown.push_str(
        "## Evidence boundary\n\n\
- `preserved` means the exported package still contains the declared deterministic token and reopens through Core.\n\
- `unsupported` means Core emitted the required stable diagnostic; it is not a successful Office feature claim.\n\
- `not-measured` means the capability requires the Office adapter or a real application harness.\n\
- Visual parity, field recalculation, clipboard ownership, OLE activation, and batch insertion remain outside this package-only report.\n",
    );
    markdown
}

fn measurement(value: &FidelityMeasurement) -> String {
    let claim = match value.claim {
        FidelityClaim::Verified => "verified",
        FidelityClaim::Partial => "partial",
        FidelityClaim::Unsupported => "unsupported",
        FidelityClaim::NotMeasured => "not-measured",
        FidelityClaim::NotApplicable => "not-applicable",
    };
    value
        .score
        .map(|score| format!("{claim} ({score:.3})"))
        .unwrap_or_else(|| claim.to_string())
}

fn layer_status(value: LayerStatus) -> &'static str {
    match value {
        LayerStatus::Passed => "passed",
        LayerStatus::Failed => "failed",
        LayerStatus::Skipped => "skipped",
    }
}

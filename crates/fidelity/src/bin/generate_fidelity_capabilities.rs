use std::fmt::Write as _;
use std::path::PathBuf;

use latexsnipper_ast::{FidelityClaim, FidelityMeasurement};
use latexsnipper_conversion::DocumentExportService;

fn main() {
    if let Err(error) = execute() {
        eprintln!("generate-fidelity-capabilities: {error}");
        std::process::exit(2);
    }
}

fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--output"), Some(path), None) => PathBuf::from(path),
        _ => return Err("usage: generate-fidelity-capabilities --output PATH".into()),
    };
    let matrix = DocumentExportService::capability_matrix();
    let office_pdf = ["DOCX", "PPTX", "XLSX", "PDF"];
    let mut markdown = String::from(
        "# Generated Office and PDF fidelity capabilities\n\n\
This file is generated from `DocumentExportService::capability_matrix()`. Do not edit it by hand.\n\n\
Claims are independent. In particular, `verified` structural validity does not imply visual parity.\n\n\
| Input | Output | structuralValidity | semanticPreservation | layoutPreservation | visualFidelity | editability | roundTripFidelity |\n\
|---|---|---|---|---|---|---|---|\n",
    );
    for entry in &matrix.entries {
        let input = entry.input.as_deref().unwrap_or("?");
        let output = entry.output.as_deref().unwrap_or("?");
        if !office_pdf.contains(&input) && !office_pdf.contains(&output) {
            continue;
        }
        let dimensions = &entry.fidelity_dimensions;
        writeln!(
            markdown,
            "| {input} | {output} | {} | {} | {} | {} | {} | {} |",
            claim(&dimensions.structural_validity),
            claim(&dimensions.semantic_preservation),
            claim(&dimensions.layout_preservation),
            claim(&dimensions.visual_fidelity),
            claim(&dimensions.editability),
            claim(&dimensions.round_trip_fidelity),
        )?;
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, markdown)?;
    Ok(())
}

fn claim(measurement: &FidelityMeasurement) -> &'static str {
    match measurement.claim {
        FidelityClaim::Verified => "verified",
        FidelityClaim::Partial => "partial",
        FidelityClaim::Unsupported => "unsupported",
        FidelityClaim::NotMeasured => "not-measured",
        FidelityClaim::NotApplicable => "not-applicable",
    }
}

use std::fmt::Write as _;
use std::path::PathBuf;

use latexsnipper_conversion::{
    CapabilityRegistry, CapabilityTarget, FormulaConversionMode, FormulaInputFormat,
};

fn markdown() -> String {
    let mut text = String::from(
        "# Generated formula string conversion capabilities\n\n\
Generated from `CapabilityRegistry::formula_conversions`; do not edit the table by hand.\n\n\
Scope: Rust `DocumentConverter::convert_formula_string`, semantic string outputs only.\n\
Compiled support is not complete syntax coverage, losslessness or visual parity.\n\
Strict currently validates the supported LaTeX source subset before OMML export.\n\
Best-effort may lose unsupported syntax/style; reconstructed LaTeX is not author source.\n\n\
Python exposes `convert_formula` and `formula_conversion_capabilities`; C exposes model-free formula symbols.\n\
WASM exposes `convert_formula_v3` and `formula_capabilities_v3`; JS provides typed direct-module helpers.\n\
See `crates/ffi/include/latexsnipper_session.h` for C and `crates/wasm/js/README.md` for synchronous JS/worker boundaries.\n\
The new entry point bounds source/reconstructed LaTeX to 64 KiB, 64 lexical nesting levels and 512 structural tokens.\n\
These are conservative per-call budgets, not syntax validation; existing APIs are unchanged.\n\
OLE is a host container, not a Core string format. VSTO is not a format.\n\
Visual/package outputs still use the existing export registry and runtime requirements.\n\n\
| Target | Input | Path | Best-effort outputs | Strict outputs |\n\
|---|---|---|---|---|\n",
    );
    for target in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        let routes = CapabilityRegistry::formula_conversions(target);
        for &input in FormulaInputFormat::all() {
            let outputs = |mode| {
                let values = routes
                    .iter()
                    .filter(|entry| entry.input == input && entry.mode == mode && entry.available)
                    .map(|entry| entry.output)
                    .collect::<Vec<_>>();
                if values.is_empty() {
                    "unsupported".to_string()
                } else {
                    values.join(", ")
                }
            };
            let path = routes
                .iter()
                .find(|entry| entry.input == input)
                .unwrap()
                .path;
            writeln!(
                text,
                "| {} | {} | {} | {} | {} |",
                match target {
                    CapabilityTarget::Native => "native",
                    CapabilityTarget::Wasm32UnknownUnknown => "wasm32-unknown-unknown",
                },
                input.name(),
                path,
                outputs(FormulaConversionMode::BestEffort),
                outputs(FormulaConversionMode::Strict)
            )
            .unwrap();
        }
    }
    text
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (mode, output) = match (args.next(), args.next(), args.next()) {
        (Some(mode), Some(path), None) if mode == "--output" || mode == "--check" => {
            (mode, PathBuf::from(path))
        }
        _ => return Err("usage: generate_formula_capabilities (--output | --check) PATH".into()),
    };
    let generated = markdown();
    if mode == "--check" {
        let actual = std::fs::read_to_string(output)?.replace("\r\n", "\n");
        if actual != generated {
            return Err("generated formula capability documentation is stale".into());
        }
    } else {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, generated)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn checked_in_matrix_matches_the_registry() {
        assert_eq!(
            super::markdown(),
            include_str!("../../../../docs/generated/formula-capabilities.md")
                .replace("\r\n", "\n")
        );
    }
}

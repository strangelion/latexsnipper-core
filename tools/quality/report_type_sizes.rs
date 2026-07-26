use std::mem::{align_of, size_of};
use std::process::Command;

use latexsnipper_ast::{Formula, Inline, RecognitionProvenance, TransformationEvidence};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let type_entry = |name: &str, size: usize, alignment: usize| {
        serde_json::json!({
            "name": name,
            "sizeBytes": size,
            "alignmentBytes": alignment,
        })
    };
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "commit": commit,
            "target": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "pointerWidthBits": size_of::<usize>() * 8,
            },
            "types": [
                type_entry("Inline", size_of::<Inline>(), align_of::<Inline>()),
                type_entry("Formula", size_of::<Formula>(), align_of::<Formula>()),
                type_entry(
                    "RecognitionProvenance",
                    size_of::<RecognitionProvenance>(),
                    align_of::<RecognitionProvenance>(),
                ),
                type_entry(
                    "TransformationEvidence",
                    size_of::<TransformationEvidence>(),
                    align_of::<TransformationEvidence>(),
                ),
            ],
        }))?
    );
    Ok(())
}

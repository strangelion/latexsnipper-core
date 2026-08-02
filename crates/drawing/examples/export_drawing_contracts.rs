use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use latexsnipper_drawing::{
    drawing_readiness, DrawingArtifactRef, DrawingCompatibility, DrawingDocument,
    DrawingOfficePayload, DrawingOutputFormat, DrawingSecurityPolicy, DrawingSource,
    DrawingSourceLanguage,
};
use schemars::schema_for;
use serde::Serialize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check = std::env::args()
        .skip(1)
        .any(|argument| argument == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let payload = payload_fixture()?;
    let readiness = drawing_readiness(&DrawingSecurityPolicy::default(), &BTreeMap::new());
    let files = [
        (
            root.join("contracts/schema/drawing-office-payload-v1.schema.json"),
            pretty(&schema_for!(DrawingOfficePayload))?,
        ),
        (
            root.join("contracts/schema/drawing-readiness-v1.schema.json"),
            pretty(&schema_for!(latexsnipper_drawing::DrawingReadiness))?,
        ),
        (
            root.join("contracts/fixtures/drawing-office-payload-v1.json"),
            pretty(&payload)?,
        ),
        (
            root.join("contracts/fixtures/drawing-readiness-v1.json"),
            pretty(&readiness)?,
        ),
    ];
    for (path, content) in files {
        if check {
            check_file(&path, &content)?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, content)?;
            println!("wrote {}", path.strip_prefix(&root)?.display());
        }
    }
    Ok(())
}

fn pretty(value: &impl Serialize) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value).map(|text| format!("{text}\n"))
}

fn check_file(path: &Path, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let actual = fs::read_to_string(path)?;
    if actual.replace("\r\n", "\n") != expected {
        return Err(format!(
            "{} drifted; run `cargo run -p latexsnipper-drawing --features contract-schema --example export_drawing_contracts`",
            path.display()
        )
        .into());
    }
    println!("verified {}", path.display());
    Ok(())
}

fn payload_fixture() -> Result<DrawingOfficePayload, serde_json::Error> {
    let document = DrawingDocument {
        schema_version: 1,
        id: "drawing-contract-v1".to_owned(),
        source_language: DrawingSourceLanguage::DrawingJson,
        package_profiles: Vec::new(),
        source: DrawingSource {
            text: r#"{"kind":"rect","bounds":[0,0,160,90]}"#.to_owned(),
        },
        compatibility: DrawingCompatibility::VisualCompatible,
        canvas: latexsnipper_drawing::DrawingCanvas {
            width: 160.0,
            height: 90.0,
            view_box: [0.0, 0.0, 160.0, 90.0],
        },
        layers: Vec::new(),
        objects: vec![latexsnipper_drawing::DrawingObject::Rect {
            id: "rect-1".to_owned(),
            bounds: [0.0, 0.0, 160.0, 90.0],
        }],
        raw_nodes: Vec::new(),
        resources: Vec::new(),
        datasets: Vec::new(),
        provenance: Default::default(),
    };
    DrawingOfficePayload::new(
        document,
        DrawingArtifactRef {
            format: DrawingOutputFormat::Svg,
            content_ref: "artifacts/drawing-contract-v1.svg".to_owned(),
            sha256: "a".repeat(64),
            sanitizer_report_sha256: Some("b".repeat(64)),
        },
        vec![DrawingArtifactRef {
            format: DrawingOutputFormat::Png,
            content_ref: "artifacts/drawing-contract-v1.png".to_owned(),
            sha256: "c".repeat(64),
            sanitizer_report_sha256: None,
        }],
        160.0,
        90.0,
        "contract-renderer@1+sha256:verified",
        None,
    )
}

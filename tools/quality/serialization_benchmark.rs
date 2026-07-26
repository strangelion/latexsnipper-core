use std::time::Instant;

use latexsnipper_ast::{
    Formula, PostProcessResult, RecognitionProvenance, TransformationEvidence, TransformationMode,
    TriggerDecision, ValidationEvidence,
};
use serde_json::{json, Value};

const ITERATIONS: usize = 20;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for count in [100usize, 1_000] {
        let values = formula_values(count)?;
        cases.push(measure_value(
            &format!("formulas-{count}-inline-provenance"),
            count,
            &values,
        )?);
        cases.push(measure_value(
            &format!("formulas-{count}-registry-prototype"),
            count,
            &registry_prototype(&values),
        )?);
    }
    let cells = table_cell_values(10_000);
    cases.push(measure_value("table-cells-10000", 10_000, &cells)?);
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "iterations": ITERATIONS,
            "cases": cases,
            "registryDesignStatus": "evaluation-only",
            "registryDesign": {
                "shape": "Document.provenanceRegistry + formula.provenanceId",
                "migration": [
                    "Readers continue accepting inline recognitionProvenance.",
                    "Writers opt in only after a document schema version bump.",
                    "A compatibility writer expands provenanceId for legacy clients."
                ],
                "notImplementedInAst": true
            }
        }))?
    );
    Ok(())
}

fn formula_values(count: usize) -> Result<Value, serde_json::Error> {
    let formulas = (0..count)
        .map(|index| {
            let raw = format!(r"\frac{{x_{index}}}{{y}}");
            let corrected = format!(r"\frac{{x_{index}}}{{y}}");
            let validation = valid_evidence();
            let mut formula = Formula::latex(corrected.clone());
            formula.confidence = 0.91;
            formula.recognition_provenance = Some(Box::new(RecognitionProvenance {
                model_id: "trocr-deit".to_owned(),
                model_version: "models-v3.1.0".to_owned(),
                runtime: "onnxruntime".to_owned(),
                provider: "cpu".to_owned(),
                source_region: None,
                raw_confidence: Some(0.91),
                normalized_confidence: Some(0.91),
                transformations: vec![TransformationEvidence {
                    rule_id: "normalize-whitespace-v1".to_owned(),
                    before_sha256: "0".repeat(64),
                    after_sha256: "1".repeat(64),
                    reason: "benchmark representative evidence".to_owned(),
                    confidence_delta: 0.0,
                    mode: TransformationMode::Automatic,
                    version: "1".to_owned(),
                }],
            }));
            formula.recognition_evidence = Some(Box::new(PostProcessResult {
                raw: raw.clone(),
                normalized: raw.clone(),
                corrected,
                diff: None,
                trigger: TriggerDecision {
                    should_run: true,
                    triggers: vec!["normalization".to_owned()],
                },
                raw_confidence: 0.91,
                normalized_confidence: 0.91,
                validation: validation.clone(),
                corrected_validation: validation,
                transformations: Vec::new(),
                review_required: false,
                status_code: None,
            }));
            formula
        })
        .collect::<Vec<_>>();
    serde_json::to_value(formulas)
}

fn registry_prototype(inline: &Value) -> Value {
    let mut formulas = inline.as_array().cloned().unwrap_or_default();
    let provenance = formulas
        .first()
        .and_then(|formula| formula.get("recognition_provenance"))
        .cloned()
        .unwrap_or(Value::Null);
    for formula in &mut formulas {
        if let Some(object) = formula.as_object_mut() {
            object.remove("recognition_provenance");
            object.insert("provenanceId".to_owned(), json!("p0"));
        }
    }
    json!({
        "provenanceRegistry": {"p0": provenance},
        "formulas": formulas,
    })
}

fn table_cell_values(count: usize) -> Value {
    Value::Array(
        (0..count)
            .map(|index| {
                json!({
                    "content": [],
                    "colspan": 1,
                    "rowspan": 1,
                    "data_type": "Formula",
                    "formula": format!("=A{}*10%", index + 1),
                })
            })
            .collect(),
    )
}

fn measure_value(name: &str, item_count: usize, value: &Value) -> Result<Value, serde_json::Error> {
    let encoded = serde_json::to_vec(value)?;
    let serialize_started = Instant::now();
    for _ in 0..ITERATIONS {
        std::hint::black_box(serde_json::to_vec(value)?);
    }
    let serialize_ms = serialize_started.elapsed().as_secs_f64() * 1000.0 / ITERATIONS as f64;
    let deserialize_started = Instant::now();
    for _ in 0..ITERATIONS {
        std::hint::black_box(serde_json::from_slice::<Value>(&encoded)?);
    }
    let deserialize_ms = deserialize_started.elapsed().as_secs_f64() * 1000.0 / ITERATIONS as f64;
    Ok(json!({
        "name": name,
        "itemCount": item_count,
        "jsonBytes": encoded.len(),
        "serializeMeanMs": serialize_ms,
        "deserializeMeanMs": deserialize_ms,
    }))
}

fn valid_evidence() -> ValidationEvidence {
    ValidationEvidence {
        balanced_groups: true,
        environments_closed: true,
        left_right_balanced: true,
        duplicate_token_run: false,
        dangling_command: false,
        unexpected_eos: false,
        truncated: false,
        matrix_shape_valid: true,
    }
}

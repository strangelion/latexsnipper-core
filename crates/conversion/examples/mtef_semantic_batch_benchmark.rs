//! Repository-authored in-memory benchmark, not Word/MathType conversion.
use std::hint::black_box;
use std::time::Instant;

use latexsnipper_conversion::mtef_semantic::read_mtef_v5;
use latexsnipper_conversion::mtef_semantic_batch::{read_mtef_v5_batch, BatchStats};

const OCCURRENCES: usize = 10_000;
const TRIALS: usize = 5;

fn fixtures() -> Vec<Vec<u8>> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mtef-semantic-v1.json"
    ))
    .unwrap();
    fixture["accepted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            format!(
                "{} {}",
                fixture["header_hex"].as_str().unwrap(),
                row["body_hex"].as_str().unwrap()
            )
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
        })
        .collect()
}

fn numbers() -> Vec<Vec<u8>> {
    (0..1250)
        .map(|value| {
            let mut bytes = vec![5, 1, 0, 7, 0, b'p', b'i', b'l', b'o', b't', 0, 0, 10, 1, 0];
            for digit in value.to_string().bytes() {
                bytes.extend([2, 0, 136, digit, 0]);
            }
            bytes.extend([0, 0]);
            bytes
        })
        .collect()
}

fn unique_headers() -> Vec<Vec<u8>> {
    (0..OCCURRENCES)
        .map(|value| {
            let mut bytes = vec![5, 1, 0, 7, 0];
            bytes.extend(format!("pilot{value}").bytes());
            bytes.extend([0, 0, 10, 1, 0, 2, 0, 131, b'x', 0, 0, 0]);
            bytes
        })
        .collect()
}

fn median(times: &mut [f64]) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn measure(name: &str, sources: &[Vec<u8>]) -> serde_json::Value {
    let input: Vec<_> = (0..OCCURRENCES)
        .map(|index| sources[index % sources.len()].as_slice())
        .collect();
    let mut direct_ms = Vec::new();
    let mut batch_ms = Vec::new();
    let mut stats = BatchStats::default();
    for trial in 0..=TRIALS {
        let start = Instant::now();
        let direct: Vec<_> = input
            .iter()
            .map(|bytes| read_mtef_v5(black_box(bytes)))
            .collect();
        let direct_elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let batch = read_mtef_v5_batch(black_box(&input)).unwrap();
        let batch_elapsed = start.elapsed().as_secs_f64() * 1000.0;
        stats = batch.stats();
        for (index, direct) in direct.iter().enumerate() {
            let shared = batch.get(index).unwrap();
            assert!(direct.ast.is_some() && shared.report.ast.is_some());
            assert!(direct.error.is_none() && shared.report.error.is_none());
            assert_eq!(direct.latex, shared.report.latex);
            assert_eq!(direct.losses, shared.report.losses);
            assert_eq!(shared.source.as_ptr(), input[index].as_ptr());
        }
        black_box(&direct);
        black_box(&batch);
        if trial > 0 {
            direct_ms.push(direct_elapsed);
            batch_ms.push(batch_elapsed);
        }
    }
    let direct_median = median(&mut direct_ms);
    let batch_median = median(&mut batch_ms);
    serde_json::json!({"name": name, "occurrences": stats.occurrences, "unique_reads": stats.unique_reads,
        "reused_reads": stats.reused_reads, "input_bytes": stats.input_bytes,
        "inspected_records": stats.inspected_records, "ast_work": stats.ast_work,
        "structured_array_items": stats.structured_array_items,
        "stored_losses": stats.losses, "stored_output_bytes": stats.output_bytes,
        "output_and_loss_agreement": true, "direct_median_ms": direct_median,
        "batch_median_ms": batch_median, "median_speedup": direct_median / batch_median,
        "direct_trials_ms": direct_ms, "batch_trials_ms": batch_ms})
}

fn main() {
    let authored = fixtures();
    let report = serde_json::json!({"schema_version": 1, "profile_version": 1,
        "scope": "Authored raw v5 bytes; in-memory framing, AST and canonical LaTeX only; no Office, container IO, visual accuracy or third-party samples",
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "warmup_trials": 1, "measured_trials": TRIALS,
        "cases": [measure("one-fraction-repeated", &authored[1..2]),
            measure("five-mixed-structures", &authored), measure("1250-distinct-numbers-repeated", &numbers()),
            measure("10000-distinct-headers-no-reuse", &unique_headers())]});
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

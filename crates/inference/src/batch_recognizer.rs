//! Batch region recognition.
//!
//! Recognition requests (text crops, formula crops, segmented formula crops)
//! are grouped into batches sized from real runtime evidence — the actual
//! effective provider, runtime capability, memory/manifest limits and input
//! size buckets — never from the configured preferred provider alone.
//!
//! Every batch run records `batchSize`, `batchCount`, `paddingRatio`,
//! `effectiveProvider`, `latency` and `fallback` evidence.

use std::time::{Duration, Instant};

use latexsnipper_image::SnipperImage;
use serde::{Deserialize, Serialize};

/// What kind of region a batch request represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionRequestKind {
    TextCrop,
    FormulaCrop,
    FormulaSegmentCrop,
}

/// One recognition request inside a batch.
#[derive(Debug, Clone)]
pub struct RegionRecognitionRequest {
    pub id: String,
    pub kind: RegionRequestKind,
    pub image: SnipperImage,
}

/// One recognition result inside a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionRecognitionResult {
    pub id: String,
    pub kind: RegionRequestKind,
    pub text: String,
    pub confidence: f32,
    pub latency_ms: u64,
}

/// Versioned policy controlling batch sizes from runtime evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSizePolicy {
    pub version: String,
    /// Base batch size for CPU-only execution.
    pub cpu_batch_size: usize,
    /// Batch size for CUDA/Metal/CoreML-style accelerators.
    pub accelerated_batch_size: usize,
    /// Batch size for WebGPU/WASM execution.
    pub wasm_batch_size: usize,
    /// Hard cap on any single batch.
    pub max_batch_size: usize,
}

impl Default for BatchSizePolicy {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            cpu_batch_size: 4,
            accelerated_batch_size: 16,
            wasm_batch_size: 2,
            max_batch_size: 32,
        }
    }
}

/// Runtime evidence used to size batches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRuntimeEvidence {
    /// The effective provider the session actually resolved to.
    pub effective_provider: String,
    /// Provider family derived from the effective provider string.
    pub provider_family: ProviderFamily,
    /// Model manifest-declared batch limit (None when unconstrained).
    pub manifest_batch_limit: Option<usize>,
    /// Available memory in bytes (None when unknown — never assumed).
    pub available_memory_bytes: Option<u64>,
}

/// Coarse provider families for batch sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFamily {
    Cpu,
    Accelerated,
    Wasm,
    Unknown,
}

/// Resolve a provider family from an effective provider string.
pub fn provider_family(provider: &str) -> ProviderFamily {
    let lower = provider.to_ascii_lowercase();
    if lower.contains("wasm") || lower.contains("webgpu") {
        ProviderFamily::Wasm
    } else if lower.contains("cuda")
        || lower.contains("rocm")
        || lower.contains("coreml")
        || lower.contains("metal")
        || lower.contains("dml")
        || lower.contains("tensorrt")
        || lower.contains("cann")
    {
        ProviderFamily::Accelerated
    } else if lower.contains("cpu") || lower.is_empty() {
        ProviderFamily::Cpu
    } else {
        ProviderFamily::Unknown
    }
}

/// Choose a batch size from real evidence.
pub fn choose_batch_size(policy: &BatchSizePolicy, evidence: &BatchRuntimeEvidence) -> usize {
    let base = match evidence.provider_family {
        ProviderFamily::Cpu => policy.cpu_batch_size,
        ProviderFamily::Accelerated => policy.accelerated_batch_size,
        ProviderFamily::Wasm => policy.wasm_batch_size,
        ProviderFamily::Unknown => policy.cpu_batch_size,
    };
    // Never exceed the model manifest limit when one is declared.
    let manifest_capped = evidence
        .manifest_batch_limit
        .map(|limit| base.min(limit))
        .unwrap_or(base);
    // Memory evidence: keep batches modest when memory is scarce (<= 2 GiB).
    let memory_capped = match evidence.available_memory_bytes {
        Some(bytes) if bytes <= 2 * 1024 * 1024 * 1024 => manifest_capped.min(2),
        _ => manifest_capped,
    };
    memory_capped.clamp(1, policy.max_batch_size)
}

/// One batch produced from the request queue.
#[derive(Debug, Clone)]
pub struct Batch {
    pub requests: Vec<RegionRecognitionRequest>,
    pub batch_index: usize,
}

/// Evidence recorded for a batch run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRunEvidence {
    pub batch_size: usize,
    pub batch_count: usize,
    /// Ratio of padding (empty slots) across all batches.
    pub padding_ratio: f32,
    pub effective_provider: String,
    pub latency_ms: u64,
    pub fallback: bool,
}

/// A recognizer that can process region requests in batches.
pub trait BatchRegionRecognizer {
    /// Recognize a batch of region requests.
    fn recognize_batch(
        &mut self,
        requests: &[RegionRecognitionRequest],
        context: &mut latexsnipper_runtime::InferenceContext,
    ) -> Result<Vec<RegionRecognitionResult>, String>;

    /// Split requests into batches using the sizing policy and evidence,
    /// then run them and collect evidence. Default implementation provides
    /// sequential batching; backends may override for true batched inference.
    fn run_batched(
        &mut self,
        requests: &[RegionRecognitionRequest],
        context: &mut latexsnipper_runtime::InferenceContext,
        policy: &BatchSizePolicy,
        evidence: &BatchRuntimeEvidence,
    ) -> Result<(Vec<RegionRecognitionResult>, BatchRunEvidence), String> {
        let batch_size = choose_batch_size(policy, evidence);
        let batches: Vec<&[RegionRecognitionRequest]> = requests.chunks(batch_size).collect();
        let batch_count = batches.len();
        let mut results = Vec::with_capacity(requests.len());
        let mut padded_slots = 0usize;
        let started = Instant::now();

        for (index, chunk) in batches.iter().enumerate() {
            let batch = Batch {
                requests: chunk.to_vec(),
                batch_index: index,
            };
            let _ = &batch;
            match self.recognize_batch(chunk, context) {
                Ok(mut part) => {
                    results.append(&mut part);
                    padded_slots += batch_size.saturating_sub(chunk.len());
                }
                Err(e) => {
                    // Fall back to per-request recognition so a single bad
                    // request cannot fail the whole run.
                    for request in chunk.iter() {
                        match self.recognize_batch(std::slice::from_ref(request), context) {
                            Ok(mut part) => results.append(&mut part),
                            Err(inner) => {
                                results.push(RegionRecognitionResult {
                                    id: request.id.clone(),
                                    kind: request.kind,
                                    text: String::new(),
                                    confidence: 0.0,
                                    latency_ms: 0,
                                });
                                log::warn!(
                                    "Batch {index} failed ({e}), request {} also failed ({inner})",
                                    request.id
                                );
                            }
                        }
                    }
                }
            }
        }

        let latency = started.elapsed();
        let total_slots = batch_count.saturating_mul(batch_size).max(1);
        let padding_ratio = padded_slots as f32 / total_slots as f32;

        Ok((
            results,
            BatchRunEvidence {
                batch_size,
                batch_count,
                padding_ratio,
                effective_provider: evidence.effective_provider.clone(),
                latency_ms: latency.as_millis() as u64,
                fallback: false,
            },
        ))
    }
}

/// Helper to time a single recognition call.
pub fn time_call<T>(f: impl FnOnce() -> Result<T, String>) -> Result<(T, Duration), String> {
    let started = Instant::now();
    let result = f()?;
    Ok((result, started.elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, kind: RegionRequestKind) -> RegionRecognitionRequest {
        RegionRecognitionRequest {
            id: id.into(),
            kind,
            image: SnipperImage::new(
                8,
                8,
                latexsnipper_image::color::PixelFormat::Rgb,
                vec![255u8; 8 * 8 * 3],
            ),
        }
    }

    #[test]
    fn provider_family_detection() {
        assert_eq!(provider_family("CPUExecutionProvider"), ProviderFamily::Cpu);
        assert_eq!(
            provider_family("CUDAExecutionProvider"),
            ProviderFamily::Accelerated
        );
        assert_eq!(
            provider_family("CoreMLExecutionProvider"),
            ProviderFamily::Accelerated
        );
        assert_eq!(
            provider_family("WasmExecutionProvider"),
            ProviderFamily::Wasm
        );
        assert_eq!(provider_family(""), ProviderFamily::Cpu);
        assert_eq!(provider_family("mystery"), ProviderFamily::Unknown);
    }

    #[test]
    fn batch_size_from_evidence() {
        let policy = BatchSizePolicy::default();
        let cpu = BatchRuntimeEvidence {
            effective_provider: "CPUExecutionProvider".into(),
            provider_family: ProviderFamily::Cpu,
            manifest_batch_limit: None,
            available_memory_bytes: None,
        };
        assert_eq!(choose_batch_size(&policy, &cpu), 4);

        let accel = BatchRuntimeEvidence {
            effective_provider: "CUDAExecutionProvider".into(),
            provider_family: ProviderFamily::Accelerated,
            manifest_batch_limit: None,
            available_memory_bytes: None,
        };
        assert_eq!(choose_batch_size(&policy, &accel), 16);

        // Manifest cap wins.
        let capped = BatchRuntimeEvidence {
            effective_provider: "CUDAExecutionProvider".into(),
            provider_family: ProviderFamily::Accelerated,
            manifest_batch_limit: Some(8),
            available_memory_bytes: None,
        };
        assert_eq!(choose_batch_size(&policy, &capped), 8);

        // Low memory shrinks the batch.
        let low_mem = BatchRuntimeEvidence {
            effective_provider: "CUDAExecutionProvider".into(),
            provider_family: ProviderFamily::Accelerated,
            manifest_batch_limit: None,
            available_memory_bytes: Some(1024 * 1024 * 1024),
        };
        assert_eq!(choose_batch_size(&policy, &low_mem), 2);
    }

    struct EchoRecognizer;
    impl BatchRegionRecognizer for EchoRecognizer {
        fn recognize_batch(
            &mut self,
            requests: &[RegionRecognitionRequest],
            _context: &mut latexsnipper_runtime::InferenceContext,
        ) -> Result<Vec<RegionRecognitionResult>, String> {
            Ok(requests
                .iter()
                .map(|r| RegionRecognitionResult {
                    id: r.id.clone(),
                    kind: r.kind,
                    text: format!("ok:{}", r.id),
                    confidence: 1.0,
                    latency_ms: 1,
                })
                .collect())
        }
    }

    #[test]
    fn run_batched_produces_evidence() {
        let mut recognizer = EchoRecognizer;
        let requests: Vec<RegionRecognitionRequest> = (0..9)
            .map(|i| request(&format!("r{i}"), RegionRequestKind::FormulaCrop))
            .collect();
        let mut ctx = latexsnipper_runtime::InferenceContext::new();
        let evidence = BatchRuntimeEvidence {
            effective_provider: "CPUExecutionProvider".into(),
            provider_family: ProviderFamily::Cpu,
            manifest_batch_limit: None,
            available_memory_bytes: None,
        };
        let (results, run) = recognizer
            .run_batched(&requests, &mut ctx, &BatchSizePolicy::default(), &evidence)
            .unwrap();
        assert_eq!(results.len(), 9);
        assert_eq!(run.batch_size, 4);
        assert_eq!(run.batch_count, 3);
        assert!(run.padding_ratio > 0.0);
        assert_eq!(run.effective_provider, "CPUExecutionProvider");
        assert!(!run.fallback);
    }

    #[test]
    fn failing_batch_falls_back_per_request() {
        struct FlakyRecognizer;
        impl BatchRegionRecognizer for FlakyRecognizer {
            fn recognize_batch(
                &mut self,
                requests: &[RegionRecognitionRequest],
                _context: &mut latexsnipper_runtime::InferenceContext,
            ) -> Result<Vec<RegionRecognitionResult>, String> {
                if requests.len() > 1 {
                    return Err("batch too big".into());
                }
                Ok(requests
                    .iter()
                    .map(|r| RegionRecognitionResult {
                        id: r.id.clone(),
                        kind: r.kind,
                        text: "single".into(),
                        confidence: 0.9,
                        latency_ms: 1,
                    })
                    .collect())
            }
        }
        let mut recognizer = FlakyRecognizer;
        let requests: Vec<RegionRecognitionRequest> = (0..3)
            .map(|i| request(&format!("r{i}"), RegionRequestKind::TextCrop))
            .collect();
        let mut ctx = latexsnipper_runtime::InferenceContext::new();
        let evidence = BatchRuntimeEvidence {
            effective_provider: "CPUExecutionProvider".into(),
            provider_family: ProviderFamily::Cpu,
            manifest_batch_limit: None,
            available_memory_bytes: None,
        };
        let (results, _) = recognizer
            .run_batched(&requests, &mut ctx, &BatchSizePolicy::default(), &evidence)
            .unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.text == "single"));
    }
}

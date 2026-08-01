use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use latexsnipper_api_types::{
    ProviderValidationKey, ProviderValidationLevel, ProviderValidationReport,
};
use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::{
    tensor_map_sha256, ExecutionProviderSpec, ProviderEnvironmentFingerprint, ProviderSmokeFixture,
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeProbe,
    TensorMap,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeEvidence {
    schema_version: u32,
    model: String,
    model_sha256: String,
    input_sha256: String,
    reference_provider: String,
    reference_output_sha256: Option<String>,
    tolerance: f32,
    validations: Vec<ProviderSmokeResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSmokeResult {
    #[serde(flatten)]
    validation: ProviderValidationReport,
    error_code: Option<String>,
    output_sha256: Option<String>,
    max_absolute_error: Option<f32>,
    session_create_ms: Option<f64>,
    inference_ms: Option<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let fixture_path =
        PathBuf::from(args.next().ok_or(
            "usage: provider_smoke FIXTURE OUTPUT [PROVIDERS_CSV] [BENCHMARK_PROVIDERS_CSV]",
        )?);
    let output =
        PathBuf::from(args.next().ok_or(
            "usage: provider_smoke FIXTURE OUTPUT [PROVIDERS_CSV] [BENCHMARK_PROVIDERS_CSV]",
        )?);
    let providers = args
        .next()
        .unwrap_or_else(|| "cpu,directml,cuda,coreml,tensorrt".to_owned())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let benchmark_requested = args
        .next()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    let fixture = ProviderSmokeFixture::load(&fixture_path)?;
    let model = fixture.model_path();
    let inputs = fixture.input_tensors()?;
    let input_sha256 = tensor_map_sha256(&inputs)?;

    let (cpu_result, cpu_outputs) = run_provider(
        &fixture,
        &model,
        "cpu",
        &inputs,
        None,
        benchmark_requested.contains("cpu"),
    );
    if !cpu_result.validation.smoke_inference_passed {
        let evidence = SmokeEvidence {
            schema_version: 1,
            model: fixture.model.path.display().to_string(),
            model_sha256: fixture.model_sha256().to_owned(),
            input_sha256,
            reference_provider: "cpu".to_owned(),
            reference_output_sha256: cpu_result.output_sha256.clone(),
            tolerance: fixture.tolerance.unwrap_or_default(),
            validations: vec![cpu_result],
        };
        std::fs::write(output, serde_json::to_vec_pretty(&evidence)?)?;
        return Err("CPU reference smoke inference failed".into());
    }
    let reference_hash = cpu_result.output_sha256.clone();
    let mut by_provider = BTreeMap::from([("cpu".to_owned(), cpu_result)]);
    for provider in providers {
        if provider == "cpu" {
            continue;
        }
        let (result, _) = run_provider(
            &fixture,
            &model,
            &provider,
            &inputs,
            cpu_outputs.as_ref(),
            benchmark_requested.contains(&provider),
        );
        by_provider.insert(provider, result);
    }
    let evidence = SmokeEvidence {
        schema_version: 1,
        model: fixture.model.path.display().to_string(),
        model_sha256: fixture.model_sha256().to_owned(),
        input_sha256,
        reference_provider: "cpu".to_owned(),
        reference_output_sha256: reference_hash,
        tolerance: fixture.tolerance.unwrap_or_default(),
        validations: by_provider.into_values().collect(),
    };
    std::fs::write(output, serde_json::to_vec_pretty(&evidence)?)?;
    Ok(())
}

fn run_provider(
    fixture: &ProviderSmokeFixture,
    model: &Path,
    provider: &str,
    inputs: &TensorMap,
    reference: Option<&TensorMap>,
    benchmark_requested: bool,
) -> (ProviderSmokeResult, Option<TensorMap>) {
    let base = ProviderValidationReport {
        provider: provider.to_owned(),
        validation_level: ProviderValidationLevel::Declared,
        library_detected: false,
        probe_passed: false,
        session_created: false,
        smoke_inference_passed: false,
        benchmark_measured: false,
        benchmark_validated: false,
        scope: latexsnipper_api_types::ValidationScope::CurrentProcess,
        reusable_across_restart: false,
        validated_at: 0,
        duration_ms: 0,
        runtime_instance_id: String::new(),
        session_generation: 0,
        last_failure_code: None,
        key: None,
        stale: false,
        diagnostics: Vec::new(),
    };
    let mut result = ProviderSmokeResult {
        validation: base,
        error_code: None,
        output_sha256: None,
        max_absolute_error: None,
        session_create_ms: None,
        inference_ms: None,
    };
    if !platform_supports(provider) {
        result.validation.diagnostics.push(format!(
            "provider is not supported on {}",
            std::env::consts::OS
        ));
        result.error_code = Some("PROVIDER_LIBRARY_NOT_FOUND".to_owned());
        return (result, None);
    }

    let factory = OnnxRuntimeFactory::new(
        model
            .parent()
            .expect("validated model path has a parent")
            .to_path_buf(),
    );
    let probe = factory.probe();
    result.validation.key = Some(provider_key(provider, &probe, fixture.model_sha256()));
    result.validation.library_detected = probe.available;
    if !probe.available {
        result.validation.diagnostics.push(
            probe
                .reason_unavailable
                .unwrap_or_else(|| "ONNX Runtime could not be loaded".to_owned()),
        );
        result.error_code = Some("PROVIDER_LOAD_FAILED".to_owned());
        return (result, None);
    }
    result.validation.validation_level = ProviderValidationLevel::LibraryDetected;
    let provider_detected = probe
        .capabilities
        .execution_providers
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(provider));
    if !provider_detected {
        result
            .validation
            .diagnostics
            .push("ONNX Runtime did not report this execution provider as available".to_owned());
        result.error_code = Some("PROVIDER_LIBRARY_NOT_FOUND".to_owned());
        return (result, None);
    }
    result.validation.probe_passed = true;
    result.validation.validation_level = ProviderValidationLevel::ProbePassed;

    let artifacts =
        RuntimeArtifacts::new(RuntimeKind::OnnxRuntime).with_file("model", model.to_path_buf());
    let options = RuntimeOptions {
        providers: vec![ExecutionProviderSpec::new(provider)],
        ..RuntimeOptions::default()
    };
    let session_started = Instant::now();
    let session = match factory.create_session(&artifacts, &options) {
        Ok(session) => session,
        Err(error) => {
            result
                .validation
                .diagnostics
                .push(redact_machine_paths(&error.to_string()));
            result.error_code = Some("PROVIDER_SESSION_CREATE_FAILED".to_owned());
            return (result, None);
        }
    };
    result.session_create_ms = Some(session_started.elapsed().as_secs_f64() * 1000.0);
    result.validation.session_created = true;
    result.validation.validation_level = ProviderValidationLevel::SessionCreated;

    let inference_started = Instant::now();
    let response = match session.run(RunRequest::new(inputs.clone())) {
        Ok(response) => response,
        Err(error) => {
            result
                .validation
                .diagnostics
                .push(redact_machine_paths(&error.to_string()));
            result.error_code = Some("PROVIDER_SMOKE_INFERENCE_FAILED".to_owned());
            return (result, None);
        }
    };
    result.inference_ms = Some(inference_started.elapsed().as_secs_f64() * 1000.0);
    if let Err(error) = fixture.validate_outputs(&response.outputs) {
        result
            .validation
            .diagnostics
            .push(redact_machine_paths(&error.to_string()));
        result.error_code = Some("PROVIDER_OUTPUT_MISMATCH".to_owned());
        return (result, Some(response.outputs));
    }
    result.output_sha256 = tensor_map_sha256(&response.outputs).ok();
    if let Some(reference) = reference {
        let tolerance = fixture.tolerance.unwrap_or_default();
        match max_absolute_error(reference, &response.outputs) {
            Some(error) if error <= tolerance => result.max_absolute_error = Some(error),
            Some(error) => {
                result.max_absolute_error = Some(error);
                result.error_code = Some("PROVIDER_OUTPUT_MISMATCH".to_owned());
                result.validation.diagnostics.push(format!(
                    "maximum absolute error {error} exceeds tolerance {tolerance}"
                ));
                return (result, Some(response.outputs));
            }
            None => {
                result.error_code = Some("PROVIDER_OUTPUT_MISMATCH".to_owned());
                result
                    .validation
                    .diagnostics
                    .push("output names, shapes, or dtypes differ from CPU".to_owned());
                return (result, Some(response.outputs));
            }
        }
    } else {
        result.max_absolute_error = Some(0.0);
    }
    result.validation.smoke_inference_passed = true;
    result.validation.benchmark_measured = benchmark_requested
        && (0..3).all(|_| {
            session
                .run(RunRequest::new(inputs.clone()))
                .and_then(|response| fixture.validate_outputs(&response.outputs))
                .is_ok()
        });
    result.validation.validation_level = if result.validation.benchmark_measured {
        ProviderValidationLevel::BenchmarkMeasured
    } else {
        ProviderValidationLevel::SmokeInferencePassed
    };
    (result, Some(response.outputs))
}

fn platform_supports(provider: &str) -> bool {
    match provider {
        "cpu" => true,
        "directml" => cfg!(target_os = "windows"),
        "cuda" | "tensorrt" => cfg!(any(target_os = "windows", target_os = "linux")),
        "coreml" => cfg!(target_os = "macos"),
        _ => false,
    }
}

fn max_absolute_error(reference: &TensorMap, actual: &TensorMap) -> Option<f32> {
    if reference.len() != actual.len() {
        return None;
    }
    let mut maximum = 0.0f32;
    for (name, expected) in reference {
        let observed = actual.get(name)?;
        if expected.shape() != observed.shape() || expected.dtype() != observed.dtype() {
            return None;
        }
        let expected = expected.as_f32_slice()?;
        let observed = observed.as_f32_slice()?;
        maximum = expected
            .iter()
            .zip(observed)
            .map(|(left, right)| (left - right).abs())
            .fold(maximum, f32::max);
    }
    Some(maximum)
}

fn provider_key(
    provider: &str,
    probe: &RuntimeProbe,
    smoke_model_sha256: &str,
) -> ProviderValidationKey {
    let fingerprint = ProviderEnvironmentFingerprint::collect(
        env!("CARGO_PKG_VERSION"),
        provider,
        probe,
        Some(smoke_model_sha256),
    );
    ProviderValidationKey {
        core_version: fingerprint.core_version,
        runtime_version: fingerprint.runtime_version,
        provider: fingerprint.provider,
        provider_library_fingerprint: fingerprint.provider_library_fingerprint,
        os: fingerprint.os,
        architecture: fingerprint.architecture,
        device_driver_fingerprint: fingerprint.device_driver_fingerprint,
        smoke_model_sha256: fingerprint.smoke_model_sha256,
        runtime_binary_sha256: fingerprint.runtime_binary_sha256,
        provider_library_sha256: fingerprint.provider_library_sha256,
        device_identity: fingerprint.device_identity,
    }
}

fn redact_machine_paths(message: &str) -> String {
    let mut redacted = message.to_owned();
    if let Ok(repo) = std::env::current_dir() {
        for spelling in [
            repo.display().to_string(),
            repo.display().to_string().replace('\\', "/"),
        ] {
            redacted = redacted.replace(&spelling, "<repo>");
        }
    }
    redacted
}

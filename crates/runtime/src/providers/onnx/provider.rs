use std::collections::BTreeMap;
use std::collections::BTreeSet;

use latexsnipper_foundation::{Result, SnipperError};
use ort::ep::{
    ArbitrarilyConfigurableExecutionProvider, ExecutionProvider, ExecutionProviderDispatch,
};
use ort::session::builder::SessionBuilder;
use serde_json::Value;

use super::platform::Acceleration;
use crate::{DeviceKind, ExecutionProviderSpec, RuntimeOptions};

pub(super) struct ConfiguredProviders {
    pub builder: SessionBuilder,
    pub selected: String,
    pub active: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub(super) fn configure(
    mut builder: SessionBuilder,
    options: &RuntimeOptions,
    detected: Acceleration,
) -> Result<ConfiguredProviders> {
    let specs = effective_provider_specs(options, detected);
    validate_provider_order(&specs)?;
    let allow_cpu = specs
        .iter()
        .any(|provider| provider.name.eq_ignore_ascii_case("cpu"));
    let mut active = Vec::new();
    let mut diagnostics = Vec::new();

    for spec in &specs {
        match provider_dispatch(spec)? {
            ProviderDispatch::Cpu => {}
            ProviderDispatch::Available { name, dispatch } => {
                match builder.with_execution_providers([dispatch.error_on_failure()]) {
                    Ok(next) => {
                        builder = next;
                        active.push(name.to_owned());
                    }
                    Err(error) => {
                        let message = format!(
                            "ONNX Runtime provider '{}' registration failed: {}; continuing with the declared fallback chain",
                            name.to_ascii_lowercase(),
                            error
                        );
                        builder = error.recover();
                        diagnostics.push(message);
                    }
                }
            }
            ProviderDispatch::Unavailable { name, reason } => diagnostics.push(format!(
                "ONNX Runtime provider '{name}' is unavailable: {reason}"
            )),
        }
    }

    if active.is_empty() && !allow_cpu {
        let requested = specs
            .iter()
            .map(|provider| provider.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let detail = if diagnostics.is_empty() {
            "no supported provider was configured".to_owned()
        } else {
            diagnostics.join("; ")
        };
        return Err(SnipperError::Runtime(format!(
            "none of the requested ONNX Runtime providers [{requested}] can run and CPU fallback was not declared: {detail}"
        )));
    }

    if !allow_cpu {
        builder = builder.with_disable_cpu_fallback().map_err(|error| {
            SnipperError::Runtime(format!(
                "failed to disable implicit ONNX Runtime CPU fallback: {error}"
            ))
        })?;
    } else {
        active.push("CPU".to_owned());
    }

    let selected = active.first().cloned().unwrap_or_else(|| "CPU".to_owned());
    Ok(ConfiguredProviders {
        builder,
        selected,
        active,
        diagnostics,
    })
}

fn validate_provider_order(specs: &[ExecutionProviderSpec]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (index, provider) in specs.iter().enumerate() {
        let name = canonical_provider_name(&provider.name);
        if !seen.insert(name.clone()) {
            return Err(SnipperError::Runtime(format!(
                "ONNX Runtime provider '{name}' is declared more than once"
            )));
        }
        if name == "cpu" && index + 1 != specs.len() {
            return Err(SnipperError::Runtime(
                "ONNX Runtime CPU provider must be the final fallback".to_owned(),
            ));
        }
    }
    Ok(())
}

fn effective_provider_specs(
    options: &RuntimeOptions,
    detected: Acceleration,
) -> Vec<ExecutionProviderSpec> {
    if !options.providers.is_empty() {
        return options.providers.clone();
    }

    match options.device {
        DeviceKind::Cpu => vec![ExecutionProviderSpec::cpu()],
        DeviceKind::Gpu => gpu_provider_specs(),
        DeviceKind::Npu => vec![
            ExecutionProviderSpec::new("qnn"),
            ExecutionProviderSpec::cpu(),
        ],
        DeviceKind::Auto => match detected {
            Acceleration::Tensorrt => vec![
                ExecutionProviderSpec::new("tensorrt"),
                ExecutionProviderSpec::new("cuda"),
                ExecutionProviderSpec::cpu(),
            ],
            Acceleration::Cuda12 | Acceleration::Cuda13 => vec![
                ExecutionProviderSpec::new("cuda"),
                ExecutionProviderSpec::cpu(),
            ],
            Acceleration::Directml => vec![
                ExecutionProviderSpec::new("directml"),
                ExecutionProviderSpec::cpu(),
            ],
            Acceleration::Coreml => vec![
                ExecutionProviderSpec::new("coreml"),
                ExecutionProviderSpec::cpu(),
            ],
            Acceleration::CpuOnly => vec![ExecutionProviderSpec::cpu()],
        },
    }
}

fn gpu_provider_specs() -> Vec<ExecutionProviderSpec> {
    vec![
        #[cfg(any(
            all(target_os = "windows", target_arch = "x86_64"),
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            )
        ))]
        ExecutionProviderSpec::new("tensorrt"),
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        ExecutionProviderSpec::new("cuda"),
        #[cfg(target_os = "windows")]
        ExecutionProviderSpec::new("directml"),
        #[cfg(target_vendor = "apple")]
        ExecutionProviderSpec::new("coreml"),
        ExecutionProviderSpec::cpu(),
    ]
}

enum ProviderDispatch {
    Cpu,
    Available {
        name: &'static str,
        dispatch: ExecutionProviderDispatch,
    },
    Unavailable {
        name: String,
        reason: String,
    },
}

fn provider_dispatch(spec: &ExecutionProviderSpec) -> Result<ProviderDispatch> {
    let name = canonical_provider_name(&spec.name);
    match name.as_str() {
        "cpu" => {
            if spec.options.is_empty() {
                Ok(ProviderDispatch::Cpu)
            } else {
                Err(invalid_provider_option(
                    "cpu",
                    "the CPU provider does not accept options",
                ))
            }
        }
        "tensorrt" => tensorrt_dispatch(&spec.options),
        "cuda" => cuda_dispatch(&spec.options),
        "directml" => directml_dispatch(&spec.options),
        "coreml" => coreml_dispatch(&spec.options),
        "openvino" | "qnn" => Ok(ProviderDispatch::Unavailable {
            name,
            reason: "this latexsnipper-runtime build does not enable that provider".to_owned(),
        }),
        _ => Err(SnipperError::Runtime(format!(
            "unknown ONNX Runtime execution provider '{}'",
            spec.name
        ))),
    }
}

fn canonical_provider_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
        .trim_end_matches("executionprovider")
        .to_owned()
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
fn tensorrt_dispatch(options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    let mut provider = ort::ep::TensorRT::default();
    for (key, value) in options {
        provider = provider.with_arbitrary_config(
            tensorrt_option_key(key)?,
            scalar_option_value("tensorrt", key, value)?,
        );
    }
    if !provider.is_available().unwrap_or(false) {
        return Ok(ProviderDispatch::Unavailable {
            name: "tensorrt".to_owned(),
            reason: "the loaded ONNX Runtime library does not expose TensorrtExecutionProvider"
                .to_owned(),
        });
    }
    Ok(ProviderDispatch::Available {
        name: "TensorRT",
        dispatch: provider.build(),
    })
}

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
fn tensorrt_dispatch(_options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    Ok(ProviderDispatch::Unavailable {
        name: "tensorrt".to_owned(),
        reason: "TensorRT EP is only supported on Windows x86_64 and Linux x86_64/aarch64"
            .to_owned(),
    })
}

fn tensorrt_option_key(key: &str) -> Result<&str> {
    let normalized = match key {
        "device_id" => "device_id",
        "max_workspace_size" => "trt_max_workspace_size",
        "min_subgraph_size" => "trt_min_subgraph_size",
        "max_partition_iterations" => "trt_max_partition_iterations",
        "fp16" | "fp16_enable" => "trt_fp16_enable",
        "int8" | "int8_enable" => "trt_int8_enable",
        "dla" | "dla_enable" => "trt_dla_enable",
        "dla_core" => "trt_dla_core",
        "engine_cache" | "engine_cache_enable" => "trt_engine_cache_enable",
        "engine_cache_path" => "trt_engine_cache_path",
        "engine_cache_prefix" => "trt_engine_cache_prefix",
        "timing_cache" | "timing_cache_enable" => "trt_timing_cache_enable",
        "timing_cache_path" => "trt_timing_cache_path",
        "force_timing_cache" => "trt_force_timing_cache",
        "force_sequential_engine_build" => "trt_force_sequential_engine_build",
        "context_memory_sharing" => "trt_context_memory_sharing_enable",
        "detailed_build_log" => "trt_detailed_build_log",
        "build_heuristics" => "trt_build_heuristics_enable",
        "sparsity" => "trt_sparsity_enable",
        "builder_optimization_level" => "trt_builder_optimization_level",
        "auxiliary_streams" => "trt_auxiliary_streams",
        "tactic_sources" => "trt_tactic_sources",
        "extra_plugin_lib_paths" => "trt_extra_plugin_lib_paths",
        "profile_min_shapes" => "trt_profile_min_shapes",
        "profile_opt_shapes" => "trt_profile_opt_shapes",
        "profile_max_shapes" => "trt_profile_max_shapes",
        "cuda_graph" | "cuda_graph_enable" => "trt_cuda_graph_enable",
        key if key.starts_with("trt_") => key,
        _ => {
            return Err(invalid_provider_option(
                "tensorrt",
                &format!("unsupported option '{key}'"),
            ));
        }
    };
    Ok(normalized)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn cuda_dispatch(options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    let mut provider = ort::ep::CUDA::default();
    for (key, value) in options {
        provider = provider.with_arbitrary_config(key, scalar_option_value("cuda", key, value)?);
    }
    if !provider.is_available().unwrap_or(false) {
        return Ok(ProviderDispatch::Unavailable {
            name: "cuda".to_owned(),
            reason: "the loaded ONNX Runtime library does not expose CUDAExecutionProvider"
                .to_owned(),
        });
    }
    Ok(ProviderDispatch::Available {
        name: "CUDA",
        dispatch: provider.build(),
    })
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn cuda_dispatch(_options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    Ok(ProviderDispatch::Unavailable {
        name: "cuda".to_owned(),
        reason: "CUDA EP is only supported on Windows and Linux".to_owned(),
    })
}

#[cfg(target_os = "windows")]
fn directml_dispatch(options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    let mut provider = ort::ep::DirectML::default();
    for (key, value) in options {
        match key.as_str() {
            "device_id" => {
                let device_id = value
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| {
                        invalid_provider_option("directml", "'device_id' must be a 32-bit integer")
                    })?;
                provider = provider.with_device_id(device_id);
            }
            _ => {
                return Err(invalid_provider_option(
                    "directml",
                    &format!("unsupported option '{key}'"),
                ));
            }
        }
    }
    if !provider.is_available().unwrap_or(false) {
        return Ok(ProviderDispatch::Unavailable {
            name: "directml".to_owned(),
            reason: "the loaded ONNX Runtime library does not expose DmlExecutionProvider"
                .to_owned(),
        });
    }
    Ok(ProviderDispatch::Available {
        name: "DirectML",
        dispatch: provider.build(),
    })
}

#[cfg(not(target_os = "windows"))]
fn directml_dispatch(_options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    Ok(ProviderDispatch::Unavailable {
        name: "directml".to_owned(),
        reason: "DirectML EP is only supported on Windows".to_owned(),
    })
}

#[cfg(target_os = "macos")]
fn coreml_dispatch(options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    let mut provider = ort::ep::CoreML::default();
    for (key, value) in options {
        provider = provider.with_arbitrary_config(key, scalar_option_value("coreml", key, value)?);
    }
    if !provider.is_available().unwrap_or(false) {
        return Ok(ProviderDispatch::Unavailable {
            name: "coreml".to_owned(),
            reason: "the loaded ONNX Runtime library does not expose CoreMLExecutionProvider"
                .to_owned(),
        });
    }
    Ok(ProviderDispatch::Available {
        name: "CoreML",
        dispatch: provider.build(),
    })
}

#[cfg(not(target_os = "macos"))]
fn coreml_dispatch(_options: &BTreeMap<String, Value>) -> Result<ProviderDispatch> {
    Ok(ProviderDispatch::Unavailable {
        name: "coreml".to_owned(),
        reason: "Core ML EP is only supported on macOS".to_owned(),
    })
}

fn scalar_option_value(provider: &str, key: &str, value: &Value) -> Result<String> {
    match value {
        Value::Bool(value) => Ok(if *value { "1" } else { "0" }.to_owned()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.clone()),
        Value::Null | Value::Array(_) | Value::Object(_) => Err(invalid_provider_option(
            provider,
            &format!("option '{key}' must be a string, number, or boolean"),
        )),
    }
}

fn invalid_provider_option(provider: &str, detail: &str) -> SnipperError {
    SnipperError::Runtime(format!(
        "invalid ONNX Runtime {provider} provider configuration: {detail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_provider_order_is_preserved() {
        let options = RuntimeOptions {
            providers: vec![
                ExecutionProviderSpec::new("tensorrt"),
                ExecutionProviderSpec::new("cuda"),
                ExecutionProviderSpec::cpu(),
            ],
            ..RuntimeOptions::default()
        };
        let providers = effective_provider_specs(&options, Acceleration::CpuOnly);
        let names: Vec<_> = providers
            .iter()
            .map(|provider| provider.name.as_str())
            .collect();
        assert_eq!(names, ["tensorrt", "cuda", "cpu"]);
    }

    #[test]
    fn automatic_tensorrt_plan_has_cuda_and_cpu_fallbacks() {
        let providers =
            effective_provider_specs(&RuntimeOptions::default(), Acceleration::Tensorrt);
        let names: Vec<_> = providers
            .iter()
            .map(|provider| provider.name.as_str())
            .collect();
        assert_eq!(names, ["tensorrt", "cuda", "cpu"]);
    }

    #[test]
    fn tensorrt_friendly_options_map_to_ort_keys() {
        assert_eq!(
            tensorrt_option_key("engine_cache_path").unwrap(),
            "trt_engine_cache_path"
        );
        assert_eq!(tensorrt_option_key("fp16").unwrap(), "trt_fp16_enable");
        assert_eq!(
            tensorrt_option_key("profile_opt_shapes").unwrap(),
            "trt_profile_opt_shapes"
        );
        assert_eq!(
            tensorrt_option_key("trt_engine_hw_compatible").unwrap(),
            "trt_engine_hw_compatible"
        );
    }

    #[test]
    fn nested_provider_option_is_rejected() {
        let error = scalar_option_value("tensorrt", "profile", &serde_json::json!({})).unwrap_err();
        assert!(error
            .to_string()
            .contains("must be a string, number, or boolean"));
    }

    #[test]
    fn canonical_provider_aliases_are_accepted() {
        assert_eq!(canonical_provider_name("TensorRT"), "tensorrt");
        assert_eq!(
            canonical_provider_name("TensorrtExecutionProvider"),
            "tensorrt"
        );
        assert_eq!(canonical_provider_name("Direct-ML"), "directml");
    }

    #[test]
    fn cpu_must_be_the_final_fallback() {
        let error = validate_provider_order(&[
            ExecutionProviderSpec::cpu(),
            ExecutionProviderSpec::new("cuda"),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("must be the final fallback"));
    }

    #[test]
    fn duplicate_provider_is_rejected_after_alias_normalization() {
        let error = validate_provider_order(&[
            ExecutionProviderSpec::new("TensorRT"),
            ExecutionProviderSpec::new("tensorrt-execution-provider"),
            ExecutionProviderSpec::cpu(),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("declared more than once"));
    }
}

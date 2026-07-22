//! Paddle-specific runtime options derived from common runtime options.

use std::path::PathBuf;

use latexsnipper_runtime::{DeviceKind, RuntimeOptions};

/// Paddle Inference configuration options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaddleOptions {
    /// Explicit LaTeXSnipper Paddle bridge library or packaged runtime root.
    pub library_path: Option<PathBuf>,
    pub enable_gpu: bool,
    pub gpu_device_id: i32,
    pub gpu_memory_pool_mb: u64,
    pub enable_memory_optim: bool,
    pub enable_ir_optim: bool,
    pub cpu_threads: usize,
}

impl Default for PaddleOptions {
    fn default() -> Self {
        Self {
            library_path: None,
            enable_gpu: false,
            gpu_device_id: 0,
            gpu_memory_pool_mb: 100,
            enable_memory_optim: false,
            enable_ir_optim: true,
            cpu_threads: 0,
        }
    }
}

impl PaddleOptions {
    pub fn from_runtime(options: &RuntimeOptions) -> Self {
        let first_provider = options
            .providers
            .first()
            .map(|provider| (provider.name.to_ascii_lowercase(), &provider.options));
        let enable_gpu = match first_provider.as_ref().map(|(name, _)| name.as_str()) {
            Some("cuda" | "gpu" | "paddle-gpu") => true,
            Some("cpu") => false,
            _ => options.device == DeviceKind::Gpu,
        };

        let provider_options = first_provider.as_ref().map(|(_, options)| *options);
        let extra = &options.extra;
        Self {
            library_path: string_option(extra, "libraryPath")
                .or_else(|| string_option(extra, "paddleHome"))
                .map(PathBuf::from),
            enable_gpu,
            gpu_device_id: integer_option(provider_options, "device_id")
                .or_else(|| integer_option(Some(extra), "gpuDeviceId"))
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(0),
            gpu_memory_pool_mb: integer_option(provider_options, "memory_pool_mb")
                .or_else(|| integer_option(Some(extra), "gpuMemoryPoolMb"))
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(100),
            enable_memory_optim: bool_option(extra, "enableMemoryOptim").unwrap_or(false),
            enable_ir_optim: options.graph_optimization,
            cpu_threads: options.max_threads,
        }
    }
}

fn string_option(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    options
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn integer_option(
    options: Option<&std::collections::BTreeMap<String, serde_json::Value>>,
    key: &str,
) -> Option<i64> {
    options?.get(key)?.as_i64()
}

fn bool_option(
    options: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<bool> {
    options.get(key).and_then(serde_json::Value::as_bool)
}

#[cfg(test)]
mod tests {
    use latexsnipper_runtime::{ExecutionProviderSpec, RuntimeOptions};

    use super::*;

    #[test]
    fn provider_order_controls_paddle_device() {
        let mut options = RuntimeOptions {
            providers: vec![ExecutionProviderSpec::new("cuda").with_option("device_id", 2)],
            max_threads: 6,
            ..RuntimeOptions::default()
        };
        options
            .extra
            .insert("gpuMemoryPoolMb".to_owned(), 256.into());

        let paddle = PaddleOptions::from_runtime(&options);
        assert!(paddle.enable_gpu);
        assert_eq!(paddle.gpu_device_id, 2);
        assert_eq!(paddle.gpu_memory_pool_mb, 256);
        assert_eq!(paddle.cpu_threads, 6);
    }
}

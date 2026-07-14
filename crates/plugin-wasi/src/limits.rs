use latexsnipper_plugin::PluginResourceLimitsV3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiResourceLimits {
    pub timeout_millis: u64,
    pub memory_bytes: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub diagnostic_count: usize,
    pub diagnostic_bytes: usize,
    pub model_artifact_bytes: usize,
    pub temporary_storage_bytes: usize,
    pub table_elements: u32,
    pub resources: usize,
    pub fuel: u64,
    pub max_concurrent_executions: usize,
}

impl Default for WasiResourceLimits {
    fn default() -> Self {
        Self {
            timeout_millis: 5_000,
            memory_bytes: 128 * 1024 * 1024,
            input_bytes: 16 * 1024 * 1024,
            output_bytes: 16 * 1024 * 1024,
            diagnostic_count: 256,
            diagnostic_bytes: 256 * 1024,
            model_artifact_bytes: 512 * 1024 * 1024,
            temporary_storage_bytes: 128 * 1024 * 1024,
            table_elements: 10_000,
            resources: 128,
            fuel: 50_000_000,
            max_concurrent_executions: 1,
        }
    }
}

impl WasiResourceLimits {
    pub fn from_manifest(value: &PluginResourceLimitsV3) -> Self {
        let defaults = Self::default();
        Self {
            timeout_millis: value.timeout_millis.unwrap_or(defaults.timeout_millis),
            memory_bytes: bounded_usize(value.memory_bytes, defaults.memory_bytes),
            input_bytes: bounded_usize(value.input_bytes, defaults.input_bytes),
            output_bytes: bounded_usize(value.output_bytes, defaults.output_bytes),
            diagnostic_count: value
                .diagnostic_count
                .map_or(defaults.diagnostic_count, |count| count as usize),
            diagnostic_bytes: bounded_usize(value.diagnostic_bytes, defaults.diagnostic_bytes),
            model_artifact_bytes: bounded_usize(
                value.model_artifact_bytes,
                defaults.model_artifact_bytes,
            ),
            temporary_storage_bytes: bounded_usize(
                value.temporary_storage_bytes,
                defaults.temporary_storage_bytes,
            ),
            table_elements: value.table_elements.unwrap_or(defaults.table_elements),
            resources: value
                .resources
                .map_or(defaults.resources, |count| count as usize),
            fuel: value.fuel.unwrap_or(defaults.fuel),
            max_concurrent_executions: value.max_concurrent_executions.max(1),
        }
    }
}

fn bounded_usize(value: Option<u64>, fallback: usize) -> usize {
    value
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(fallback)
}

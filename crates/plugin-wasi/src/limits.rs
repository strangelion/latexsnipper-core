use latexsnipper_plugin::PluginResourceLimitsV3;

use crate::{WasiDiagnostic, WasiDiagnosticCode};

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
    pub instances: usize,
    pub tables: usize,
    pub memories: usize,
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
            instances: 128,
            tables: 128,
            memories: 128,
            resources: 128,
            fuel: 50_000_000,
            max_concurrent_executions: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiResourceMinimums {
    pub timeout_millis: u64,
    pub memory_bytes: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub diagnostic_count: usize,
    pub diagnostic_bytes: usize,
    pub model_artifact_bytes: usize,
    pub temporary_storage_bytes: usize,
    pub table_elements: u32,
    pub instances: usize,
    pub tables: usize,
    pub memories: usize,
    pub resources: usize,
    pub fuel: u64,
    pub max_concurrent_executions: usize,
}

impl Default for WasiResourceMinimums {
    fn default() -> Self {
        Self {
            timeout_millis: 1,
            memory_bytes: 64 * 1024,
            input_bytes: 1,
            output_bytes: 1,
            diagnostic_count: 1,
            diagnostic_bytes: 1,
            model_artifact_bytes: 1,
            temporary_storage_bytes: 1,
            table_elements: 1,
            instances: 1,
            tables: 1,
            memories: 1,
            resources: 1,
            fuel: 1,
            max_concurrent_executions: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiHostPolicy {
    pub defaults: WasiResourceLimits,
    pub maximums: WasiResourceLimits,
    pub minimums: WasiResourceMinimums,
}

impl Default for WasiHostPolicy {
    fn default() -> Self {
        Self {
            defaults: WasiResourceLimits::default(),
            maximums: WasiResourceLimits {
                timeout_millis: 30_000,
                memory_bytes: 512 * 1024 * 1024,
                input_bytes: 64 * 1024 * 1024,
                output_bytes: 64 * 1024 * 1024,
                diagnostic_count: 4_096,
                diagnostic_bytes: 4 * 1024 * 1024,
                model_artifact_bytes: 1024 * 1024 * 1024,
                temporary_storage_bytes: 512 * 1024 * 1024,
                table_elements: 100_000,
                instances: 256,
                tables: 256,
                memories: 256,
                resources: 1_024,
                fuel: 500_000_000,
                max_concurrent_executions: 16,
            },
            minimums: WasiResourceMinimums::default(),
        }
    }
}

impl WasiResourceLimits {
    pub fn from_manifest(value: &PluginResourceLimitsV3) -> Result<Self, WasiDiagnostic> {
        WasiHostPolicy::default().grant(value)
    }
}

impl WasiHostPolicy {
    pub fn grant(
        &self,
        requested: &PluginResourceLimitsV3,
    ) -> Result<WasiResourceLimits, WasiDiagnostic> {
        self.validate()?;
        let requested_resources = requested_usize(
            "resources",
            requested.resources.map(u64::from),
            self.defaults.resources,
            self.minimums.resources,
        )?;
        Ok(WasiResourceLimits {
            timeout_millis: requested_u64(
                "timeoutMillis",
                requested.timeout_millis,
                self.defaults.timeout_millis,
                self.minimums.timeout_millis,
            )?
            .min(self.maximums.timeout_millis),
            memory_bytes: requested_usize(
                "memoryBytes",
                requested.memory_bytes,
                self.defaults.memory_bytes,
                self.minimums.memory_bytes,
            )?
            .min(self.maximums.memory_bytes),
            input_bytes: requested_usize(
                "inputBytes",
                requested.input_bytes,
                self.defaults.input_bytes,
                self.minimums.input_bytes,
            )?
            .min(self.maximums.input_bytes),
            output_bytes: requested_usize(
                "outputBytes",
                requested.output_bytes,
                self.defaults.output_bytes,
                self.minimums.output_bytes,
            )?
            .min(self.maximums.output_bytes),
            diagnostic_count: requested_usize(
                "diagnosticCount",
                requested.diagnostic_count.map(u64::from),
                self.defaults.diagnostic_count,
                self.minimums.diagnostic_count,
            )?
            .min(self.maximums.diagnostic_count),
            diagnostic_bytes: requested_usize(
                "diagnosticBytes",
                requested.diagnostic_bytes,
                self.defaults.diagnostic_bytes,
                self.minimums.diagnostic_bytes,
            )?
            .min(self.maximums.diagnostic_bytes),
            model_artifact_bytes: requested_usize(
                "modelArtifactBytes",
                requested.model_artifact_bytes,
                self.defaults.model_artifact_bytes,
                self.minimums.model_artifact_bytes,
            )?
            .min(self.maximums.model_artifact_bytes),
            temporary_storage_bytes: requested_usize(
                "temporaryStorageBytes",
                requested.temporary_storage_bytes,
                self.defaults.temporary_storage_bytes,
                self.minimums.temporary_storage_bytes,
            )?
            .min(self.maximums.temporary_storage_bytes),
            table_elements: requested_u32(
                "tableElements",
                requested.table_elements,
                self.defaults.table_elements,
                self.minimums.table_elements,
            )?
            .min(self.maximums.table_elements),
            instances: requested_resources.min(self.maximums.instances),
            tables: requested_resources.min(self.maximums.tables),
            memories: requested_resources.min(self.maximums.memories),
            resources: requested_resources.min(self.maximums.resources),
            fuel: requested_u64(
                "fuel",
                requested.fuel,
                self.defaults.fuel,
                self.minimums.fuel,
            )?
            .min(self.maximums.fuel),
            max_concurrent_executions: requested_usize(
                "maxConcurrentExecutions",
                Some(
                    u64::try_from(requested.max_concurrent_executions)
                        .map_err(|_| policy_error("maxConcurrentExecutions does not fit u64"))?,
                ),
                self.defaults.max_concurrent_executions,
                self.minimums.max_concurrent_executions,
            )?
            .min(self.maximums.max_concurrent_executions),
        })
    }

    fn validate(&self) -> Result<(), WasiDiagnostic> {
        validate_range(
            "timeoutMillis",
            self.minimums.timeout_millis,
            self.defaults.timeout_millis,
            self.maximums.timeout_millis,
        )?;
        validate_range(
            "memoryBytes",
            self.minimums.memory_bytes,
            self.defaults.memory_bytes,
            self.maximums.memory_bytes,
        )?;
        validate_range(
            "inputBytes",
            self.minimums.input_bytes,
            self.defaults.input_bytes,
            self.maximums.input_bytes,
        )?;
        validate_range(
            "outputBytes",
            self.minimums.output_bytes,
            self.defaults.output_bytes,
            self.maximums.output_bytes,
        )?;
        validate_range(
            "diagnosticCount",
            self.minimums.diagnostic_count,
            self.defaults.diagnostic_count,
            self.maximums.diagnostic_count,
        )?;
        validate_range(
            "diagnosticBytes",
            self.minimums.diagnostic_bytes,
            self.defaults.diagnostic_bytes,
            self.maximums.diagnostic_bytes,
        )?;
        validate_range(
            "modelArtifactBytes",
            self.minimums.model_artifact_bytes,
            self.defaults.model_artifact_bytes,
            self.maximums.model_artifact_bytes,
        )?;
        validate_range(
            "temporaryStorageBytes",
            self.minimums.temporary_storage_bytes,
            self.defaults.temporary_storage_bytes,
            self.maximums.temporary_storage_bytes,
        )?;
        validate_range(
            "tableElements",
            self.minimums.table_elements,
            self.defaults.table_elements,
            self.maximums.table_elements,
        )?;
        validate_range(
            "instances",
            self.minimums.instances,
            self.defaults.instances,
            self.maximums.instances,
        )?;
        validate_range(
            "tables",
            self.minimums.tables,
            self.defaults.tables,
            self.maximums.tables,
        )?;
        validate_range(
            "memories",
            self.minimums.memories,
            self.defaults.memories,
            self.maximums.memories,
        )?;
        validate_range(
            "resources",
            self.minimums.resources,
            self.defaults.resources,
            self.maximums.resources,
        )?;
        validate_range(
            "fuel",
            self.minimums.fuel,
            self.defaults.fuel,
            self.maximums.fuel,
        )?;
        validate_range(
            "maxConcurrentExecutions",
            self.minimums.max_concurrent_executions,
            self.defaults.max_concurrent_executions,
            self.maximums.max_concurrent_executions,
        )
    }
}

fn requested_u64(
    name: &str,
    value: Option<u64>,
    default: u64,
    minimum: u64,
) -> Result<u64, WasiDiagnostic> {
    let value = value.unwrap_or(default);
    if value < minimum {
        return Err(policy_error(format!(
            "requested {name} is below the host minimum"
        )));
    }
    Ok(value)
}

fn requested_u32(
    name: &str,
    value: Option<u32>,
    default: u32,
    minimum: u32,
) -> Result<u32, WasiDiagnostic> {
    let value = value.unwrap_or(default);
    if value < minimum {
        return Err(policy_error(format!(
            "requested {name} is below the host minimum"
        )));
    }
    Ok(value)
}

fn requested_usize(
    name: &str,
    value: Option<u64>,
    default: usize,
    minimum: usize,
) -> Result<usize, WasiDiagnostic> {
    let value = match value {
        Some(value) => usize::try_from(value)
            .map_err(|_| policy_error(format!("requested {name} does not fit usize")))?,
        None => default,
    };
    if value < minimum {
        return Err(policy_error(format!(
            "requested {name} is below the host minimum"
        )));
    }
    Ok(value)
}

fn validate_range<T>(name: &str, minimum: T, default: T, maximum: T) -> Result<(), WasiDiagnostic>
where
    T: Copy + Ord + Default,
{
    if minimum == T::default() || minimum > default || default > maximum {
        return Err(policy_error(format!(
            "host policy has an invalid {name} minimum/default/maximum range"
        )));
    }
    Ok(())
}

fn policy_error(message: impl Into<String>) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiResourcePolicy, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested() -> PluginResourceLimitsV3 {
        PluginResourceLimitsV3 {
            timeout_millis: None,
            memory_bytes: None,
            input_bytes: None,
            output_bytes: None,
            diagnostic_count: None,
            diagnostic_bytes: None,
            model_artifact_bytes: None,
            temporary_storage_bytes: None,
            table_elements: None,
            resources: None,
            fuel: None,
            max_concurrent_executions: 1,
        }
    }

    #[test]
    fn defaults_are_used_only_when_values_are_absent() {
        let policy = WasiHostPolicy::default();
        assert_eq!(policy.grant(&requested()).unwrap(), policy.defaults);
    }

    #[test]
    fn requests_are_clamped_to_host_maximums() {
        let mut value = requested();
        value.timeout_millis = Some(u64::MAX);
        value.memory_bytes = Some(WasiHostPolicy::default().maximums.memory_bytes as u64 + 1);
        value.fuel = Some(u64::MAX);
        value.max_concurrent_executions = usize::MAX;
        let limits = WasiHostPolicy::default().grant(&value).unwrap();
        let maximums = WasiHostPolicy::default().maximums;
        assert_eq!(limits.timeout_millis, maximums.timeout_millis);
        assert_eq!(limits.memory_bytes, maximums.memory_bytes);
        assert_eq!(limits.fuel, maximums.fuel);
        assert_eq!(
            limits.max_concurrent_executions,
            maximums.max_concurrent_executions
        );
    }

    #[test]
    fn zero_below_minimum_and_overflow_are_rejected() {
        let policy = WasiHostPolicy::default();
        let mut value = requested();
        value.timeout_millis = Some(0);
        assert!(policy.grant(&value).is_err());

        value = requested();
        value.memory_bytes = Some(1);
        assert!(policy.grant(&value).is_err());

        if usize::BITS < 64 {
            value = requested();
            value.output_bytes = Some(u64::MAX);
            assert!(policy.grant(&value).is_err());
        }
    }

    #[test]
    fn a_stricter_host_policy_wins_over_the_manifest() {
        let mut policy = WasiHostPolicy::default();
        policy.maximums.memory_bytes = 8 * 1024 * 1024;
        policy.defaults.memory_bytes = 4 * 1024 * 1024;
        let mut value = requested();
        value.memory_bytes = Some(64 * 1024 * 1024);
        assert_eq!(policy.grant(&value).unwrap().memory_bytes, 8 * 1024 * 1024);
    }
}

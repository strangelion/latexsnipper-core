use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};

use crate::manifest::{PluginHook, PluginPermissions};

/// Execution boundary used by a plugin implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginExecutionClass {
    /// Trusted Rust code in the host process. Timeouts are cooperative and soft.
    TrustedInProcess,
    /// A versioned child-process protocol that the host can terminate.
    IsolatedProcess,
    /// A WASI Component instance with host-controlled resources.
    WasiComponent,
}

/// A cheap cancellation signal shared between the host and cooperative plugins.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(plugin_error(
                "PLUGIN_CANCELLED: Plugin execution was cancelled",
            ));
        }
        Ok(())
    }

    pub(crate) fn same_signal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }
}

/// A typed diagnostic emitted by the execution context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionNote {
    pub code: String,
    pub message: String,
}

/// Thread-safe diagnostic collector available to cooperative plugins.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticSink {
    notes: Arc<Mutex<Vec<PluginExecutionNote>>>,
}

impl DiagnosticSink {
    pub fn push(&self, code: impl Into<String>, message: impl Into<String>) {
        if let Ok(mut notes) = self.notes.lock() {
            notes.push(PluginExecutionNote {
                code: code.into(),
                message: message.into(),
            });
        }
    }

    pub fn snapshot(&self) -> Vec<PluginExecutionNote> {
        self.notes
            .lock()
            .map(|notes| notes.clone())
            .unwrap_or_default()
    }
}

/// Canonical policy for host-brokered plugin operations.
///
/// These checks do not OS-sandbox arbitrary filesystem or network calls made by
/// a native isolated-process plugin.
#[derive(Debug, Clone)]
pub struct EffectivePluginPermissions {
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    network_hosts: Vec<String>,
    environment_variables: Vec<String>,
    model_access: Vec<String>,
    temporary_directory: bool,
    capability_registration: bool,
    importer_registration: bool,
    exporter_registration: bool,
    runtime_registration: bool,
    pub memory_limit_bytes: Option<u64>,
    pub output_limit_bytes: Option<u64>,
    pub timeout_millis: Option<u64>,
    pub max_concurrent_executions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectivePermissionSummary {
    pub enforcement_scope: &'static str,
    pub native_process_os_sandboxed: bool,
    pub filesystem_read_root_count: usize,
    pub filesystem_write_root_count: usize,
    pub network_host_count: usize,
    pub environment_variable_count: usize,
    pub model_grant_count: usize,
    pub temporary_directory: bool,
    pub capability_registration: bool,
    pub importer_registration: bool,
    pub exporter_registration: bool,
    pub runtime_registration: bool,
    pub memory_limit_bytes: Option<u64>,
    pub output_limit_bytes: Option<u64>,
    pub timeout_millis: Option<u64>,
    pub max_concurrent_executions: usize,
}

impl EffectivePluginPermissions {
    pub fn from_manifest(permissions: &PluginPermissions, base: &Path) -> Result<Self> {
        let mut read_paths = permissions.filesystem_paths.clone();
        read_paths.extend(permissions.filesystem_read_paths.iter().cloned());
        Ok(Self {
            read_roots: canonical_roots(&read_paths, base)?,
            write_roots: canonical_roots(&permissions.filesystem_write_paths, base)?,
            network_hosts: normalize_values(&permissions.network_hosts),
            environment_variables: normalize_values(&permissions.environment_variables),
            model_access: normalize_values(&permissions.model_access),
            temporary_directory: permissions.temporary_directory,
            capability_registration: permissions.capability_registration,
            importer_registration: permissions.importer_registration,
            exporter_registration: permissions.exporter_registration,
            runtime_registration: permissions.runtime_registration,
            memory_limit_bytes: permissions.memory_limit_bytes,
            output_limit_bytes: permissions.output_limit_bytes,
            timeout_millis: permissions.timeout_millis,
            max_concurrent_executions: permissions.max_concurrent_executions.max(1),
        })
    }

    pub fn deny_all() -> Self {
        Self {
            read_roots: Vec::new(),
            write_roots: Vec::new(),
            network_hosts: Vec::new(),
            environment_variables: Vec::new(),
            model_access: Vec::new(),
            temporary_directory: false,
            capability_registration: false,
            importer_registration: false,
            exporter_registration: false,
            runtime_registration: false,
            memory_limit_bytes: None,
            output_limit_bytes: None,
            timeout_millis: None,
            max_concurrent_executions: 1,
        }
    }

    pub fn summary(&self) -> EffectivePermissionSummary {
        EffectivePermissionSummary {
            enforcement_scope: "brokered-host-operations",
            native_process_os_sandboxed: false,
            filesystem_read_root_count: self.read_roots.len(),
            filesystem_write_root_count: self.write_roots.len(),
            network_host_count: self.network_hosts.len(),
            environment_variable_count: self.environment_variables.len(),
            model_grant_count: self.model_access.len(),
            temporary_directory: self.temporary_directory,
            capability_registration: self.capability_registration,
            importer_registration: self.importer_registration,
            exporter_registration: self.exporter_registration,
            runtime_registration: self.runtime_registration,
            memory_limit_bytes: self.memory_limit_bytes,
            output_limit_bytes: self.output_limit_bytes,
            timeout_millis: self.timeout_millis,
            max_concurrent_executions: self.max_concurrent_executions,
        }
    }

    pub fn check_read_path(&self, path: &Path) -> Result<PathBuf> {
        check_path(path, &self.read_roots, "filesystem read")
    }

    pub fn check_write_path(&self, path: &Path) -> Result<PathBuf> {
        check_path(path, &self.write_roots, "filesystem write")
    }

    pub fn check_network_host(&self, host: &str) -> Result<()> {
        check_value(host, &self.network_hosts, "network host")
    }

    pub fn environment_value(&self, name: &str) -> Result<Option<String>> {
        check_value(name, &self.environment_variables, "environment variable")?;
        Ok(std::env::var(name).ok())
    }

    pub fn check_model_access(&self, model: &str) -> Result<()> {
        check_value(model, &self.model_access, "model")
    }

    pub fn check_temporary_directory(&self) -> Result<()> {
        check_flag(self.temporary_directory, "temporary directory")
    }

    pub fn check_hook_registration(&self, hook: PluginHook) -> Result<()> {
        let allowed = match hook {
            PluginHook::RegisterImporter => self.importer_registration,
            PluginHook::RegisterExporter => self.exporter_registration,
            PluginHook::RegisterRuntime => self.runtime_registration,
            PluginHook::RegisterModelAdapter => self.capability_registration,
            _ => true,
        };
        check_flag(allowed, "capability registration")
    }
}

/// Context passed to trusted plugins that support cooperative cancellation.
#[derive(Debug, Clone)]
pub struct PluginExecutionContext {
    pub cancellation: CancellationToken,
    pub deadline: Option<Instant>,
    pub permissions: EffectivePluginPermissions,
    pub diagnostics: DiagnosticSink,
}

impl PluginExecutionContext {
    pub fn check_cancelled(&self) -> Result<()> {
        self.cancellation.check_cancelled()
    }

    pub fn check_deadline(&self) -> Result<()> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.cancellation.cancel();
            return Err(plugin_error(
                "PLUGIN_SOFT_TIMEOUT: Plugin execution deadline exceeded",
            ));
        }
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<()> {
        self.check_cancelled()?;
        self.check_deadline()
    }
}

fn canonical_roots(values: &[String], base: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::with_capacity(values.len());
    for value in values {
        let relative = Path::new(value);
        if relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(permission_error("Permission path contains '..' traversal"));
        }
        let path = if relative.is_absolute() {
            relative.to_path_buf()
        } else {
            base.join(relative)
        };
        let canonical = path.canonicalize().map_err(|error| {
            permission_error(format!("Could not canonicalize permission path: {error}"))
        })?;
        roots.push(canonical);
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn normalize_values(values: &[String]) -> Vec<String> {
    let mut normalized: Vec<_> = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn check_path(path: &Path, roots: &[PathBuf], operation: &str) -> Result<PathBuf> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(permission_error(format!(
            "Denied {operation}: path contains traversal"
        )));
    }
    let canonical = canonicalize_target(path)?;
    if !roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(permission_error(format!(
            "Denied {operation} outside granted roots"
        )));
    }
    Ok(canonical)
}

fn canonicalize_target(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|error| permission_error(error.to_string()));
    }
    let parent = path
        .parent()
        .ok_or_else(|| permission_error("Permission target has no parent"))?
        .canonicalize()
        .map_err(|error| permission_error(error.to_string()))?;
    let name = path
        .file_name()
        .ok_or_else(|| permission_error("Permission target has no file name"))?;
    Ok(parent.join(name))
}

fn check_value(value: &str, allowed: &[String], kind: &str) -> Result<()> {
    let value = value.trim().to_ascii_lowercase();
    if allowed.iter().any(|candidate| candidate == &value) {
        return Ok(());
    }
    Err(permission_error(format!("Denied {kind} '{value}'")))
}

fn check_flag(allowed: bool, operation: &str) -> Result<()> {
    if allowed {
        Ok(())
    } else {
        Err(permission_error(format!("Denied {operation}")))
    }
}

fn permission_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(format!("PLUGIN_PERMISSION_DENIED: {}", message.into()))
}

fn plugin_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_permissions_deny_ambient_access() {
        let permissions = EffectivePluginPermissions::deny_all();
        assert!(permissions.check_network_host("example.com").is_err());
        assert!(permissions.environment_value("PATH").is_err());
        assert!(permissions.check_model_access("formula-rec").is_err());
        assert!(permissions.check_temporary_directory().is_err());
        assert!(permissions
            .check_hook_registration(PluginHook::RegisterExporter)
            .is_err());
    }

    #[test]
    fn traversal_is_rejected_before_canonicalization() {
        let permissions = EffectivePluginPermissions::deny_all();
        assert!(permissions.check_read_path(Path::new("../secret")).is_err());
        assert!(permissions
            .check_write_path(Path::new("../output"))
            .is_err());
    }

    #[test]
    fn explicit_grants_are_narrow_and_registration_is_typed() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper-permissions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let read_root = root.join("read");
        let write_root = root.join("write");
        std::fs::create_dir_all(&read_root).unwrap();
        std::fs::create_dir_all(&write_root).unwrap();
        std::fs::write(read_root.join("input.txt"), b"fixture").unwrap();
        let permissions = PluginPermissions {
            filesystem_read_paths: vec![read_root.to_string_lossy().to_string()],
            filesystem_write_paths: vec![write_root.to_string_lossy().to_string()],
            network_hosts: vec!["models.example.invalid".to_string()],
            environment_variables: vec!["LATEXSNIPPER_TEST_UNSET".to_string()],
            model_access: vec!["formula-rec".to_string()],
            temporary_directory: true,
            exporter_registration: true,
            ..PluginPermissions::default()
        };
        let effective = EffectivePluginPermissions::from_manifest(&permissions, &root).unwrap();

        assert!(effective
            .check_read_path(&read_root.join("input.txt"))
            .is_ok());
        assert!(effective
            .check_write_path(&write_root.join("output.txt"))
            .is_ok());
        assert!(effective.check_read_path(&write_root).is_err());
        assert!(effective
            .check_write_path(&read_root.join("new.txt"))
            .is_err());
        assert!(effective
            .check_network_host("models.example.invalid")
            .is_ok());
        assert!(effective
            .check_network_host("other.example.invalid")
            .is_err());
        assert!(effective
            .environment_value("LATEXSNIPPER_TEST_UNSET")
            .is_ok());
        assert!(effective.environment_value("PATH").is_err());
        assert!(effective.check_model_access("formula-rec").is_ok());
        assert!(effective.check_model_access("text-rec").is_err());
        assert!(effective.check_temporary_directory().is_ok());
        assert!(effective
            .check_hook_registration(PluginHook::RegisterExporter)
            .is_ok());
        assert!(effective
            .check_hook_registration(PluginHook::RegisterImporter)
            .is_err());
        assert!(effective
            .check_hook_registration(PluginHook::RegisterRuntime)
            .is_err());
        assert!(effective
            .check_hook_registration(PluginHook::RegisterModelAdapter)
            .is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "latexsnipper-permission-symlink-{}",
            std::process::id()
        ));
        let granted = root.join("granted");
        let outside = root.join("outside");
        std::fs::create_dir_all(&granted).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"fixture").unwrap();
        symlink(&outside, granted.join("escape")).unwrap();
        let permissions = PluginPermissions {
            filesystem_read_paths: vec![granted.to_string_lossy().to_string()],
            ..PluginPermissions::default()
        };
        let effective = EffectivePluginPermissions::from_manifest(&permissions, &root).unwrap();
        assert!(effective
            .check_read_path(&granted.join("escape").join("secret"))
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use latexsnipper_ast::CapabilityMatrix;
use latexsnipper_foundation::{Result, SnipperError};
use log::info;
use serde::Serialize;

use crate::execution::{
    CancellationToken, DiagnosticSink, EffectivePluginPermissions, PluginExecutionClass,
    PluginExecutionContext,
};
use crate::manifest::{
    legacy_core_requirement_matches, PluginClass, PluginHook, PluginManifest, PLUGIN_API_VERSION,
};
use crate::patch::{DocumentPatch, DocumentView};
use crate::plugin::Plugin;
use crate::request::PluginRequest;
use crate::response::PluginResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFailurePolicy {
    Stop,
    Continue,
    DisablePlugin,
    Rollback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiagnostic {
    pub plugin_id: String,
    pub code: &'static str,
    pub message: String,
    pub panic_contained: bool,
    pub disabled: bool,
    pub soft_timeout: bool,
    pub execution_may_still_be_running: bool,
    pub execution_class: PluginExecutionClass,
}

impl PluginDiagnostic {
    fn new(
        plugin_id: impl Into<String>,
        code: &'static str,
        message: impl Into<String>,
        execution_class: PluginExecutionClass,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            code,
            message: message.into(),
            panic_contained: false,
            disabled: false,
            soft_timeout: false,
            execution_may_still_be_running: false,
            execution_class,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginRunResult {
    pub response: PluginResponse,
    pub diagnostics: Vec<PluginDiagnostic>,
    pub executed: Vec<String>,
    pub rolled_back: bool,
}

struct PluginEntry {
    plugin: Arc<RwLock<Box<dyn Plugin>>>,
    manifest: PluginManifest,
    registration_order: u64,
    enabled: AtomicBool,
    execution_class: PluginExecutionClass,
    permissions: EffectivePluginPermissions,
    runtime: Arc<PluginRuntimeState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionStatus {
    pub execution_class: PluginExecutionClass,
    pub enabled: bool,
    pub quarantined: bool,
    pub outstanding_executions: usize,
    pub max_concurrent_executions: usize,
}

struct PluginRuntimeState {
    quarantined: AtomicBool,
    outstanding: AtomicUsize,
    max_concurrent: usize,
    cancellation_tokens: Mutex<Vec<CancellationToken>>,
}

impl PluginRuntimeState {
    fn new(max_concurrent: usize) -> Self {
        Self {
            quarantined: AtomicBool::new(false),
            outstanding: AtomicUsize::new(0),
            max_concurrent: max_concurrent.max(1),
            cancellation_tokens: Mutex::new(Vec::new()),
        }
    }

    fn begin(
        self: &Arc<Self>,
        plugin_id: &str,
        execution_class: PluginExecutionClass,
        token: CancellationToken,
    ) -> std::result::Result<PluginExecutionGuard, PluginDiagnostic> {
        if self.quarantined.load(Ordering::Acquire) {
            return Err(PluginDiagnostic::new(
                plugin_id,
                "PLUGIN_QUARANTINED",
                format!("Plugin '{plugin_id}' is quarantined after a timed-out execution"),
                execution_class,
            ));
        }
        if self
            .outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.max_concurrent).then_some(current + 1)
            })
            .is_err()
        {
            return Err(PluginDiagnostic::new(
                plugin_id,
                "PLUGIN_CONCURRENCY_LIMIT",
                format!(
                    "Plugin '{plugin_id}' reached its {} execution limit",
                    self.max_concurrent
                ),
                execution_class,
            ));
        }
        if let Ok(mut tokens) = self.cancellation_tokens.lock() {
            tokens.push(token.clone());
        }
        Ok(PluginExecutionGuard {
            runtime: Arc::clone(self),
            token,
        })
    }

    fn cancel_all(&self) {
        if let Ok(tokens) = self.cancellation_tokens.lock() {
            for token in tokens.iter() {
                token.cancel();
            }
        }
    }
}

struct PluginExecutionGuard {
    runtime: Arc<PluginRuntimeState>,
    token: CancellationToken,
}

impl Drop for PluginExecutionGuard {
    fn drop(&mut self) {
        if let Ok(mut tokens) = self.runtime.cancellation_tokens.lock() {
            tokens.retain(|token| !token.same_signal(&self.token));
        }
        self.runtime.outstanding.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct PluginRegistry {
    plugins: HashMap<String, PluginEntry>,
    next_registration_order: u64,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            next_registration_order: 0,
        }
    }

    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<()> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        if self.plugins.contains_key(&id) || self.plugins.contains_key(plugin.name()) {
            return Err(plugin_error(format!("Plugin '{id}' is already registered")));
        }
        validate_manifest(&manifest, plugin.name(), plugin.version())?;
        if manifest.class != PluginClass::BuiltInRust {
            return Err(plugin_error(format!(
                "Plugin '{}' uses {:?} and must be loaded by an external execution host",
                manifest.id, manifest.class
            )));
        }
        let permission_base = std::env::current_dir()
            .map_err(|error| plugin_error(format!("Could not resolve permission base: {error}")))?;
        let permissions =
            EffectivePluginPermissions::from_manifest(&manifest.permissions, &permission_base)?;
        let execution_class = PluginExecutionClass::TrustedInProcess;
        let runtime = Arc::new(PluginRuntimeState::new(
            permissions.max_concurrent_executions,
        ));

        let mut prospective = self.manifest_records();
        prospective.push((id.clone(), manifest.clone(), self.next_registration_order));
        ordered_records(&prospective)?;

        info!("Registering plugin: {} v{}", id, manifest.version);
        catch_plugin_panic(&id, "init", || plugin.init())?;
        self.plugins.insert(
            id,
            PluginEntry {
                plugin: Arc::new(RwLock::new(plugin)),
                manifest,
                registration_order: self.next_registration_order,
                enabled: AtomicBool::new(true),
                execution_class,
                permissions,
                runtime,
            },
        );
        self.next_registration_order = self.next_registration_order.saturating_add(1);
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> Result<()> {
        let Some(entry) = self.plugins.get_mut(id) else {
            return Ok(());
        };
        let outstanding = entry.runtime.outstanding.load(Ordering::Acquire);
        if outstanding != 0 {
            return Err(plugin_error(format!(
                "Plugin '{id}' cannot be unregistered while {outstanding} execution(s) remain"
            )));
        }
        info!("Unregistering plugin: {id}");
        let mut plugin = entry
            .plugin
            .write()
            .map_err(|_| plugin_error(format!("Plugin '{id}' lock is poisoned")))?;
        catch_plugin_panic(id, "cleanup", || plugin.cleanup())?;
        drop(plugin);
        self.plugins.remove(id);
        Ok(())
    }

    pub fn has(&self, id: &str) -> bool {
        self.plugins.contains_key(id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.plugins
            .get(id)
            .is_some_and(|entry| entry.enabled.load(Ordering::Acquire))
    }

    pub fn enable(&self, id: &str) -> Result<()> {
        let entry = self
            .plugins
            .get(id)
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' not found")))?;
        entry.enabled.store(true, Ordering::Release);
        Ok(())
    }

    pub fn disable(&self, id: &str) -> Result<()> {
        let entry = self
            .plugins
            .get(id)
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' not found")))?;
        entry.enabled.store(false, Ordering::Release);
        entry.runtime.cancel_all();
        Ok(())
    }

    pub fn execution_status(&self, id: &str) -> Option<PluginExecutionStatus> {
        self.plugins.get(id).map(|entry| PluginExecutionStatus {
            execution_class: entry.execution_class,
            enabled: entry.enabled.load(Ordering::Acquire),
            quarantined: entry.runtime.quarantined.load(Ordering::Acquire),
            outstanding_executions: entry.runtime.outstanding.load(Ordering::Acquire),
            max_concurrent_executions: entry.runtime.max_concurrent,
        })
    }

    /// Clear a soft-timeout quarantine after every outstanding worker has exited.
    pub fn reset_quarantine(&self, id: &str) -> Result<()> {
        let entry = self
            .plugins
            .get(id)
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' not found")))?;
        let outstanding = entry.runtime.outstanding.load(Ordering::Acquire);
        if outstanding != 0 {
            return Err(plugin_error(format!(
                "Plugin '{id}' still has {outstanding} outstanding execution(s)"
            )));
        }
        entry.runtime.quarantined.store(false, Ordering::Release);
        Ok(())
    }

    pub fn manifest(&self, id: &str) -> Option<&PluginManifest> {
        self.plugins.get(id).map(|entry| &entry.manifest)
    }

    pub fn list(&self) -> Vec<&str> {
        let order = self.execution_order().unwrap_or_else(|_| {
            let mut fallback: Vec<_> = self.plugins.keys().cloned().collect();
            fallback.sort();
            fallback
        });
        order
            .into_iter()
            .filter_map(|id| self.plugins.get_key_value(&id).map(|(key, _)| key.as_str()))
            .collect()
    }

    pub fn execution_order(&self) -> Result<Vec<String>> {
        ordered_records(&self.manifest_records())
    }

    pub fn capabilities(&self) -> Vec<String> {
        let mut capabilities: Vec<_> = self
            .plugins
            .values()
            .filter(|entry| entry.enabled.load(Ordering::Acquire))
            .flat_map(|entry| entry.manifest.capabilities.iter().cloned())
            .collect();
        capabilities.sort();
        capabilities.dedup();
        capabilities
    }

    /// Merge enabled plugin capabilities into the shared executable matrix.
    pub fn extend_capability_matrix(&self, matrix: &mut CapabilityMatrix) {
        for id in self.execution_order().unwrap_or_default() {
            let entry = &self.plugins[&id];
            if entry.enabled.load(Ordering::Acquire) {
                matrix
                    .entries
                    .extend(entry.manifest.format_capabilities.iter().cloned());
            }
        }
    }

    pub fn handle(&self, plugin_id: &str, request: &PluginRequest) -> Result<PluginResponse> {
        let entry = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| plugin_error(format!("Plugin '{plugin_id}' not found")))?;
        if !entry.enabled.load(Ordering::Acquire) {
            return Err(plugin_error(format!("Plugin '{plugin_id}' is disabled")));
        }
        match invoke_plugin(plugin_id, entry, request)
            .map_err(|diagnostic| plugin_error(diagnostic.message))?
        {
            PluginExecution::Response(response) => Ok(*response),
            PluginExecution::Patch(patch) => {
                let mut document = request.document.clone();
                patch.apply(&mut document)?;
                Ok(PluginResponse {
                    document,
                    metadata: request.metadata.clone(),
                })
            }
        }
    }

    pub fn handle_all(&self, request: &PluginRequest) -> Result<PluginResponse> {
        self.handle_all_with_policy(request, PluginFailurePolicy::Stop)
            .map(|result| result.response)
    }

    pub fn handle_all_with_policy(
        &self,
        request: &PluginRequest,
        policy: PluginFailurePolicy,
    ) -> Result<PluginRunResult> {
        self.handle_matching(request, policy, |_| true)
    }

    pub fn handle_filtered(
        &self,
        request: &PluginRequest,
        filter: impl Fn(&str) -> bool,
    ) -> Result<PluginResponse> {
        self.handle_matching(request, PluginFailurePolicy::Stop, filter)
            .map(|result| result.response)
    }

    fn handle_matching(
        &self,
        request: &PluginRequest,
        policy: PluginFailurePolicy,
        filter: impl Fn(&str) -> bool,
    ) -> Result<PluginRunResult> {
        let original = request.clone();
        let mut current = request.clone();
        let mut diagnostics = Vec::new();
        let mut executed = Vec::new();

        for id in self.execution_order()? {
            let entry = &self.plugins[&id];
            if !entry.enabled.load(Ordering::Acquire) || !filter(&id) {
                continue;
            }
            info!("Processing with plugin: {id}");
            let result = match invoke_plugin(&id, entry, &current) {
                Ok(PluginExecution::Patch(patch)) => patch
                    .apply(&mut current.document)
                    .map(|_| None)
                    .map_err(|error| {
                        PluginDiagnostic::new(
                            id.clone(),
                            "PLUGIN_PATCH_REJECTED",
                            error.to_string(),
                            entry.execution_class,
                        )
                    }),
                Ok(PluginExecution::Response(response)) => Ok(Some(*response)),
                Err(diagnostic) => Err(diagnostic),
            };
            match result {
                Ok(response) => {
                    executed.push(id);
                    if let Some(response) = response {
                        current = PluginRequest {
                            action: current.action.clone(),
                            document: response.document,
                            metadata: response.metadata,
                        };
                    }
                }
                Err(mut diagnostic) => match policy {
                    PluginFailurePolicy::Stop => return Err(plugin_error(diagnostic.message)),
                    PluginFailurePolicy::Continue => diagnostics.push(diagnostic),
                    PluginFailurePolicy::DisablePlugin => {
                        entry.enabled.store(false, Ordering::Release);
                        diagnostic.disabled = true;
                        diagnostics.push(diagnostic);
                    }
                    PluginFailurePolicy::Rollback => {
                        diagnostics.push(diagnostic);
                        return Ok(PluginRunResult {
                            response: PluginResponse {
                                document: original.document,
                                metadata: original.metadata,
                            },
                            diagnostics,
                            executed,
                            rolled_back: true,
                        });
                    }
                },
            }
        }

        Ok(PluginRunResult {
            response: PluginResponse {
                document: current.document,
                metadata: current.metadata,
            },
            diagnostics,
            executed,
            rolled_back: false,
        })
    }

    fn manifest_records(&self) -> Vec<(String, PluginManifest, u64)> {
        self.plugins
            .iter()
            .map(|(id, entry)| (id.clone(), entry.manifest.clone(), entry.registration_order))
            .collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_manifest(manifest: &PluginManifest, name: &str, version: &str) -> Result<()> {
    if manifest.id.is_empty() || manifest.id != name {
        return Err(plugin_error(
            "Plugin manifest ID must be non-empty and match Plugin::name()",
        ));
    }
    if manifest.version != version {
        return Err(plugin_error(
            "Plugin manifest version must match Plugin::version()",
        ));
    }
    if manifest.plugin_api_version != PLUGIN_API_VERSION {
        return Err(plugin_error(format!(
            "Plugin '{}' requires API {}, host provides {}",
            manifest.id, manifest.plugin_api_version, PLUGIN_API_VERSION
        )));
    }
    if !manifest.format_capabilities.is_empty()
        && !manifest.hooks.iter().any(|hook| {
            matches!(
                hook,
                PluginHook::RegisterImporter | PluginHook::RegisterExporter
            )
        })
    {
        return Err(plugin_error(format!(
            "Plugin '{}' declares format capabilities without a registration hook",
            manifest.id
        )));
    }
    if manifest
        .format_capabilities
        .iter()
        .any(|capability| capability.input.is_none() || capability.output.is_none())
    {
        return Err(plugin_error(format!(
            "Plugin '{}' format capabilities require input and output labels",
            manifest.id
        )));
    }
    parse_version(&manifest.version)?;
    let core = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| plugin_error(error.to_string()))?;
    let requirement = semver::VersionReq::parse(&manifest.core_version_requirement)
        .map_err(|error| plugin_error(format!("Invalid core version requirement: {error}")))?;
    if !legacy_core_requirement_matches(&requirement, &core) {
        return Err(plugin_error(format!(
            "Plugin '{}' does not support core {}",
            manifest.id, core
        )));
    }
    if !manifest.platforms.is_empty()
        && !manifest
            .platforms
            .iter()
            .any(|value| value == std::env::consts::OS)
    {
        return Err(plugin_error(format!(
            "Plugin '{}' does not support platform {}",
            manifest.id,
            std::env::consts::OS
        )));
    }
    if !manifest.architectures.is_empty()
        && !manifest
            .architectures
            .iter()
            .any(|value| value == std::env::consts::ARCH)
    {
        return Err(plugin_error(format!(
            "Plugin '{}' does not support architecture {}",
            manifest.id,
            std::env::consts::ARCH
        )));
    }
    Ok(())
}

fn parse_version(value: &str) -> Result<semver::Version> {
    semver::Version::parse(value)
        .or_else(|_| {
            let component_count = value.split('.').count();
            let normalized = match component_count {
                1 => format!("{value}.0.0"),
                2 => format!("{value}.0"),
                _ => value.to_string(),
            };
            semver::Version::parse(&normalized)
        })
        .map_err(|error| plugin_error(format!("Invalid plugin version '{value}': {error}")))
}

fn ordered_records(records: &[(String, PluginManifest, u64)]) -> Result<Vec<String>> {
    let manifests: HashMap<_, _> = records
        .iter()
        .map(|(id, manifest, order)| (id.clone(), (manifest, *order)))
        .collect();
    let mut edges: HashMap<String, HashSet<String>> = manifests
        .keys()
        .map(|id| (id.clone(), HashSet::new()))
        .collect();
    let mut indegree: HashMap<String, usize> = manifests.keys().map(|id| (id.clone(), 0)).collect();

    for (id, (manifest, _)) in &manifests {
        for dependency in &manifest.dependencies {
            let Some((dependency_manifest, _)) = manifests.get(&dependency.id) else {
                return Err(plugin_error(format!(
                    "Plugin '{id}' requires missing plugin '{}'",
                    dependency.id
                )));
            };
            let requirement = semver::VersionReq::parse(&dependency.version_requirement)
                .map_err(|error| plugin_error(error.to_string()))?;
            if !requirement.matches(&parse_version(&dependency_manifest.version)?) {
                return Err(plugin_error(format!(
                    "Plugin '{id}' requires {} {}, found {}",
                    dependency.id, dependency.version_requirement, dependency_manifest.version
                )));
            }
            add_edge(&mut edges, &mut indegree, &dependency.id, id);
        }
        for target in &manifest.after {
            if manifests.contains_key(target) {
                add_edge(&mut edges, &mut indegree, target, id);
            }
        }
        for target in &manifest.before {
            if manifests.contains_key(target) {
                add_edge(&mut edges, &mut indegree, id, target);
            }
        }
    }

    let mut ordered: Vec<String> = Vec::with_capacity(records.len());
    while ordered.len() < records.len() {
        let candidate = indegree
            .iter()
            .filter(|(id, degree)| {
                **degree == 0
                    && !ordered
                        .iter()
                        .any(|selected| selected.as_str() == id.as_str())
            })
            .map(|(id, _)| id)
            .max_by(|left, right| {
                let (left_manifest, left_order) = manifests[*left];
                let (right_manifest, right_order) = manifests[*right];
                left_manifest
                    .priority
                    .cmp(&right_manifest.priority)
                    .then_with(|| right_order.cmp(&left_order))
                    .then_with(|| right.cmp(left))
            })
            .cloned()
            .ok_or_else(|| plugin_error("Plugin dependency cycle detected"))?;
        ordered.push(candidate.clone());
        if let Some(targets) = edges.get(&candidate) {
            for target in targets {
                if let Some(degree) = indegree.get_mut(target) {
                    *degree = degree.saturating_sub(1);
                }
            }
        }
    }
    Ok(ordered)
}

fn add_edge(
    edges: &mut HashMap<String, HashSet<String>>,
    indegree: &mut HashMap<String, usize>,
    from: &str,
    to: &str,
) {
    if edges
        .entry(from.to_string())
        .or_default()
        .insert(to.to_string())
    {
        *indegree.entry(to.to_string()).or_default() += 1;
    }
}

enum PluginExecution {
    Response(Box<PluginResponse>),
    Patch(DocumentPatch),
}

fn invoke_plugin(
    plugin_id: &str,
    entry: &PluginEntry,
    request: &PluginRequest,
) -> std::result::Result<PluginExecution, PluginDiagnostic> {
    let token = CancellationToken::default();
    let guard = entry
        .runtime
        .begin(plugin_id, entry.execution_class, token.clone())?;
    let Some(timeout_millis) = entry.permissions.timeout_millis else {
        let context = PluginExecutionContext {
            cancellation: token,
            deadline: None,
            permissions: entry.permissions.clone(),
            diagnostics: DiagnosticSink::default(),
        };
        let result = invoke_locked(
            plugin_id,
            entry.execution_class,
            &entry.plugin,
            request,
            &context,
        );
        drop(guard);
        return result;
    };

    let timeout = Duration::from_millis(timeout_millis.max(1));
    let context = PluginExecutionContext {
        cancellation: token.clone(),
        deadline: Instant::now().checked_add(timeout),
        permissions: entry.permissions.clone(),
        diagnostics: DiagnosticSink::default(),
    };
    let plugin = Arc::clone(&entry.plugin);
    let plugin_id_owned = plugin_id.to_string();
    let request = request.clone();
    let execution_class = entry.execution_class;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if let Err(error) = std::thread::Builder::new()
        .name(format!("plugin-{plugin_id}"))
        .spawn(move || {
            let _guard = guard;
            let result = invoke_locked(
                &plugin_id_owned,
                execution_class,
                &plugin,
                &request,
                &context,
            );
            let _ = sender.send(result);
        })
    {
        return Err(PluginDiagnostic::new(
            plugin_id,
            "PLUGIN_WORKER_START_FAILED",
            format!("Plugin '{plugin_id}' worker could not start: {error}"),
            entry.execution_class,
        ));
    }
    match receiver.recv_timeout(timeout) {
        Ok(Err(mut diagnostic)) if diagnostic.code == "PLUGIN_SOFT_TIMEOUT" => {
            entry.runtime.quarantined.store(true, Ordering::Release);
            diagnostic.soft_timeout = true;
            diagnostic.execution_may_still_be_running =
                entry.runtime.outstanding.load(Ordering::Acquire) != 0;
            Err(diagnostic)
        }
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            token.cancel();
            entry.runtime.quarantined.store(true, Ordering::Release);
            let mut diagnostic = PluginDiagnostic::new(
                plugin_id,
                "PLUGIN_SOFT_TIMEOUT",
                format!(
                    "Plugin '{plugin_id}' exceeded its {timeout_millis}ms soft timeout and was quarantined"
                ),
                entry.execution_class,
            );
            diagnostic.soft_timeout = true;
            diagnostic.execution_may_still_be_running =
                entry.runtime.outstanding.load(Ordering::Acquire) != 0;
            Err(diagnostic)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(PluginDiagnostic::new(
            plugin_id,
            "PLUGIN_WORKER_DISCONNECTED",
            format!("Plugin '{plugin_id}' worker disconnected"),
            entry.execution_class,
        )),
    }
}

fn invoke_locked(
    plugin_id: &str,
    execution_class: PluginExecutionClass,
    plugin: &RwLock<Box<dyn Plugin>>,
    request: &PluginRequest,
    context: &PluginExecutionContext,
) -> std::result::Result<PluginExecution, PluginDiagnostic> {
    catch_plugin_panic_diagnostic(plugin_id, execution_class, || {
        let plugin = plugin
            .read()
            .map_err(|_| plugin_error(format!("Plugin '{plugin_id}' lock is poisoned")))?;
        match plugin.document_patch_with_context(DocumentView::new(&request.document), context)? {
            Some(patch) => Ok(PluginExecution::Patch(patch)),
            None => plugin
                .handle_with_context(request, context)
                .map(Box::new)
                .map(PluginExecution::Response),
        }
    })
}

fn catch_plugin_panic<T>(
    plugin_id: &str,
    stage: &str,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => Err(plugin_error(format!(
            "Plugin '{plugin_id}' panicked during {stage}: {}",
            panic_message(payload)
        ))),
    }
}

fn catch_plugin_panic_diagnostic<T>(
    plugin_id: &str,
    execution_class: PluginExecutionClass,
    operation: impl FnOnce() -> Result<T>,
) -> std::result::Result<T, PluginDiagnostic> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let message = error.to_string();
            let code = if message.contains("PLUGIN_PERMISSION_DENIED") {
                "PLUGIN_PERMISSION_DENIED"
            } else if message.contains("PLUGIN_SOFT_TIMEOUT") {
                "PLUGIN_SOFT_TIMEOUT"
            } else if message.contains("PLUGIN_CANCELLED") {
                "PLUGIN_CANCELLED"
            } else {
                "PLUGIN_ERROR"
            };
            Err(PluginDiagnostic::new(
                plugin_id,
                code,
                message,
                execution_class,
            ))
        }
        Err(payload) => {
            let mut diagnostic = PluginDiagnostic::new(
                plugin_id,
                "PLUGIN_PANIC_CONTAINED",
                format!("Plugin '{plugin_id}' panicked: {}", panic_message(payload)),
                execution_class,
            );
            diagnostic.panic_contained = true;
            Err(diagnostic)
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|value| (*value).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string())
}

fn plugin_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(message.into())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;

    use latexsnipper_ast::{Document, FidelityLevel, FormatCapability, Page};

    use super::*;
    use crate::manifest::PluginDependency;
    use crate::patch::{DocumentPatch, PatchOperation};
    use crate::plugin::{PatchPlugin, TransformPlugin};

    fn plugin(id: &str, priority: i32) -> TransformPlugin {
        let mut manifest = PluginManifest::built_in(id, "1.0.0");
        manifest.priority = priority;
        TransformPlugin::new(id, "1.0.0", |_| Ok(())).with_manifest(manifest)
    }

    #[test]
    fn duplicate_registration_has_no_init_side_effect() {
        struct CountingPlugin {
            count: Arc<AtomicUsize>,
        }
        impl Plugin for CountingPlugin {
            fn name(&self) -> &str {
                "duplicate"
            }
            fn version(&self) -> &str {
                "1.0.0"
            }
            fn init(&mut self) -> Result<()> {
                self.count.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(())
            }
            fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
                Ok(PluginResponse::new(request.document.clone()))
            }
        }

        let count = Arc::new(AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(CountingPlugin {
                count: count.clone(),
            }))
            .unwrap();
        assert!(registry
            .register(Box::new(CountingPlugin {
                count: count.clone(),
            }))
            .is_err());
        assert_eq!(count.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn order_is_priority_then_registration_and_dependencies_win() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(plugin("low", 0))).unwrap();
        registry.register(Box::new(plugin("high", 10))).unwrap();
        registry.register(Box::new(plugin("peer", 10))).unwrap();
        assert_eq!(registry.execution_order().unwrap(), ["high", "peer", "low"]);

        let mut dependent_manifest = PluginManifest::built_in("dependent", "1.0.0");
        dependent_manifest.priority = 100;
        dependent_manifest.dependencies.push(PluginDependency {
            id: "low".to_string(),
            version_requirement: "^1".to_string(),
        });
        registry
            .register(Box::new(
                TransformPlugin::new("dependent", "1.0.0", |_| Ok(()))
                    .with_manifest(dependent_manifest),
            ))
            .unwrap();
        let order = registry.execution_order().unwrap();
        assert!(
            order.iter().position(|id| id == "low") < order.iter().position(|id| id == "dependent")
        );
    }

    #[test]
    fn cleanup_failure_keeps_plugin_registered() {
        struct CleanupFailure;
        impl Plugin for CleanupFailure {
            fn name(&self) -> &str {
                "cleanup"
            }
            fn version(&self) -> &str {
                "1.0.0"
            }
            fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
                Ok(PluginResponse::new(request.document.clone()))
            }
            fn cleanup(&mut self) -> Result<()> {
                Err(plugin_error("cleanup failed"))
            }
        }
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(CleanupFailure)).unwrap();
        assert!(registry.unregister("cleanup").is_err());
        assert!(registry.has("cleanup"));
    }

    #[test]
    fn policies_contain_panics_disable_and_rollback() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(TransformPlugin::new("page", "1.0.0", |doc| {
                doc.pages.push(Page {
                    width: 1.0,
                    height: 1.0,
                    blocks: Vec::new(),
                    page_number: None,
                    layout: None,
                    background_asset_id: None,
                });
                Ok(())
            })))
            .unwrap();
        registry
            .register(Box::new(TransformPlugin::new("panic", "1.0.0", |_| {
                panic!("contained")
            })))
            .unwrap();
        let request = PluginRequest::new("transform", Document::new());

        let disabled = registry
            .handle_all_with_policy(&request, PluginFailurePolicy::DisablePlugin)
            .unwrap();
        assert_eq!(disabled.response.document.pages.len(), 1);
        assert!(disabled.diagnostics[0].panic_contained);
        assert!(!registry.is_enabled("panic"));

        registry.enable("panic").unwrap();
        let rolled_back = registry
            .handle_all_with_policy(&request, PluginFailurePolicy::Rollback)
            .unwrap();
        assert!(rolled_back.rolled_back);
        assert!(rolled_back.response.document.pages.is_empty());
    }

    #[test]
    fn capabilities_follow_enable_and_disable() {
        let mut manifest = PluginManifest::built_in("exporter", "1.0.0");
        manifest.capabilities.push("export:custom".to_string());
        manifest.hooks.push(PluginHook::RegisterExporter);
        manifest.format_capabilities.push(FormatCapability {
            input: Some("AST".to_string()),
            output: Some("custom".to_string()),
            available: true,
            supports_formula: true,
            supports_table: false,
            supports_image: false,
            supports_svg: false,
            supports_style: false,
            supports_layout: false,
            supports_office_objects: false,
            fidelity: FidelityLevel::SemanticOnly,
            known_loss: Vec::new(),
            notes: vec!["Registered by exporter plugin".to_string()],
            required_features: Vec::new(),
            external_dependencies: Vec::new(),
            platform_restrictions: Vec::new(),
            experimental: true,
        });
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(
                TransformPlugin::new("exporter", "1.0.0", |_| Ok(())).with_manifest(manifest),
            ))
            .unwrap();
        assert_eq!(registry.capabilities(), ["export:custom"]);
        let mut matrix = CapabilityMatrix {
            schema_version: "2.0.0".to_string(),
            entries: Vec::new(),
        };
        registry.extend_capability_matrix(&mut matrix);
        assert_eq!(matrix.entries.len(), 1);
        assert_eq!(matrix.entries[0].output.as_deref(), Some("custom"));
        registry.disable("exporter").unwrap();
        assert!(registry.capabilities().is_empty());
        matrix.entries.clear();
        registry.extend_capability_matrix(&mut matrix);
        assert!(matrix.entries.is_empty());
    }

    #[test]
    fn dependency_cycles_are_rejected_before_initialization() {
        let mut first_manifest = PluginManifest::built_in("first", "1.0.0");
        first_manifest.before.push("second".to_string());
        let mut second_manifest = PluginManifest::built_in("second", "1.0.0");
        second_manifest.before.push("first".to_string());

        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(
                TransformPlugin::new("first", "1.0.0", |_| Ok(())).with_manifest(first_manifest),
            ))
            .unwrap();
        let error = registry
            .register(Box::new(
                TransformPlugin::new("second", "1.0.0", |_| Ok(())).with_manifest(second_manifest),
            ))
            .unwrap_err();
        assert!(error.to_string().contains("cycle"));
        assert!(!registry.has("second"));
    }

    #[test]
    fn stop_and_continue_policies_have_distinct_behavior() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(TransformPlugin::new("failure", "1.0.0", |_| {
                Err(plugin_error("expected failure"))
            })))
            .unwrap();
        registry
            .register(Box::new(TransformPlugin::new("success", "1.0.0", |doc| {
                doc.pages.push(Page {
                    width: 1.0,
                    height: 1.0,
                    blocks: Vec::new(),
                    page_number: None,
                    layout: None,
                    background_asset_id: None,
                });
                Ok(())
            })))
            .unwrap();
        let request = PluginRequest::new("transform", Document::new());

        assert!(registry
            .handle_all_with_policy(&request, PluginFailurePolicy::Stop)
            .is_err());
        let continued = registry
            .handle_all_with_policy(&request, PluginFailurePolicy::Continue)
            .unwrap();
        assert_eq!(continued.diagnostics.len(), 1);
        assert_eq!(continued.response.document.pages.len(), 1);
    }

    #[test]
    fn soft_timeout_quarantines_and_bounds_background_workers() {
        let mut manifest = PluginManifest::built_in("slow", "1.0.0");
        manifest.permissions.timeout_millis = Some(10);
        let completed = Arc::new(AtomicBool::new(false));
        let completed_by_worker = Arc::clone(&completed);
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(
                TransformPlugin::new("slow", "1.0.0", move |_| {
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    completed_by_worker.store(true, Ordering::Release);
                    Ok(())
                })
                .with_manifest(manifest),
            ))
            .unwrap();

        let started = std::time::Instant::now();
        let result = registry
            .handle_all_with_policy(
                &PluginRequest::new("transform", Document::new()),
                PluginFailurePolicy::Continue,
            )
            .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
        assert_eq!(result.diagnostics[0].code, "PLUGIN_SOFT_TIMEOUT");
        assert!(result.diagnostics[0].soft_timeout);
        assert!(result.diagnostics[0].execution_may_still_be_running);
        let status = registry.execution_status("slow").unwrap();
        assert!(status.quarantined);
        assert_eq!(status.outstanding_executions, 1);

        let rejected = registry
            .handle_all_with_policy(
                &PluginRequest::new("transform", Document::new()),
                PluginFailurePolicy::Continue,
            )
            .unwrap();
        assert_eq!(rejected.diagnostics[0].code, "PLUGIN_QUARANTINED");
        assert_eq!(
            registry
                .execution_status("slow")
                .unwrap()
                .outstanding_executions,
            1
        );

        let wait_started = Instant::now();
        while !completed.load(Ordering::Acquire) && wait_started.elapsed() < Duration::from_secs(1)
        {
            std::thread::yield_now();
        }
        assert!(completed.load(Ordering::Acquire));
        while registry
            .execution_status("slow")
            .unwrap()
            .outstanding_executions
            != 0
        {
            std::thread::yield_now();
        }
        registry.reset_quarantine("slow").unwrap();
        assert!(!registry.execution_status("slow").unwrap().quarantined);
    }

    #[test]
    fn cooperative_plugin_observes_cancellation_and_can_retry_after_reset() {
        struct CooperativePlugin {
            invocations: Arc<AtomicUsize>,
            manifest: PluginManifest,
        }

        impl Plugin for CooperativePlugin {
            fn name(&self) -> &str {
                "cooperative"
            }

            fn version(&self) -> &str {
                "1.0.0"
            }

            fn manifest(&self) -> PluginManifest {
                self.manifest.clone()
            }

            fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
                Ok(PluginResponse::new(request.document.clone()))
            }

            fn handle_with_context(
                &self,
                request: &PluginRequest,
                context: &PluginExecutionContext,
            ) -> Result<PluginResponse> {
                if self.invocations.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
                    loop {
                        context.checkpoint()?;
                        std::thread::yield_now();
                    }
                }
                Ok(PluginResponse::new(request.document.clone()))
            }
        }

        let mut manifest = PluginManifest::built_in("cooperative", "1.0.0");
        manifest.permissions.timeout_millis = Some(10);
        let invocations = Arc::new(AtomicUsize::new(0));
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(CooperativePlugin {
                invocations: Arc::clone(&invocations),
                manifest,
            }))
            .unwrap();
        let request = PluginRequest::new("transform", Document::new());
        let first = registry
            .handle_all_with_policy(&request, PluginFailurePolicy::Continue)
            .unwrap();
        assert_eq!(first.diagnostics[0].code, "PLUGIN_SOFT_TIMEOUT");

        let started = Instant::now();
        while registry
            .execution_status("cooperative")
            .unwrap()
            .outstanding_executions
            != 0
            && started.elapsed() < Duration::from_secs(1)
        {
            std::thread::yield_now();
        }
        registry.reset_quarantine("cooperative").unwrap();
        let second = registry
            .handle_all_with_policy(&request, PluginFailurePolicy::Stop)
            .unwrap();
        assert!(second.diagnostics.is_empty());
        assert_eq!(invocations.load(AtomicOrdering::SeqCst), 2);
    }

    #[test]
    fn patch_plugin_rolls_back_partial_mutations_on_failure() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(PatchPlugin::new("patch", "1.0.0", |_| {
                Ok(DocumentPatch::new()
                    .push(PatchOperation::InsertPage {
                        index: 1,
                        page: Page::new(200.0, 200.0, 2),
                    })
                    .push(PatchOperation::RemovePage { index: 20 }))
            })))
            .unwrap();
        let mut document = Document::new();
        document.pages.push(Page::new(100.0, 100.0, 1));

        let result = registry
            .handle_all_with_policy(
                &PluginRequest::new("transform", document),
                PluginFailurePolicy::Continue,
            )
            .unwrap();
        assert_eq!(result.response.document.pages.len(), 1);
        assert_eq!(result.response.document.pages[0].width, 100.0);
        assert_eq!(result.diagnostics[0].code, "PLUGIN_PATCH_REJECTED");
    }
}

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

use latexsnipper_foundation::{Result, SnipperError};
use log::info;
use serde::Serialize;

use crate::manifest::{PluginManifest, PLUGIN_API_VERSION};
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
}

#[derive(Debug, Clone)]
pub struct PluginRunResult {
    pub response: PluginResponse,
    pub diagnostics: Vec<PluginDiagnostic>,
    pub executed: Vec<String>,
    pub rolled_back: bool,
}

struct PluginEntry {
    plugin: Box<dyn Plugin>,
    manifest: PluginManifest,
    registration_order: u64,
    enabled: AtomicBool,
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

        let mut prospective = self.manifest_records();
        prospective.push((id.clone(), manifest.clone(), self.next_registration_order));
        ordered_records(&prospective)?;

        info!("Registering plugin: {} v{}", id, manifest.version);
        catch_plugin_panic(&id, "init", || plugin.init())?;
        self.plugins.insert(
            id,
            PluginEntry {
                plugin,
                manifest,
                registration_order: self.next_registration_order,
                enabled: AtomicBool::new(true),
            },
        );
        self.next_registration_order = self.next_registration_order.saturating_add(1);
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> Result<()> {
        let Some(entry) = self.plugins.get_mut(id) else {
            return Ok(());
        };
        info!("Unregistering plugin: {id}");
        catch_plugin_panic(id, "cleanup", || entry.plugin.cleanup())?;
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

    pub fn handle(&self, plugin_id: &str, request: &PluginRequest) -> Result<PluginResponse> {
        let entry = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| plugin_error(format!("Plugin '{plugin_id}' not found")))?;
        if !entry.enabled.load(Ordering::Acquire) {
            return Err(plugin_error(format!("Plugin '{plugin_id}' is disabled")));
        }
        catch_plugin_panic(plugin_id, "handle", || entry.plugin.handle(request))
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
            match catch_plugin_panic_diagnostic(&id, || entry.plugin.handle(&current)) {
                Ok(response) => {
                    executed.push(id);
                    current = PluginRequest {
                        action: current.action.clone(),
                        document: response.document,
                        metadata: response.metadata,
                    };
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
    parse_version(&manifest.version)?;
    let core = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| plugin_error(error.to_string()))?;
    let requirement = semver::VersionReq::parse(&manifest.core_version_requirement)
        .map_err(|error| plugin_error(format!("Invalid core version requirement: {error}")))?;
    if !requirement.matches(&core) {
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
    operation: impl FnOnce() -> Result<T>,
) -> std::result::Result<T, PluginDiagnostic> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(PluginDiagnostic {
            plugin_id: plugin_id.to_string(),
            code: "PLUGIN_ERROR",
            message: error.to_string(),
            panic_contained: false,
            disabled: false,
        }),
        Err(payload) => Err(PluginDiagnostic {
            plugin_id: plugin_id.to_string(),
            code: "PLUGIN_PANIC_CONTAINED",
            message: format!("Plugin '{plugin_id}' panicked: {}", panic_message(payload)),
            panic_contained: true,
            disabled: false,
        }),
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

    use latexsnipper_ast::{Document, Page};

    use super::*;
    use crate::manifest::PluginDependency;
    use crate::plugin::TransformPlugin;

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
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(
                TransformPlugin::new("exporter", "1.0.0", |_| Ok(())).with_manifest(manifest),
            ))
            .unwrap();
        assert_eq!(registry.capabilities(), ["export:custom"]);
        registry.disable("exporter").unwrap();
        assert!(registry.capabilities().is_empty());
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
}

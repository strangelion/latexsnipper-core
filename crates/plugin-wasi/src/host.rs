use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use latexsnipper_plugin::{CancellationToken, PluginRegistrationGrantsV3};
use sha2::{Digest, Sha256};
use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, UpdateDeadline};

use crate::bindings::latexsnipper::plugin::{
    environment_broker, execution_broker, filesystem_broker, model_artifact_broker, network_broker,
    system_broker, temporary_storage_broker, types,
};
use crate::bindings::Plugin;
use crate::{
    ComponentNetworkScheme, ComponentPermissions, NetworkGrant, VerifiedComponentPackage,
    WasiDiagnostic, WasiDiagnosticCode,
};

const INTERRUPT_NONE: u8 = 0;
const INTERRUPT_CANCELLED: u8 = 1;
const INTERRUPT_TIMEOUT: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRequest {
    pub scheme: ComponentNetworkScheme,
    pub host: String,
    pub port: u16,
    pub method: String,
    pub path_and_query: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait NetworkBroker: Send + Sync {
    fn send(&self, request: &NetworkRequest) -> Result<NetworkResponse, WasiDiagnostic>;
}

#[derive(Debug, Default)]
pub struct DenyNetworkBroker;

impl NetworkBroker for DenyNetworkBroker {
    fn send(&self, _request: &NetworkRequest) -> Result<NetworkResponse, WasiDiagnostic> {
        Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiPermissionDenied,
            "no network broker was configured",
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ModelArtifactResource {
    bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct TemporaryFileResource {
    bytes: Vec<u8>,
}

pub struct CompiledWasiComponent {
    component: Component,
    sha256: String,
}

impl CompiledWasiComponent {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug)]
pub enum ComponentInvocation {
    Transform(types::Document),
    Import(types::ImportRequest),
    Export(types::ExportRequest),
}

#[derive(Debug)]
pub enum ComponentInvocationResult {
    Patch(types::DocumentPatch),
    Document(types::Document),
    Export(types::ExportResult),
}

pub struct WasiComponentHost {
    engine: Engine,
    _epoch_ticker: EpochTicker,
    package: VerifiedComponentPackage,
    environment: BTreeMap<String, String>,
    model_artifacts: BTreeMap<String, Arc<Vec<u8>>>,
    network: Arc<dyn NetworkBroker>,
    concurrency: Arc<ConcurrencyGate>,
}

impl WasiComponentHost {
    pub fn new(package: VerifiedComponentPackage) -> Result<Self, WasiDiagnostic> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(host_failure)?;
        let epoch_ticker = EpochTicker::new(engine.clone());
        let concurrency = Arc::new(ConcurrencyGate::new(
            package.permissions.limits.max_concurrent_executions,
        ));
        Ok(Self {
            engine,
            _epoch_ticker: epoch_ticker,
            package,
            environment: BTreeMap::new(),
            model_artifacts: BTreeMap::new(),
            network: Arc::new(DenyNetworkBroker),
            concurrency,
        })
    }

    pub fn with_environment(mut self, values: BTreeMap<String, String>) -> Self {
        self.environment = values
            .into_iter()
            .filter(|(name, _)| {
                self.package
                    .permissions
                    .environment
                    .contains(&name.to_ascii_uppercase())
            })
            .map(|(name, value)| (name.to_ascii_uppercase(), value))
            .collect();
        self
    }

    pub fn with_model_artifacts(mut self, values: BTreeMap<String, Vec<u8>>) -> Self {
        let mut total = 0usize;
        self.model_artifacts = values
            .into_iter()
            .filter_map(|(name, bytes)| {
                if !self.package.permissions.model_artifacts.contains(&name) {
                    return None;
                }
                let next = total.checked_add(bytes.len())?;
                if next > self.package.permissions.limits.model_artifact_bytes {
                    return None;
                }
                total = next;
                Some((name, Arc::new(bytes)))
            })
            .collect();
        self
    }

    pub fn with_network_broker(mut self, broker: Arc<dyn NetworkBroker>) -> Self {
        self.network = broker;
        self
    }

    pub fn compile(&self) -> Result<CompiledWasiComponent, WasiDiagnostic> {
        let bytes = fs::read(&self.package.component_path).map_err(host_failure)?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != self.package.component_sha256 {
            return Err(protocol_mismatch(
                "component changed after package verification",
            ));
        }
        let component = Component::new(&self.engine, &bytes)
            .map_err(|error| protocol_mismatch(error.to_string()))?;
        Ok(CompiledWasiComponent {
            component,
            sha256: digest,
        })
    }

    pub fn execute(
        &self,
        compiled: &CompiledWasiComponent,
        invocation: ComponentInvocation,
        cancellation: CancellationToken,
    ) -> Result<ComponentInvocationResult, WasiDiagnostic> {
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(
                self.package.permissions.limits.timeout_millis,
            ))
            .ok_or_else(|| host_failure("execution deadline overflow"))?;
        let _permit = self.concurrency.acquire_until(deadline, &cancellation)?;
        validate_input_size(&invocation, self.package.permissions.limits.input_bytes)?;
        let interrupt = Arc::new(AtomicU8::new(INTERRUPT_NONE));
        let state = HostState::new(
            self.package.permissions.clone(),
            cancellation.clone(),
            deadline,
            self.environment.clone(),
            self.model_artifacts.clone(),
            Arc::clone(&self.network),
        );
        let mut linker = Linker::new(&self.engine);
        Plugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state: &mut HostState| state)
            .map_err(host_failure)?;
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.store_limits);
        store
            .set_fuel(self.package.permissions.limits.fuel)
            .map_err(host_failure)?;
        store.set_epoch_deadline(1);
        let completion_cancellation = cancellation.clone();
        let callback_cancellation = cancellation;
        let callback_interrupt = Arc::clone(&interrupt);
        store.epoch_deadline_callback(move |_| {
            if callback_cancellation.is_cancelled() {
                callback_interrupt.store(INTERRUPT_CANCELLED, Ordering::Release);
                return Ok(UpdateDeadline::Interrupt);
            }
            if Instant::now() >= deadline {
                callback_interrupt.store(INTERRUPT_TIMEOUT, Ordering::Release);
                return Ok(UpdateDeadline::Interrupt);
            }
            Ok(UpdateDeadline::Continue(1))
        });
        let execution = self.execute_inner(&mut store, &linker, compiled, invocation);
        drop(store);
        match interrupt.load(Ordering::Acquire) {
            INTERRUPT_CANCELLED => Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiCancelled,
                "component execution was cancelled",
            )),
            INTERRUPT_TIMEOUT => Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiTimeout,
                "component execution exceeded its deadline",
            )),
            _ if completion_cancellation.is_cancelled() => Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiCancelled,
                "component execution was cancelled",
            )),
            _ if Instant::now() >= deadline => Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiTimeout,
                "component execution exceeded its deadline",
            )),
            _ => execution,
        }
    }

    fn execute_inner(
        &self,
        store: &mut Store<HostState>,
        linker: &Linker<HostState>,
        compiled: &CompiledWasiComponent,
        invocation: ComponentInvocation,
    ) -> Result<ComponentInvocationResult, WasiDiagnostic> {
        let bindings = Plugin::instantiate(&mut *store, &compiled.component, linker)
            .map_err(classify_runtime_error)?;
        let metadata = bindings
            .latexsnipper_plugin_lifecycle()
            .call_metadata(&mut *store)
            .map_err(classify_runtime_error)?;
        validate_metadata(&self.package, &metadata)?;
        let capabilities = bindings
            .latexsnipper_plugin_lifecycle()
            .call_declared_capabilities(&mut *store)
            .map_err(classify_runtime_error)?;
        validate_capabilities(
            &self.package.manifest.permissions.registrations,
            &capabilities,
        )?;
        let init = types::InitContext {
            core_version: env!("CARGO_PKG_VERSION").to_string(),
            granted_capabilities: granted_capabilities(&self.package.permissions),
            configuration: Vec::new(),
        };
        bindings
            .latexsnipper_plugin_lifecycle()
            .call_initialize(&mut *store, &init)
            .map_err(classify_runtime_error)?
            .map_err(plugin_error)?;
        let expected_patch_schema = match &invocation {
            ComponentInvocation::Transform(document) => Some(document.schema_version.clone()),
            _ => None,
        };
        let result = match invocation {
            ComponentInvocation::Transform(document) => bindings
                .latexsnipper_plugin_document_transformer()
                .call_transform(&mut *store, &document)
                .map_err(classify_runtime_error)?
                .map(ComponentInvocationResult::Patch)
                .map_err(plugin_error),
            ComponentInvocation::Import(request) => bindings
                .latexsnipper_plugin_importer()
                .call_import_document(&mut *store, &request)
                .map_err(classify_runtime_error)?
                .map(ComponentInvocationResult::Document)
                .map_err(plugin_error),
            ComponentInvocation::Export(request) => bindings
                .latexsnipper_plugin_exporter()
                .call_export_document(&mut *store, &request)
                .map_err(classify_runtime_error)?
                .map(ComponentInvocationResult::Export)
                .map_err(plugin_error),
        }?;
        bindings
            .latexsnipper_plugin_lifecycle()
            .call_shutdown(&mut *store)
            .map_err(classify_runtime_error)?
            .map_err(plugin_error)?;
        validate_output_size(&result, self.package.permissions.limits.output_bytes)?;
        validate_result(
            &result,
            expected_patch_schema.as_deref(),
            self.package.permissions.limits.diagnostic_count,
            self.package.permissions.limits.diagnostic_bytes,
        )?;
        Ok(result)
    }
}

struct HostState {
    table: ResourceTable,
    permissions: ComponentPermissions,
    cancellation: CancellationToken,
    deadline: Instant,
    started: Instant,
    environment: BTreeMap<String, String>,
    model_artifacts: BTreeMap<String, Arc<Vec<u8>>>,
    network: Arc<dyn NetworkBroker>,
    temporary_bytes: usize,
    resource_count: usize,
    store_limits: StoreLimits,
}

impl HostState {
    fn new(
        permissions: ComponentPermissions,
        cancellation: CancellationToken,
        deadline: Instant,
        environment: BTreeMap<String, String>,
        model_artifacts: BTreeMap<String, Arc<Vec<u8>>>,
        network: Arc<dyn NetworkBroker>,
    ) -> Self {
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(permissions.limits.memory_bytes)
            .table_elements(permissions.limits.table_elements as usize)
            .instances(permissions.limits.resources)
            .tables(permissions.limits.resources)
            .memories(permissions.limits.resources)
            .trap_on_grow_failure(true)
            .build();
        Self {
            table: ResourceTable::new(),
            permissions,
            cancellation,
            deadline,
            started: Instant::now(),
            environment,
            model_artifacts,
            network,
            temporary_bytes: 0,
            resource_count: 0,
            store_limits,
        }
    }

    fn checkpoint(&self) -> wasmtime::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(wasmtime::Error::msg("PLUGIN_WASI_CANCELLED"));
        }
        if Instant::now() >= self.deadline {
            return Err(wasmtime::Error::msg("PLUGIN_WASI_TIMEOUT"));
        }
        Ok(())
    }
}

impl types::Host for HostState {}

impl execution_broker::Host for HostState {
    fn is_cancelled(&mut self) -> wasmtime::Result<bool> {
        Ok(self.cancellation.is_cancelled() || Instant::now() >= self.deadline)
    }

    fn remaining_millis(&mut self) -> wasmtime::Result<Option<u64>> {
        Ok(self
            .deadline
            .checked_duration_since(Instant::now())
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64))
    }
}

impl filesystem_broker::Host for HostState {
    fn read(
        &mut self,
        grant_id: String,
        relative_path: String,
    ) -> wasmtime::Result<Result<Vec<u8>, filesystem_broker::AccessError>> {
        self.checkpoint()?;
        let path = match self
            .permissions
            .resolve_path(&grant_id, &relative_path, false)
        {
            Ok(path) => path,
            Err(_) => return Ok(Err(filesystem_broker::AccessError::PermissionDenied)),
        };
        match fs::read(path) {
            Ok(bytes) if bytes.len() <= self.permissions.limits.output_bytes => Ok(Ok(bytes)),
            Ok(_) => Ok(Err(filesystem_broker::AccessError::SizeLimit)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Err(filesystem_broker::AccessError::NotFound))
            }
            Err(_) => Ok(Err(filesystem_broker::AccessError::HostFailure)),
        }
    }

    fn write(
        &mut self,
        grant_id: String,
        relative_path: String,
        payload: Vec<u8>,
    ) -> wasmtime::Result<Result<(), filesystem_broker::AccessError>> {
        self.checkpoint()?;
        if payload.len() > self.permissions.limits.input_bytes {
            return Ok(Err(filesystem_broker::AccessError::SizeLimit));
        }
        let path = match self
            .permissions
            .resolve_path(&grant_id, &relative_path, true)
        {
            Ok(path) => path,
            Err(_) => return Ok(Err(filesystem_broker::AccessError::PermissionDenied)),
        };
        Ok(fs::write(path, payload).map_err(|_| filesystem_broker::AccessError::HostFailure))
    }
}

impl environment_broker::Host for HostState {
    fn get(
        &mut self,
        name: String,
    ) -> wasmtime::Result<Result<Option<String>, types::PluginError>> {
        self.checkpoint()?;
        let normalized = name.to_ascii_uppercase();
        if !self.permissions.environment.contains(&normalized) {
            return Ok(Err(permission_plugin_error(
                "environment variable is not granted",
            )));
        }
        Ok(Ok(self.environment.get(&normalized).cloned()))
    }
}

impl network_broker::Host for HostState {
    fn send(
        &mut self,
        request: network_broker::Request,
    ) -> wasmtime::Result<Result<network_broker::Response, types::PluginError>> {
        self.checkpoint()?;
        let scheme = match request.destination.scheme {
            network_broker::Scheme::Https => ComponentNetworkScheme::Https,
            network_broker::Scheme::Http => ComponentNetworkScheme::Http,
            network_broker::Scheme::Tcp => ComponentNetworkScheme::Tcp,
        };
        let grant = NetworkGrant {
            scheme,
            host: request.destination.host.clone(),
            port: request.destination.port,
        };
        if !self.permissions.permits_network(&grant) {
            return Ok(Err(permission_plugin_error(
                "network destination is not granted",
            )));
        }
        if !request.path_and_query.starts_with('/')
            || request.path_and_query.contains("\r")
            || request.path_and_query.contains("\n")
            || request.body.len() > self.permissions.limits.input_bytes
        {
            return Ok(Err(invalid_input_plugin_error("invalid network request")));
        }
        let response = self.network.send(&NetworkRequest {
            scheme,
            host: grant.host,
            port: grant.port,
            method: request.method,
            path_and_query: request.path_and_query,
            body: request.body,
        });
        match response {
            Ok(response) if response.body.len() <= self.permissions.limits.output_bytes => {
                Ok(Ok(network_broker::Response {
                    status: response.status,
                    body: response.body,
                }))
            }
            Ok(_) => Ok(Err(invalid_input_plugin_error(
                "network response exceeds output limit",
            ))),
            Err(error) => Ok(Err(permission_plugin_error(error.to_string()))),
        }
    }
}

impl model_artifact_broker::HostArtifact for HostState {
    fn size(&mut self, resource: Resource<ModelArtifactResource>) -> wasmtime::Result<u64> {
        self.checkpoint()?;
        Ok(self.table.get(&resource)?.bytes.len() as u64)
    }

    fn read(
        &mut self,
        resource: Resource<ModelArtifactResource>,
        offset: u64,
        length: u32,
    ) -> wasmtime::Result<Result<Vec<u8>, types::PluginError>> {
        self.checkpoint()?;
        let artifact = self.table.get(&resource)?;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let requested = length as usize;
        if requested > self.permissions.limits.output_bytes || start > artifact.bytes.len() {
            return Ok(Err(invalid_input_plugin_error("invalid artifact range")));
        }
        let end = start.saturating_add(requested).min(artifact.bytes.len());
        Ok(Ok(artifact.bytes[start..end].to_vec()))
    }

    fn drop(&mut self, resource: Resource<ModelArtifactResource>) -> wasmtime::Result<()> {
        self.table.delete(resource)?;
        self.resource_count = self.resource_count.saturating_sub(1);
        Ok(())
    }
}

impl model_artifact_broker::Host for HostState {
    fn open(
        &mut self,
        name: String,
    ) -> wasmtime::Result<Result<Resource<ModelArtifactResource>, types::PluginError>> {
        self.checkpoint()?;
        if !self.permissions.model_artifacts.contains(&name) {
            return Ok(Err(permission_plugin_error(
                "model artifact is not granted",
            )));
        }
        if self.resource_count >= self.permissions.limits.resources {
            return Ok(Err(invalid_input_plugin_error("resource limit exceeded")));
        }
        let Some(bytes) = self.model_artifacts.get(&name).cloned() else {
            return Ok(Err(invalid_input_plugin_error(
                "model artifact is unavailable",
            )));
        };
        let resource = self.table.push(ModelArtifactResource { bytes })?;
        self.resource_count += 1;
        Ok(Ok(resource))
    }
}

impl temporary_storage_broker::HostTemporaryFile for HostState {
    fn size(&mut self, resource: Resource<TemporaryFileResource>) -> wasmtime::Result<u64> {
        self.checkpoint()?;
        Ok(self.table.get(&resource)?.bytes.len() as u64)
    }

    fn read(
        &mut self,
        resource: Resource<TemporaryFileResource>,
        offset: u64,
        length: u32,
    ) -> wasmtime::Result<Result<Vec<u8>, types::PluginError>> {
        self.checkpoint()?;
        let file = self.table.get(&resource)?;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let requested = length as usize;
        if requested > self.permissions.limits.output_bytes || start > file.bytes.len() {
            return Ok(Err(invalid_input_plugin_error(
                "invalid temporary file range",
            )));
        }
        let end = start.saturating_add(requested).min(file.bytes.len());
        Ok(Ok(file.bytes[start..end].to_vec()))
    }

    fn write(
        &mut self,
        resource: Resource<TemporaryFileResource>,
        offset: u64,
        payload: Vec<u8>,
    ) -> wasmtime::Result<Result<(), types::PluginError>> {
        self.checkpoint()?;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let end = start.saturating_add(payload.len());
        let old_len = self.table.get(&resource)?.bytes.len();
        let new_total = self
            .temporary_bytes
            .saturating_sub(old_len)
            .saturating_add(end);
        if end < start || new_total > self.permissions.limits.temporary_storage_bytes {
            return Ok(Err(invalid_input_plugin_error(
                "temporary storage limit exceeded",
            )));
        }
        let file = self.table.get_mut(&resource)?;
        file.bytes.resize(end, 0);
        file.bytes[start..end].copy_from_slice(&payload);
        self.temporary_bytes = new_total;
        Ok(Ok(()))
    }

    fn truncate(
        &mut self,
        resource: Resource<TemporaryFileResource>,
        length: u64,
    ) -> wasmtime::Result<Result<(), types::PluginError>> {
        self.checkpoint()?;
        let length = usize::try_from(length).unwrap_or(usize::MAX);
        let old_len = self.table.get(&resource)?.bytes.len();
        let new_total = self
            .temporary_bytes
            .saturating_sub(old_len)
            .saturating_add(length);
        if new_total > self.permissions.limits.temporary_storage_bytes {
            return Ok(Err(invalid_input_plugin_error(
                "temporary storage limit exceeded",
            )));
        }
        self.table.get_mut(&resource)?.bytes.resize(length, 0);
        self.temporary_bytes = new_total;
        Ok(Ok(()))
    }

    fn drop(&mut self, resource: Resource<TemporaryFileResource>) -> wasmtime::Result<()> {
        let file = self.table.delete(resource)?;
        self.temporary_bytes = self.temporary_bytes.saturating_sub(file.bytes.len());
        self.resource_count = self.resource_count.saturating_sub(1);
        Ok(())
    }
}

impl temporary_storage_broker::Host for HostState {
    fn create(
        &mut self,
    ) -> wasmtime::Result<Result<Resource<TemporaryFileResource>, types::PluginError>> {
        self.checkpoint()?;
        if !self.permissions.temporary_storage {
            return Ok(Err(permission_plugin_error(
                "temporary storage is not granted",
            )));
        }
        if self.resource_count >= self.permissions.limits.resources {
            return Ok(Err(invalid_input_plugin_error("resource limit exceeded")));
        }
        let resource = self.table.push(TemporaryFileResource::default())?;
        self.resource_count += 1;
        Ok(Ok(resource))
    }
}

impl system_broker::Host for HostState {
    fn monotonic_millis(&mut self) -> wasmtime::Result<Result<u64, types::PluginError>> {
        self.checkpoint()?;
        if !self.permissions.clocks {
            return Ok(Err(permission_plugin_error("clock access is not granted")));
        }
        Ok(Ok(
            self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        ))
    }

    fn random_bytes(
        &mut self,
        length: u32,
    ) -> wasmtime::Result<Result<Vec<u8>, types::PluginError>> {
        self.checkpoint()?;
        if !self.permissions.randomness {
            return Ok(Err(permission_plugin_error("randomness is not granted")));
        }
        if length as usize > self.permissions.limits.output_bytes {
            return Ok(Err(invalid_input_plugin_error(
                "random byte request exceeds output limit",
            )));
        }
        let mut bytes = vec![0; length as usize];
        getrandom::fill(&mut bytes)
            .map_err(|error| wasmtime::Error::msg(format!("random source failed: {error}")))?;
        Ok(Ok(bytes))
    }
}

fn validate_metadata(
    package: &VerifiedComponentPackage,
    metadata: &types::PluginMetadata,
) -> Result<(), WasiDiagnostic> {
    if metadata.id != package.manifest.id
        || metadata.name != package.manifest.name
        || metadata.version != package.manifest.version
        || metadata.plugin_api_version != package.manifest.interfaces.plugin_api
        || metadata.wit_version
            != package
                .manifest
                .interfaces
                .component_wit
                .unwrap_or_default()
    {
        return Err(protocol_mismatch(
            "component metadata does not match verified manifest",
        ));
    }
    Ok(())
}

fn validate_capabilities(
    registrations: &PluginRegistrationGrantsV3,
    capabilities: &[types::Capability],
) -> Result<(), WasiDiagnostic> {
    for capability in capabilities {
        let allowed = match capability {
            types::Capability::DocumentTransform => registrations.capabilities,
            types::Capability::Importer => registrations.importers,
            types::Capability::Exporter => registrations.exporters,
            _ => true,
        };
        if !allowed {
            return Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiPermissionDenied,
                "component declared a capability without registration grant",
            ));
        }
    }
    Ok(())
}

fn granted_capabilities(permissions: &ComponentPermissions) -> Vec<types::Capability> {
    let mut result = Vec::new();
    if permissions.filesystem.values().any(|grant| !grant.writable) {
        result.push(types::Capability::FilesystemRead);
    }
    if permissions.filesystem.values().any(|grant| grant.writable) {
        result.push(types::Capability::FilesystemWrite);
    }
    if !permissions.environment.is_empty() {
        result.push(types::Capability::EnvironmentRead);
    }
    if !permissions.network.is_empty() {
        result.push(types::Capability::NetworkRequest);
    }
    if !permissions.model_artifacts.is_empty() {
        result.push(types::Capability::ModelArtifactRead);
    }
    if permissions.temporary_storage {
        result.push(types::Capability::TemporaryStorage);
    }
    if permissions.clocks {
        result.push(types::Capability::ClockRead);
    }
    if permissions.randomness {
        result.push(types::Capability::RandomRead);
    }
    result
}

fn validate_input_size(
    invocation: &ComponentInvocation,
    limit: usize,
) -> Result<(), WasiDiagnostic> {
    let size = match invocation {
        ComponentInvocation::Transform(document) => document.payload.len(),
        ComponentInvocation::Import(request) => request.payload.len(),
        ComponentInvocation::Export(request) => request.document.payload.len(),
    };
    if size > limit {
        return Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiOutputLimit,
            "component input exceeds configured limit",
        ));
    }
    Ok(())
}

fn validate_output_size(
    result: &ComponentInvocationResult,
    limit: usize,
) -> Result<(), WasiDiagnostic> {
    let size = match result {
        ComponentInvocationResult::Patch(patch) => patch
            .operations
            .iter()
            .map(|operation| match operation {
                types::PatchOperation::ReplaceDocument(value) => value.payload.len(),
                types::PatchOperation::SetMetadata(value) => value.key.len() + value.value.len(),
                types::PatchOperation::RemoveMetadata(value) => value.len(),
            })
            .sum(),
        ComponentInvocationResult::Document(document) => document.payload.len(),
        ComponentInvocationResult::Export(result) => result.payload.len(),
    };
    if size > limit {
        return Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiOutputLimit,
            "component output exceeds configured limit",
        ));
    }
    Ok(())
}

fn validate_result(
    result: &ComponentInvocationResult,
    expected_patch_schema: Option<&str>,
    diagnostic_count_limit: usize,
    diagnostic_bytes_limit: usize,
) -> Result<(), WasiDiagnostic> {
    let diagnostics = match result {
        ComponentInvocationResult::Patch(patch) => {
            if expected_patch_schema != Some(patch.base_schema_version.as_str())
                || patch.operations.iter().any(|operation| match operation {
                    types::PatchOperation::ReplaceDocument(value) => value.media_type.is_empty(),
                    types::PatchOperation::SetMetadata(value) => value.key.is_empty(),
                    types::PatchOperation::RemoveMetadata(value) => value.is_empty(),
                })
            {
                return Err(WasiDiagnostic::new(
                    WasiDiagnosticCode::PluginWasiInvalidPatch,
                    "component returned an invalid document patch",
                ));
            }
            &patch.diagnostics
        }
        ComponentInvocationResult::Document(_) => return Ok(()),
        ComponentInvocationResult::Export(result) => &result.diagnostics,
    };
    let diagnostic_bytes = diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic.message.len() + diagnostic.field.as_ref().map_or(0, String::len)
        })
        .sum::<usize>();
    if diagnostics.len() > diagnostic_count_limit || diagnostic_bytes > diagnostic_bytes_limit {
        return Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiOutputLimit,
            "component diagnostics exceed configured limits",
        ));
    }
    Ok(())
}

fn plugin_error(error: types::PluginError) -> WasiDiagnostic {
    let code = match error.code {
        types::PluginErrorCode::PermissionDenied => WasiDiagnosticCode::PluginWasiPermissionDenied,
        types::PluginErrorCode::Cancelled => WasiDiagnosticCode::PluginWasiCancelled,
        types::PluginErrorCode::InvalidInput => WasiDiagnosticCode::PluginWasiInvalidPatch,
        types::PluginErrorCode::Unsupported | types::PluginErrorCode::Internal => {
            WasiDiagnosticCode::PluginWasiTrap
        }
    };
    WasiDiagnostic::new(code, error.message)
}

fn permission_plugin_error(message: impl Into<String>) -> types::PluginError {
    types::PluginError {
        code: types::PluginErrorCode::PermissionDenied,
        message: message.into(),
        diagnostics: Vec::new(),
    }
}

fn invalid_input_plugin_error(message: impl Into<String>) -> types::PluginError {
    types::PluginError {
        code: types::PluginErrorCode::InvalidInput,
        message: message.into(),
        diagnostics: Vec::new(),
    }
}

fn classify_runtime_error(error: wasmtime::Error) -> WasiDiagnostic {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let code = if lower.contains("fuel") || lower.contains("epoch") || lower.contains("interrupt") {
        WasiDiagnosticCode::PluginWasiTimeout
    } else if lower.contains("memory") && (lower.contains("limit") || lower.contains("grow")) {
        WasiDiagnosticCode::PluginWasiMemoryLimit
    } else if lower.contains("component") || lower.contains("import") || lower.contains("export") {
        WasiDiagnosticCode::PluginWasiProtocolMismatch
    } else {
        WasiDiagnosticCode::PluginWasiTrap
    };
    WasiDiagnostic::new(code, message)
}

fn protocol_mismatch(message: impl Into<String>) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiProtocolMismatch, message)
}

fn host_failure(error: impl std::fmt::Display) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, error.to_string())
}

struct ConcurrencyGate {
    limit: usize,
    active: Mutex<usize>,
    ready: Condvar,
}

struct EpochTicker {
    stop: mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl EpochTicker {
    fn new(engine: Engine) -> Self {
        let (stop, receiver) = mpsc::channel();
        let thread = thread::spawn(move || loop {
            match receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => engine.increment_epoch(),
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl ConcurrencyGate {
    fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            active: Mutex::new(0),
            ready: Condvar::new(),
        }
    }

    fn acquire_until(
        self: &Arc<Self>,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<ConcurrencyPermit, WasiDiagnostic> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while *active >= self.limit {
            if cancellation.is_cancelled() {
                return Err(WasiDiagnostic::new(
                    WasiDiagnosticCode::PluginWasiCancelled,
                    "component execution was cancelled while waiting for capacity",
                ));
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(WasiDiagnostic::new(
                    WasiDiagnosticCode::PluginWasiTimeout,
                    "component execution timed out while waiting for capacity",
                ));
            };
            let (next, _) = self
                .ready
                .wait_timeout(active, remaining.min(Duration::from_millis(5)))
                .unwrap_or_else(|error| error.into_inner());
            active = next;
        }
        *active += 1;
        Ok(ConcurrencyPermit {
            gate: Arc::clone(self),
        })
    }
}

struct ConcurrencyPermit {
    gate: Arc<ConcurrencyGate>,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        let mut active = self
            .gate
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *active = active.saturating_sub(1);
        self.gate.ready.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::component::TypedFunc;

    fn interrupt_engine() -> Engine {
        let mut config = Config::new();
        config.epoch_interruption(true);
        config.consume_fuel(true);
        Engine::new(&config).unwrap()
    }

    fn component(engine: &Engine, body: &str) -> Component {
        let bytes = wat::parse_str(body).unwrap();
        Component::new(engine, bytes).unwrap()
    }

    fn infinite_component(engine: &Engine) -> Component {
        component(
            engine,
            r#"
                (component
                    (core module $module
                        (func (export "run")
                            (loop $forever (br $forever))))
                    (core instance $instance (instantiate $module))
                    (func $run (canon lift (core func $instance "run")))
                    (export "run" (func $run)))
            "#,
        )
    }

    fn finite_component(engine: &Engine) -> Component {
        component(
            engine,
            r#"
                (component
                    (core module $module (func (export "run")))
                    (core instance $instance (instantiate $module))
                    (func $run (canon lift (core func $instance "run")))
                    (export "run" (func $run)))
            "#,
        )
    }

    fn instantiate_probe(
        engine: &Engine,
        component: &Component,
        cancellation: CancellationToken,
        deadline: Instant,
        interrupt: Arc<AtomicU8>,
    ) -> (Store<()>, TypedFunc<(), ()>) {
        let linker = Linker::new(engine);
        let mut store = Store::new(engine, ());
        store.set_fuel(u64::MAX).unwrap();
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            if cancellation.is_cancelled() {
                interrupt.store(INTERRUPT_CANCELLED, Ordering::Release);
                return Ok(UpdateDeadline::Interrupt);
            }
            if Instant::now() >= deadline {
                interrupt.store(INTERRUPT_TIMEOUT, Ordering::Release);
                return Ok(UpdateDeadline::Interrupt);
            }
            Ok(UpdateDeadline::Continue(1))
        });
        let instance = linker.instantiate(&mut store, component).unwrap();
        let function = instance
            .get_typed_func::<(), ()>(&mut store, "run")
            .unwrap();
        (store, function)
    }

    #[test]
    fn epoch_ticker_hard_interrupts_an_infinite_component() {
        let engine = interrupt_engine();
        let _ticker = EpochTicker::new(engine.clone());
        let component = infinite_component(&engine);
        let interrupt = Arc::new(AtomicU8::new(INTERRUPT_NONE));
        let (mut store, function) = instantiate_probe(
            &engine,
            &component,
            CancellationToken::default(),
            Instant::now() + Duration::from_millis(25),
            Arc::clone(&interrupt),
        );
        assert!(function.call(&mut store, ()).is_err());
        assert_eq!(interrupt.load(Ordering::Acquire), INTERRUPT_TIMEOUT);
    }

    #[test]
    fn cancellation_interrupts_and_engine_remains_reusable() {
        let engine = interrupt_engine();
        let _ticker = EpochTicker::new(engine.clone());
        let component = infinite_component(&engine);
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let interrupt = Arc::new(AtomicU8::new(INTERRUPT_NONE));
        let (mut store, function) = instantiate_probe(
            &engine,
            &component,
            cancellation,
            Instant::now() + Duration::from_secs(1),
            Arc::clone(&interrupt),
        );
        assert!(function.call(&mut store, ()).is_err());
        assert_eq!(interrupt.load(Ordering::Acquire), INTERRUPT_CANCELLED);
        drop(store);

        let finite = finite_component(&engine);
        let (mut store, function) = instantiate_probe(
            &engine,
            &finite,
            CancellationToken::default(),
            Instant::now() + Duration::from_secs(1),
            Arc::new(AtomicU8::new(INTERRUPT_NONE)),
        );
        function.call(&mut store, ()).unwrap();
        function.post_return(&mut store).unwrap();
    }
}

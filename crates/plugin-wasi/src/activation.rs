use latexsnipper_plugin::{CancellationToken, PluginManifestV3, RemotePluginStore, TrustState};
use semver::Version;

use crate::{
    CompiledWasiComponent, ComponentInvocation, ComponentInvocationResult, WasiComponentHost,
    WasiComponentPackageVerifier, WasiDiagnostic, WasiDiagnosticCode, WasiHostPolicy,
};

/// Executable view of a signed, verified, and explicitly enabled remote WASI
/// plugin. Activation re-verifies the unpacked package with the host's
/// handle-relative verifier before compiling the component.
pub struct ActivatedRemoteWasiPlugin {
    manifest: PluginManifestV3,
    package_sha256: String,
    store: RemotePluginStore,
    host: WasiComponentHost,
    compiled: CompiledWasiComponent,
}

impl ActivatedRemoteWasiPlugin {
    pub fn activate(store: &RemotePluginStore, id: &str) -> Result<Self, WasiDiagnostic> {
        Self::activate_with_policy(store, id, WasiHostPolicy::default())
    }

    pub fn activate_with_policy(
        store: &RemotePluginStore,
        id: &str,
        policy: WasiHostPolicy,
    ) -> Result<Self, WasiDiagnostic> {
        let (directory, installed) = store
            .enabled_package_directory(id)
            .map_err(|error| activation_error(error.to_string()))?;
        let core_version = Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|error| activation_error(error.to_string()))?;
        let package = WasiComponentPackageVerifier::new(core_version)
            .with_host_policy(policy)
            .verify_directory(directory)?;
        let registry_manifest = serde_json::to_vec(&installed.manifest)
            .map_err(|error| activation_error(error.to_string()))?;
        let host_manifest = serde_json::to_vec(&package.manifest)
            .map_err(|error| activation_error(error.to_string()))?;
        if package.manifest.id != id
            || package.component_sha256
                != installed
                    .manifest
                    .artifact
                    .as_ref()
                    .map(|artifact| artifact.sha256.to_ascii_lowercase())
                    .unwrap_or_default()
            || host_manifest != registry_manifest
        {
            return Err(activation_error(
                "verified remote plugin identity or artifact changed during activation",
            ));
        }
        let manifest = package.manifest.clone();
        let host = WasiComponentHost::new(package)?;
        let compiled = host.compile()?;
        Ok(Self {
            manifest,
            package_sha256: installed.provenance.package_sha256,
            store: store.clone(),
            host,
            compiled,
        })
    }

    pub fn manifest(&self) -> &PluginManifestV3 {
        &self.manifest
    }

    pub fn component_sha256(&self) -> &str {
        self.compiled.sha256()
    }

    pub fn execute(
        &self,
        invocation: ComponentInvocation,
        cancellation: CancellationToken,
    ) -> Result<ComponentInvocationResult, WasiDiagnostic> {
        self.ensure_still_enabled()?;
        self.host.execute(&self.compiled, invocation, cancellation)
    }

    fn ensure_still_enabled(&self) -> Result<(), WasiDiagnostic> {
        let current = self
            .store
            .get(&self.manifest.id)
            .map_err(|error| activation_error(error.to_string()))?
            .ok_or_else(|| activation_error("remote plugin is no longer installed"))?;
        let current_manifest = serde_json::to_vec(&current.manifest)
            .map_err(|error| activation_error(error.to_string()))?;
        let activated_manifest = serde_json::to_vec(&self.manifest)
            .map_err(|error| activation_error(error.to_string()))?;
        if !current.active
            || !current.enabled
            || current.trust_state != TrustState::VerifiedWasiComponent
            || !current
                .provenance
                .package_sha256
                .eq_ignore_ascii_case(&self.package_sha256)
            || current_manifest != activated_manifest
        {
            return Err(activation_error(
                "remote plugin was disabled, revoked, updated, or replaced after activation",
            ));
        }
        Ok(())
    }
}

fn activation_error(message: impl Into<String>) -> WasiDiagnostic {
    let message = message.into();
    let boundary = message
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= 1021)
        .last()
        .unwrap_or(0);
    let message = if message.len() <= 1024 {
        message
    } else {
        format!("{}...", &message[..boundary])
    };
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiProtocolMismatch, message)
}

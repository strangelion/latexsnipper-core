use latexsnipper_plugin::{CancellationToken, PluginManifestV3, RemotePluginStore, TrustState};
use semver::Version;
use sha2::{Digest, Sha256};

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
    pub fn with_protected_approval_storage(
        mut self,
        isolation: std::sync::Arc<dyn crate::ApprovalStorageIsolation>,
    ) -> Result<Self, WasiDiagnostic> {
        self.ensure_still_enabled()?;
        self.host = self.host.with_protected_approval_storage(isolation)?;
        Ok(self)
    }
    pub fn approval_controller(&self) -> std::sync::Arc<crate::SensitiveActionController> {
        self.host.approval_controller()
    }

    /// Opt-in scope identity, not consent. Includes the installed archive identity.
    pub fn with_remembered_approval_binding(
        self,
        trusted_domain: &str,
    ) -> Result<Self, WasiDiagnostic> {
        self.bind_remembered_profile(trusted_domain, None)
    }

    pub fn remembered_approval_binding(&self) -> Option<&str> {
        self.host.remembered_approval_binding()
    }

    /// Trusted-host opt-in. Bind the archive scope before attaching a policy store.
    pub fn with_remembered_policies(
        mut self,
        policies: std::sync::Arc<crate::RememberedApprovalPolicies>,
    ) -> Result<Self, WasiDiagnostic> {
        self.ensure_still_enabled()?;
        self.host = self.host.with_remembered_policies(policies)?;
        Ok(self)
    }

    pub(crate) fn bind_remembered_profile(
        mut self,
        trusted_domain: &str,
        profile_sha256: Option<&str>,
    ) -> Result<Self, WasiDiagnostic> {
        if trusted_domain.is_empty() || trusted_domain.len() > 4096 {
            return Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiInvalidInput,
                "remembered approval host identity exceeds bounds",
            ));
        }
        self.ensure_still_enabled()?;
        let identity = serde_json::to_vec(&(
            "core-wasi-remote-remembered-binding-v1",
            trusted_domain,
            self.package_sha256.to_ascii_lowercase(),
            profile_sha256,
        ))
        .map_err(|_| activation_error("cannot bind remembered remote approval identity"))?;
        // This route exposes no custom broker/configuration injection. Version this
        // identity if its fixed broker policy changes; never accept it from a guest.
        self.host = self.host.with_remembered_approval_binding(
            &hex::encode(Sha256::digest(identity)),
            b"core-remote-default-deny-broker-v1",
        )?;
        Ok(self)
    }

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

    pub(crate) fn resource_limits(&self) -> &crate::WasiResourceLimits {
        self.host.resource_limits()
    }

    pub fn execute(
        &self,
        invocation: ComponentInvocation,
        cancellation: CancellationToken,
    ) -> Result<ComponentInvocationResult, WasiDiagnostic> {
        self.ensure_still_enabled()?;
        self.host.execute(&self.compiled, invocation, cancellation)
    }

    pub(crate) fn ensure_still_enabled(&self) -> Result<(), WasiDiagnostic> {
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

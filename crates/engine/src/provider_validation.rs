//! Process-local provider validation cache with environment-bound reuse.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use latexsnipper_api_types::{
    CoreErrorCode, EphemeralProviderKey, ProviderValidationKey, ProviderValidationLevel,
    ProviderValidationReport, ValidationScope,
};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::is_weak_observation;

pub struct ProviderValidationStore {
    ephemeral: RwLock<BTreeMap<EphemeralProviderKey, ProviderValidationReport>>,
    persistent: RwLock<BTreeMap<ProviderValidationKey, ProviderValidationReport>>,
    runtime_instance_id: String,
    session_generation: AtomicU64,
}

impl Default for ProviderValidationStore {
    fn default() -> Self {
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
        let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let runtime_instance_id = current_process_id().map_or_else(
            || format!("wasm-{instance}"),
            |process_id| format!("{process_id}-{instance}"),
        );
        Self {
            ephemeral: RwLock::new(BTreeMap::new()),
            persistent: RwLock::new(BTreeMap::new()),
            runtime_instance_id,
            session_generation: AtomicU64::new(0),
        }
    }
}

impl ProviderValidationStore {
    pub fn record(&self, mut report: ProviderValidationReport) -> Result<()> {
        let key = report.key.clone().ok_or_else(|| {
            SnipperError::Runtime(
                "provider validation reports must carry a complete environment key".to_owned(),
            )
        })?;
        if !key.provider.eq_ignore_ascii_case(&report.provider) {
            return Err(SnipperError::Runtime(
                "provider validation key does not match report provider".to_owned(),
            ));
        }
        if report.validated_at == 0 {
            report.validated_at = now_unix_millis();
        }
        if report.runtime_instance_id.is_empty() {
            report.runtime_instance_id = self.runtime_instance_id.clone();
        }
        report.stale = false;
        match report.scope {
            ValidationScope::CurrentProcess => {
                report.reusable_across_restart = false;
                if report.session_generation == 0 {
                    report.session_generation = self.next_session_generation();
                }
                let ephemeral_key = EphemeralProviderKey {
                    process_id: current_process_id(),
                    runtime_instance_id: report.runtime_instance_id.clone(),
                    session_generation: report.session_generation,
                    provider: report.provider.to_ascii_lowercase(),
                    smoke_model_sha256: key.smoke_model_sha256.clone(),
                };
                self.ephemeral
                    .write()
                    .map_err(|_| {
                        SnipperError::Runtime("provider validation store poisoned".to_owned())
                    })?
                    .insert(ephemeral_key, report);
                Ok(())
            }
            ValidationScope::PersistentTrusted => {
                if !report.reusable_across_restart || !persistent_key_is_strong(&key) {
                    return Err(SnipperError::Runtime(
                        "persistent provider evidence requires strong runtime binary, provider library, driver, device, and smoke-model identity"
                            .to_owned(),
                    ));
                }
                self.persistent
                    .write()
                    .map_err(|_| {
                        SnipperError::Runtime("provider validation store poisoned".to_owned())
                    })?
                    .insert(key, report);
                Ok(())
            }
            ValidationScope::Stale => Err(SnipperError::Runtime(
                "stale provider evidence cannot be recorded as reusable validation".to_owned(),
            )),
        }
    }

    /// Return an exact cached result. If only a report for a different
    /// environment exists, return a visibly stale ProbePassed downgrade.
    pub fn lookup(
        &self,
        current: &ProviderValidationKey,
    ) -> Result<Option<ProviderValidationReport>> {
        let ephemeral = self
            .ephemeral
            .read()
            .map_err(|_| SnipperError::Runtime("provider validation store poisoned".to_owned()))?;
        let exact_ephemeral = ephemeral
            .iter()
            .filter(|(key, _)| {
                key.process_id == current_process_id()
                    && key.runtime_instance_id == self.runtime_instance_id
                    && key.provider.eq_ignore_ascii_case(&current.provider)
                    && key.smoke_model_sha256 == current.smoke_model_sha256
            })
            .max_by_key(|(key, _)| key.session_generation)
            .map(|(_, report)| report.clone());
        drop(ephemeral);
        if exact_ephemeral.is_some() {
            return Ok(exact_ephemeral);
        }

        let persistent = self
            .persistent
            .read()
            .map_err(|_| SnipperError::Runtime("provider validation store poisoned".to_owned()))?;
        if let Some(report) = persistent.get(current) {
            return Ok(Some(report.clone()));
        }
        let mut stale_candidates = persistent
            .values()
            .filter(|report| report.provider.eq_ignore_ascii_case(&current.provider))
            .cloned()
            .collect::<Vec<_>>();
        stale_candidates.sort_by(|left, right| {
            let left_key = left.key.as_ref();
            let right_key = right.key.as_ref();
            let left_platform = left_key.is_some_and(|key| {
                key.os == current.os && key.architecture == current.architecture
            });
            let right_platform = right_key.is_some_and(|key| {
                key.os == current.os && key.architecture == current.architecture
            });
            right_platform
                .cmp(&left_platform)
                .then_with(|| right.validated_at.cmp(&left.validated_at))
                .then_with(|| left.runtime_instance_id.cmp(&right.runtime_instance_id))
        });
        let stale = stale_candidates.into_iter().next();
        Ok(stale.map(|mut report| {
            report.validation_level = ProviderValidationLevel::ProbePassed;
            report.session_created = false;
            report.smoke_inference_passed = false;
            report.benchmark_measured = false;
            report.benchmark_validated = false;
            report.scope = ValidationScope::Stale;
            report.reusable_across_restart = false;
            report.key = Some(current.clone());
            report.stale = true;
            report.diagnostics.push(format!(
                "{}: cached provider validation belongs to a different runtime, library, device, driver, or smoke model",
                CoreErrorCode::ProviderValidationStale.as_str()
            ));
            report
        }))
    }

    pub fn clear(&self) {
        self.clear_ephemeral();
        if let Ok(mut reports) = self.persistent.write() {
            reports.clear();
        }
    }

    pub fn clear_ephemeral(&self) {
        if let Ok(mut reports) = self.ephemeral.write() {
            reports.clear();
        }
        self.session_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn runtime_instance_id(&self) -> &str {
        &self.runtime_instance_id
    }

    pub fn next_session_generation(&self) -> u64 {
        self.session_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn len(&self) -> usize {
        self.ephemeral
            .read()
            .map(|reports| reports.len())
            .unwrap_or_default()
            + self
                .persistent
                .read()
                .map(|reports| reports.len())
                .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn current_process_id() -> Option<u32> {
    Some(std::process::id())
}

#[cfg(target_arch = "wasm32")]
fn current_process_id() -> Option<u32> {
    None
}

fn persistent_key_is_strong(key: &ProviderValidationKey) -> bool {
    [
        key.runtime_version.as_str(),
        key.provider_library_fingerprint.as_str(),
        key.device_driver_fingerprint.as_str(),
        key.smoke_model_sha256.as_str(),
        key.runtime_binary_sha256.as_str(),
        key.provider_library_sha256.as_str(),
        key.device_identity.as_str(),
    ]
    .iter()
    .all(|value| !value.is_empty() && !is_weak_observation(value))
}

fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(runtime_version: &str, library: &str) -> ProviderValidationKey {
        ProviderValidationKey {
            core_version: "3.1.0".to_owned(),
            runtime_version: runtime_version.to_owned(),
            provider: "directml".to_owned(),
            provider_library_fingerprint: library.to_owned(),
            os: "windows".to_owned(),
            architecture: "x86_64".to_owned(),
            device_driver_fingerprint: "gpu-driver".to_owned(),
            smoke_model_sha256: "a".repeat(64),
            runtime_binary_sha256: "b".repeat(64),
            provider_library_sha256: "c".repeat(64),
            device_identity: "pci:0000:01:00.0".to_owned(),
        }
    }

    fn report(key: ProviderValidationKey) -> ProviderValidationReport {
        ProviderValidationReport {
            provider: "directml".to_owned(),
            validation_level: ProviderValidationLevel::SmokeInferencePassed,
            library_detected: true,
            probe_passed: true,
            session_created: true,
            smoke_inference_passed: true,
            benchmark_measured: false,
            benchmark_validated: false,
            scope: ValidationScope::CurrentProcess,
            reusable_across_restart: false,
            validated_at: 1,
            duration_ms: 5,
            runtime_instance_id: String::new(),
            session_generation: 0,
            last_failure_code: None,
            key: Some(key),
            stale: false,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn exact_environment_reuses_smoke_result() {
        let store = ProviderValidationStore::default();
        let current = key("ort-1", "library-a");
        store.record(report(current.clone())).unwrap();
        let cached = store.lookup(&current).unwrap().unwrap();
        assert_eq!(
            cached.validation_level,
            ProviderValidationLevel::SmokeInferencePassed
        );
        assert!(!cached.stale);
    }

    #[test]
    fn runtime_or_library_change_downgrades_to_probe() {
        let store = ProviderValidationStore::default();
        let mut persistent = report(key("ort-1", "library-a"));
        persistent.scope = ValidationScope::PersistentTrusted;
        persistent.reusable_across_restart = true;
        store.record(persistent).unwrap();
        for current in [key("ort-2", "library-a"), key("ort-1", "library-b")] {
            let cached = store.lookup(&current).unwrap().unwrap();
            assert_eq!(
                cached.validation_level,
                ProviderValidationLevel::ProbePassed
            );
            assert!(cached.stale);
            assert!(!cached.session_created);
            assert!(!cached.smoke_inference_passed);
        }
    }

    #[test]
    fn smoke_failure_is_preserved_and_never_promoted() {
        let store = ProviderValidationStore::default();
        let current = key("ort-1", "library-a");
        let mut failed = report(current.clone());
        failed.validation_level = ProviderValidationLevel::SessionCreated;
        failed.smoke_inference_passed = false;
        failed.diagnostics.push(
            CoreErrorCode::ProviderSmokeInferenceFailed
                .as_str()
                .to_owned(),
        );
        store.record(failed).unwrap();
        let cached = store.lookup(&current).unwrap().unwrap();
        assert_eq!(
            cached.validation_level,
            ProviderValidationLevel::SessionCreated
        );
        assert!(!cached.smoke_inference_passed);
        assert!(cached
            .diagnostics
            .iter()
            .any(|message| message == "PROVIDER_SMOKE_INFERENCE_FAILED"));
    }

    #[test]
    fn descriptive_runtime_hashes_cannot_key_smoke_cache() {
        let store = ProviderValidationStore::default();
        let mut weak = key("ort-1", "runtime-version-sha256:abc");
        weak.device_driver_fingerprint = "runtime-device-sha256:def".to_owned();
        let mut persistent = report(weak);
        persistent.scope = ValidationScope::PersistentTrusted;
        persistent.reusable_across_restart = true;
        let error = store.record(persistent).unwrap_err();
        assert!(error
            .to_string()
            .contains("persistent provider evidence requires strong"));
    }

    #[test]
    fn ephemeral_smoke_survives_readiness_refresh_but_not_session_reset() {
        let store = ProviderValidationStore::default();
        let current = key("ort-1", "runtime-version-sha256:abc");
        store.record(report(current.clone())).unwrap();
        assert!(
            store
                .lookup(&current)
                .unwrap()
                .unwrap()
                .smoke_inference_passed
        );
        store.clear_ephemeral();
        assert!(store.lookup(&current).unwrap().is_none());
    }

    #[test]
    fn stale_report_selection_is_deterministic_and_prefers_platform_then_time() {
        let store = ProviderValidationStore::default();
        let mut older = report(key("ort-old", "library-a"));
        older.scope = ValidationScope::PersistentTrusted;
        older.reusable_across_restart = true;
        older.validated_at = 10;
        store.record(older).unwrap();

        let mut newer_key = key("ort-new", "library-b");
        newer_key.os = "linux".to_owned();
        let mut newer = report(newer_key);
        newer.scope = ValidationScope::PersistentTrusted;
        newer.reusable_across_restart = true;
        newer.validated_at = 20;
        store.record(newer).unwrap();

        let stale = store
            .lookup(&key("ort-current", "library-c"))
            .unwrap()
            .unwrap();
        assert_eq!(stale.validated_at, 10);
        assert_eq!(stale.scope, ValidationScope::Stale);
    }
}

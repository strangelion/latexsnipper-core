//! Process-local provider validation cache with environment-bound reuse.

use std::collections::HashMap;
use std::sync::RwLock;

use latexsnipper_api_types::{
    CoreErrorCode, ProviderValidationKey, ProviderValidationLevel, ProviderValidationReport,
};
use latexsnipper_foundation::{Result, SnipperError};

#[derive(Default)]
pub struct ProviderValidationStore {
    reports: RwLock<HashMap<ProviderValidationKey, ProviderValidationReport>>,
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
        report.stale = false;
        self.reports
            .write()
            .map_err(|_| SnipperError::Runtime("provider validation store poisoned".to_owned()))?
            .insert(key, report);
        Ok(())
    }

    /// Return an exact cached result. If only a report for a different
    /// environment exists, return a visibly stale ProbePassed downgrade.
    pub fn lookup(
        &self,
        current: &ProviderValidationKey,
    ) -> Result<Option<ProviderValidationReport>> {
        let reports = self
            .reports
            .read()
            .map_err(|_| SnipperError::Runtime("provider validation store poisoned".to_owned()))?;
        if let Some(report) = reports.get(current) {
            return Ok(Some(report.clone()));
        }
        let stale = reports
            .values()
            .find(|report| report.provider.eq_ignore_ascii_case(&current.provider))
            .cloned();
        Ok(stale.map(|mut report| {
            report.validation_level = ProviderValidationLevel::ProbePassed;
            report.session_created = false;
            report.smoke_inference_passed = false;
            report.benchmark_validated = false;
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
        if let Ok(mut reports) = self.reports.write() {
            reports.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.reports
            .read()
            .map(|reports| reports.len())
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
            benchmark_validated: false,
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
        store.record(report(key("ort-1", "library-a"))).unwrap();
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
}

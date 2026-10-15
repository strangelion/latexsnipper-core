//! Explicit owned preference objects. No process-global registry or disk IO.
use std::sync::{Mutex, MutexGuard, TryLockError};

use latexsnipper_conversion::formula_completion::preferences::persistence::*;
use latexsnipper_conversion::formula_completion::{
    fragments::*, preferences::*, scoped::FormulaDefinitionScope,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::formula_completion::{encode_result, input_format, revision, scoped_completion_error};
use crate::formula_fragments::{catalog, dict_json_utf8, snapshot};
use crate::{json_to_python, python_error, BindingError};

fn error(code: &str, message: &str) -> BindingError {
    BindingError {
        code: code.into(),
        message: message.into(),
        detail: None,
        retryable: code == "PREFERENCE_BUSY",
    }
}
fn preference_error(value: FormulaPreferenceError) -> BindingError {
    if let FormulaPreferenceError::Completion(value) = value {
        return scoped_completion_error(value);
    }
    error(
        match value {
            FormulaPreferenceError::InvalidIdentity => "INVALID_ARGUMENT",
            FormulaPreferenceError::ScopeMismatch => "SCOPE_MISMATCH",
            FormulaPreferenceError::StaleFeedback => "STALE_FEEDBACK",
            FormulaPreferenceError::LimitExceeded => "LIMIT_EXCEEDED",
            FormulaPreferenceError::Completion(_) => unreachable!(),
        },
        &value.to_string(),
    )
}
fn storage_error(value: FormulaPreferenceStorageError) -> BindingError {
    error(
        match value {
            FormulaPreferenceStorageError::Disabled => "PREFERENCE_STORAGE_DISABLED",
            FormulaPreferenceStorageError::InvalidKey => "INVALID_ARGUMENT",
            FormulaPreferenceStorageError::InvalidSnapshot => "INVALID_SNAPSHOT",
            FormulaPreferenceStorageError::AuthenticationFailed => "AUTHENTICATION_FAILED",
            FormulaPreferenceStorageError::ScopeMismatch => "SCOPE_MISMATCH",
            FormulaPreferenceStorageError::StaleCheckpoint => "STALE_CHECKPOINT",
            FormulaPreferenceStorageError::SensitiveStorageDenied => "SENSITIVE_STORAGE_DENIED",
            FormulaPreferenceStorageError::LimitExceeded => "LIMIT_EXCEEDED",
        },
        &value.to_string(),
    )
}
fn decode<T: DeserializeOwned>(encoded: &str) -> Result<T, BindingError> {
    if encoded.len() > 4096 {
        return Err(BindingError::invalid(
            "Preference metadata exceeds 4 KiB JSON.",
        ));
    }
    serde_json::from_str(encoded).map_err(|_| {
        BindingError::invalid("Invalid closed preference metadata or revision encoding.")
    })
}
fn host_key(key: &Bound<'_, PyBytes>) -> PyResult<[u8; 32]> {
    key.as_bytes().try_into().map_err(|_| {
        python_error(
            key.py(),
            BindingError::invalid("Preference key must be exactly 32 bytes."),
        )
    })
}

/// Opaque, non-constructible and non-serializable. Not a permission credential.
#[pyclass(
    frozen,
    name = "FormulaPreferenceCapture",
    module = "latexsnipper_core._native"
)]
pub struct PreferenceCapture {
    value: FormulaPreferenceCapture,
}

/// Opaque instance-bound, single-use undo receipt. Never contains formula text.
#[pyclass(
    frozen,
    name = "FormulaPreferenceReceipt",
    module = "latexsnipper_core._native"
)]
pub struct PreferenceReceipt {
    value: FormulaPreferenceReceipt,
}

#[pyclass(frozen, module = "latexsnipper_core._native")]
pub struct FormulaPreferences {
    state: Mutex<Option<FormulaPreferenceStore>>,
}
impl FormulaPreferences {
    fn lock(&self) -> Result<MutexGuard<'_, Option<FormulaPreferenceStore>>, BindingError> {
        self.state.try_lock().map_err(|cause| match cause {
            TryLockError::WouldBlock => error(
                "PREFERENCE_BUSY",
                "Preference instance is busy; no operation was replayed.",
            ),
            TryLockError::Poisoned(_) => {
                BindingError::internal("Preference instance lock is poisoned.")
            }
        })
    }
    fn with_store<T>(
        &self,
        action: impl FnOnce(&mut FormulaPreferenceStore) -> Result<T, BindingError>,
    ) -> Result<T, BindingError> {
        let mut state = self.lock()?;
        action(
            state
                .as_mut()
                .ok_or_else(|| error("PREFERENCE_CLOSED", "Preference instance is closed."))?,
        )
    }
}

#[pymethods]
impl FormulaPreferences {
    #[new]
    #[pyo3(signature = (*, scope, key))]
    fn new(py: Python<'_>, scope: &Bound<'_, PyDict>, key: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let encoded = dict_json_utf8(scope)?;
        let key = host_key(key)?;
        let state = py
            .detach(move || {
                FormulaPreferenceStore::new(decode::<FormulaDefinitionScope>(&encoded)?, key)
                    .map_err(preference_error)
            })
            .map_err(|cause| python_error(py, cause))?;
        Ok(Self {
            state: Mutex::new(Some(state)),
        })
    }

    /// Explicit restore never reads files or recovers old authorization/tokens.
    #[staticmethod]
    #[pyo3(signature = (*, scope, key, storage_key, expected_checkpoint, snapshot_json, permission))]
    #[allow(clippy::too_many_arguments)]
    fn restore_checkpoint(
        py: Python<'_>,
        scope: &Bound<'_, PyDict>,
        key: &Bound<'_, PyBytes>,
        storage_key: &Bound<'_, PyBytes>,
        expected_checkpoint: &Bound<'_, PyAny>,
        snapshot_json: String,
        permission: &Bound<'_, PyDict>,
    ) -> PyResult<Self> {
        if snapshot_json.len() > FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES {
            return Err(python_error(
                py,
                storage_error(FormulaPreferenceStorageError::LimitExceeded),
            ));
        }
        let scope = dict_json_utf8(scope)?;
        let key = host_key(key)?;
        let storage_key = host_key(storage_key)?;
        let expected_checkpoint =
            revision(expected_checkpoint).map_err(|cause| python_error(py, cause))?;
        let permission = dict_json_utf8(permission)?;
        let state = py
            .detach(move || {
                FormulaPreferenceStore::restore_checkpoint(
                    decode::<FormulaDefinitionScope>(&scope)?,
                    key,
                    &storage_key,
                    expected_checkpoint,
                    &snapshot_json,
                    decode::<FormulaPreferenceStoragePermission>(&permission)?,
                )
                .map_err(storage_error)
            })
            .map_err(|cause| python_error(py, cause))?;
        Ok(Self {
            state: Mutex::new(Some(state)),
        })
    }

    #[pyo3(signature = (*, storage_key, checkpoint, permission))]
    fn export_checkpoint(
        &self,
        py: Python<'_>,
        storage_key: &Bound<'_, PyBytes>,
        checkpoint: &Bound<'_, PyAny>,
        permission: &Bound<'_, PyDict>,
    ) -> PyResult<String> {
        let storage_key = host_key(storage_key)?;
        let checkpoint = revision(checkpoint).map_err(|cause| python_error(py, cause))?;
        let permission = dict_json_utf8(permission)?;
        py.detach(move || {
            let permission = decode::<FormulaPreferenceStoragePermission>(&permission)?;
            self.with_store(|state| {
                state
                    .export_checkpoint(&storage_key, checkpoint, permission)
                    .map_err(storage_error)
            })
        })
        .map_err(|cause| python_error(py, cause))
    }

    #[getter]
    fn closed(&self, py: Python<'_>) -> PyResult<bool> {
        self.lock()
            .map(|state| state.is_none())
            .map_err(|cause| python_error(py, cause))
    }

    fn info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let encoded = py
            .detach(|| {
                self.with_store(|state| encode_result(&json!({
            "schemaVersion": 1, "scope": state.scope(), "policy": state.policy(),
            "eventCount": state.event_count(), "maxEvents": FORMULA_PREFERENCE_MAX_EVENTS,
            "inMemory": true, "persistence": false, "automaticCapture": false,
            "authenticatedCheckpoints": true, "automaticStorage": false,
            "maxSnapshotBytes": FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,
            "opaqueFeedbackObjects": true, "maxCatalogBytes": 1024 * 1024,
            "maxResultDataBytes": 1024 * 1024, "maxMetadataBytes": 4096
        })))
            })
            .map_err(|cause| python_error(py, cause))?;
        json_to_python(py, encoded)
    }

    fn set_policy(&self, py: Python<'_>, policy: &Bound<'_, PyDict>) -> PyResult<()> {
        let encoded = dict_json_utf8(policy)?;
        py.detach(move || {
            let policy = decode::<FormulaPreferencePolicy>(&encoded)?;
            self.with_store(|state| state.set_policy(policy).map_err(preference_error))
        })
        .map_err(|cause| python_error(py, cause))
    }

    fn capture_context(&self, py: Python<'_>) -> PyResult<PreferenceCapture> {
        py.detach(|| {
            self.with_store(|state| {
                Ok(PreferenceCapture {
                    value: state.capture_context(),
                })
            })
        })
        .map_err(|cause| python_error(py, cause))
    }

    #[pyo3(signature = (source, *, cursor, revision, catalog, input_format="latex"))]
    fn rank_fragments(
        &self,
        py: Python<'_>,
        source: String,
        cursor: usize,
        revision: &Bound<'_, PyAny>,
        catalog: &Bound<'_, PyDict>,
        input_format: &str,
    ) -> PyResult<Py<PyAny>> {
        let revision = self::revision(revision).map_err(|cause| python_error(py, cause))?;
        let input_format =
            self::input_format(input_format).map_err(|cause| python_error(py, cause))?;
        let encoded = dict_json_utf8(catalog)?;
        let result = py.detach(move || {
            let request = FormulaFragmentCompletionRequest { source, cursor, revision, input_format, catalog: self::catalog(&encoded)? };
            self.with_store(|state| {
                let result = state.rank_fragments(&request).map_err(preference_error)?;
                encode_result(&json!({"completion":result.completion, "order":result.order, "features":result.features}))
            })
        }).map_err(|cause| python_error(py, cause))?;
        json_to_python(py, result)
    }

    /// Capture BEFORE dispatch; invoke only AFTER a real host edit succeeded.
    #[pyo3(signature = (capture, source, *, cursor, revision, catalog, snapshot, candidate_id, event_id, input_format="latex"))]
    #[allow(clippy::too_many_arguments)]
    fn record_acceptance_at(
        &self,
        py: Python<'_>,
        capture: &PreferenceCapture,
        source: String,
        cursor: usize,
        revision: &Bound<'_, PyAny>,
        catalog: &Bound<'_, PyDict>,
        snapshot: &Bound<'_, PyDict>,
        candidate_id: String,
        event_id: &Bound<'_, PyAny>,
        input_format: &str,
    ) -> PyResult<Option<PreferenceReceipt>> {
        let revision = self::revision(revision).map_err(|cause| python_error(py, cause))?;
        let event_id = self::revision(event_id).map_err(|cause| python_error(py, cause))?;
        let input_format =
            self::input_format(input_format).map_err(|cause| python_error(py, cause))?;
        if candidate_id.is_empty() || candidate_id.len() > 256 {
            return Err(python_error(
                py,
                BindingError::invalid("Invalid preference candidate ID length."),
            ));
        }
        let encoded_catalog = dict_json_utf8(catalog)?;
        let encoded_snapshot = dict_json_utf8(snapshot)?;
        py.detach(move || {
            let request = FormulaFragmentCompletionApplyRequest {
                source,
                cursor,
                revision,
                input_format,
                catalog: self::catalog(&encoded_catalog)?,
                snapshot: self::snapshot(&encoded_snapshot)?,
                candidate_id,
            };
            self.with_store(|state| {
                state
                    .record_acceptance_at(&capture.value, &request, event_id)
                    .map(|receipt| receipt.map(|value| PreferenceReceipt { value }))
                    .map_err(preference_error)
            })
        })
        .map_err(|cause| python_error(py, cause))
    }

    fn record_undo(&self, py: Python<'_>, receipt: &PreferenceReceipt) -> PyResult<()> {
        py.detach(|| {
            self.with_store(|state| state.record_undo(&receipt.value).map_err(preference_error))
        })
        .map_err(|cause| python_error(py, cause))
    }

    #[pyo3(signature = (*, generation, key))]
    fn clear(
        &self,
        py: Python<'_>,
        generation: &Bound<'_, PyAny>,
        key: &Bound<'_, PyBytes>,
    ) -> PyResult<()> {
        let generation = revision(generation).map_err(|cause| python_error(py, cause))?;
        let key = host_key(key)?;
        py.detach(move || {
            self.with_store(|state| state.clear(generation, key).map_err(preference_error))
        })
        .map_err(|cause| python_error(py, cause))
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| {
            self.lock().map(|mut state| {
                state.take();
            })
        })
        .map_err(|cause| python_error(py, cause))
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.with_store(|_| Ok(()))
            .map_err(|cause| python_error(slf.py(), cause))?;
        Ok(slf)
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        _exception_type: &Bound<'_, PyAny>,
        _exception_value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_closed_bounded_and_errors_do_not_echo_values() {
        for invalid in [
            r#"{}"#,
            r#"{"rankingEnabled":true,"feedbackEnabled":true,"allowSensitiveFeedback":1}"#,
            r#"{"rankingEnabled":true,"feedbackEnabled":true,"allowSensitiveFeedback":false,"SECRET":true}"#,
        ] {
            let error = decode::<FormulaPreferencePolicy>(invalid).unwrap_err();
            assert_eq!(error.code, "INVALID_ARGUMENT");
            assert!(!error.message.contains("SECRET"));
        }
        assert!(decode::<FormulaPreferencePolicy>(&" ".repeat(4097)).is_err());
    }
    #[test]
    fn busy_and_closed_instances_never_wait_or_replay() {
        let scope = FormulaDefinitionScope {
            user_sha256: "a".repeat(64),
            project_sha256: None,
            document_sha256: None,
            generation: 0,
        };
        let instance = FormulaPreferences {
            state: Mutex::new(Some(FormulaPreferenceStore::new(scope, [42; 32]).unwrap())),
        };
        let mut lock = instance.lock().unwrap();
        assert_eq!(
            instance.with_store(|_| Ok(())).unwrap_err().code,
            "PREFERENCE_BUSY"
        );
        lock.take();
        drop(lock);
        assert_eq!(
            instance.with_store(|_| Ok(())).unwrap_err().code,
            "PREFERENCE_CLOSED"
        );
    }
}

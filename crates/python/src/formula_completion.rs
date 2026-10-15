//! Model-free Python editing functions backed by the shared Core request types.
use latexsnipper_conversion::formula_completion::{
    scoped::{
        FormulaDefinitionCatalog, ScopedFormulaCompletionApplyRequest,
        ScopedFormulaCompletionError, ScopedFormulaCompletionRequest, ScopedFormulaSnapshot,
        FORMULA_COMPLETION_MAX_DEFINITIONS, FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
        SCOPED_FORMULA_COMPLETION_VERSION,
    },
    FormulaCompletionApplyRequest, FormulaCompletionError, FormulaCompletionInputFormat,
    FormulaCompletionRequest, FormulaCompletionSnapshot, FORMULA_COMPLETION_MAX_BYTES,
    FORMULA_COMPLETION_MAX_CANDIDATES, FORMULA_COMPLETION_MAX_DEPTH, FORMULA_COMPLETION_MAX_NODES,
    FORMULA_COMPLETION_VERSION,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyInt, PyModule};
use serde::Serialize;
use serde_json::json;

use crate::{json_to_python, python_error, serialize_json, BindingError};

const MAX_COMPLETION_DATA_BYTES: usize = 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 4096;
const MAX_CATALOG_BYTES: usize = 64 * 1024;

fn scoped_completion_error(error: ScopedFormulaCompletionError) -> BindingError {
    BindingError {
        code: match error {
            ScopedFormulaCompletionError::InvalidCatalog => "INVALID_CATALOG",
            ScopedFormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
            ScopedFormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
            ScopedFormulaCompletionError::StaleContext => "STALE_CONTEXT",
            ScopedFormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
        }
        .into(),
        message: error.to_string(),
        detail: None,
        retryable: false,
    }
}

fn completion_error(error: FormulaCompletionError) -> BindingError {
    BindingError {
        code: match error {
            FormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
            FormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
            FormulaCompletionError::StaleContext => "STALE_CONTEXT",
            FormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
        }
        .into(),
        message: error.to_string(),
        detail: None,
        retryable: false,
    }
}

fn encode_result(data: &impl Serialize) -> Result<String, BindingError> {
    let encoded = serialize_json(data)?;
    if encoded.len() > MAX_COMPLETION_DATA_BYTES {
        return Err(BindingError {
            code: "OUTPUT_TOO_LARGE".into(),
            message: "Serialized completion data exceeds the 1 MiB limit.".into(),
            detail: None,
            retryable: false,
        });
    }
    Ok(encoded)
}

fn input_format(value: &str) -> Result<FormulaCompletionInputFormat, BindingError> {
    match value {
        "latex" => Ok(FormulaCompletionInputFormat::Latex),
        _ => Err(BindingError::invalid(
            "Completion input_format must be exactly latex.",
        )),
    }
}

fn revision(value: &Bound<'_, PyAny>) -> Result<u64, BindingError> {
    if value.is_instance_of::<PyBool>() || !value.is_instance_of::<PyInt>() {
        return Err(BindingError::invalid(
            "revision must be an unsigned 64-bit integer, not a bool or string.",
        ));
    }
    value
        .extract()
        .map_err(|_| BindingError::invalid("revision must be between 0 and 18446744073709551615."))
}

fn snapshot(source: &str) -> Result<FormulaCompletionSnapshot, BindingError> {
    if source.len() > MAX_SNAPSHOT_BYTES {
        return Err(BindingError::invalid("Completion snapshot exceeds 4 KiB."));
    }
    serde_json::from_str(source).map_err(|_| {
        BindingError::invalid("Invalid completion snapshot schema or revision encoding.")
    })
}

fn complete_content(request: &FormulaCompletionRequest) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(completion_error)?)
}

fn apply_content(request: &FormulaCompletionApplyRequest) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(completion_error)?)
}

fn scoped_snapshot(source: &str) -> Result<ScopedFormulaSnapshot, BindingError> {
    if source.len() > MAX_SNAPSHOT_BYTES {
        return Err(BindingError::invalid(
            "Scoped completion snapshot exceeds 4 KiB.",
        ));
    }
    serde_json::from_str(source).map_err(|_| {
        BindingError::invalid("Invalid scoped completion snapshot schema or revision encoding.")
    })
}

fn catalog(source: &str) -> Result<FormulaDefinitionCatalog, BindingError> {
    if source.len() > MAX_CATALOG_BYTES {
        return Err(BindingError::invalid(
            "Completion catalog JSON exceeds 64 KiB.",
        ));
    }
    serde_json::from_str(source).map_err(|_| {
        BindingError::invalid("Invalid completion catalog schema or version encoding.")
    })
}

fn complete_scoped_content(
    request: &ScopedFormulaCompletionRequest,
) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(scoped_completion_error)?)
}

fn apply_scoped_content(
    request: &ScopedFormulaCompletionApplyRequest,
) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(scoped_completion_error)?)
}

fn dict_json(value: &Bound<'_, PyDict>) -> PyResult<String> {
    PyModule::import(value.py(), "json")?
        .call_method1("dumps", (value,))?
        .extract()
}

/// Query schema-v2 edits using explicit host-authorized scoped signatures.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, catalog, input_format="latex"))]
pub(super) fn complete_formula_scoped(
    py: Python<'_>,
    source: String,
    cursor: usize,
    revision: &Bound<'_, PyAny>,
    catalog: &Bound<'_, PyDict>,
    input_format: &str,
) -> PyResult<Py<PyAny>> {
    let revision = self::revision(revision).map_err(|error| python_error(py, error))?;
    let input_format = self::input_format(input_format).map_err(|error| python_error(py, error))?;
    let encoded = dict_json(catalog)?;
    let catalog = self::catalog(&encoded).map_err(|error| python_error(py, error))?;
    let request = ScopedFormulaCompletionRequest {
        source,
        cursor,
        revision,
        input_format,
        catalog,
    };
    let json = py
        .detach(move || complete_scoped_content(&request))
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Recheck current editor, catalog and scope; the host owns writes and reset epochs.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, catalog, snapshot, candidate_id, input_format="latex"))]
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_formula_completion_scoped(
    py: Python<'_>,
    source: String,
    cursor: usize,
    revision: &Bound<'_, PyAny>,
    catalog: &Bound<'_, PyDict>,
    snapshot: &Bound<'_, PyDict>,
    candidate_id: String,
    input_format: &str,
) -> PyResult<Py<PyAny>> {
    let revision = self::revision(revision).map_err(|error| python_error(py, error))?;
    let input_format = self::input_format(input_format).map_err(|error| python_error(py, error))?;
    let catalog = self::catalog(&dict_json(catalog)?).map_err(|error| python_error(py, error))?;
    let snapshot =
        scoped_snapshot(&dict_json(snapshot)?).map_err(|error| python_error(py, error))?;
    let request = ScopedFormulaCompletionApplyRequest {
        source,
        cursor,
        revision,
        input_format,
        catalog,
        snapshot,
        candidate_id,
    };
    let json = py
        .detach(move || apply_scoped_content(&request))
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Feature discovery is separate from the existing v1 capability shape.
#[pyfunction]
pub(super) fn formula_scoped_completion_capabilities(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let json = encode_result(&json!({
        "schemaVersion": SCOPED_FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"], "offsetEncoding": "utf-8-bytes",
        "revisionArgumentEncoding": "python-int-u64",
        "revisionEncoding": "canonical-decimal-u64-string",
        "scopeIdentityEncoding": "opaque-lowercase-sha256",
        "definitionRevisionEncoding": "canonical-decimal-u64-string",
        "generationEncoding": "canonical-decimal-u64-string",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES, "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH, "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxResultDataBytes": MAX_COMPLETION_DATA_BYTES, "maxSnapshotBytes": MAX_SNAPSHOT_BYTES,
        "maxCatalogBytes": MAX_CATALOG_BYTES, "maxDefinitions": FORMULA_COMPLETION_MAX_DEFINITIONS,
        "maxDefinitionArguments": FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
        "macroExpansion": false, "persistence": false
    })).map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Query finite Core LaTeX editing candidates; cursor uses UTF-8 bytes.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, input_format="latex"))]
pub(super) fn complete_formula(
    py: Python<'_>,
    source: String,
    cursor: usize,
    revision: &Bound<'_, PyAny>,
    input_format: &str,
) -> PyResult<Py<PyAny>> {
    let request = FormulaCompletionRequest {
        source,
        cursor,
        revision: self::revision(revision).map_err(|error| python_error(py, error))?,
        input_format: self::input_format(input_format).map_err(|error| python_error(py, error))?,
    };
    let json = py
        .detach(move || complete_content(&request))
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Return an explicitly selected canonical edit; the host owns writes and undo.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, snapshot, candidate_id, input_format="latex"))]
pub(super) fn apply_formula_completion(
    py: Python<'_>,
    source: String,
    cursor: usize,
    revision: &Bound<'_, PyAny>,
    snapshot: &Bound<'_, PyDict>,
    candidate_id: String,
    input_format: &str,
) -> PyResult<Py<PyAny>> {
    let revision = self::revision(revision).map_err(|error| python_error(py, error))?;
    let input_format = self::input_format(input_format).map_err(|error| python_error(py, error))?;
    let encoded: String = PyModule::import(py, "json")?
        .call_method1("dumps", (snapshot,))?
        .extract()?;
    let request = FormulaCompletionApplyRequest {
        source,
        cursor,
        revision,
        input_format,
        snapshot: self::snapshot(&encoded).map_err(|error| python_error(py, error))?,
        candidate_id,
    };
    let json = py
        .detach(move || apply_content(&request))
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Independent from the formula conversion registry and recognition sessions.
#[pyfunction]
pub(super) fn formula_completion_capabilities(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let json = encode_result(&json!({
        "schemaVersion": FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"],
        "offsetEncoding": "utf-8-bytes",
        "revisionEncoding": "canonical-decimal-u64-string",
        "revisionArgumentEncoding": "python-int-u64",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES,
        "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH,
        "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxResultDataBytes": MAX_COMPLETION_DATA_BYTES,
        "maxSnapshotBytes": MAX_SNAPSHOT_BYTES
    }))
    .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn scoped_catalog() -> Value {
        json!({"scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
            "documentSha256": "c".repeat(64), "generation": "18446744073709551615"},
            "definitions": [{"id": "pair", "revision": "18446744073709551615", "userSha256": "a".repeat(64),
                "projectSha256": "b".repeat(64), "command": "custompair", "arguments": ["math", "text"]}]})
    }

    #[test]
    fn python_scoped_completion_preserves_definitions_and_stale_scope_guards() {
        let request = ScopedFormulaCompletionRequest {
            source: "中\\cust".into(),
            cursor: 8,
            revision: u64::MAX,
            input_format: FormulaCompletionInputFormat::Latex,
            catalog: catalog(&scoped_catalog().to_string()).unwrap(),
        };
        let reply: Value =
            serde_json::from_str(&complete_scoped_content(&request).unwrap()).unwrap();
        assert_eq!(
            reply["candidates"][0]["definition"]["revision"],
            "18446744073709551615"
        );
        let mut apply = ScopedFormulaCompletionApplyRequest {
            source: request.source,
            cursor: request.cursor,
            revision: request.revision,
            input_format: request.input_format,
            catalog: request.catalog,
            snapshot: scoped_snapshot(&reply["snapshot"].to_string()).unwrap(),
            candidate_id: "definition:pair".into(),
        };
        let applied: Value = serde_json::from_str(&apply_scoped_content(&apply).unwrap()).unwrap();
        assert_eq!(applied["source"], "中\\custompair{}{}");
        assert_eq!(applied["tabStops"].as_array().unwrap().len(), 2);
        apply.catalog.scope.generation = 0;
        assert_eq!(
            apply_scoped_content(&apply).unwrap_err().code,
            "STALE_CONTEXT"
        );
        apply.catalog.scope.generation = u64::MAX;
        apply.catalog.definitions[0].arguments.clear();
        assert_eq!(
            apply_scoped_content(&apply).unwrap_err().code,
            "STALE_CONTEXT"
        );
        apply.catalog.definitions[0].command = "mysymbol2".into();
        assert_eq!(
            apply_scoped_content(&apply).unwrap_err().code,
            "INVALID_CATALOG"
        );
    }

    #[test]
    fn python_scoped_catalog_and_snapshot_are_closed_bounded_and_lossless() {
        let valid = scoped_catalog();
        for path in ["generation", "revision", "body", "scope", "catalog"] {
            let mut invalid = valid.clone();
            match path {
                "generation" => invalid["scope"]["generation"] = json!(0),
                "revision" => invalid["definitions"][0]["revision"] = json!("01"),
                "body" => invalid["definitions"][0]["body"] = json!(r"\input{private}"),
                "scope" => invalid["scope"]["extra"] = json!(true),
                _ => invalid["extra"] = json!(true),
            }
            assert_eq!(
                catalog(&invalid.to_string()).unwrap_err().code,
                "INVALID_ARGUMENT"
            );
        }
        assert!(catalog(&" ".repeat(MAX_CATALOG_BYTES + 1)).is_err());
        let valid = json!({"schemaVersion": 2, "sourceSha256": "a".repeat(64),
            "revision": "18446744073709551615", "cursor": 0, "definitionsSha256": "b".repeat(64)});
        assert_eq!(
            scoped_snapshot(&valid.to_string()).unwrap().revision,
            u64::MAX
        );
        let mut invalid = valid;
        invalid["insert"] = json!("untrusted");
        assert!(scoped_snapshot(&invalid.to_string()).is_err());
        assert!(scoped_snapshot(&" ".repeat(MAX_SNAPSHOT_BYTES + 1)).is_err());
    }

    #[test]
    fn python_completion_reuses_core_snapshot_and_edit_guards() {
        let request = FormulaCompletionRequest {
            source: "中\\fr".into(),
            cursor: 6,
            revision: u64::MAX,
            input_format: FormulaCompletionInputFormat::Latex,
        };
        let reply: Value = serde_json::from_str(&complete_content(&request).unwrap()).unwrap();
        assert_eq!(reply["snapshot"]["revision"], "18446744073709551615");
        let mut apply = FormulaCompletionApplyRequest {
            source: request.source,
            cursor: request.cursor,
            revision: request.revision,
            input_format: request.input_format,
            snapshot: snapshot(&reply["snapshot"].to_string()).unwrap(),
            candidate_id: "builtin:frac".into(),
        };
        let applied: Value = serde_json::from_str(&apply_content(&apply).unwrap()).unwrap();
        assert_eq!(applied["source"], "中\\frac{}{}");
        assert_eq!(applied["cursor"], 9);
        apply.revision = 0;
        assert_eq!(apply_content(&apply).unwrap_err().code, "STALE_CONTEXT");
        apply.revision = u64::MAX;
        apply.candidate_id = "arbitrary".into();
        assert_eq!(apply_content(&apply).unwrap_err().code, "UNKNOWN_CANDIDATE");
    }

    #[test]
    fn python_completion_snapshot_and_result_bounds_are_closed_and_serialized() {
        let valid =
            json!({"schemaVersion":1,"sourceSha256":"a".repeat(64),"cursor":0,"revision":"0"});
        for value in [json!(0), json!("01"), json!("18446744073709551616")] {
            let mut changed = valid.clone();
            changed["revision"] = value;
            assert_eq!(
                snapshot(&changed.to_string()).unwrap_err().code,
                "INVALID_ARGUMENT"
            );
        }
        let mut extra = valid.clone();
        extra["insert"] = json!("untrusted");
        assert!(snapshot(&extra.to_string()).is_err());
        assert!(snapshot(&" ".repeat(MAX_SNAPSHOT_BYTES + 1)).is_err());
        assert!(input_format("latex").is_ok());
        assert!(input_format("typst").is_err());
        let overhead = serialize_json(&json!({"source":""})).unwrap().len();
        assert!(
            encode_result(&json!({"source":"x".repeat(MAX_COMPLETION_DATA_BYTES - overhead)}))
                .is_ok()
        );
        assert_eq!(
            encode_result(&json!({"source":"\"".repeat(MAX_COMPLETION_DATA_BYTES / 2)}))
                .unwrap_err()
                .code,
            "OUTPUT_TOO_LARGE"
        );
    }
}

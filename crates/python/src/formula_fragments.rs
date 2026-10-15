//! Explicit private-template editing, separate from v1/v2 and recognition state.
use crate::formula_completion::{
    encode_result, input_format, revision, scoped_completion_error, MAX_COMPLETION_DATA_BYTES,
    MAX_SNAPSHOT_BYTES,
};
use crate::{json_to_python, python_error, BindingError};
use latexsnipper_conversion::formula_completion::{
    fragments::{
        FormulaFragmentCatalog, FormulaFragmentCompletionApplyRequest,
        FormulaFragmentCompletionRequest, FormulaFragmentSnapshot,
        FORMULA_COMPLETION_MAX_FRAGMENTS, FORMULA_COMPLETION_MAX_FRAGMENT_BYTES,
        FRAGMENT_FORMULA_COMPLETION_VERSION,
    },
    scoped::{
        FORMULA_COMPLETION_ARGUMENT_KINDS, FORMULA_COMPLETION_MAX_DEFINITIONS,
        FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
    },
    FORMULA_COMPLETION_MAX_BYTES, FORMULA_COMPLETION_MAX_CANDIDATES, FORMULA_COMPLETION_MAX_DEPTH,
    FORMULA_COMPLETION_MAX_NODES,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule};
use serde_json::json;

// Keep the existing v2 64-KiB catalog budget unchanged. Fragments have their own
// larger transport budget, still bounded independently of Core source limits.
const MAX_FRAGMENT_CATALOG_BYTES: usize = 1024 * 1024;

pub(super) fn dict_json_utf8(value: &Bound<'_, PyDict>) -> PyResult<String> {
    let options = PyDict::new(value.py());
    // Apply the advertised UTF-8 budget, not an inflated ASCII-escaped size.
    options.set_item("ensure_ascii", false)?;
    PyModule::import(value.py(), "json")?
        .call_method("dumps", (value,), Some(&options))?
        .extract()
        .map_err(|_| {
            python_error(
                value.py(),
                BindingError::invalid("Fragment JSON must contain valid Unicode scalars."),
            )
        })
}

pub(super) fn catalog(encoded: &str) -> Result<FormulaFragmentCatalog, BindingError> {
    if encoded.len() > MAX_FRAGMENT_CATALOG_BYTES {
        return Err(BindingError::invalid(
            "Fragment catalog JSON exceeds 1 MiB.",
        ));
    }
    serde_json::from_str(encoded)
        .map_err(|_| BindingError::invalid("Invalid fragment catalog schema or version encoding."))
}

pub(super) fn snapshot(encoded: &str) -> Result<FormulaFragmentSnapshot, BindingError> {
    if encoded.len() > MAX_SNAPSHOT_BYTES {
        return Err(BindingError::invalid(
            "Fragment completion snapshot exceeds 4 KiB.",
        ));
    }
    serde_json::from_str(encoded)
        .map_err(|_| BindingError::invalid("Invalid fragment snapshot schema or version encoding."))
}

fn complete_content(request: &FormulaFragmentCompletionRequest) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(scoped_completion_error)?)
}

fn apply_content(request: &FormulaFragmentCompletionApplyRequest) -> Result<String, BindingError> {
    encode_result(&request.execute().map_err(scoped_completion_error)?)
}

/// The host authorizes retrieval before supplying this current local catalog.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, catalog, input_format="latex"))]
pub(super) fn complete_formula_fragments(
    py: Python<'_>,
    source: String,
    cursor: usize,
    revision: &Bound<'_, PyAny>,
    catalog: &Bound<'_, PyDict>,
    input_format: &str,
) -> PyResult<Py<PyAny>> {
    let revision = self::revision(revision).map_err(|error| python_error(py, error))?;
    let input_format = self::input_format(input_format).map_err(|error| python_error(py, error))?;
    let encoded = dict_json_utf8(catalog)?;
    let json = py
        .detach(move || {
            let request = FormulaFragmentCompletionRequest {
                source,
                cursor,
                revision,
                input_format,
                catalog: self::catalog(&encoded)?,
            };
            complete_content(&request)
        })
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Recheck actual current policy, scope and editor state; never write documents.
#[pyfunction]
#[pyo3(signature = (source, *, cursor, revision, catalog, snapshot, candidate_id, input_format="latex"))]
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_formula_completion_fragments(
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
    let encoded_catalog = dict_json_utf8(catalog)?;
    let encoded_snapshot = dict_json_utf8(snapshot)?;
    let json = py
        .detach(move || {
            let request = FormulaFragmentCompletionApplyRequest {
                source,
                cursor,
                revision,
                input_format,
                catalog: self::catalog(&encoded_catalog)?,
                snapshot: self::snapshot(&encoded_snapshot)?,
                candidate_id,
            };
            apply_content(&request)
        })
        .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

/// Discover additive schema-v3 templates without changing old capabilities.
#[pyfunction]
pub(super) fn formula_fragment_completion_capabilities(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let json = encode_result(&json!({
        "schemaVersion": FRAGMENT_FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"], "offsetEncoding": "utf-8-bytes",
        "revisionArgumentEncoding": "python-int-u64", "revisionEncoding": "canonical-decimal-u64-string",
        "scopeIdentityEncoding": "opaque-lowercase-sha256", "definitionRevisionEncoding": "canonical-decimal-u64-string",
        "fragmentRevisionEncoding": "canonical-decimal-u64-string", "generationEncoding": "canonical-decimal-u64-string",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES, "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH, "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxResultDataBytes": MAX_COMPLETION_DATA_BYTES, "maxSnapshotBytes": MAX_SNAPSHOT_BYTES,
        "maxCatalogBytes": MAX_FRAGMENT_CATALOG_BYTES, "maxFragments": FORMULA_COMPLETION_MAX_FRAGMENTS,
        "maxFragmentBytes": FORMULA_COMPLETION_MAX_FRAGMENT_BYTES, "maxDefinitions": FORMULA_COMPLETION_MAX_DEFINITIONS,
        "maxDefinitionArguments": FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS, "argumentKinds": FORMULA_COMPLETION_ARGUMENT_KINDS,
        "maxLeadingOptionalArguments": 1, "retrievalSource": "explicit-host-catalog", "sensitiveDocumentRequiresHostAllow": true,
        "macroExpansion": false, "persistence": false, "learning": false
    })).map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn catalog_json() -> Value {
        json!({"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":null,
            "documentSha256":"c".repeat(64),"generation":"18446744073709551615"},"definitions":[]},
            "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
            "fragments":[{"id":"saved","revision":"18446744073709551615","userSha256":"a".repeat(64),
                "projectSha256":null,"documentSha256":"c".repeat(64),"trigger":"bayes","label":"Saved formula","source":r"\frac{α}{}"}]})
    }

    #[test]
    fn fragments_keep_exact_versions_owned_data_and_current_policy_guards() {
        let request = FormulaFragmentCompletionRequest {
            source: "中😀\\bay".into(),
            cursor: 11,
            revision: u64::MAX,
            input_format:
                latexsnipper_conversion::formula_completion::FormulaCompletionInputFormat::Latex,
            catalog: catalog(&catalog_json().to_string()).unwrap(),
        };
        let reply: Value = serde_json::from_str(&complete_content(&request).unwrap()).unwrap();
        assert_eq!(reply["context"]["schemaVersion"], 3);
        assert_eq!(
            reply["candidates"][0]["fragment"]["revision"],
            "18446744073709551615"
        );
        let mut apply = FormulaFragmentCompletionApplyRequest {
            source: request.source,
            cursor: request.cursor,
            revision: request.revision,
            input_format: request.input_format,
            catalog: request.catalog,
            snapshot: snapshot(&reply["snapshot"].to_string()).unwrap(),
            candidate_id: "fragment:saved".into(),
        };
        let accepted: Value = serde_json::from_str(&apply_content(&apply).unwrap()).unwrap();
        assert_eq!(accepted["source"], "中😀\\frac{α}{}");
        assert_eq!(accepted["cursor"], 17);
        apply.catalog.policy.retrieval_enabled = false;
        assert_eq!(apply_content(&apply).unwrap_err().code, "STALE_CONTEXT");
        apply.catalog.policy.retrieval_enabled = true;
        apply.catalog.commands.scope.generation = 0;
        assert_eq!(apply_content(&apply).unwrap_err().code, "STALE_CONTEXT");
        apply.catalog.commands.scope.generation = u64::MAX;
        apply.catalog.fragments[0].source = r"\frac{".into();
        assert_eq!(apply_content(&apply).unwrap_err().code, "INVALID_CATALOG");
    }

    #[test]
    fn fragment_transport_is_closed_bounded_and_rejects_lossy_revisions() {
        for field in ["revision", "generation", "policy", "field", "catalog"] {
            let mut invalid = catalog_json();
            match field {
                "revision" => invalid["fragments"][0]["revision"] = json!(42),
                "generation" => invalid["commands"]["scope"]["generation"] = json!("01"),
                "policy" => invalid["policy"]["retrievalEnabled"] = json!("true"),
                "field" => invalid["fragments"][0]["filePath"] = json!("not-opened"),
                _ => invalid["unknown"] = json!(true),
            }
            assert_eq!(
                catalog(&invalid.to_string()).unwrap_err().code,
                "INVALID_ARGUMENT"
            );
        }
        assert!(catalog(&" ".repeat(MAX_FRAGMENT_CATALOG_BYTES + 1)).is_err());
        let valid = json!({"schemaVersion":3,"sourceSha256":"a".repeat(64),"catalogSha256":"b".repeat(64),
            "revision":"18446744073709551615","cursor":0});
        assert_eq!(snapshot(&valid.to_string()).unwrap().revision, u64::MAX);
        let mut invalid = valid.clone();
        invalid["insert"] = json!("untrusted");
        assert_eq!(
            snapshot(&invalid.to_string()).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        invalid = valid;
        invalid["revision"] = json!(0);
        assert!(snapshot(&invalid.to_string()).is_err());
        assert!(snapshot(&" ".repeat(MAX_SNAPSHOT_BYTES + 1)).is_err());
    }

    #[test]
    fn fragment_output_budget_and_next_request_remain_independent() {
        let mut data = catalog_json();
        let mut entry = data["fragments"][0].clone();
        entry["source"] = json!(format!("x %{}\n", "\"".repeat(8188)));
        data["fragments"] = json!((0..60)
            .map(|index| {
                let mut entry = entry.clone();
                entry["id"] = json!(format!("entry-{index}"));
                entry
            })
            .collect::<Vec<_>>());
        let source = format!("{}\\bay", "x".repeat(3900));
        let mut request = FormulaFragmentCompletionRequest {
            cursor: source.len(),
            source,
            revision: 7,
            input_format:
                latexsnipper_conversion::formula_completion::FormulaCompletionInputFormat::Latex,
            catalog: catalog(&data.to_string()).unwrap(),
        };
        assert_eq!(
            complete_content(&request).unwrap_err().code,
            "OUTPUT_TOO_LARGE"
        );
        request.catalog = catalog(&catalog_json().to_string()).unwrap();
        assert!(complete_content(&request).is_ok());
    }
}

#[path = "support/formula_profile_store.rs"]
mod fixture;
use latexsnipper_conversion::{conversion_registry::*, FormulaConversionMode};
use latexsnipper_plugin_wasi::WasiFormulaProfile;

fn request(exporter: bool, source: &str) -> RegisteredConversionRequest {
    RegisteredConversionRequest {
        schema_version: 1,
        input: if exporter { "latex" } else { "fixture:power" }.into(),
        output: if exporter { "fixture:power" } else { "omml" }.into(),
        mode: FormulaConversionMode::BestEffort,
        backend: Some("fixture:profile".into()),
        content: source.into(),
        context_sha256: "a".repeat(64),
    }
}

#[test]
fn profiles_execute_actual_import_export_and_bind_validator_identity() {
    let (_temporary, store) = fixture::store();
    let definition = fixture::profile(false);
    let profile = WasiFormulaProfile::from_json(&serde_json::to_vec(&definition).unwrap()).unwrap();
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    let mut prepared = profile
        .prepare_with_cancellation(&store, cancellation.clone())
        .unwrap();
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    prepared
        .registry
        .set_backend_enabled(&prepared.backend_id, true)
        .unwrap();
    let result = prepared.registry.convert(&request(false, "x:2")).unwrap();
    assert!(result.candidate.content.contains("m:sSup"));
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2+y"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::InputRejected
    );
    let mut changed = definition.clone();
    changed["format"]["validation"]["pattern"] = serde_json::json!("[a-z]:[0-8]");
    let mut other = WasiFormulaProfile::from_json(&serde_json::to_vec(&changed).unwrap())
        .unwrap()
        .prepare(&store)
        .unwrap();
    other
        .registry
        .set_backend_enabled(&other.backend_id, true)
        .unwrap();
    let altered = other.registry.convert(&request(false, "x:2")).unwrap();
    assert_eq!(result.candidate.content, altered.candidate.content);
    assert_ne!(result.configuration_sha256, altered.configuration_sha256);
    assert_ne!(result.route_sha256, altered.route_sha256);
    cancellation.cancel();
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendFailed
    );
    let exporter =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(true)).unwrap())
            .unwrap();
    let mut exported = exporter.prepare(&store).unwrap();
    exported
        .registry
        .set_backend_enabled(&exported.backend_id, true)
        .unwrap();
    assert_eq!(
        exported
            .registry
            .convert(&request(true, "$$y^4$$"))
            .unwrap()
            .candidate
            .content,
        "y:4"
    );
    store.set_enabled("fixture.component", false).unwrap();
    assert_eq!(
        exported
            .registry
            .convert(&request(true, "x^2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendFailed
    );
    assert!(exporter.prepare(&store).is_err());
    assert!(!store.get("fixture.component").unwrap().unwrap().enabled);
}

#[test]
fn malformed_profile_does_not_create_a_store_or_mutate_caller_registry() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent-store");
    let store = latexsnipper_plugin::RemotePluginStore::new(root.clone());
    let mut profile =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(false)).unwrap())
            .unwrap();
    profile.backend_id = "core:builtin".into();
    assert!(profile.prepare(&store).is_err());
    assert!(!root.exists());
    let profile =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(false)).unwrap())
            .unwrap();
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    cancellation.cancel();
    assert!(profile
        .prepare_with_cancellation(&store, cancellation)
        .is_err());
    assert!(!root.exists());
}

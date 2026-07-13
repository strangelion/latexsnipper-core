use latexsnipper_runtime::{MemoryModelResolver, ModelId, ModelResolver};

#[test]
fn memory_resolver_normalizes_keys_and_tracks_contents() {
    let resolver = MemoryModelResolver::new();
    assert!(resolver.is_empty());

    resolver.store("\\formula-rec\\trocr\\config.json", b"{}".to_vec());
    resolver.store("formula-rec/trocr/model.onnx", vec![1, 2, 3]);

    assert!(resolver.has("/formula-rec/trocr/config.json"));
    assert_eq!(resolver.len(), 2);
    assert_eq!(
        resolver.get("formula-rec/trocr/model.onnx"),
        Some(vec![1, 2, 3])
    );

    assert!(resolver.remove("/formula-rec/trocr/config.json"));
    assert!(!resolver.remove("formula-rec/trocr/missing.bin"));
    resolver.clear();
    assert!(resolver.is_empty());
}

#[test]
fn memory_resolver_resolves_named_artifacts() {
    let resolver = MemoryModelResolver::new();
    let id = ModelId::new("formula-rec", "trocr");
    resolver.store(
        "formula-rec/trocr/config.json",
        br#"{"kind":"test"}"#.to_vec(),
    );
    resolver.store("formula-rec/trocr/model.onnx", vec![7, 8, 9]);
    resolver.store("text-rec/other/model.onnx", vec![10]);

    let text = resolver.read_text_artifact(&id, "config.json").unwrap();
    assert_eq!(text, r#"{"kind":"test"}"#);

    let handle = resolver.resolve_artifact(&id, "model.onnx").unwrap();
    assert_eq!(handle.id(), "formula-rec/trocr/model.onnx");
    assert_eq!(handle.model_bytes(), Some(&[7, 8, 9][..]));

    let mut artifacts = resolver.list_artifacts(&id);
    artifacts.sort();
    assert_eq!(artifacts, vec!["config.json", "model.onnx"]);
}

#[test]
fn memory_resolver_rejects_missing_and_non_utf8_artifacts() {
    let resolver = MemoryModelResolver::new();
    let id = ModelId::new("formula-rec", "trocr");
    resolver.store("formula-rec/trocr/tokenizer.json", vec![0xff, 0xfe]);

    let missing = resolver.resolve_artifact(&id, "missing.onnx").unwrap_err();
    assert!(missing.to_string().contains("Artifact not found"));

    let invalid = resolver
        .read_text_artifact(&id, "tokenizer.json")
        .unwrap_err();
    assert!(invalid.to_string().contains("not valid UTF-8"));
}

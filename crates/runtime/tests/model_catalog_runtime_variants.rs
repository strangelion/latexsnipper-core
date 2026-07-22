//! Tests that every official model in the manifest template declares explicit
//! `runtimeVariants` with valid artifacts, selectable statuses, and consistent
//! fallback references.

use std::collections::HashSet;

#[test]
fn every_official_model_declares_runtime_variants() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts")
        .join("model-manifest.template.json");

    let json = std::fs::read_to_string(&path).expect("read official model manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&json).expect("parse official model manifest");

    let categories = manifest["categories"]
        .as_object()
        .expect("categories must be an object");

    assert!(!categories.is_empty(), "manifest must declare at least one category");

    for (category_name, category) in categories {
        let variants = category["variants"]
            .as_array()
            .unwrap_or_else(|| panic!("{category_name}: variants must be an array"));

        assert!(!variants.is_empty(), "{category_name}: must have at least one variant");

        for variant in variants {
            let variant_id = variant["id"]
                .as_str()
                .unwrap_or_else(|| panic!("{category_name}: variant missing id"));

            let key = format!("{category_name}/{variant_id}");

            // 1. Must have explicit runtimeVariants
            let runtime_variants = variant["runtimeVariants"].as_array().unwrap_or_else(|| {
                panic!("{key}: has no explicit runtimeVariants array")
            });
            assert!(
                !runtime_variants.is_empty(),
                "{key}: runtimeVariants must not be empty"
            );

            // 2. No duplicate runtime variant ids
            let mut runtime_ids = HashSet::new();
            for rv in runtime_variants {
                let rv_id = rv["id"].as_str().unwrap_or_else(|| {
                    panic!("{key}: runtime variant missing id field")
                });
                assert!(
                    runtime_ids.insert(rv_id.to_string()),
                    "{key}: duplicate runtime variant id '{rv_id}'"
                );
            }

            // 3. At least one variant must be selectable (stable or experimental)
            let has_selectable = runtime_variants.iter().any(|rv| {
                let status = rv["status"].as_str().unwrap_or("stable");
                matches!(status, "stable" | "experimental")
            });
            assert!(
                has_selectable,
                "{key}: no selectable runtime variant (must have at least one stable or experimental)"
            );

            // 4. Each variant must have artifacts
            for rv in runtime_variants {
                let rv_id = rv["id"].as_str().unwrap_or("?");
                let artifacts = rv["artifacts"].as_object().unwrap_or_else(|| {
                    panic!(
                        "{key}/{rv_id}: artifacts must be an object"
                    )
                });
                assert!(
                    !artifacts.is_empty(),
                    "{key}/{rv_id}: artifacts must not be empty"
                );
            }

            // 5. Fallbacks must reference existing runtime variant ids
            for rv in runtime_variants {
                let rv_id = rv["id"].as_str().unwrap_or("?");
                if let Some(fallbacks) = rv["fallbacks"].as_array() {
                    for fallback in fallbacks {
                        let fb = fallback.as_str().unwrap_or_else(|| {
                            panic!(
                                "{key}/{rv_id}: fallback entry is not a string"
                            )
                        });
                        assert!(
                            runtime_ids.contains(fb),
                            "{key}/{rv_id}: references missing fallback '{fb}'"
                        );
                    }
                }
            }

            // 6. Every artifact file must appear in the top-level "files" array
            let files: HashSet<&str> = variant["files"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|f| f.as_str()).collect())
                .unwrap_or_default();

            for rv in runtime_variants {
                let rv_id = rv["id"].as_str().unwrap_or("?");
                if let Some(artifacts) = rv["artifacts"].as_object() {
                    for (artifact_kind, artifact_path) in artifacts {
                        let path_str = artifact_path.as_str().unwrap_or_else(|| {
                            panic!(
                                "{key}/{rv_id}: artifact '{artifact_kind}' value is not a string"
                            )
                        });
                        assert!(
                            files.contains(path_str),
                            "{key}/{rv_id}: artifact '{artifact_kind}' file '{path_str}' \
                             is not declared in the top-level 'files' array"
                        );
                    }
                }
            }
        }
    }
}

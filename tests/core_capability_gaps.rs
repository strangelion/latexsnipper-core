use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("failed to parse {}: {error}", path.display());
    })
}

#[test]
fn capability_gap_inventory_has_valid_statuses_and_live_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory = read_json(&root.join("quality/core-capability-gaps.v1.json"));
    assert_eq!(inventory["schemaVersion"], 1);

    let allowed: BTreeSet<&str> = inventory["statusVocabulary"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let capabilities = inventory["capabilities"].as_array().unwrap();
    assert!(!capabilities.is_empty());

    let mut ids = BTreeSet::new();
    for capability in capabilities {
        let id = capability["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate capability id: {id}");
        let status = capability["status"].as_str().unwrap();
        assert!(
            allowed.contains(status),
            "invalid status '{status}' for {id}"
        );
        assert!(
            !capability["nextMilestone"]
                .as_str()
                .unwrap()
                .trim()
                .is_empty(),
            "missing next milestone for {id}"
        );

        let evidence = capability["evidence"].as_array().unwrap();
        assert!(!evidence.is_empty(), "missing evidence for {id}");
        for relative in evidence {
            let relative = relative.as_str().unwrap();
            assert!(
                root.join(relative).is_file(),
                "evidence path for {id} does not exist: {relative}"
            );
        }
    }
}

#[test]
fn real_dataset_blockers_stay_synchronized_with_the_inventory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory = read_json(&root.join("quality/core-capability-gaps.v1.json"));
    let real_data = read_json(&root.join("quality/real-dataset-status.json"));
    let capabilities = inventory["capabilities"].as_array().unwrap();

    let status = |id: &str| {
        capabilities
            .iter()
            .find(|capability| capability["id"] == id)
            .and_then(|capability| capability["status"].as_str())
            .unwrap()
    };

    assert_eq!(status("real-formula-quality"), real_data["status"]);
    assert_eq!(status("real-table-quality"), real_data["table"]["status"]);
}

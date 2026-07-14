#![no_main]

use latexsnipper_model::{ModelManifest, ModelManifestV3};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        if let Ok(manifest) = serde_json::from_str::<ModelManifestV3>(text) {
            let _ = manifest.validate_contract();
        }
        if let Ok(manifest) = ModelManifest::parse(text) {
            let _ = manifest.validate();
            let _ = ModelManifestV3::migrate_from_v2(manifest);
        }
    }
});

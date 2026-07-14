#![no_main]

use latexsnipper_plugin::{PluginManifest, PluginManifestV3};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(1 << 20)];
    if let Ok(manifest) = serde_json::from_slice::<PluginManifestV3>(data) {
        let _ = manifest.validate_contract();
    }
    if let Ok(manifest) = serde_json::from_slice::<PluginManifest>(data) {
        let _ = PluginManifestV3::migrate_from_v2(manifest);
    }
});

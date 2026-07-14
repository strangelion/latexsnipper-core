#![no_main]

use latexsnipper_plugin::PluginManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<PluginManifest>(&data[..data.len().min(1 << 20)]);
});

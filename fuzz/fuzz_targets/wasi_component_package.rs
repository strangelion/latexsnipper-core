#![no_main]

use latexsnipper_plugin_wasi::verify_component_artifact_bytes;
use libfuzzer_sys::fuzz_target;
use semver::Version;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let declared = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let manifest_len = declared.min(data.len() - 4).min(1 << 20);
    let manifest = &data[4..4 + manifest_len];
    let component = &data[4 + manifest_len..data.len().min(4 + manifest_len + (4 << 20))];
    let core = Version::new(3, 0, 0);
    let _ = verify_component_artifact_bytes(&core, manifest, component);
});

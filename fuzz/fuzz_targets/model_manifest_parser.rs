#![no_main]

use latexsnipper_model::ModelManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        if let Ok(manifest) = ModelManifest::parse(text) {
            let _ = manifest.validate();
        }
    }
});

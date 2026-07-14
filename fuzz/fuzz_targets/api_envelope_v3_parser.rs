#![no_main]

use latexsnipper_api_types::ApiEnvelopeV3;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(1 << 20)];
    if let Ok(envelope) = serde_json::from_slice::<ApiEnvelopeV3<serde_json::Value>>(data) {
        let _ = envelope.has_valid_contract();
    }
});

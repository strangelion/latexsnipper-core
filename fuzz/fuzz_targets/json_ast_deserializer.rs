#![no_main]

use latexsnipper_ast::Document;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Document>(&data[..data.len().min(1 << 20)]);
});

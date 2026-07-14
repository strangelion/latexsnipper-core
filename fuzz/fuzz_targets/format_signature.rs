#![no_main]

use latexsnipper_conversion::DocumentImporter;
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 1 << 20;

fuzz_target!(|data: &[u8]| {
    let _ = DocumentImporter::detect_format(&data[..data.len().min(MAX_INPUT)], None);
});

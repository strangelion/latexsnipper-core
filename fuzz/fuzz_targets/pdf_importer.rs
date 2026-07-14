#![no_main]

use latexsnipper_conversion::extract_pdf_text_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = extract_pdf_text_bytes(&data[..data.len().min(2 << 20)]);
});

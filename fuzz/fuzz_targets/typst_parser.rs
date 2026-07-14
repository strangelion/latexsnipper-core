#![no_main]

use latexsnipper_conversion::parse_typst_to_latex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        let _ = parse_typst_to_latex(text);
    }
});

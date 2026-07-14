#![no_main]

use latexsnipper_conversion::parse_svg;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        let _ = parse_svg(text);
    }
});

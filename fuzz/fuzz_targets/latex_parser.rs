#![no_main]

use latexsnipper_syntax::latex::LatexParser;
use latexsnipper_syntax::Parser as _;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        let _ = LatexParser.parse(text);
    }
});

#![no_main]

use latexsnipper_conversion::{
    parse_mathml_to_latex, parse_omml_to_latex, parse_word_table_ooxml,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(1 << 20)]) {
        let _ = parse_mathml_to_latex(text);
        let _ = parse_omml_to_latex(text);
        let _ = parse_word_table_ooxml(text);
    }
});

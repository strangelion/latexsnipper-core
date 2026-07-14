#![no_main]

use latexsnipper_ast::{ImportOptions, InputFormat};
use latexsnipper_conversion::DocumentImporter;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(1 << 20)];
    let options = ImportOptions {
        preserve_unknown_parts: true,
        max_input_size: 1 << 20,
        max_decompressed_size: 2 << 20,
        max_zip_entries: 64,
        max_xml_depth: 32,
        max_xml_elements: 10_000,
        ..ImportOptions::default()
    };
    let _ = DocumentImporter::from_bytes(data, Some(InputFormat::OfficeDocx), options);
});

use std::io::Write;

use flate2::{write::ZlibEncoder, Compression};
use latexsnipper_conversion::{
    formula_source_probe::{ProbeError, SourceFormat, MAX_SOURCE_BYTES},
    png_formula_source::{inspect_png_formula_sources as inspect, PngSourceError as Error, *},
};

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
    bytes.extend(kind);
    bytes.extend(data);
    bytes.extend(crc32fast::hash(&bytes[4..]).to_be_bytes());
    bytes
}

fn header() -> Vec<u8> {
    chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0])
}

fn png(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(header());
    for chunk in chunks {
        bytes.extend(chunk);
    }
    // One white grayscale pixel, with filter None.
    bytes.extend(chunk(b"IDAT", &zlib(&[0, 255])));
    bytes.extend(chunk(b"IEND", &[]));
    bytes
}

fn itxt(key: &str, source: &[u8], compressed: bool) -> Vec<u8> {
    let mut body = key.as_bytes().to_vec();
    body.extend([0, u8::from(compressed), 0]);
    body.extend(b"zh-Hans\0");
    body.extend("公式源".as_bytes());
    body.push(0);
    body.extend(if compressed {
        zlib(source)
    } else {
        source.to_vec()
    });
    chunk(b"iTXt", &body)
}

#[test]
fn reads_latin1_and_utf8_sources_with_chunk_provenance() {
    let text = chunk(b"tEXt", b"LaTeX\0  \\text{caf\xe9}+x^2\n");
    let utf8 = itxt("latex", "\\text{中文}+y".as_bytes(), false);
    let image = png(&[text.clone(), utf8]);
    let report = inspect(&image).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert_eq!(report.sources[0].source, "  \\text{café}+x^2\n");
    assert_eq!(report.sources[0].keyword, "LaTeX");
    assert_eq!(&image[report.sources[0].chunk_span.clone()], text);
    assert_eq!(report.sources[1].source, "\\text{中文}+y");
    assert_eq!(report.sources[1].language, "zh-Hans");
    assert_eq!(report.sources[1].translated_keyword, "公式源");
    assert_eq!(report.conflicting_formats, [SourceFormat::Latex]);
}

#[test]
fn supports_both_compressed_text_carriers_without_normalizing() {
    let mut body = b"latex\0\0".to_vec();
    body.extend(zlib(b"\\frac{1}{2}\n"));
    let image = png(&[
        chunk(b"zTXt", &body),
        itxt("latex", b"\\frac{1}{2}\n", true),
    ]);
    let report = inspect(&image).unwrap();
    assert!(report.sources.iter().all(|s| s.compressed));
    assert_eq!(report.sources[0].source, "\\frac{1}{2}\n");
    assert!(report.conflicting_formats.is_empty());
}

#[test]
fn keeps_xml_annotations_and_reports_cross_carrier_latex_conflicts() {
    let mathml = br#"<math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mi>x</mi><annotation encoding="application/x-tex">x&amp;y</annotation></semantics></math>"#;
    let omml = br#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#;
    let image = png(&[
        itxt("MathML", mathml, true),
        itxt("OMML", omml, false),
        chunk(b"tEXt", b"latex\0z"),
    ]);
    let report = inspect(&image).unwrap();
    assert_eq!(report.sources[0].source.as_bytes(), mathml);
    assert_eq!(report.sources[1].source.as_bytes(), omml);
    let annotation = &report.sources[0].latex_annotations[0];
    assert_eq!(annotation.source, "x&y");
    assert!(report.sources[0].source[annotation.xml_span.clone()].starts_with("<annotation"));
    assert_eq!(report.conflicting_formats, [SourceFormat::Latex]);
    assert_eq!(
        inspect(&png(&[itxt("omml", mathml, false)])).unwrap_err(),
        Error::InvalidFormula(ProbeError::FormatMismatch)
    );
}

#[test]
fn ignores_descriptions_xmp_and_unknown_fields_without_inflating() {
    let image = png(&[
        chunk(b"tEXt", b"Description\0\\frac12"),
        chunk(b"zTXt", b"Comment\0\0invalid-zlib"),
        itxt("XML:com.adobe.xmp", b"<latex>x</latex>", false),
    ]);
    let report = inspect(&image).unwrap();
    assert!(report.sources.is_empty());
    assert_eq!(report.ignored_text_chunks, 3);
    assert!(inspect(&png(&[])).unwrap().sources.is_empty());
}

#[test]
fn validates_crc_and_every_truncated_prefix_without_panicking() {
    assert_eq!(
        chunk(b"IEND", &[]),
        [0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]
    );
    let image = png(&[itxt("latex", b"x^2", true)]);
    for end in 0..image.len() {
        assert!(inspect(&image[..end]).is_err(), "prefix {end}");
    }
    let mut bad = image.clone();
    bad[41] ^= 1;
    assert_eq!(inspect(&bad).unwrap_err(), Error::InvalidChecksum);
    let mut trailer = image;
    trailer.push(0);
    assert_eq!(inspect(&trailer).unwrap_err(), Error::InvalidFraming);
}

#[test]
fn rejects_ambiguous_or_unsupported_critical_framing() {
    for extra in [header(), chunk(b"IEND", &[]), chunk(b"PLTE", &[0, 0, 0])] {
        assert_eq!(inspect(&png(&[extra])).unwrap_err(), Error::InvalidFraming);
    }
    assert_eq!(
        inspect(&png(&[chunk(b"ABCD", &[])])).unwrap_err(),
        Error::UnsupportedCriticalChunk
    );
    assert_eq!(
        inspect(&png(&[chunk(b"abca", &[])])).unwrap_err(),
        Error::InvalidFraming
    );
    let separated = png(&[
        chunk(b"IDAT", &zlib(&[0, 255])),
        chunk(b"tEXt", b"latex\0x"),
    ]);
    assert_eq!(inspect(&separated).unwrap_err(), Error::InvalidFraming);
    let mut missing_header = b"\x89PNG\r\n\x1a\n".to_vec();
    missing_header.extend(chunk(b"tEXt", b"latex\0x"));
    assert_eq!(inspect(&missing_header).unwrap_err(), Error::InvalidFraming);
}

#[test]
fn rejects_malformed_recognized_text_and_xml() {
    for body in [
        b"latex".as_slice(),
        b" latex\0x",
        b"la  tex\0x",
        b"latex\0x\0y",
        b"\0x",
    ] {
        assert_eq!(
            inspect(&png(&[chunk(b"tEXt", body)])).unwrap_err(),
            Error::InvalidText
        );
    }
    assert_eq!(
        inspect(&png(&[itxt("latex", &[0xff], false)])).unwrap_err(),
        Error::InvalidText
    );
    assert_eq!(
        inspect(&png(&[chunk(b"tEXt", b"latex\0")])).unwrap_err(),
        Error::InvalidFormula(ProbeError::EmptySource)
    );
    let dangerous = br#"<!DOCTYPE math [<!ENTITY x SYSTEM "file:///private">]><math xmlns="http://www.w3.org/1998/Math/MathML">&x;</math>"#;
    assert!(matches!(
        inspect(&png(&[itxt("mathml", dangerous, false)])).unwrap_err(),
        Error::InvalidFormula(_)
    ));
    for body in [
        b"latex\0\x02\0\0\0x".as_slice(),
        b"latex\0\x01\x01\0\0x",
        b"latex\0",
    ] {
        assert_eq!(
            inspect(&png(&[chunk(b"iTXt", body)])).unwrap_err(),
            Error::InvalidCompression
        );
    }
}

#[test]
fn requires_complete_single_zlib_stream_and_enforces_expansion_budget() {
    let good = zlib(b"x^2");
    let mut bad_checksum = good.clone();
    *bad_checksum.last_mut().unwrap() ^= 1;
    let mut trailing = good.clone();
    trailing.push(0);
    for stream in [
        Vec::new(),
        good[..good.len() - 1].to_vec(),
        bad_checksum,
        trailing,
    ] {
        let mut body = b"latex\0\0".to_vec();
        body.extend(stream);
        assert_eq!(
            inspect(&png(&[chunk(b"zTXt", &body)])).unwrap_err(),
            Error::InvalidCompression
        );
    }
    assert_eq!(
        inspect(&png(&[itxt(
            "latex",
            &vec![b'x'; MAX_SOURCE_BYTES + 1],
            true
        )]))
        .unwrap_err(),
        Error::MetadataLimit
    );
}

#[test]
fn enforces_input_chunk_field_and_aggregate_metadata_budgets() {
    assert_eq!(
        inspect(&vec![0; MAX_PNG_BYTES + 1]).unwrap_err(),
        Error::InputLimit
    );
    assert_eq!(
        inspect(&png(&vec![chunk(b"aaAa", &[]); MAX_CHUNKS])).unwrap_err(),
        Error::ChunkLimit
    );
    assert_eq!(
        inspect(&png(&vec![
            chunk(b"tEXt", b"Comment\0x");
            MAX_TEXT_CHUNKS + 1
        ]))
        .unwrap_err(),
        Error::MetadataLimit
    );
    assert_eq!(
        inspect(&png(&vec![
            chunk(b"tEXt", b"latex\0x");
            MAX_FORMULA_FIELDS + 1
        ]))
        .unwrap_err(),
        Error::MetadataLimit
    );
    let large = itxt("latex", &vec![b'x'; MAX_SOURCE_BYTES], true);
    assert_eq!(
        inspect(&png(&vec![large; 5])).unwrap_err(),
        Error::MetadataLimit
    );
    let mut large_unknown = b"Comment\0".to_vec();
    large_unknown.resize(MAX_METADATA_BYTES + 1, b'x');
    assert_eq!(
        inspect(&png(&[chunk(b"tEXt", &large_unknown)])).unwrap_err(),
        Error::MetadataLimit
    );
}

#[test]
fn finds_sources_after_idat_and_preserves_duplicate_fields() {
    let mut image = png(&[]);
    image.truncate(image.len() - 12);
    let source = chunk(b"tEXt", b"latex\0x");
    image.extend(&source);
    image.extend(&source);
    image.extend(chunk(b"IEND", &[]));
    let report = inspect(&image).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert!(report.conflicting_formats.is_empty());
}

#[cfg(feature = "native")]
#[test]
fn native_import_preserves_image_and_exposes_candidates_without_creating_formulas() {
    use base64::Engine;
    use latexsnipper_ast::{AssetStorage, ImportOptions, InputFormat};
    use latexsnipper_conversion::importer::DocumentImporter;
    use sha2::{Digest, Sha256};

    let bytes = png(&[chunk(b"tEXt", b"latex\0x^2"), itxt("latex", b"y^2", true)]);
    let document = DocumentImporter::from_bytes(
        &bytes,
        Some(InputFormat::ImagePng),
        ImportOptions::default(),
    )
    .unwrap();
    assert!(document.pages[0].blocks.is_empty());
    let asset = &document.assets[0];
    match &asset.storage {
        AssetStorage::InlineBase64 { data } => assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap(),
            bytes
        ),
        _ => panic!("original image must remain inline"),
    }
    let metadata = &asset.metadata["formula_source_candidates_v1"];
    assert_eq!(
        metadata["carrierSha256"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert_eq!(metadata["sourceKind"], "declared-metadata");
    assert_eq!(metadata["sources"][0]["source"], "x^2");
    assert_eq!(metadata["sources"][1]["source"], "y^2");
    assert_eq!(metadata["conflictingFormats"], serde_json::json!(["latex"]));
    assert!(document
        .diagnostics
        .iter()
        .any(|d| d.code == "W_PNG_FORMULA_SOURCE_CONFLICT"));

    let bad = png(&[chunk(b"tEXt", b"latex\0")]);
    let document = DocumentImporter::from_bytes(&bad, None, ImportOptions::default()).unwrap();
    assert!(document.assets[0].metadata.is_empty());
    assert!(document
        .diagnostics
        .iter()
        .any(|d| d.code == "W_PNG_FORMULA_SOURCE_REJECTED"));
    assert!(document.pages[0].blocks.is_empty());

    let document = DocumentImporter::from_bytes(&png(&[]), None, ImportOptions::default()).unwrap();
    assert!(document.assets[0].metadata.is_empty());
    assert!(document.diagnostics.is_empty());
}

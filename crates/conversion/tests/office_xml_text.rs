#![cfg(feature = "native")]

use latexsnipper_ast::{Block, Document, Inline};
use latexsnipper_conversion::{read_docx_bytes, read_pptx_bytes, read_xlsx_bytes};
use std::io::{Cursor, Write};

fn package(parts: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in parts {
        zip.start_file(*name, zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn docx(content: &str) -> Vec<u8> {
    package(&[(
        "word/document.xml",
        format!("<document><body><p><r><t>{content}</t></r></p></body></document>"),
    )])
}

fn pptx(content: &str) -> Vec<u8> {
    package(&[
        ("ppt/presentation.xml", "<presentation><sldIdLst><sldId id=\"1\" r:id=\"rId1\"/></sldIdLst></presentation>".into()),
        ("ppt/_rels/presentation.xml.rels", "<Relationships><Relationship Id=\"rId1\" Target=\"slides/slide1.xml\"/></Relationships>".into()),
        ("ppt/slides/slide1.xml", format!("<sld><cSld><spTree><sp><txBody><p><r><t>{content}</t></r></p></txBody></sp></spTree></cSld></sld>")),
    ])
}

fn xlsx(content: &str, shared: bool) -> Vec<u8> {
    let cell = if shared {
        "<c r=\"A1\" t=\"s\"><v>0</v></c>".to_string()
    } else {
        format!("<c r=\"A1\" t=\"inlineStr\"><is><t>{content}</t></is></c>")
    };
    let mut parts = vec![(
        "xl/worksheets/sheet1.xml",
        format!("<worksheet><sheetData><row r=\"1\">{cell}</row></sheetData></worksheet>"),
    )];
    if shared {
        parts.push((
            "xl/sharedStrings.xml",
            format!("<sst><si><t>{content}</t></si></sst>"),
        ));
    }
    package(&parts)
}

fn text(doc: &Document) -> String {
    fn blocks_text(blocks: &[Block]) -> String {
        blocks
            .iter()
            .map(|block| match block {
                Block::Paragraph(p) => p
                    .inlines
                    .iter()
                    .filter_map(|inline| match inline {
                        Inline::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect(),
                Block::Table(t) => t
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .map(|cell| blocks_text(&cell.content))
                    .collect(),
                _ => String::new(),
            })
            .collect()
    }
    doc.pages
        .iter()
        .map(|page| blocks_text(&page.blocks))
        .collect()
}

const CONTENT: &str =
    " a &amp; b &lt; c &gt; d &quot;x&quot; &apos;y&apos; &#20013;&#x1F600; <![CDATA[&amp;]]> ";
const EXPECTED: &str = " a & b < c > d \"x\" 'y' 中😀 &amp; ";

#[test]
fn docx_preserves_references_cdata_and_spaces() {
    assert_eq!(text(&read_docx_bytes(&docx(CONTENT)).unwrap()), EXPECTED);
}

#[test]
fn pptx_preserves_references_cdata_and_spaces() {
    assert_eq!(text(&read_pptx_bytes(&pptx(CONTENT)).unwrap()), EXPECTED);
}

#[test]
fn xlsx_inline_and_shared_strings_preserve_references_cdata_and_spaces() {
    for shared in [false, true] {
        assert_eq!(
            text(&read_xlsx_bytes(&xlsx(CONTENT, shared)).unwrap()),
            EXPECTED
        );
    }
}

#[test]
fn unsupported_and_illegal_references_do_not_return_silent_success() {
    for content in [
        "a &missing; b",
        "a &#0; b",
        "a &#xD800; b",
        "a &#x110000; b",
    ] {
        assert!(read_docx_bytes(&docx(content)).is_err());
        assert!(read_pptx_bytes(&pptx(content)).is_err());
        for shared in [false, true] {
            assert!(read_xlsx_bytes(&xlsx(content, shared)).is_err());
        }
    }
}

#[test]
fn docx_link_and_field_display_keep_decoded_text_and_run_style() {
    let bytes = package(&[
        ("word/document.xml", "<document><body><p><hyperlink r:id=\"r1\"><r><rPr><b/></rPr><t> a &amp; b </t></r></hyperlink><fldSimple instr=\"REF eq1\"><r><t> 1 &amp; 2 </t></r></fldSimple></p></body></document>".into()),
        ("word/_rels/document.xml.rels", "<Relationships><Relationship Id=\"r1\" Target=\"https://example.test/\"/></Relationships>".into()),
    ]);
    let document = read_docx_bytes(&bytes).unwrap();
    let Block::Paragraph(p) = &document.pages[0].blocks[0] else {
        panic!("Expected paragraph")
    };
    let Inline::Link(link) = &p.inlines[0] else {
        panic!("Expected link")
    };
    let label: String = link
        .content
        .iter()
        .map(|inline| {
            let Inline::Text(run) = inline else {
                panic!("Expected text")
            };
            assert_eq!(run.bold, Some(true));
            run.text.as_str()
        })
        .collect();
    assert_eq!(label, " a & b ");
    let Inline::CrossReference(reference) = &p.inlines[1] else {
        panic!("Expected reference")
    };
    assert_eq!(reference.display_text.as_deref(), Some(" 1 & 2 "));
}

#[test]
fn xlsx_formula_and_cached_string_keep_xml_references() {
    let bytes = package(&[("xl/worksheets/sheet1.xml", "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"str\"><f>IF(A2&lt;2,&quot;a &amp; b&quot;,&quot;c&quot;)</f><v> a &amp; b </v></c></row></sheetData></worksheet>".into())]);
    let document = read_xlsx_bytes(&bytes).unwrap();
    let Block::Table(table) = &document.pages[0].blocks[1] else {
        panic!("Expected table")
    };
    assert_eq!(
        table.rows[0].cells[0].formula.as_deref(),
        Some("IF(A2<2,\"a & b\",\"c\")")
    );
    assert_eq!(text(&document), " a & b ");
}

#[test]
fn mismatched_xml_end_tags_are_errors_not_partial_documents() {
    let invalid = "<t>a</wrong>";
    assert!(read_docx_bytes(&docx(invalid)).is_err());
    assert!(read_pptx_bytes(&pptx(invalid)).is_err());
    for shared in [false, true] {
        assert!(read_xlsx_bytes(&xlsx(invalid, shared)).is_err());
    }
}

#[test]
fn docx_self_closing_run_properties_respect_explicit_disabled_values() {
    for properties in [
        "<b val=\"0\"/><i val=\"false\"/><u val=\"none\"/>",
        "<b val=\"off\"></b><i val=\"0\"></i><u val=\"none\"></u>",
    ] {
        let bytes = package(&[("word/document.xml", format!("<document><body><p><r><rPr>{properties}</rPr><t>a &amp; b</t></r></p></body></document>"))]);
        let document = read_docx_bytes(&bytes).unwrap();
        let Block::Paragraph(p) = &document.pages[0].blocks[0] else {
            panic!("Expected paragraph")
        };
        for inline in &p.inlines {
            let Inline::Text(run) = inline else {
                panic!("Expected run")
            };
            assert_eq!(run.bold, Some(false));
            assert_eq!(run.italic, Some(false));
            assert_eq!(run.underline, Some(false));
        }
    }
}

#[test]
fn docx_table_with_properties_and_unicode_keeps_decoded_text() {
    let bytes = package(&[("word/document.xml", format!("<w:document><w:body><w:tbl><w:tblPr/><w:tr><w:tc><w:p><w:r><w:t>中文{CONTENT}</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"))]);
    let document = read_docx_bytes(&bytes).unwrap();
    assert_eq!(text(&document), format!("中文{EXPECTED}"));
    assert!(matches!(&document.pages[0].blocks[0], Block::Table(_)));
}

#[test]
fn docx_table_text_does_not_accept_illegal_references() {
    for content in ["&missing;", "&#0;"] {
        let bytes = package(&[("word/document.xml", format!("<w:document><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>{content}</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"))]);
        assert!(read_docx_bytes(&bytes).is_err());
    }
}

#[test]
fn adjacent_docx_tables_leave_unicode_paragraphs_and_cell_counts_intact() {
    let table = "<w:tbl><w:tblPr><w:tblStyle w:val=\"Normal\"/></w:tblPr><w:tr><w:tc><w:p><w:r><w:t>中 &amp; 文</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let bytes = package(&[("word/document.xml", format!("<w:document><w:body><w:p><w:r><w:t>前</w:t></w:r></w:p>{table}{table}<w:p><w:r><w:t>后</w:t></w:r></w:p></w:body></w:document>"))]);
    let document = read_docx_bytes(&bytes).unwrap();
    assert_eq!(text(&document), "前中 & 文中 & 文后");
    assert_eq!(
        document.pages[0]
            .blocks
            .iter()
            .filter(|block| matches!(block, Block::Table(_)))
            .count(),
        2
    );
}

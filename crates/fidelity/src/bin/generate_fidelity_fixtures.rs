use std::fs;
use std::io::Write;
use std::path::Path;

use latexsnipper_conversion::package_export::{
    OFFICE_THEME, PPTX_LAYOUT, PPTX_LAYOUT_RELS, PPTX_MASTER, PPTX_MASTER_RELS,
    PPTX_PRESENTATION_PROPERTIES, XLSX_STYLES,
};
use latexsnipper_fidelity::{validate_ooxml_package_structure, FidelityFormat};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use zip::write::FileOptions;

fn main() {
    if let Err(error) = generate() {
        eprintln!("generate-fidelity-fixtures: {error}");
        std::process::exit(2);
    }
}

fn generate() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new("fidelity/fixtures");
    fs::create_dir_all(root)?;
    write_docx(&root.join("office-rich.docx"))?;
    write_pptx(&root.join("presentation-rich.pptx"))?;
    write_xlsx(&root.join("workbook-rich.xlsx"))?;
    write_pdf(&root.join("pdf-rich.pdf"))?;
    Ok(())
}

fn options() -> FileOptions {
    FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644)
}

fn package(
    path: &Path,
    format: FidelityFormat,
    parts: &[(&str, &[u8])],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    for (name, bytes) in parts {
        zip.start_file(*name, options())?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;

    // Validate OPC structure immediately after writing.
    let bytes = fs::read(path)?;
    validate_ooxml_package_structure(&bytes, format)?;

    Ok(())
}

/// A valid 1x1 white PNG image (67 bytes).
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // PNG signature
    0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // IHDR
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
    0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c, // RGBA
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, // IDAT
    0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66,
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, // IEND
    0xae, 0x42, 0x60, 0x82,
];

// ---------------------------------------------------------------------------
// Unified OOXML helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct RelationshipSpec {
    id: &'static str,
    relationship_type: &'static str,
    target: &'static str,
}

fn relationships_xml(relationships: &[RelationshipSpec]) -> String {
    let mut xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships ",
        "xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
    )
    .to_string();

    for rel in relationships {
        xml.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
            rel.id, rel.relationship_type, rel.target,
        ));
    }

    xml.push_str("</Relationships>");
    xml
}

fn root_relationships(main_target: &'static str) -> String {
    relationships_xml(&[
        RelationshipSpec {
            id: "rId1",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
            target: main_target,
        },
        RelationshipSpec {
            id: "rId2",
            relationship_type:
                "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
            target: "docProps/core.xml",
        },
        RelationshipSpec {
            id: "rId3",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
            target: "docProps/app.xml",
        },
    ])
}

fn content_types_xml(defaults: &[(&str, &str)], overrides: &[(&str, &str)]) -> String {
    let mut xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Types ",
        "xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">"
    )
    .to_string();

    for (extension, content_type) in defaults {
        xml.push_str(&format!(
            "<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>"
        ));
    }

    for (part_name, content_type) in overrides {
        xml.push_str(&format!(
            "<Override PartName=\"{part_name}\" ContentType=\"{content_type}\"/>"
        ));
    }

    xml.push_str("</Types>");
    xml
}

fn core_properties_xml() -> &'static str {
    concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<cp:coreProperties ",
        "xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" ",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\">",
        "<dc:title>Fidelity Fixture</dc:title>",
        "</cp:coreProperties>"
    )
}

fn app_properties_xml() -> &'static str {
    concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Properties ",
        "xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\">",
        "<Application>LaTeXSnipper</Application>",
        "</Properties>"
    )
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

fn write_docx(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root_rels = root_relationships("word/document.xml");

    let content_types = content_types_xml(
        &[
            (
                "rels",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
            ("xml", "application/xml"),
            ("png", "image/png"),
        ],
        &[
            (
                "/word/document.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            ),
            (
                "/word/styles.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            ),
            (
                "/word/numbering.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
            ),
            (
                "/word/header1.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
            ),
            (
                "/word/footer1.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
            ),
            (
                "/word/footnotes.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
            ),
            (
                "/word/comments.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            ),
            (
                "/docProps/core.xml",
                "application/vnd.openxmlformats-package.core-properties+xml",
            ),
            (
                "/docProps/app.xml",
                "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            ),
        ],
    );

    let docx_numbering = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:numbering ",
        "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:abstractNum w:abstractNumId=\"0\">",
        "<w:lvl w:ilvl=\"0\">",
        "<w:start w:val=\"1\"/>",
        "<w:numFmt w:val=\"decimal\"/>",
        "<w:lvlText w:val=\"%1.\"/>",
        "<w:lvlJc w:val=\"left\"/>",
        "</w:lvl>",
        "</w:abstractNum>",
        "<w:num w:numId=\"1\">",
        "<w:abstractNumId w:val=\"0\"/>",
        "</w:num>",
        "</w:numbering>",
    );

    let document = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<w:document ",
        "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
        "xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" ",
        "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
        "<w:body>",
        // Heading with bold + color
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>",
        "<w:r><w:rPr><w:b/><w:color w:val=\"336699\"/></w:rPr><w:t>Office Fidelity Heading</w:t></w:r>",
        "<w:r><w:t> styled run</w:t></w:r></w:p>",
        // Numbered list item
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>",
        "<w:r><w:t>List item</w:t></w:r></w:p>",
        // Table with merged cell
        "<w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>",
        "<w:p><w:r><w:t>Merged cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        // OMML formula (must be inside w:p)
        "<w:p>",
        "<m:oMathPara>",
        "<m:oMath><m:r><m:t>x+1</m:t></m:r></m:oMath>",
        "</m:oMathPara>",
        "</w:p>",
        // Image with full DrawingML structure
        "<w:p><w:r><w:drawing>",
        "<wp:inline>",
        "<wp:extent cx=\"9525\" cy=\"9525\"/>",
        "<wp:docPr id=\"1\" name=\"Fidelity Image\"/>",
        "<a:graphic>",
        "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
        "<pic:pic>",
        "<pic:nvPicPr>",
        "<pic:cNvPr id=\"1\" name=\"image1.png\"/>",
        "<pic:cNvPicPr/>",
        "</pic:nvPicPr>",
        "<pic:blipFill>",
        "<a:blip r:embed=\"rIdImage\"/>",
        "<a:stretch><a:fillRect/></a:stretch>",
        "</pic:blipFill>",
        "<pic:spPr>",
        "<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"9525\" cy=\"9525\"/></a:xfrm>",
        "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>",
        "</pic:spPr>",
        "</pic:pic>",
        "</a:graphicData>",
        "</a:graphic>",
        "</wp:inline>",
        "</w:drawing></w:r></w:p>",
        // Footnote reference (inside w:r), comment, track changes
        "<w:p>",
        "<w:r><w:footnoteReference w:id=\"1\"/></w:r>",
        "<w:commentRangeStart w:id=\"0\"/>",
        "<w:ins><w:r><w:t>Inserted</w:t></w:r></w:ins>",
        "<w:del><w:r><w:delText>Deleted</w:delText></w:r></w:del>",
        "</w:p>",
        // Section properties
        "<w:sectPr><w:headerReference r:id=\"rIdHeader\"/>",
        "<w:footerReference r:id=\"rIdFooter\"/>",
        "<w:type w:val=\"nextPage\"/></w:sectPr>",
        "</w:body></w:document>",
    );

    let rels = relationships_xml(&[
        RelationshipSpec {
            id: "rIdStyles",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
            target: "styles.xml",
        },
        RelationshipSpec {
            id: "rIdNumbering",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
            target: "numbering.xml",
        },
        RelationshipSpec {
            id: "rIdImage",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
            target: "media/image1.png",
        },
        RelationshipSpec {
            id: "rIdHeader",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
            target: "header1.xml",
        },
        RelationshipSpec {
            id: "rIdFooter",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer",
            target: "footer1.xml",
        },
        RelationshipSpec {
            id: "rIdFootnotes",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
            target: "footnotes.xml",
        },
        RelationshipSpec {
            id: "rIdComments",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
            target: "comments.xml",
        },
    ]);

    package(
        path,
        FidelityFormat::Docx,
        &[
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", root_rels.as_bytes()),
            ("docProps/core.xml", core_properties_xml().as_bytes()),
            ("docProps/app.xml", app_properties_xml().as_bytes()),
            ("word/document.xml", document.as_bytes()),
            ("word/_rels/document.xml.rels", rels.as_bytes()),
            ("word/styles.xml", br#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="Heading1"/></w:styles>"#),
            ("word/numbering.xml", docx_numbering.as_bytes()),
            ("word/header1.xml", br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:hdr>"#),
            ("word/footer1.xml", br#"<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Footer</w:t></w:r></w:p></w:ftr>"#),
            ("word/footnotes.xml", br#"<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="1"><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:footnote></w:footnotes>"#),
            ("word/comments.xml", br#"<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="0"><w:p><w:r><w:t>Comment</w:t></w:r></w:p></w:comment></w:comments>"#),
            ("word/media/image1.png", ONE_PIXEL_PNG),
            ("customXml/fidelity.xml", b"<fidelity opaque=\"true\">DOCX_OPAQUE_PART</fidelity>"),
        ],
    )
}

// ---------------------------------------------------------------------------
// PPTX
// ---------------------------------------------------------------------------

fn write_pptx(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root_rels = root_relationships("ppt/presentation.xml");

    let content_types = content_types_xml(
        &[
            (
                "rels",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
            ("xml", "application/xml"),
            ("png", "image/png"),
        ],
        &[
            (
                "/ppt/presentation.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            ),
            (
                "/ppt/presProps.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml",
            ),
            (
                "/ppt/slides/slide1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
            ),
            (
                "/ppt/slideMasters/slideMaster1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
            ),
            (
                "/ppt/slideLayouts/slideLayout1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
            ),
            (
                "/ppt/theme/theme1.xml",
                "application/vnd.openxmlformats-officedocument.theme+xml",
            ),
            (
                "/docProps/core.xml",
                "application/vnd.openxmlformats-package.core-properties+xml",
            ),
            (
                "/docProps/app.xml",
                "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            ),
        ],
    );

    let presentation = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<p:presentation ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
        "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">",
        "<p:sldMasterIdLst>",
        "<p:sldMasterId id=\"2147483648\" r:id=\"rIdMaster\"/>",
        "</p:sldMasterIdLst>",
        "<p:sldIdLst>",
        "<p:sldId id=\"256\" r:id=\"rId1\"/>",
        "</p:sldIdLst>",
        "<p:sldSz cx=\"12192000\" cy=\"6858000\"/>",
        "<p:notesSz cx=\"6858000\" cy=\"9144000\"/>",
        "</p:presentation>",
    );

    let presentation_rels = relationships_xml(&[
        RelationshipSpec {
            id: "rId1",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
            target: "slides/slide1.xml",
        },
        RelationshipSpec {
            id: "rIdMaster",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster",
            target: "slideMasters/slideMaster1.xml",
        },
        RelationshipSpec {
            id: "rIdPresProps",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps",
            target: "presProps.xml",
        },
    ]);

    // Proper PresentationML slide with spTree, text box, and picture.
    let slide = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<p:sld ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
        "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">",
        "<p:cSld>",
        "<p:spTree>",
        // Required spTree root properties
        "<p:nvGrpSpPr>",
        "<p:cNvPr id=\"1\" name=\"\"/>",
        "<p:cNvGrpSpPr/>",
        "<p:nvPr/>",
        "</p:nvGrpSpPr>",
        "<p:grpSpPr>",
        "<a:xfrm>",
        "<a:off x=\"0\" y=\"0\"/>",
        "<a:ext cx=\"0\" cy=\"0\"/>",
        "<a:chOff x=\"0\" y=\"0\"/>",
        "<a:chExt cx=\"0\" cy=\"0\"/>",
        "</a:xfrm>",
        "</p:grpSpPr>",
        // Text box with full nvSpPr structure
        "<p:sp>",
        "<p:nvSpPr>",
        "<p:cNvPr id=\"2\" name=\"Text Box\"/>",
        "<p:cNvSpPr txBox=\"1\"/>",
        "<p:nvPr/>",
        "</p:nvSpPr>",
        "<p:spPr>",
        "<a:xfrm>",
        "<a:off x=\"400000\" y=\"300000\"/>",
        "<a:ext cx=\"8000000\" cy=\"600000\"/>",
        "</a:xfrm>",
        "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>",
        "<a:noFill/>",
        "</p:spPr>",
        "<p:txBody>",
        "<a:bodyPr/>",
        "<a:lstStyle/>",
        "<a:p>",
        "<a:r>",
        "<a:rPr lang=\"en-US\"/>",
        "<a:t>Presentation Fidelity Text Box</a:t>",
        "</a:r>",
        "<a:endParaRPr lang=\"en-US\"/>",
        "</a:p>",
        "</p:txBody>",
        "</p:sp>",
        // Picture with full structure
        "<p:pic>",
        "<p:nvPicPr>",
        "<p:cNvPr id=\"3\" name=\"image1.png\"/>",
        "<p:cNvPicPr/>",
        "<p:nvPr/>",
        "</p:nvPicPr>",
        "<p:blipFill>",
        "<a:blip r:embed=\"rIdImage\"/>",
        "<a:stretch><a:fillRect/></a:stretch>",
        "</p:blipFill>",
        "<p:spPr>",
        "<a:xfrm>",
        "<a:off x=\"400000\" y=\"1000000\"/>",
        "<a:ext cx=\"1000000\" cy=\"1000000\"/>",
        "</a:xfrm>",
        "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>",
        "</p:spPr>",
        "</p:pic>",
        "</p:spTree>",
        "</p:cSld>",
        "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",
        "</p:sld>",
    );

    let slide_rels = relationships_xml(&[
        RelationshipSpec {
            id: "rIdLayout",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout",
            target: "../slideLayouts/slideLayout1.xml",
        },
        RelationshipSpec {
            id: "rIdImage",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
            target: "../media/image1.png",
        },
    ]);

    package(
        path,
        FidelityFormat::Pptx,
        &[
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", root_rels.as_bytes()),
            ("docProps/core.xml", core_properties_xml().as_bytes()),
            ("docProps/app.xml", app_properties_xml().as_bytes()),
            ("ppt/presentation.xml", presentation.as_bytes()),
            (
                "ppt/_rels/presentation.xml.rels",
                presentation_rels.as_bytes(),
            ),
            ("ppt/presProps.xml", PPTX_PRESENTATION_PROPERTIES.as_bytes()),
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
            ("ppt/slideMasters/slideMaster1.xml", PPTX_MASTER.as_bytes()),
            (
                "ppt/slideMasters/_rels/slideMaster1.xml.rels",
                PPTX_MASTER_RELS.as_bytes(),
            ),
            ("ppt/slideLayouts/slideLayout1.xml", PPTX_LAYOUT.as_bytes()),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                PPTX_LAYOUT_RELS.as_bytes(),
            ),
            ("ppt/theme/theme1.xml", OFFICE_THEME.as_bytes()),
            ("ppt/media/image1.png", ONE_PIXEL_PNG),
        ],
    )
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

fn write_xlsx(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root_rels = root_relationships("xl/workbook.xml");

    let content_types = content_types_xml(
        &[
            (
                "rels",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
            ("xml", "application/xml"),
        ],
        &[
            (
                "/xl/workbook.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
            ),
            (
                "/xl/worksheets/sheet1.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
            ),
            (
                "/xl/styles.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
            ),
            (
                "/xl/tables/table1.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
            ),
            (
                "/xl/drawings/drawing1.xml",
                "application/vnd.openxmlformats-officedocument.drawing+xml",
            ),
            (
                "/xl/charts/chart1.xml",
                "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            ),
            (
                "/xl/pivotTables/pivotTable1.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml",
            ),
            ("/xl/vbaProject.bin", "application/vnd.ms-office.vbaProject"),
            (
                "/xl/embeddings/oleObject1.bin",
                "application/vnd.openxmlformats-officedocument.oleObject",
            ),
            (
                "/docProps/core.xml",
                "application/vnd.openxmlformats-package.core-properties+xml",
            ),
            (
                "/docProps/app.xml",
                "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            ),
        ],
    );

    let workbook = br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Fidelity" sheetId="1" r:id="rId1"/></sheets></workbook>"#;

    let rels = relationships_xml(&[
        RelationshipSpec {
            id: "rId1",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
            target: "worksheets/sheet1.xml",
        },
        RelationshipSpec {
            id: "rIdStyles",
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
            target: "styles.xml",
        },
    ]);

    let sheet = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<worksheet ",
        "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
        "<dimension ref=\"A1:D3\"/>",
        "<cols>",
        "<col min=\"1\" max=\"4\" width=\"18\" customWidth=\"1\"/>",
        "</cols>",
        "<sheetData>",
        // Row 1: header row (table column names)
        "<row r=\"1\" ht=\"24\" customHeight=\"1\">",
        "<c r=\"A1\" t=\"inlineStr\"><is><t>Column1</t></is></c>",
        "<c r=\"B1\" t=\"inlineStr\"><is><t>Column2</t></is></c>",
        "<c r=\"C1\" t=\"inlineStr\"><is><t>Column3</t></is></c>",
        "<c r=\"D1\" t=\"inlineStr\"><is><t>Column4</t></is></c>",
        "</row>",
        // Row 2: data row
        "<row r=\"2\">",
        "<c r=\"A2\"><f>SUM(C2,8)</f><v>50</v></c>",
        "<c r=\"B2\" t=\"b\"><v>1</v></c>",
        "<c r=\"C2\" t=\"n\"><v>42</v></c>",
        "<c r=\"D2\" t=\"e\"><v>#N/A</v></c>",
        "</row>",
        "</sheetData>",
        // Merge outside table range
        "<mergeCells count=\"1\"><mergeCell ref=\"A3:D3\"/></mergeCells>",
        // Conditional formatting on data row
        "<conditionalFormatting sqref=\"C2\">",
        "<cfRule type=\"cellIs\" priority=\"1\" operator=\"greaterThan\"><formula>10</formula></cfRule>",
        "</conditionalFormatting>",
        "<tableParts count=\"1\"><tablePart r:id=\"rIdTable\"/></tableParts>",
        "</worksheet>",
    );

    let sheet_rels = relationships_xml(&[RelationshipSpec {
        id: "rIdTable",
        relationship_type:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table",
        target: "../tables/table1.xml",
    }]);

    // Table covers only data rows (A1:D2), not the merged range A3:D3
    let table_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<table ",
        "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
        "id=\"1\" name=\"FidelityTable\" displayName=\"FidelityTable\" ",
        "ref=\"A1:D2\" headerRowCount=\"1\" totalsRowShown=\"0\">",
        "<autoFilter ref=\"A1:D2\"/>",
        "<tableColumns count=\"4\">",
        "<tableColumn id=\"1\" name=\"Column1\"/>",
        "<tableColumn id=\"2\" name=\"Column2\"/>",
        "<tableColumn id=\"3\" name=\"Column3\"/>",
        "<tableColumn id=\"4\" name=\"Column4\"/>",
        "</tableColumns>",
        "<tableStyleInfo name=\"TableStyleMedium2\" ",
        "showFirstColumn=\"0\" showLastColumn=\"0\" ",
        "showRowStripes=\"1\" showColumnStripes=\"0\"/>",
        "</table>",
    );

    package(
        path,
        FidelityFormat::Xlsx,
        &[
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", root_rels.as_bytes()),
            ("docProps/core.xml", core_properties_xml().as_bytes()),
            ("docProps/app.xml", app_properties_xml().as_bytes()),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
            ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
            ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
            ("xl/styles.xml", XLSX_STYLES.as_bytes()),
            ("xl/tables/table1.xml", table_xml.as_bytes()),
            ("xl/drawings/drawing1.xml", br#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"><xdr:graphicFrame/></xdr:wsDr>"#),
            ("xl/charts/chart1.xml", br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart/></c:chartSpace>"#),
            ("xl/pivotTables/pivotTable1.xml", br#"<pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="FidelityPivot"/>"#),
            ("xl/vbaProject.bin", b"VBA_PROJECT_FIDELITY_MACRO"),
            ("xl/embeddings/oleObject1.bin", b"XLSX_EMBEDDED_OBJECT"),
            ("xl/customXml/fidelity.xml", b"<fidelity>XLSX_OPAQUE_PART</fidelity>"),
        ],
    )
}

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

fn write_pdf(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_file_id = document.add_object(Stream::new(
        dictionary! {"Length1" => 8},
        b"FONTDATA".to_vec(),
    ));
    let descriptor_id = document.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "FidelityEmbedded",
        "Flags" => 4,
        "FontBBox" => vec![0.into(), (-200).into(), 1000.into(), 900.into()],
        "ItalicAngle" => 0,
        "Ascent" => 800,
        "Descent" => -200,
        "CapHeight" => 700,
        "StemV" => 80,
        "FontFile2" => font_file_id,
    });
    let cid_font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "FidelityEmbedded",
        "CIDSystemInfo" => dictionary! {"Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0},
        "FontDescriptor" => descriptor_id,
    });
    let cjk_font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "FidelityEmbedded",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![cid_font_id.into()],
    });
    let latin_font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"
    });
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 1,
            "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        vec![0x80, 0x80, 0x80],
    ));
    let gs_id = document.add_object(dictionary! {
        "Type" => "ExtGState", "CA" => 0.5, "ca" => 0.5
    });
    let base_content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![50.into(), 740.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal("PDF Fidelity left column")],
            ),
            Operation::new("Td", vec![250.into(), 0.into()]),
            Operation::new("Tj", vec![Object::string_literal("right column")]),
            Operation::new("ET", vec![]),
            Operation::new("m", vec![50.into(), 700.into()]),
            Operation::new("l", vec![550.into(), 700.into()]),
            Operation::new("S", vec![]),
            Operation::new("gs", vec![Object::Name(b"GS1".to_vec())]),
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    100.into(),
                    0.into(),
                    0.into(),
                    100.into(),
                    50.into(),
                    560.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Scan1".to_vec())]),
            Operation::new("Q", vec![]),
        ],
    }
    .encode()?;
    let overlay_content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F2".to_vec()), 14.into()]),
            Operation::new(
                "Tm",
                vec![
                    0.into(),
                    1.into(),
                    (-1).into(),
                    0.into(),
                    500.into(),
                    100.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::String(
                    vec![0x4f, 0x60, 0x59, 0x7d],
                    lopdf::StringFormat::Hexadecimal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()?;
    let base_id = document.add_object(Stream::new(Dictionary::new(), base_content));
    let overlay_id = document.add_object(Stream::new(Dictionary::new(), overlay_content));
    let annotation_id = document.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text",
        "Rect" => vec![50.into(), 500.into(), 70.into(), 520.into()],
        "Contents" => Object::string_literal("Fidelity annotation"),
    });
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! {
            "Font" => dictionary! {"F1" => latin_font_id, "F2" => cjk_font_id},
            "XObject" => dictionary! {"Scan1" => image_id},
            "ExtGState" => dictionary! {"GS1" => gs_id},
        },
        "Contents" => vec![base_id.into(), overlay_id.into()],
        "Annots" => vec![annotation_id.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1
        }),
    );
    let catalog_id = document.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    document.trailer.set("Root", catalog_id);
    document.save(path)?;
    Ok(())
}

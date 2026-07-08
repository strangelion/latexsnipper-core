//! OOXML fragment writer — converts Document AST blocks into Word OOXML body XML.
//!
//! Produces a `<w:body>`-compatible XML fragment suitable for insertion into
//! a Word document via the Office Open XML format. Supports:
//! - Text paragraphs with bold/italic/underline runs
//! - Formulas as OMML (`<m:oMathPara>`)
//! - Tables (`<w:tbl>`) via delegation to `word_ooxml_table_writer`
//! - Images as `<w:drawing>` placeholders (requires asset resolver for actual bytes)
//!
//! The output can be wrapped in a Flat OPC XML or inserted directly via
//! Word's `Range.InsertXML` API.

use latexsnipper_ast::{Block, Inline};

/// Write a list of blocks as a Word OOXML body fragment.
///
/// Each block becomes one or more `<w:p>` (paragraph) elements.
/// Returns the XML string, ready for insertion into `<w:body>...</w:body>`.
pub fn write_ooxml_fragment(blocks: &[Block], assets: &[latexsnipper_ast::MediaAsset]) -> String {
    let mut parts = Vec::new();

    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                parts.push(write_paragraph(&p.inlines, assets));
            }
            Block::Heading(h) => {
                parts.push(write_paragraph(&h.inlines, assets));
            }
            Block::Formula(f) => {
                let omml = crate::omml::latex_to_omml(&f.formula.as_latex());
                if !omml.is_empty() {
                    parts.push(format!(
                        "<w:p><w:r><w:rPr></w:rPr><w:br/><m:oMathPara>{}</m:oMathPara></w:r></w:p>",
                        omml
                    ));
                }
            }
            Block::Table(t) => {
                parts.push(crate::word_ooxml_table_writer::write_word_table_ooxml(t));
            }
            Block::Code(c) => {
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                    xml_escape(&c.code)
                ));
            }
            Block::HorizontalRule(_) => {
                parts.push(
                    r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="12" w:space="1" w:color="auto"/></w:pBdr></w:pPr></w:p>"#.to_string(),
                );
            }
            Block::Quote(q) => {
                let inner = write_ooxml_fragment(&q.blocks, assets);
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">"</w:t></w:r></w:p>{}<w:p><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">"</w:t></w:r></w:p>"#,
                    inner
                ));
            }
            Block::List(l) => {
                for item in &l.items {
                    parts.push(write_ooxml_fragment(&item.content, assets));
                }
            }
            Block::Figure(fig) => {
                if let Some(ref aid) = fig.asset_id {
                    let asset = assets.iter().find(|a| a.id == *aid);
                    let img_rel = format!("rId_{}", aid.0);
                    if asset.is_some() {
                        parts.push(format!(
                            r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
                            <wp:extent cx="914400" cy="914400"/>
                            <a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                            <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
                            <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
                            <pic:blipFill><a:blip r:embed="{}"/></pic:blipFill>
                            <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></pic:spPr>
                            </pic:pic>
                            </a:graphicData>
                            </a:graphic>
                            </wp:inline></w:drawing></w:r></w:p>"#,
                            img_rel
                        ));
                        // Emit a caption paragraph if present
                        if let Some(ref cap) = fig.caption {
                            parts.push(format!(
                                r#"<w:p><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                                xml_escape(cap)
                            ));
                        }
                    } else {
                        // Asset not found — placeholder text
                        parts.push(format!(
                            r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[image: {}]</w:t></w:r></w:p>"#,
                            aid.0
                        ));
                    }
                } else {
                    // No asset_id → try legacy image_data or caption
                    if let Some(ref _data) = fig.image_data {
                        parts.push(format!(
                            r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[embedded image]</w:t></w:r></w:p>"#
                        ));
                    }
                }
            }
            Block::TextBox(tb) => {
                let inner = write_ooxml_fragment(&tb.content, assets);
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:shd w:fill="F2F2F2"/></w:rPr><w:t xml:space="preserve">[textbox]</w:t></w:r></w:p>{}"#,
                    inner
                ));
            }
            Block::Chart(c) => {
                let name = format!("{:?}", c.chart_type).to_lowercase();
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[Chart: {}]</w:t></w:r></w:p>"#,
                    name
                ));
            }
            Block::Shape(s) => {
                let name = format!("{:?}", s.shape_type).to_lowercase();
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[Shape: {}]</w:t></w:r></w:p>"#,
                    name
                ));
            }
            Block::EmbeddedObject(e) => {
                let kind = format!("{:?}", e.kind).to_lowercase();
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[Embedded: {}]</w:t></w:r></w:p>"#,
                    kind
                ));
            }
            Block::Annotation(a) => {
                let kind = format!("{:?}", a.kind).to_lowercase();
                parts.push(format!(
                    r#"<w:p><w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[Annotation: {}]</w:t></w:r></w:p>"#,
                    kind
                ));
            }
            _ => {}
        }
    }

    parts.join("\n")
}

/// Write a paragraph from inlines, producing `<w:p>` XML with runs.
fn write_paragraph(inlines: &[Inline], _assets: &[latexsnipper_ast::MediaAsset]) -> String {
    let mut runs = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let mut rpr = String::new();
                if t.bold == Some(true) {
                    rpr.push_str("<w:b/>");
                }
                if t.italic == Some(true) {
                    rpr.push_str("<w:i/>");
                }
                if t.underline == Some(true) {
                    rpr.push_str("<w:u w:val=\"single\"/>");
                }
                if rpr.is_empty() {
                    runs.push(format!(
                        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        xml_escape(&t.text)
                    ));
                } else {
                    runs.push(format!(
                        r#"<w:r><w:rPr>{}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        rpr,
                        xml_escape(&t.text)
                    ));
                }
            }
            Inline::Formula(f) => {
                let omml = crate::omml::latex_to_omml(&f.as_latex());
                if !omml.is_empty() {
                    runs.push(format!("<m:oMath>{}</m:oMath>", omml));
                }
            }
            Inline::Image(img) => {
                if let Some(ref aid) = img.asset_id {
                    runs.push(format!(
                        r#"<w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[image: {}]</w:t></w:r>"#,
                        aid.0
                    ));
                }
            }
            Inline::LineBreak | Inline::SoftBreak => {
                runs.push(r#"<w:r><w:br/></w:r>"#.to_string());
            }
            Inline::Span(s) => {
                let inner = write_paragraph(&s.content, _assets);
                // Strip <w:p> and </w:p> wrappers from inner content
                let cleaned = inner
                    .replace("<w:p>", "")
                    .replace("</w:p>", "")
                    .replace('\n', "")
                    .trim()
                    .to_string();
                if !cleaned.is_empty() {
                    runs.push(cleaned);
                }
            }
            Inline::Link(l) => {
                let inner = write_paragraph(&l.content, _assets);
                let cleaned = inner
                    .replace("<w:p>", "")
                    .replace("</w:p>", "")
                    .replace('\n', "")
                    .trim()
                    .to_string();
                runs.push(format!(
                    r#"<w:hyperlink r:id="{}">{}</w:hyperlink>"#,
                    xml_escape(&l.target),
                    cleaned
                ));
            }
            Inline::Code(c) => {
                runs.push(format!(
                    r#"<w:r><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&c.code)
                ));
            }
            Inline::Superscript(inner) => {
                let inner_str = write_paragraph(inner, _assets)
                    .replace("<w:p>", "")
                    .replace("</w:p>", "");
                runs.push(format!(
                    r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&inner_str)
                ));
            }
            Inline::Subscript(inner) => {
                let inner_str = write_paragraph(inner, _assets)
                    .replace("<w:p>", "")
                    .replace("</w:p>", "");
                runs.push(format!(
                    r#"<w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&inner_str)
                ));
            }
            Inline::Anchor(a) => {
                runs.push(format!(
                    r#"<w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[anchor: {}]</w:t></w:r>"#,
                    a.id
                ));
            }
            Inline::CrossReference(x) => {
                runs.push(format!(
                    r#"<w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[xref: {}]</w:t></w:r>"#,
                    x.target_id
                ));
            }
            Inline::CitationGroup(c) => {
                let keys: Vec<&str> = c.citations.iter().map(|ci| ci.key.as_str()).collect();
                runs.push(format!(
                    r#"<w:r><w:rPr><w:i/><w:color w:val="808080"/></w:rPr><w:t xml:space="preserve">[cite: {}]</w:t></w:r>"#,
                    keys.join(", ")
                ));
            }
            _ => {}
        }
    }

    if runs.is_empty() {
        String::new()
    } else {
        format!("<w:p>{}</w:p>", runs.join("\n"))
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

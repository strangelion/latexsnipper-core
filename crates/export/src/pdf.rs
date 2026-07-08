use std::io::BufWriter;

use crate::generator::Generator;
use crate::render_tree::{RenderNode, RenderTree};
use latexsnipper_foundation::{Result, SnipperError};
use printpdf::{BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfLayerIndex, PdfPageIndex};

const PAGE_WIDTH: f32 = 595.0;
const PAGE_HEIGHT: f32 = 842.0;
const MARGIN_LEFT: f32 = 50.0;
const MARGIN_TOP: f32 = 50.0;
const LINE_HEIGHT: f32 = 14.0;
const FONT_SIZE: f32 = 11.0;

pub struct PdfGenerator;

impl Generator for PdfGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<String> {
        let (doc, page_idx, layer_idx) = PdfDocument::new(
            "LaTeXSnipper",
            Mm(PAGE_WIDTH * 0.3528),
            Mm(PAGE_HEIGHT * 0.3528),
            "Layer 1",
        );

        let font = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| SnipperError::Export(format!("Failed to add font: {}", e)))?;

        let font_bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| SnipperError::Export(format!("Failed to add bold font: {}", e)))?;

        let font_mono = doc
            .add_builtin_font(BuiltinFont::Courier)
            .map_err(|e| SnipperError::Export(format!("Failed to add mono font: {}", e)))?;

        let mut current_page_idx = page_idx;
        let mut current_layer_idx = layer_idx;
        let mut y = PAGE_HEIGHT - MARGIN_TOP;
        let mut page_num: u32 = 0;

        for node in &tree.nodes {
            if let RenderNode::Page(children) = node {
                if page_num > 0 {
                    let (new_page, new_layer) =
                        doc.add_page(Mm(PAGE_WIDTH * 0.3528), Mm(PAGE_HEIGHT * 0.3528), "Layer 1");
                    current_page_idx = new_page;
                    current_layer_idx = new_layer;
                    y = PAGE_HEIGHT - MARGIN_TOP;
                }
                page_num += 1;

                for child in children {
                    let result = render_node(
                        child,
                        &doc,
                        current_page_idx,
                        current_layer_idx,
                        &font,
                        &font_bold,
                        &font_mono,
                        &mut y,
                    )?;
                    match result {
                        RenderResult::Ok(new_y) => y = new_y,
                        RenderResult::NewPage => {
                            let (new_page, new_layer) = doc.add_page(
                                Mm(PAGE_WIDTH * 0.3528),
                                Mm(PAGE_HEIGHT * 0.3528),
                                "Layer 1",
                            );
                            current_page_idx = new_page;
                            current_layer_idx = new_layer;
                            y = PAGE_HEIGHT - MARGIN_TOP;
                        }
                    }
                }
            }
        }

        let mut buf = BufWriter::new(Vec::new());
        doc.save(&mut buf)
            .map_err(|e| SnipperError::Export(format!("Failed to save PDF: {}", e)))?;

        let bytes = buf
            .into_inner()
            .map_err(|e| SnipperError::Export(format!("Failed to get PDF bytes: {}", e)))?;

        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    fn extension(&self) -> &str {
        "pdf"
    }
    fn mime_type(&self) -> &str {
        "application/pdf"
    }
    fn name(&self) -> &str {
        "pdf"
    }
}

enum RenderResult {
    Ok(f32),
    NewPage,
}

fn get_layer(
    doc: &printpdf::PdfDocumentReference,
    page_idx: PdfPageIndex,
    layer_idx: PdfLayerIndex,
) -> printpdf::PdfLayerReference {
    let page_ref = doc.get_page(page_idx);
    page_ref.get_layer(layer_idx)
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    node: &RenderNode,
    doc: &printpdf::PdfDocumentReference,
    page_idx: PdfPageIndex,
    layer_idx: PdfLayerIndex,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    font_mono: &IndirectFontRef,
    y: &mut f32,
) -> Result<RenderResult> {
    if *y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
        return Ok(RenderResult::NewPage);
    }

    let layer = get_layer(doc, page_idx, layer_idx);

    match node {
        RenderNode::Text(text) => {
            if !text.is_empty() {
                layer.use_text(
                    text.as_str(),
                    FONT_SIZE,
                    Mm(MARGIN_LEFT * 0.3528),
                    Mm(*y * 0.3528),
                    font,
                );
            }
            Ok(RenderResult::Ok(*y - LINE_HEIGHT))
        }
        RenderNode::Formula {
            latex,
            display_mode,
        } => {
            let prefix = if *display_mode { "$$ " } else { "$ " };
            let suffix = if *display_mode { " $$" } else { " $" };
            let text = format!("{}{}{}", prefix, latex, suffix);
            layer.use_text(
                text.as_str(),
                FONT_SIZE,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font,
            );
            let new_y = if *display_mode {
                *y - LINE_HEIGHT * 2.0
            } else {
                *y - LINE_HEIGHT
            };
            Ok(RenderResult::Ok(new_y))
        }
        RenderNode::Paragraph(nodes) => {
            let mut current_y = *y;
            for child in nodes {
                if current_y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
                    return Ok(RenderResult::NewPage);
                }
                let result = render_node(
                    child,
                    doc,
                    page_idx,
                    layer_idx,
                    font,
                    font_bold,
                    font_mono,
                    &mut current_y,
                )?;
                match result {
                    RenderResult::Ok(new_y) => current_y = new_y,
                    RenderResult::NewPage => return Ok(RenderResult::NewPage),
                }
            }
            Ok(RenderResult::Ok(current_y - LINE_HEIGHT))
        }
        RenderNode::Heading { level, nodes } => {
            let size = 20.0 - (*level as f32) * 2.0;
            let text: String = nodes.iter().map(node_to_text).collect();
            layer.use_text(
                text.as_str(),
                size,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font_bold,
            );
            Ok(RenderResult::Ok(*y - size - 4.0))
        }
        RenderNode::Table { rows } => {
            let mut current_y = *y;
            let col_width = 120.0;

            for row in rows {
                if current_y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
                    return Ok(RenderResult::NewPage);
                }

                let mut x = MARGIN_LEFT;
                for cell in row {
                    let text: String = cell.iter().map(node_to_text).collect();
                    layer.use_text(
                        text.as_str(),
                        9.0,
                        Mm(x * 0.3528),
                        Mm(current_y * 0.3528),
                        font,
                    );
                    x += col_width;
                }
                current_y -= LINE_HEIGHT + 2.0;
            }

            Ok(RenderResult::Ok(current_y - LINE_HEIGHT))
        }
        RenderNode::List { ordered, items } => {
            let mut current_y = *y;
            for (i, item) in items.iter().enumerate() {
                if current_y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
                    return Ok(RenderResult::NewPage);
                }
                let text: String = item.iter().map(node_to_text).collect();
                let bullet = if *ordered {
                    format!("{}.", i + 1)
                } else {
                    "\u{2022}".to_string()
                };
                let line = format!("  {} {}", bullet, text);
                layer.use_text(
                    line.as_str(),
                    FONT_SIZE,
                    Mm(MARGIN_LEFT * 0.3528),
                    Mm(current_y * 0.3528),
                    font,
                );
                current_y -= LINE_HEIGHT;
            }
            Ok(RenderResult::Ok(current_y))
        }
        RenderNode::Code { code, .. } => {
            for line in code.lines() {
                if *y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
                    return Ok(RenderResult::NewPage);
                }
                layer.use_text(
                    line,
                    10.0,
                    Mm((MARGIN_LEFT + 10.0) * 0.3528),
                    Mm(*y * 0.3528),
                    font_mono,
                );
                *y -= LINE_HEIGHT;
            }
            Ok(RenderResult::Ok(*y - 4.0))
        }
        RenderNode::Quote(nodes) => {
            let mut current_y = *y;
            for child in nodes {
                if current_y < MARGIN_LEFT + LINE_HEIGHT * 2.0 {
                    return Ok(RenderResult::NewPage);
                }
                let text = node_to_text(child);
                let line = format!("| {}", text);
                layer.use_text(
                    line.as_str(),
                    FONT_SIZE,
                    Mm((MARGIN_LEFT + 10.0) * 0.3528),
                    Mm(current_y * 0.3528),
                    font,
                );
                current_y -= LINE_HEIGHT;
            }
            Ok(RenderResult::Ok(current_y - 4.0))
        }
        RenderNode::HorizontalRule => {
            let line = "-".repeat(70);
            layer.use_text(
                line.as_str(),
                8.0,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font,
            );
            Ok(RenderResult::Ok(*y - LINE_HEIGHT))
        }
        RenderNode::Page(_) => Ok(RenderResult::Ok(*y)),
        RenderNode::Image { alt_text, .. } => {
            let text = alt_text.as_deref().unwrap_or("[image]");
            layer.use_text(
                text,
                FONT_SIZE,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font,
            );
            Ok(RenderResult::Ok(*y - LINE_HEIGHT))
        }
        RenderNode::Figure { caption, .. } => {
            let text = if caption.is_empty() {
                "[figure]".to_string()
            } else {
                render_nodes_to_text(caption)
            };
            layer.use_text(
                &text,
                FONT_SIZE,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font,
            );
            Ok(RenderResult::Ok(*y - LINE_HEIGHT))
        }
        RenderNode::Unsupported {
            block_type,
            message,
        } => {
            let text = format!("[unsupported {}: {}]", block_type, message);
            layer.use_text(
                &text,
                FONT_SIZE,
                Mm(MARGIN_LEFT * 0.3528),
                Mm(*y * 0.3528),
                font,
            );
            Ok(RenderResult::Ok(*y - LINE_HEIGHT))
        }
    }
}

fn node_to_text(node: &RenderNode) -> String {
    match node {
        RenderNode::Text(t) => t.clone(),
        RenderNode::Formula { latex, .. } => format!("[{}]", latex),
        RenderNode::Paragraph(nodes) => render_nodes_to_text(nodes),
        RenderNode::Heading { nodes, .. } => render_nodes_to_text(nodes),
        RenderNode::Table { rows } => {
            let mut result = String::new();
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| render_nodes_to_text(c)).collect();
                result.push_str(&cells.join(" | "));
                result.push('\n');
            }
            result
        }
        RenderNode::List { items, .. } => items
            .iter()
            .map(|i| render_nodes_to_text(i))
            .collect::<Vec<_>>()
            .join("\n"),
        RenderNode::Code { code, .. } => code.clone(),
        RenderNode::Quote(nodes) => render_nodes_to_text(nodes),
        RenderNode::HorizontalRule => "---".to_string(),
        RenderNode::Page(_) => String::new(),
        RenderNode::Image { alt_text, .. } => {
            alt_text.clone().unwrap_or_else(|| "[image]".to_string())
        }
        RenderNode::Figure { caption, .. } => {
            if caption.is_empty() {
                "[figure]".to_string()
            } else {
                render_nodes_to_text(caption)
            }
        }
        RenderNode::Unsupported {
            block_type,
            message,
        } => {
            format!("[unsupported {}: {}]", block_type, message)
        }
    }
}

fn render_nodes_to_text(nodes: &[RenderNode]) -> String {
    nodes.iter().map(node_to_text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::RenderTree;

    #[test]
    fn pdf_generator_produces_valid_header() {
        let tree = RenderTree {
            nodes: vec![RenderNode::Page(vec![RenderNode::Text(
                "Hello World".into(),
            )])],
        };
        let generator = PdfGenerator;
        let output = generator.generate(&tree).unwrap();
        assert!(output.starts_with("%PDF"));
        assert!(output.ends_with("%%EOF\n") || output.ends_with("%%EOF"));
    }

    #[test]
    fn pdf_generator_metadata() {
        let gen = PdfGenerator;
        assert_eq!(gen.extension(), "pdf");
        assert_eq!(gen.mime_type(), "application/pdf");
        assert_eq!(gen.name(), "pdf");
    }

    #[test]
    fn pdf_generator_handles_all_node_types() {
        let tree = RenderTree {
            nodes: vec![RenderNode::Page(vec![
                RenderNode::Heading {
                    level: 1,
                    nodes: vec![RenderNode::Text("Title".into())],
                },
                RenderNode::Paragraph(vec![RenderNode::Text("Text".into())]),
                RenderNode::Formula {
                    latex: "E=mc^2".into(),
                    display_mode: true,
                },
                RenderNode::Table {
                    rows: vec![vec![
                        vec![RenderNode::Text("A".into())],
                        vec![RenderNode::Text("B".into())],
                    ]],
                },
                RenderNode::List {
                    ordered: false,
                    items: vec![
                        vec![RenderNode::Text("Item 1".into())],
                        vec![RenderNode::Text("Item 2".into())],
                    ],
                },
                RenderNode::Code {
                    language: Some("rust".into()),
                    code: "fn main() {}".into(),
                },
                RenderNode::HorizontalRule,
            ])],
        };
        let generator = PdfGenerator;
        let output = generator.generate(&tree).unwrap();
        assert!(output.starts_with("%PDF"));
    }
}

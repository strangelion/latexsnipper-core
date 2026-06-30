use latexsnipper_foundation::Result;
use crate::generator::Generator;
use crate::render_tree::{RenderTree, RenderNode};

/// Minimal PDF generator — outputs a basic PDF with text content.
/// For production use, consider using printpdf or genpdf crate.
pub struct PdfGenerator;

impl Generator for PdfGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<String> {
        let mut content = String::new();

        for node in &tree.nodes {
            render_node(node, &mut content);
        }

        let pdf = generate_minimal_pdf(&content);
        Ok(pdf)
    }

    fn extension(&self) -> &str { "pdf" }
    fn mime_type(&self) -> &str { "application/pdf" }
    fn name(&self) -> &str { "pdf" }
}

fn render_node(node: &RenderNode, content: &mut String) {
    match node {
        RenderNode::Text(t) => {
            content.push_str(t);
            content.push('\n');
        }
        RenderNode::Formula { latex, .. } => {
            content.push_str(&format!("[Formula: {}]", latex));
            content.push('\n');
        }
        RenderNode::Paragraph(nodes) => {
            for child in nodes {
                render_node(child, content);
            }
            content.push('\n');
        }
        RenderNode::Page(nodes) => {
            content.push_str("--- Page ---\n");
            for child in nodes {
                render_node(child, content);
            }
        }
    }
}

/// Generate a minimal valid PDF file with text content.
fn generate_minimal_pdf(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let mut objects: Vec<String> = Vec::new();
    let mut obj_offsets = Vec::new();

    // Object 1: Catalog
    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string());

    // Object 2: Pages
    objects.push("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string());

    // Object 3: Page
    objects.push("3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n".to_string());

    // Object 4: Content stream
    let mut stream = String::from("BT\n/F1 12 Tf\n1 780 Td\n");
    for line in &lines {
        let escaped = line.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}");
        stream.push_str(&format!("({}) '\n", escaped));
    }
    stream.push_str("ET\n");

    objects.push(format!("4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        stream.len(), stream));

    // Object 5: Font
    objects.push("5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string());

    // Build PDF
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offset = pdf.len();

    for obj in &objects {
        obj_offsets.push(offset);
        pdf.push_str(obj);
        offset = pdf.len();
    }

    // Cross-reference table
    let xref_offset = pdf.len();
    pdf.push_str("xref\n");
    pdf.push_str(&format!("0 {}\n", objects.len() + 1));
    pdf.push_str("0000000000 65535 f \n");
    for off in &obj_offsets {
        pdf.push_str(&format!("{:010} 00000 n \n", off));
    }

    // Trailer
    pdf.push_str("trailer\n");
    pdf.push_str(&format!("<< /Size {} /Root 1 0 R >>\n", objects.len() + 1));
    pdf.push_str("startxref\n");
    pdf.push_str(&format!("{}\n", xref_offset));
    pdf.push_str("%%EOF\n");

    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::{RenderTree, RenderNode};

    #[test]
    fn pdf_generator_produces_valid_header() {
        let tree = RenderTree { nodes: vec![RenderNode::Text("Hello World".into())] };
        let generator = PdfGenerator;
        let output = generator.generate(&tree).unwrap();
        assert!(output.starts_with("%PDF-1.4"));
        assert!(output.contains("Hello World"));
        assert!(output.ends_with("%%EOF\n"));
    }

    #[test]
    fn pdf_generator_metadata() {
        let gen = PdfGenerator;
        assert_eq!(gen.extension(), "pdf");
        assert_eq!(gen.mime_type(), "application/pdf");
        assert_eq!(gen.name(), "pdf");
    }
}

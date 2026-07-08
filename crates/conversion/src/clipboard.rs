use latexsnipper_ast::{Block, Document, Inline};

/// A multi-format clipboard bundle suitable for Office paste operations.
///
/// Contains up to four representations of the same content:
/// - **HTML** (with MathML for Office equation compatibility)
/// - **RTF** (for Word paste)
/// - **Plain text** (universal fallback)
/// - **PNG reference** (visual fallback, requires external rasterization)
#[derive(Debug, Clone)]
pub struct ClipboardBundle {
    pub html: String,
    pub rtf: String,
    pub plain_text: String,
    pub png_base64: Option<String>,
}

impl ClipboardBundle {
    /// Build a clipboard bundle from a Document AST.
    pub fn from_document(doc: &Document) -> Self {
        let plain_text = extract_plain_text(doc);
        let html = build_clipboard_html(doc);
        let rtf = build_rtf_fallback(&plain_text);

        Self {
            html,
            rtf,
            plain_text,
            png_base64: None,
        }
    }

    /// Attach a PNG base64 preview to the bundle.
    pub fn with_png(mut self, png_base64: impl Into<String>) -> Self {
        self.png_base64 = Some(png_base64.into());
        self
    }

    /// Return the bundle as a list of (mime_type, content) pairs,
    /// ordered by preference (HTML first, then RTF, then plain text).
    /// Useful for clipboard APIs that accept multiple formats.
    pub fn to_mime_pairs(&self) -> Vec<(&'static str, &str)> {
        let mut pairs = Vec::with_capacity(4);
        pairs.push(("text/html", self.html.as_str()));
        pairs.push(("text/rtf", self.rtf.as_str()));
        pairs.push(("text/plain", self.plain_text.as_str()));
        if let Some(ref png) = self.png_base64 {
            pairs.push(("image/png", png.as_str()));
        }
        pairs
    }
}

// ── Plain text extraction ──

fn extract_plain_text(doc: &Document) -> String {
    let mut parts = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            let text = block_to_plain_text(block);
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }
    parts.join("\n")
}

fn block_to_plain_text(block: &Block) -> String {
    match block {
        Block::Heading(h) => inlines_to_plain_text(&h.inlines),
        Block::Paragraph(p) => inlines_to_plain_text(&p.inlines),
        Block::Formula(f) => f.formula.as_latex().to_string(),
        Block::Table(t) => {
            let mut rows = Vec::new();
            for row in &t.rows {
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let inlines = cell.collect_inlines();
                        inlines_to_plain_text(&inlines)
                    })
                    .collect();
                rows.push(cells.join("\t"));
            }
            rows.join("\n")
        }
        Block::Figure(f) => f.caption.as_deref().unwrap_or("").to_string(),
        Block::List(l) => l
            .items
            .iter()
            .flat_map(|item| item.content.iter().map(block_to_plain_text))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Quote(q) => q
            .blocks
            .iter()
            .map(block_to_plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Code(c) => c.code.clone(),
        Block::HorizontalRule(_) => "---".to_string(),
        Block::Handwriting(hw) => inlines_to_plain_text(&hw.inlines),
        Block::DescriptionList(dl) => dl
            .items
            .iter()
            .flat_map(|item| {
                let mut parts = Vec::new();
                if let Some(label) = &item.label {
                    parts.push(inlines_to_plain_text(label));
                }
                for b in &item.content {
                    parts.push(block_to_plain_text(b));
                }
                parts
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::TableOfContents => "Table of Contents".to_string(),
        Block::Theorem(t) => {
            let content: Vec<String> = t.content.iter().map(block_to_plain_text).collect();
            format!("{}.\n{}", t.name, content.join("\n"))
        }
        Block::Proof(p) => {
            let content: Vec<String> = p.content.iter().map(block_to_plain_text).collect();
            format!("Proof.\n{}□", content.join("\n"))
        }
        Block::Minipage(m) => m
            .content
            .iter()
            .map(block_to_plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Float(f) => f
            .content
            .iter()
            .map(block_to_plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Block::TextBox(tb) => tb
            .content
            .iter()
            .map(block_to_plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Chart(c) => c
            .title
            .as_ref()
            .map(|t| inlines_to_plain_text(t))
            .unwrap_or_default(),
        Block::Shape(s) => inlines_to_plain_text(&s.text),
        Block::EmbeddedObject(_) => "[embedded object]".to_string(),
        Block::Annotation(a) => a
            .content
            .iter()
            .map(inline_to_plain_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::PageBreak(_) => "[page break]".to_string(),
        Block::SectionBreak(sb) => format!("[section break: {:?}]", sb.kind),
        Block::HeaderFooter(hf) => format!("[header/footer: {:?}]", hf.kind),
    }
}

fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(inline_to_plain_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn inline_to_plain_text(inline: &Inline) -> String {
    match inline {
        Inline::Text(t) => t.text.clone(),
        Inline::Formula(f) => f.as_latex().to_string(),
        Inline::Image(_) => "[image]".to_string(),
        Inline::Footnote { content } => {
            format!("[^{}]", inline_to_plain_text(content))
        }
        Inline::Label { .. } => String::new(),
        Inline::Reference { key, .. } => format!("({})", key),
        Inline::Citation { key, .. } => format!("[{}]", key),
        Inline::LineBreak | Inline::SoftBreak => "\n".to_string(),
        Inline::Span(s) => inlines_to_plain_text(&s.content),
        Inline::Link(l) => {
            let text = inlines_to_plain_text(&l.content);
            format!("{} ({})", text, l.target)
        }
        Inline::Code(c) => c.code.clone(),
        Inline::Superscript(inner) | Inline::Subscript(inner) => inlines_to_plain_text(inner),
    }
}

// ── Clipboard HTML ──

fn build_clipboard_html(doc: &Document) -> String {
    let mut parts = Vec::new();
    parts.push(
        "<html>\
         <head>\
         <meta charset=\"utf-8\">\
         <style>\
         table { border-collapse: collapse; }\
         td, th { border: 1pt solid black; padding: 4pt; }\
         </style>\
         </head>\
         <body>"
            .to_string(),
    );

    for page in &doc.pages {
        for block in &page.blocks {
            if let Some(html) = block_to_clipboard_html(block) {
                parts.push(html);
            }
        }
    }

    parts.push("</body></html>".to_string());
    parts.join("\n")
}

fn block_to_clipboard_html(block: &Block) -> Option<String> {
    match block {
        Block::Heading(h) => {
            let tag = format!("h{}", h.level);
            let text = inlines_to_clipboard_html(&h.inlines);
            Some(format!("<{}>{}</{}>", tag, text, tag))
        }
        Block::Paragraph(p) => {
            let text = inlines_to_clipboard_html(&p.inlines);
            if text.is_empty() {
                None
            } else {
                Some(format!("<p>{}</p>", text))
            }
        }
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            // Use MathML for Office equation compatibility
            if f.formula.display_mode {
                Some(format!(
                    "<p style=\"text-align:center\"><math><mi>{}</mi></math></p>",
                    html_escape(latex)
                ))
            } else {
                Some(format!(
                    "<span><math><mi>{}</mi></math></span>",
                    html_escape(latex)
                ))
            }
        }
        Block::Table(t) => {
            if t.rows.is_empty() {
                return None;
            }
            let mut rows = Vec::new();
            for row in &t.rows {
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let inlines = cell.collect_inlines();
                        let content = inlines_to_clipboard_html(&inlines);
                        format!("<td>{}</td>", content)
                    })
                    .collect();
                rows.push(format!("<tr>{}</tr>", cells.join("")));
            }
            Some(format!("<table>{}</table>", rows.join("")))
        }
        Block::Figure(f) => {
            let caption = f.caption.as_deref().unwrap_or("");
            Some(format!(
                "<figure><figcaption>{}</figcaption></figure>",
                html_escape(caption)
            ))
        }
        Block::List(l) => {
            let tag = if l.is_ordered() { "ol" } else { "ul" };
            let items: Vec<String> = l
                .items
                .iter()
                .map(|item| {
                    let inner = item
                        .content
                        .iter()
                        .filter_map(block_to_clipboard_html)
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("<li>{}</li>", inner)
                })
                .collect();
            Some(format!("<{}>{}</{}>", tag, items.join(""), tag))
        }
        Block::Code(c) => Some(format!("<pre><code>{}</code></pre>", html_escape(&c.code))),
        Block::Quote(q) => {
            let content: Vec<String> = q
                .blocks
                .iter()
                .filter_map(block_to_clipboard_html)
                .collect();
            if content.is_empty() {
                None
            } else {
                Some(format!("<blockquote>{}</blockquote>", content.join("\n")))
            }
        }
        Block::HorizontalRule(_) => Some("<hr>".to_string()),
        Block::Handwriting(hw) => {
            let text = inlines_to_clipboard_html(&hw.inlines);
            Some(format!("<i>{}</i>", text))
        }
        _ => None,
    }
}

fn inlines_to_clipboard_html(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t) => {
                let mut text = html_escape(&t.text);
                if t.bold == Some(true) {
                    text = format!("<b>{}</b>", text);
                }
                if t.italic == Some(true) {
                    text = format!("<i>{}</i>", text);
                }
                if t.underline == Some(true) {
                    text = format!("<u>{}</u>", text);
                }
                text
            }
            Inline::Formula(f) => {
                let latex = f.as_latex();
                format!("<span><math><mi>{}</mi></math></span>", html_escape(latex))
            }
            Inline::Image(_) => "[image]".to_string(),
            Inline::LineBreak => "<br>".to_string(),
            Inline::SoftBreak => "\n".to_string(),
            Inline::Span(s) => inlines_to_clipboard_html(&s.content),
            Inline::Link(l) => {
                let text = inlines_to_clipboard_html(&l.content);
                format!("<a href=\"{}\">{}</a>", html_escape(&l.target), text)
            }
            Inline::Code(c) => format!("<code>{}</code>", html_escape(&c.code)),
            Inline::Superscript(inner) => {
                format!("<sup>{}</sup>", inlines_to_clipboard_html(inner))
            }
            Inline::Subscript(inner) => {
                format!("<sub>{}</sub>", inlines_to_clipboard_html(inner))
            }
            _ => String::new(),
        })
        .collect()
}

// ── RTF fallback (simplified) ──

fn build_rtf_fallback(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\n', "\\line\n");
    format!(
        r"{{\rtf1\ansi\deff0 {{\fonttbl {{\f0 Times New Roman;}}}}
{{\colortbl ;\red0\green0\blue0;}}
\f0\fs24 {}
}}",
        escaped
    )
}

// ── Utilities ──

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

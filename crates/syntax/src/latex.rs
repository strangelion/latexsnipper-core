use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, SourceInfo, Span, TextRun,
};
use latexsnipper_foundation::Result;

use crate::parser::Parser;
use crate::renderer::Renderer;
use crate::{ParsedDocument, SourceMap};

/// LaTeX parser — converts LaTeX string to Document AST.
pub struct LatexParser;

impl LatexParser {
    /// Parse LaTeX while preserving source spans and parser-local provisional IDs.
    ///
    /// This additive API intentionally leaves the `Parser` trait unchanged.
    pub fn parse_with_source_map(&self, input: &str) -> Result<ParsedDocument> {
        parse_latex_with_source_map(input)
    }
}

/// Parse LaTeX content and retain a byte-accurate source map.
///
/// The `latex:<kind>:<index>` values placed in `SourceInfo.stable_id` here are
/// parser-local provisional identities. Stateful callers must reconcile them
/// into their own persistent identities before exposing a session API.
pub fn parse_latex_with_source_map(input: &str) -> Result<ParsedDocument> {
    let (blocks, source_map) = parse_latex_content_with_source_map(input);
    Ok(ParsedDocument {
        document: Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks,
                page_number: None,
                layout: None,
                background_asset_id: None,
            }],
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        },
        source_map,
    })
}

impl Parser for LatexParser {
    fn parse(&self, input: &str) -> Result<Document> {
        Ok(parse_latex_with_source_map(input)?.document)
    }

    fn name(&self) -> &str {
        "latex"
    }
}

/// LaTeX renderer — converts Document AST to LaTeX string.
pub struct LatexRenderer;

impl Renderer for LatexRenderer {
    fn render(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        let latex = f.formula.as_latex();
                        if f.formula.display_mode {
                            parts.push(format!("$$\n{}\n$$", latex));
                        } else {
                            parts.push(format!("${}$", latex));
                        }
                    }
                    Block::Paragraph(p) => {
                        let text: String = p
                            .inlines
                            .iter()
                            .map(|i| match i {
                                Inline::Text(t) => t.text.clone(),
                                Inline::Formula(f) => {
                                    if f.display_mode {
                                        format!("$$\n{}\n$$", f.as_latex())
                                    } else {
                                        format!("${}$", f.as_latex())
                                    }
                                }
                                _ => String::new(),
                            })
                            .collect();
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(parts.join("\n\n"))
    }

    fn name(&self) -> &str {
        "latex"
    }
}

/// Parse LaTeX content into blocks while retaining source information.
fn parse_latex_content_with_source_map(input: &str) -> (Vec<Block>, SourceMap) {
    let mut blocks = Vec::new();
    let mut source_map = SourceMap::new();
    let mut cursor = 0;
    let mut block_index = 0;

    while cursor < input.len() {
        let remaining = &input[cursor..];
        // Check the first delimiter before searching for a matching display
        // delimiter. Searching the entire remaining source for `$$` on every
        // inline formula would make large fragment parses quadratic.
        if let Some(start) = remaining.find('$') {
            if start + 1 < remaining.len() && remaining.as_bytes()[start + 1] == b'$' {
                let after_start = &remaining[start + 2..];
                if let Some(end) = after_start.find("$$") {
                    let formula_start = cursor + start + 2;
                    let formula_end = formula_start + end;
                    let formula = after_start[..end].trim().to_string();
                    // Add any text before the formula
                    if let Some((start, end)) = trim_byte_range(input, cursor, cursor + start) {
                        let stable_id = next_provisional_id(&mut block_index, "paragraph");
                        let source = SourceInfo::new()
                            .with_stable_id(stable_id.clone())
                            .with_span(Span::new(start, end));
                        source_map.insert(stable_id, Span::new(start, end));
                        blocks.push(Block::Paragraph(ParagraphBlock {
                            inlines: vec![Inline::Text(TextRun::new(&input[start..end]))],
                            geometry: None,
                            source: Some(source),
                            style: None,
                        }));
                    }
                    let stable_id = next_provisional_id(&mut block_index, "formula");
                    let span = Span::new(cursor + start, formula_end + 2);
                    let source = SourceInfo::new()
                        .with_stable_id(stable_id.clone())
                        .with_span(span);
                    source_map.insert(stable_id, span);
                    blocks.push(Block::Formula(FormulaBlock {
                        formula: Formula::latex(formula).with_source_info(source.clone()),
                        label: None,
                        number: None,
                        environment: None,
                        geometry: None,
                        source: Some(source),
                    }));
                    cursor = formula_end + 2;
                    continue;
                } else {
                    // This lightweight fragment parser must never advance past
                    // an unmatched delimiter and silently lose source text.
                    if let Some((start, end)) = trim_byte_range(input, cursor, input.len()) {
                        let stable_id = next_provisional_id(&mut block_index, "paragraph");
                        let source = SourceInfo::new()
                            .with_stable_id(stable_id.clone())
                            .with_span(Span::new(start, end));
                        source_map.insert(stable_id, Span::new(start, end));
                        blocks.push(Block::Paragraph(ParagraphBlock {
                            inlines: vec![Inline::Text(TextRun::new(&input[start..end]))],
                            geometry: None,
                            source: Some(source),
                            style: None,
                        }));
                    }
                    break;
                }
            }
        }

        // Try to find inline math $...$
        if let Some(start) = remaining.find('$') {
            // Make sure it's not $$
            if start + 1 < remaining.len() && remaining.as_bytes()[start + 1] == b'$' {
                // Skip $$, will be handled above
                cursor += start + 2;
                continue;
            }
            let after_start = &remaining[start + 1..];
            if let Some(end) = after_start.find('$') {
                let formula_start = cursor + start + 1;
                let formula_end = formula_start + end;
                let formula = after_start[..end].trim().to_string();
                if let Some((start, end)) = trim_byte_range(input, cursor, cursor + start) {
                    let stable_id = next_provisional_id(&mut block_index, "paragraph");
                    let source = SourceInfo::new()
                        .with_stable_id(stable_id.clone())
                        .with_span(Span::new(start, end));
                    source_map.insert(stable_id, Span::new(start, end));
                    blocks.push(Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new(&input[start..end]))],
                        geometry: None,
                        source: Some(source),
                        style: None,
                    }));
                }
                let stable_id = next_provisional_id(&mut block_index, "formula");
                let span = Span::new(cursor + start, formula_end + 1);
                let source = SourceInfo::new()
                    .with_stable_id(stable_id.clone())
                    .with_span(span);
                source_map.insert(stable_id, span);
                blocks.push(Block::Formula(FormulaBlock {
                    formula: {
                        let mut f = Formula::latex(formula).with_source_info(source.clone());
                        f.display_mode = false;
                        f
                    },
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: Some(source),
                }));
                cursor = formula_end + 1;
                continue;
            }
        }

        // No more math, treat rest as text
        if let Some((start, end)) = trim_byte_range(input, cursor, input.len()) {
            let stable_id = next_provisional_id(&mut block_index, "paragraph");
            let source = SourceInfo::new()
                .with_stable_id(stable_id.clone())
                .with_span(Span::new(start, end));
            source_map.insert(stable_id, Span::new(start, end));
            blocks.push(Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new(&input[start..end]))],
                geometry: None,
                source: Some(source),
                style: None,
            }));
        }
        break;
    }

    (blocks, source_map)
}

fn next_provisional_id(block_index: &mut usize, kind: &str) -> String {
    let id = format!("latex:{kind}:{}", *block_index);
    *block_index += 1;
    id
}

fn trim_byte_range(input: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let segment = &input[start..end];
    let leading = segment.len() - segment.trim_start().len();
    let trimmed_end = start + segment.trim_end().len();
    let trimmed_start = start + leading;
    (trimmed_start < trimmed_end).then_some((trimmed_start, trimmed_end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_aware_parser_preserves_inline_and_display_formula_spans() {
        let input = "前言 $x^2$ 后文\n\n$$\ny\n$$";
        let parsed = parse_latex_with_source_map(input).unwrap();

        let inline_start = input.find("$x^2$").unwrap();
        let display_start = input.find("$$\ny\n$$").unwrap();
        assert_eq!(
            parsed.source_map.span_for("latex:formula:1"),
            Some(Span::new(inline_start, inline_start + "$x^2$".len()))
        );
        assert_eq!(
            parsed.source_map.span_for("latex:formula:3"),
            Some(Span::new(display_start, input.len()))
        );
        assert_eq!(parsed.document.pages[0].blocks.len(), 4);
    }

    #[test]
    fn malformed_display_delimiter_is_preserved_as_text() {
        let input = "abc $$ x";
        let parsed = parse_latex_with_source_map(input).unwrap();
        assert_eq!(parsed.document.pages[0].blocks.len(), 1);
        assert_eq!(
            parsed.source_map.span_for("latex:paragraph:0"),
            Some(Span::new(0, input.len()))
        );
    }
}

use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, FormulaSource, Inline, Metadata, NodeIdGenerator, Page,
    ParagraphBlock, TextRun,
};

/// Parse a Markdown string into a Document AST.
/// `$$...$$` → display formula block, `$...$` → inline formula within paragraph.
pub fn parse_markdown_to_document(md: &str) -> Document {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current_inlines: Vec<Inline> = Vec::new();

    let chars: Vec<char> = md.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '$' {
            if i + 1 < len && chars[i + 1] == '$' {
                if let Some(end) = find_display_math_end(&chars, i + 2) {
                    let formula_str: String = chars[i + 2..end].iter().collect();
                    let formula_str = formula_str.trim();
                    blocks.push(Block::Formula(FormulaBlock {
                        formula: Formula::latex(formula_str),
                        geometry: None,
                        source: None,
                    }));
                    i = end + 2;
                    continue;
                }
            }

            if let Some(end) = find_inline_math_end(&chars, i + 1) {
                let formula_str: String = chars[i + 1..end].iter().collect();
                let formula_str = formula_str.trim();
                let mut f = Formula::latex(formula_str);
                f.display_mode = false;
                current_inlines.push(Inline::Formula(f));
                i = end + 1;
                continue;
            }

            push_text(&mut current_inlines, "$");
            i += 1;
        } else if chars[i] == '\n' && i + 1 < len && chars[i + 1] == '\n' {
            flush_paragraph(&mut blocks, &mut current_inlines);
            i += 2;
        } else if chars[i] == '#' {
            let (level, title, consumed) = parse_heading(&chars, i);
            if level > 0 {
                flush_paragraph(&mut blocks, &mut current_inlines);
                blocks.push(Block::Heading(latexsnipper_ast::HeadingBlock {
                    level,
                    inlines: vec![Inline::Text(TextRun::new(title))],
                    id: None,
                    geometry: None,
                    source: None,
                }));
                i = consumed;
                continue;
            }
            push_char(&mut current_inlines, chars[i]);
            i += 1;
        } else {
            push_char(&mut current_inlines, chars[i]);
            i += 1;
        }
    }

    flush_paragraph(&mut blocks, &mut current_inlines);

    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            blocks,
            page_number: None,
        }],
        id_gen: NodeIdGenerator::new(),
    }
}

fn flush_paragraph(blocks: &mut Vec<Block>, inlines: &mut Vec<Inline>) {
    let trimmed: Vec<Inline> = inlines
        .iter()
        .filter(|i| {
            if let Inline::Text(t) = i {
                !t.text.trim().is_empty()
            } else {
                true
            }
        })
        .cloned()
        .collect();
    if !trimmed.is_empty() {
        blocks.push(Block::Paragraph(ParagraphBlock {
            inlines: trimmed,
            geometry: None,
            source: None,
        }));
    }
    inlines.clear();
}

fn push_text(inlines: &mut Vec<Inline>, text: &str) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        t.text.push_str(text);
    } else {
        inlines.push(Inline::Text(TextRun::new(text)));
    }
}

fn push_char(inlines: &mut Vec<Inline>, ch: char) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        t.text.push(ch);
    } else {
        inlines.push(Inline::Text(TextRun::new(ch.to_string())));
    }
}

fn find_display_math_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '$' && chars[i + 1] == '$' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_inline_math_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '$' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_heading(chars: &[char], start: usize) -> (u8, String, usize) {
    let mut level: u8 = 0;
    let mut i = start;
    while i < chars.len() && chars[i] == '#' {
        level += 1;
        i += 1;
    }
    if level > 6 || i >= chars.len() || chars[i] != ' ' {
        return (0, String::new(), start);
    }
    i += 1;
    let mut title = String::new();
    while i < chars.len() && chars[i] != '\n' {
        title.push(chars[i]);
        i += 1;
    }
    (level, title.trim().to_string(), i)
}

/// Parse a Markdown string to LaTeX string (text + inline math).
pub fn parse_markdown_to_latex(md: &str) -> Result<String, String> {
    let doc = parse_markdown_to_document(md);
    let mut result = String::new();
    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Paragraph(p) => {
                    for inline in &p.inlines {
                        match inline {
                            Inline::Text(t) => result.push_str(&t.text),
                            Inline::Formula(f) => {
                                if let FormulaSource::Latex(s) = &f.source {
                                    if f.display_mode {
                                        result.push_str(&format!("$$ {} $$", s));
                                    } else {
                                        result.push_str(&format!("${}$", s));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    result.push('\n');
                }
                Block::Heading(h) => {
                    let prefix = "#".repeat(h.level as usize);
                    let title: String = h
                        .inlines
                        .iter()
                        .filter_map(|i| {
                            if let Inline::Text(t) = i {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect();
                    result.push_str(&format!("{} {}\n", prefix, title));
                }
                Block::Formula(f) => {
                    if let FormulaSource::Latex(s) = &f.formula.source {
                        result.push_str(&format!("$$ {} $$\n", s));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_math() {
        let md = "$$ E = mc^2 $$";
        let doc = parse_markdown_to_document(md);
        assert_eq!(doc.pages[0].blocks.len(), 1);
        if let Block::Formula(f) = &doc.pages[0].blocks[0] {
            assert!(f.formula.display_mode);
            if let FormulaSource::Latex(s) = &f.formula.source {
                assert_eq!(s, "E = mc^2");
            }
        } else {
            panic!("Expected formula block");
        }
    }

    #[test]
    fn inline_math() {
        let md = "The equation $x^2$ is important.";
        let doc = parse_markdown_to_document(md);
        assert_eq!(doc.pages[0].blocks.len(), 1);
        if let Block::Paragraph(p) = &doc.pages[0].blocks[0] {
            assert_eq!(p.inlines.len(), 3);
        } else {
            panic!("Expected paragraph block");
        }
    }

    #[test]
    fn heading() {
        let md = "# Title\n\nSome text.";
        let doc = parse_markdown_to_document(md);
        assert_eq!(doc.pages[0].blocks.len(), 2);
        if let Block::Heading(h) = &doc.pages[0].blocks[0] {
            assert_eq!(h.level, 1);
        }
    }

    #[test]
    fn mixed() {
        let md = "# Math\n\n$$ \\frac{a}{b} $$\n\nText with $x_i$ inline.";
        let doc = parse_markdown_to_document(md);
        assert!(doc.pages[0].blocks.len() >= 3);
    }
}

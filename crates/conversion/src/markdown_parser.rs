use latexsnipper_ast::{
    Block, CodeBlock, Document, Formula, FormulaBlock, FormulaSource, Inline, ListBlock, ListItem,
    Metadata, NodeIdGenerator, Page, ParagraphBlock, QuoteBlock, TextRun,
};

/// Parse a Markdown string into a Document AST.
/// Supports: headings, paragraphs, display/inline math, bold, italic, code, lists, blockquotes, horizontal rules.
pub fn parse_markdown_to_document(md: &str) -> Document {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current_inlines: Vec<Inline> = Vec::new();
    let mut blockquote_lines: Vec<String> = Vec::new();

    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Display math: $$...$$
        if line.trim().starts_with("$$") && line.trim().ends_with("$$") && line.trim().len() > 4 {
            flush_paragraph(&mut blocks, &mut current_inlines);
            let formula_str = line.trim()[2..line.trim().len() - 2].trim();
            blocks.push(Block::Formula(FormulaBlock {
                formula: Formula::latex(formula_str),
                geometry: None,
                source: None,
            }));
            i += 1;
            continue;
        }

        // Display math block: $$ on its own line
        if line.trim() == "$$" {
            flush_paragraph(&mut blocks, &mut current_inlines);
            i += 1;
            let mut formula_lines = Vec::new();
            while i < lines.len() && lines[i].trim() != "$$" {
                formula_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing $$
            }
            let formula_str = formula_lines.join("\n").trim().to_string();
            blocks.push(Block::Formula(FormulaBlock {
                formula: Formula::latex(&formula_str),
                geometry: None,
                source: None,
            }));
            continue;
        }

        // Horizontal rule
        if is_horizontal_rule(line) {
            flush_paragraph(&mut blocks, &mut current_inlines);
            blocks.push(Block::HorizontalRule(
                latexsnipper_ast::HorizontalRuleBlock::new(),
            ));
            i += 1;
            continue;
        }

        // Heading
        if let Some((level, title)) = parse_heading_line(line) {
            flush_paragraph(&mut blocks, &mut current_inlines);
            blocks.push(Block::Heading(latexsnipper_ast::HeadingBlock {
                level,
                inlines: vec![Inline::Text(TextRun::new(title))],
                id: None,
                geometry: None,
                source: None,
            }));
            i += 1;
            continue;
        }

        // Code block
        if line.trim_start().starts_with("```") {
            flush_paragraph(&mut blocks, &mut current_inlines);
            let lang = line.trim_start().trim_start_matches('`').trim().to_string();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            blocks.push(Block::Code(CodeBlock {
                language: if lang.is_empty() { None } else { Some(lang) },
                code: code_lines.join("\n"),
                geometry: None,
                source: None,
            }));
            continue;
        }

        // Blockquote
        if line.trim_start().starts_with('>') {
            flush_paragraph(&mut blocks, &mut current_inlines);
            let content = line.trim_start().trim_start_matches('>').trim();
            blockquote_lines.push(content.to_string());
            i += 1;
            // Check if next line is still blockquote
            if i < lines.len() && lines[i].trim_start().starts_with('>') {
                continue;
            } else {
                // Flush blockquote
                let quote_text = blockquote_lines.join("\n");
                let quote_doc = parse_markdown_to_document(&quote_text);
                let quote_blocks: Vec<Block> =
                    quote_doc.pages.into_iter().flat_map(|p| p.blocks).collect();
                blocks.push(Block::Quote(QuoteBlock {
                    blocks: quote_blocks,
                    attribution: None,
                    geometry: None,
                    source: None,
                }));
                blockquote_lines.clear();
                continue;
            }
        }

        // List item
        if let Some((ordered, item_text)) = parse_list_item(line) {
            flush_paragraph(&mut blocks, &mut current_inlines);
            let mut items = vec![ListItem {
                inlines: parse_inline_text(item_text),
                checked: None,
                source: None,
            }];
            i += 1;
            // Collect consecutive list items
            while i < lines.len() {
                if let Some((o, t)) = parse_list_item(lines[i]) {
                    if o == ordered {
                        items.push(ListItem {
                            inlines: parse_inline_text(t),
                            checked: None,
                            source: None,
                        });
                        i += 1;
                        continue;
                    }
                }
                break;
            }
            blocks.push(Block::List(ListBlock {
                ordered,
                items,
                geometry: None,
                source: None,
            }));
            continue;
        }

        // Empty line - flush paragraph
        if line.trim().is_empty() {
            flush_paragraph(&mut blocks, &mut current_inlines);
            i += 1;
            continue;
        }

        // Regular text - parse inline formatting
        let inlines = parse_inline_line(line);
        current_inlines.extend(inlines);
        i += 1;
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
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
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
            style: None,
        }));
    }
    inlines.clear();
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

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let ch = chars[0];
    if ch != '-' && ch != '*' && ch != '_' {
        return false;
    }
    chars.iter().all(|&c| c == ch || c == ' ')
}

fn parse_heading_line(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim();
    let mut level: u8 = 0;
    for ch in trimmed.chars() {
        if ch == '#' {
            level += 1;
        } else {
            break;
        }
    }
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed[level as usize..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((level, rest.to_string()))
}

fn parse_list_item(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim();
    // Ordered list: "1. ", "2. ", etc.
    if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        if let Some(rest) = rest.strip_prefix('.') {
            if let Some(rest) = rest.strip_prefix(' ') {
                return Some((true, rest));
            }
        }
    }
    // Unordered list: "- ", "* ", "+ "
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return Some((false, rest));
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return Some((false, rest));
    }
    if let Some(rest) = trimmed.strip_prefix("+ ") {
        return Some((false, rest));
    }
    None
}

fn parse_inline_text(text: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inline math
        if chars[i] == '$' {
            if let Some(end) = find_inline_math_end(&chars, i + 1) {
                let formula_str: String = chars[i + 1..end].iter().collect();
                let formula_str = formula_str.trim();
                let mut f = Formula::latex(formula_str);
                f.display_mode = false;
                inlines.push(Inline::Formula(f));
                i = end + 1;
                continue;
            }
        }

        // Bold: **text** or __text__
        if i + 1 < len && (chars[i] == '*' && chars[i + 1] == '*')
            || (chars[i] == '_' && chars[i + 1] == '_')
        {
            let marker = chars[i];
            if let Some(end) = find_double_marker_end(&chars, i + 2, marker) {
                let inner: String = chars[i + 2..end].iter().collect();
                let inner_inlines = parse_inline_text(&inner);
                for mut inline in inner_inlines {
                    if let Inline::Text(t) = &mut inline {
                        t.bold = Some(true);
                    }
                    inlines.push(inline);
                }
                i = end + 2;
                continue;
            }
        }

        // Italic: *text* or _text_
        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(end) = find_single_marker_end(&chars, i + 1, marker) {
                let inner: String = chars[i + 1..end].iter().collect();
                let inner_inlines = parse_inline_text(&inner);
                for mut inline in inner_inlines {
                    if let Inline::Text(t) = &mut inline {
                        t.italic = Some(true);
                    }
                    inlines.push(inline);
                }
                i = end + 1;
                continue;
            }
        }

        // Inline code: `code`
        if chars[i] == '`' {
            if let Some(end) = find_inline_code_end(&chars, i + 1) {
                let code: String = chars[i + 1..end].iter().collect();
                inlines.push(Inline::Text(TextRun {
                    text: code,
                    style: None,
                    bold: None,
                    italic: None,
                    underline: None,
                    strikethrough: None,
                    source: None,
                }));
                i = end + 1;
                continue;
            }
        }

        // Regular character
        if let Some(Inline::Text(t)) = inlines.last_mut() {
            t.text.push(chars[i]);
        } else {
            inlines.push(Inline::Text(TextRun::new(chars[i].to_string())));
        }
        i += 1;
    }

    inlines
}

fn parse_inline_line(line: &str) -> Vec<Inline> {
    parse_inline_text(line)
}

fn find_double_marker_end(chars: &[char], start: usize, marker: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == marker && chars[i + 1] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single_marker_end(chars: &[char], start: usize, marker: char) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_inline_code_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '`' {
            return Some(i);
        }
        i += 1;
    }
    None
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

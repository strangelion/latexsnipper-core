use latexsnipper_ast::{
    Block, BulletStyle, CodeBlock, Document, Formula, FormulaBlock, Inline, ListBlock, ListItem,
    ListStyle, Metadata, NodeIdGenerator, NumberingStyle, Page, ParagraphBlock, QuoteBlock,
    TextRun,
};

/// Parsed HTML tag: (tag_name, attributes, self_closing, end_position)
type ParsedTag = (String, Vec<(String, String)>, bool, usize);

/// Parse an HTML string into a Document AST.
/// Supports: headings, paragraphs, bold, italic, code, lists, blockquotes, horizontal rules, math.
pub fn parse_html_to_document(html: &str) -> Document {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current_inlines: Vec<Inline> = Vec::new();

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // HTML comment
        if i + 3 < len
            && chars[i] == '<'
            && chars[i + 1] == '!'
            && chars[i + 2] == '-'
            && chars[i + 3] == '-'
        {
            if let Some(end) = find_comment_end(&chars, i + 4) {
                i = end;
                continue;
            }
        }

        // HTML tag
        if chars[i] == '<' {
            if let Some((tag, attrs, self_closing, end)) = parse_tag(&chars, i) {
                match tag.as_str() {
                    // Headings
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let level = tag.chars().last().unwrap().to_digit(10).unwrap_or(1) as u8;
                        let content = extract_tag_content(&chars, end, &tag);
                        let content = content.trim();
                        blocks.push(Block::Heading(latexsnipper_ast::HeadingBlock {
                            level,
                            inlines: vec![Inline::Text(TextRun::new(content.to_string()))],
                            id: get_attr(&attrs, "id"),
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    // Paragraph
                    "p" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, &tag);
                        let inlines = parse_inline_html(&content);
                        if !inlines.is_empty() {
                            blocks.push(Block::Paragraph(ParagraphBlock {
                                inlines,
                                geometry: None,
                                source: None,
                                style: None,
                            }));
                        }
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    // Preformatted code
                    "pre" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, "pre");
                        // Extract language from child <code> tag if present
                        let lang = if let Some(code_start) = content.find("<code") {
                            if let Some(class_start) = content[code_start..].find("class=\"") {
                                let class = &content[code_start + class_start + 7..];
                                if let Some(class_end) = class.find('"') {
                                    let class = &class[..class_end];
                                    class.strip_prefix("language-").map(|s| s.to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let code = strip_html_tags(&content);
                        blocks.push(Block::Code(CodeBlock {
                            language: lang,
                            code,
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "pre").unwrap_or(len);
                        continue;
                    }
                    // Inline code
                    "code" => {
                        let content = extract_tag_content(&chars, end, "code");
                        let code = content.trim().to_string();
                        current_inlines.push(Inline::Text(TextRun {
                            text: code,
                            style: None,
                            bold: None,
                            italic: None,
                            underline: None,
                            strikethrough: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "code").unwrap_or(len);
                        continue;
                    }
                    // Bold
                    "strong" | "b" => {
                        let content = extract_tag_content(&chars, end, &tag);
                        let inlines = parse_inline_html(&content);
                        for mut inline in inlines {
                            if let Inline::Text(t) = &mut inline {
                                t.bold = Some(true);
                            }
                            current_inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    // Italic
                    "em" | "i" => {
                        let content = extract_tag_content(&chars, end, &tag);
                        let inlines = parse_inline_html(&content);
                        for mut inline in inlines {
                            if let Inline::Text(t) = &mut inline {
                                t.italic = Some(true);
                            }
                            current_inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    // Underline
                    "u" => {
                        let content = extract_tag_content(&chars, end, "u");
                        let inlines = parse_inline_html(&content);
                        for mut inline in inlines {
                            if let Inline::Text(t) = &mut inline {
                                t.underline = Some(true);
                            }
                            current_inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, "u").unwrap_or(len);
                        continue;
                    }
                    // Blockquote
                    "blockquote" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, "blockquote");
                        let quote_doc = parse_html_to_document(&content);
                        let quote_blocks: Vec<Block> =
                            quote_doc.pages.into_iter().flat_map(|p| p.blocks).collect();
                        blocks.push(Block::Quote(QuoteBlock {
                            blocks: quote_blocks,
                            attribution: None,
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "blockquote").unwrap_or(len);
                        continue;
                    }
                    // Unordered list
                    "ul" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, "ul");
                        let items = parse_list_items(&content, false);
                        blocks.push(Block::List(ListBlock {
                            style: Some(ListStyle::Bullet(BulletStyle::Disc)),
                            start: None,
                            items,
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "ul").unwrap_or(len);
                        continue;
                    }
                    // Ordered list
                    "ol" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, "ol");
                        let items = parse_list_items(&content, true);
                        blocks.push(Block::List(ListBlock {
                            style: Some(ListStyle::Ordered(NumberingStyle::Decimal)),
                            start: None,
                            items,
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "ol").unwrap_or(len);
                        continue;
                    }
                    // Horizontal rule
                    "hr" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        blocks.push(Block::HorizontalRule(
                            latexsnipper_ast::HorizontalRuleBlock::new(),
                        ));
                        i = if self_closing {
                            end
                        } else {
                            find_closing_tag(&chars, end, "hr").unwrap_or(len)
                        };
                        continue;
                    }
                    // Math (MathJax)
                    "math" => {
                        flush_paragraph(&mut blocks, &mut current_inlines);
                        let content = extract_tag_content(&chars, end, "math");
                        let formula_str = content.trim().to_string();
                        blocks.push(Block::Formula(FormulaBlock {
                            formula: Formula::latex(&formula_str),
                            geometry: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "math").unwrap_or(len);
                        continue;
                    }
                    // Skip other tags
                    _ => {
                        i = end;
                        continue;
                    }
                }
            }
        }

        // Regular character
        push_char(&mut current_inlines, chars[i]);
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

fn push_char(inlines: &mut Vec<Inline>, ch: char) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        t.text.push(ch);
    } else {
        inlines.push(Inline::Text(TextRun::new(ch.to_string())));
    }
}

fn find_comment_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < chars.len() {
        if chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>' {
            return Some(i + 3);
        }
        i += 1;
    }
    None
}

fn parse_tag(chars: &[char], start: usize) -> Option<ParsedTag> {
    if chars[start] != '<' {
        return None;
    }
    let mut i = start + 1;
    let mut tag = String::new();
    while i < chars.len() && chars[i].is_alphanumeric() {
        tag.push(chars[i]);
        i += 1;
    }
    if tag.is_empty() {
        return None;
    }

    let mut attrs = Vec::new();
    let mut self_closing = false;

    // Parse attributes
    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        if i >= chars.len() {
            break;
        }

        // Check for self-closing or end of tag
        if chars[i] == '/' {
            self_closing = true;
            i += 1;
            if i < chars.len() && chars[i] == '>' {
                i += 1;
            }
            break;
        }
        if chars[i] == '>' {
            i += 1;
            break;
        }

        // Parse attribute name
        let mut attr_name = String::new();
        while i < chars.len()
            && chars[i] != '='
            && chars[i] != '>'
            && chars[i] != '/'
            && !chars[i].is_whitespace()
        {
            attr_name.push(chars[i]);
            i += 1;
        }

        if attr_name.is_empty() {
            i += 1;
            continue;
        }

        // Parse attribute value
        let mut attr_value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let quote = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    attr_value.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // skip closing quote
                }
            } else {
                while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '>' {
                    attr_value.push(chars[i]);
                    i += 1;
                }
            }
        }

        attrs.push((attr_name, attr_value));
    }

    Some((tag, attrs, self_closing, i))
}

fn get_attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
}

fn extract_tag_content(chars: &[char], start: usize, tag: &str) -> String {
    let end = find_closing_tag(chars, start, tag).unwrap_or(chars.len());
    chars[start..end].iter().collect()
}

fn find_closing_tag(chars: &[char], start: usize, tag: &str) -> Option<usize> {
    let mut i = start;
    let closing = format!("</{}>", tag);
    let closing_chars: Vec<char> = closing.chars().collect();

    while i + closing_chars.len() <= chars.len() {
        if chars[i..i + closing_chars.len()] == closing_chars[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '<' {
            // Skip tag
            while i < len && chars[i] != '>' {
                i += 1;
            }
            if i < len {
                i += 1; // skip '>'
            }
        } else if chars[i] == '&' {
            // HTML entity
            let mut entity = String::new();
            entity.push(chars[i]);
            i += 1;
            while i < len && chars[i] != ';' && entity.len() < 10 {
                entity.push(chars[i]);
                i += 1;
            }
            if i < len {
                entity.push(chars[i]);
                i += 1;
            }
            match entity.as_str() {
                "&amp;" => result.push('&'),
                "&lt;" => result.push('<'),
                "&gt;" => result.push('>'),
                "&quot;" => result.push('"'),
                "&apos;" => result.push('\''),
                "&nbsp;" => result.push(' '),
                _ => result.push_str(&entity),
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn parse_inline_html(html: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // HTML entity
        if chars[i] == '&' {
            let mut entity = String::new();
            entity.push(chars[i]);
            i += 1;
            while i < len && chars[i] != ';' && entity.len() < 10 {
                entity.push(chars[i]);
                i += 1;
            }
            if i < len {
                entity.push(chars[i]);
                i += 1;
            }
            let ch = match entity.as_str() {
                "&amp;" => '&',
                "&lt;" => '<',
                "&gt;" => '>',
                "&quot;" => '"',
                "&apos;" => '\'',
                "&nbsp;" => ' ',
                _ => '?',
            };
            if let Some(Inline::Text(t)) = inlines.last_mut() {
                t.text.push(ch);
            } else {
                inlines.push(Inline::Text(TextRun::new(ch.to_string())));
            }
            continue;
        }

        // HTML tag
        if chars[i] == '<' {
            if let Some((tag, _, _, end)) = parse_tag(&chars, i) {
                match tag.as_str() {
                    "strong" | "b" => {
                        let content = extract_tag_content(&chars, end, &tag);
                        let inner = parse_inline_html(&content);
                        for mut inline in inner {
                            if let Inline::Text(t) = &mut inline {
                                t.bold = Some(true);
                            }
                            inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    "em" | "i" => {
                        let content = extract_tag_content(&chars, end, &tag);
                        let inner = parse_inline_html(&content);
                        for mut inline in inner {
                            if let Inline::Text(t) = &mut inline {
                                t.italic = Some(true);
                            }
                            inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, &tag).unwrap_or(len);
                        continue;
                    }
                    "u" => {
                        let content = extract_tag_content(&chars, end, "u");
                        let inner = parse_inline_html(&content);
                        for mut inline in inner {
                            if let Inline::Text(t) = &mut inline {
                                t.underline = Some(true);
                            }
                            inlines.push(inline);
                        }
                        i = find_closing_tag(&chars, end, "u").unwrap_or(len);
                        continue;
                    }
                    "code" => {
                        let content = extract_tag_content(&chars, end, "code");
                        let code = content.trim().to_string();
                        inlines.push(Inline::Text(TextRun {
                            text: code,
                            style: None,
                            bold: None,
                            italic: None,
                            underline: None,
                            strikethrough: None,
                            source: None,
                        }));
                        i = find_closing_tag(&chars, end, "code").unwrap_or(len);
                        continue;
                    }
                    "br" => {
                        if let Some(Inline::Text(t)) = inlines.last_mut() {
                            t.text.push('\n');
                        } else {
                            inlines.push(Inline::Text(TextRun::new("\n".to_string())));
                        }
                        i = end;
                        continue;
                    }
                    _ => {
                        i = end;
                        continue;
                    }
                }
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

fn parse_list_items(content: &str, _ordered: bool) -> Vec<ListItem> {
    let mut items = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }

        if i >= len {
            break;
        }

        // Find <li> tag
        if i + 3 < len
            && chars[i] == '<'
            && chars[i + 1] == 'l'
            && chars[i + 2] == 'i'
            && chars[i + 3] == '>'
        {
            let content_start = i + 4;
            if let Some(end) = find_closing_tag(&chars, content_start, "li") {
                let item_content: String = chars[content_start..end].iter().collect();
                let inlines = parse_inline_html(&item_content);
                items.push(ListItem {
                    marker: None,
                    content: vec![Block::Paragraph(ParagraphBlock {
                        inlines,
                        geometry: None,
                        source: None,
                        style: None,
                    })],
                    checked: None,
                    source: None,
                });
                i = end + 5; // skip "</li>"
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading() {
        let html = "<h1>Title</h1><p>Text.</p>";
        let doc = parse_html_to_document(html);
        assert!(doc.pages[0].blocks.len() >= 2);
        let heading = doc.pages[0]
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Heading(_)));
        assert!(heading.is_some());
        if let Block::Heading(h) = heading.unwrap() {
            assert_eq!(h.level, 1);
        }
    }

    #[test]
    fn test_paragraph() {
        let html = "<p>Hello <strong>world</strong>!</p>";
        let doc = parse_html_to_document(html);
        assert!(!doc.pages[0].blocks.is_empty());
        let para = doc.pages[0]
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Paragraph(_)));
        assert!(para.is_some());
        if let Block::Paragraph(p) = para.unwrap() {
            assert!(p.inlines.len() >= 2);
        }
    }

    #[test]
    fn test_list() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let doc = parse_html_to_document(html);
        let list = doc.pages[0]
            .blocks
            .iter()
            .find(|b| matches!(b, Block::List(_)));
        assert!(list.is_some());
        if let Block::List(l) = list.unwrap() {
            assert_eq!(l.items.len(), 2);
            assert!(!l.is_ordered());
        }
    }

    #[test]
    fn test_code() {
        let html = "<pre><code class=\"language-rust\">fn main() {}</code></pre>";
        let doc = parse_html_to_document(html);
        let code = doc.pages[0]
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Code(_)));
        assert!(code.is_some());
        if let Block::Code(c) = code.unwrap() {
            assert_eq!(c.language.as_deref(), Some("rust"));
            assert!(c.code.contains("fn main"));
        }
    }
}

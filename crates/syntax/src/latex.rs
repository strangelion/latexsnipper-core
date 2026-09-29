use latexsnipper_ast::{
    Block, Diagnostic, DiagnosticLevel, Document, Formula, FormulaBlock, Inline, Page,
    ParagraphBlock, SourceInfo, Span, TextRun,
};
use latexsnipper_foundation::Result;

use crate::parser::Parser;
use crate::renderer::Renderer;
use crate::{ParsedDocument, SourceMap};

/// LaTeX parser — converts LaTeX string to Document AST.
pub struct LatexParser;

/// A group was opened with `{` but no matching `}` was found.
pub const W_LATEX_UNCLOSED_GROUP: &str = "W_LATEX_UNCLOSED_GROUP";
/// A math delimiter was opened but not closed.
pub const W_LATEX_UNCLOSED_DELIMITER: &str = "W_LATEX_UNCLOSED_DELIMITER";
/// A closing group appeared without a matching opening group.
pub const E_LATEX_UNMATCHED_GROUP: &str = "E_LATEX_UNMATCHED_GROUP";
/// A LaTeX environment was opened but not closed.
pub const E_LATEX_UNCLOSED_ENVIRONMENT: &str = "E_LATEX_UNCLOSED_ENVIRONMENT";
/// A LaTeX environment was closed out of order or without being opened.
pub const E_LATEX_ENVIRONMENT_MISMATCH: &str = "E_LATEX_ENVIRONMENT_MISMATCH";
/// A closing math delimiter did not match the active delimiter.
pub const E_LATEX_DELIMITER_MISMATCH: &str = "E_LATEX_DELIMITER_MISMATCH";

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
    let diagnostics = validate_latex_structure(input);
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
            diagnostics,
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        },
        source_map,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MathDelimiter {
    InlineDollar,
    DisplayDollar,
    Parenthesis,
    Bracket,
}

impl MathDelimiter {
    fn label(self) -> &'static str {
        match self {
            Self::InlineDollar => "$",
            Self::DisplayDollar => "$$",
            Self::Parenthesis => r"\(",
            Self::Bracket => r"\[",
        }
    }
}

/// Validate balanced LaTeX groups, environments, and math delimiters.
///
/// This intentionally remains a structural validation pass rather than a
/// package-aware TeX compiler. Recoverable problems are warnings so existing
/// callers can continue to render a best-effort preview. Errors identify input
/// that strict consumers should reject.
pub fn validate_latex_structure(input: &str) -> Vec<Diagnostic> {
    let bytes = input.as_bytes();
    let mut diagnostics = Vec::new();
    let mut groups = Vec::new();
    let mut environments: Vec<(String, usize)> = Vec::new();
    let mut math_delimiters: Vec<(MathDelimiter, usize)> = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                index = input[index..]
                    .find('\n')
                    .map_or(bytes.len(), |offset| index + offset + 1);
            }
            b'\\' => {
                if let Some((command, next_index)) = control_sequence(input, index) {
                    match command {
                        "begin" | "end" => {
                            if let Some((name, end_index)) = environment_name(input, next_index) {
                                if command == "begin" {
                                    environments.push((name.to_string(), index));
                                } else {
                                    match environments.last() {
                                        Some((active, _)) if active == name => {
                                            environments.pop();
                                        }
                                        Some((active, _)) => diagnostics.push(
                                            structural_diagnostic(
                                                DiagnosticLevel::Error,
                                                E_LATEX_ENVIRONMENT_MISMATCH,
                                                format!(
                                                    "environment '{name}' closes while '{active}' is active"
                                                ),
                                                input,
                                                index,
                                                end_index,
                                                false,
                                            ),
                                        ),
                                        None => diagnostics.push(structural_diagnostic(
                                            DiagnosticLevel::Error,
                                            E_LATEX_ENVIRONMENT_MISMATCH,
                                            format!(
                                                "environment '{name}' is closed without being opened"
                                            ),
                                            input,
                                            index,
                                            end_index,
                                            false,
                                        )),
                                    }
                                }
                                index = end_index;
                                continue;
                            }
                        }
                        "(" => {
                            math_delimiters.push((MathDelimiter::Parenthesis, index));
                            index = next_index;
                            continue;
                        }
                        "[" => {
                            math_delimiters.push((MathDelimiter::Bracket, index));
                            index = next_index;
                            continue;
                        }
                        ")" => {
                            close_math_delimiter(
                                &mut math_delimiters,
                                MathDelimiter::Parenthesis,
                                input,
                                index,
                                next_index,
                                &mut diagnostics,
                            );
                            index = next_index;
                            continue;
                        }
                        "]" => {
                            close_math_delimiter(
                                &mut math_delimiters,
                                MathDelimiter::Bracket,
                                input,
                                index,
                                next_index,
                                &mut diagnostics,
                            );
                            index = next_index;
                            continue;
                        }
                        _ => {}
                    }
                    index = next_index;
                } else {
                    index += 1;
                }
            }
            b'{' => {
                groups.push(index);
                index += 1;
            }
            b'}' => {
                if groups.pop().is_none() {
                    diagnostics.push(structural_diagnostic(
                        DiagnosticLevel::Error,
                        E_LATEX_UNMATCHED_GROUP,
                        "closing group has no matching opening group",
                        input,
                        index,
                        index + 1,
                        false,
                    ));
                }
                index += 1;
            }
            b'$' => {
                let (delimiter, width) = if bytes.get(index + 1) == Some(&b'$') {
                    (MathDelimiter::DisplayDollar, 2)
                } else {
                    (MathDelimiter::InlineDollar, 1)
                };
                if math_delimiters.last().map(|item| item.0) == Some(delimiter) {
                    math_delimiters.pop();
                } else if math_delimiters.is_empty() {
                    math_delimiters.push((delimiter, index));
                } else {
                    let active = math_delimiters.last().expect("checked non-empty").0;
                    diagnostics.push(structural_diagnostic(
                        DiagnosticLevel::Error,
                        E_LATEX_DELIMITER_MISMATCH,
                        format!(
                            "math delimiter '{}' cannot close active delimiter '{}'",
                            delimiter.label(),
                            active.label()
                        ),
                        input,
                        index,
                        index + width,
                        false,
                    ));
                }
                index += width;
            }
            _ => index += 1,
        }
    }

    for start in groups {
        diagnostics.push(structural_diagnostic(
            DiagnosticLevel::Warning,
            W_LATEX_UNCLOSED_GROUP,
            "opening group has no matching closing group",
            input,
            start,
            start + 1,
            true,
        ));
    }
    for (name, start) in environments {
        diagnostics.push(structural_diagnostic(
            DiagnosticLevel::Error,
            E_LATEX_UNCLOSED_ENVIRONMENT,
            format!("environment '{name}' has no matching \\end{{{name}}}"),
            input,
            start,
            input.len(),
            false,
        ));
    }
    for (delimiter, start) in math_delimiters {
        diagnostics.push(structural_diagnostic(
            DiagnosticLevel::Warning,
            W_LATEX_UNCLOSED_DELIMITER,
            format!(
                "math delimiter '{}' has no matching closing delimiter",
                delimiter.label()
            ),
            input,
            start,
            input.len(),
            true,
        ));
    }

    diagnostics.sort_by_key(|diagnostic| {
        diagnostic
            .source
            .as_ref()
            .and_then(|source| source.span)
            .map_or(usize::MAX, |span| span.start)
    });
    diagnostics
}

fn control_sequence(input: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = input.as_bytes();
    let first = *bytes.get(start + 1)?;
    if first.is_ascii_alphabetic() {
        let mut end = start + 2;
        while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
            end += 1;
        }
        Some((&input[start + 1..end], end))
    } else {
        // A non-alphabetic control symbol spans exactly one character; advance
        // by its UTF-8 width so multi-byte input cannot be sliced mid-character.
        let width = input[start + 1..].chars().next()?.len_utf8();
        let end = start + 1 + width;
        Some((&input[start + 1..end], end))
    }
}

fn environment_name(input: &str, command_end: usize) -> Option<(&str, usize)> {
    let bytes = input.as_bytes();
    if bytes.get(command_end) != Some(&b'{') {
        return None;
    }
    let name_start = command_end + 1;
    let relative_end = input[name_start..].find('}')?;
    let name_end = name_start + relative_end;
    Some((&input[name_start..name_end], name_end + 1))
}

fn close_math_delimiter(
    active: &mut Vec<(MathDelimiter, usize)>,
    closing: MathDelimiter,
    input: &str,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match active.last().map(|item| item.0) {
        Some(opening) if opening == closing => {
            active.pop();
        }
        Some(opening) => diagnostics.push(structural_diagnostic(
            DiagnosticLevel::Error,
            E_LATEX_DELIMITER_MISMATCH,
            format!(
                "math delimiter '{}' cannot close active delimiter '{}'",
                closing.label(),
                opening.label()
            ),
            input,
            start,
            end,
            false,
        )),
        None => diagnostics.push(structural_diagnostic(
            DiagnosticLevel::Error,
            E_LATEX_DELIMITER_MISMATCH,
            format!(
                "closing math delimiter '{}' has no matching opener",
                closing.label()
            ),
            input,
            start,
            end,
            false,
        )),
    }
}

fn structural_diagnostic(
    level: DiagnosticLevel,
    code: &str,
    message: impl Into<String>,
    input: &str,
    start: usize,
    end: usize,
    recoverable: bool,
) -> Diagnostic {
    Diagnostic::new(level, code, message)
        .with_source(
            SourceInfo::new().with_span(Span::new(start.min(input.len()), end.min(input.len()))),
        )
        .with_recoverable(recoverable)
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
        let Some(delimited) = find_next_formula(input, cursor) else {
            push_text_block(
                input,
                cursor,
                input.len(),
                &mut blocks,
                &mut source_map,
                &mut block_index,
            );
            break;
        };
        let Some(end) = find_formula_end(input, &delimited) else {
            // Never advance past an unmatched delimiter and silently lose text.
            push_text_block(
                input,
                cursor,
                input.len(),
                &mut blocks,
                &mut source_map,
                &mut block_index,
            );
            break;
        };
        push_text_block(
            input,
            cursor,
            delimited.start,
            &mut blocks,
            &mut source_map,
            &mut block_index,
        );
        push_formula_block(
            input,
            &delimited,
            end,
            &mut blocks,
            &mut source_map,
            &mut block_index,
        );
        cursor = end + delimited.close.len();
    }

    (blocks, source_map)
}

#[derive(Debug, Clone)]
struct DelimitedFormula<'a> {
    start: usize,
    open: &'a str,
    close: &'a str,
    display_mode: bool,
}

fn find_next_formula(input: &str, cursor: usize) -> Option<DelimitedFormula<'_>> {
    let bytes = input.as_bytes();
    let mut index = cursor;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if bytes.get(index + 1) == Some(&b'(') => {
                return Some(DelimitedFormula {
                    start: index,
                    open: r"\(",
                    close: r"\)",
                    display_mode: false,
                });
            }
            b'\\' if bytes.get(index + 1) == Some(&b'[') => {
                return Some(DelimitedFormula {
                    start: index,
                    open: r"\[",
                    close: r"\]",
                    display_mode: true,
                });
            }
            b'\\' => index = (index + 2).min(bytes.len()),
            b'$' => {
                let display_mode = bytes.get(index + 1) == Some(&b'$');
                let delimiter = if display_mode { "$$" } else { "$" };
                return Some(DelimitedFormula {
                    start: index,
                    open: delimiter,
                    close: delimiter,
                    display_mode,
                });
            }
            _ => index += 1,
        }
    }
    None
}

fn find_formula_end(input: &str, formula: &DelimitedFormula<'_>) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = formula.start + formula.open.len();
    while index + formula.close.len() <= bytes.len() {
        if bytes[index..].starts_with(formula.close.as_bytes()) {
            if formula.close == "$"
                && (bytes.get(index + 1) == Some(&b'$')
                    || (index > 0 && bytes.get(index - 1) == Some(&b'$')))
            {
                index += 1;
                continue;
            }
            return Some(index);
        }
        if bytes[index] == b'\\' && !formula.close.starts_with('\\') {
            index = (index + 2).min(bytes.len());
        } else {
            index += 1;
        }
    }
    None
}

fn push_text_block(
    input: &str,
    start: usize,
    end: usize,
    blocks: &mut Vec<Block>,
    source_map: &mut SourceMap,
    block_index: &mut usize,
) {
    let Some((start, end)) = trim_byte_range(input, start, end) else {
        return;
    };
    let stable_id = next_provisional_id(block_index, "paragraph");
    let span = Span::new(start, end);
    let source = SourceInfo::new()
        .with_stable_id(stable_id.clone())
        .with_span(span);
    source_map.insert(stable_id, span);
    blocks.push(Block::Paragraph(ParagraphBlock {
        inlines: vec![Inline::Text(TextRun::new(&input[start..end]))],
        geometry: None,
        source: Some(source),
        style: None,
    }));
}

fn push_formula_block(
    input: &str,
    delimited: &DelimitedFormula<'_>,
    formula_end: usize,
    blocks: &mut Vec<Block>,
    source_map: &mut SourceMap,
    block_index: &mut usize,
) {
    let content_start = delimited.start + delimited.open.len();
    let formula = input[content_start..formula_end].trim().to_string();
    let span = Span::new(delimited.start, formula_end + delimited.close.len());
    let stable_id = next_provisional_id(block_index, "formula");
    let source = SourceInfo::new()
        .with_stable_id(stable_id.clone())
        .with_span(span);
    source_map.insert(stable_id, span);
    let mut formula = Formula::latex(formula).with_source_info(source.clone());
    formula.display_mode = delimited.display_mode;
    blocks.push(Block::Formula(FormulaBlock {
        formula,
        label: None,
        number: None,
        environment: None,
        geometry: None,
        source: Some(source),
    }));
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
        assert_eq!(parsed.document.diagnostics.len(), 1);
        assert_eq!(
            parsed.document.diagnostics[0].code,
            W_LATEX_UNCLOSED_DELIMITER
        );
        assert!(parsed.document.diagnostics[0].recoverable);
    }

    #[test]
    fn structural_validation_distinguishes_recoverable_groups_from_fatal_environments() {
        let recoverable = parse_latex_with_source_map(r"\frac{1}{").unwrap();
        assert_eq!(recoverable.document.diagnostics.len(), 1);
        assert_eq!(
            recoverable.document.diagnostics[0].code,
            W_LATEX_UNCLOSED_GROUP
        );
        assert!(recoverable.document.diagnostics[0].recoverable);

        let fatal = parse_latex_with_source_map(r"\begin{matrix}1&2").unwrap();
        assert_eq!(fatal.document.diagnostics.len(), 1);
        assert_eq!(
            fatal.document.diagnostics[0].code,
            E_LATEX_UNCLOSED_ENVIRONMENT
        );
        assert_eq!(fatal.document.diagnostics[0].level, DiagnosticLevel::Error);
        assert!(!fatal.document.diagnostics[0].recoverable);
    }

    #[test]
    fn structural_validation_ignores_comments_and_escaped_group_characters() {
        let diagnostics = validate_latex_structure("text \\{ literal % { ignored\n$ok$");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn structural_validation_supports_parenthesis_and_bracket_math_delimiters() {
        let input = r"prefix \(x + 1\) and \[y\] suffix";
        let parsed = parse_latex_with_source_map(input).unwrap();
        assert!(parsed.document.diagnostics.is_empty());
        let formulas = parsed.document.pages[0]
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Formula(formula) => Some(&formula.formula),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(formulas.len(), 2);
        assert!(!formulas[0].display_mode);
        assert!(formulas[1].display_mode);
        assert_eq!(formulas[0].as_latex(), "x + 1");
        assert_eq!(formulas[1].as_latex(), "y");
    }

    #[test]
    fn control_symbols_can_be_multibyte_characters() {
        // Regression: `\` followed by a multi-byte character used to slice
        // inside that character and panic (fuzz artifact
        // crash-e03db579357ffc95a71c4bb288f2acc49cbd6367).
        for input in ["\u{5dd}", "\\\u{5dd}", "a\\\u{4e2d}b", "\\n\\\u{5dd}\n."] {
            let _ = validate_latex_structure(input);
            let _ = parse_latex_with_source_map(input).unwrap();
        }

        let input = "\\\u{5dd}";
        let (command, next) = control_sequence(input, 0).expect("control sequence");
        assert_eq!(command, "\u{5dd}");
        assert_eq!(next, 1 + "\u{5dd}".len());
    }

    #[test]
    fn delimiter_search_is_safe_for_unicode_formula_content() {
        let input = r"中文前缀 \(\text{速度}=变量\) 中文后缀";
        let parsed = parse_latex_with_source_map(input).unwrap();
        assert!(parsed.document.diagnostics.is_empty());
        let formula = parsed.document.pages[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Formula(formula) => Some(&formula.formula),
                _ => None,
            })
            .expect("unicode formula should be parsed");
        assert_eq!(formula.as_latex(), r"\text{速度}=变量");
    }
}

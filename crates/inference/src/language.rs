//! Language detection for multilingual OCR output.
//!
//! Detects whether a text string is primarily Chinese, English, or mixed,
//! and provides language-appropriate postprocessing (spacing, punctuation).

/// Detected language category for a text segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// English / Latin script (includes digits, punctuation)
    English,
    /// Chinese / CJK script (Chinese characters, Japanese kanji)
    Chinese,
    /// Mixed Latin + CJK (e.g. "Hello 你好 World")
    Mixed,
    /// Unknown or other scripts
    Other,
}

/// Language detector — analyzes text to determine its language category.
pub struct LanguageDetector;

impl LanguageDetector {
    /// Detect the primary language of the given text.
    pub fn detect(text: &str) -> Language {
        let mut latin_chars = 0usize;
        let mut cjk_chars = 0usize;
        let mut other_chars = 0usize;

        for ch in text.chars() {
            if ch.is_ascii() && (ch.is_ascii_alphabetic() || ch.is_ascii_digit()) {
                latin_chars += 1;
            } else if is_cjk(ch) {
                cjk_chars += 1;
            } else if !ch.is_whitespace() && !ch.is_ascii_punctuation() {
                other_chars += 1;
            }
        }

        let total = latin_chars + cjk_chars + other_chars;
        if total == 0 {
            return Language::Other;
        }

        let latin_ratio = latin_chars as f32 / total as f32;
        let cjk_ratio = cjk_chars as f32 / total as f32;

        if latin_ratio > 0.0 && cjk_ratio > 0.0 {
            Language::Mixed
        } else if cjk_ratio > 0.5 {
            Language::Chinese
        } else if latin_ratio > 0.5 {
            Language::English
        } else {
            Language::Other
        }
    }

    /// Post-process text based on detected language.
    /// Inserts appropriate spacing between Latin and CJK segments,
    /// normalizes punctuation, and applies language-specific rules.
    pub fn postprocess(text: &str) -> String {
        let lang = Self::detect(text);
        match lang {
            Language::Mixed => insert_latin_cjk_spaces(text),
            Language::Chinese => normalize_cjk_text(text),
            Language::English => normalize_english_text(text),
            Language::Other => text.to_string(),
        }
    }
}

/// Check if a character is in the CJK range.
fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'      // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'    // CJK Unified Ideographs Extension A
        | '\u{F900}'..='\u{FAFF}'    // CJK Compatibility Ideographs
        | '\u{2F800}'..='\u{2FA1F}' // CJK Compatibility Ideographs Supplement
        | '\u{3000}'..='\u{303F}'    // CJK Symbols and Punctuation
    )
}

/// Check if a character is Latin alphabet.
fn is_latin(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

/// Check if a character is a CJK punctuation.
fn is_cjk_punct(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{FF00}'..='\u{FFEF}' // Fullwidth forms
    )
}

/// Insert spaces between Latin and CJK text segments for mixed-language output.
fn insert_latin_cjk_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 1 {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len() + 8);

    for i in 0..chars.len() {
        result.push(chars[i]);

        if i + 1 < chars.len() {
            let cur = chars[i];
            let next = chars[i + 1];

            let should_space = match (char_category(cur), char_category(next)) {
                (CharCat::Latin, CharCat::CJK)
                | (CharCat::CJK, CharCat::Latin)
                | (CharCat::Latin, CharCat::CjkPunct)
                | (CharCat::CjkPunct, CharCat::Latin)
                | (CharCat::Digit, CharCat::CJK)
                | (CharCat::CJK, CharCat::Digit)
                | (CharCat::Digit, CharCat::Latin)
                | (CharCat::Punct, CharCat::CJK)
                | (CharCat::CJK, CharCat::Punct) => true,
                _ => false,
            };

            if should_space {
                result.push(' ');
            }
        }
    }

    result
}

/// Normalize CJK text: ensure proper spacing around punctuation.
fn normalize_cjk_text(text: &str) -> String {
    // CJK text typically doesn't need spaces between characters.
    // But we should normalize punctuation spacing.
    let mut result = String::with_capacity(text.len());

    for ch in text.chars() {
        // Remove spaces before CJK punctuation (full-width punctuation already has spacing)
        if ch.is_ascii_whitespace() {
            continue; // Remove extra whitespace in CJK context
        }
        result.push(ch);
    }

    result
}

/// Normalize English text: basic cleanup.
fn normalize_english_text(text: &str) -> String {
    // English text — already handled by standard OCR output.
    // Collapse multiple spaces.
    let mut result = String::with_capacity(text.len());
    let mut prev_space = false;

    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(ch);
            prev_space = false;
        }
    }

    result.trim().to_string()
}

/// Character category for spacing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharCat {
    Latin,
    CJK,
    Digit,
    Punct,
    CjkPunct,
    Space,
    Other,
}

fn char_category(ch: char) -> CharCat {
    if ch.is_ascii_whitespace() {
        CharCat::Space
    } else if is_latin(ch) {
        CharCat::Latin
    } else if ch.is_ascii_digit() {
        CharCat::Digit
    } else if is_cjk(ch) || is_cjk_punct(ch) {
        if is_cjk_punct(ch) {
            CharCat::CjkPunct
        } else {
            CharCat::CJK
        }
    } else if ch.is_ascii_punctuation() {
        CharCat::Punct
    } else {
        CharCat::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_english() {
        assert_eq!(LanguageDetector::detect("Hello World"), Language::English);
        assert_eq!(LanguageDetector::detect("OCR test 123"), Language::English);
    }

    #[test]
    fn test_detect_chinese() {
        assert_eq!(LanguageDetector::detect("中文识别测试"), Language::Chinese);
        assert_eq!(LanguageDetector::detect("你好世界"), Language::Chinese);
    }

    #[test]
    fn test_detect_mixed() {
        assert_eq!(LanguageDetector::detect("Hello 你好"), Language::Mixed);
        assert_eq!(LanguageDetector::detect("中文OCR测试Test"), Language::Mixed);
    }

    #[test]
    fn test_postprocess_mixed_spacing() {
        assert_eq!(LanguageDetector::postprocess("Hello你好"), "Hello 你好");
        assert_eq!(LanguageDetector::postprocess("你好World"), "你好 World");
        assert_eq!(
            LanguageDetector::postprocess("The quick brown fox"),
            "The quick brown fox"
        );
    }

    #[test]
    fn test_postprocess_cjk_removes_extra_spaces() {
        let result = LanguageDetector::postprocess("中文 识别 测试");
        assert_eq!(result, "中文识别测试");
    }

    #[test]
    fn test_postprocess_english_normalizes_spaces() {
        let result = LanguageDetector::postprocess("Hello   World  ");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_detect_empty() {
        assert_eq!(LanguageDetector::detect(""), Language::Other);
    }

    #[test]
    fn test_latin_cjk_mixed_digit_spacing() {
        assert_eq!(LanguageDetector::postprocess("测试123abc"), "测试 123 abc");
    }
}

//! Shared finite builtin signatures; these never expand or execute TeX macros.
use super::FormulaSlot;

pub(super) fn slots(name: &str) -> &'static [FormulaSlot] {
    match name {
        "frac" | "dfrac" | "tfrac" | "cfrac" => &[
            FormulaSlot::FractionNumerator,
            FormulaSlot::FractionDenominator,
        ],
        "binom" | "dbinom" | "tbinom" => &[FormulaSlot::BinomialUpper, FormulaSlot::BinomialLower],
        "text" | "textrm" | "textbf" | "textit" | "textsf" | "texttt" | "textnormal" | "textmd"
        | "textsc" | "textsl" | "textup" | "mbox" | "fbox" => &[FormulaSlot::Text],
        "vec"
        | "hat"
        | "bar"
        | "overline"
        | "underline"
        | "overbrace"
        | "underbrace"
        | "dot"
        | "ddot"
        | "dddot"
        | "ddddot"
        | "tilde"
        | "widetilde"
        | "widehat"
        | "check"
        | "breve"
        | "acute"
        | "grave"
        | "mathring"
        | "overrightarrow"
        | "overleftarrow"
        | "overleftrightarrow"
        | "underleftarrow"
        | "underrightarrow"
        | "underleftrightarrow"
        | "mathrm"
        | "mathbb"
        | "mathbf"
        | "mathit"
        | "mathcal"
        | "mathsf"
        | "mathtt"
        | "mathfrak"
        | "mathnormal"
        | "mathscr"
        | "boldsymbol"
        | "bm"
        | "boxed"
        | "ensuremath"
        | "phantom"
        | "hphantom"
        | "vphantom"
        | "operatorname" => &[FormulaSlot::MathArgument],
        "overset" | "underset" | "stackrel" => {
            &[FormulaSlot::MathArgument, FormulaSlot::MathArgument]
        }
        "begin" | "end" => &[FormulaSlot::EnvironmentName],
        _ => &[],
    }
}

pub(super) fn builtin_command(name: &str) -> bool {
    !slots(name).is_empty() || matches!(name, "sqrt" | "verb")
}

pub(super) fn starred(name: &str) -> bool {
    name == "operatorname"
}

/// Locate a bracket argument without expanding control sequences or macros.
pub(super) fn optional_end(source: &str, mut pos: usize) -> (usize, bool) {
    let mut braces = 0usize;
    while let Some(ch) = source[pos..].chars().next() {
        match ch {
            ']' if braces == 0 => return (pos, true),
            '}' if braces == 0 => return (pos, false),
            '{' => braces += 1,
            '}' => braces -= 1,
            '%' => {
                while source
                    .as_bytes()
                    .get(pos)
                    .is_some_and(|ch| !matches!(ch, b'\r' | b'\n'))
                {
                    pos += 1;
                }
                continue;
            }
            '\\' => {
                pos += 1;
                if source[pos..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                    while source
                        .as_bytes()
                        .get(pos)
                        .is_some_and(u8::is_ascii_alphabetic)
                    {
                        pos += 1;
                    }
                } else if let Some(ch) = source[pos..].chars().next() {
                    pos += ch.len_utf8();
                }
                continue;
            }
            _ => {}
        }
        pos += ch.len_utf8();
    }
    (pos, false)
}

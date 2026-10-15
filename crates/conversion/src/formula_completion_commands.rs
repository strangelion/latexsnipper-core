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

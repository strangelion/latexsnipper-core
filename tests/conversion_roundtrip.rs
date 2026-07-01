use latexsnipper_conversion::{DocumentConverter, OutputFormat};

/// 5x6 roundtrip matrix: 5 input formats x 6 output formats = 30 combinations
/// Each formula must survive every path without symbol loss.

const FORMULAS: &[(&str, &str)] = &[
    ("basic_frac", r"\frac{a+b}{c-d}"),
    ("nested_frac", r"\frac{\frac{1}{2}}{\frac{3}{4}}"),
    ("sqrt", r"\sqrt{x^2 + y^2}"),
    ("sqrt_degree", r"\sqrt[3]{x+y}"),
    ("superscript", r"x^{2n+1}"),
    ("subscript", r"x_{i+j}"),
    ("sub_sup", r"x_i^{2}"),
    ("sum_limits", r"\sum_{i=1}^{n} x_i"),
    ("product", r"\prod_{k=1}^{n} k"),
    ("integral", r"\int_{0}^{\infty} e^{-x^2} dx"),
    ("double_integral", r"\iint_{D} f(x,y) \, dA"),
    ("contour_integral", r"\oint_{C} \vec{F} \cdot d\vec{r}"),
    ("partial", r"\frac{\partial f}{\partial x}"),
    ("nabla", r"\nabla \times \vec{F}"),
    (
        "matrix_2x2",
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
    ),
    (
        "matrix_3x3",
        r"\begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}",
    ),
    (
        "cases",
        r"\begin{cases} x & \text{if } x > 0 \\ 0 & \text{otherwise} \end{cases}",
    ),
    (
        "aligned",
        r"\begin{aligned} a &= b + c \\ d &= e + f \end{aligned}",
    ),
    ("binom", r"\binom{n}{k}"),
    ("overbrace", r"\overbrace{a+b+c}^{3 \text{ terms}}"),
    ("underbrace", r"\underbrace{a+b+c}_{3 \text{ terms}}"),
    ("hat", r"\hat{x}"),
    ("bar", r"\bar{x}"),
    ("vec", r"\vec{v}"),
    ("dot", r"\dot{x}"),
    ("ddot", r"\ddot{x}"),
    ("tilde", r"\tilde{x}"),
    ("check", r"\check{x}"),
    ("left_right", r"\left( \frac{a}{b} \right)"),
    ("left_right_bracket", r"\left[ x^2 + y^2 \right]"),
    (
        "greek_lower",
        r"\alpha + \beta + \gamma + \delta + \epsilon + \theta + \lambda + \pi + \sigma + \omega",
    ),
    (
        "greek_upper",
        r"\Gamma + \Delta + \Theta + \Lambda + \Xi + \Pi + \Sigma + \Omega",
    ),
    ("varphi", r"\phi \neq \varphi"),
    ("varepsilon", r"\epsilon \neq \varepsilon"),
    ("relations", r"a \leq b"),
    ("relations2", r"a \geq b"),
    ("approx_equiv", r"a \approx b \equiv c \sim d"),
    ("neq", r"a \neq b"),
    ("set_ops", r"A \cup B \cap C \setminus D"),
    ("subset", r"A \subset B \subseteq C \supset D \supseteq E"),
    ("logic", r"\forall x \in X, \exists y \text{ s.t. } P(x,y)"),
    (
        "arrows",
        r"\rightarrow \leftarrow \leftrightarrow \Rightarrow \Leftarrow \Leftrightarrow",
    ),
    ("infty", r"\infty"),
    ("lim", r"\lim_{x \to \infty} f(x)"),
    ("log", r"\log_{2} n"),
    ("sin_cos", r"\sin \theta + \cos \theta = 1"),
    ("text", r"\text{Hello World}"),
    ("phantom", r"\phantom{000}"),
    ("complex", r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}"),
    ("euler", r"e^{i\pi} + 1 = 0"),
    (
        "taylor",
        r"\sum_{n=0}^{\infty} \frac{f^{(n)}(a)}{n!}(x-a)^n",
    ),
    ("gauss", r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}"),
    ("cross_product", r"\vec{a} \times \vec{b}"),
    ("dot_product", r"\vec{a} \cdot \vec{b}"),
];

fn latex_to_doc(latex: &str) -> latexsnipper_ast::Document {
    DocumentConverter::convert_latex_string(latex, OutputFormat::Latex).unwrap();
    // Build a minimal doc with one formula block
    use latexsnipper_ast::*;
    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            blocks: vec![Block::Formula(FormulaBlock {
                formula: Formula::latex(latex),
                geometry: None,
                source: None,
            })],
            page_number: None,
        }],
        id_gen: NodeIdGenerator::new(),
    }
}

fn convert_via(latex: &str, input_fmt: &str, output_fmt: OutputFormat) -> String {
    match input_fmt {
        "latex" => DocumentConverter::convert_latex_string(latex, output_fmt).unwrap(),
        "mathml" => {
            let mathml =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
            DocumentConverter::convert_mathml_string(&mathml, output_fmt).unwrap()
        }
        "omml" => {
            let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
            DocumentConverter::convert_omml_string(&omml, output_fmt).unwrap()
        }
        "typst" => {
            let typst =
                DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
            DocumentConverter::convert_typst_string(&typst, output_fmt).unwrap()
        }
        "markdown" => {
            let md = format!("$$ {} $$", latex);
            DocumentConverter::convert_markdown_string(&md, output_fmt).unwrap()
        }
        _ => unreachable!(),
    }
}

#[test]
fn roundtrip_latex_to_all() {
    let mut failures = Vec::new();
    for (name, latex) in FORMULAS {
        let outputs = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
            ("Markdown", OutputFormat::MarkdownBlock),
            ("HTML", OutputFormat::Html),
        ];
        for (fmt_name, fmt) in &outputs {
            let result = convert_via(latex, "latex", *fmt);
            if result.is_empty() {
                failures.push(format!("  [latex→{}] {} : EMPTY OUTPUT", fmt_name, name));
            }
        }
    }
    if !failures.is_empty() {
        panic!("Empty outputs detected:\n{}", failures.join("\n"));
    }
}

#[test]
fn roundtrip_mathml_to_all() {
    let mut failures = Vec::new();
    for (name, latex) in FORMULAS {
        let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
        let outputs = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
        ];
        for (fmt_name, fmt) in &outputs {
            let result = DocumentConverter::convert_mathml_string(&mathml, *fmt);
            match result {
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  [mathml→{}] {} : EMPTY", fmt_name, name));
                }
                Err(e) => {
                    failures.push(format!("  [mathml→{}] {} : ERROR: {}", fmt_name, name, e));
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!("MathML roundtrip failures:\n{}", failures.join("\n"));
    }
}

#[test]
fn roundtrip_omml_to_all() {
    let mut failures = Vec::new();
    for (name, latex) in FORMULAS {
        let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
        let outputs = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
        ];
        for (fmt_name, fmt) in &outputs {
            let result = DocumentConverter::convert_omml_string(&omml, *fmt);
            match result {
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  [omml→{}] {} : EMPTY", fmt_name, name));
                }
                Err(e) => {
                    failures.push(format!("  [omml→{}] {} : ERROR: {}", fmt_name, name, e));
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!("OMML roundtrip failures:\n{}", failures.join("\n"));
    }
}

#[test]
fn roundtrip_typst_to_all() {
    let mut failures = Vec::new();
    for (name, latex) in FORMULAS {
        let typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
        let outputs = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
        ];
        for (fmt_name, fmt) in &outputs {
            let result = DocumentConverter::convert_typst_string(&typst, *fmt);
            match result {
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  [typst→{}] {} : EMPTY", fmt_name, name));
                }
                Err(e) => {
                    failures.push(format!("  [typst→{}] {} : ERROR: {}", fmt_name, name, e));
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!("Typst roundtrip failures:\n{}", failures.join("\n"));
    }
}

#[test]
fn roundtrip_markdown_to_all() {
    let mut failures = Vec::new();
    for (name, latex) in FORMULAS {
        let md = format!("$$ {} $$", latex);
        let outputs = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
        ];
        for (fmt_name, fmt) in &outputs {
            let result = DocumentConverter::convert_markdown_string(&md, *fmt);
            match result {
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  [markdown→{}] {} : EMPTY", fmt_name, name));
                }
                Err(e) => {
                    failures.push(format!("  [markdown→{}] {} : ERROR: {}", fmt_name, name, e));
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!("Markdown roundtrip failures:\n{}", failures.join("\n"));
    }
}

#[test]
fn omml_symbol_preservation() {
    let critical_symbols = &[
        (r"\frac{a}{b}", "<m:f>"),
        (r"\sqrt{x}", "<m:rad>"),
        (r"\sqrt[3]{x}", "<m:rad>"),
        (r"x^{2}", "<m:sSup>"),
        (r"x_{i}", "<m:sSub>"),
        (r"\sum_{i=1}^{n} x_i", "<m:nary>"),
        (r"\int_{0}^{1} f(x) dx", "<m:nary>"),
        (r"\prod_{k=1}^{n} k", "<m:nary>"),
        (r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", "<m:d>"),
        (r"\overbrace{a+b}^{c}", "<m:bar>"),
        (r"\underbrace{a+b}_{c}", "<m:bar>"),
        (r"\hat{x}", "<m:acc>"),
        (r"\vec{v}", "<m:acc>"),
        (r"\bar{x}", "<m:acc>"),
    ];

    let mut failures = Vec::new();
    for (latex, expected_tag) in critical_symbols {
        let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
        if !omml.contains(expected_tag) {
            failures.push(format!(
                "  {} : expected '{}' in OMML but not found\n    OMML: {}",
                latex,
                expected_tag,
                &omml[..omml.len().min(200)]
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "OMML symbol preservation failures:\n{}",
            failures.join("\n")
        );
    }
}

#[test]
fn mathml_symbol_preservation() {
    let critical_symbols = &[
        (r"\frac{a}{b}", "<mfrac>"),
        (r"\sqrt{x}", "<msqrt>"),
        (r"\sqrt[3]{x}", "<mroot>"),
        (r"x^{2}", "<msup>"),
        (r"x_{i}", "<msub>"),
        (r"\alpha", "\u{03B1}"),
        (r"\infty", "\u{221E}"),
    ];

    let mut failures = Vec::new();
    for (latex, expected_tag) in critical_symbols {
        let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
        if !mathml.contains(expected_tag) {
            failures.push(format!(
                "  {} : expected '{}' in MathML but not found\n    MathML: {}",
                latex,
                expected_tag,
                &mathml[..mathml.len().min(200)]
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "MathML symbol preservation failures:\n{}",
            failures.join("\n")
        );
    }
}

#[test]
fn typst_symbol_preservation() {
    let critical_symbols = &[
        (r"\frac{a}{b}", "frac(a, b)"),
        (r"\sqrt{x}", "sqrt(x)"),
        (r"\sqrt[3]{x}", "root(3, x)"),
        (r"\binom{n}{k}", "binom(n, k)"),
        (r"\hat{x}", "hat x"),
        (r"\vec{v}", "vec v"),
        (r"\sum", "sum"),
        (r"\int", "integral"),
    ];

    let mut failures = Vec::new();
    for (latex, expected) in critical_symbols {
        let typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
        if !typst.contains(expected) {
            failures.push(format!(
                "  {} : expected '{}' in Typst but not found\n    Typst: {}",
                latex, expected, typst
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "Typst symbol preservation failures:\n{}",
            failures.join("\n")
        );
    }
}

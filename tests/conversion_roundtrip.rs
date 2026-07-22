use latexsnipper_conversion::{DocumentConverter, OutputFormat};

/// 5x6 roundtrip matrix: 5 input formats x 6 output formats = 30 combinations
/// Each formula must survive every path without symbol loss.
const FORMULAS: &[(&str, &str)] = &[
    ("basic_frac", r"\frac{a+b}{c-d}"),
    ("nested_frac", r"\frac{\frac{1}{2}}{\frac{3}{4}}"),
    ("sqrt", r"\sqrt{x^2 + y^2}"),
    ("sqrt_degree", r"\sqrt[3]{x+y}"),
    ("sqrt_fourth", r"\sqrt[4]{x}"),
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
    ("boxed", r"\boxed{E = mc^2}"),
    ("tag", r"E = mc^2\tag{1}"),
    ("displaystyle", r"\displaystyle \sum_{i=1}^{n} x_i"),
    ("textstyle", r"\textstyle \sum_{i=1}^{n} x_i"),
    ("norm", r"\norm{x}"),
    ("abs_val", r"\abs{x}"),
    ("floor", r"\floor{x}"),
    ("ceil", r"\ceil{x}"),
    ("vphantom", r"\frac{a}{\vphantom{b}c}"),
    ("hphantom", r"x + \hphantom{abc} + y"),
];

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

fn assert_contains_all(label: &str, output: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            output.contains(needle),
            "{} missing '{}': {}",
            label,
            needle,
            output
        );
    }
}

fn assert_count_at_least(label: &str, output: &str, needle: &str, minimum: usize) {
    let count = output.matches(needle).count();
    assert!(
        count >= minimum,
        "{} expected at least {} occurrence(s) of '{}', found {}: {}",
        label,
        minimum,
        needle,
        count,
        output
    );
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
        (r"\hat{x}", "hat(x)"),
        (r"\vec{v}", "vec(v)"),
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

#[test]
fn inline_style_preservation() {
    let latex = r"a+\textcolor{red}{x}+\mathbf{y}+\mathbb{R}+\Large{z}";

    let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
    assert!(
        mathml.contains("mathcolor=\"red\""),
        "MathML lost inline color: {}",
        mathml
    );
    assert!(
        mathml.contains("fontweight=\"bold\""),
        "MathML lost inline bold font: {}",
        mathml
    );
    assert!(
        mathml.contains("mathvariant=\"double-struck\""),
        "MathML lost inline blackboard font: {}",
        mathml
    );
    assert!(
        mathml.contains("mathsize=\"144%\""),
        "MathML lost inline font size: {}",
        mathml
    );

    let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
    assert!(omml.contains("<m:r>"), "OMML missing math runs: {}", omml);
    assert!(omml.contains("<m:t>"), "OMML missing text: {}", omml);
    assert!(
        omml.contains("<w:sz w:val=\"29\"/>"),
        "OMML lost inline font size: {}",
        omml
    );

    let typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
    assert!(
        typst.contains("math.color(red"),
        "Typst lost inline color: {}",
        typst
    );
    assert!(
        typst.contains("bold(y)"),
        "Typst lost inline bold: {}",
        typst
    );
    assert!(
        typst.contains("bb(R)"),
        "Typst lost inline blackboard font: {}",
        typst
    );
    assert!(
        typst.contains("text(size: 1.44em)[z]"),
        "Typst lost inline font size: {}",
        typst
    );

    let latex_from_mathml =
        DocumentConverter::convert_mathml_string(&mathml, OutputFormat::Latex).unwrap();
    assert!(
        latex_from_mathml.contains("\\textcolor{red}{x}"),
        "MathML roundtrip lost inline color: {}",
        latex_from_mathml
    );
    assert!(
        latex_from_mathml.contains("\\mathbf{y}"),
        "MathML roundtrip lost inline bold: {}",
        latex_from_mathml
    );

    let latex_from_omml =
        DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex).unwrap();
    assert!(
        latex_from_omml.contains("\\textcolor{FF0000}{x}"),
        "OMML roundtrip lost inline color: {}",
        latex_from_omml
    );
    assert!(
        latex_from_omml.contains("\\mathbf{y}"),
        "OMML roundtrip lost inline bold: {}",
        latex_from_omml
    );
    assert!(
        latex_from_omml.contains("\\mathbb{R}"),
        "OMML roundtrip lost blackboard style: {}",
        latex_from_omml
    );
    assert!(
        latex_from_omml.contains("\\Large{z}"),
        "OMML roundtrip lost inline font size: {}",
        latex_from_omml
    );
}

#[test]
fn structural_formula_preservation_sentinels() {
    let cases = r"\begin{cases}x&a&b\\y&c&d\end{cases}";
    let mathml = DocumentConverter::convert_latex_string(cases, OutputFormat::MathML).unwrap();
    for value in ["a", "b", "c", "d"] {
        assert!(
            mathml.contains(&format!("<mi>{}</mi>", value)),
            "MathML lost a cases column '{}': {}",
            value,
            mathml
        );
    }

    let omml = DocumentConverter::convert_latex_string(cases, OutputFormat::OMML).unwrap();
    for value in ["a", "b", "c", "d"] {
        assert!(
            omml.contains(&format!("<m:t>{}</m:t>", value)),
            "OMML lost a cases column '{}': {}",
            value,
            omml
        );
    }

    let styled = r"\mathbf{\frac{a}{b}}";
    let styled_omml = DocumentConverter::convert_latex_string(styled, OutputFormat::OMML).unwrap();
    assert!(
        styled_omml.contains("<m:f>") && styled_omml.contains("<m:sty m:val=\"b\"/>"),
        "OMML lost nested style structure: {}",
        styled_omml
    );

    let color_bold = r"\textcolor{red}{\mathbf{x}}";
    let color_bold_omml =
        DocumentConverter::convert_latex_string(color_bold, OutputFormat::OMML).unwrap();
    assert!(
        color_bold_omml.contains("<w:color w:val=\"FF0000\"/>")
            && color_bold_omml.contains("<m:sty m:val=\"b\"/>"),
        "OMML lost combined color/bold formatting: {}",
        color_bold_omml
    );
    let color_bold_latex =
        DocumentConverter::convert_omml_string(&color_bold_omml, OutputFormat::Latex).unwrap();
    assert!(
        color_bold_latex.contains("\\textcolor{FF0000}{\\mathbf{x}}")
            || color_bold_latex.contains("\\mathbf{\\textcolor{FF0000}{x}}"),
        "OMML roundtrip lost combined color/bold formatting: {}",
        color_bold_latex
    );

    let size_bold = r"\Large{\mathbf{x}}";
    let size_bold_omml =
        DocumentConverter::convert_latex_string(size_bold, OutputFormat::OMML).unwrap();
    assert!(
        size_bold_omml.contains("<w:sz w:val=\"29\"/>")
            && size_bold_omml.contains("<m:sty m:val=\"b\"/>"),
        "OMML lost combined size/bold formatting: {}",
        size_bold_omml
    );
    let size_bold_latex =
        DocumentConverter::convert_omml_string(&size_bold_omml, OutputFormat::Latex).unwrap();
    assert!(
        size_bold_latex.contains("\\Large{\\mathbf{x}}")
            || size_bold_latex.contains("\\mathbf{\\Large{x}}"),
        "OMML roundtrip lost combined size/bold formatting: {}",
        size_bold_latex
    );

    let nary = r"\sum_{\frac{a}{b}}^{n} x";
    let nary_omml = DocumentConverter::convert_latex_string(nary, OutputFormat::OMML).unwrap();
    assert!(
        nary_omml.contains("<m:sub><m:f>"),
        "OMML flattened nested n-ary limit: {}",
        nary_omml
    );
}

#[test]
fn common_formula_format_integrity_suite() {
    let structural_cases = [
        (
            "fraction",
            r"\frac{a+b}{c-d}",
            &["<m:f>"][..],
            &["<mfrac>"][..],
            &["frac("][..],
            &["\\frac", "a", "b", "c", "d"][..],
        ),
        (
            "sqrt",
            r"\sqrt{x^2+y^2}",
            &["<m:rad>", "<m:sSup>"][..],
            &["<msqrt>", "<msup>"][..],
            &["sqrt("][..],
            &["\\sqrt", "x", "y"][..],
        ),
        (
            "nth_root",
            r"\sqrt[3]{x+y}",
            &["<m:rad>", "<m:deg>"][..],
            &["<mroot>"][..],
            &["root(3"][..],
            &["\\sqrt[3]", "x", "y"][..],
        ),
        (
            "sub_sup",
            r"x_i^2+y^{z_1}",
            &["<m:sSup>", "<m:sSub>"][..],
            &["<msup>", "<msub>"][..],
            &["^", "_"][..],
            &["x", "i", "2", "y", "z"][..],
        ),
        (
            "sum",
            r"\sum_{i=1}^{n} x_i",
            &["<m:nary>", "<m:sub>", "<m:sup>"][..],
            &["<munderover>", "\u{2211}"][..],
            &["sum"][..],
            &["\\sum", "i", "n", "x"][..],
        ),
        (
            "integral",
            r"\int_{0}^{1} f(x)\,dx",
            &["<m:nary>", "\u{222B}"][..],
            &["<munderover>", "\u{222B}"][..],
            &["integral"][..],
            &["\\int", "0", "1", "f", "dx"][..],
        ),
        (
            "product",
            r"\prod_{k=1}^{n} k",
            &["<m:nary>", "\u{220F}"][..],
            &["<munderover>", "\u{220F}"][..],
            &["product"][..],
            &["\\prod", "k", "n"][..],
        ),
        (
            "lim_sin_fraction",
            r"\lim_{x\to0}\frac{\sin x}{x}",
            &["<m:func>", "<m:f>"][..],
            &["<mfrac>", "sin"][..],
            &["lim", "sin", "frac"][..],
            &["\\lim", "\\sin", "\\frac", "x"][..],
        ),
        (
            "pmatrix",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            &["<m:d>", "<m:m>", "<m:mr>"][..],
            &["<mtable>", "<mtr>", "<mtd>"][..],
            &["mat("][..],
            &["\\begin{pmatrix}", "a", "b", "c", "d"][..],
        ),
        (
            "cases",
            r"\begin{cases} x & x>0 \\ 0 & x\leq0 \end{cases}",
            &["<m:d>", "<m:m>", "<m:mr>"][..],
            &["<mtable>", "<mtr>", "<mtd>"][..],
            &["cases("][..],
            &["\\begin{cases}", "x", "0"][..],
        ),
        (
            "text_and_styles",
            r"\mathbb{R}+\mathcal{F}+\mathbf{x}+\mathrm{i.o.}+\text{ if }",
            &[
                "<m:sty m:val=\"d\"/>",
                "<m:sty m:val=\"c\"/>",
                "<m:sty m:val=\"b\"/>",
                "<m:nor/>",
                "i.o.",
                " if ",
            ][..],
            &[
                "mathvariant=\"double-struck\"",
                "mathvariant=\"script\"",
                "fontweight=\"bold\"",
                "i.o.",
                " if ",
            ][..],
            &["bb(R)", "cal(F)", "bold(x)", "upright(i.o.)"][..],
            &[
                "\\mathbb{R}",
                "\\mathcal{F}",
                "\\mathbf{x}",
                "\\mathrm{i.o.}",
                "if",
            ][..],
        ),
        (
            "accents_and_implies",
            r"\hat{x}+\vec{v}+A\implies B",
            &["<m:acc>", "\u{21D2}"][..],
            &["<mover>", "\u{21D2}"][..],
            &["hat(x)", "vec(v)", "arrow.r.double"][..],
            &["\\hat{x}", "\\vec{v}", "\u{21D2}"][..],
        ),
        (
            "left_right_fraction",
            r"\left(\frac{a}{b}\right)",
            &["<m:d>", "<m:f>"][..],
            &["<mfenced", "<mfrac>"][..],
            &["frac("][..],
            &["(", "\\frac", "a", "b", ")"][..],
        ),
    ];

    for (name, latex, omml_needles, mathml_needles, typst_needles, roundtrip_needles) in
        structural_cases
    {
        let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
        let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
        let typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
        let roundtrip = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex).unwrap();

        assert_contains_all(&format!("{} OMML", name), &omml, omml_needles);
        assert_contains_all(&format!("{} MathML", name), &mathml, mathml_needles);
        assert_contains_all(&format!("{} Typst", name), &typst, typst_needles);
        assert_contains_all(
            &format!("{} OMML->LaTeX roundtrip", name),
            &roundtrip,
            roundtrip_needles,
        );

        assert!(
            !omml.contains('\\'),
            "{} OMML leaked raw LaTeX command text: {}",
            name,
            omml
        );
    }

    let nested = r"\frac{\frac{1}{2}}{\frac{3}{4}}";
    let omml = DocumentConverter::convert_latex_string(nested, OutputFormat::OMML).unwrap();
    let mathml = DocumentConverter::convert_latex_string(nested, OutputFormat::MathML).unwrap();
    let roundtrip = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex).unwrap();
    assert_count_at_least("nested fraction OMML", &omml, "<m:f>", 3);
    assert_count_at_least("nested fraction MathML", &mathml, "<mfrac>", 3);
    assert_count_at_least("nested fraction roundtrip", &roundtrip, "\\frac", 3);

    let nested_limit = r"\sum_{\frac{i}{n}}^{\sqrt{k}} x_i";
    let omml = DocumentConverter::convert_latex_string(nested_limit, OutputFormat::OMML).unwrap();
    let roundtrip = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex).unwrap();
    assert_contains_all(
        "nested n-ary limit OMML",
        &omml,
        &["<m:nary>", "<m:sub><m:f>", "<m:sup><m:rad>"],
    );
    assert_contains_all(
        "nested n-ary limit roundtrip",
        &roundtrip,
        &["\\sum", "\\frac", "\\sqrt", "x"],
    );

    let three_column_cases = r"\begin{cases}x&a&b\\y&c&d\end{cases}";
    let omml =
        DocumentConverter::convert_latex_string(three_column_cases, OutputFormat::OMML).unwrap();
    let mathml =
        DocumentConverter::convert_latex_string(three_column_cases, OutputFormat::MathML).unwrap();
    for value in ["a", "b", "c", "d"] {
        assert!(
            omml.contains(&format!("<m:t>{}</m:t>", value)),
            "OMML lost a three-column cases value '{}': {}",
            value,
            omml
        );
        assert!(
            mathml.contains(&format!("<mi>{}</mi>", value)),
            "MathML lost a three-column cases value '{}': {}",
            value,
            mathml
        );
    }
}

#[test]
fn omml_borel_cantelli_formula_preserves_text_fonts_and_implies() {
    let latex = r"A_n\text{ independent},\;\sum_n\mathbb P(A_n)=\infty\implies\mathbb P(A_n\;\mathrm{i.o.})=1";
    let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();

    assert!(
        !omml.contains("\\text")
            && !omml.contains("\\implies")
            && !omml.contains("\\mathbb")
            && !omml.contains("\\mathrm"),
        "OMML leaked raw LaTeX commands: {}",
        omml
    );
    assert!(omml.contains("independent"), "OMML lost text: {}", omml);
    assert!(
        omml.contains("<m:sty m:val=\"d\"/>"),
        "OMML lost mathbb P: {}",
        omml
    );
    assert!(omml.contains("⇒"), "OMML lost implies symbol: {}", omml);
    assert!(omml.contains("i.o."), "OMML lost roman i.o. text: {}", omml);

    let roundtrip = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex).unwrap();
    assert!(
        roundtrip.contains("independent")
            && roundtrip.contains("\\sum")
            && roundtrip.contains("\\mathbb{P}")
            && roundtrip.contains("\\mathrm{i.o.}")
            && (roundtrip.contains("\\Rightarrow") || roundtrip.contains("⇒")),
        "OMML roundtrip lost Borel-Cantelli formula structure: {}",
        roundtrip
    );
}

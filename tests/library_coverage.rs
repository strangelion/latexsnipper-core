use latexsnipper_conversion::{DocumentConverter, OutputFormat};

/// Real-world formulas extracted from obsidian-formula-library.
/// Tests all key symbol patterns that must survive roundtrip conversion.

const LIBRARY_FORMULAS: &[(&str, &str)] = &[
    // === Algebra ===
    ("vector_space", r"V\text{ over }\mathbb{F}"),
    ("inner_product", r"\langle x,y\rangle"),
    ("tensor_product", r"V\otimes W"),
    ("direct_sum", r"W\oplus W^\perp"),
    ("eigenvalue", r"Av=\lambda v"),
    ("trace", r"\operatorname{tr}A"),
    ("determinant", r"\det A"),
    ("matrix_mult", r"(AB)_{ij}=\sum_k A_{ik}B_{kj}"),
    ("cramer", r"x_i=\frac{\det A_i}{\det A}"),
    (
        "gram_schmidt",
        r"u_k=v_k-\sum_{j}\operatorname{proj}_{u_j}v_k",
    ),
    ("jordan_block", r"J_k(\lambda)"),
    ("svd", r"A=U\Sigma V^*"),
    ("lie_bracket", r"[X,Y]"),
    ("lie_algebra", r"\mathfrak g=T_eG"),
    ("adjoint", r"F\dashv G"),
    ("functor", r"F:\mathcal{C}\to\mathcal{D}"),
    ("natural_trans", r"\eta:F\Rightarrow G"),
    ("limit", r"\varprojlim F"),
    ("colimit", r"\varinjlim F"),
    (
        "chain_complex",
        r"\cdots\to C_{n+1}\xrightarrow{\partial}C_n\xrightarrow{\partial}C_{n-1}\to\cdots",
    ),
    ("ext_functor", r"\operatorname{Ext}^n_R(M,N)"),
    ("tor_functor", r"\operatorname{Tor}^R_n(M,N)"),
    // === Analysis ===
    ("sequence_limit", r"\lim_{n\to\infty}a_n=L"),
    ("function_limit", r"\lim_{x\to a}f(x)=L"),
    ("derivative", r"f'(x)=\lim_{h\to0}\frac{f(x+h)-f(x)}{h}"),
    ("partial_deriv", r"\frac{\partial f}{\partial x_i}"),
    (
        "gradient",
        r"\nabla f=\left(\frac{\partial f}{\partial x_1},\ldots,\frac{\partial f}{\partial x_n}\right)",
    ),
    (
        "divergence",
        r"\nabla\cdot\mathbf F=\sum_i\frac{\partial F_i}{\partial x_i}",
    ),
    ("curl", r"\nabla\times\mathbf F"),
    (
        "taylor",
        r"f(x)=\sum_{k=0}^{n}\frac{f^{(k)}(a)}{k!}(x-a)^k+R_n(x)",
    ),
    (
        "integration_by_parts",
        r"\int_a^b u\,dv=uv\big|_a^b-\int_a^b v\,du",
    ),
    (
        "green",
        r"\oint_{\partial D}P\,dx+Q\,dy=\iint_D\left(\frac{\partial Q}{\partial x}-\frac{\partial P}{\partial y}\right)dA",
    ),
    (
        "stokes",
        r"\oint_{\partial S}\mathbf F\cdot d\mathbf r=\iint_S(\nabla\times\mathbf F)\cdot\mathbf n\,dS",
    ),
    (
        "divergence_thm",
        r"\iiint_V\nabla\cdot\mathbf F\,dV=\iint_{\partial V}\mathbf F\cdot\mathbf n\,dS",
    ),
    (
        "holder",
        r"\int_\Omega|fg|dx\le\left(\int_\Omega|f|^p\right)^{1/p}\left(\int_\Omega|g|^q\right)^{1/q}",
    ),
    (
        "minkowski",
        r"\left(\int|f+g|^p\right)^{1/p}\le\left(\int|f|^p\right)^{1/p}+\left(\int|g|^p\right)^{1/p}",
    ),
    ("young", r"ab\le\frac{a^p}{p}+\frac{b^q}{q}"),
    ("jensen", r"\varphi\!\left(\int f\right)\le\int\varphi(f)"),
    (
        "sobolev",
        r"\|u\|_{L^q(\mathbb R^n)}\le C\|\nabla u\|_{L^p(\mathbb R^n)}",
    ),
    // === Complex Analysis ===
    (
        "residue",
        r"\operatorname{Res}(f,a)=\frac{1}{2\pi i}\oint_\gamma f(z)\,dz",
    ),
    (
        "cauchy_integral",
        r"f(a)=\frac1{2\pi i}\oint_\gamma\frac{f(z)}{z-a}\,dz",
    ),
    (
        "residue_theorem",
        r"\oint_\gamma f(z)dz=2\pi i\sum_k\operatorname{Res}(f,a_k)",
    ),
    ("laurent", r"f(z)=\sum_{n=-\infty}^{\infty}a_n(z-a)^n"),
    // === Functional Analysis ===
    (
        "spectrum",
        r"\sigma(T)=\{\lambda:T-\lambda I\text{ not invertible}\}",
    ),
    (
        "fredholm",
        r"\operatorname{ind}T=\dim\ker T-\dim\operatorname{coker}T",
    ),
    ("spectral_radius", r"r(T)=\lim_{n\to\infty}\|T^n\|^{1/n}"),
    (
        "fourier_transform",
        r"\widehat f(\xi)=\int_{\mathbb{R}^n}f(x)e^{-2\pi ix\cdot\xi}dx",
    ),
    ("convolution", r"(f*g)(x)=\int f(y)g(x-y)dy"),
    // === Geometry ===
    ("pythagorean", r"a^2+b^2=c^2"),
    (
        "law_of_sines",
        r"\frac a{\sin A}=\frac b{\sin B}=\frac c{\sin C}",
    ),
    ("law_of_cosines", r"c^2=a^2+b^2-2ab\cos C"),
    ("heron", r"\Delta=\sqrt{s(s-a)(s-b)(s-c)}"),
    ("curvature", r"\kappa=\frac{|r'\times r''|}{|r'|^3}"),
    (
        "torsion",
        r"\tau=\frac{(r'\times r'')\cdot r'''}{|r'\times r''|^2}",
    ),
    (
        "gauss_bonnet",
        r"\int_MK\,dA+\int_{\partial M}k_g\,ds=2\pi\chi(M)",
    ),
    (
        "riemann_tensor",
        r"R(X,Y)Z=\nabla_X\nabla_YZ-\nabla_Y\nabla_XZ-\nabla_{[X,Y]}Z",
    ),
    // === Number Theory ===
    (
        "euler_totient",
        r"\varphi(n)=n\prod_{p\mid n}\left(1-\frac1p\right)",
    ),
    (
        "mobius",
        r"\mu(n)=\begin{cases}1,&n=1\\(-1)^k,&n=p_1\cdots p_k\\0,&p^2\mid n\end{cases}",
    ),
    (
        "riemann_zeta",
        r"\zeta(s)=\sum_{n=1}^\infty n^{-s}=\prod_p(1-p^{-s})^{-1}",
    ),
    (
        "landau_symbol",
        r"\Lambda(n)=\begin{cases}\log p,&n=p^k\\0,&\text{otherwise}\end{cases}",
    ),
    // === Probability / Statistics ===
    ("expected", r"\mathbb E[X]=\int x\,dF(x)"),
    (
        "variance",
        r"\operatorname{Var}(X)=\mathbb E[X^2]-\mathbb E[X]^2",
    ),
    ("bayes", r"P(A|B)=\frac{P(B|A)P(A)}{P(B)}"),
    // === Physics ===
    ("einstein", r"E=mc^2"),
    (
        "schrodinger",
        r"i\hbar\frac{\partial}{\partial t}\Psi=\hat H\Psi",
    ),
    (
        "maxwell_div",
        r"\nabla\cdot\mathbf E=\frac{\rho}{\varepsilon_0}",
    ),
    (
        "maxwell_curl",
        r"\nabla\times\mathbf B=\mu_0\mathbf J+\mu_0\varepsilon_0\frac{\partial\mathbf E}{\partial t}",
    ),
    // === Chemistry ===
    (
        "henderson",
        r"\mathrm{pH} = \mathrm{p}K_a + \log\frac{[\ce{A-}]}{[\ce{HA}]}",
    ),
    ("nernst", r"E = E^\circ - \frac{RT}{nF}\ln Q"),
    ("gibbs", r"\Delta G = \Delta H - T\Delta S"),
    ("arrhenius", r"k = Ae^{-E_a/RT}"),
    ("ideal_gas", r"PV=nRT"),
    // === Delimiters & Structure ===
    ("norm", r"\left\| x \right\|"),
    ("floor", r"\lfloor x \rfloor"),
    ("ceiling", r"\lceil x \rceil"),
    ("bra_ket", r"\left\langle \psi \middle| \phi \right\rangle"),
    (
        "cases_env",
        r"\begin{cases} x & \text{if } x > 0 \\ 0 & \text{otherwise} \end{cases}",
    ),
    (
        "aligned_env",
        r"\begin{aligned} a &= b + c \\ d &= e + f \end{aligned}",
    ),
    (
        "matrix_env",
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
    ),
    (
        "bmatrix_env",
        r"\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}",
    ),
    // === Arrows ===
    ("implies", r"A\implies B"),
    ("iff", r"A\iff B"),
    ("xrightarrow", r"A\xrightarrow{f}B"),
    ("hookrightarrow", r"A\hookrightarrow B"),
    ("mapsto", r"x\mapsto f(x)"),
    ("longmapsto", r"x\longmapsto f(x)"),
    // === Complex formulas ===
    (
        "gauss_integral",
        r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}",
    ),
    ("euler_identity", r"e^{i\pi} + 1 = 0"),
    (
        "taylor_series",
        r"\sum_{n=0}^{\infty} \frac{f^{(n)}(a)}{n!}(x-a)^n",
    ),
    ("quadratic", r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}"),
    (
        "weyl_character",
        r"\chi_\lambda=\frac{\sum_{w\in W}\operatorname{sgn}(w)e^{w(\lambda+\rho)}}{\sum_{w\in W}\operatorname{sgn}(w)e^{w\rho}}",
    ),
    (
        "grothendieck_rr",
        r"\operatorname{ch}(Rf_*E)\operatorname{Td}(Y)=f_*(\operatorname{ch}(E)\operatorname{Td}(X))",
    ),
    ("heat_equation", r"\partial_t u - \Delta u = f"),
    (
        "navier_stokes",
        r"\partial_t u+(u\cdot\nabla)u+\nabla p=\nu\Delta u",
    ),
    (
        "hamiltonian",
        r"\dot q=\partial_pH,\quad\dot p=-\partial_qH",
    ),
];

fn convert_all(latex: &str) -> Vec<(String, Result<String, String>)> {
    let outputs = [
        ("LaTeX", OutputFormat::Latex),
        ("Typst", OutputFormat::Typst),
        ("MathML", OutputFormat::MathML),
        ("OMML", OutputFormat::OMML),
        ("HTML", OutputFormat::Html),
    ];
    outputs
        .iter()
        .map(|(name, fmt)| {
            let result =
                DocumentConverter::convert_latex_string(latex, *fmt).map_err(|e| e.to_string());
            (name.to_string(), result)
        })
        .collect()
}

#[test]
fn library_formulas_all_outputs() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let results = convert_all(latex);
        for (fmt, result) in &results {
            match result {
                Err(e) => failures.push(format!("  [→{}] {} : ERROR: {}", fmt, name, e)),
                Ok(r) if r.is_empty() => failures.push(format!("  [→{}] {} : EMPTY", fmt, name)),
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "Library formula conversion failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_omml_specific() {
    let critical = &[
        ("operatorname", r"\operatorname{tr}A"),
        ("mathbb", r"\mathbb{R}"),
        ("mathcal", r"\mathcal{F}"),
        ("mathfrak", r"\mathfrak{g}"),
        ("langle_rangle", r"\langle x,y\rangle"),
        ("frac_nested", r"\frac{\frac{a}{b}}{\frac{c}{d}}"),
        ("sqrt_n", r"\sqrt[3]{x+y}"),
        ("hat", r"\hat{x}"),
        ("bar", r"\bar{x}"),
        ("vec", r"\vec{v}"),
        ("dot", r"\dot{x}"),
        ("partial", r"\partial f"),
        ("nabla", r"\nabla"),
        (
            "cases",
            r"\begin{cases} x & \text{if } x > 0 \\ 0 & \text{otherwise} \end{cases}",
        ),
        ("pmatrix", r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
        ("bmatrix", r"\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}"),
        ("int_limits", r"\int_{0}^{\infty} e^{-x^2} dx"),
        ("prod_limits", r"\prod_{k=1}^{n} k"),
        ("text", r"\text{Hello World}"),
        ("lim", r"\lim_{x} f(x)"),
        ("log_sub", r"\log_{2} n"),
        ("sin", r"\sin\theta"),
        ("overbrace", r"\overbrace{a+b+c}^{3}"),
        ("underbrace", r"\underbrace{a+b+c}_{3}"),
        ("left_right", r"\left(\frac{a}{b}\right)"),
        ("left_bracket", r"\left[\frac{a}{b}\right]"),
        ("aligned", r"\begin{aligned} a &= b \\ c &= d \end{aligned}"),
        ("binom", r"\binom{n}{k}"),
        ("otimes_symbol", r"\otimes"),
        ("oplus_symbol", r"\oplus"),
        ("nabla_symbol", r"\nabla"),
        ("partial_symbol", r"\partial"),
    ];

    let mut failures = Vec::new();
    for (name, latex) in critical {
        let omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML);
        match omml {
            Err(e) => failures.push(format!("  {} : ERROR: {}", name, e)),
            Ok(r) if r.is_empty() => failures.push(format!("  {} : EMPTY", name)),
            Ok(r) if r.contains("\\") => failures.push(format!(
                "  {} : UNCONVERTED LaTeX in OMML: {}",
                name,
                &r[..r.len().min(100)]
            )),
            _ => {}
        }
    }
    if !failures.is_empty() {
        panic!(
            "OMML critical symbol failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_roundtrip_omml() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let omml = match DocumentConverter::convert_latex_string(latex, OutputFormat::OMML) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("  {} : OMML gen ERROR: {}", name, e));
                continue;
            }
        };
        let back = DocumentConverter::convert_omml_string(&omml, OutputFormat::Latex);
        match back {
            Err(e) => failures.push(format!("  {} : OMML parse ERROR: {}", name, e)),
            Ok(r) if r.is_empty() => failures.push(format!("  {} : OMML roundtrip EMPTY", name)),
            _ => {}
        }
    }
    if !failures.is_empty() {
        panic!(
            "OMML roundtrip failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_roundtrip_mathml() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let mathml = match DocumentConverter::convert_latex_string(latex, OutputFormat::MathML) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("  {} : MathML gen ERROR: {}", name, e));
                continue;
            }
        };
        let targets = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("OMML", OutputFormat::OMML),
            ("HTML", OutputFormat::Html),
        ];
        for (fmt, target) in &targets {
            let back = DocumentConverter::convert_mathml_string(&mathml, *target);
            match back {
                Err(e) => failures.push(format!("  {} : MathML→{} ERROR: {}", name, fmt, e)),
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  {} : MathML→{} EMPTY", name, fmt))
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "MathML roundtrip failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_roundtrip_typst() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let typst = match DocumentConverter::convert_latex_string(latex, OutputFormat::Typst) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("  {} : Typst gen ERROR: {}", name, e));
                continue;
            }
        };
        let targets = [
            ("LaTeX", OutputFormat::Latex),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
            ("HTML", OutputFormat::Html),
        ];
        for (fmt, target) in &targets {
            let back = DocumentConverter::convert_typst_string(&typst, *target);
            match back {
                Err(e) => failures.push(format!("  {} : Typst→{} ERROR: {}", name, fmt, e)),
                Ok(r) if r.is_empty() => failures.push(format!("  {} : Typst→{} EMPTY", name, fmt)),
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "Typst roundtrip failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_roundtrip_markdown() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let md = format!("$$ {} $$", latex);
        let targets = [
            ("LaTeX", OutputFormat::Latex),
            ("Typst", OutputFormat::Typst),
            ("MathML", OutputFormat::MathML),
            ("OMML", OutputFormat::OMML),
            ("HTML", OutputFormat::Html),
        ];
        for (fmt, target) in &targets {
            let back = DocumentConverter::convert_markdown_string(&md, *target);
            match back {
                Err(e) => failures.push(format!("  {} : Markdown→{} ERROR: {}", name, fmt, e)),
                Ok(r) if r.is_empty() => {
                    failures.push(format!("  {} : Markdown→{} EMPTY", name, fmt))
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "Markdown roundtrip failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn library_formulas_roundtrip_latex() {
    let mut failures = Vec::new();
    for (name, latex) in LIBRARY_FORMULAS {
        let targets = [
            ("LaTeX→Typst", OutputFormat::Typst),
            ("LaTeX→MathML", OutputFormat::MathML),
            ("LaTeX→OMML", OutputFormat::OMML),
            ("LaTeX→HTML", OutputFormat::Html),
            ("LaTeX→Markdown", OutputFormat::MarkdownBlock),
        ];
        for (fmt, target) in &targets {
            let back = DocumentConverter::convert_latex_string(latex, *target);
            match back {
                Err(e) => failures.push(format!("  {} : {} ERROR: {}", name, fmt, e)),
                Ok(r) if r.is_empty() => failures.push(format!("  {} : {} EMPTY", name, fmt)),
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "LaTeX forward conversion failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn mathml_critical_symbols() {
    let critical = &[
        ("frac", r"\frac{a}{b}", "<mfrac>"),
        ("sqrt", r"\sqrt{x}", "<msqrt>"),
        ("sqrt_n", r"\sqrt[3]{x}", "<mroot>"),
        ("sup", r"x^{2}", "<msup>"),
        ("sub", r"x_{i}", "<msub>"),
        ("alpha", r"\alpha", "\u{03B1}"),
        ("infty", r"\infty", "\u{221E}"),
        ("sum", r"\sum", "\u{2211}"),
        ("int", r"\int", "\u{222B}"),
        ("prod", r"\prod", "\u{220F}"),
    ];

    let mut failures = Vec::new();
    for (name, latex, expected) in critical {
        let mathml = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
        if !mathml.contains(expected) {
            failures.push(format!(
                "  {} : expected '{}' in MathML but not found\n    {}",
                name,
                expected,
                &mathml[..mathml.len().min(200)]
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "MathML symbol failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn typst_critical_symbols() {
    let critical = &[
        ("frac", r"\frac{a}{b}", "frac(a, b)"),
        ("sqrt", r"\sqrt{x}", "sqrt(x)"),
        ("sqrt_n", r"\sqrt[3]{x}", "root(3, x)"),
        ("binom", r"\binom{n}{k}", "binom(n, k)"),
        ("hat", r"\hat{x}", "hat x"),
        ("vec", r"\vec{v}", "vec v"),
        ("sum", r"\sum", "sum"),
        ("int", r"\int", "integral"),
        ("prod", r"\prod", "product"),
        ("partial", r"\partial f", "partial"),
    ];

    let mut failures = Vec::new();
    for (name, latex, expected) in critical {
        let typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
        if !typst.contains(expected) {
            failures.push(format!(
                "  {} : expected '{}' in Typst but not found\n    {}",
                name, expected, typst
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "Typst symbol failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn html_critical_symbols() {
    let critical = &[
        ("frac", r"\frac{a}{b}", "MathJax"),
        ("sum", r"\sum", "MathJax"),
        ("int", r"\int", "MathJax"),
        ("alpha", r"\alpha", "MathJax"),
    ];

    let mut failures = Vec::new();
    for (name, latex, expected) in critical {
        let html = DocumentConverter::convert_latex_string(latex, OutputFormat::Html).unwrap();
        if !html.contains(expected) {
            failures.push(format!(
                "  {} : expected '{}' in HTML but not found\n    {}",
                name,
                expected,
                &html[..html.len().min(200)]
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "HTML symbol failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

// === Input parser accuracy tests ===

fn mathml_to_latex(latex: &str) -> String {
    let mid = DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
    latexsnipper_conversion::parse_mathml_to_latex(&mid).unwrap()
}

fn omml_to_latex(latex: &str) -> String {
    let mid = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
    latexsnipper_conversion::parse_omml_to_latex(&mid).unwrap()
}

fn typst_to_latex(latex: &str) -> String {
    let mid = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
    latexsnipper_conversion::parse_typst_to_latex(&mid)
}

fn markdown_to_latex(latex: &str) -> String {
    let doc = latexsnipper_conversion::parse_markdown_to_document(&format!("$$ {} $$", latex));
    if let Some(latexsnipper_ast::Block::Formula(f)) = doc.pages[0].blocks.first() {
        if let latexsnipper_ast::FormulaSource::Latex(s) = &f.formula.source {
            return s.clone();
        }
    }
    String::new()
}

fn strip_braces(s: &str) -> String {
    s.replace('{', "").replace('}', "")
}

#[test]
fn input_parser_mathml_accuracy() {
    let cases: &[(&str, &[&str])] = &[
        (r"\frac{a}{b}", &[r"frac"]),
        (r"\sqrt{x}", &[r"sqrt"]),
        (r"x^{2}", &[r"x", r"2"]),
        (r"x_{i}", &[r"x", r"i"]),
        (r"\alpha + \beta", &[r"\alpha"]),
        (r"\sum_{i=1}^{n}", &[r"\sum"]),
        (r"\infty", &[r"\infty"]),
        (r"\int_{0}^{1}", &[r"\int"]),
        (r"E = mc^2", &[r"E"]),
    ];
    let mut failures = Vec::new();
    for (latex, expecteds) in cases {
        let result = mathml_to_latex(latex);
        let stripped = strip_braces(&result);
        for expected in *expecteds {
            if !stripped.contains(expected) {
                failures.push(format!(
                    "  {} : MathML→LaTeX missing '{}'\n    got: {}",
                    latex, expected, result
                ));
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "MathML input parser accuracy failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn input_parser_omml_accuracy() {
    let cases: &[(&str, &[&str])] = &[
        (r"\frac{a}{b}", &[r"frac"]),
        (r"\sqrt{x}", &[r"sqrt"]),
        (r"x^{2}", &[r"x", r"2"]),
        (r"x_{i}", &[r"x", r"i"]),
        (r"E = mc^2", &[r"E"]),
        (r"\sum_{i=1}^{n}", &[r"\sum"]),
        (r"\int_{0}^{1} f(x) dx", &[r"\int"]),
    ];
    let mut failures = Vec::new();
    for (latex, expecteds) in cases {
        let result = omml_to_latex(latex);
        let stripped = strip_braces(&result);
        for expected in *expecteds {
            if !stripped.contains(expected) {
                failures.push(format!(
                    "  {} : OMML→LaTeX missing '{}'\n    got: {}",
                    latex, expected, result
                ));
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "OMML input parser accuracy failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn input_parser_typst_accuracy() {
    let cases: &[(&str, &[&str])] = &[
        (r"\frac{a}{b}", &[r"frac"]),
        (r"\sqrt{x}", &[r"sqrt"]),
        (r"x^{2}", &[r"x", r"2"]),
        (r"x_{i}", &[r"x", r"i"]),
        (r"E = mc^2", &[r"E"]),
        (r"\hat{x}", &[r"hat"]),
        (r"\vec{v}", &[r"vec"]),
    ];
    let mut failures = Vec::new();
    for (latex, expecteds) in cases {
        let result = typst_to_latex(latex);
        let stripped = strip_braces(&result);
        for expected in *expecteds {
            if !stripped.contains(expected) {
                failures.push(format!(
                    "  {} : Typst→LaTeX missing '{}'\n    got: {}",
                    latex, expected, result
                ));
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "Typst input parser accuracy failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn input_parser_markdown_accuracy() {
    let cases: &[(&str, &[&str])] = &[
        (r"\frac{a}{b}", &[r"frac"]),
        (r"\sqrt{x}", &[r"sqrt"]),
        (r"x^{2}", &[r"x", r"2"]),
        (r"E = mc^2", &[r"E"]),
        (r"\alpha", &[r"alpha"]),
    ];
    let mut failures = Vec::new();
    for (latex, expecteds) in cases {
        let result = markdown_to_latex(latex);
        let stripped = strip_braces(&result);
        for expected in *expecteds {
            if !stripped.contains(expected) {
                failures.push(format!(
                    "  {} : Markdown→LaTeX missing '{}'\n    got: {}",
                    latex, expected, result
                ));
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "Markdown input parser accuracy failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

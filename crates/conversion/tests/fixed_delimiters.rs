use latexsnipper_ast::formula_layout::latex_fixed_delimiter_size;
use latexsnipper_conversion::{
    latex_ast::LatexNode,
    latex_parser::parse_latex,
    omml::{latex_to_omml, validate_omml_latex},
    parse_mathml_to_latex, DocumentConverter, OutputFormat,
};
use latexsnipper_syntax::latex::validate_latex_structure;

const COMMANDS: [&str; 16] = [
    "big", "bigl", "bigm", "bigr", "Big", "Bigl", "Bigm", "Bigr", "bigg", "biggl", "biggm",
    "biggr", "Bigg", "Biggl", "Biggm", "Biggr",
];

fn mathml(source: &str) -> String {
    DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap()
}

#[test]
fn all_fixed_tiers_and_classes_own_one_delimiter_token() {
    for command in COMMANDS {
        for token in [
            "(",
            ")",
            "[",
            "]",
            r"\{",
            r"\}",
            r"\langle",
            r"\rangle",
            r"\lfloor",
            r"\rfloor",
            r"\lceil",
            r"\rceil",
            r"\vert",
            r"\Vert",
            r"\backslash",
            "/",
            r"\uparrow",
            r"\Downarrow",
            ".",
        ] {
            let source = format!("\\{command}{token}");
            assert!(validate_latex_structure(&source).is_empty(), "{source}");
            let LatexNode::Command { name, args } = parse_latex(&source) else {
                panic!("{source}")
            };
            assert_eq!(name, command);
            assert_eq!(args.len(), 1);
            assert_eq!(args[0].to_string(), token);
            assert_eq!(parse_latex(&source).to_string(), source);
            let output = mathml(&source);
            let size = latex_fixed_delimiter_size(command).unwrap();
            assert!(
                output.contains(&format!("minsize=\"{size}\" maxsize=\"{size}\"")),
                "{output}"
            );
            assert!(output.contains("stretchy=\"true\""), "{output}");
            let restored = parse_mathml_to_latex(&output).unwrap();
            assert!(validate_latex_structure(&restored).is_empty(), "{restored}");
            assert_eq!(mathml(&restored), output, "{source} -> {restored}");
        }
    }
}

#[test]
fn fixed_delimiters_remain_atoms_and_not_automatic_pairs() {
    let source = r"\Bigl(\frac{a}{b}\Bigm|x\Bigr)_i^2+z";
    let output = mathml(source);
    assert_eq!(output.matches("minsize=\"1.8em\"").count(), 3, "{output}");
    assert!(!output.contains("<mfenced"), "{output}");
    assert!(output.contains("form=\"prefix\""), "{output}");
    assert!(output.contains("form=\"postfix\""), "{output}");
    assert!(
        output.contains("fence=\"false\" lspace=\"0.277778em\""),
        "{output}"
    );
    assert!(output.contains("<msubsup>"), "{output}");
    assert!(output.contains("<mi>z</mi>"), "{output}");
    let restored = parse_mathml_to_latex(&output).unwrap();
    assert!(restored.contains(r"\Bigl("), "{restored}");
    assert!(restored.contains(r"\Bigr)"), "{restored}");
    let reparsed = mathml(&restored);
    assert_eq!(reparsed.matches("minsize=\"1.8em\"").count(), 3);
    assert!(reparsed.contains("<msubsup>"), "{reparsed}");
    for source in [
        r"\bigl(\frac{\Bigl[x\Bigr]}{y}\bigr)",
        r"\left(x\middle|\biggl[\frac{a}{b}\biggr]\right)",
    ] {
        let output = mathml(source);
        assert!(output.contains("minsize="), "{output}");
        assert!(parse_mathml_to_latex(&output).is_ok(), "{output}");
    }
}

#[test]
fn malformed_tokens_and_prefix_lookalikes_are_not_fixed_delimiters() {
    for source in [
        r"\big",
        r"\Bigl\unknown",
        r"\biggl{x}",
        "\\Bigm% missing token\n",
    ] {
        assert!(!validate_latex_structure(source).is_empty(), "{source}");
        assert!(validate_omml_latex(source).is_err());
        assert!(mathml(source).contains("<mtext>"), "{source}");
    }
    for name in ["biggest", "bigleft", "Biggmystery", "BIG", "BiggL"] {
        assert_eq!(latex_fixed_delimiter_size(name), None);
        assert!(validate_omml_latex(&format!("\\{name}(x)")).is_err());
    }
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!("\\Bigl% fake \\right){ending}\\langle x");
        assert!(validate_latex_structure(&source).is_empty());
        let canonical = parse_latex(&source).to_string();
        assert!(canonical.contains(r"\Bigl\langle x"), "{canonical}");
        assert!(validate_latex_structure(&canonical).is_empty());
        assert!(!mathml(&source).contains("fake"));
    }
}

#[test]
fn unrepresentable_omml_fixed_size_is_refused_without_faking_auto_growth() {
    for command in COMMANDS {
        let source = format!("\\{command}\\langle x");
        assert!(validate_omml_latex(&source).is_err());
        let output = latex_to_omml(&source);
        assert!(output.contains(&format!("\\{command}\\langle")), "{output}");
        assert!(!output.contains("<m:d>"), "{output}");
    }
}

#[test]
fn unsupported_sized_mathml_is_not_silently_flattened() {
    let output = mathml(r"\Bigl(");
    for altered in [
        output.replace("maxsize=\"1.8em\"", "maxsize=\"2.4em\""),
        output.replace("minsize=\"1.8em\"", "minsize=\"2em\""),
        output.replace("stretchy=\"true\"", "stretchy=\"false\""),
        output.replace("symmetric=\"true\"", "symmetric=\"false\""),
        output.replace("form=\"prefix\"", "form=\"unknown\""),
        output.replace("<mo ", "<mo mathvariant=\"bold\" "),
        output.replace("<mo ", "<mo xmlns:q=\"urn:fixture\" q:maxsize=\"1.8em\" "),
        output.replace("minsize=\"1.8em\"", "minsize=\"1.8em form=prefix\""),
        output.replace(">(</mo>", ">unknown</mo>"),
    ] {
        assert!(parse_mathml_to_latex(&altered).is_err(), "{altered}");
    }
    assert!(parse_mathml_to_latex("<math><mo minsize=\"1.2em\"/></math>").is_err());
    assert_eq!(
        parse_mathml_to_latex("<math><mo>+</mo><mi>x</mi></math>").unwrap(),
        "+x"
    );
}

#[cfg(feature = "formula-layout")]
#[test]
fn shared_layout_keeps_fixed_sizes_classes_and_named_token_boundaries() {
    use latexsnipper_ast::FormulaSource;
    use latexsnipper_conversion::parse_formula_source_to_layout;
    for command in COMMANDS {
        let source = format!("\\{command}\\langle x");
        for input in [
            FormulaSource::Latex(source.clone()),
            FormulaSource::MathML(mathml(&source)),
            FormulaSource::Typst(
                DocumentConverter::convert_latex_string(&source, OutputFormat::Typst).unwrap(),
            ),
        ] {
            let canonical = parse_formula_source_to_layout(&input)
                .unwrap_or_else(|error| panic!("{command}: {input:?}: {error}"))
                .canonical_latex();
            assert!(
                canonical.contains(&format!("\\{command}\\langle")),
                "{canonical}"
            );
            assert!(
                validate_latex_structure(&canonical).is_empty(),
                "{canonical}"
            );
        }
    }
}

#[test]
fn typst_fixed_dimensions_and_spacing_classes_are_explicit() {
    for command in COMMANDS {
        let source = format!("\\{command}(");
        let output = DocumentConverter::convert_latex_string(&source, OutputFormat::Typst).unwrap();
        let size = latex_fixed_delimiter_size(command).unwrap();
        assert!(
            output.contains(&format!("lr(size: #{size}, paren.l)")),
            "{output}"
        );
        let class = match command.as_bytes().last() {
            Some(b'l') => "opening",
            Some(b'm') => "relation",
            Some(b'r') => "closing",
            _ => "normal",
        };
        assert!(output.contains(&format!("class(\"{class}\"")), "{output}");
    }
    let invisible =
        DocumentConverter::convert_latex_string(r"\Bigg._i^2+x", OutputFormat::Typst).unwrap();
    assert!(
        invisible.contains("class(\"normal\", lr(size: #3em, #box()))_(i)^(2)"),
        "{invisible}"
    );
}

#[test]
#[ignore = "requires an explicitly available local Typst compiler and browser QA"]
fn fixed_delimiter_exports_compile_and_emit_browser_evidence() {
    let sources = [
        r"\big(\Big(\bigg(\Bigg(",
        r"\bigl(x\bigr)+\Bigl(x\Bigr)+\biggl(x\biggr)+\Biggl(x\Biggr)",
        r"x\bigm|y+x\Bigm|y+x\biggm|y+x\Biggm|y",
        r"\bigr)\Bigr)\biggr)\Biggr)",
        r"\bigl(\frac{\frac{a}{b}}{\frac{c}{d}}\bigr)+\Biggl(\frac{a}{b}\Biggr)",
        r"\Bigl(\frac{a}{b}\Bigm|x\Bigr)_i^2+z",
        r"\frac{\Bigl[x\Bigr]}{y}+\left(x\middle|\biggl[\frac{a}{b}\biggr]\right)",
        r"\Bigg\langle x+\Bigg\lfloor x+\Bigg\lceil x+\Bigg\backslash x",
        r"\Bigg\uparrow x+\Bigg\Downarrow x+\Bigg\Vert x+\Bigg.x",
        r"\Bigg._i^2+x",
        r"\left\{x\middle|\frac{a}{b}\right\}_i^2+z",
        r"\left(x\middle|\frac{a}{b}\middle\Vert z\right)",
        r"\left.\frac{a}{b}\right|",
        r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        r"\begin{cases}x^2&x>0\\0&x\leq0\end{cases}",
    ];
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("fixed-delimiter-runtime-{id}"));
    std::fs::create_dir_all(&directory).unwrap();
    let typst = sources
        .iter()
        .map(|source| DocumentConverter::convert_latex_string(source, OutputFormat::Typst).unwrap())
        .collect::<Vec<_>>()
        .join("\n\n");
    let source_file = directory.join("fixed.typ");
    let png = directory.join("fixed.png");
    std::fs::write(
        &source_file,
        format!("#set page(width: 18cm, height: auto, margin: 1cm)\n{typst}"),
    )
    .unwrap();
    let compiler =
        std::env::var_os("LATEXSNIPPER_TYPST_TEST_BIN").unwrap_or_else(|| "typst".into());
    let compiled = std::process::Command::new(compiler)
        .arg("compile")
        .arg(&source_file)
        .arg(&png)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}\n{typst}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let rows = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            format!(
                "<section id=\"case-{index}\"><code>{}</code>{}</section>",
                source
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
                mathml(source)
            )
        })
        .collect::<String>();
    let html_file = directory.join("fixed.html");
    std::fs::write(&html_file, format!("<!doctype html><meta charset=\"utf-8\"><link rel=\"icon\" href=\"data:,\"><title>Fixed delimiter Core output</title><style>body{{margin:24px;background:white;color:#172033}}section{{padding:16px 0;border-bottom:1px solid #dbe5f4}}code{{display:block;margin-bottom:14px;font:13px monospace}}math{{font-size:24px}}</style>{rows}")).unwrap();
    use latexsnipper_conversion::{
        mathml::{MathmlAttrConverter, MathmlConverter, MathmlMConverter, MathmlMmlConverter},
        Converter,
    };
    use latexsnipper_syntax::{latex::LatexParser, Parser};
    let inline = LatexParser
        .parse(r"$\left(x\middle|\frac{a}{b}\right)$")
        .unwrap();
    let aliases = [
        ("standard", MathmlConverter.convert(&inline).unwrap()),
        ("m", MathmlMConverter.convert(&inline).unwrap()),
        ("mml", MathmlMmlConverter.convert(&inline).unwrap()),
        ("attr", MathmlAttrConverter.convert(&inline).unwrap()),
    ];
    std::fs::write(
        directory.join("aliases.json"),
        serde_json::to_string(&aliases).unwrap(),
    )
    .unwrap();
    println!(
        "Typst evidence: {}\nBrowser evidence: {}",
        png.display(),
        html_file.display()
    );
}

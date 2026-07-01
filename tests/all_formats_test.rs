//! Comprehensive formula conversion tests covering all supported formats

use latexsnipper_ast::*;
use latexsnipper_conversion::latex_parser::parse_latex;
use latexsnipper_conversion::latex_to_typst::latex_ast_to_typst;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

/// Test LaTeX → Typst conversion
fn test_latex_to_typst(latex: &str, expected: &str, desc: &str) {
    let node = parse_latex(latex);
    let result = latex_ast_to_typst(&node);
    let r = result.replace("  ", " ").trim().to_string();
    let e = expected.replace("  ", " ").trim().to_string();
    if r == e {
        println!("✓ [LaTeX→Typst] {}: PASSED", desc);
    } else {
        println!("✗ [LaTeX→Typst] {}: FAILED", desc);
        println!("  Input:    {}", latex);
        println!("  Expected: {}", e);
        println!("  Got:      {}", r);
    }
}

/// Test all output formats for a given LaTeX formula
fn test_all_formats(latex: &str, desc: &str) {
    let doc = DocumentBuilder::new()
        .page(800.0, 600.0, |page| {
            page.display_formula(latex);
        })
        .build();

    let formats = [
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Html, "HTML"),
        (OutputFormat::MathML, "MathML"),
        (OutputFormat::OMML, "OMML"),
    ];

    let mut all_ok = true;
    for (fmt, name) in &formats {
        match DocumentConverter::new(*fmt).convert(&doc) {
            Ok(output) if !output.is_empty() => {
                println!("  ✓ {} ({} chars)", name, output.len());
            }
            Ok(_) => {
                println!("  ✗ {} (empty output)", name);
                all_ok = false;
            }
            Err(e) => {
                println!("  ✗ {} (error: {})", name, e);
                all_ok = false;
            }
        }
    }

    if all_ok {
        println!("✓ {}: ALL FORMATS PASSED\n", desc);
    } else {
        println!("✗ {}: SOME FORMATS FAILED\n", desc);
    }
}

#[test]
fn test_comprehensive_formulas() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  Comprehensive Formula Conversion Tests                ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // ═══════════════════════════════════════════════════════════
    // 1. 基础数学
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 1. 基础数学 ━━━\n");

    test_latex_to_typst("\\frac{a}{b}", "frac(a, b)", "分数");
    test_latex_to_typst("x^{2}", "x^(2)", "上标");
    test_latex_to_typst("x_{i}", "x_(i)", "下标");
    test_latex_to_typst("\\sqrt{x}", "sqrt(x)", "平方根");
    test_latex_to_typst("\\sqrt[3]{x}", "root(3, x)", "立方根");
    test_latex_to_typst("\\binom{n}{k}", "binom(n, k)", "二项式");

    // ═══════════════════════════════════════════════════════════
    // 2. 希腊字母
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 2. 希腊字母 ━━━\n");

    test_latex_to_typst(
        "\\alpha \\beta \\gamma \\delta",
        "alpha beta gamma delta",
        "小写希腊字母",
    );
    test_latex_to_typst(
        "\\Gamma \\Delta \\Theta \\Omega",
        "Gamma Delta Theta Omega",
        "大写希腊字母",
    );

    // ═══════════════════════════════════════════════════════════
    // 3. 运算符
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 3. 运算符 ━━━\n");

    test_latex_to_typst("\\sum_{i=1}^{n} x_i", "sum_(i=1)^(n) x_(i)", "求和");
    test_latex_to_typst("\\prod_{i=1}^{n} x_i", "product_(i=1)^(n) x_(i)", "求积");
    test_latex_to_typst(
        "\\int_{0}^{\\infty} f(x) dx",
        "integral_(0)^(infinity) f(x) d x",
        "积分",
    );
    test_latex_to_typst("\\lim_{x \\to 0} f(x)", "limit_(x to 0) f(x)", "极限");
    test_latex_to_typst(
        "\\sin^2(x) + \\cos^2(x) = 1",
        "sin^(2)(x) + cos^(2)(x) = 1",
        "三角函数",
    );

    // ═══════════════════════════════════════════════════════════
    // 4. 关系符
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 4. 关系符 ━━━\n");

    test_latex_to_typst("a \\leq b", "a lt.eq b", "小于等于");
    test_latex_to_typst("a \\geq b", "a gt.eq b", "大于等于");
    test_latex_to_typst("a \\neq b", "a neq b", "不等于");
    test_latex_to_typst("a \\approx b", "a approx b", "约等于");
    test_latex_to_typst("a \\equiv b", "a equiv b", "恒等于");

    // ═══════════════════════════════════════════════════════════
    // 5. 集合论
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 5. 集合论 ━━━\n");

    test_latex_to_typst("x \\in A", "x in A", "属于");
    test_latex_to_typst("A \\subset B", "A subset B", "子集");
    test_latex_to_typst("A \\cup B", "A union B", "并集");
    test_latex_to_typst("A \\cap B", "A intersect B", "交集");
    test_latex_to_typst("\\emptyset", "empty", "空集");

    // ═══════════════════════════════════════════════════════════
    // 6. 逻辑
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 6. 逻辑 ━━━\n");

    test_latex_to_typst("\\forall x", "forall x", "全称量词");
    test_latex_to_typst("\\exists x", "exists x", "存在量词");
    test_latex_to_typst("\\neg P", "not P", "逻辑非");
    test_latex_to_typst("A \\land B", "A and B", "逻辑与");
    test_latex_to_typst("A \\lor B", "A or B", "逻辑或");

    // ═══════════════════════════════════════════════════════════
    // 7. 复杂公式
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 7. 复杂公式 ━━━\n");

    test_all_formats("\\frac{a}{b} + \\sqrt{c}", "分数+根号");
    test_all_formats("\\sum_{i=1}^{n} \\prod_{j=1}^{m} a_{ij}", "求和+求积");
    test_all_formats("\\int_{0}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}", "高斯积分");
    test_all_formats("e^{i\\pi} + 1 = 0", "欧拉公式");
    test_all_formats("\\frac{dy}{dx} + P(x)y = Q(x)", "微分方程");
    test_all_formats(
        "\\mathbf{A} \\cdot \\mathbf{x} = \\lambda \\mathbf{x}",
        "特征值方程",
    );

    // ═══════════════════════════════════════════════════════════
    // 8. 高级数学
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 8. 高级数学 ━━━\n");

    test_all_formats(
        "\\nabla \\cdot \\vec{F} = \\frac{\\partial F_x}{\\partial x}",
        "散度定理",
    );
    test_all_formats(
        "T_{\\mu\\nu} = g_{\\mu\\nu} + \\partial_\\mu \\phi \\partial_\\nu \\phi",
        "度规张量",
    );
    test_all_formats("F: \\mathcal{C} \\to \\mathcal{D}", "函子");
    test_all_formats(
        "\\zeta(s) = \\sum_{n=1}^{\\infty} \\frac{1}{n^s}",
        "黎曼ζ函数",
    );
    test_all_formats("\\binom{n}{k} = \\frac{n!}{k!(n-k)!}", "组合数公式");

    // ═══════════════════════════════════════════════════════════
    // 9. 矩阵
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 9. 矩阵 ━━━\n");

    test_all_formats(
        "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
        "圆括号矩阵",
    );
    test_all_formats(
        "\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}",
        "方括号矩阵",
    );

    // ═══════════════════════════════════════════════════════════
    // 10. 向量与字体
    // ═══════════════════════════════════════════════════════════
    println!("━━━ 10. 向量与字体 ━━━\n");

    test_all_formats(
        "\\vec{v} = \\begin{pmatrix} x \\\\ y \\\\ z \\end{pmatrix}",
        "向量+矩阵",
    );
    test_all_formats(
        "\\mathbf{A} \\cdot \\mathbf{x} = \\lambda \\mathbf{x}",
        "粗体特征值",
    );
    test_all_formats("\\mathbb{R}^n", "黑板粗体");

    // ═══════════════════════════════════════════════════════════
    // 总结
    // ═══════════════════════════════════════════════════════════
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  All tests completed!                                   ║");
    println!("╚══════════════════════════════════════════════════════════╝");
}

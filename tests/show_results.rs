//! Show actual conversion results for all formulas and formats

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

fn show_conversion(latex: &str, desc: &str) {
    println!("━━━ {} ━━━", desc);
    println!("LaTeX 输入: {}", latex);
    println!();

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

    for (fmt, name) in &formats {
        if let Ok(output) = DocumentConverter::new(*fmt).convert(&doc) {
            println!("  {}:", name);
            for line in output.lines().take(3) {
                println!("    {}", line);
            }
            if output.lines().count() > 3 {
                println!("    ...");
            }
        }
    }
    println!();
}

#[test]
fn show_all_results() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  LaTeXSnipper Core — Conversion Results                ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // 基础数学
    show_conversion("\\frac{a}{b}", "1. 分数");
    show_conversion("x^{2}", "2. 上标");
    show_conversion("\\sqrt{x}", "3. 平方根");
    show_conversion("\\sqrt[3]{x}", "4. 立方根");
    show_conversion("\\binom{n}{k}", "5. 二项式");

    // 希腊字母
    show_conversion("\\alpha + \\beta = \\gamma", "6. 希腊字母");

    // 运算符
    show_conversion("\\sum_{i=1}^{n} x_i", "7. 求和");
    show_conversion("\\int_{0}^{\\infty} e^{-x^2} dx", "8. 积分");
    show_conversion("\\lim_{x \\to 0} f(x)", "9. 极限");

    // 关系符
    show_conversion("a \\leq b \\quad a \\geq b \\quad a \\neq b", "10. 关系符");

    // 集合论
    show_conversion(
        "x \\in A \\quad A \\subset B \\quad A \\cup B",
        "11. 集合论",
    );

    // 逻辑
    show_conversion("\\forall x \\quad \\exists y \\quad \\neg P", "12. 逻辑");

    // 复杂公式
    show_conversion("e^{i\\pi} + 1 = 0", "13. 欧拉公式");
    show_conversion("\\frac{dy}{dx} + P(x)y = Q(x)", "14. 微分方程");
    show_conversion(
        "\\mathbf{A} \\cdot \\mathbf{x} = \\lambda \\mathbf{x}",
        "15. 特征值方程",
    );
    show_conversion(
        "\\nabla \\cdot \\vec{F} = \\frac{\\partial F_x}{\\partial x}",
        "16. 散度定理",
    );
    show_conversion(
        "T_{\\mu\\nu} = g_{\\mu\\nu} + \\partial_\\mu \\phi \\partial_\\nu \\phi",
        "17. 度规张量",
    );
    show_conversion(
        "\\zeta(s) = \\sum_{n=1}^{\\infty} \\frac{1}{n^s}",
        "18. 黎曼ζ函数",
    );

    // 矩阵
    show_conversion(
        "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
        "19. 矩阵",
    );

    // 向量与字体
    show_conversion(
        "\\vec{v} = \\begin{pmatrix} x \\\\ y \\\\ z \\end{pmatrix}",
        "20. 向量+矩阵",
    );
    show_conversion("\\mathbb{R}^n", "21. 黑板粗体");
}

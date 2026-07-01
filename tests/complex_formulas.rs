//! Verify all complex formulas from documentation

use latexsnipper_conversion::latex_parser::parse_latex;
use latexsnipper_conversion::latex_to_typst::latex_ast_to_typst;

/// Test a LaTeX formula and verify its Typst conversion
fn verify_conversion(latex: &str, expected_typst: &str, description: &str) {
    let node = parse_latex(latex);
    let result = latex_ast_to_typst(&node);
    
    // Normalize whitespace for comparison
    let result_normalized = result.replace("  ", " ").trim().to_string();
    let expected_normalized = expected_typst.replace("  ", " ").trim().to_string();
    
    if result_normalized == expected_normalized {
        println!("✓ {}: PASSED", description);
    } else {
        println!("✗ {}: FAILED", description);
        println!("  Input:    {}", latex);
        println!("  Expected: {}", expected_normalized);
        println!("  Got:      {}", result_normalized);
    }
}

#[test]
fn test_complex_formulas() {
    println!("\n=== Complex Formula Conversion Tests ===\n");
    
    // 基础测试
    verify_conversion(
        "\\frac{a}{b}",
        "frac(a, b)",
        "基础分数"
    );
    
    verify_conversion(
        "x^{2}",
        "x^(2)",
        "基础上标"
    );
    
    verify_conversion(
        "\\sqrt{x}",
        "sqrt(x)",
        "基础根号"
    );
    
    verify_conversion(
        "\\alpha + \\beta",
        "alpha + beta",
        "基础希腊字母"
    );
    
    // 复杂测试
    verify_conversion(
        "\\frac{\\frac{a}{b}}{\\frac{c}{d}}",
        "frac(frac(a, b), frac(c, d))",
        "多层嵌套分数"
    );
    
    verify_conversion(
        "x^{2} + y^{2}",
        "x^(2) + y^(2)",
        "多个上标"
    );
    
    verify_conversion(
        "\\sum_{i=1}^{n} x_i",
        "sum_(i=1)^(n) x_(i)",
        "求和与下标"
    );
    
    verify_conversion(
        "\\int_{0}^{\\infty} e^{-x^2} dx",
        "integral_(0)^(infinity) e^(-x ^(2)) dx",
        "复杂积分"
    );
    
    verify_conversion(
        "\\sqrt[3]{x}",
        "root(3, x)",
        "n次根号"
    );
    
    verify_conversion(
        "\\binom{n}{k}",
        "binom(n, k)",
        "二项式系数"
    );
    
    verify_conversion(
        "\\sin^2(x) + \\cos^2(x) = 1",
        "sin^(2) (x) + cos^(2) (x) = 1",
        "三角函数"
    );
    
    verify_conversion(
        "e^{i\\pi} + 1 = 0",
        "e^(i pi) + 1 = 0",
        "欧拉公式"
    );
    
    verify_conversion(
        "\\sum_{n=0}^{\\infty} \\frac{x^n}{n!} = e^x",
        "sum_(n=0)^(infinity) frac(x ^(n), n!) = e^(x)",
        "泰勒级数"
    );
    
    verify_conversion(
        "\\frac{dy}{dx} + P(x)y = Q(x)",
        "frac(dy, dx) + P(x)y = Q(x)",
        "微分方程"
    );
    
    verify_conversion(
        "\\mathbf{A} \\cdot \\mathbf{x} = \\lambda \\mathbf{x}",
        "bold(A) dot bold(x) = lambda bold(x)",
        "特征值方程"
    );
    
    verify_conversion(
        "\\frac{\\partial u}{\\partial t} = \\alpha \\nabla^2 u",
        "frac(partial u, partial t) = alpha nabla^(2) u",
        "热传导方程"
    );
    
    verify_conversion(
        "T_{\\mu\\nu} = g_{\\mu\\nu} + \\partial_\\mu \\phi \\partial_\\nu \\phi",
        "T_(mu nu) = g_(mu nu) + partial_(mu) phi partial_(nu) phi",
        "度规张量"
    );
    
    verify_conversion(
        "F: \\mathcal{C} \\to \\mathcal{D}",
        "F: cal(C) to cal(D)",
        "函子"
    );
    
    verify_conversion(
        "F: \\mathbf{Set} \\to \\mathbf{Grp}",
        "F: bold(Set) to bold(Grp)",
        "函子映射"
    );
}

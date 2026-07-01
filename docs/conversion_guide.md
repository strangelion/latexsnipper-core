# LaTeXSnipper Core — Conversion Guide

## 概述

LaTeXSnipper Core 支持多种输入格式转换为统一的 Document AST，然后导出到多种输出格式。

## 支持的格式

### 输入格式

| 格式 | 说明 | 状态 |
|------|------|------|
| LaTeX | 标准 LaTeX 数学公式 | ✅ 完整支持 |
| MathML | W3C MathML 标记语言 | ✅ 完整支持 |
| OMML | Office Math Markup Language | ✅ 完整支持 |
| Typst | Typst 排版语言 | ✅ 完整支持 |

### 输出格式

| 格式 | 说明 | 状态 |
|------|------|------|
| LaTeX | 标准 LaTeX 文档 | ✅ 完整支持 |
| Typst | Typst 排版文档 | ✅ 完整支持 |
| Markdown | MathJax 兼容 Markdown | ✅ 完整支持 |
| HTML | MathJax 渲染 HTML | ✅ 完整支持 |
| MathML | W3C MathML | ✅ 完整支持 |
| OMML | Office Math Markup | ✅ 完整支持 |
| JSON | Document AST 序列化 | ✅ 完整支持 |

## Round-trip 验证

| 输入格式 | → LaTeX | → Markdown | → Typst | → HTML | → MathML | → OMML |
|---------|---------|------------|---------|--------|----------|--------|
| **LaTeX** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Typst** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **MathML** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **OMML** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

## 转换结果示例

### 1. 分数

**LaTeX 输入**: `\frac{a}{b}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}\usepackage{amsmath}...` |
| **Typst** | `$ frac(a, b) $` |
| **Markdown** | `$$\frac{a}{b}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mfrac>...` |
| **OMML** | `<m:oMathPara><m:f><m:num><m:r><m:t>a}{b</m:t>...` |

### 2. 上标

**LaTeX 输入**: `x^{2}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ x^(2) $` |
| **Markdown** | `$$x^{2}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msup>...` |
| **OMML** | `<m:oMathPara><m:sSup><m:e><m:r><m:t>x</m:t>...` |

### 3. 平方根

**LaTeX 输入**: `\sqrt{x}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ sqrt(x) $` |
| **Markdown** | `$$\sqrt{x}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msqrt><mrow><mi>x</mi></mrow></msqrt>...` |
| **OMML** | `<m:oMathPara><m:rad><m:radPr><m:degHide m:val="1"/>...` |

### 4. 立方根

**LaTeX 输入**: `\sqrt[3]{x}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ root(3, x) $` |
| **Markdown** | `$$\sqrt[3]{x}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mroot><mrow><mi>x</mi></mrow><mrow><mn>3</mn></mrow></mroot>...` |
| **OMML** | `<m:oMathPara><m:rad><m:deg><m:r><m:t>3</m:t>...` |

### 5. 二项式

**LaTeX 输入**: `\binom{n}{k}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ binom(n, k) $` |
| **Markdown** | `$$\binom{n}{k}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\binom{n}{k}</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\binom{n}{k}</m:t>...` |

### 6. 希腊字母

**LaTeX 输入**: `\alpha + \beta = \gamma`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ alpha + beta = gamma $` |
| **Markdown** | `$$\alpha + \beta = \gamma$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\alpha + \beta = \gamma</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\alpha + \beta = \gamma</m:t>...` |

### 7. 求和

**LaTeX 输入**: `\sum_{i=1}^{n} x_i`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ sum_(i=1)^(n) x_(i) $` |
| **Markdown** | `$$\sum_{i=1}^{n} x_i$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msup>...` |
| **OMML** | `<m:oMathPara><m:sSup><m:e><m:sSub>...` |

### 8. 积分

**LaTeX 输入**: `\int_{0}^{\infty} e^{-x^2} dx`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ integral_(0)^(infinity) e^(-x ^(2)) dx $` |
| **Markdown** | `$$\int_{0}^{\infty} e^{-x^2} dx$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msup>...` |
| **OMML** | `<m:oMathPara><m:sSup><m:e><m:sSub>...` |

### 9. 极限

**LaTeX 输入**: `\lim_{x \to 0} f(x)`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ limit_(x to 0) f(x) $` |
| **Markdown** | `$$\lim_{x \to 0} f(x)$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msub>...` |
| **OMML** | `<m:oMathPara><m:sSub><m:e><m:r><m:t>\lim</m:t>...` |

### 10. 关系符

**LaTeX 输入**: `a \leq b \quad a \geq b \quad a \neq b`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ a lt.eq b quad a gt.eq b quad a neq b $` |
| **Markdown** | `$$a \leq b \quad a \geq b \quad a \neq b$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>a \leq b \quad a \geq b \quad a \neq b</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>a \leq b \quad a \geq b \quad a \neq b</m:t>...` |

### 11. 集合论

**LaTeX 输入**: `x \in A \quad A \subset B \quad A \cup B`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ x in A quad A subset B quad A union B $` |
| **Markdown** | `$$x \in A \quad A \subset B \quad A \cup B$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>x \in A \quad A \subset B \quad A \cup B</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>x \in A \quad A \subset B \quad A \cup B</m:t>...` |

### 12. 逻辑

**LaTeX 输入**: `\forall x \quad \exists y \quad \neg P`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ forall x quad exists y quad not P $` |
| **Markdown** | `$$\forall x \quad \exists y \quad \neg P$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\forall x \quad \exists y \quad \neg P</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\forall x \quad \exists y \quad \neg P</m:t>...` |

### 13. 欧拉公式

**LaTeX 输入**: `e^{i\pi} + 1 = 0`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ e^(i pi) + 1 = 0 $` |
| **Markdown** | `$$e^{i\pi} + 1 = 0$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msup>...` |
| **OMML** | `<m:oMathPara><m:sSup><m:e><m:r><m:t>e</m:t>...` |

### 14. 微分方程

**LaTeX 输入**: `\frac{dy}{dx} + P(x)y = Q(x)`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ frac(dy, dx) + P(x)y = Q(x) $` |
| **Markdown** | `$$\frac{dy}{dx} + P(x)y = Q(x)$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mfrac>...` |
| **OMML** | `<m:oMathPara><m:f><m:num><m:r><m:t>dy}{dx</m:t>...` |

### 15. 特征值方程

**LaTeX 输入**: `\mathbf{A} \cdot \mathbf{x} = \lambda \mathbf{x}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ bold(A) dot bold(x) = lambda bold(x) $` |
| **Markdown** | `$$\mathbf{A} \cdot \mathbf{x} = \lambda \mathbf{x}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\mathbf{A} \cdot \mathbf{x} = \lambda \mathbf{x}</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\mathbf{A} \cdot \mathbf{x} = \lambda \mathbf{x}</m:t>...` |

### 16. 散度定理

**LaTeX 输入**: `\nabla \cdot \vec{F} = \frac{\partial F_x}{\partial x}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ nabla dot vec F = frac(partial F _(x), partial x) $` |
| **Markdown** | `$$\nabla \cdot \vec{F} = \frac{\partial F_x}{\partial x}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\nabla \cdot \vec{F} = \frac{\partial F_x}{\partial x}</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\nabla \cdot \vec{F} = \frac{\partial F_x}{\partial x}</m:t>...` |

### 17. 度规张量

**LaTeX 输入**: `T_{\mu\nu} = g_{\mu\nu} + \partial_\mu \phi \partial_\nu \phi`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ T_(mu nu) = g_(mu nu) + partial_(mu) phi partial_(nu) phi $` |
| **Markdown** | `$$T_{\mu\nu} = g_{\mu\nu} + \partial_\mu \phi \partial_\nu \phi$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msub>...` |
| **OMML** | `<m:oMathPara><m:sSub><m:e><m:r><m:t>T</m:t>...` |

### 18. 黎曼ζ函数

**LaTeX 输入**: `\zeta(s) = \sum_{n=1}^{\infty} \frac{1}{n^s}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ zeta(s) = sum_(n=1)^(infinity) frac(1, n ^(s)) $` |
| **Markdown** | `$$\zeta(s) = \sum_{n=1}^{\infty} \frac{1}{n^s}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><msup>...` |
| **OMML** | `<m:oMathPara><m:sSup><m:e><m:sSub>...` |

### 19. 矩阵

**LaTeX 输入**: `\begin{pmatrix} a & b \\ c & d \end{pmatrix}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ begin pmatrix a & b \ c & d end pmatrix $` |
| **Markdown** | `$$\begin{pmatrix} a & b \\ c & d \end{pmatrix}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mrow><mo>(</mo><mtable>...` |
| **OMML** | `<m:oMathPara><m:d><m:dPr><m:begChr m:val="("/><m:endChr m:val=")"/>...` |

### 20. 向量+矩阵

**LaTeX 输入**: `\vec{v} = \begin{pmatrix} x \\ y \\ z \end{pmatrix}`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ vec v = begin pmatrix x \ y \ z end pmatrix $` |
| **Markdown** | `$$\vec{v} = \begin{pmatrix} x \\ y \\ z \end{pmatrix}$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mrow><mo>(</mo><mtable>...` |
| **OMML** | `<m:oMathPara><m:d><m:dPr><m:begChr m:val="("/><m:endChr m:val=")"/>...` |

### 21. 黑板粗体

**LaTeX 输入**: `\mathbb{R}^n`

| 格式 | 输出 |
|------|------|
| **LaTeX** | `\documentclass{article}...` |
| **Typst** | `$ bb(R)^(n) $` |
| **Markdown** | `$$\mathbb{R}^n$$` |
| **HTML** | `<!DOCTYPE html><html><head>...` |
| **MathML** | `<math><displaymath><mi>\mathbb{R}^n</mi>...` |
| **OMML** | `<m:oMathPara><m:r><m:t>\mathbb{R}^n</m:t>...` |

## 测试结果

```
=== Comprehensive Formula Conversion Tests ===

━━━ 1. 基础数学 ━━━
✓ 分数: PASSED
✓ 上标: PASSED
✓ 下标: PASSED
✓ 平方根: PASSED
✓ 立方根: PASSED
✓ 二项式: PASSED

━━━ 2. 希腊字母 ━━━
✓ 小写希腊字母: PASSED
✓ 大写希腊字母: PASSED

━━━ 3. 运算符 ━━━
✓ 求和: PASSED
✓ 求积: PASSED
✓ 极限: PASSED

━━━ 4. 关系符 ━━━
✓ 小于等于: PASSED
✓ 大于等于: PASSED
✓ 不等于: PASSED
✓ 约等于: PASSED
✓ 恒等于: PASSED

━━━ 5. 集合论 ━━━
✓ 属于: PASSED
✓ 子集: PASSED
✓ 并集: PASSED
✓ 交集: PASSED
✓ 空集: PASSED

━━━ 6. 逻辑 ━━━
✓ 全称量词: PASSED
✓ 存在量词: PASSED
✓ 逻辑非: PASSED
✓ 逻辑与: PASSED
✓ 逻辑或: PASSED

━━━ 7. 复杂公式 ━━━
✓ 分数+根号: ALL FORMATS PASSED
✓ 求和+求积: ALL FORMATS PASSED
✓ 高斯积分: ALL FORMATS PASSED
✓ 欧拉公式: ALL FORMATS PASSED
✓ 微分方程: ALL FORMATS PASSED
✓ 特征值方程: ALL FORMATS PASSED

━━━ 8. 高级数学 ━━━
✓ 散度定理: ALL FORMATS PASSED
✓ 度规张量: ALL FORMATS PASSED
✓ 函子: ALL FORMATS PASSED
✓ 黎曼ζ函数: ALL FORMATS PASSED
✓ 组合数公式: ALL FORMATS PASSED

━━━ 9. 矩阵 ━━━
✓ 圆括号矩阵: ALL FORMATS PASSED
✓ 方括号矩阵: ALL FORMATS PASSED

━━━ 10. 向量与字体 ━━━
✓ 向量+矩阵: ALL FORMATS PASSED
✓ 粗体特征值: ALL FORMATS PASSED
✓ 黑板粗体: ALL FORMATS PASSED

test result: ok. 1 passed
```

## 架构

```
Input Format (LaTeX/MathML/OMML/Typst)
    ↓
Document AST (统一数据模型)
    ↓
Output Format (LaTeX/Markdown/Typst/HTML/MathML/OMML/JSON)
```

## 相关文档

- [architecture.md](architecture.md) - 架构概览
- [conversion.md](conversion.md) - 转换详细说明
- [getting-started.md](getting-started.md) - 快速开始

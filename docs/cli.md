# CLI Crate

> 命令行工具 — snipper 命令

## 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| `main` | main.rs | clap CLI 入口 + 子命令 |

## 子命令

| 命令 | 说明 | 状态 |
|------|------|------|
| `snipper recognize` | 识别图像内容 | Mock 模式可用 |
| `snipper parse` | 解析 LaTeX 为 AST | ✅ 可用 |
| `snipper render` | 渲染 AST 为 LaTeX | ✅ 可用 |
| `snipper version` | 显示版本信息 | ✅ 可用 |

## 使用示例

```bash
# Mock 模式识别
snipper recognize --input image.png --mode formula

# 解析 LaTeX
snipper parse --latex "$\frac{a+b}{c}$"

# 渲染 AST
snipper render --latex "$\frac{a+b}{c}$"

# 版本信息
snipper version
```

## 依赖关系

```
CLI
↑ 依赖 Engine, Mock, Syntax, Export
```

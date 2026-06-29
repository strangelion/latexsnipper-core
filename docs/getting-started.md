# Getting Started

> 开发者入门指南

## 环境要求

| 工具 | 版本 | 说明 |
|------|------|------|
| Rust | stable | 通过 rustup 安装 |
| rustfmt | — | `rustup component add rustfmt` |
| clippy | — | `rustup component add clippy` |

## 克隆与构建

```bash
# 克隆仓库
git clone https://github.com/strangelion/latexsnipper-core.git
cd latexsnipper-core

# 构建所有 crate
cargo build

# 运行测试
cargo test

# 代码检查
cargo clippy -- -D warnings

# 格式化
cargo fmt --check
```

## 运行 CLI

```bash
# Mock 模式识别
cargo run -p latexsnipper-cli -- recognize --input fixtures/formula.png --mode formula

# 解析 LaTeX
cargo run -p latexsnipper-cli -- parse --latex '$\frac{a+b}{c}$'

# 渲染 AST
cargo run -p latexsnipper-cli -- render --latex '$\frac{a+b}{c}$'

# 版本信息
cargo run -p latexsnipper-cli -- version
```

## 项目结构

```
latexsnipper-core/
├── crates/              # 所有 Rust crate
│   ├── foundation/      # 基础设施
│   ├── ast/             # Document AST
│   ├── tensor/          # 推理 I/O 张量
│   ├── image/           # 图像处理
│   ├── runtime/         # 推理运行时抽象
│   ├── model/           # 模型管理
│   ├── inference/       # 推理能力
│   ├── pipeline/        # 节点化流水线
│   ├── syntax/          # Parser + Renderer
│   ├── conversion/      # 格式转换（规划中）
│   ├── export/          # 导出能力
│   ├── engine/          # 核心引擎
│   ├── plugin/          # 插件 API（规划中）
│   ├── mock/            # 测试 Mock
│   ├── ffi/             # Android/iOS FFI
│   ├── wasm/            # WebAssembly（规划中）
│   ├── cli/             # 命令行工具
│   └── tests/           # 集成测试
├── docs/                # 架构文档
├── fixtures/            # 测试图片
├── models-download/     # 模型清单
└── Cargo.toml           # Workspace 配置
```

## 添加新 Crate

1. 在 `crates/` 下创建目录
2. 添加 `Cargo.toml`（参考现有 crate）
3. 在根 `Cargo.toml` 的 `[workspace] members` 中注册
4. 在 `docs/` 中添加对应文档

## 运行测试

```bash
# 全部测试
cargo test

# 单个 crate 测试
cargo test -p latexsnipper-ast
cargo test -p latexsnipper-image
cargo test -p latexsnipper-syntax

# 显示测试输出
cargo test -- --nocapture
```

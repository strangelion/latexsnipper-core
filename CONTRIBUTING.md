# Contributing to LaTeXSnipper Core

Thank you for your interest in contributing! This document provides guidelines and information for contributors.

## Development Setup

### Prerequisites

- Rust 1.75+ (see `rust-toolchain.toml`)
- Cargo

### Getting Started

```bash
# Clone the repository
git clone https://github.com/strangelion/latexsnipper-core.git
cd latexsnipper-core

# Build
cargo build

# Run tests
cargo test --workspace
```

## Code Style

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- Run `cargo clippy` to catch common mistakes
- Keep functions focused and small
- Write meaningful commit messages

## Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests if applicable
5. Ensure all tests pass (`cargo test --workspace`)
6. Run formatter and linter:
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   ```
7. Commit your changes (`git commit -m 'Add amazing feature'`)
8. Push to the branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

## Reporting Bugs

When filing an issue, please include:

- A clear and descriptive title
- Steps to reproduce the problem
- Expected behavior vs actual behavior
- Your environment (OS, Rust version, etc.)
- Any relevant error messages or logs

## Feature Requests

We welcome feature requests! Please open an issue with:

- A clear description of the feature
- Use cases and motivation
- Any implementation ideas you have

## Code of Conduct

Please be respectful and constructive in all interactions. We are committed to providing a welcoming and inclusive experience for everyone.

## License

By contributing, you agree that your contributions will be licensed under the AGPL-3.0 License.

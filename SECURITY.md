# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability within LaTeXSnipper Core, please send an email to [SECURITY_EMAIL] instead of using the issue tracker. All security vulnerabilities will be promptly addressed.

Please include the following information in your report:

- Type of issue (e.g., buffer overflow, code injection, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

This information will help us triage your report more quickly.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.x     | :white_check_mark: |

## Security Considerations

### Model Security

- Models are verified using SHA256 checksums
- Only download models from official releases
- Verify model integrity before use

### Runtime Security

- ONNX Runtime sessions are isolated
- Memory safety is guaranteed by Rust's type system
- Input validation is performed at API boundaries

### Platform Security

- FFI boundaries are carefully audited
- WASM modules run in sandboxed environments
- No unsafe code without explicit safety comments

## Best Practices

1. Always validate input images before processing
2. Use the latest stable Rust version
3. Keep dependencies updated
4. Review security advisories for dependencies

## Acknowledgments

We would like to thank all security researchers who responsibly disclose vulnerabilities.

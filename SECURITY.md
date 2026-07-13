# Security policy

Please report suspected vulnerabilities privately through GitHub Security
Advisories. Do not include private documents, model credentials, or production
tokens in a public issue.

## Input boundaries

`DocumentImporter` applies configurable limits before OOXML parsing: compressed
entry count, total decompressed bytes, compression ratio, enclosed relative
paths, XML nesting depth, XML element count, forbidden DTD/entity declarations,
and external or absolute relationship targets. It also limits parsed pages and
assets. `ImportOptions` owns these budgets; callers processing untrusted data
should lower them for their deployment.

The WASM adapter separately enforces RGBA length, maximum pixel count,
per-artifact bytes, total model bytes, SHA-256 checks, transaction rollback, and
LRU eviction. ONNX artifacts are parsed by Tract before they become live.

## Trust boundaries

- Built-in Rust plugins are trusted in-process code. Panics are contained, but
  they are not a security sandbox.
- Native ABI and WASI Component manifests describe permissions and budgets;
  loading/executing those classes remains unsupported until their hosts are
  implemented and tested.
- Remote model/API credentials must be supplied through environment variables or
  platform secret stores. Never commit them.
- Binary output should be reopened and validated before it replaces a target.

## Supported versions

Security fixes are applied to the current `main` line and the newest tagged
release. Older releases may require upgrading before a fix is available.

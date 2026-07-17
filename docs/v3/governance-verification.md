# Governance Verification — Core 3.0.0 GA

**Purpose:** Execute and document the final pre-release governance checks.
Each item must be completed and evidence saved before creating the v3.0.0 tag.

---

## 1. Scheduled Hardening Workflow

**Status:** [x] Passed

```bash
# Trigger the scheduled hardening workflow
gh workflow run scheduled.yml --ref main

# Wait for completion
gh run list --workflow=scheduled.yml --limit=1

# Check results
gh run view <run-id>
```

**Evidence:** Save the run ID and URL.

| Check | Status | Run ID |
|-------|--------|--------|
| Dependency audit | [ ] | |
| Benchmark regression | [ ] | |
| libFuzzer smoke | [ ] | |
| Production model WASM | [ ] | |
| Model URL verification | [ ] | |

---

## 2. Browser Tests

**Status:** [ ] Not run / [ ] Passed / [ ] Failed

```bash
# Run WASM/TypeScript tests
cd crates/wasm/js
npm ci
npm run typecheck
npm test
npm run build
npm run build:example
npm run smoke:packages

# Run browser production profile tests
cargo test --locked -p latexsnipper-wasm --test production_profiles
```

**Browsers to test manually:**

| Browser | Version | Status |
|---------|---------|--------|
| Chrome | Latest | [ ] |
| Firefox | Latest | [ ] |

---

## 3. libFuzzer Smoke

**Status:** [ ] Not run / [ ] Passed / [ ] Failed

```bash
# Run bounded fuzz campaign (requires nightly)
rustup toolchain install nightly
cargo +nightly install cargo-fuzz --locked

targets=(
  format_signature zip_package_importer ooxml_relationship_parser
  xml_parser svg_parser pdf_importer json_ast_deserializer latex_parser
  typst_parser markdown_math_parser model_manifest_parser plugin_manifest_parser
)

for target in "${targets[@]}"; do
  mkdir -p "fuzz/artifacts/$target"
  cargo +nightly fuzz run "$target" "fuzz/corpus/$target" -- \
    -max_total_time=10 \
    -max_len=1048576 \
    -artifact_prefix="fuzz/artifacts/$target/"
done
```

**Result:** [ ] All targets passed / [ ] Crashes found (document below)

| Target | Status | Crash artifact |
|--------|--------|----------------|
| format_signature | [ ] | |
| zip_package_importer | [ ] | |
| ooxml_relationship_parser | [ ] | |
| xml_parser | [ ] | |
| svg_parser | [ ] | |
| pdf_importer | [ ] | |
| json_ast_deserializer | [ ] | |
| latex_parser | [ ] | |
| typst_parser | [ ] | |
| markdown_math_parser | [ ] | |
| model_manifest_parser | [ ] | |
| plugin_manifest_parser | [ ] | |

---

## 4. Model URL Verification

**Status:** [ ] Not run / [ ] Passed / [ ] Failed

```bash
# Run model URL verification (Windows)
gh workflow run scheduled.yml --ref main -f job=model-url-verification

# Or run locally on Windows
./scripts/setup-real-model-tests.ps1
```

**Result:** [ ] All URLs verified / [ ] Failures found

---

## 5. Benchmark Artifacts

**Status:** [ ] Not collected / [ ] Collected

```bash
# Run benchmark suite
cargo bench --locked -p latexsnipper-tests --bench core_bench | tee target/benchmark-output.txt

# Build comparable JSON
grep '^benchmark_json=' target/benchmark-output.txt \
  | sed 's/^benchmark_json=//' \
  | jq -s --arg commit "$(git rev-parse HEAD)" \
    '{schemaVersion: 1, commit: $commit, benchmarks: .}' \
    > target/benchmark-results.json
```

**Result:** [ ] Benchmark JSON saved / [ ] Compared to baseline

| Benchmark | ns/iter | Status |
|-----------|---------|--------|
| ast_text_collector_256_formula_blocks | TBD | [ ] |
| conversion_mathml_cases_64 | TBD | [ ] |
| conversion_omml_cases_64 | TBD | [ ] |
| conversion_typst_cases_64 | TBD | [ ] |
| pipeline_graph_8_transform_nodes | TBD | [ ] |
| export_svg | TBD | [ ] |
| export_png | TBD | [ ] |
| export_pdf | TBD | [ ] |
| export_docx | TBD | [ ] |
| export_pptx | TBD | [ ] |
| export_xlsx | TBD | [ ] |
| import_docx | TBD | [ ] |
| import_pptx | TBD | [ ] |
| import_xlsx | TBD | [ ] |
| plugin_chain_8 | TBD | [ ] |

---

## 6. GitHub Ruleset / CODEOWNERS

**Status:** [ ] Not verified / [ ] Verified

Check the following GitHub settings:

| Setting | Status | Notes |
|---------|--------|-------|
| Branch protection on `main` | [ ] | Require PR reviews |
| CODEOWNERS file exists | [ ] | |
| Required status checks configured | [ ] | |
| Signed commits required | [ ] | |
| Admin restrictions | [ ] | |

```bash
# Check if CODEOWNERS exists
cat .github/CODEOWNERS 2>/dev/null || echo "No CODEOWNERS file"

# Check branch protection via API
gh api repos/strangelion/latexsnipper-core/branches/main/protection
```

---

## 7. Final Release Checklist

Before creating the v3.0.0 tag, verify:

| Item | Status |
|------|--------|
| Workspace version is 3.0.0 | [ ] |
| All internal crate versions are 3.0.0 | [ ] |
| WASM JS package version is 3.0.0 | [ ] |
| Cargo.lock is committed | [ ] |
| Contract freeze is up to date | [ ] |
| All CI checks pass on main | [ ] |
| SECURITY_REVIEW.md is committed | [ ] |
| Model evidence document is complete | [ ] |
| Visual smoke tests passed | [ ] |
| CHANGELOG.md is updated for 3.0.0 | [ ] |
| README.md reflects GA status | [ ] |

---

## 8. Create Release Tag

After all checks pass:

```bash
# Verify version consistency
python scripts/verify_release_version.py --stable

# Verify contract freeze
python scripts/verify_v3_contract_freeze.py

# Create and push the tag
git tag -a v3.0.0 -m "Core 3.0.0 GA release"
git push origin v3.0.0
```

**This will trigger the release workflow which:**
1. Re-runs version and contract checks with `--stable` flag
2. Builds CLI binaries for Linux x86_64, Windows x86_64, Apple Silicon macOS
3. Builds WASM packages
4. Publishes crates to crates.io
5. Creates GitHub release with artifacts

---

## Sign-off

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Release manager | | | |
| Security reviewer | | | |
| QA | | | |

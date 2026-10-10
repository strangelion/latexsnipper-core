import { createRequire } from "node:module";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import wasm from "vite-plugin-wasm";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../../..");
const nodeEntry = resolve(repositoryRoot, "target/wasm-nodejs/latexsnipper_wasm.js");
const webEntry = resolve(repositoryRoot, "target/wasm-web/latexsnipper_wasm.js");
const bundlerEntry = resolve(repositoryRoot, "target/wasm-bundler/latexsnipper_wasm.js");

const require = createRequire(import.meta.url);
const nodePackage = require(nodeEntry);
const apiInfo = nodePackage.api_info_v2();
if (!apiInfo || apiInfo.ok !== true) {
  throw new Error("Node package did not return a successful API envelope");
}
const apiInfoV3 = nodePackage.api_info_v3();
if (
  !apiInfoV3 ||
  apiInfoV3.ok !== true ||
  apiInfoV3.versions?.apiEnvelopeVersion !== 3
) {
  throw new Error("Node package did not return a successful v3 API envelope");
}
const capabilitiesV3 = nodePackage.capabilities_v3();
if (
  !capabilitiesV3 ||
  capabilitiesV3.ok !== true ||
  capabilitiesV3.data?.schemaVersion !== 3
) {
  throw new Error("Node package did not return a plain-object v3 capability document");
}

function formulaSmoke(api) {
  const envelope = api.formula_capabilities_v3();
  if (!envelope.ok || envelope.versions.apiEnvelopeVersion !== 3 || envelope.data.length !== 144) {
    throw new Error("Formula capability projection is invalid");
  }
  const samples = {
    latex: String.raw`\frac{a}{b}`,
    mathml: "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
    omml: '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">'
      + '<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num>'
      + '<m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>',
    typst: "frac(a, b)",
    markdown: String.raw`A fraction $\frac{a}{b}$`,
  };
  let converted = 0;
  let rejected = 0;
  for (const route of envelope.data) {
    if (route.target !== "wasm32-unknown-unknown" || !route.limitations.length) {
      throw new Error("Formula route lost its WASM target or limitations");
    }
    const result = api.convert_formula_v3(samples[route.input] ?? "x", route.input, route.output, route.mode);
    if (result.ok !== route.available) throw new Error(`Formula route mismatch: ${JSON.stringify(route)}`);
    if (result.ok) {
      if (!result.data.content) throw new Error("Formula conversion returned empty content");
      assert.deepEqual(result.data.capability, route);
      converted += 1;
    } else {
      if (result.error.code !== "UNSUPPORTED_FORMAT" || result.error.details.detail !== route.unavailableReason) {
        throw new Error("Formula route rejection differs from its capabilities");
      }
      rejected += 1;
    }
  }
  if (converted !== 46 || rejected !== 98) throw new Error("Unexpected formula route counts");
  if (!api.convert_formula_v3(samples.latex, "latex", "omml").ok) throw new Error("Default strict route failed");
  for (const [source, input, output, mode, code] of [
    ["x", "ole", "omml", undefined, "INVALID_ARGUMENT"],
    ["x", "latex", "pdf", undefined, "INVALID_ARGUMENT"],
    ["x", "latex", "omml", "lossless", "INVALID_ARGUMENT"],
    [" ", "latex", "omml", undefined, "CONVERSION_FAILED"],
    [String.raw`\unknownmacro+x`, "latex", "omml", undefined, "CONVERSION_FAILED"],
    ["x".repeat(64 * 1024 + 1), "latex", "omml", undefined, "INPUT_TOO_LARGE"],
    ["{".repeat(65) + "x" + "}".repeat(65), "latex", "omml", "best-effort", "INPUT_TOO_LARGE"],
    ["<!DOCTYPE math><math><mi>x</mi></math>", "mathml", "omml", "best-effort", "CONVERSION_FAILED"],
  ]) {
    const result = api.convert_formula_v3(source, input, output, mode);
    if (result.ok || result.error.code !== code || result.error.recoverable !== false) {
      throw new Error(`Formula error mismatch: expected ${code}, received ${JSON.stringify(result)}`);
    }
  }
  if (!api.capabilities_v3().ok) throw new Error("Formula errors affected legacy state");
  for (const input of ["latex", "mathml", "omml", "typst"]) {
    const fragment = api.convert_formula_fragment_v3(samples[input], input, "best-effort");
    assert.equal(fragment.ok, true);
    assert.equal(fragment.data.contentKind, "latex-fragment");
    assert.equal(fragment.data.content, String.raw`\frac{a}{b}`);
    assert.equal(fragment.data.capability.output, "latex_display");
    assert.equal(api.convert_formula_v3(samples[input], input, "markdown_inline", "best-effort").data.content, String.raw`$\frac{a}{b}$`);
  }
  const invalidFragment = api.convert_formula_fragment_v3(String.raw`\documentclass{article}x`, "latex", "best-effort");
  assert.equal(invalidFragment.ok, false);
  assert.equal(invalidFragment.error.code, "CONVERSION_FAILED");
}
formulaSmoke(nodePackage);
const { convertFormula, convertFormulaFragment, formulaConversionCapabilities } = await import("../dist/index.js");
assert.deepEqual(formulaConversionCapabilities(nodePackage), nodePackage.formula_capabilities_v3());
assert.equal(convertFormula(nodePackage, String.raw`\frac{a}{b}`, {
  inputFormat: "latex", outputFormat: "omml",
}).ok, true);
assert.equal(convertFormulaFragment(nodePackage, "frac(a,b)", { inputFormat: "typst", mode: "best-effort" }).data.contentKind, "latex-fragment");

const webPackage = await import(pathToFileURL(webEntry).href);
if (typeof webPackage.default !== "function") {
  throw new Error("Web ESM package does not expose the async initializer");
}
if (typeof webPackage.convert_formula_v3 !== "function" || typeof webPackage.formula_capabilities_v3 !== "function") {
  throw new Error("Web ESM package lacks additive formula exports");
}
await webPackage.default({
  module_or_path: await readFile(resolve(repositoryRoot, "target/wasm-web/latexsnipper_wasm_bg.wasm")),
});
formulaSmoke(webPackage);

const normalizedBundlerEntry = bundlerEntry.replaceAll("\\", "/");
await build({
  configFile: false,
  logLevel: "silent",
  plugins: [
    wasm(),
    {
      name: "latexsnipper-generated-package-smoke",
      resolveId(id) {
        return id === "virtual:entry" ? "\0virtual:entry" : null;
      },
      load(id) {
        if (id !== "\0virtual:entry") return null;
        return `import { api_info_v2, api_info_v3, convert_formula_v3, formula_capabilities_v3 } from ${JSON.stringify(normalizedBundlerEntry)}; export { api_info_v2, api_info_v3, convert_formula_v3, formula_capabilities_v3 };`;
      },
    },
  ],
  build: {
    write: false,
    target: "esnext",
    rollupOptions: { input: "virtual:entry" },
  },
});

console.log("Node, web ESM, and bundler package smoke passed");

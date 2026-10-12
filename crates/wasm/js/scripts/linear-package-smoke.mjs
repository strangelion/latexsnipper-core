// Real generated package conformance; finite formulas, not renderer acceptance.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const prefix = process.argv[2] ?? "wasm-conversion-nodejs";
assert.ok(["wasm-conversion-nodejs", "wasm-registry-nodejs"].includes(prefix));
const api = createRequire(import.meta.url)(resolve(root, `target/${prefix}/latexsnipper_wasm.js`));
const samples = { latex: String.raw`\frac{a}{b}`, typst: "frac(a,b)",
  markdown: String.raw`$\frac{a}{b}$`, "ascii-math": "a/b", "unicode-math": "a/b",
  mathml: "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
  omml: '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>' };
const routes = api.formula_capabilities_v3().data;
assert.equal(routes.length, 176);
let successes = 0;
for (const route of routes) {
  const result = api.convert_formula_v3(samples[route.input] ?? "x", route.input, route.output, route.mode);
  assert.equal(result.ok, route.available, JSON.stringify({route, result}));
  if (result.ok) { successes += 1; assert.deepEqual(result.data.capability, route); }
  else assert.equal(result.error.code, "UNSUPPORTED_FORMAT");
}
assert.equal(successes, 78);
for (const [source, input, output] of [["color(red)(x)", "ascii-math", "omml"],
  ["∫_0^1 x", "unicode-math", "ascii-math"], ["body $x$", "markdown", "unicode-math"],
  [String.raw`\unknown{x}`, "latex", "ascii-math"]]) {
  assert.equal(api.convert_formula_v3(source, input, output, "best-effort").error.code, "CONVERSION_FAILED");
}
assert.equal(api.convert_formula_v3("a/b", "ascii-math", "unicode-math", "best-effort").data.content, "(a)/(b)");
console.log("Actual linear formula WASM: 78 converted, 98 unavailable, grammar and mixed-document refusals passed");

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
  ["∫1 x", "unicode-math", "ascii-math"], ["body $x$", "markdown", "unicode-math"],
  [String.raw`\unknown{x}`, "latex", "ascii-math"]]) {
  const result = api.convert_formula_v3(source, input, output, "best-effort");
  assert.equal(result.ok, false, JSON.stringify({source, input, output, result}));
  assert.equal(result.error.code, "CONVERSION_FAILED");
}
const nary = api.convert_formula_v3("∫_0^1 x", "unicode-math", "ascii-math", "best-effort");
assert.equal(nary.ok, true);
assert.equal(nary.data.content, "{:int_(0)^(1) {:x:}:}");
for (const [source, canonical] of [["sin x", "sin⁡〖x〗"],
  ["∫_0^1 x", "〖〖∫〗_(0)〗^(1)▒〖x〗"], ['"rate"', '"rate"']]) {
  const result = api.convert_formula_v3(source, "unicode-math", "unicode-math", "best-effort");
  assert.equal(result.ok, true);
  assert.equal(result.data.content, canonical);
}
assert.equal(api.convert_formula_v3("a/b", "ascii-math", "unicode-math", "best-effort").data.content, "(a)/(b)");
for (const [source, location] of [["∫1_0^1▒x+b", "undOvr"], ["∑2_i^n▒x+b", "subSup"]]) {
  const canonical = api.convert_formula_v3(source, "unicode-math", "unicode-math", "best-effort");
  assert.equal(canonical.ok, true);
  const xml = api.convert_formula_v3(source, "unicode-math", "omml", "best-effort");
  assert.equal(xml.ok, true);
  assert.ok(xml.data.content.includes(`<m:limLoc m:val="${location}"/>`));
  const restored = api.convert_formula_v3(xml.data.content, "omml", "unicode-math", "best-effort");
  assert.equal(restored.ok, true);
  assert.equal(restored.data.content, canonical.data.content);
}
const noGrow = "∫65_0^1▒x+b";
const noGrowCanonical = api.convert_formula_v3(noGrow, "unicode-math", "unicode-math", "best-effort");
assert.equal(noGrowCanonical.ok, true);
assert.equal(noGrowCanonical.data.content, "〖〖∫65〗_(0)〗^(1)▒〖x〗 + b");
const noGrowXml = api.convert_formula_v3(noGrow, "unicode-math", "omml", "best-effort");
assert.equal(noGrowXml.ok, true);
assert.ok(noGrowXml.data.content.includes('<m:grow m:val="0"/>'));
const noGrowRestored = api.convert_formula_v3(noGrowXml.data.content, "omml", "unicode-math", "best-effort");
assert.equal(noGrowRestored.ok, true);
assert.equal(noGrowRestored.data.content, noGrowCanonical.data.content);
for (const [source, stretch] of [[noGrow, "false"], ["∑130_i^n▒x+b", "true"], ["∫88▒x", "false"]]) {
  const canonical = api.convert_formula_v3(source, "unicode-math", "unicode-math", "best-effort");
  const mathml = api.convert_formula_v3(source, "unicode-math", "mathml", "best-effort");
  assert.equal(canonical.ok, true);
  assert.equal(mathml.ok, true);
  assert.ok(mathml.data.content.includes(`stretchy="${stretch}"`));
  assert.ok(!mathml.data.content.includes("LatexSnipperInternal"));
  const restored = api.convert_formula_v3(mathml.data.content, "mathml", "unicode-math", "best-effort");
  assert.equal(restored.ok, true);
  assert.equal(restored.data.content, canonical.data.content);
  const rebuilt = api.convert_formula_v3(mathml.data.content, "mathml", "omml", "best-effort");
  assert.equal(rebuilt.ok, true);
  assert.ok(rebuilt.data.content.includes(`<m:grow m:val="${stretch === "true" ? "1" : "0"}"/>`));
}
const badGrowth = api.convert_formula_v3('<math><mo stretchy="yes">∫</mo></math>', "mathml", "unicode-math", "best-effort");
assert.equal(badGrowth.ok, false);
assert.equal(badGrowth.error.code, "CONVERSION_FAILED");
for (const [source, closing] of [["sin⁡〖x+y〗+b", "</m:func>"], ["∫_0^1▒〖x+y〗+b", "</m:nary>"]]) {
  const result = api.convert_formula_v3(source, "unicode-math", "omml", "best-effort");
  assert.equal(result.ok, true);
  const content = result.data.content;
  const end = content.indexOf(closing);
  assert.ok(end >= 0);
  assert.ok(content.slice(0, end).includes("<m:t>y</m:t>"));
  assert.ok(!content.slice(0, end).includes("<m:t>b</m:t>"));
  assert.ok(content.slice(end).includes("<m:t>b</m:t>"));
}
const noGrowRejected = api.convert_formula_v3(noGrow, "unicode-math", "latex_display", "best-effort");
assert.equal(noGrowRejected.ok, false);
assert.equal(noGrowRejected.error.code, "CONVERSION_FAILED");
const slots = api.convert_formula_v3("∫24▒x", "unicode-math", "omml", "best-effort");
assert.equal(slots.ok, true);
assert.ok(slots.data.content.includes('<m:subHide m:val="0"/>'));
assert.ok(slots.data.content.includes('<m:supHide m:val="0"/>'));
assert.ok(slots.data.content.includes('<m:sub></m:sub>'));
assert.ok(slots.data.content.includes('<m:sup></m:sup>'));
const slotsRestored = api.convert_formula_v3(slots.data.content, "omml", "unicode-math", "best-effort");
assert.equal(slotsRestored.ok, true);
assert.equal(slotsRestored.data.content, "∫24▒〖x〗");
console.log("Actual linear formula WASM: 78 converted, 98 unavailable, grammar, growth, empty slots and mixed-document refusals passed");

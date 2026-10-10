// Compare actual generated packages; this is not host/visual fidelity acceptance.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, stat } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const require = createRequire(import.meta.url);
const full = require(resolve(root, "target/wasm-full-nodejs/latexsnipper_wasm.js"));
const conversion = require(resolve(root, "target/wasm-conversion-nodejs/latexsnipper_wasm.js"));
const sample = {
  latex: String.raw`\frac{a}{b}`,
  mathml: "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
  omml: '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>',
  typst: "frac(a,b)",
  markdown: String.raw`A fraction $\frac{a}{b}$`,
};
assert.equal(full.api_info_v3().data.v2CompatibilityExports, true);
assert.equal(conversion.api_info_v3().data.v2CompatibilityExports, false);
assert.deepEqual(full.formula_capabilities_v3(), conversion.formula_capabilities_v3());
for (const fn of ["recognize_v2", "load_model_v2", "capabilities_v3", "clear_models_v2", "convert_v3"]) {
  assert.equal(typeof full[fn], "function");
  assert.equal(typeof conversion[fn], "undefined");
}
const routes = full.formula_capabilities_v3().data;
for (const route of routes) {
  const args = [sample[route.input] ?? "x", route.input, route.output, route.mode];
  assert.deepEqual(conversion.convert_formula_v3(...args), full.convert_formula_v3(...args));
}
for (const input of ["latex", "typst", "mathml", "omml"]) {
  assert.deepEqual(conversion.convert_formula_fragment_v3(sample[input], input, "best-effort"), full.convert_formula_fragment_v3(sample[input], input, "best-effort"));
}
for (const [source, input] of [["x".repeat(65537), "latex"], [String.raw`\documentclass{article}x`, "latex"], ["x", "mtef"]]) {
  assert.deepEqual(conversion.convert_formula_fragment_v3(source, input, "best-effort"), full.convert_formula_fragment_v3(source, input, "best-effort"));
}

const profiles = [];
for (const profile of ["full", "conversion"]) {
  const directory = resolve(root, `target/wasm-${profile}-web`);
  const bytes = await readFile(resolve(directory, "latexsnipper_wasm_bg.wasm"));
  const module = await import(pathToFileURL(resolve(directory, "latexsnipper_wasm.js")).href);
  const start = performance.now();
  const exports = await module.default({ module_or_path: bytes });
  const initializeWallMillis = performance.now() - start;
  const linearMemoryBytesAfterInit = exports.memory.buffer.byteLength;
  assert.equal(module.convert_formula_fragment_v3("frac(a,b)", "typst", "best-effort").data.content, String.raw`\frac{a}{b}`);
  profiles.push({ profile, wasmBytes: bytes.byteLength, wasmSha256: createHash("sha256").update(bytes).digest("hex"), javascriptBytes: (await stat(resolve(directory, "latexsnipper_wasm.js"))).size, initializeWallMillis, linearMemoryBytesAfterInit });
}
console.log(JSON.stringify({ scope: "generated Node parity and Web ESM initialization under Node; not Obsidian", routesCompared: routes.length, fragmentsCompared: 4, profiles }, null, 2));

// Actual generated WASM callbacks, not a mock JS conversion implementation.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const prefix = process.argv[2] ?? "wasm-conversion-nodejs";
assert.ok(["wasm-conversion-nodejs", "wasm-registry-nodejs"].includes(prefix));
const api = createRequire(import.meta.url)(resolve(root, `target/${prefix}/latexsnipper_wasm.js`));
const request = (content) => ({ schemaVersion: 1, input: "fixture:power", output: "omml", mode: "best-effort",
  backend: "fixture:js", content, contextSha256: "a".repeat(64) });
const policy = { schemaVersion: 1, backends: [] };
const descriptor = { schemaVersion: 1, id: "fixture:js", version: "1.0.0", configurationSha256: "b".repeat(64),
  license: "AGPL-3.0", limitations: ["Finite power fixture; trusted JS only"],
  routes: [{ input: "fixture:power", output: "omml", mode: "best-effort", target: "wasm32-unknown-unknown" }],
  maxInputBytes: 65536, maxOutputBytes: 262144 };
const registry = api.create_formula_registry_v1();
let calls = 0;
let ready = true;
let output = "normal";
assert.equal(registry.register_format(JSON.stringify({ schemaVersion: 1, id: "fixture:power",
  mimeType: "application/vnd.fixture.power", description: "Finite power" }), (source) => /^[a-z]:[0-9]$/.test(source)).ok, true);
assert.equal(registry.register_backend(JSON.stringify(descriptor), () => ready, (json) => {
  calls += 1;
  const source = JSON.parse(json).content;
  if (output === "throw") throw new Error("fixture callback failure");
  if (output === "async") return Promise.resolve("not a synchronous reply");
  if (output === "unsafe") return JSON.stringify({ content: '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" onclick="evil()"/>', losses: [] });
  if (output === "close") registry.close();
  const result = api.convert_formula_v3(`${source[0]}^${source[2]}`, "latex", "omml", "strict");
  return JSON.stringify({ content: result.data.content, losses: [] });
}).ok, true);
const convert = (value, selectedPolicy = policy) => registry.convert(JSON.stringify(value), JSON.stringify(selectedPolicy));
assert.equal(convert(request("x:2")).data.outcome.error.code, "BACKEND_DISABLED");
assert.equal(calls, 0);
assert.equal(registry.set_enabled("fixture:js", true).ok, true);
const first = convert(request("x:2"));
const second = convert(request("y:4"));
assert.equal(first.data.outcome.kind, "converted");
assert.notEqual(first.data.outcome.result.candidate.content, second.data.outcome.result.candidate.content);
assert.notEqual(first.data.decisionSha256, second.data.decisionSha256);
assert.equal(convert(request("x:2+y")).data.outcome.kind, "rejected");
assert.equal(calls, 2);
for (const mode of ["throw", "async", "unsafe"]) {
  output = mode;
  const before = calls;
  const result = convert(request("x:2"), { schemaVersion: 1, backends: ["core:builtin"] });
  assert.equal(result.data.outcome.kind, "rejected");
  assert.equal(result.data.attempts.length, 1);
  assert.equal(calls, before + 1);
}
ready = false;
assert.equal(convert(request("x:2"), { schemaVersion: 1, backends: ["core:builtin"] }).data.attempts.length, 1);
ready = true; output = "close";
assert.equal(convert(request("x:2")).ok, false);
assert.equal(registry.capabilities().ok, false);
registry.close(); registry.free();
for (let index = 0; index < 70; index += 1) {
  const fresh = api.create_formula_registry_v1(); fresh.close(); fresh.free();
}
assert.throws(() => api.create_formula_registry_v1("unknown:configuration"));
console.log("Actual WASM registry: dynamic output, validation, default disabled, terminal errors, callback close and cleanup passed");

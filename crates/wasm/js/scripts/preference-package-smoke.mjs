import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { FormulaPreferences } from "../dist/preferences.js";
import { preferenceProof } from "./preference-proof.mjs";
import { checkpointProof } from "./preference-checkpoint-proof.mjs";
const require = createRequire(import.meta.url);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const api = require(resolve(root, "target", process.argv[2] ?? "wasm-scoped-nodejs", "latexsnipper_wasm.js"));
assert.equal(api.formula_preference_capabilities_v3().data.persistence, false);
const result = await preferenceProof((configuration) => new FormulaPreferences(api, configuration),
  (request) => api.complete_formula_fragments_v3(JSON.stringify(request)), { capacity: true });
assert.equal(api.formula_preference_capabilities_v3().data.authenticatedCheckpoints, true);
const checkpoints = await checkpointProof((configuration) => new FormulaPreferences(api, configuration),
  (configuration) => FormulaPreferences.restore(api, configuration),
  (request) => api.complete_formula_fragments_v3(JSON.stringify(request)), { capacity: true });
const configuration = { schemaVersion: 1, scope: { userSha256: "a".repeat(64), projectSha256: null, documentSha256: null, generation: "0" }, keyHex: "ab".repeat(32) };
const transient = api.create_formula_preferences_v1(JSON.stringify(configuration));
let snapshotJson;
try {
  const exported = transient.command(JSON.stringify({ kind: "export-checkpoint", storageKeyHex: "cd".repeat(32),
    checkpoint: "1", permission: { storageEnabled: true, allowSensitiveStorage: false } }));
  assert.equal(exported.ok, true); snapshotJson = exported.data.snapshotJson;
} finally { transient.free(); }
const restoration = { ...configuration, storageKeyHex: "cd".repeat(32), expectedCheckpoint: "1", snapshotJson,
  permission: { storageEnabled: true, allowSensitiveStorage: false } };
const cryptoDescriptor = Object.getOwnPropertyDescriptor(globalThis, "crypto");
assert.equal(cryptoDescriptor?.configurable, true, "Node smoke requires a replaceable crypto fixture");
try {
  Object.defineProperty(globalThis, "crypto", { value: undefined, configurable: true });
  assert.throws(() => api.create_formula_preferences_v1(JSON.stringify(configuration)),
    (cause) => cause?.ok === false && cause.error.code === "PREFERENCE_UNAVAILABLE");
  assert.throws(() => api.restore_formula_preferences_v1(JSON.stringify(restoration)),
    (cause) => cause?.ok === false && cause.error.code === "PREFERENCE_UNAVAILABLE");
} finally { Object.defineProperty(globalThis, "crypto", cryptoDescriptor); }
assert.throws(() => api.create_formula_preferences_v1(JSON.stringify({ ...configuration, keyHex: "0".repeat(64) })));
const native = api.create_formula_preferences_v1(JSON.stringify(configuration));
try {
  for (const json of ['{"kind":"info","ignored":"private-marker"}', '{"kind":"capture","kind":"info"}',
    '{"kind":"clear","generation":1,"keyHex":"private-marker"}']) {
    const reply = native.command(json);
    assert.equal(reply.error.code, "INVALID_JSON");
    assert.equal(JSON.stringify(reply).includes("private-marker"), false);
  }
  assert.equal(native.command(" ".repeat(1024 * 1024 + 1)).error.code, "INPUT_TOO_LARGE");
  assert.equal(native.command(' '.repeat(4096) + '{"kind":"info"}').error.code, "INPUT_TOO_LARGE");
  const huge = { source: "x".repeat(3900) + "\\lo", cursor: 3903, revision: "0", inputFormat: "latex",
    catalog: { commands: { scope: configuration.scope, definitions: [] }, policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
      fragments: Array.from({ length: 60 }, (_, index) => ({ id: `entry${index}`, revision: "0", userSha256: configuration.scope.userSha256,
        projectSha256: null, documentSha256: null, trigger: "local", label: "Large fixture", source: "x %" + '"'.repeat(8188) + "\n" })) } };
  assert.equal(native.command(JSON.stringify({ kind: "rank-fragments", request: huge })).error.code, "OUTPUT_TOO_LARGE");
  assert.equal(native.command('{"kind":"info"}').ok, true);
  assert.equal(native.close().ok, true);
  assert.equal(native.command('{"kind":"info"}').error.code, "PREFERENCE_CLOSED");
  assert.equal(native.close().ok, true);
} finally { native.free(); }
for (const [input, expected] of [
  [JSON.stringify({ ...restoration, PRIVATE_SECRET: true }), "INVALID_JSON"],
  [JSON.stringify(restoration).replace('"schemaVersion":1', '"schemaVersion":1,"schemaVersion":1'), "INVALID_JSON"],
  [JSON.stringify({ ...restoration, expectedCheckpoint: 1 }), "INVALID_JSON"],
  [JSON.stringify({ ...restoration, snapshotJson: " ".repeat(256 * 1024 + 1) }), "LIMIT_EXCEEDED"],
  [" ".repeat(1024 * 1024 + 1), "INPUT_TOO_LARGE"],
]) {
  assert.throws(() => api.restore_formula_preferences_v1(input), (cause) => {
    assert.equal(cause?.error?.code, expected); assert.equal(JSON.stringify(cause).includes("PRIVATE_SECRET"), false); return true;
  });
}
const owners = [];
try {
  for (let i = 0; i < 64; i++) owners.push(api.create_formula_preferences_v1(JSON.stringify(configuration)));
  assert.throws(() => api.create_formula_preferences_v1(JSON.stringify(configuration)));
  assert.throws(() => api.restore_formula_preferences_v1(JSON.stringify(restoration)), (cause) => cause?.error?.code === "LIMIT_EXCEEDED");
  owners[0].close();
  const replacement = api.create_formula_preferences_v1(JSON.stringify(configuration)); replacement.free();
  const restored = api.restore_formula_preferences_v1(JSON.stringify(restoration)); restored.free();
} finally { for (const owner of owners) owner.free(); }
console.log(JSON.stringify({ profile: process.argv[2], ...result, ...checkpoints, ownerLimit: 64, rawClosedJson: true }));

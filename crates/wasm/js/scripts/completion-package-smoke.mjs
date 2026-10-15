import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

export function completionSmoke(api) {
  const scopedCaps = api.formula_scoped_completion_capabilities_v3();
  assert.equal(scopedCaps.data.schemaVersion, 2);
  assert.equal(scopedCaps.data.maxDefinitions, 64);
  assert.equal(scopedCaps.data.persistence, false);
  const catalog = { scope: { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: "18446744073709551615" },
    definitions: [{ id: "pair", revision: "18446744073709551615", userSha256: "a".repeat(64), projectSha256: "b".repeat(64), command: "custompair", arguments: ["math", "text"] }] };
  const scopedInput = { source: "中😀\\cust", cursor: 12, revision: "18446744073709551615", inputFormat: "latex", catalog };
  const scoped = api.complete_formula_scoped_v3(JSON.stringify(scopedInput));
  assert.equal(scoped.ok, true, JSON.stringify(scoped));
  assert.equal(scoped.data.context.schemaVersion, 2);
  assert.equal(scoped.data.context.nodes[0].parent, null);
  assert.equal(scoped.data.context.cell, null);
  assert.deepEqual(scoped.data.candidates[0].definition, { id: "pair", revision: "18446744073709551615" });
  const scopedApply = { ...scopedInput, snapshot: scoped.data.snapshot, candidateId: "definition:pair" };
  const accepted = api.apply_formula_completion_scoped_v3(JSON.stringify(scopedApply));
  assert.deepEqual(accepted.data, { source: "中😀\\custompair{}{}", cursor: 19, tabStops: [{ start: 19, end: 19 }, { start: 21, end: 21 }] });
  for (const field of ["generation", "projectSha256", "documentSha256", "signature", "definitionRevision"]) {
    const changed = structuredClone(catalog);
    if (field === "signature") changed.definitions[0].arguments = [];
    else if (field === "definitionRevision") changed.definitions[0].revision = "0";
    else changed.scope[field] = field === "generation" ? "0" : "d".repeat(64);
    assert.equal(api.apply_formula_completion_scoped_v3(JSON.stringify({ ...scopedApply, catalog: changed })).error.code, "STALE_CONTEXT");
  }
  const foreign = structuredClone(catalog);
  foreign.definitions[0].userSha256 = "d".repeat(64);
  foreign.definitions[0].command = "privateInvalid123";
  foreign.definitions[0].arguments = Array(20).fill("math");
  const hidden = api.complete_formula_scoped_v3(JSON.stringify({ ...scopedInput, catalog: foreign }));
  assert.deepEqual(hidden.data.candidates, []);
  assert.equal(JSON.stringify(hidden).includes("privateInvalid123"), false);
  const invalid = structuredClone(catalog);
  invalid.definitions[0].command = "mysymbol2";
  assert.equal(api.complete_formula_scoped_v3(JSON.stringify({ ...scopedInput, catalog: invalid })).error.code, "INVALID_CATALOG");
  invalid.definitions[0].revision = 42;
  assert.equal(api.complete_formula_scoped_v3(JSON.stringify({ ...scopedInput, catalog: invalid })).error.code, "INVALID_JSON");
  const builtin = api.complete_formula_scoped_v3(JSON.stringify({ ...scopedInput, source: "\\fr", cursor: 3 }));
  assert.equal(builtin.data.candidates[0].definition, null);
  assert.equal(api.apply_formula_completion_scoped_v3(JSON.stringify({ ...scopedApply, candidateId: "unknown" })).error.code, "UNKNOWN_CANDIDATE");
  assert.equal(api.complete_formula_scoped_v3(" ".repeat(1024 * 1024 + 1)).error.code, "INPUT_TOO_LARGE");
  const caps = api.formula_completion_capabilities_v3();
  assert.equal(caps.ok, true);
  assert.deepEqual(caps.data.inputFormats, ["latex"]);
  assert.equal(caps.data.maxResultDataBytes, 1024 * 1024);
  for (const revision of ["0", "9007199254740993", "18446744073709551615"]) {
    const params = { source: "中😀\\fr", cursor: 10, revision, inputFormat: "latex" };
    const queried = api.complete_formula_v3(JSON.stringify(params));
    assert.equal(queried.ok, true, JSON.stringify(queried));
    assert.equal(queried.data.snapshot.revision, revision);
    assert.equal(queried.data.completion.context.revision, revision);
    assert.equal(queried.data.completion.context.nodes[0].parent, null);
    assert.equal(queried.data.completion.context.cell, null);
    const apply = { ...params, snapshot: queried.data.snapshot, candidateId: "builtin:frac" };
    const applied = api.apply_formula_completion_v3(JSON.stringify(apply));
    assert.equal(applied.ok, true, JSON.stringify(applied));
    assert.deepEqual(applied.data, { source: "中😀\\frac{}{}", cursor: 13,
      tabStops: [{ start: 13, end: 13 }, { start: 15, end: 15 }] });
    assert.equal(api.apply_formula_completion_v3(JSON.stringify({ ...apply, revision: revision === "0" ? "1" : "0" })).error.code, "STALE_CONTEXT");
    assert.equal(api.apply_formula_completion_v3(JSON.stringify({ ...apply, candidateId: "untrusted" })).error.code, "UNKNOWN_CANDIDATE");
  }
  const valid = { source: "中\\fr", cursor: 6, revision: "0", inputFormat: "latex" };
  for (const [changed, code] of [
    [{ revision: 1 }, "INVALID_JSON"], [{ revision: "01" }, "INVALID_JSON"],
    [{ revision: "18446744073709551616" }, "INVALID_JSON"],
    [{ source: "\ud800", cursor: 0 }, "INVALID_JSON"],
    [{ inputFormat: "typst" }, "INVALID_JSON"], [{ insert: "untrusted" }, "INVALID_JSON"],
    [{ cursor: 1 }, "INVALID_CURSOR"], [{ source: "x".repeat(65537) }, "LIMIT_EXCEEDED"],
    [{ source: "x".repeat(4096) }, "LIMIT_EXCEEDED"],
  ]) {
    const failed = api.complete_formula_v3(JSON.stringify({ ...valid, ...changed }));
    assert.equal(failed.ok, false);
    assert.equal(failed.error.code, code, JSON.stringify(failed));
  }
  assert.equal(api.complete_formula_v3('{"source":"x","inputFormat":"latex","cursor":1,"revision":"0","revision":"1"}').error.code, "INVALID_JSON");
  assert.equal(api.complete_formula_v3(" ".repeat(1024 * 1024 + 1)).error.code, "INPUT_TOO_LARGE");
  assert.equal(api.complete_formula_v3(JSON.stringify(valid)).ok, true);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const prefix = process.argv[2] ?? "wasm-conversion-nodejs";
  assert.match(prefix, /^wasm(?:-[a-z]+)*-nodejs$/);
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
  completionSmoke(createRequire(import.meta.url)(resolve(root, `target/${prefix}/latexsnipper_wasm.js`)));
  console.log("Generated WASM completion: precise revisions, Unicode ranges, null metadata, limits and stale guards passed");
}

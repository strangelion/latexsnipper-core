import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

export function completionSmoke(api) {
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

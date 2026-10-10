import * as assert from "node:assert/strict";
import test from "node:test";
import { convertFormula, convertFormulaFragment, formulaConversionCapabilities, type WasmFormulaApi, type WasmFormulaFragmentApi } from "../src/formula.js";
import type { ApiEnvelopeV3 } from "../src/types.js";

const failure: ApiEnvelopeV3<never> = {
  ok: false,
  versions: {
    apiEnvelopeVersion: 3, capabilitySchemaVersion: 3, diagnosticSchemaVersion: 1,
    documentSchemaVersion: "3.0", coreVersion: "test",
  },
  error: { code: "UNSUPPORTED_FORMAT", message: "unsupported", recoverable: false },
};

test("formula helper defaults to strict and preserves the raw v3 result", () => {
  const calls: unknown[][] = [];
  const api: WasmFormulaApi = {
    formula_capabilities_v3: () => failure,
    convert_formula_v3: (...args) => { calls.push(args); return failure; },
  };
  assert.equal(formulaConversionCapabilities(api), failure);
  assert.equal(convertFormula(api, "x", { inputFormat: "latex", outputFormat: "omml" }), failure);
  assert.deepEqual(calls[0], ["x", "latex", "omml", "strict"]);
  assert.equal(convertFormula(api, "frac(a,b)", {
    inputFormat: "typst", outputFormat: "latex", mode: "best-effort",
  }), failure);
  assert.deepEqual(calls[1], ["frac(a,b)", "typst", "latex", "best-effort"]);
});

test("bare formula helper uses the explicit additive export and preserves errors", () => {
  const calls: unknown[][] = [];
  const api: WasmFormulaFragmentApi = {
    formula_capabilities_v3: () => failure,
    convert_formula_v3: () => { throw new Error("legacy document export must not be used"); },
    convert_formula_fragment_v3: (...args) => { calls.push(args); return failure; },
  };
  assert.equal(convertFormulaFragment(api, "frac(a,b)", { inputFormat: "typst", mode: "best-effort" }), failure);
  assert.deepEqual(calls[0], ["frac(a,b)", "typst", "best-effort"]);
  assert.equal(convertFormulaFragment(api, "x", { inputFormat: "latex" }), failure);
  assert.deepEqual(calls[1], ["x", "latex", "strict"]);
});

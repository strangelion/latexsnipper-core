// Browser-only lifecycle fixture: real Core conversions plus a deliberate Wasm stall.
const profile = new URL(import.meta.url).searchParams.get("profile") ?? "baseline";
const packages = { baseline: "wasm-fragment-web", full: "wasm-full-web", conversion: "wasm-conversion-web" };
if (!Object.hasOwn(packages, profile)) throw new Error("Unknown test WASM profile");
const core = await import(new URL(`../../../../target/${packages[profile]}/latexsnipper_wasm.js`, import.meta.url).href);

const spinModule = new WebAssembly.Module(new Uint8Array([
  0, 97, 115, 109, 1, 0, 0, 0,
  1, 4, 1, 96, 0, 0,
  3, 2, 1, 0,
  7, 8, 1, 4, 115, 112, 105, 110, 0, 0,
  10, 9, 1, 7, 0, 3, 64, 12, 0, 11, 11,
]));
const spin = new WebAssembly.Instance(spinModule).exports.spin;

export default async function initialize() {
  await core.default();
}

export function convert_formula_v3(content, input, output, mode) {
  if (content === "fixture:oversize") {
    const result = core.convert_formula_v3("x", "latex", "omml", "strict");
    result.data.content = "x".repeat(256 * 1024 + 1);
    return result;
  }
  if (content === "fixture:infinite") {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  return core.convert_formula_v3(content, input, output, mode);
}

export const convert_formula_fragment_v3 = core.convert_formula_fragment_v3;

export function create_formula_preferences_v1(configurationJson) {
  const owner = core.create_formula_preferences_v1(configurationJson);
  return {
    close: () => owner.close(), free: () => owner.free(),
    command(commandJson) {
      const command = JSON.parse(commandJson);
      if (command.request?.source === "fixture:infinite") {
        postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
        spin();
      }
      return owner.command(commandJson);
    },
  };
}

export function restore_formula_preferences_v1(configurationJson) {
  const owner = core.restore_formula_preferences_v1(configurationJson);
  // A valid whitespace suffix stalls AFTER actual restoration/owner allocation,
  // making timeout/abort prove that unknown state is discarded, never replayed.
  if (JSON.parse(configurationJson).snapshotJson.endsWith("\n")) {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  return owner;
}

export function complete_formula_v3(requestJson) {
  const request = JSON.parse(requestJson);
  if (request.source === "fixture:infinite") {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  if (request.source === "fixture:oversize") {
    const result = core.complete_formula_v3(JSON.stringify({ source: "\\fr", cursor: 3, inputFormat: "latex", revision: "0" }));
    result.data.completion.candidates[0].insert = "x".repeat(1024 * 1024 + 64 * 1024 + 1);
    return result;
  }
  return core.complete_formula_v3(requestJson);
}
export const apply_formula_completion_v3 = core.apply_formula_completion_v3;

export function complete_formula_scoped_v3(requestJson) {
  const request = JSON.parse(requestJson);
  if (request.source === "fixture:infinite") {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  if (request.source === "fixture:oversize") {
    const result = core.complete_formula_scoped_v3(JSON.stringify({ ...request, source: "\\cust", cursor: 5 }));
    result.data.candidates[0].edit.insert = "x".repeat(1024 * 1024 + 64 * 1024 + 1);
    return result;
  }
  return core.complete_formula_scoped_v3(requestJson);
}
export const apply_formula_completion_scoped_v3 = core.apply_formula_completion_scoped_v3;

export function complete_formula_fragments_v3(requestJson) {
  const request = JSON.parse(requestJson);
  if (request.source === "fixture:infinite") {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  if (request.source === "fixture:oversize") {
    const result = core.complete_formula_fragments_v3(JSON.stringify({ ...request, source: "\\bay", cursor: 4 }));
    result.data.candidates[0].edit.insert = "x".repeat(1024 * 1024 + 64 * 1024 + 1);
    return result;
  }
  return core.complete_formula_fragments_v3(requestJson);
}
export function apply_formula_completion_fragments_v3(requestJson) {
  const request = JSON.parse(requestJson);
  if (request.source === "fixture:infinite") {
    postMessage({ protocolVersion: 1, type: "progress", requestId: "fixture:probe", stage: "fixture-spin", progress: 0 });
    spin();
  }
  return core.apply_formula_completion_fragments_v3(requestJson);
}

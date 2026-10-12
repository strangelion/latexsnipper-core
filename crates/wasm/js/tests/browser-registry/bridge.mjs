// App-owned test bridge, explicitly selected at Worker initialization.
import initialize, * as api from "/wasm/latexsnipper_wasm.js";
export default () => initialize({ module_or_path: "/wasm/latexsnipper_wasm_bg.wasm" });
export const init = api.init;
export const convert_formula_v3 = api.convert_formula_v3;
export const convert_formula_fragment_v3 = api.convert_formula_fragment_v3;
export function create_formula_registry_v1(key) {
  if (key === undefined) return api.create_formula_registry_v1();
  if (key !== "fixture:power" && key !== "fixture:loop") throw new Error("Unknown app-owned configuration key");
  const registry = api.create_formula_registry_v1();
  try {
    const format = registry.register_format(JSON.stringify({ schemaVersion: 1, id: "fixture:power",
      mimeType: "application/vnd.fixture.power", description: "Finite power only" }), (value) => /^[a-z]:[0-9]$/.test(value));
    if (!format.ok) throw new Error(format.error.code);
    const backend = registry.register_backend(JSON.stringify({ schemaVersion: 1, id: "fixture:js", version: "1.0.0",
      configurationSha256: (key === "fixture:loop" ? "c" : "b").repeat(64), license: "AGPL-3.0",
      limitations: ["Trusted local fixture, not a browser WASI sandbox"],
      routes: [{ input: "fixture:power", output: "omml", mode: "best-effort", target: "wasm32-unknown-unknown" }],
      maxInputBytes: 65536, maxOutputBytes: 262144 }), () => true, (json) => {
        const content = JSON.parse(json).content;
        if (key === "fixture:loop" && content === "z:9") { while (true) { /* hard-interruption fixture */ } }
        const result = api.convert_formula_v3(`${content[0]}^${content[2]}`, "latex", "omml", "strict");
        if (!result.ok) throw new Error(result.error.code);
        return JSON.stringify({ content: result.data.content, losses: [] });
      });
    if (!backend.ok) throw new Error(backend.error.code);
    return registry;
  } catch (error) { registry.close(); registry.free(); throw error; }
}

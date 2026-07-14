import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../../..");
for (const packageName of ["wasm-web", "wasm-bundler", "wasm-node"]) {
  assert.equal(existsSync(resolve(root, `target/${packageName}/LICENSE`)), true);
}
const web = await import(pathToFileURL(resolve(root, "target/wasm-web/latexsnipper_wasm.js")));
assert.equal(typeof web.default, "function");
assert.equal(typeof web.capabilities_v2, "function");

const nodeImported = await import(pathToFileURL(resolve(root, "target/wasm-node/latexsnipper_wasm.js")));
const node = nodeImported.default ?? nodeImported;
assert.equal(typeof node.api_info_v2, "function");
const info = node.api_info_v2();
assert.equal(info.ok, true);
assert.equal(info.data.wasmApiVersion, 2);

const bundler = await import(pathToFileURL(resolve(root, "target/wasm-bundler/latexsnipper_wasm.js")));
assert.equal(typeof bundler.capabilities_v2, "function");

console.log("web ESM, Node, and bundler package imports passed");

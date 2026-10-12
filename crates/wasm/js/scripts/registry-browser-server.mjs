// Loopback-only allowlisted fixture server; never expose arbitrary repository files.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const prefix = process.argv[2] ?? "wasm-registry-web";
if (!["wasm-registry-web", "wasm-conversion-web"].includes(prefix)) throw new Error("Unknown WASM fixture prefix");
const server = createServer(async (request, response) => {
  try {
    const path = new URL(request.url, "http://localhost").pathname;
    if (path === "/favicon.ico") { response.writeHead(204); response.end(); return; }
    if (path === "/") {
      response.writeHead(200, { "content-type": "text/html", "cache-control": "no-store" });
      response.end('<!doctype html><title>Core registry Worker test</title><link rel="icon" href="data:,"><pre>Running</pre><script type="module" src="/fixture/test.mjs"></script>'); return;
    }
    let file;
    if (/^\/sdk\/[a-z-]+\.js$/.test(path)) file = resolve(root, "crates/wasm/js/dist", path.slice(5));
    if (["/wasm/latexsnipper_wasm.js", "/wasm/latexsnipper_wasm_bg.wasm"].includes(path)) file = resolve(root, "target", prefix, path.slice(6));
    if (["/fixture/bridge.mjs", "/fixture/test.mjs"].includes(path)) file = resolve(root, "crates/wasm/js/tests/browser-registry", path.slice(9));
    if (!file) { response.writeHead(404); response.end(); return; }
    const bytes = await readFile(file);
    response.writeHead(200, { "content-type": file.endsWith(".wasm") ? "application/wasm" : "text/javascript",
      "cache-control": "no-store" }); response.end(bytes);
  } catch { response.writeHead(404); response.end(); }
});
server.listen(0, "127.0.0.1", () => console.log(`REGISTRY_BROWSER_URL=http://127.0.0.1:${server.address().port}/`));
process.on("SIGINT", () => server.close(() => process.exit(0)));

// Loopback-only static conformance server; never serves arbitrary repository files.
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { stat, realpath } from "node:fs/promises";
import { dirname, resolve, sep, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const port = Number(process.argv[2] ?? 8766);
if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error("Invalid loopback test port");
const prefixes = ["/crates/wasm/js/dist/", "/target/wasm-fragment-web/"];
const fixtures = ["/crates/wasm/js/scripts/formula-worker-browser-fixture.mjs", "/crates/wasm/js/scripts/formula-worker-browser-smoke.html"];
const mime = { ".js": "text/javascript", ".mjs": "text/javascript", ".html": "text/html", ".wasm": "application/wasm" };
const realRoot = await realpath(root);
const assetRoots = prefixes.map((prefix) => resolve(realRoot, `.${prefix}`) + sep);
const fixtureFiles = fixtures.map((fixture) => resolve(realRoot, `.${fixture}`));

createServer(async (request, response) => {
  try {
    const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
    const file = resolve(root, `.${path}`);
    if (request.method !== "GET" || !file.startsWith(root + sep)
      || !(fixtures.includes(path) || prefixes.some((prefix) => path.startsWith(prefix)))
      || !mime[extname(file)]) {
      response.writeHead(404).end();
      return;
    }
    const actualFile = await realpath(file);
    if (!(fixtureFiles.includes(actualFile) || assetRoots.some((base) => actualFile.startsWith(base))) || !mime[extname(actualFile)]) {
      response.writeHead(404).end();
      return;
    }
    const info = await stat(actualFile);
    if (!info.isFile() || info.size > 64 * 1024 * 1024) throw new Error("Unavailable test asset");
    response.writeHead(200, { "Content-Type": mime[extname(file)], "Content-Length": info.size, "Cache-Control": "no-store" });
    const stream = createReadStream(actualFile);
    stream.on("error", () => response.destroy());
    stream.pipe(response);
  } catch {
    response.writeHead(404).end();
  }
}).listen(port, "127.0.0.1", () => console.log(`Formula Worker conformance: http://127.0.0.1:${port}/crates/wasm/js/scripts/formula-worker-browser-smoke.html`));

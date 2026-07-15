import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import wasm from "vite-plugin-wasm";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../../..");
const nodeEntry = resolve(repositoryRoot, "target/wasm-nodejs/latexsnipper_wasm.js");
const webEntry = resolve(repositoryRoot, "target/wasm-web/latexsnipper_wasm.js");
const bundlerEntry = resolve(repositoryRoot, "target/wasm-bundler/latexsnipper_wasm.js");

const require = createRequire(import.meta.url);
const nodePackage = require(nodeEntry);
const apiInfo = nodePackage.api_info_v2();
if (!apiInfo || apiInfo.ok !== true) {
  throw new Error("Node package did not return a successful API envelope");
}

const webPackage = await import(pathToFileURL(webEntry).href);
if (typeof webPackage.default !== "function") {
  throw new Error("Web ESM package does not expose the async initializer");
}

const normalizedBundlerEntry = bundlerEntry.replaceAll("\\", "/");
await build({
  configFile: false,
  logLevel: "silent",
  plugins: [
    wasm(),
    {
      name: "latexsnipper-generated-package-smoke",
      resolveId(id) {
        return id === "virtual:entry" ? "\0virtual:entry" : null;
      },
      load(id) {
        if (id !== "\0virtual:entry") return null;
        return `import { api_info_v2 } from ${JSON.stringify(normalizedBundlerEntry)}; export { api_info_v2 };`;
      },
    },
  ],
  build: {
    write: false,
    target: "esnext",
    rollupOptions: { input: "virtual:entry" },
  },
});

console.log("Node, web ESM, and bundler package smoke passed");

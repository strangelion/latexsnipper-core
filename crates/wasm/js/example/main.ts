import { WasmWorkerClient, browserRuntimeCapabilities } from "../src/index.js";

const status = document.querySelector<HTMLParagraphElement>("#status")!;
const workerUrl = new URL("../src/worker-entry.ts", import.meta.url);
const client = new WasmWorkerClient({
  workerUrl,
  moduleUrl: "/wasm/latexsnipper_wasm.js",
  wasmUrl: "/wasm/latexsnipper_wasm_bg.wasm",
});

void client.ready().then(
  () => { status.textContent = `Worker ready: ${JSON.stringify(browserRuntimeCapabilities())}`; },
  (error: unknown) => { status.textContent = `Worker startup failed: ${String(error)}`; },
);

document.querySelector<HTMLButtonElement>("#cancel")!.addEventListener("click", () => {
  status.textContent = client.cancel("active-request") ? "Request cancelled; worker restarting" : "No active request";
});

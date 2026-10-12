import { WasmWorkerClient } from "/sdk/client.js";
const check = (condition, message) => { if (!condition) throw new Error(message); };
const request = (content) => ({ schemaVersion: 1, input: "fixture:power", output: "omml", mode: "best-effort",
  backend: "fixture:js", content, contextSha256: "a".repeat(64) });
const client = new WasmWorkerClient({ workerUrl: "/sdk/worker-entry.js", moduleUrl: "/fixture/bridge.mjs",
  maxFormulaDurationMillis: 5000 });
globalThis.registryTestResult = { state: "running" };
try {
  await client.ready();
  const registry = await client.createFormulaRegistry("fixture:power");
  check((await registry.convert(request("x:2"))).data.outcome.error.code === "BACKEND_DISABLED", "Default route enabled unexpectedly");
  check((await registry.setEnabled("fixture:js", true)).ok, "Explicit enable failed");
  const first = await registry.convert(request("x:2"));
  const second = await registry.convert(request("y:4"));
  check(first.data.outcome.kind === "converted" && second.data.outcome.kind === "converted", "Dynamic conversion failed");
  check(first.data.outcome.result.candidate.content !== second.data.outcome.result.candidate.content, "Output ignored source");
  check(first.data.decisionSha256 !== second.data.decisionSha256, "Decision identity did not change");
  await registry.close();
  const loop = await client.createFormulaRegistry("fixture:loop");
  await loop.setEnabled("fixture:js", true);
  const abort = new AbortController();
  const active = loop.convert(request("z:9"), undefined, { signal: abort.signal });
  const cancelled = active.then(() => "unexpected-success", (error) => error.detail?.code);
  await new Promise((resolve) => setTimeout(resolve, 100));
  const queued = loop.capabilities().then(() => "unexpected-success", (error) => error.detail?.code);
  abort.abort();
  check(await cancelled === "CANCELLED", "Active callback was not hard-cancelled");
  check(await queued === "WORKER_REGISTRY_STALE", "Old queued registry command was replayed");
  await client.ready();
  const stale = await loop.capabilities().then(() => "unexpected-success", (error) => error.detail?.code);
  check(stale === "WORKER_REGISTRY_STALE", "Old handle survived reset");
  const timeout = await client.createFormulaRegistry("fixture:loop");
  await timeout.setEnabled("fixture:js", true);
  const timed = await timeout.convert(request("z:9"), undefined, { timeoutMillis: 100 }).then(
    () => "unexpected-success", (error) => error.detail?.code);
  check(timed === "WORKER_TASK_TIMEOUT", "Infinite callback did not time out");
  await client.ready();
  const fresh = await client.createFormulaRegistry();
  const builtin = await fresh.convert({ ...request("x^2"), input: "latex", backend: null, mode: "strict" });
  check(builtin.data.outcome.kind === "converted", "Explicit fresh registry failed after recovery");
  await fresh.close();
  globalThis.registryTestResult = { state: "passed", dynamicOutput: true, defaultDisabled: true,
    hardAbort: true, timeout: true, staleInvalidation: true, noRegistryReplay: true, freshRecovery: true };
} catch (error) {
  globalThis.registryTestResult = { state: "failed", message: error instanceof Error ? error.message : String(error) };
} finally {
  client.terminate();
  document.querySelector("pre").textContent = JSON.stringify(globalThis.registryTestResult, null, 2);
}

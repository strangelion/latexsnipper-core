import * as assert from "node:assert/strict";
import test from "node:test";
import { FormulaRegistry, type NativeFormulaRegistry, type RegisteredFormulaRequest } from "../src/registry.js";
import { WasmWorkerClient, WorkerRuntimeError } from "../src/client.js";
import { validateWorkerRequest } from "../src/worker-request-validation.js";
import type { WorkerLike, WorkerRequest, WorkerResponse } from "../src/types.js";
import { WorkerFormulaRegistry } from "../src/worker-registry.js";

const request: RegisteredFormulaRequest = { schemaVersion: 1, input: "latex", output: "omml", mode: "strict",
  backend: null, content: "x^2", contextSha256: "a".repeat(64) };

class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null = null;
  requests: WorkerRequest[] = [];
  terminated = false;
  postMessage(value: WorkerRequest): void {
    this.requests.push(value);
    if (value.type === "initialize" || value.type === "formula-registry-create") queueMicrotask(() =>
      this.emit(value.requestId, value.type === "initialize" ? true : { registryId: "registry:1" }));
  }
  emit(requestId: string, data: unknown): void {
    this.onmessage?.({ data: { protocolVersion: 1, type: "result", requestId, data } } as MessageEvent<WorkerResponse>);
  }
  terminate(): void { this.terminated = true; }
}
const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

test("closed registry wire shape rejects URLs, unknown fields and UTF-8 overflow", () => {
  const base = { protocolVersion: 1, requestId: "r", type: "formula-registry-command", registryId: "registry:1",
    command: { kind: "convert", request, policy: { schemaVersion: 1, backends: [] } } };
  assert.equal(validateWorkerRequest(base).ok, true);
  assert.equal(validateWorkerRequest({ ...base, moduleUrl: "https://evil.invalid/module.js" }).ok, false);
  assert.equal(validateWorkerRequest({ ...base, command: { ...base.command, request: { ...request, ignored: true } } }).ok, false);
  assert.equal(validateWorkerRequest({ ...base, command: { ...base.command, request: { ...request, content: "中".repeat(30000) } } }).ok, false);
  assert.equal(validateWorkerRequest({ protocolVersion: 1, type: "formula-registry-create", requestId: "r", configurationKey: "https://example.invalid" }).ok, false);
});

test("worker owns queued input; closure and restart never replay old registry commands", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => {
    const worker = new FakeWorker(); workers.push(worker); return worker;
  } });
  const registry = await client.createFormulaRegistry("demo:power");
  const input = { ...request };
  const controller = new AbortController();
  const active = registry.convert(input, undefined, { signal: controller.signal });
  input.content = "changed";
  await tick();
  const command = workers[0]!.requests.find((value) => value.type === "formula-registry-command");
  assert.ok(command?.type === "formula-registry-command" && command.command.kind === "convert");
  assert.equal(command.command.request.content, "x^2");
  const queued = registry.capabilities();
  const activeRejected = assert.rejects(active, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED");
  const queuedRejected = assert.rejects(queued, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_REGISTRY_STALE");
  controller.abort();
  await Promise.all([activeRejected, queuedRejected]);
  await client.ready();
  assert.equal(workers[0]!.terminated, true);
  assert.equal(workers[1]!.requests.some((value) => value.type === "formula-registry-command" || value.type === "formula-registry-create"), false);
  await assert.rejects(registry.convert(request), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_REGISTRY_STALE");
  const fresh = await client.createFormulaRegistry();
  const closing = fresh.close();
  await tick();
  const close = workers[1]!.requests.find((value) => value.type === "formula-registry-command");
  assert.ok(close);
  workers[1]!.emit(close.requestId, { ok: true, data: true });
  await closing;
  assert.equal(fresh.closed, true);
  client.terminate();
});

test("direct wrapper defers free during a close from its trusted callback", () => {
  let freed = 0;
  let registry: FormulaRegistry;
  const native = { convert: () => { registry.close(); return { ok: false }; }, close: () => ({ ok: true }),
    free: () => { freed += 1; } } as unknown as NativeFormulaRegistry;
  registry = new FormulaRegistry({ create_formula_registry_v1: () => native });
  registry.convert(request);
  assert.equal(registry.closed, true);
  assert.equal(freed, 1);
  registry.close(); assert.equal(freed, 1);
});

test("direct wrapper frees even when a trusted close callback throws", () => {
  let freed = 0;
  const native = { close: () => { throw new Error("fixture close failure"); },
    free: () => { freed += 1; } } as unknown as NativeFormulaRegistry;
  const registry = new FormulaRegistry({ create_formula_registry_v1: () => native });
  assert.throws(() => registry.close(), /fixture close failure/);
  assert.equal(registry.closed, true);
  registry.close();
  assert.equal(freed, 1);
});

test("worker close can be retried after admission failure and concurrent closes share completion", async () => {
  let calls = 0;
  let finish: (() => void) | undefined;
  const registry = new WorkerFormulaRegistry(() => {
    calls += 1;
    if (calls === 1) return Promise.reject(new Error("fixture queue full"));
    return new Promise((resolve) => { finish = () => resolve(true); });
  });
  await assert.rejects(registry.close(), /queue full/);
  assert.equal(registry.closed, false);
  const closing = registry.close();
  assert.equal(registry.close(), closing);
  await tick();
  assert.equal(calls, 2);
  await assert.rejects(registry.capabilities(), /closed/);
  finish!();
  await closing;
  assert.equal(registry.closed, true);
  await registry.close();
  assert.equal(calls, 2);
});

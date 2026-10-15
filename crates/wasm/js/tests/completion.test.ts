import * as assert from "node:assert/strict";
import test from "node:test";
import { completeFormula, applyFormulaCompletion, utf16ToFormulaByteOffset, formulaByteOffsetToUtf16,
  type WasmFormulaCompletionApi } from "../src/completion.js";
import { WasmWorkerClient, WorkerRuntimeError } from "../src/client.js";
import { validateWorkerRequest } from "../src/worker-request-validation.js";
import type { ApiEnvelopeV3, WorkerLike, WorkerRequest, WorkerResponse } from "../src/types.js";

const source = "中😀\\fr";
const input = { source, inputFormat: "latex" as const, cursor: 10, revision: "18446744073709551615" };
const snapshot = { schemaVersion: 1, sourceSha256: "a".repeat(64), cursor: 10, revision: input.revision };
const apply = { ...input, snapshot, candidateId: "builtin:frac" };
const failure: ApiEnvelopeV3<never> = { ok: false,
  versions: { apiEnvelopeVersion: 3, capabilitySchemaVersion: 3, diagnosticSchemaVersion: 1, documentSchemaVersion: "3.0", coreVersion: "test" },
  error: { code: "STALE_CONTEXT", message: "stale", recoverable: false } };

test("completion offsets preserve Unicode boundaries and refuse surrogate replacement", () => {
  for (const [units, bytes] of [[0, 0], [1, 3], [3, 7], [4, 8], [5, 9], [6, 10]]) {
    assert.equal(utf16ToFormulaByteOffset(source, units!), bytes);
    assert.equal(formulaByteOffsetToUtf16(source, bytes!), units);
  }
  for (const offset of [2, -1, 7, 1.5, NaN]) assert.throws(() => utf16ToFormulaByteOffset(source, offset), RangeError);
  for (const offset of [1, 2, 4, 5, 6, -1, 11, 1.5]) assert.throws(() => formulaByteOffsetToUtf16(source, offset), RangeError);
  for (const invalid of ["\ud800", "\udc00", "x\ud800y"]) {
    assert.throws(() => utf16ToFormulaByteOffset(invalid, 0), RangeError);
    assert.throws(() => formulaByteOffsetToUtf16(invalid, 0), RangeError);
  }
});

test("direct completion helpers preserve decimal strings and Core failures", () => {
  const seen: string[] = [];
  const api: WasmFormulaCompletionApi = { formula_completion_capabilities_v3: () => failure,
    complete_formula_v3: (value) => { seen.push(value); return failure; },
    apply_formula_completion_v3: (value) => { seen.push(value); return failure; } };
  assert.equal(completeFormula(api, input), failure);
  assert.equal(applyFormulaCompletion(api, apply), failure);
  assert.deepEqual(JSON.parse(seen[0]!), input);
  assert.deepEqual(JSON.parse(seen[1]!), apply);
});

test("completion Worker messages are closed and validate exact revisions and offsets", () => {
  const message = { protocolVersion: 1, type: "complete-formula", requestId: "complete:1", input };
  assert.equal(validateWorkerRequest(message).ok, true);
  assert.equal(validateWorkerRequest({ ...message, type: "apply-formula-completion", input: apply }).ok, true);
  assert.equal(validateWorkerRequest({ ...message, input: { source: "", cursor: 0, inputFormat: "latex", revision: "0" } }).ok, true);
  for (const changed of [
    { revision: 1 }, { revision: "01" }, { revision: "18446744073709551616" }, { revision: "1\n" },
    { revision: "-1" }, { revision: "1e3" }, { inputFormat: "typst" }, { cursor: 4 }, { cursor: 11 },
    { source: "\ud800", cursor: 0 }, { source: "中".repeat(22000), cursor: 0 }, { insert: "untrusted" },
  ]) assert.equal(validateWorkerRequest({ ...message, input: { ...input, ...changed } }).ok, false, JSON.stringify(changed));
  for (const changed of [{ moduleUrl: "https://elsewhere.invalid/" }, { backend: "untrusted" }]) {
    assert.equal(validateWorkerRequest({ ...message, ...changed }).ok, false);
  }
  for (const changed of [{ revision: 1 }, { insert: "untrusted" }, { sourceSha256: "a".repeat(64) + "\n" }, { cursor: 65537 }]) {
    assert.equal(validateWorkerRequest({ ...message, type: "apply-formula-completion", input: { ...apply, snapshot: { ...snapshot, ...changed } } }).ok, false);
  }
});

class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror = null;
  onmessageerror = null;
  readonly requests: WorkerRequest[] = [];
  terminated = false;
  postMessage(request: WorkerRequest): void {
    this.requests.push(request);
    if (request.type === "initialize") queueMicrotask(() => this.emit(request.requestId, true));
  }
  terminate(): void { this.terminated = true; }
  emit(requestId: string, data: unknown): void {
    this.onmessage?.({ data: { protocolVersion: 1, type: "result", requestId, data } } as MessageEvent<WorkerResponse>);
  }
  completion(): Extract<WorkerRequest, { type: "complete-formula" | "apply-formula-completion" }> {
    const request = this.requests.find((value) => value.type === "complete-formula" || value.type === "apply-formula-completion");
    assert.ok(request);
    return request;
  }
}
const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));
const errorCode = (code: string) => (value: unknown): boolean => value instanceof WorkerRuntimeError && value.detail.code === code;

test("completion queue owns snapshots and keeps bounded result and cancellation behavior", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxQueueLength: 1,
    maxCompletionResultBytes: 100, workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  await client.ready();
  const abort = new AbortController();
  const pending = client.completeFormula({ ...input, requestId: "first" }, { signal: abort.signal });
  const rejected = assert.rejects(pending, errorCode("CANCELLED"));
  await tick();
  const oldWire = workers[0]!.completion();
  const owned = { ...apply, snapshot: { ...snapshot } };
  const next = client.applyFormulaCompletion(owned);
  owned.snapshot.revision = "0";
  await assert.rejects(client.completeFormula(input), errorCode("WORKER_QUEUE_FULL"));
  abort.abort();
  await rejected;
  assert.equal(workers[0]!.terminated, true);
  await client.ready(); await tick();
  const wire = workers[1]!.completion();
  assert.equal(wire.type, "apply-formula-completion");
  if (wire.type === "apply-formula-completion") assert.equal(wire.input.snapshot.revision, snapshot.revision);
  workers[0]!.emit(oldWire.requestId, "late result");
  workers[1]!.emit(wire.requestId, failure);
  await assert.rejects(next, errorCode("WORKER_RESULT_LIMIT"));
  const healthy = client.completeFormula(input);
  await tick();
  const latest = workers[1]!.requests.at(-1)!;
  workers[1]!.emit(latest.requestId, { ok: true });
  assert.deepEqual(await healthy, { ok: true });
  assert.equal(workers.every((worker) => !worker.requests.some((value) => value.type === "load-model")), true);
  client.terminate();
});

test("completion active deadline restarts the Worker without retrying the old task", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => {
    const worker = new FakeWorker(); workers.push(worker); return worker;
  } });
  await client.ready();
  await assert.rejects(client.completeFormula(input, { timeoutMillis: 10 }), errorCode("WORKER_TASK_TIMEOUT"));
  await client.ready();
  assert.equal(workers[0]!.terminated, true);
  assert.equal(workers[1]!.requests.some((value) => value.type === "complete-formula"), false);
  await assert.rejects(client.completeFormula({ ...input, revision: "01" }), errorCode("WORKER_INVALID_REQUEST"));
  client.terminate();
});

test("pre-aborted and queued completion cancellation keep the active worker intact", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  await client.ready();
  const signal = new AbortController(); signal.abort();
  await assert.rejects(client.completeFormula(input, { signal: signal.signal }), errorCode("CANCELLED"));
  const active = client.completeFormula(input);
  await tick();
  const queuedSignal = new AbortController();
  const queued = client.applyFormulaCompletion(apply, { signal: queuedSignal.signal });
  const rejected = assert.rejects(queued, errorCode("CANCELLED"));
  queuedSignal.abort(); await rejected;
  assert.equal(worker.terminated, false);
  worker.emit(worker.completion().requestId, { ok: true });
  await active;
  assert.equal(worker.requests.filter((request) => request.type === "apply-formula-completion").length, 0);
  client.terminate();
});

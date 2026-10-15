import * as assert from "node:assert/strict";
import test from "node:test";
import { completeFormula, applyFormulaCompletion, utf16ToFormulaByteOffset, formulaByteOffsetToUtf16,
  completeFormulaScoped, applyFormulaCompletionScoped, type ScopedFormulaCompletionRequest,
  type ScopedFormulaCompletionApplyRequest, type WasmScopedFormulaCompletionApi,
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
  completion(): Extract<WorkerRequest, { type: "complete-formula" | "apply-formula-completion" | "complete-formula-scoped" | "apply-formula-completion-scoped" }> {
    const request = this.requests.find((value) => value.type === "complete-formula" || value.type === "apply-formula-completion" || value.type === "complete-formula-scoped" || value.type === "apply-formula-completion-scoped");
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

const scopedInput: ScopedFormulaCompletionRequest = { source: "中\\cust", cursor: 8, revision: input.revision, inputFormat: "latex",
  catalog: { scope: { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: input.revision },
    definitions: [{ id: "pair", revision: input.revision, userSha256: "a".repeat(64), projectSha256: "b".repeat(64), command: "custompair", arguments: ["math", "text"] }] } };
const scopedApply: ScopedFormulaCompletionApplyRequest = { ...scopedInput, candidateId: "definition:pair",
  snapshot: { schemaVersion: 2, sourceSha256: "d".repeat(64), definitionsSha256: "e".repeat(64), cursor: 8, revision: input.revision } };

test("scoped direct helpers retain Core results and exact catalog generations", () => {
  const seen: string[] = [];
  const api: WasmScopedFormulaCompletionApi = { formula_scoped_completion_capabilities_v3: () => failure,
    complete_formula_scoped_v3: (value) => { seen.push(value); return failure; },
    apply_formula_completion_scoped_v3: (value) => { seen.push(value); return failure; } };
  assert.equal(completeFormulaScoped(api, scopedInput), failure);
  assert.equal(applyFormulaCompletionScoped(api, scopedApply), failure);
  assert.deepEqual(JSON.parse(seen[0]!), scopedInput);
  assert.deepEqual(JSON.parse(seen[1]!), scopedApply);
});

test("scoped messages enforce closed transport without inspecting foreign private signatures", () => {
  const message = { protocolVersion: 1, type: "complete-formula-scoped", requestId: "scope:1", input: scopedInput };
  assert.equal(validateWorkerRequest(message).ok, true);
  assert.equal(validateWorkerRequest({ ...message, type: "apply-formula-completion-scoped", input: scopedApply }).ok, true);
  assert.equal(validateWorkerRequest({ ...message, type: "complete-formula" }).ok, false);
  assert.equal(validateWorkerRequest({ ...message, input }).ok, false);
  for (const field of ["generation", "definitionRevision", "scopeExtra", "definitionBody", "catalogExtra", "source", "budget", "count"]) {
    const invalid = structuredClone(scopedInput) as unknown as Record<string, any>;
    switch (field) {
      case "generation": invalid.catalog.scope.generation = 0; break;
      case "definitionRevision": invalid.catalog.definitions[0].revision = "01"; break;
      case "scopeExtra": invalid.catalog.scope.extra = true; break;
      case "definitionBody": invalid.catalog.definitions[0].body = "untrusted"; break;
      case "catalogExtra": invalid.catalog.extra = true; break;
      case "source": invalid.source = "\ud800"; invalid.cursor = 0; break;
      case "budget": invalid.catalog.definitions[0].command = "x".repeat(65536); break;
      case "count": invalid.catalog.definitions = Array(65).fill(invalid.catalog.definitions[0]); break;
    }
    assert.equal(validateWorkerRequest({ ...message, input: invalid }).ok, false, field);
  }
  for (const changed of [{ schemaVersion: 1 }, { definitionsSha256: "bad" }, { definitionsSha256: "a".repeat(64) + "\n" }, { insert: "untrusted" }]) {
    assert.equal(validateWorkerRequest({ ...message, type: "apply-formula-completion-scoped", input: { ...scopedApply, snapshot: { ...scopedApply.snapshot, ...changed } } }).ok, false);
  }
  const foreign = structuredClone(scopedInput);
  foreign.catalog.definitions[0]!.userSha256 = "f".repeat(64);
  foreign.catalog.definitions[0]!.command = "privateInvalid123";
  foreign.catalog.definitions[0]!.arguments = Array(20).fill("math");
  assert.equal(validateWorkerRequest({ ...message, input: foreign }).ok, true);
});

test("scoped queue owns catalog and snapshot, discards old responses and bounds results", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxQueueLength: 1,
    maxCompletionResultBytes: 100, workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  await client.ready();
  const abort = new AbortController();
  const pending = client.completeFormulaScoped(scopedInput, { signal: abort.signal });
  const rejected = assert.rejects(pending, errorCode("CANCELLED"));
  await tick();
  const old = workers[0]!.completion();
  const owned = structuredClone(scopedApply);
  const next = client.applyFormulaCompletionScoped(owned);
  owned.catalog.scope.generation = "0";
  owned.catalog.definitions[0]!.command = "mutated";
  owned.snapshot.definitionsSha256 = "0".repeat(64);
  await assert.rejects(client.completeFormulaScoped(scopedInput), errorCode("WORKER_QUEUE_FULL"));
  abort.abort(); await rejected; await client.ready(); await tick();
  const wire = workers[1]!.completion();
  assert.equal(wire.type, "apply-formula-completion-scoped");
  if (wire.type === "apply-formula-completion-scoped") assert.deepEqual(wire.input, scopedApply);
  workers[0]!.emit(old.requestId, "late result");
  workers[1]!.emit(wire.requestId, failure);
  await assert.rejects(next, errorCode("WORKER_RESULT_LIMIT"));
  client.terminate();
});

test("scoped timeout never replays an edit and queued abort does not terminate active work", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => {
    const worker = new FakeWorker(); workers.push(worker); return worker;
  } });
  await client.ready();
  await assert.rejects(client.applyFormulaCompletionScoped(scopedApply, { timeoutMillis: 10 }), errorCode("WORKER_TASK_TIMEOUT"));
  await client.ready();
  assert.equal(workers[0]!.terminated, true);
  assert.equal(workers[1]!.requests.some((value) => value.type === "apply-formula-completion-scoped"), false);
  const active = client.completeFormulaScoped(scopedInput); await tick();
  const abort = new AbortController();
  const queued = client.applyFormulaCompletionScoped(scopedApply, { signal: abort.signal });
  const rejected = assert.rejects(queued, errorCode("CANCELLED")); abort.abort(); await rejected;
  assert.equal(workers[1]!.terminated, false);
  workers[1]!.emit(workers[1]!.completion().requestId, { ok: true }); await active;
  await assert.rejects(client.completeFormulaScoped({ ...scopedInput, revision: "01" }), errorCode("WORKER_INVALID_REQUEST"));
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

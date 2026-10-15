import * as assert from "node:assert/strict";
import test from "node:test";
import { completeFormulaFragments, applyFormulaCompletionFragments, formulaFragmentCompletionCapabilities,
  type FormulaFragmentCompletionRequest, type FormulaFragmentCompletionApplyRequest,
  type WasmFragmentFormulaCompletionApi } from "../src/completion.js";
import { WasmWorkerClient, WorkerRuntimeError } from "../src/client.js";
import { validateWorkerRequest } from "../src/worker-request-validation.js";
import type { ApiEnvelopeV3, WorkerLike, WorkerRequest, WorkerResponse } from "../src/types.js";

const revision = "18446744073709551615";
const input: FormulaFragmentCompletionRequest = { source: "中😀\\bay", cursor: 11, revision, inputFormat: "latex", catalog: {
  commands: { scope: { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: revision }, definitions: [] },
  policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
  fragments: [{ id: "bayes", revision, userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), trigger: "bayes", label: "Local fraction", source: "\\frac{α}{}" }],
} };
const apply: FormulaFragmentCompletionApplyRequest = { ...input, candidateId: "fragment:bayes",
  snapshot: { schemaVersion: 3, sourceSha256: "d".repeat(64), catalogSha256: "e".repeat(64), cursor: 11, revision } };
const failure: ApiEnvelopeV3<never> = { ok: false,
  versions: { apiEnvelopeVersion: 3, capabilitySchemaVersion: 3, diagnosticSchemaVersion: 1, documentSchemaVersion: "3.0", coreVersion: "test" },
  error: { code: "STALE_CONTEXT", message: "stale", recoverable: false } };
const message = (value: unknown = input, type = "complete-formula-fragments") => ({ protocolVersion: 1, type, requestId: "fragments:1", input: value });

test("fragment helpers preserve the exact profile and never downgrade Core failures", () => {
  const seen: string[] = [];
  const api: WasmFragmentFormulaCompletionApi = { formula_fragment_completion_capabilities_v3: () => failure,
    complete_formula_fragments_v3: (json) => { seen.push(json); return failure; },
    apply_formula_completion_fragments_v3: (json) => { seen.push(json); return failure; } };
  assert.equal(completeFormulaFragments(api, input), failure);
  assert.equal(applyFormulaCompletionFragments(api, apply), failure);
  assert.equal(formulaFragmentCompletionCapabilities(api), failure);
  assert.deepEqual(seen.map((json) => JSON.parse(json)), [input, apply]);
});

test("fragment transport closes catalogs, policy, revisions and snapshots", () => {
  assert.equal(validateWorkerRequest(message()).ok, true);
  assert.equal(validateWorkerRequest(message(apply, "apply-formula-completion-fragments")).ok, true);
  assert.equal(validateWorkerRequest(message(input, "complete-formula-scoped")).ok, false);
  for (const mutate of [
    (value: any) => { delete value.catalog.policy.allowInSensitiveDocument; },
    (value: any) => { value.catalog.policy.retrievalEnabled = 1; },
    (value: any) => { value.catalog.policy.permission = true; },
    (value: any) => { value.catalog.commands.scope.generation = 0; },
    (value: any) => { value.catalog.fragments[0].revision = "01"; },
    (value: any) => { value.catalog.fragments[0].source = "\ud800"; },
    (value: any) => { value.catalog.fragments[0].execute = true; },
    (value: any) => { value.catalog.fragments = Array(65).fill(value.catalog.fragments[0]); },
    (value: any) => { value.catalog.fragments[0].source = "x".repeat(1024 * 1024); },
  ]) {
    const invalid = structuredClone(input); mutate(invalid);
    assert.equal(validateWorkerRequest(message(invalid)).ok, false);
  }
  for (const changed of [{ schemaVersion: 2 }, { catalogSha256: "bad" }, { definitionsSha256: "a".repeat(64) }, { revision: 0 }]) {
    assert.equal(validateWorkerRequest(message({ ...apply, snapshot: { ...apply.snapshot, ...changed } }, "apply-formula-completion-fragments")).ok, false);
  }
  const foreign = structuredClone(input);
  foreign.catalog.fragments[0]!.userSha256 = "foreign";
  foreign.catalog.fragments[0]!.trigger = "privateInvalid123";
  foreign.catalog.fragments[0]!.source = "PRIVATE_SECRET\\documentclass{article}";
  assert.equal(validateWorkerRequest(message(foreign)).ok, true, "Core filters before private semantic checks");
});

test("fragment budget counts actual UTF-8 JSON and leaves legacy catalog budget intact", () => {
  const wide = structuredClone(input);
  wide.catalog.fragments = Array.from({ length: 64 }, (_, index) => ({ ...wide.catalog.fragments[0]!, id: `entry${index}`, source: "α".repeat(3900) }));
  assert.equal(validateWorkerRequest(message(wide)).ok, true);
  const legacy = { ...input, catalog: structuredClone(input.catalog.commands) };
  legacy.catalog.definitions = [{ id: "huge", revision, userSha256: "a".repeat(64), projectSha256: null, command: "x".repeat(65536), arguments: [] }];
  assert.equal(validateWorkerRequest(message(legacy, "complete-formula-scoped")).ok, false);
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
}
const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));
const code = (expected: string) => (error: unknown): boolean => error instanceof WorkerRuntimeError && error.detail.code === expected;

test("fragment queue owns nested policy and content, drops late responses and recovers", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxQueueLength: 1, maxCompletionResultBytes: 100,
    workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  try {
    await client.ready();
    const abort = new AbortController();
    const pending = client.completeFormulaFragments(input, { signal: abort.signal });
    const rejected = assert.rejects(pending, code("CANCELLED"));
    await tick();
    const old = workers[0]!.requests.at(-1)!;
    const owned = structuredClone(apply);
    const next = client.applyFormulaCompletionFragments(owned);
    owned.catalog.policy.retrievalEnabled = false;
    owned.catalog.fragments[0]!.source = "mutated";
    owned.snapshot.catalogSha256 = "0".repeat(64);
    await assert.rejects(client.completeFormulaFragments(input), code("WORKER_QUEUE_FULL"));
    abort.abort(); await rejected; await client.ready(); await tick();
    assert.equal(workers[0]!.terminated, true);
    const wire = workers[1]!.requests.at(-1)!;
    assert.equal(wire.type, "apply-formula-completion-fragments");
    if (wire.type === "apply-formula-completion-fragments") assert.deepEqual(wire.input, apply);
    workers[0]!.emit(old.requestId, { ok: true });
    workers[1]!.emit(wire.requestId, failure);
    await assert.rejects(next, code("WORKER_RESULT_LIMIT"));
    const healthy = client.completeFormulaFragments(input); await tick();
    workers[1]!.emit(workers[1]!.requests.at(-1)!.requestId, { ok: true });
    assert.deepEqual(await healthy, { ok: true });
  } finally { client.terminate(); }
});

test("fragment acceptance timeout is never replayed and queued abort preserves active work", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js",
    workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  try {
    await client.ready();
    await assert.rejects(client.applyFormulaCompletionFragments(apply, { timeoutMillis: 10 }), code("WORKER_TASK_TIMEOUT"));
    await client.ready();
    assert.equal(workers[0]!.terminated, true);
    assert.equal(workers[1]!.requests.some((value) => value.type === "apply-formula-completion-fragments"), false);
    const active = client.completeFormulaFragments(input); await tick();
    const abort = new AbortController();
    const queued = client.applyFormulaCompletionFragments(apply, { signal: abort.signal });
    const rejected = assert.rejects(queued, code("CANCELLED")); abort.abort(); await rejected;
    assert.equal(workers[1]!.terminated, false);
    workers[1]!.emit(workers[1]!.requests.at(-1)!.requestId, { ok: true }); await active;
  } finally { client.terminate(); }
});

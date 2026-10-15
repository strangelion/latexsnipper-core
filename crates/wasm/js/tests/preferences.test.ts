import * as assert from "node:assert/strict";
import test from "node:test";
import { FormulaPreferences, type NativeFormulaPreferences, type FormulaPreferenceConfiguration, type FormulaPreferenceRestoreConfiguration } from "../src/preferences.js";
import { WorkerFormulaPreferences } from "../src/worker-preferences.js";
import { WasmWorkerClient, WorkerRuntimeError } from "../src/client.js";
import { validateWorkerRequest } from "../src/worker-request-validation.js";
import type { WorkerLike, WorkerRequest, WorkerResponse } from "../src/types.js";

// Fixed fixture identity, never a production feedback key.
const configuration: FormulaPreferenceConfiguration = { schemaVersion: 1, keyHex: "ab".repeat(32),
  scope: { userSha256: "a".repeat(64), projectSha256: null, documentSha256: null, generation: "0" } };
const restoration: FormulaPreferenceRestoreConfiguration = { ...configuration, storageKeyHex: "cd".repeat(32),
  expectedCheckpoint: "18446744073709551615", snapshotJson: "{}", permission: { storageEnabled: true, allowSensitiveStorage: false } };
const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));
class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null = null;
  requests: WorkerRequest[] = [];
  terminated = false;
  autoRestore = true;
  onRestore: (() => void) | undefined;
  postMessage(value: WorkerRequest): void {
    this.requests.push(value);
    if (value.type === "formula-preferences-restore") this.onRestore?.();
    if (value.type === "initialize" || value.type === "formula-preferences-create" || (value.type === "formula-preferences-restore" && this.autoRestore)) queueMicrotask(() =>
      this.emit(value.requestId, value.type === "initialize" ? true : { ownerId: "preferences:1" }));
  }
  emit(requestId: string, data: unknown): void {
    this.onmessage?.({ data: { protocolVersion: 1, type: "result", requestId, data } } as MessageEvent<WorkerResponse>);
  }
  terminate(): void { this.terminated = true; }
}

test("preference transport is closed, finite and precision preserving", () => {
  const create = { protocolVersion: 1, requestId: "p", type: "formula-preferences-create", configuration };
  assert.equal(validateWorkerRequest(create).ok, true);
  for (const bad of [{ ...configuration, ignored: true }, { ...configuration, keyHex: "private" },
    { ...configuration, scope: { ...configuration.scope, generation: 1 } },
    { ...configuration, scope: { ...configuration.scope, generation: "18446744073709551616" } }]) {
    assert.equal(validateWorkerRequest({ ...create, configuration: bad }).ok, false);
  }
  const base = { protocolVersion: 1, requestId: "p", type: "formula-preferences-command", ownerId: "preferences:1" };
  for (const command of [{ kind: "info", ignored: true }, { kind: "capture", ignored: true },
    { kind: "set-policy", policy: { rankingEnabled: true, feedbackEnabled: true } },
    { kind: "clear", generation: "01", keyHex: configuration.keyHex },
    { kind: "discard-capture", captureId: "x".repeat(97) }]) {
    assert.equal(validateWorkerRequest({ ...base, command }).ok, false);
  }
  assert.equal(validateWorkerRequest({ ...base, command: { kind: "clear", generation: "18446744073709551615", keyHex: configuration.keyHex } }).ok, true);
  assert.equal(validateWorkerRequest({ ...base, url: "https://evil.invalid", command: { kind: "info" } }).ok, false);
});

test("checkpoint wire has independent closed consent, exact u64 strings and a UTF-8 snapshot budget", () => {
  const base = { protocolVersion: 1, requestId: "restore", type: "formula-preferences-restore", configuration: restoration };
  assert.equal(validateWorkerRequest(base).ok, true);
  for (const bad of [{ ...restoration, ignored: true }, { ...restoration, expectedCheckpoint: 1 },
    { ...restoration, expectedCheckpoint: "01" }, { ...restoration, expectedCheckpoint: "18446744073709551616" },
    { ...restoration, permission: { storageEnabled: true } },
    { ...restoration, permission: { ...restoration.permission, storageEnabled: 1 } },
    { ...restoration, permission: { ...restoration.permission, PRIVATE_SECRET: true } },
    { ...restoration, snapshotJson: "中".repeat(90000) }, { ...restoration, snapshotJson: " ".repeat(256 * 1024 + 1) }]) {
    const result = validateWorkerRequest({ ...base, configuration: bad });
    assert.equal(result.ok, false);
    assert.equal(JSON.stringify(result).includes("PRIVATE_SECRET"), false);
  }
  assert.equal(validateWorkerRequest({ ...base, url: "https://evil.invalid" }).ok, false);
  const command = { kind: "export-checkpoint", storageKeyHex: restoration.storageKeyHex,
    checkpoint: restoration.expectedCheckpoint, permission: restoration.permission };
  const wire = { protocolVersion: 1, requestId: "export", type: "formula-preferences-command", ownerId: "preferences:1", command };
  assert.equal(validateWorkerRequest(wire).ok, true);
  assert.equal(validateWorkerRequest({ ...wire, command: { ...command, checkpoint: 1 } }).ok, false);
  assert.equal(validateWorkerRequest({ ...wire, command: { ...command, permission: { storageEnabled: true } } }).ok, false);
});

test("direct restore selects only its explicit factory and frees once after getter-driven export closure", () => {
  let restored = 0, commanded = 0, freed = 0;
  const native = { command: () => { commanded++; return { ok: true, data: true }; },
    close: () => ({ ok: true, data: true }), free: () => { freed++; } } as unknown as NativeFormulaPreferences;
  const owner = FormulaPreferences.restore({ restore_formula_preferences_v1(json) {
    restored++; assert.deepEqual(JSON.parse(json), restoration); return native;
  } }, restoration);
  assert.equal(restored, 1);
  const permission = { get storageEnabled() { owner.close(); return true; }, allowSensitiveStorage: false };
  assert.throws(() => owner.exportCheckpoint(restoration.storageKeyHex, "1", permission), /closed/);
  assert.equal(commanded, 0); assert.equal(freed, 1); owner.close(); assert.equal(freed, 1);
  assert.throws(() => FormulaPreferences.restore({} as never, restoration), /factory/);
  assert.throws(() => FormulaPreferences.restore({ restore_formula_preferences_v1: () => native },
    { ...restoration, permission: { storageEnabled: true } } as never), /restore configuration/);
});

test("queued restore owns its input and uses the independent 4 KiB owner result budget", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxCompletionResultBytes: 16, workerFactory: () => worker });
  try {
    const owner = await client.createFormulaPreferences(configuration);
    const active = owner.captureContext(); await tick();
    const request = worker.requests.find((value) => value.type === "formula-preferences-command"); assert.ok(request);
    const input = structuredClone(restoration);
    const restored = client.restoreFormulaPreferences(input);
    input.permission.storageEnabled = false; input.expectedCheckpoint = "1"; input.scope.generation = "2";
    worker.emit(request.requestId, true); await active;
    await restored;
    const sent = worker.requests.find((value) => value.type === "formula-preferences-restore");
    assert.ok(sent?.type === "formula-preferences-restore"); assert.deepEqual(sent.configuration, restoration);
    assert.equal(worker.terminated, false);
  } finally { client.terminate(); }
});

for (const mode of ["abort", "timeout", "crash"] as const) {
  test(`active checkpoint restore ${mode} drops queued restores/feedback without replay`, { timeout: 5000 }, async () => {
    const workers: FakeWorker[] = [];
    let entered!: () => void;
    const started = new Promise<void>((resolve) => { entered = resolve; });
    const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => {
      const worker = new FakeWorker(); worker.autoRestore = false; worker.onRestore = entered; workers.push(worker); return worker;
    } });
    try {
      const controller = new AbortController();
      const pending = client.restoreFormulaPreferences(restoration, mode === "timeout" ? { timeoutMillis: 15 } : { signal: controller.signal });
      const failed = assert.rejects(pending, (cause: unknown) => cause instanceof WorkerRuntimeError);
      await started;
      const queued = client.restoreFormulaPreferences(restoration);
      const refused = assert.rejects(queued, (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "WORKER_PREFERENCES_STALE");
      if (mode === "abort") controller.abort();
      if (mode === "crash") workers[0]!.onerror?.({ message: "fixture crash" } as ErrorEvent);
      await Promise.all([failed, refused]); await client.ready();
      assert.equal(workers[0]!.terminated, true);
      assert.equal(workers[1]!.requests.some((value) => value.type.startsWith("formula-preferences-")), false);
      workers[1]!.autoRestore = true;
      await client.restoreFormulaPreferences(restoration);
    } finally { client.terminate(); }
  });
}

test("direct preference wrapper validates, frees once and guards getter-driven closure", () => {
  let calls = 0, freed = 0;
  const native = { command: () => { calls += 1; return { ok: true, data: true }; }, close: () => ({ ok: true, data: true }),
    free: () => { freed += 1; } } as unknown as NativeFormulaPreferences;
  const owner = new FormulaPreferences({ create_formula_preferences_v1: () => native }, configuration);
  const command = { kind: "set-policy" as const, get policy() { owner.close(); return { rankingEnabled: true, feedbackEnabled: true, allowSensitiveFeedback: false }; } };
  assert.throws(() => owner.command(command), /closed/);
  assert.equal(calls, 0);
  assert.equal(freed, 1);
  owner.close(); assert.equal(freed, 1);
  assert.throws(() => owner.info(), /closed/);
  assert.throws(() => new FormulaPreferences({ create_formula_preferences_v1: () => native }, { ...configuration, keyHex: "invalid" }), /configuration/);
});

test("preference close retries after queue refusal or Core refusal and coalesces callers", async () => {
  let calls = 0;
  const owner = new WorkerFormulaPreferences(() => {
    calls += 1;
    return calls === 1 ? Promise.reject(new Error("queue full")) : Promise.resolve({ ok: calls !== 2, data: true });
  });
  await assert.rejects(owner.close(), /queue full/);
  assert.equal(owner.closed, false);
  await assert.rejects(owner.close(), /refused/);
  assert.equal(owner.closed, false);
  const closing = owner.close(); assert.equal(owner.close(), closing);
  await assert.rejects(owner.info(), /closed/);
  await closing; await owner.close();
  assert.equal(calls, 3);
});

for (const mode of ["abort", "timeout", "crash"] as const) {
  test(`preference ${mode} invalidates queued feedback and owners without replay`, async () => {
    const workers: FakeWorker[] = [];
    const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => {
      const worker = new FakeWorker(); workers.push(worker); return worker;
    } });
    try {
      const owner = await client.createFormulaPreferences(configuration);
      const controller = new AbortController();
      const active = owner.captureContext(mode === "timeout" ? { timeoutMillis: 15 } : { signal: controller.signal });
      const activeRejected = assert.rejects(active, (cause: unknown) => cause instanceof WorkerRuntimeError);
      await tick();
      const queued = owner.command({ kind: "undo", receiptId: "old-receipt" });
      const queuedRejected = assert.rejects(queued, (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "WORKER_PREFERENCES_STALE");
      if (mode === "abort") controller.abort();
      if (mode === "crash") workers[0]!.onerror?.({ message: "fixture crash" } as ErrorEvent);
      await Promise.all([activeRejected, queuedRejected]); await client.ready();
      assert.equal(workers[0]!.terminated, true);
      assert.equal(workers[1]!.requests.some((r) => r.type.startsWith("formula-preferences-")), false);
      await assert.rejects(owner.info(), (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "WORKER_PREFERENCES_STALE");
      const fresh = await client.createFormulaPreferences(configuration);
      const pending = fresh.info(); await tick();
      const request = workers[1]!.requests.find((r) => r.type === "formula-preferences-command"); assert.ok(request);
      workers[1]!.emit(request.requestId, { ok: true, data: { eventCount: 0 } });
      assert.equal((await pending).ok, true);
    } finally { client.terminate(); }
  });
}

test("queued preference input is cloned and queued cancellation does not reset its owner", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  try {
    const owner = await client.createFormulaPreferences(configuration);
    const policy = { rankingEnabled: true, feedbackEnabled: true, allowSensitiveFeedback: false };
    const active = owner.setPolicy(policy); policy.feedbackEnabled = false; await tick();
    const request = worker.requests.find((r) => r.type === "formula-preferences-command");
    assert.ok(request?.type === "formula-preferences-command" && request.command.kind === "set-policy");
    assert.equal(request.command.policy.feedbackEnabled, true);
    const controller = new AbortController();
    const queued = owner.captureContext({ signal: controller.signal });
    const rejected = assert.rejects(queued, (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "CANCELLED");
    controller.abort(); await rejected;
    worker.emit(request.requestId, { ok: true, data: { policy } }); await active;
    assert.equal(worker.terminated, false);
    assert.equal(worker.requests.filter((r) => r.type === "formula-preferences-command").length, 1);
  } finally { client.terminate(); }
});

test("lost preference result drops unknown owned state rather than replaying feedback", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxCompletionResultBytes: 16, workerFactory: () => {
    const worker = new FakeWorker(); workers.push(worker); return worker;
  } });
  try {
    const owner = await client.createFormulaPreferences(configuration);
    const pending = owner.captureContext();
    const rejected = assert.rejects(pending, (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "WORKER_RESULT_LIMIT");
    await tick();
    const request = workers[0]!.requests.find((r) => r.type === "formula-preferences-command"); assert.ok(request);
    workers[0]!.emit(request.requestId, { ok: true, data: { captureId: "x".repeat(96) } });
    await rejected; await client.ready();
    assert.equal(workers[0]!.terminated, true);
    assert.equal(workers[1]!.requests.some((r) => r.type.startsWith("formula-preferences-")), false);
    await assert.rejects(owner.info(), (cause: unknown) => cause instanceof WorkerRuntimeError && cause.detail.code === "WORKER_PREFERENCES_STALE");
  } finally { client.terminate(); }
});

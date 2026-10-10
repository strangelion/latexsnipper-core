import * as assert from "node:assert/strict";
import test from "node:test";

import {
  WasmWorkerClient,
  WorkerRuntimeError,
  type WorkerLike,
  type WorkerRequest,
  type WorkerResponse,
} from "../src/index.js";

class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null = null;
  readonly requests: WorkerRequest[] = [];
  terminated = false;
  failRecognitionPost = false;
  initialized = false;
  formulaBeforeReady = false;

  constructor(private readonly respondToControl = true) {}

  postMessage(request: WorkerRequest): void {
    if (request.type === "recognize" && this.failRecognitionPost) {
      throw new Error("fixture recognition post failed");
    }
    this.requests.push(request);
    if (request.type === "convert-formula" && !this.initialized) this.formulaBeforeReady = true;
    if (this.respondToControl && (request.type === "initialize" || request.type === "load-model")) {
      queueMicrotask(() => {
        if (request.type === "initialize") this.initialized = true;
        this.emit({
        protocolVersion: 1,
        type: "result",
        requestId: request.requestId,
        data: { ok: true },
        });
      });
    }
  }

  terminate(): void {
    this.terminated = true;
  }

  emit(response: WorkerResponse): void {
    this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
  }

  crash(message = "fixture worker crashed"): void {
    this.onerror?.({ message } as ErrorEvent);
  }

  messageError(): void {
    this.onmessageerror?.({ data: undefined } as MessageEvent<unknown>);
  }

  recognition(index = 0): Extract<WorkerRequest, { type: "recognize" }> {
    const requests = this.requests.filter((request): request is Extract<WorkerRequest, { type: "recognize" }> => request.type === "recognize");
    const request = requests[index];
    if (!request) throw new Error(`Missing recognition request at index ${index}`);
    return request;
  }

  formula(index = 0): Extract<WorkerRequest, { type: "convert-formula" }> {
    const request = this.requests.filter((request): request is Extract<WorkerRequest, { type: "convert-formula" }> => request.type === "convert-formula")[index];
    if (!request) throw new Error(`Missing formula request at index ${index}`);
    return request;
  }
}

async function tick(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

test("model-free formulas share ordering and queue limits with recognition", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker, maxQueueLength: 1 });
  await client.ready();
  const formula = client.convertFormula({ requestId: "formula", content: "frac(a,b)", inputFormat: "typst", outputFormat: "latex-fragment", mode: "best-effort" });
  await tick();
  assert.equal(worker.requests.some((request) => request.type === "load-model"), false);
  const recognition = client.recognize({ requestId: "image", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  await assert.rejects(client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" }), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_QUEUE_FULL");
  const wire = worker.formula();
  assert.equal(wire.input.mode, "best-effort");
  assert.equal(worker.requests.some((request) => request.type === "recognize"), false);
  worker.emit({ protocolVersion: 1, type: "result", requestId: wire.requestId, data: { ok: true, data: { content: "\\frac{a}{b}", contentKind: "latex-fragment" } } });
  assert.deepEqual(await formula, { ok: true, data: { content: "\\frac{a}{b}", contentKind: "latex-fragment" } });
  await tick();
  worker.emit({ protocolVersion: 1, type: "result", requestId: worker.recognition().requestId, data: "image result" });
  assert.equal(await recognition, "image result");
  client.terminate();
});

test("formula AbortSignal hard-cancels active work and suppresses old results", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  await client.ready();
  const signal = new AbortController();
  const first = client.convertFormula({ requestId: "cancel", content: "x", inputFormat: "latex", outputFormat: "omml" }, { signal: signal.signal });
  const rejected = assert.rejects(first, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED" && error.detail.hardCancellation === true);
  const next = client.convertFormula({ requestId: "next", content: "y", inputFormat: "latex", outputFormat: "omml" });
  await tick();
  const old = workers[0]!.formula();
  signal.abort();
  await rejected;
  await tick();
  assert.equal(workers[0]!.terminated, true);
  assert.equal(workers[1]!.requests.some((request) => request.type === "load-model"), false);
  workers[0]!.emit({ protocolVersion: 1, type: "result", requestId: old.requestId, data: "stale" });
  workers[1]!.emit({ protocolVersion: 1, type: "result", requestId: workers[1]!.formula().requestId, data: "current" });
  assert.equal(await next, "current");
  client.terminate();
});

test("cancellation in the enqueue microtask window waits for replacement initialization", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; } });
  await client.ready();
  const signal = new AbortController();
  const first = client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" }, { signal: signal.signal });
  const rejected = assert.rejects(first, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED");
  await tick();
  const next = client.convertFormula({ content: "y", inputFormat: "latex", outputFormat: "omml" });
  signal.abort();
  await rejected;
  await tick();
  workers[1]!.emit({ protocolVersion: 1, type: "result", requestId: workers[1]!.formula().requestId, data: "current" });
  assert.equal(await next, "current");
  client.terminate();
  assert.equal(workers[1]!.formulaBeforeReady, false);
});

test("formula timeout terminates a stuck worker and subsequent formula work recovers", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => { const worker = new FakeWorker(); workers.push(worker); return worker; }, maxFormulaDurationMillis: 10 });
  await client.ready();
  await assert.rejects(client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" }), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_TASK_TIMEOUT" && error.detail.workerRestarted === true);
  await client.ready();
  assert.equal(workers[0]!.terminated, true);
  const next = client.convertFormula({ content: "y", inputFormat: "latex", outputFormat: "omml" }, { timeoutMillis: 1000 });
  await tick();
  workers[1]!.emit({ protocolVersion: 1, type: "result", requestId: workers[1]!.formula().requestId, data: "recovered" });
  assert.equal(await next, "recovered");
  client.terminate();
});

test("formula input and result limits preserve queue usability", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker, maxFormulaInputBytes: 3, maxFormulaResultBytes: 8 });
  await client.ready();
  await assert.rejects(client.convertFormula({ content: "中文", inputFormat: "latex", outputFormat: "omml" }), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_FORMULA_INPUT_LIMIT");
  assert.equal(worker.requests.some((request) => request.type === "convert-formula"), false);
  const first = client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" });
  const rejected = assert.rejects(first, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_RESULT_LIMIT");
  await tick();
  worker.emit({ protocolVersion: 1, type: "result", requestId: worker.formula().requestId, data: "too much output" });
  await rejected;
  const next = client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" });
  await tick();
  worker.emit({ protocolVersion: 1, type: "result", requestId: worker.formula(1).requestId, data: "ok" });
  assert.equal(await next, "ok");
  client.terminate();
});

test("formula queued and pre-aborted signals do not restart the active worker", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  await client.ready();
  const stopped = new AbortController();
  stopped.abort();
  await assert.rejects(client.convertFormula({ content: "x", inputFormat: "latex", outputFormat: "omml" }, { signal: stopped.signal }), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED");
  const active = client.convertFormula({ requestId: "active", content: "x", inputFormat: "latex", outputFormat: "omml" });
  await tick();
  await assert.rejects(client.convertFormula({ requestId: "active", content: "x", inputFormat: "latex", outputFormat: "omml" }), (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "DUPLICATE_REQUEST_ID");
  const signal = new AbortController();
  const queued = client.convertFormula({ content: "y", inputFormat: "latex", outputFormat: "omml" }, { signal: signal.signal });
  const rejected = assert.rejects(queued, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED" && error.detail.hardCancellation === false);
  signal.abort();
  await rejected;
  assert.equal(worker.terminated, false);
  worker.emit({ protocolVersion: 1, type: "result", requestId: worker.formula().requestId, data: "ok" });
  assert.equal(await active, "ok");
  assert.equal(worker.requests.filter((request) => request.type === "convert-formula").length, 1);
  client.terminate();
});

test("hard cancellation terminates, restarts, reloads models, and suppresses stale output", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
  });
  await client.ready();
  await client.loadModel({ name: "text/model.onnx", bytes: new Uint8Array([1, 2]), expectedSha256: "00" });

  const progress: string[] = [];
  const first = client.recognize(
    { requestId: "first", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" },
    (event) => progress.push(event.stage),
  );
  const firstRejected = assert.rejects(first, (error: unknown) => {
    if (!(error instanceof WorkerRuntimeError)) return false;
    assert.equal(error.detail.code, "CANCELLED");
    assert.equal(error.detail.hardCancellation, true);
    return true;
  });
  const second = client.recognize({ requestId: "second", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  await tick();
  const firstWireId = workers[0]?.recognition().requestId;
  assert.match(firstWireId ?? "", /^internal:recognize:/);
  assert.equal(client.cancel("first"), true);
  assert.equal(client.cancel("first"), false);
  await firstRejected;
  assert.equal(workers[0]?.terminated, true);

  workers[0]?.emit({ protocolVersion: 1, type: "progress", requestId: firstWireId!, stage: "stale", progress: 1 });
  workers[0]?.emit({ protocolVersion: 1, type: "result", requestId: firstWireId!, data: "stale" });
  await tick();
  assert.deepEqual(progress, []);
  assert.equal(workers.length, 2);
  assert.equal(workers[1]?.requests.some((request) => request.type === "load-model"), true);
  const restarted = workers[1]?.recognition();
  assert.match(restarted?.requestId ?? "", /^internal:recognize:/);
  workers[1]?.emit({ protocolVersion: 1, type: "result", requestId: restarted!.requestId, data: "ok" });
  assert.equal(await second, "ok");
  client.terminate();
});

test("worker crashes restart automatically, reject active work, and continue queued work", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
  });
  await client.ready();
  const active = client.recognize({ requestId: "active", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  const queued = client.recognize({ requestId: "queued", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  const activeRejected = assert.rejects(
    active,
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_CRASHED" && error.detail.workerRestarted === true,
  );
  await tick();
  workers[0]?.crash();
  await activeRejected;
  await tick();
  assert.equal(workers[0]?.terminated, true);
  assert.equal(workers.length, 2);
  const resumed = workers[1]?.recognition();
  workers[1]?.emit({ protocolVersion: 1, type: "result", requestId: resumed!.requestId, data: "recovered" });
  assert.equal(await queued, "recovered");
  client.terminate();
});

test("worker message deserialization failures follow the same recovery path", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
  });
  await client.ready();
  const active = client.recognize({ requestId: "message-error", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  const rejected = assert.rejects(
    active,
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_CRASHED",
  );
  await tick();
  workers[0]?.messageError();
  await rejected;
  await client.ready();
  assert.equal(workers[0]?.terminated, true);
  assert.equal(workers.length, 2);
  client.terminate();
});

test("recognition postMessage failures reject active work and restart", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => {
      const worker = new FakeWorker();
      worker.failRecognitionPost = workers.length === 0;
      workers.push(worker);
      return worker;
    },
  });
  await client.ready();
  await assert.rejects(
    client.recognize({ requestId: "post-failure", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" }),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_POST_MESSAGE_FAILED",
  );
  await client.ready();
  assert.equal(workers[0]?.terminated, true);
  assert.equal(workers.length, 2);
  client.terminate();
});

test("user request IDs cannot collide with internal RPC wire IDs", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  await client.ready();
  const internalId = worker.requests[0]!.requestId;
  assert.match(internalId, /^internal:init:/);
  const recognition = client.recognize({ requestId: internalId, width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  await tick();
  const wire = worker.recognition();
  assert.notEqual(wire.requestId, internalId);
  worker.emit({ protocolVersion: 1, type: "result", requestId: wire.requestId, data: "isolated" });
  assert.equal(await recognition, "isolated");
  client.terminate();
});

test("RPC timeout terminates a hung worker and makes a recovered worker ready", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    rpcTimeoutMillis: 20,
    workerFactory: () => {
      const worker = new FakeWorker(workers.length > 0);
      workers.push(worker);
      return worker;
    },
  });
  await assert.rejects(
    client.ready(),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_RPC_TIMEOUT",
  );
  await client.ready();
  assert.equal(workers[0]?.terminated, true);
  assert.equal(workers.length, 2);
  client.terminate();
});

test("load-model RPC honors an already-aborted signal", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  await client.ready();
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    client.loadModel(
      { name: "aborted.onnx", bytes: new Uint8Array([1]), expectedSha256: "00" },
      { signal: controller.signal },
    ),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_RPC_ABORTED",
  );
  assert.equal(worker.requests.filter((request) => request.type === "load-model").length, 0);
  client.terminate();
});

test("requests are ordered, queued cancellation works, and invalid IDs are rejected", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", workerFactory: () => worker });
  await client.ready();
  const one = client.recognize({ requestId: "one", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  const two = client.recognize({ requestId: "two", width: 1, height: 1, pixels: new Uint8Array(4), mode: "text" });
  const twoRejected = assert.rejects(two, (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "CANCELLED");
  await tick();
  assert.equal(worker.requests.filter((request) => request.type === "recognize").length, 1);
  assert.equal(client.cancel("unknown"), false);
  assert.equal(client.cancel("two"), true);
  await twoRejected;
  const active = worker.recognition();
  worker.emit({ protocolVersion: 1, type: "result", requestId: active.requestId, data: 1 });
  assert.equal(await one, 1);
  client.terminate();
});

test("browser budgets reject oversized models and images before worker allocation", async () => {
  const worker = new FakeWorker();
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => worker,
    maxModelBytes: 2,
    maxTotalModelBytes: 3,
    maxImageWidth: 2,
    maxImageHeight: 2,
    maxImagePixels: 4,
  });
  await client.ready();
  await assert.rejects(
    client.loadModel({ name: "large.onnx", bytes: new Uint8Array(3), expectedSha256: "00" }),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "MODEL_ARTIFACT_LIMIT",
  );
  await client.loadModel({ name: "first.onnx", bytes: new Uint8Array(2), expectedSha256: "00" });
  await assert.rejects(
    client.loadModel({ name: "second.onnx", bytes: new Uint8Array(2), expectedSha256: "00" }),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "MODEL_TOTAL_LIMIT",
  );
  await assert.rejects(
    client.recognize({ width: 3, height: 1, pixels: new Uint8Array(12), mode: "table" }),
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_IMAGE_LIMIT",
  );
  assert.equal(worker.requests.filter((request) => request.type === "recognize").length, 0);
  client.terminate();
});

test("task duration and result budgets reject work and preserve recovery", async () => {
  const workers: FakeWorker[] = [];
  const client = new WasmWorkerClient({
    workerUrl: "worker.js",
    moduleUrl: "wasm.js",
    workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
    maxTaskDurationMillis: 20,
    maxResultBytes: 4,
  });
  await client.ready();
  await assert.rejects(
    client.recognize({ width: 1, height: 1, pixels: new Uint8Array(4), mode: "handwriting" }),
    (error: unknown) => error instanceof WorkerRuntimeError
      && error.detail.code === "WORKER_TASK_TIMEOUT"
      && error.detail.workerRestarted === true,
  );
  await client.ready();
  assert.equal(workers[0]?.terminated, true);

  const result = client.recognize({ width: 1, height: 1, pixels: new Uint8Array(4), mode: "table" });
  await tick();
  const request = workers[1]!.recognition();
  workers[1]!.emit({ protocolVersion: 1, type: "result", requestId: request.requestId, data: "oversized" });
  await assert.rejects(
    result,
    (error: unknown) => error instanceof WorkerRuntimeError && error.detail.code === "WORKER_RESULT_LIMIT",
  );
  client.terminate();
});

test("invalid budget configuration is rejected synchronously", () => {
  assert.throws(
    () => new WasmWorkerClient({ workerUrl: "worker.js", moduleUrl: "wasm.js", maxQueueLength: 0 }),
    RangeError,
  );
  assert.throws(
    () => new WasmWorkerClient({
      workerUrl: "worker.js",
      moduleUrl: "wasm.js",
      maxModelBytes: 2,
      maxTotalModelBytes: 1,
    }),
    RangeError,
  );
});

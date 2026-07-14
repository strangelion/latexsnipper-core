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
  readonly requests: WorkerRequest[] = [];
  terminated = false;

  postMessage(request: WorkerRequest): void {
    this.requests.push(request);
    if (request.type === "initialize" || request.type === "load-model") {
      queueMicrotask(() => this.emit({
        protocolVersion: 1,
        type: "result",
        requestId: request.requestId,
        data: { ok: true },
      }));
    }
  }

  terminate(): void {
    this.terminated = true;
  }

  emit(response: WorkerResponse): void {
    this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>);
  }

  recognition(index = 0): Extract<WorkerRequest, { type: "recognize" }> {
    const requests = this.requests.filter((request): request is Extract<WorkerRequest, { type: "recognize" }> => request.type === "recognize");
    const request = requests[index];
    if (!request) throw new Error(`Missing recognition request at index ${index}`);
    return request;
  }
}

async function tick(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

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
  assert.equal(workers[0]?.recognition().requestId, "first");
  assert.equal(client.cancel("first"), true);
  assert.equal(client.cancel("first"), false);
  await firstRejected;
  assert.equal(workers[0]?.terminated, true);

  workers[0]?.emit({ protocolVersion: 1, type: "progress", requestId: "first", stage: "stale", progress: 1 });
  workers[0]?.emit({ protocolVersion: 1, type: "result", requestId: "first", data: "stale" });
  await tick();
  assert.deepEqual(progress, []);
  assert.equal(workers.length, 2);
  assert.equal(workers[1]?.requests.some((request) => request.type === "load-model"), true);
  const restarted = workers[1]?.recognition();
  assert.equal(restarted?.requestId, "second");
  workers[1]?.emit({ protocolVersion: 1, type: "result", requestId: "second", data: "ok" });
  assert.equal(await second, "ok");
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

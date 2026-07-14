import {
  WORKER_PROTOCOL_VERSION,
  type ModelArtifact,
  type ProgressEvent,
  type RecognitionInput,
  type WorkerError,
  type WorkerFactory,
  type WorkerInitOptions,
  type WorkerLike,
  type WorkerRequest,
  type WorkerResponse,
} from "./types.js";

interface PendingCall {
  resolve(value: unknown): void;
  reject(error: WorkerRuntimeError): void;
}

interface RecognitionTask extends PendingCall {
  requestId: string;
  input: Omit<RecognitionInput, "requestId">;
  onProgress?: (event: ProgressEvent) => void;
  generation: number;
}

export class WorkerRuntimeError extends Error {
  constructor(public readonly detail: WorkerError) {
    super(detail.message);
    this.name = "WorkerRuntimeError";
  }
}

export interface WasmWorkerClientOptions extends WorkerInitOptions {
  workerUrl: string | URL;
  workerFactory?: WorkerFactory;
  maxQueueLength?: number;
}

export class WasmWorkerClient {
  private worker!: WorkerLike;
  private readonly calls = new Map<string, PendingCall>();
  private readonly models = new Map<string, ModelArtifact>();
  private readonly queue: RecognitionTask[] = [];
  private active?: RecognitionTask;
  private generation = 0;
  private sequence = 0;
  private initialized: Promise<void>;
  private terminated = false;
  private readonly maxQueueLength: number;

  constructor(private readonly options: WasmWorkerClientOptions) {
    this.maxQueueLength = options.maxQueueLength ?? 32;
    this.initialized = this.startWorker();
  }

  ready(): Promise<void> {
    return this.initialized;
  }

  async loadModel(artifact: ModelArtifact): Promise<unknown> {
    await this.initialized;
    const owned = { ...artifact, bytes: artifact.bytes.slice() };
    const result = await this.call({
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "load-model",
      requestId: this.nextId("model"),
      artifact: owned,
    });
    this.models.set(owned.name, owned);
    return result;
  }

  recognize(input: RecognitionInput, onProgress?: (event: ProgressEvent) => void): Promise<unknown> {
    if (this.terminated) return Promise.reject(this.runtimeError("WORKER_TERMINATED", "Worker client is terminated"));
    const requestId = input.requestId ?? this.nextId("recognize");
    if (this.hasRequest(requestId)) {
      return Promise.reject(this.runtimeError("DUPLICATE_REQUEST_ID", `Request '${requestId}' already exists`));
    }
    if (this.queue.length >= this.maxQueueLength) {
      return Promise.reject(this.runtimeError("WORKER_QUEUE_FULL", "Recognition queue is full"));
    }
    return new Promise((resolve, reject) => {
      this.queue.push({
        requestId,
        input: { width: input.width, height: input.height, pixels: input.pixels.slice(), mode: input.mode },
        onProgress,
        resolve,
        reject,
        generation: this.generation,
      });
      void this.pump();
    });
  }

  cancel(requestId: string): boolean {
    if (this.active?.requestId === requestId) {
      const cancelled = this.active;
      this.active = undefined;
      this.generation += 1;
      this.worker.terminate();
      this.rejectAllCalls("WORKER_RESTARTED", "Worker restarted after hard cancellation");
      cancelled.reject(this.runtimeError("CANCELLED", `Request '${requestId}' was cancelled`, {
        hardCancellation: true,
        workerRestarted: true,
      }));
      this.initialized = this.startWorker().then(() => this.reloadModels());
      void this.initialized.then(() => this.pump());
      return true;
    }
    const index = this.queue.findIndex((task) => task.requestId === requestId);
    if (index < 0) return false;
    const [cancelled] = this.queue.splice(index, 1);
    cancelled?.reject(this.runtimeError("CANCELLED", `Queued request '${requestId}' was cancelled`, {
      hardCancellation: false,
      workerRestarted: false,
    }));
    return true;
  }

  terminate(): void {
    if (this.terminated) return;
    this.terminated = true;
    this.worker.terminate();
    this.active?.reject(this.runtimeError("WORKER_TERMINATED", "Worker client terminated"));
    this.active = undefined;
    for (const task of this.queue.splice(0)) task.reject(this.runtimeError("WORKER_TERMINATED", "Worker client terminated"));
    this.rejectAllCalls("WORKER_TERMINATED", "Worker client terminated");
  }

  private async startWorker(): Promise<void> {
    if (this.terminated) throw this.runtimeError("WORKER_TERMINATED", "Worker client is terminated");
    const worker = this.options.workerFactory?.() ?? new Worker(this.options.workerUrl, { type: "module" });
    this.worker = worker;
    const workerGeneration = this.generation;
    worker.onmessage = (event) => this.onMessage(event.data, workerGeneration);
    worker.onerror = (event) => this.onWorkerError(event.message || "Worker failed", workerGeneration);
    await this.call({
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "initialize",
      requestId: this.nextId("init"),
      options: { moduleUrl: this.options.moduleUrl, wasmUrl: this.options.wasmUrl },
    });
  }

  private async reloadModels(): Promise<void> {
    for (const artifact of this.models.values()) {
      await this.call({
        protocolVersion: WORKER_PROTOCOL_VERSION,
        type: "load-model",
        requestId: this.nextId("reload"),
        artifact: { ...artifact, bytes: artifact.bytes.slice() },
      });
    }
  }

  private async pump(): Promise<void> {
    await this.initialized;
    if (this.active || this.terminated) return;
    const task = this.queue.shift();
    if (!task) return;
    task.generation = this.generation;
    this.active = task;
    this.worker.postMessage({
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "recognize",
      requestId: task.requestId,
      input: task.input,
    });
  }

  private onMessage(response: WorkerResponse, workerGeneration: number): void {
    if (workerGeneration !== this.generation || response.protocolVersion !== WORKER_PROTOCOL_VERSION) return;
    if (response.type === "progress") {
      if (this.active?.requestId === response.requestId) {
        this.active.onProgress?.({ requestId: response.requestId, stage: response.stage, progress: response.progress });
      }
      return;
    }
    const call = this.calls.get(response.requestId);
    if (call) {
      this.calls.delete(response.requestId);
      response.type === "result" ? call.resolve(response.data) : call.reject(new WorkerRuntimeError(response.error));
      return;
    }
    if (this.active?.requestId !== response.requestId) return;
    const task = this.active;
    this.active = undefined;
    response.type === "result" ? task.resolve(response.data) : task.reject(new WorkerRuntimeError(response.error));
    void this.pump();
  }

  private onWorkerError(message: string, workerGeneration: number): void {
    if (workerGeneration !== this.generation || this.terminated) return;
    this.active?.reject(this.runtimeError("WORKER_CRASHED", message));
    this.active = undefined;
    this.rejectAllCalls("WORKER_CRASHED", message);
  }

  private call(request: WorkerRequest): Promise<unknown> {
    return new Promise((resolve, reject) => {
      this.calls.set(request.requestId, { resolve, reject });
      this.worker.postMessage(request);
    });
  }

  private rejectAllCalls(code: string, message: string): void {
    for (const call of this.calls.values()) call.reject(this.runtimeError(code, message));
    this.calls.clear();
  }

  private hasRequest(requestId: string): boolean {
    return this.active?.requestId === requestId || this.queue.some((task) => task.requestId === requestId);
  }

  private nextId(prefix: string): string {
    this.sequence += 1;
    return `${prefix}-${this.sequence}`;
  }

  private runtimeError(code: string, message: string, extra: Partial<WorkerError> = {}): WorkerRuntimeError {
    return new WorkerRuntimeError({ code, message, recoverable: true, ...extra });
  }
}

export function warnIfMainThreadInference(): void {
  if (typeof window !== "undefined" && typeof document !== "undefined") {
    console.warn("LaTeXSnipper: heavy WASM inference on the main thread can block the UI; use WasmWorkerClient.");
  }
}

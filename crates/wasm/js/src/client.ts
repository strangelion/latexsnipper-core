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
  wireRequestId: string;
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
  rpcTimeoutMillis?: number;
}

export interface WorkerCallOptions {
  timeoutMillis?: number;
  signal?: AbortSignal;
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
  private readonly rpcTimeoutMillis: number;
  private restarting?: Promise<void>;

  constructor(private readonly options: WasmWorkerClientOptions) {
    this.maxQueueLength = options.maxQueueLength ?? 32;
    this.rpcTimeoutMillis = options.rpcTimeoutMillis ?? 30_000;
    if (!Number.isFinite(this.rpcTimeoutMillis) || this.rpcTimeoutMillis <= 0) {
      throw new RangeError("rpcTimeoutMillis must be a positive finite number");
    }
    this.initialized = this.startWorker();
  }

  ready(): Promise<void> {
    return this.initialized;
  }

  async loadModel(artifact: ModelArtifact, callOptions: WorkerCallOptions = {}): Promise<unknown> {
    await this.initialized;
    const owned = { ...artifact, bytes: artifact.bytes.slice() };
    const result = await this.call({
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "load-model",
      requestId: this.nextId("model"),
      artifact: owned,
    }, callOptions);
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
        wireRequestId: this.nextId("recognize"),
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
      this.restartWorker(
        "WORKER_RESTARTED",
        "Worker restarted after hard cancellation",
        this.runtimeError("CANCELLED", `Request '${requestId}' was cancelled`, {
          hardCancellation: true,
          workerRestarted: true,
        }),
      );
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
    worker.onmessageerror = () => this.onWorkerError("Worker message deserialization failed", workerGeneration);
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
    try {
      await this.initialized;
    } catch {
      return;
    }
    if (this.active || this.terminated) return;
    const task = this.queue.shift();
    if (!task) return;
    task.generation = this.generation;
    this.active = task;
    try {
      this.worker.postMessage({
        protocolVersion: WORKER_PROTOCOL_VERSION,
        type: "recognize",
        requestId: task.wireRequestId,
        input: task.input,
      });
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.restartWorker(
        "WORKER_POST_MESSAGE_FAILED",
        message,
        this.runtimeError("WORKER_POST_MESSAGE_FAILED", message, { workerRestarted: true }),
      );
    }
  }

  private onMessage(response: WorkerResponse, workerGeneration: number): void {
    if (workerGeneration !== this.generation) return;
    if (response.protocolVersion !== WORKER_PROTOCOL_VERSION) {
      this.restartWorker(
        "WORKER_PROTOCOL_MISMATCH",
        "Worker returned an unsupported protocol version",
      );
      return;
    }
    if (response.type === "progress") {
      if (this.active?.wireRequestId === response.requestId) {
        this.active.onProgress?.({ requestId: this.active.requestId, stage: response.stage, progress: response.progress });
      }
      return;
    }
    const call = this.calls.get(response.requestId);
    if (call) {
      this.calls.delete(response.requestId);
      response.type === "result" ? call.resolve(response.data) : call.reject(new WorkerRuntimeError(response.error));
      return;
    }
    if (this.active?.wireRequestId !== response.requestId) {
      this.restartWorker("WORKER_PROTOCOL_CORRUPTION", `Worker returned unknown response ID '${response.requestId}'`);
      return;
    }
    const task = this.active;
    this.active = undefined;
    response.type === "result" ? task.resolve(response.data) : task.reject(new WorkerRuntimeError(response.error));
    void this.pump();
  }

  private onWorkerError(message: string, workerGeneration: number): void {
    if (workerGeneration !== this.generation || this.terminated) return;
    this.restartWorker("WORKER_CRASHED", message, this.runtimeError("WORKER_CRASHED", message, {
      workerRestarted: true,
    }));
  }

  private call(request: WorkerRequest, options: WorkerCallOptions = {}): Promise<unknown> {
    if (options.signal?.aborted) {
      return Promise.reject(this.runtimeError("WORKER_RPC_ABORTED", `Worker RPC '${request.type}' was aborted`));
    }
    const timeoutMillis = options.timeoutMillis ?? this.rpcTimeoutMillis;
    if (!Number.isFinite(timeoutMillis) || timeoutMillis <= 0) {
      return Promise.reject(this.runtimeError("WORKER_RPC_INVALID_TIMEOUT", "Worker RPC timeout must be positive"));
    }
    return new Promise((resolve, reject) => {
      let settled = false;
      const cleanup = (): void => {
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
      };
      const pending: PendingCall = {
        resolve: (value) => {
          if (settled) return;
          settled = true;
          cleanup();
          resolve(value);
        },
        reject: (error) => {
          if (settled) return;
          settled = true;
          cleanup();
          reject(error);
        },
      };
      const failAndRestart = (code: string, message: string): void => {
        if (!this.calls.delete(request.requestId)) return;
        pending.reject(this.runtimeError(code, message, { workerRestarted: true }));
        this.restartWorker(code, message);
      };
      const timer = setTimeout(() => {
        failAndRestart("WORKER_RPC_TIMEOUT", `Worker RPC '${request.type}' exceeded ${timeoutMillis} ms`);
      }, timeoutMillis);
      const abort = (): void => {
        failAndRestart("WORKER_RPC_ABORTED", `Worker RPC '${request.type}' was aborted`);
      };
      options.signal?.addEventListener("abort", abort, { once: true });
      this.calls.set(request.requestId, pending);
      try {
        this.worker.postMessage(request);
      } catch (cause) {
        failAndRestart(
          "WORKER_POST_MESSAGE_FAILED",
          cause instanceof Error ? cause.message : String(cause),
        );
      }
    });
  }

  private restartWorker(code: string, message: string, activeError?: WorkerRuntimeError): void {
    if (this.terminated) return;
    const active = this.active;
    this.active = undefined;
    this.generation += 1;
    this.worker.terminate();
    this.rejectAllCalls(code, message);
    active?.reject(activeError ?? this.runtimeError(code, message, { workerRestarted: true }));

    if (this.restarting) {
      this.rejectQueue("WORKER_RECOVERY_FAILED", "Worker failed again during recovery");
      return;
    }
    const recovery = this.startWorker().then(() => this.reloadModels());
    this.restarting = recovery;
    this.initialized = recovery;
    void recovery
      .then(() => this.pump())
      .catch((cause: unknown) => {
        this.rejectQueue(
          "WORKER_RECOVERY_FAILED",
          cause instanceof Error ? cause.message : String(cause),
        );
      })
      .finally(() => {
        if (this.restarting === recovery) this.restarting = undefined;
      });
  }

  private rejectAllCalls(code: string, message: string): void {
    for (const call of this.calls.values()) call.reject(this.runtimeError(code, message));
    this.calls.clear();
  }

  private rejectQueue(code: string, message: string): void {
    for (const task of this.queue.splice(0)) task.reject(this.runtimeError(code, message));
  }

  private hasRequest(requestId: string): boolean {
    return this.active?.requestId === requestId || this.queue.some((task) => task.requestId === requestId);
  }

  private nextId(prefix: string): string {
    this.sequence += 1;
    return `internal:${prefix}:${this.sequence}`;
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

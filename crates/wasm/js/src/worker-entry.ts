import {
  WORKER_PROTOCOL_VERSION,
  type WorkerRequest,
  type WorkerResponse,
} from "./types.js";

interface WasmApi {
  default?: (wasmUrl?: string) => Promise<unknown>;
  init?: () => void;
  load_model_v2(name: string, bytes: Uint8Array, expectedSha256?: string): unknown;
  clear_models_v2(): unknown;
  cancel_recognition_v2(): unknown;
  recognize_v2_with_progress(
    width: number,
    height: number,
    pixels: Uint8Array,
    mode: string,
    progress: (event: { stage: string; progress: number }) => void,
  ): Promise<unknown>;
}

type WorkerScope = typeof globalThis & {
  postMessage(message: WorkerResponse): void;
  onmessage: ((event: MessageEvent<WorkerRequest>) => void) | null;
};

const scope = globalThis as WorkerScope;
let api: WasmApi | undefined;
let queue = Promise.resolve();

function respond(message: WorkerResponse): void {
  scope.postMessage(message);
}

function error(requestId: string, code: string, message: string, details?: unknown): void {
  respond({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "error",
    requestId,
    error: { code, message, recoverable: true, details },
  });
}

async function handle(request: WorkerRequest): Promise<void> {
  if (request.protocolVersion !== WORKER_PROTOCOL_VERSION) {
    error(request.requestId, "WORKER_PROTOCOL_MISMATCH", "Unsupported worker protocol version");
    return;
  }
  try {
    if (request.type === "initialize") {
      const loaded = (await import(request.options.moduleUrl)) as WasmApi;
      if (loaded.default) await loaded.default(request.options.wasmUrl);
      loaded.init?.();
      api = loaded;
      respond({ protocolVersion: WORKER_PROTOCOL_VERSION, type: "result", requestId: request.requestId, data: true });
      return;
    }
    if (!api) {
      error(request.requestId, "WORKER_NOT_INITIALIZED", "Initialize the worker before use");
      return;
    }
    if (request.type === "load-model") {
      const data = api.load_model_v2(
        request.artifact.name,
        request.artifact.bytes,
        request.artifact.expectedSha256,
      );
      respond({ protocolVersion: WORKER_PROTOCOL_VERSION, type: "result", requestId: request.requestId, data });
      return;
    }
    if (request.type === "clear-models") {
      const data = api.clear_models_v2();
      respond({ protocolVersion: WORKER_PROTOCOL_VERSION, type: "result", requestId: request.requestId, data });
      return;
    }
    if (request.type === "cooperative-cancel") {
      const data = api.cancel_recognition_v2();
      respond({ protocolVersion: WORKER_PROTOCOL_VERSION, type: "result", requestId: request.requestId, data });
      return;
    }
    const data = await api.recognize_v2_with_progress(
      request.input.width,
      request.input.height,
      request.input.pixels,
      request.input.mode,
      (event) => respond({
        protocolVersion: WORKER_PROTOCOL_VERSION,
        type: "progress",
        requestId: request.requestId,
        stage: event.stage,
        progress: event.progress,
      }),
    );
    respond({ protocolVersion: WORKER_PROTOCOL_VERSION, type: "result", requestId: request.requestId, data });
  } catch (cause) {
    error(
      request.requestId,
      "WORKER_OPERATION_FAILED",
      cause instanceof Error ? cause.message : String(cause),
    );
  }
}

function isTrustedMessageEvent(event: MessageEvent<WorkerRequest>): boolean {
  const eventOrigin = event.origin;
  if (!eventOrigin) {
    return true;
  }

  return eventOrigin === globalThis.location.origin;
}

scope.onmessage = (event) => {
  if (!isTrustedMessageEvent(event)) {
    return;
  }

  const request = event.data;
  queue = queue.then(() => handle(request)).catch((cause: unknown) => {
    error(request.requestId, "WORKER_QUEUE_FAILED", cause instanceof Error ? cause.message : String(cause));
  });
};

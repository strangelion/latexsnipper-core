export const WORKER_PROTOCOL_VERSION = 1 as const;

export interface WorkerInitOptions {
  moduleUrl: string;
  wasmUrl?: string;
}

export interface ModelArtifact {
  name: string;
  bytes: Uint8Array;
  expectedSha256: string;
}

export interface RecognitionInput {
  requestId?: string;
  width: number;
  height: number;
  pixels: Uint8Array;
  mode: string;
}

export interface ProgressEvent {
  requestId: string;
  stage: string;
  progress: number;
}

export type WorkerRequest =
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "initialize";
      requestId: string;
      options: WorkerInitOptions;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "load-model";
      requestId: string;
      artifact: ModelArtifact;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "recognize";
      requestId: string;
      input: Omit<RecognitionInput, "requestId">;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      /** Stage-boundary only; this message cannot preempt a running serialized handler. */
      type: "cooperative-cancel";
      requestId: string;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "clear-models";
      requestId: string;
    };

export type WorkerResponse =
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "result";
      requestId: string;
      data: unknown;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "progress";
      requestId: string;
      stage: string;
      progress: number;
    }
  | {
      protocolVersion: typeof WORKER_PROTOCOL_VERSION;
      type: "error";
      requestId: string;
      error: WorkerError;
    };

export interface WorkerError {
  code: string;
  message: string;
  recoverable: boolean;
  hardCancellation?: boolean;
  workerRestarted?: boolean;
  details?: unknown;
}

export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer?: Transferable[]): void;
  terminate(): void;
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null;
}

export type WorkerFactory = () => WorkerLike;

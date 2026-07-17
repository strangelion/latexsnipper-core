import {
  WORKER_PROTOCOL_VERSION,
  type WorkerRequest,
} from "./types.js";

export type WorkerRequestValidationResult =
  | { ok: true; request: WorkerRequest }
  | {
      ok: false;
      requestId?: string;
      code: "WORKER_INVALID_REQUEST" | "WORKER_PROTOCOL_MISMATCH";
      message: string;
    };

const MAX_REQUEST_ID_LENGTH = 256;
const MAX_NAME_LENGTH = 256;
const MAX_URL_LENGTH = 8_192;
const MAX_MODE_LENGTH = 128;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength;
}

function isOptionalString(value: unknown, maxLength: number): value is string | undefined {
  return value === undefined || isNonEmptyString(value, maxLength);
}

function requestIdFrom(value: unknown): string | undefined {
  if (!isRecord(value)) return undefined;
  const rid = value.requestId;
  return isNonEmptyString(rid, MAX_REQUEST_ID_LENGTH) ? rid : undefined;
}

function invalid(value: unknown, message: string): WorkerRequestValidationResult {
  return { ok: false, requestId: requestIdFrom(value), code: "WORKER_INVALID_REQUEST", message };
}

function validateInitialize(value: Record<string, unknown>): WorkerRequestValidationResult {
  const options = value.options;
  if (!isRecord(options)) {
    return invalid(value, "Initialize request requires an options object");
  }
  if (!isNonEmptyString(options.moduleUrl, MAX_URL_LENGTH)) {
    return invalid(value, "Initialize options.moduleUrl must be a non-empty string");
  }
  if (!isOptionalString(options.wasmUrl, MAX_URL_LENGTH)) {
    return invalid(value, "Initialize options.wasmUrl must be a string when provided");
  }
  return { ok: true, request: value as unknown as WorkerRequest };
}

function validateLoadModel(value: Record<string, unknown>): WorkerRequestValidationResult {
  const artifact = value.artifact;
  if (!isRecord(artifact)) {
    return invalid(value, "Load-model request requires an artifact object");
  }
  if (!isNonEmptyString(artifact.name, MAX_NAME_LENGTH)) {
    return invalid(value, "Model artifact name must be a non-empty string");
  }
  if (!(artifact.bytes instanceof Uint8Array)) {
    return invalid(value, "Model artifact bytes must be a Uint8Array");
  }
  if (typeof artifact.expectedSha256 !== "string" || !/^[0-9a-fA-F]{64}$/.test(artifact.expectedSha256)) {
    return invalid(value, "Model expectedSha256 must be a 64-character hexadecimal SHA-256 digest");
  }
  return { ok: true, request: value as unknown as WorkerRequest };
}

function validateRecognize(value: Record<string, unknown>): WorkerRequestValidationResult {
  const input = value.input;
  if (!isRecord(input)) {
    return invalid(value, "Recognize request requires an input object");
  }
  if (!Number.isSafeInteger(input.width) || (input.width as number) <= 0) {
    return invalid(value, "Recognition width must be a positive safe integer");
  }
  if (!Number.isSafeInteger(input.height) || (input.height as number) <= 0) {
    return invalid(value, "Recognition height must be a positive safe integer");
  }
  if (!(input.pixels instanceof Uint8Array)) {
    return invalid(value, "Recognition pixels must be a Uint8Array");
  }
  if (!isNonEmptyString(input.mode, MAX_MODE_LENGTH)) {
    return invalid(value, "Recognition mode must be a non-empty string");
  }
  const pixelCount = (input.width as number) * (input.height as number);
  if (!Number.isSafeInteger(pixelCount)) {
    return invalid(value, "Recognition dimensions overflow the safe integer range");
  }
  const expectedBytes = pixelCount * 4;
  if (!Number.isSafeInteger(expectedBytes) || input.pixels.byteLength !== expectedBytes) {
    return invalid(value, "RGBA pixel length does not match image dimensions");
  }
  return { ok: true, request: value as unknown as WorkerRequest };
}

export function validateWorkerRequest(value: unknown): WorkerRequestValidationResult {
  if (!isRecord(value)) {
    return invalid(value, "Worker message must be an object");
  }
  if (!isNonEmptyString(value.requestId, MAX_REQUEST_ID_LENGTH)) {
    return invalid(value, "Worker requestId must be a non-empty string");
  }
  if (value.protocolVersion !== WORKER_PROTOCOL_VERSION) {
    return {
      ok: false,
      requestId: value.requestId as string,
      code: "WORKER_PROTOCOL_MISMATCH",
      message: `Unsupported worker protocol version '${String(value.protocolVersion)}'`,
    };
  }
  switch (value.type) {
    case "initialize":
      return validateInitialize(value);
    case "load-model":
      return validateLoadModel(value);
    case "recognize":
      return validateRecognize(value);
    case "cooperative-cancel":
    case "clear-models":
      return { ok: true, request: value as unknown as WorkerRequest };
    default:
      return invalid(value, `Unsupported worker request type '${String(value.type)}'`);
  }
}

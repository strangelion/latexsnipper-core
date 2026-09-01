import * as assert from "node:assert/strict";
import test from "node:test";

import { WORKER_PROTOCOL_VERSION } from "../src/types.js";
import { validateWorkerRequest } from "../src/worker-request-validation.js";

test("accepts a valid initialize request", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "initialize",
    requestId: "init:1",
    options: { moduleUrl: "./pkg.js" },
  });
  assert.equal(result.ok, true);
});

test("accepts same-origin module and wasm asset URLs", () => {
  const result = validateWorkerRequest(
    {
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "initialize",
      requestId: "init:same-origin",
      options: {
        moduleUrl: "https://app.example/assets/pkg.js",
        wasmUrl: "/assets/pkg_bg.wasm",
      },
    },
    { assetBaseUrl: "https://app.example/workers/worker.js" },
  );
  assert.equal(result.ok, true);
});

test("accepts a same-origin blob module URL", () => {
  const result = validateWorkerRequest(
    {
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "initialize",
      requestId: "init:blob",
      options: { moduleUrl: "blob:https://app.example/2e953931-b93f-43b2-a31f-18a0138c7bb1" },
    },
    { assetBaseUrl: "https://app.example/workers/worker.js" },
  );
  assert.equal(result.ok, true);
});

for (const [name, moduleUrl] of [
  ["cross-origin", "https://cdn.example/pkg.js"],
  ["data", "data:text/javascript,export default {}"],
  ["javascript", "javascript:alert(1)"],
  ["file", "file:///tmp/pkg.js"],
  ["malformed", "http://[invalid"],
] as const) {
  test(`rejects ${name} module URL`, () => {
    const result = validateWorkerRequest(
      {
        protocolVersion: WORKER_PROTOCOL_VERSION,
        type: "initialize",
        requestId: `init:${name}`,
        options: { moduleUrl },
      },
      { assetBaseUrl: "https://app.example/workers/worker.js" },
    );
    assert.equal(result.ok, false);
  });
}

test("rejects a cross-origin wasm URL", () => {
  const result = validateWorkerRequest(
    {
      protocolVersion: WORKER_PROTOCOL_VERSION,
      type: "initialize",
      requestId: "init:foreign-wasm",
      options: {
        moduleUrl: "/assets/pkg.js",
        wasmUrl: "https://cdn.example/pkg_bg.wasm",
      },
    },
    { assetBaseUrl: "https://app.example/workers/worker.js" },
  );
  assert.equal(result.ok, false);
});

test("rejects non-object messages", () => {
  assert.equal(validateWorkerRequest(null).ok, false);
  assert.equal(validateWorkerRequest("bad").ok, false);
  assert.equal(validateWorkerRequest(42).ok, false);
  assert.equal(validateWorkerRequest([]).ok, false);
});

test("rejects missing requestId", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "clear-models",
  });
  assert.equal(result.ok, false);
});

test("rejects protocol mismatch", () => {
  const result = validateWorkerRequest({
    protocolVersion: 999,
    type: "clear-models",
    requestId: "request:1",
  });
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.code, "WORKER_PROTOCOL_MISMATCH");
    assert.equal(result.requestId, "request:1");
  }
});

test("rejects unknown request type", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "execute-arbitrary-code",
    requestId: "request:1",
  });
  assert.equal(result.ok, false);
});

test("accepts cooperative-cancel", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "cooperative-cancel",
    requestId: "cancel:1",
  });
  assert.equal(result.ok, true);
});

test("accepts clear-models", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "clear-models",
    requestId: "clear:1",
  });
  assert.equal(result.ok, true);
});

test("rejects malformed model artifact", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "load-model",
    requestId: "model:1",
    artifact: { name: "model", bytes: "not-bytes", expectedSha256: "invalid" },
  });
  assert.equal(result.ok, false);
});

test("rejects model with short SHA-256", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "load-model",
    requestId: "model:1",
    artifact: { name: "model", bytes: new Uint8Array(3), expectedSha256: "abc" },
  });
  assert.equal(result.ok, false);
});

test("accepts a valid model artifact", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "load-model",
    requestId: "model:1",
    artifact: { name: "model", bytes: new Uint8Array([1, 2, 3]), expectedSha256: "a".repeat(64) },
  });
  assert.equal(result.ok, true);
});

test("rejects mismatched RGBA buffer", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "recognize",
    requestId: "rec:1",
    input: { width: 2, height: 2, pixels: new Uint8Array(15), mode: "formula" },
  });
  assert.equal(result.ok, false);
});

test("rejects zero dimensions", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "recognize",
    requestId: "rec:1",
    input: { width: 0, height: 2, pixels: new Uint8Array(0), mode: "formula" },
  });
  assert.equal(result.ok, false);
});

test("rejects non-Uint8Array pixels", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "recognize",
    requestId: "rec:1",
    input: { width: 1, height: 1, pixels: [1, 2, 3, 4], mode: "formula" },
  });
  assert.equal(result.ok, false);
});

test("accepts valid recognition input", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "recognize",
    requestId: "rec:1",
    input: { width: 2, height: 2, pixels: new Uint8Array(16), mode: "formula" },
  });
  assert.equal(result.ok, true);
});

test("rejects missing initialize options", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "initialize",
    requestId: "init:1",
  });
  assert.equal(result.ok, false);
});

test("rejects empty moduleUrl", () => {
  const result = validateWorkerRequest({
    protocolVersion: WORKER_PROTOCOL_VERSION,
    type: "initialize",
    requestId: "init:1",
    options: { moduleUrl: "" },
  });
  assert.equal(result.ok, false);
});

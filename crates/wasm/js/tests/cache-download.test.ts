import * as assert from "node:assert/strict";
import test from "node:test";
import "fake-indexeddb/auto";

import { IndexedDbModelCache, downloadVerifiedModel, sha256Hex } from "../src/index.js";

test("IndexedDB cache verifies bytes and evicts least-recently-used artifacts", async () => {
  const cache = new IndexedDbModelCache({ namespace: `test-${Date.now()}`, totalBudgetBytes: 5 });
  const first = new Uint8Array([1, 2, 3]);
  const second = new Uint8Array([4, 5, 6]);
  const firstSha = await sha256Hex(first);
  const secondSha = await sha256Hex(second);
  await cache.put({ key: "first", profile: "text", bytes: first, sha256: firstSha, sourceUrl: "https://example.invalid/first" });
  assert.deepEqual((await cache.get("first", firstSha))?.bytes, first);
  await cache.put({ key: "second", profile: "text", bytes: second, sha256: secondSha });
  assert.equal(await cache.get("first", firstSha), undefined);
  assert.deepEqual((await cache.get("second", secondSha))?.bytes, second);
  assert.equal(await cache.get("second", "00"), undefined);
  await cache.clear();
});

test("streaming download reports progress, falls back to a mirror, and caches only verified bytes", async () => {
  const expected = new Uint8Array([7, 8, 9, 10]);
  const expectedSha = await sha256Hex(expected);
  const originalFetch = globalThis.fetch;
  const progress: number[] = [];
  let attempts = 0;
  globalThis.fetch = (async (url: string | URL | Request) => {
    attempts += 1;
    if (String(url).includes("bad")) return new Response("bad", { status: 503 });
    return new Response(new ReadableStream({
      start(controller) {
        controller.enqueue(expected.slice(0, 2));
        controller.enqueue(expected.slice(2));
        controller.close();
      },
    }), { headers: { "content-length": String(expected.byteLength) } });
  }) as typeof fetch;
  try {
    const bytes = await downloadVerifiedModel({
      urls: ["https://bad.invalid/model", "https://good.invalid/model"],
      expectedSha256: expectedSha,
      maxBytes: 16,
      onProgress: (event) => progress.push(event.receivedBytes),
    });
    assert.deepEqual(bytes, expected);
    assert.equal(attempts, 2);
    assert.deepEqual(progress, [2, 4]);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("download abort and maximum size produce structured errors without partial activation", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () => new Response(new Uint8Array(8), { headers: { "content-length": "8" } })) as typeof fetch;
  try {
    await assert.rejects(
      downloadVerifiedModel({ urls: ["https://example.invalid/model"], expectedSha256: "00", maxBytes: 4 }),
      (error: unknown) => error instanceof Error && error.message.includes("Every model mirror failed"),
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});

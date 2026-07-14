import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { PNG } from "pngjs";

const EXPECTED_SHA256 = "af9a0a4f317ff0709ce752067807f819cb15d883f8ecad89f28df1c6ee2d9c92";
const root = resolve(import.meta.dirname, "../../../..");
const modelPath = resolve(
  process.argv[2] ??
    resolve(root, "target/test-models/PP-LCNet_x1_0_doc_ori_inference.onnx"),
);
const imagePath = resolve(
  process.argv[3] ?? resolve(root, "tests/fixtures/openocr/text-en.png"),
);
const outputPath = process.argv[4] ? resolve(process.argv[4]) : undefined;

const modelBytes = await readFile(modelPath);
const actualSha256 = createHash("sha256").update(modelBytes).digest("hex");
assert.equal(actualSha256, EXPECTED_SHA256, "production model checksum mismatch");

const image = PNG.sync.read(await readFile(imagePath));
assert.equal(image.data.length, image.width * image.height * 4, "fixture must decode to RGBA");

const imported = await import(
  pathToFileURL(resolve(root, "target/wasm-node/latexsnipper_wasm.js"))
);
const wasm = imported.default ?? imported;
assert.equal(typeof wasm.production_orientation_smoke_v2, "function");

const response = wasm.production_orientation_smoke_v2(
  modelBytes,
  image.width,
  image.height,
  image.data,
);
assert.equal(response.ok, true, JSON.stringify(response.error));
assert.equal(response.data.model, "PP-LCNet_x1_0_doc_ori");
assert.equal(response.data.runtime, "tract-wasm");
assert.deepEqual(response.data.inputShape, [1, 3, 224, 224]);
assert.ok(response.data.outputShape.length > 0);
assert.ok(response.data.scores.length >= 4);
assert.ok(response.data.scores.every(Number.isFinite));
assert.ok([0, 90, 180, 270].includes(response.data.predictedDegrees));

const report = {
  schemaVersion: 1,
  benchmarkClass: "production-model-wasm-compatibility",
  accuracyClaim: false,
  model: {
    name: response.data.model,
    source: "PaddlePaddle/PP-LCNet_x1_0_doc_ori_onnx",
    sourceRevision: "7330ab7039123e46af2dc03154b9969aa412c61d",
    license: "Apache-2.0",
    sha256: actualSha256,
    bytes: response.data.modelBytes,
  },
  fixture: {
    path: "tests/fixtures/openocr/text-en.png",
    width: image.width,
    height: image.height,
  },
  runtime: response.data.runtime,
  inputName: response.data.inputName,
  inputShape: response.data.inputShape,
  outputShape: response.data.outputShape,
  predictedDegrees: response.data.predictedDegrees,
  scores: response.data.scores,
  metrics: {
    coldSessionMs: response.data.coldSessionMs,
    coldInferenceMs: response.data.coldInferenceMs,
    warmInferenceMs: response.data.warmInferenceMs,
    estimatedWorkingSetBytes: response.data.estimatedWorkingSetBytes,
  },
};

const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) {
  await writeFile(outputPath, serialized, "utf8");
}
process.stdout.write(serialized);

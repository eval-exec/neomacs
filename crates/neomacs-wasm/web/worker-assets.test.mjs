import assert from "node:assert/strict";
import test from "node:test";

import { fetchEditorWorkerAssets } from "./worker-assets.mjs";

const encoder = new TextEncoder();

function response(contents, status = 200) {
  const bytes = encoder.encode(contents);
  return {
    ok: status >= 200 && status < 300,
    status,
    arrayBuffer: async () => bytes.slice().buffer,
  };
}

const startMessage = {
  wasmUrl: "worker.wasm",
  runtimeImageUrl: "runtime.portable",
  runtimeImageIdUrl: "runtime.portable.sha256",
  runtimeResourceBundleUrl: "runtime.bundle",
  runtimeResourceIdUrl: "runtime.sha256",
};

test("editor Worker fetches both authenticated runtime asset pairs", async () => {
  const requested = [];
  const responses = new Map([
    ["worker.wasm", response("wasm")],
    ["runtime.portable", response("image")],
    ["runtime.portable.sha256", response("image digest")],
    ["runtime.bundle", response("resources")],
    ["runtime.sha256", response("digest")],
  ]);

  const assets = await fetchEditorWorkerAssets(startMessage, async (url) => {
    requested.push(url);
    return responses.get(url);
  });

  assert.deepEqual(requested, [
    "worker.wasm",
    "runtime.portable",
    "runtime.portable.sha256",
    "runtime.bundle",
    "runtime.sha256",
  ]);
  assert.equal(assets.wasmResponse, responses.get("worker.wasm"));
  assert.equal(new TextDecoder().decode(assets.runtimeImage), "image");
  assert.equal(new TextDecoder().decode(assets.runtimeImageId), "image digest");
  assert.equal(new TextDecoder().decode(assets.runtimeResourceBundle), "resources");
  assert.equal(new TextDecoder().decode(assets.runtimeResourceId), "digest");
});

test("editor Worker names a failed runtime resource fetch", async () => {
  const responses = new Map([
    ["worker.wasm", response("wasm")],
    ["runtime.portable", response("image")],
    ["runtime.portable.sha256", response("image digest")],
    ["runtime.bundle", response("missing", 404)],
    ["runtime.sha256", response("digest")],
  ]);

  await assert.rejects(
    fetchEditorWorkerAssets(startMessage, async (url) => responses.get(url)),
    /failed to fetch runtime resource bundle: 404/,
  );
});

test("editor Worker names a failed portable runtime image ID fetch", async () => {
  const responses = new Map([
    ["worker.wasm", response("wasm")],
    ["runtime.portable", response("image")],
    ["runtime.portable.sha256", response("missing", 404)],
    ["runtime.bundle", response("resources")],
    ["runtime.sha256", response("digest")],
  ]);

  await assert.rejects(
    fetchEditorWorkerAssets(startMessage, async (url) => responses.get(url)),
    /failed to fetch portable runtime image ID: 404/,
  );
});

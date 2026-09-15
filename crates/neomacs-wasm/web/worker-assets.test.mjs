import assert from "node:assert/strict";
import test from "node:test";

import { fetchEditorWorkerAssets, observeAssetDownload } from "./worker-assets.mjs";

const encoder = new TextEncoder();

test("compressed download does not compare decoded bytes with encoded length", async () => {
  const updates = [];
  const response = observeAssetDownload(new Response("decoded bytes", {
    headers: { "Content-Length": "3", "Content-Encoding": "gzip" },
  }), update => updates.push(update));
  await response.text();
  assert.deepEqual(updates.at(-1), { received: 13, total: null, complete: true });
});

test("frontend download reports bytes and completion while remaining streamable", async () => {
  const updates = [];
  const response = observeAssetDownload(new Response("frontend", {
    headers: { "Content-Length": "8", "Content-Type": "application/wasm" },
  }), update => updates.push(update));
  assert.deepEqual(updates[0], { received: 0, total: 8, complete: false });
  assert.equal(response.headers.get("Content-Type"), "application/wasm");
  assert.equal(await response.text(), "frontend");
  assert.deepEqual(updates.at(-1), { received: 8, total: 8, complete: true });
});

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

for (const declared of [true, false]) {
  test(`download completion waits for the streamed Wasm body (length declared: ${declared})`, async () => {
    const updates = [];
    const assets = await fetchEditorWorkerAssets(startMessage, async () => new Response("data", {
      headers: declared ? { "Content-Length": "4" } : {},
    }), update => updates.push(update));
    assert.equal(updates.at(-1).complete, false);
    await assets.wasmResponse.arrayBuffer();
    assert.deepEqual(updates.at(-1), { received: 20, total: declared ? 20 : null, complete: true });
  });
}

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

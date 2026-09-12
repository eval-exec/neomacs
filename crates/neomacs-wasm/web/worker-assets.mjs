const assetSpecs = [
  ["wasmResponse", "wasmUrl", "editor Worker Wasm"],
  ["runtimeImage", "runtimeImageUrl", "portable runtime image"],
  ["runtimeImageId", "runtimeImageIdUrl", "portable runtime image ID"],
  ["runtimeResourceBundle", "runtimeResourceBundleUrl", "runtime resource bundle"],
  ["runtimeResourceId", "runtimeResourceIdUrl", "runtime resource bundle ID"],
];

/** Declared length of one response, or null when the server withheld it. */
function declaredLength(response) {
  const raw = response?.headers?.get?.("Content-Length");
  if (raw === null || raw === undefined) return null;
  const length = Number(raw);
  return Number.isFinite(length) && length >= 0 ? length : null;
}

/**
 * Wrap a response so bytes are counted as they flow, without consuming it.
 *
 * The Wasm response is handed to `WebAssembly.instantiateStreaming`, so the
 * body must stay a live stream and the headers must survive (the streaming
 * compiler requires `Content-Type: application/wasm`). Counting therefore
 * happens in a pass-through transform rather than by buffering here.
 *
 * Returns the response unchanged when streams are unavailable — notably for
 * the plain `{ok, status, arrayBuffer}` doubles used by the unit tests, whose
 * bytes are instead counted on completion by the caller.
 */
function countingResponse(response, onChunk) {
  if (!response?.body || typeof TransformStream !== "function") return null;
  const counter = new TransformStream({
    transform(chunk, controller) {
      onChunk(chunk.byteLength ?? chunk.length ?? 0);
      controller.enqueue(chunk);
    },
  });
  return new Response(response.body.pipeThrough(counter), {
    status: response.status,
    headers: response.headers,
  });
}

/**
 * Fetch the complete immutable input set for one editor Worker instance.
 *
 * `onProgress` is optional and receives `{received, total}` as bytes arrive;
 * `total` is null when any response withheld its `Content-Length`, so a caller
 * must render an indeterminate state rather than a false percentage. This is
 * the editor's longest startup phase on a real link — about 88 MB on a first
 * visit — and it used to report nothing at all, which reads as a hang.
 */
export async function fetchEditorWorkerAssets(message, fetchAsset = globalThis.fetch, onProgress) {
  const responses = await Promise.all(
    assetSpecs.map(([, urlField]) => fetchAsset(message[urlField])),
  );
  for (let index = 0; index < assetSpecs.length; index += 1) {
    const [, , description] = assetSpecs[index];
    const response = responses[index];
    if (!response?.ok) {
      throw new Error(`failed to fetch ${description}: ${response?.status ?? "no response"}`);
    }
  }

  const lengths = responses.map(declaredLength);
  const total = lengths.every((length) => length !== null)
    ? lengths.reduce((sum, length) => sum + length, 0)
    : null;
  let received = 0;
  const report = () => {
    try {
      onProgress?.({ received, total });
    } catch {
      // Progress reporting is advisory: never fail a load because the host
      // could not render an update.
    }
  };
  const count = (bytes) => {
    received += bytes;
    report();
  };
  report();

  const counted = responses.map((response) => countingResponse(response, count) ?? response);

  const assets = { wasmResponse: counted[0] };
  await Promise.all(
    assetSpecs.slice(1).map(async ([resultField], index) => {
      const response = counted[index + 1];
      const buffer = await response.arrayBuffer();
      // Streamed responses were already counted chunk by chunk.
      if (!responses[index + 1]?.body) count(buffer.byteLength);
      assets[resultField] = new Uint8Array(buffer);
    }),
  );
  return assets;
}

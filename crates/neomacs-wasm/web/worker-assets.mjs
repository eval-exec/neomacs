const assetSpecs = [
  ["wasmResponse", "wasmUrl", "editor Worker Wasm"],
  ["runtimeImage", "runtimeImageUrl", "portable runtime image"],
  ["runtimeImageId", "runtimeImageIdUrl", "portable runtime image ID"],
  ["runtimeResourceBundle", "runtimeResourceBundleUrl", "runtime resource bundle"],
  ["runtimeResourceId", "runtimeResourceIdUrl", "runtime resource bundle ID"],
];

/** Declared length of one response, or null when the server withheld it. */
function declaredLength(response) {
  // Fetch exposes decoded chunks, but Content-Length describes the encoded
  // transfer. Do not claim a percentage when those byte counts differ.
  const encoding = response?.headers?.get?.("Content-Encoding");
  if (encoding && encoding.toLowerCase() !== "identity") return null;
  const raw = response?.headers?.get?.("Content-Length");
  if (raw === null || raw === undefined) return null;
  const length = Number(raw);
  return Number.isFinite(length) && length >= 0 ? length : null;
}

function reportProgress(callback, progress) {
  try {
    callback?.(progress);
  } catch {
    // Progress is advisory; a UI failure must not abort the asset stream.
  }
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
function countingResponse(response, onChunk, onComplete) {
  if (!response?.body || typeof TransformStream !== "function") return null;
  const counter = new TransformStream({
    transform(chunk, controller) {
      onChunk(chunk.byteLength ?? chunk.length ?? 0);
      controller.enqueue(chunk);
    },
    flush() { onComplete(); },
  });
  return new Response(response.body.pipeThrough(counter), {
    status: response.status,
    headers: response.headers,
  });
}

/** Observe a single streamed asset without buffering it before compilation. */
export function observeAssetDownload(response, onProgress) {
  if (!response.ok) throw new Error(`failed to fetch editor frontend: ${response.status}`);
  let received = 0;
  const total = declaredLength(response);
  reportProgress(onProgress, { received, total, complete: false });
  return countingResponse(response, bytes => {
    received += bytes;
    reportProgress(onProgress, { received, total, complete: false });
  }, () => reportProgress(onProgress, { received, total, complete: true })) ?? response;
}

/**
 * Buffered assets paired with the sidecar file naming their SHA-256.
 * A transfer that corrupts bytes in flight would otherwise surface much
 * later as an unrelated-sounding mount failure inside the Worker.
 */
const digestPairs = [
  ["runtimeImage", "runtimeImageId", "portable runtime image"],
  ["runtimeResourceBundle", "runtimeResourceId", "runtime resource bundle"],
];

async function sha256Hex(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
    byte => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Fetch the complete immutable input set for one editor Worker instance.
 *
 * `onProgress` is optional and receives `{received, total, complete}` as bytes arrive;
 * `total` is null when any response withheld its `Content-Length`, so a caller
 * must render an indeterminate state rather than a false percentage. This is
 * the editor's longest startup phase on a real link — about 88 MB on a first
 * visit — and it used to report nothing at all, which reads as a hang.
 *
 * The buffered image and resource bundles are checked against their shipped
 * SHA-256 sidecars before anything reaches the Worker, and one corrupted
 * transfer is retried rather than reported as a startup failure.
 */
export async function fetchEditorWorkerAssets(message, fetchAsset = globalThis.fetch, onProgress) {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await fetchEditorWorkerAssetsOnce(message, fetchAsset, onProgress);
    } catch (error) {
      if (attempt >= 1 || !/(portable )?runtime (image|resource bundle) digest mismatch/.test(String(error?.message))) {
        throw error;
      }
      reportProgress(onProgress, {received: 0, total: null, complete: false});
    }
  }
}

async function fetchEditorWorkerAssetsOnce(message, fetchAsset, onProgress) {
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
  let completed = 0;
  const report = () => reportProgress(onProgress, {
    received, total, complete: completed === responses.length,
  });
  const count = (bytes) => {
    received += bytes;
    report();
  };
  report();

  const finish = () => { completed += 1; report(); };
  const counted = responses.map((response) => countingResponse(response, count, finish) ?? response);

  const assets = { wasmResponse: counted[0] };
  await Promise.all(
    assetSpecs.slice(1).map(async ([resultField], index) => {
      const response = counted[index + 1];
      const buffer = await response.arrayBuffer();
      // Streamed responses were already counted chunk by chunk.
      if (!responses[index + 1]?.body) { count(buffer.byteLength); finish(); }
      assets[resultField] = new Uint8Array(buffer);
    }),
  );

  for (const [dataField, idField, description] of digestPairs) {
    const expected = new TextDecoder().decode(assets[idField]).trim();
    // Skip verification when the sidecar is not a bare digest (for example
    // the placeholder bodies used by unit-test response doubles).
    if (!/^[0-9a-f]{64}$/.test(expected)) continue;
    if (await sha256Hex(assets[dataField]) !== expected) {
      throw new Error(`${description} digest mismatch; retrying download`);
    }
  }
  return assets;
}

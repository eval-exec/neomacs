// Optional, independently versioned Lisp assets. No user files are installed
// or overwritten: Rust mounts the verified archive as read-only resources.
const MAX_PACKAGE_BYTES = 64 * 1024 * 1024;

async function digest(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
    byte => byte.toString(16).padStart(2, "0")).join("");
}

export async function fetchPackageAssets(manifestUrl, {
  fetcher = globalThis.fetch, cache = null, onProgress, onDetail = () => {},
} = {}) {
  const response = await fetcher(manifestUrl, {signal: AbortSignal.timeout(30000)});
  if (!response.ok) throw new Error(`package manifest: HTTP ${response.status}`);
  const manifest = await response.json();
  if (manifest.schema !== 1 || !/^[0-9a-f]{64}$/.test(manifest.sha256)) {
    throw new Error("invalid package manifest");
  }
  if (!cache) {
    try { cache = await globalThis.caches?.open("neomacs-wasm-packages-v1"); }
    catch { /* Storage restrictions must not prevent an uncached download. */ }
  }
  const key = new URL(`/.neomacs-packages/${manifest.sha256}`, manifestUrl).href;
  let cached;
  try { cached = await cache?.match(key); } catch { /* Download instead. */ }
  if (cached) {
    const bytes = new Uint8Array(await cached.arrayBuffer());
    if (bytes.length <= MAX_PACKAGE_BYTES && await digest(bytes) === manifest.sha256) {
      onDetail("Using cached Treemacs, doom-themes, and keycast bundle");
      onProgress?.({received: bytes.length, total: bytes.length, complete: true});
      return {archive: bytes, id: new TextEncoder().encode(manifest.sha256)};
    }
  }
  onDetail("Downloading Treemacs 3.2, doom-themes, keycast, and Lisp dependencies");
  const download = await fetcher(new URL("packages.bundle", manifestUrl), {signal: AbortSignal.timeout(30000)});
  if (!download.ok) throw new Error(`package download: HTTP ${download.status}`);
  const rawLength = download.headers.get("Content-Length");
  const length = rawLength && !download.headers.get("Content-Encoding") ? Number(rawLength) : null;
  const total = Number.isFinite(length) && length > 0 ? length : null;
  const reader = download.body.getReader();
  const chunks = [];
  let received = 0;
  for (;;) {
    const {value, done} = await reader.read();
    if (done) break;
    received += value.byteLength;
    if (received > MAX_PACKAGE_BYTES) {
      await reader.cancel();
      throw new Error("package bundle exceeds size limit");
    }
    chunks.push(value);
    onProgress?.({received, total, complete: false});
  }
  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  if (await digest(bytes) !== manifest.sha256) throw new Error("package bundle digest mismatch");
  try { await cache?.put(key, new Response(bytes)); }
  catch { onDetail("Package cache unavailable; continuing with verified downloaded assets"); }
  onProgress?.({received, total, complete: true});
  return {archive: bytes, id: new TextEncoder().encode(manifest.sha256)};
}

// Browser HTTP transport. No Lisp state or editor callbacks belong here.
// Bodies remain bytes: character decoding belongs to the URL consumer.
// Fetch owns redirects, decompression, TLS, and header filtering. Returned
// metadata is not a raw HTTP wire response. Browser CORS rules still apply.
// Credentials are deliberately omitted until the Lisp credential policy is
// integrated; this adapter must not implicitly reuse browser login sessions.
// AbortSignal covers both response headers and body consumption. The default
// limit bounds retained payload bytes, not browser-internal network buffering.
export async function fetchHttp(
  { url, method = "GET", headers = [], body },
  { signal, maxResponseBytes = 16 * 1024 * 1024 } = {},
) {
  const target = new URL(url);
  if (target.protocol !== "http:" && target.protocol !== "https:") {
    throw new TypeError("HTTP transport requires an absolute HTTP(S) URL");
  }
  if (!Number.isSafeInteger(maxResponseBytes) || maxResponseBytes < 0) {
    throw new RangeError("maxResponseBytes must be a nonnegative safe integer");
  }
  const response = await fetch(url, { method, headers, body, credentials: "omit", signal });
  const chunks = [];
  let length = 0;
  if (response.body) {
    const reader = response.body.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value.byteLength > maxResponseBytes - length) {
          const error = new Error(`HTTP response exceeds ${maxResponseBytes} bytes`);
          error.name = "HttpResponseTooLargeError";
          throw error;
        }
        chunks.push(value);
        length += value.byteLength;
      }
    } catch (error) {
      await reader.cancel(error).catch(() => {});
      throw error;
    } finally {
      reader.releaseLock();
    }
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return {
    url: response.url,
    status: response.status,
    headers: Array.from(response.headers),
    body: bytes,
  };
}

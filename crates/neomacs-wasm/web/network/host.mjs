// Raw-worker HTTP ABI. The map holds only owned browser data, never VM state.
import { fetchHttp } from "./http.mjs";

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const MAX_METADATA = 64 * 1024;
const MAX_REQUEST_BODY = 1024 * 1024;
const MAX_REQUESTS = 8;

export function createHttpHostImports(getMemory, wake) {
  const requests = new Map();
  let nextId = 1;
  const bytes = (pointer, length) => new Uint8Array(getMemory().buffer, pointer, length);
  const part = (id, field) => {
    const request = requests.get(id);
    if (!request || request.state === "pending") return null;
    return field === 0 ? request.metadata : field === 1 ? request.body : null;
  };
  return {
    http_start(pointer, length, bodyPointer, bodyLength) {
      if (requests.size >= MAX_REQUESTS || nextId > 0xffffffff
          || length > MAX_METADATA || bodyLength > MAX_REQUEST_BODY) return 0;
      let request;
      try {
        request = JSON.parse(decoder.decode(bytes(pointer, length)));
        request.body = request.hasBody ? bytes(bodyPointer, bodyLength).slice() : undefined;
      } catch { return 0; }
      const id = nextId++;
      const controller = new AbortController();
      const pending = { state: "pending", controller };
      requests.set(id, pending);
      const timeout = setTimeout(() => controller.abort(new DOMException("HTTP request timed out", "TimeoutError")), 30000);
      pending.timeout = timeout;
      fetchHttp(request, { signal: controller.signal }).then(response => {
        const metadata = encoder.encode(JSON.stringify([response.status, response.url, response.headers]));
        if (metadata.length > MAX_METADATA) throw new Error("HTTP response metadata exceeds limit");
        if (requests.get(id) === pending) {
          requests.set(id, { state: "complete", metadata, body: response.body });
          wake();
        }
      }).catch(error => {
        if (requests.get(id) === pending) {
          requests.set(id, { state: "failed", metadata: encoder.encode(`${error.name}: ${error.message}`).slice(0, MAX_METADATA), body: new Uint8Array() });
          wake();
        }
      }).finally(() => clearTimeout(timeout));
      return id;
    },
    http_poll(id) {
      switch (requests.get(id)?.state) {
        case "pending": return 0;
        case "complete": return 1;
        case "failed": return 2;
        default: return 3;
      }
    },
    http_result_len(id, field) { return part(id, field)?.length ?? 0; },
    http_copy_result(id, field, pointer, capacity) {
      const source = part(id, field);
      if (!source || source.length > capacity) return 0;
      bytes(pointer, source.length).set(source);
      return source.length;
    },
    http_cancel(id) {
      const request = requests.get(id);
      requests.delete(id);
      if (request?.state === "pending") {
        clearTimeout(request.timeout);
        request.controller.abort();
      }
    },
  };
}

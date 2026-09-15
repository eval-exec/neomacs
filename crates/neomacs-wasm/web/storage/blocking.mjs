// Synchronous editor-side transport. Only the editor worker may block here;
// the storage worker keeps its event loop free to service asynchronous OPFS.
import { HostFileSystemError } from "../opfs-storage.mjs";

const HEADER_BYTES = 16;
const COMPLETE = 1;
const RESIZE = 2;
const BINARY = 1;
const ERROR = 2;
const operations = ["stat", "read", "readDirectory", "write", "createDirectory",
  "removeFile", "removeDirectory", "rename", "canonicalize"];

class StorageTransportError extends Error {}

export async function openBlockingFileSystem() {
  const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
  try {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("storage worker startup timed out")), 30000);
      worker.onmessage = ({ data }) => {
        clearTimeout(timeout);
        if (data?.type === "ready") resolve();
        else reject(new Error(data?.message || "storage worker startup failed"));
      };
      worker.onerror = event => {
        clearTimeout(timeout);
        reject(new Error(event.message));
      };
    });
  } catch (error) {
    worker.terminate();
    throw error;
  }
  let stopped = false;
  function call(operation, args) {
    if (stopped) throw new Error("storage worker is stopped");
    try {
      let buffer = new SharedArrayBuffer(HEADER_BYTES + 4096);
      worker.postMessage({ type: "operation", operation, args, buffer });
      for (;;) {
        const state = new Int32Array(buffer, 0, 4);
        if (Atomics.wait(state, 0, 0, 30000) === "timed-out") {
          throw new StorageTransportError("storage worker operation timed out");
        }
        const length = Atomics.load(state, 1);
        if (length < 0) throw new StorageTransportError("invalid storage reply length");
        if (Atomics.load(state, 0) === RESIZE) {
          // The operation is NOT repeated: only its completed result is copied
          // into a larger reply. Large reads have no fixed mailbox-size limit.
          buffer = new SharedArrayBuffer(HEADER_BYTES + length);
          worker.postMessage({ type: "result", buffer });
          continue;
        }
        if (Atomics.load(state, 0) !== COMPLETE || length > buffer.byteLength - HEADER_BYTES) {
          throw new StorageTransportError("invalid storage reply");
        }
        const bytes = new Uint8Array(buffer, HEADER_BYTES, length).slice();
        const kind = Atomics.load(state, 2);
        if (kind === BINARY) return bytes;
        const value = JSON.parse(new TextDecoder().decode(bytes));
        if (kind === ERROR) {
          if (value.status !== undefined) throw new HostFileSystemError(value.status, value.message);
          const error = new Error(value.message);
          error.name = value.name;
          throw error;
        }
        return value;
      }
    } catch (error) {
      // Ordinary filesystem errors are returned with the browser's name/status.
      // Transport failures make completion uncertain: never replay a mutation.
      if (error instanceof StorageTransportError) {
        stopped = true;
        worker.terminate();
      }
      throw error;
    }
  }
  return Object.fromEntries(operations.map(name => [name, (...args) => call(name, args)]));
}

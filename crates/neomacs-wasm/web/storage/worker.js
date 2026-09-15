// Owns browser storage, never Lisp state or the evaluator's linear memory.
import { OriginPrivateFileSystem } from "../opfs-storage.mjs";

const operations = new Set(["stat", "read", "readDirectory", "write", "createDirectory",
  "removeFile", "removeDirectory", "rename", "canonicalize"]);
let pending = null;

function reply(buffer) {
  const state = new Int32Array(buffer, 0, 4);
  Atomics.store(state, 1, pending.bytes.byteLength);
  Atomics.store(state, 2, pending.kind);
  if (buffer.byteLength - 16 < pending.bytes.byteLength) {
    Atomics.store(state, 0, 2);
  } else {
    new Uint8Array(buffer, 16, pending.bytes.byteLength).set(pending.bytes);
    pending = null;
    Atomics.store(state, 0, 1);
  }
  Atomics.notify(state, 0);
}

try {
  const filesystem = await OriginPrivateFileSystem.open();
  self.onmessage = async ({ data }) => {
    if (data.type === "result") {
      reply(data.buffer);
      return;
    }
    try {
      if (data.type !== "operation" || !operations.has(data.operation)) {
        throw new Error("invalid storage operation");
      }
      const value = await filesystem[data.operation](...data.args);
      pending = value instanceof Uint8Array
        ? { kind: 1, bytes: value }
        : { kind: 0, bytes: new TextEncoder().encode(JSON.stringify(value ?? null)) };
    } catch (error) {
      pending = { kind: 2, bytes: new TextEncoder().encode(JSON.stringify({
        name: error.name, message: error.message, status: error.status,
      })) };
    }
    reply(data.buffer);
  };
  self.postMessage({ type: "ready" });
} catch (error) {
  self.postMessage({ type: "failed", message: error.message });
}

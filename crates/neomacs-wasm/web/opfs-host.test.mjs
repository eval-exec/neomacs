import assert from "node:assert/strict";
import test from "node:test";
import { createOpfsHostImports, HostFileSystemError, HOST_STATUS } from "./opfs-storage.mjs";

// Both transports expose the same Rust import ABI. Only JSPI may return a
// Promise; the Atomics transport must return an integer before reentering Wasm.
for (const asynchronous of [false, true]) {
  test(`filesystem imports preserve results and errors (${asynchronous ? "JSPI" : "blocking"})`, async () => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const bytes = new Uint8Array(memory.buffer);
    const path = new TextEncoder().encode("/neomacs-fake/test");
    bytes.set(path);
    const read = name => {
      assert.equal(name, "/neomacs-fake/test");
      return Uint8Array.of(0, 127, 255);
    };
    const missing = () => { throw new HostFileSystemError(HOST_STATUS.NOT_FOUND, "missing"); };
    const imports = createOpfsHostImports({
      read: asynchronous ? async name => read(name) : read,
      stat: asynchronous ? async () => missing() : missing,
    }, () => memory);
    const result = imports.fs_read(0, path.length);
    assert.equal(result instanceof Promise, asynchronous);
    assert.equal(await result, HOST_STATUS.OK);
    assert.equal(imports.fs_result_len(), 3);
    assert.equal(imports.fs_copy_result(100, 3), 3);
    assert.deepEqual(bytes.slice(100, 103), Uint8Array.of(0, 127, 255));
    assert.equal(await imports.fs_stat(0, path.length), HOST_STATUS.NOT_FOUND);
    assert.equal(imports.fs_result_len(), 0);
    const length = imports.fs_result_error_len();
    imports.fs_copy_result_error(100, length);
    assert.equal(new TextDecoder().decode(bytes.slice(100, 100 + length)), "missing");
  });
}

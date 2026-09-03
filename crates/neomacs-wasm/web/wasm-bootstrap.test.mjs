import assert from "node:assert/strict";
import test from "node:test";

import { initializeWasmFrontend } from "./wasm-bootstrap.mjs";

test("browser bootstrap gives wasm-bindgen the frontend module URL", async () => {
  const moduleUrl = new URL("https://example.test/neomacs_wasm_bg.wasm");
  let receivedOptions = null;

  await initializeWasmFrontend(async (options) => {
    receivedOptions = options;
  }, moduleUrl);

  assert.deepEqual(receivedOptions, { module_or_path: moduleUrl });
});

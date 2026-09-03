import assert from "node:assert/strict";
import test from "node:test";

import {
  initializeWasmFrontend,
  observeFirstEditorPresentation,
} from "./wasm-bootstrap.mjs";

test("browser bootstrap gives wasm-bindgen the frontend module URL", async () => {
  const moduleUrl = new URL("https://example.test/neomacs_wasm_bg.wasm");
  let receivedOptions = null;

  await initializeWasmFrontend(async (options) => {
    receivedOptions = options;
  }, moduleUrl);

  assert.deepEqual(receivedOptions, { module_or_path: moduleUrl });
});

test("frontend stays unready until Rust reports a presented editor frame", async () => {
  let resolvePresentation;
  const presentation = new Promise((resolve) => {
    resolvePresentation = resolve;
  });
  let readyPresentation = null;

  const observation = observeFirstEditorPresentation(
    () => presentation,
    (id) => {
      readyPresentation = id;
    },
    () => assert.fail("presentation should not fail"),
  );

  assert.equal(readyPresentation, null);
  resolvePresentation("41");
  await observation;
  assert.equal(readyPresentation, "41");
});

test("renderer readiness rejection reaches the failure handler unchanged", async () => {
  const expected = new Error("no compatible browser graphics backend");
  let received = null;

  await observeFirstEditorPresentation(
    () => Promise.reject(expected),
    () => assert.fail("presentation should not become ready"),
    (error) => {
      received = error;
    },
  );

  assert.equal(received, expected);
});

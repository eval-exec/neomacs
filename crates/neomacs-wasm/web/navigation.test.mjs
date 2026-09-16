import assert from "node:assert/strict";
import test from "node:test";
import { createNavigationHostImports, openExternalUrl } from "./navigation.mjs";

test("profile opens in a detached browser tab", () => {
  let destination;
  const tab = { opener: {}, location: { replace: url => { destination = url; } } };
  assert.equal(openExternalUrl("https://github.com/eval-exec", (url, target) => {
    assert.equal(url, "about:blank");
    assert.equal(target, "_blank");
    return tab;
  }), true);
  assert.equal(tab.opener, null);
  assert.equal(destination, "https://github.com/eval-exec");
});

test("a blocked tab reports that a user-clickable fallback is needed", () => {
  assert.equal(openExternalUrl("https://github.com/eval-exec", () => null), false);
});

test("worker accepts only absolute web URLs and copies them out of Wasm memory", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const sent = [];
  const host = createNavigationHostImports(() => memory, url => sent.push(url));
  const bytes = new TextEncoder().encode("https://github.com/eval-exec");
  new Uint8Array(memory.buffer).set(bytes);
  assert.equal(host.open_external_url(0, bytes.length), 1);
  new Uint8Array(memory.buffer).fill(0);
  assert.deepEqual(sent, ["https://github.com/eval-exec"]);
  for (const value of ["javascript:alert(1)", "file:///home/test", "/relative", "not a URL"]) {
    const bytes = new TextEncoder().encode(value);
    new Uint8Array(memory.buffer).set(bytes);
    assert.equal(host.open_external_url(0, bytes.length), 0);
    assert.throws(() => openExternalUrl(value, () => assert.fail("must not open")));
  }
  assert.equal(sent.length, 1);
  assert.equal(host.open_external_url(memory.buffer.byteLength, 1), 0);
});

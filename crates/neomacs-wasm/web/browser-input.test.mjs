import assert from "node:assert/strict";
import test from "node:test";

import { installBrowserInput } from "./browser-input.mjs";

class FakeEventTarget {
  constructor() {
    this.listeners = new Map();
    this.value = "";
    this.focusCalls = 0;
  }

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  dispatch(type, event = {}) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  focus() {
    this.focusCalls += 1;
  }
}

function keyEvent(key, overrides = {}) {
  return {
    key,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    isComposing: false,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    ...overrides,
  };
}

function harness() {
  const root = new FakeEventTarget();
  const textInput = new FakeEventTarget();
  const batches = [];
  let viewportCalls = 0;
  installBrowserInput({
    root,
    textInput,
    enqueueInput: (events) => batches.push(events),
    targetFrame: () => "41",
    sendViewport: () => {
      viewportCalls += 1;
    },
  });
  return { root, textInput, batches, viewportCalls: () => viewportCalls };
}

test("plain keyboard text is committed through the text service exactly once", () => {
  const { root, textInput, batches } = harness();
  const down = keyEvent("a");

  root.dispatch("keydown", down);
  assert.equal(down.defaultPrevented, false);
  assert.deepEqual(batches, []);

  textInput.value = "a";
  textInput.dispatch("input", { isComposing: false });
  assert.deepEqual(batches, [[{
    type: "text-committed",
    text: "a",
    target: "41",
  }]]);
  assert.equal(textInput.value, "");
});

test("IME updates remain local until one final Unicode commit", () => {
  const { textInput, batches } = harness();

  textInput.dispatch("compositionstart");
  textInput.value = "に";
  textInput.dispatch("input", { isComposing: true });
  assert.deepEqual(batches, []);

  textInput.value = "日本";
  textInput.dispatch("compositionend");
  textInput.dispatch("input", { isComposing: false });
  assert.deepEqual(batches, [[{
    type: "text-committed",
    text: "日本",
    target: "41",
  }]]);
});

test("command keys bypass the text service with sampled modifiers", () => {
  const { root, batches } = harness();
  const down = keyEvent("ArrowLeft", { ctrlKey: true });

  root.dispatch("keydown", down);

  assert.equal(down.defaultPrevented, true);
  assert.deepEqual(batches, [[{
    type: "key",
    symbol: 0xff51,
    modifiers: {
      shift: false,
      control: true,
      meta: false,
      super_: false,
    },
    state: "pressed",
    target: "41",
  }]]);
});

test("page lifecycle emits at most one typed close request", () => {
  const { root, batches } = harness();

  root.dispatch("pagehide");
  root.dispatch("beforeunload");

  assert.deepEqual(batches, [[{
    type: "close-requested",
    target: "41",
  }]]);
});

test("pointer activation restores the browser text service focus", () => {
  const { root, textInput } = harness();

  root.dispatch("pointerdown");

  assert.equal(textInput.focusCalls, 1);
});

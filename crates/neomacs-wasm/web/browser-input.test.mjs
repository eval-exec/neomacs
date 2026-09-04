import assert from "node:assert/strict";
import test from "node:test";

import {
  installBrowserInput,
  observeBrowserEditorGeometry,
  observeBrowserViewport,
} from "./browser-input.mjs";

class FakeEventTarget {
  constructor() {
    this.listeners = new Map();
    this.listenerOptions = new Map();
    this.value = "";
    this.focusCalls = 0;
  }

  addEventListener(type, listener, options) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
    const optionList = this.listenerOptions.get(type) ?? [];
    optionList.push(options);
    this.listenerOptions.set(type, optionList);
  }

  removeEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    this.listeners.set(type, listeners.filter((candidate) => candidate !== listener));
  }

  dispatch(type, event = {}) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  focus() {
    this.focusCalls += 1;
  }
}

class FakeBrowserTarget extends FakeEventTarget {
  constructor() {
    super();
    this.devicePixelRatio = 1.75;
    this.mediaQueries = [];
  }

  matchMedia(media) {
    const query = new FakeEventTarget();
    query.media = media;
    this.mediaQueries.push(query);
    return query;
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
  const root = new FakeBrowserTarget();
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

test("device-scale-only changes publish and re-arm viewport observation", () => {
  const { root, viewportCalls } = harness();
  const initialQuery = root.mediaQueries[0];

  assert.equal(initialQuery.media, "(resolution: 1.75dppx)");
  root.devicePixelRatio = 2;
  initialQuery.dispatch("change");

  assert.equal(viewportCalls(), 1);
  assert.equal(root.mediaQueries[1].media, "(resolution: 2dppx)");
  assert.equal(initialQuery.listeners.get("change").length, 0);
});

test("HiDPI viewport observations keep editor geometry in CSS pixels", () => {
  const browser = {
    innerWidth: 1975,
    innerHeight: 1100,
    devicePixelRatio: 1.75,
  };

  assert.deepEqual(observeBrowserViewport(browser), {
    width: 1975,
    height: 1100,
    scale_factor: 1.75,
  });
});

test("HiDPI startup keeps font measurements in editor logical pixels", () => {
  const browser = {
    innerWidth: 1975,
    innerHeight: 1100,
    devicePixelRatio: 1.75,
  };

  assert.deepEqual(observeBrowserEditorGeometry(browser), {
    width: 1975,
    height: 1100,
    scale_factor: 1.75,
    character_width: 8,
    character_height: 16,
    font_pixel_size: 16,
  });
});

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

test("installation claims editor keyboard focus and browser focus restores it", () => {
  const { root, textInput, batches } = harness();

  assert.equal(textInput.focusCalls, 1);

  root.dispatch("focus");
  assert.equal(textInput.focusCalls, 2);
  assert.deepEqual(batches, [[{
    type: "focus-changed",
    focused: true,
    target: "41",
  }]]);
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

test("pointer activation restores text focus after canvas target handlers", () => {
  const { root, textInput } = harness();

  assert.deepEqual(root.listenerOptions.get("pointerdown"), [false]);
  root.dispatch("pointerdown");

  assert.equal(textInput.focusCalls, 2);
});

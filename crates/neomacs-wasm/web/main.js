import init, { install_worker_presentation } from "./neomacs_wasm.js";

const MAILBOX_CAPACITY = 1024 * 1024;
const MAILBOX_HEADER_BYTES = 16;
const encoder = new TextEncoder();
const status = document.querySelector("#browser-status");

let worker = null;
let workerStrategy = null;
let mailbox = null;
let inputSequence = 1;
let inputInFlight = false;
let inputQueue = [];
let targetFrame = 0;
let activePresentation = null;

function showFailure(error) {
  status.dataset.state = "failed";
  status.textContent = `Neomacs failed to start: ${error instanceof Error ? error.message : String(error)}`;
  console.error(error);
}

function enqueueInput(events) {
  if (events.length === 0) return;
  inputQueue.push({ sequence: inputSequence++, events });
  flushInput();
}

function flushInput() {
  if (!workerStrategy || inputInFlight || inputQueue.length === 0) return;
  const batch = inputQueue[0];
  if (workerStrategy === "jspi") {
    worker.postMessage({ type: "input", batch });
  } else {
    const bytes = encoder.encode(JSON.stringify(batch));
    if (bytes.byteLength > MAILBOX_CAPACITY) {
      showFailure(new Error(`browser input batch exceeds ${MAILBOX_CAPACITY} bytes`));
      return;
    }
    const state = new Int32Array(mailbox, 0, 4);
    if (Atomics.load(state, 0) !== 0) return;
    new Uint8Array(mailbox, MAILBOX_HEADER_BYTES, bytes.byteLength).set(bytes);
    Atomics.store(state, 1, bytes.byteLength);
    Atomics.store(state, 0, 1);
    Atomics.notify(state, 0, 1);
  }
  inputInFlight = true;
}

function inputAccepted() {
  if (inputInFlight) inputQueue.shift();
  inputInFlight = false;
  flushInput();
}

function installFrame(payload) {
  const receipt = install_worker_presentation(new Uint8Array(payload));
  const [presentation, target] = receipt.split(",").map(Number);
  const events = [{ type: "presentation-activated", presentation, target }];
  if (activePresentation !== null) {
    events.push({ type: "presentation-retired", presentation: activePresentation });
  }
  activePresentation = presentation;
  targetFrame = target;
  enqueueInput(events);
  status.textContent = `Neomacs ready (${workerStrategy} Worker suspension)`;
  status.dataset.state = "ready";
}

function modifierSample(event) {
  return {
    shift: event.shiftKey,
    control: event.ctrlKey,
    meta: event.altKey,
    super_: event.metaKey,
  };
}

const NAMED_KEY_SYMBOLS = new Map([
  ["Backspace", 0xff08],
  ["Tab", 0xff09],
  ["Enter", 0xff0d],
  ["Escape", 0xff1b],
  ["Home", 0xff50],
  ["ArrowLeft", 0xff51],
  ["ArrowUp", 0xff52],
  ["ArrowRight", 0xff53],
  ["ArrowDown", 0xff54],
  ["PageUp", 0xff55],
  ["PageDown", 0xff56],
  ["End", 0xff57],
  ["Insert", 0xff63],
  ["Delete", 0xffff],
]);

function keySymbol(event) {
  if (NAMED_KEY_SYMBOLS.has(event.key)) return NAMED_KEY_SYMBOLS.get(event.key);
  if (event.key.length === 1) return event.key.codePointAt(0);
  const functionKey = /^F(\d{1,2})$/.exec(event.key);
  if (functionKey) {
    const number = Number(functionKey[1]);
    if (number >= 1 && number <= 35) return 0xffbd + number;
  }
  return null;
}

function sendKey(event, state) {
  if (event.isComposing) return;
  const symbol = keySymbol(event);
  if (symbol === null) return;
  event.preventDefault();
  enqueueInput([{
    type: "key",
    symbol,
    modifiers: modifierSample(event),
    state,
    target: targetFrame,
  }]);
}

function sendViewport() {
  const scale = globalThis.devicePixelRatio || 1;
  enqueueInput([{
    type: "viewport-changed",
    width: Math.max(1, Math.round(globalThis.innerWidth * scale)),
    height: Math.max(1, Math.round(globalThis.innerHeight * scale)),
    scale_factor: scale,
    target: targetFrame,
  }]);
}

function installBrowserInput() {
  globalThis.addEventListener("keydown", (event) => sendKey(event, "pressed"), true);
  globalThis.addEventListener("keyup", (event) => sendKey(event, "released"), true);
  globalThis.addEventListener("resize", sendViewport);
  globalThis.addEventListener("focus", () => enqueueInput([{
    type: "focus-changed",
    focused: true,
    target: targetFrame,
  }]));
  globalThis.addEventListener("blur", () => enqueueInput([{
    type: "focus-changed",
    focused: false,
    target: targetFrame,
  }]));
}

async function start() {
  if (!("gpu" in navigator)) {
    throw new Error("this browser does not expose WebGPU");
  }
  if (typeof Worker !== "function") {
    throw new Error("this browser does not expose module Workers");
  }

  const jspi = typeof WebAssembly.Suspending === "function"
    && typeof WebAssembly.promising === "function";
  if (!jspi && !globalThis.crossOriginIsolated) {
    throw new Error("this browser needs JSPI or cross-origin isolation for Atomics input waits");
  }

  await init();
  mailbox = globalThis.crossOriginIsolated
    ? new SharedArrayBuffer(MAILBOX_HEADER_BYTES + MAILBOX_CAPACITY)
    : null;
  worker = new Worker(new URL("./editor-worker.js", import.meta.url), { type: "module" });
  worker.onmessage = (event) => {
    const message = event.data;
    if (message?.type === "probe-waiting") {
      if (mailbox) {
        const state = new Int32Array(mailbox, 0, 4);
        Atomics.store(state, 0, 1);
        Atomics.notify(state, 0, 1);
      }
      worker.postMessage({ type: "wake-probe" });
    } else if (message?.type === "ready") {
      workerStrategy = message.strategy;
      status.textContent = "Restoring Neomacs editor session…";
      installBrowserInput();
      sendViewport();
      flushInput();
    } else if (message?.type === "input-accepted") {
      inputAccepted();
    } else if (message?.type === "frame") {
      try {
        installFrame(message.payload);
      } catch (error) {
        showFailure(error);
        worker.terminate();
      }
    } else if (message?.type === "status") {
      status.textContent = message.message;
    } else if (message?.type === "failed") {
      showFailure(new Error(message.message));
    } else if (message?.type === "exited") {
      showFailure(new Error(`editor Worker exited with status ${message.exitCode}`));
    }
  };
  worker.onerror = (event) => showFailure(new Error(event.message));

  const scale = globalThis.devicePixelRatio || 1;
  worker.postMessage({
    type: "start",
    wasmUrl: new URL("./neomacs_wasm_worker.wasm", import.meta.url).href,
    runtimeImageUrl: new URL("./assets/neomacs.portable", import.meta.url).href,
    mailbox,
    startup: {
      protocol_version: 1,
      width: Math.max(1, Math.round(globalThis.innerWidth * scale)),
      height: Math.max(1, Math.round(globalThis.innerHeight * scale)),
      scale_factor: scale,
      character_width: 8 * scale,
      character_height: 16 * scale,
      font_pixel_size: 16 * scale,
      color_scheme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
    },
  });
}

start().catch(showFailure);

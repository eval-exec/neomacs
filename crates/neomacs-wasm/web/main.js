import init, {
  install_worker_presentation,
  worker_protocol_version,
} from "./neomacs_wasm.js";
import { installBrowserInput } from "./browser-input.mjs";

const MAILBOX_CAPACITY = 1024 * 1024;
const MAILBOX_HEADER_BYTES = 16;
const encoder = new TextEncoder();
const status = document.querySelector("#browser-status");

let worker = null;
let workerStrategy = null;
let mailbox = null;
let inputSequence = 1n;
let inputInFlight = false;
let inputQueue = [];
let targetFrame = "0";
let activePresentation = null;

function showFailure(error) {
  status.dataset.state = "failed";
  status.textContent = `Neomacs failed to start: ${error instanceof Error ? error.message : String(error)}`;
  console.error(error);
}

function enqueueInput(events) {
  if (events.length === 0) return;
  inputQueue.push({ sequence: (inputSequence++).toString(), events });
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

function inputAccepted(sequence) {
  const expected = inputQueue[0]?.sequence;
  if (!inputInFlight || expected !== sequence) {
    throw new Error(
      `editor Worker acknowledged input ${String(sequence)}; expected ${String(expected)}`,
    );
  }
  inputQueue.shift();
  inputInFlight = false;
  flushInput();
}

function installFrame(payload) {
  const receipt = install_worker_presentation(new Uint8Array(payload));
  const presentation = receipt.presentation;
  const target = receipt.target;
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
      installBrowserInput({
        root: globalThis,
        textInput: document.querySelector("#browser-text-input"),
        enqueueInput,
        targetFrame: () => targetFrame,
        sendViewport,
      });
      sendViewport();
      flushInput();
    } else if (message?.type === "input-accepted") {
      try {
        inputAccepted(message.sequence);
      } catch (error) {
        showFailure(error);
        worker.terminate();
      }
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
      worker.terminate();
    } else if (message?.type === "exited") {
      status.dataset.state = message.exitCode === 0 ? "stopped" : "failed";
      status.textContent = `Neomacs stopped (status ${message.exitCode})`;
      worker = null;
    }
  };
  worker.onerror = (event) => showFailure(new Error(event.message));

  const scale = globalThis.devicePixelRatio || 1;
  worker.postMessage({
    type: "start",
    wasmUrl: new URL("./neomacs_wasm_worker.wasm", import.meta.url).href,
    runtimeImageUrl: new URL("./assets/neomacs.portable", import.meta.url).href,
    runtimeResourceBundleUrl: new URL(
      "./assets/neomacs-runtime.bundle",
      import.meta.url,
    ).href,
    runtimeResourceIdUrl: new URL(
      "./assets/neomacs-runtime.sha256",
      import.meta.url,
    ).href,
    mailbox,
    startup: {
      protocol_version: worker_protocol_version(),
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

import init, {
  install_worker_presentation,
  browser_pointer_input,
  set_presentation_callback,
  wait_for_first_editor_presentation,
  worker_protocol_version,
} from "./neomacs_wasm.js";
import {
  installBrowserInput,
  observeBrowserEditorGeometry,
  observeBrowserViewport,
} from "./browser-input.mjs";
import {
  initializeWasmFrontend,
  observeFirstEditorPresentation,
} from "./wasm-bootstrap.mjs";

const MAILBOX_CAPACITY = 1024 * 1024;
const MAILBOX_HEADER_BYTES = 16;
const encoder = new TextEncoder();
const status = document.querySelector("#browser-status");
const progress = document.querySelector("#browser-progress");
const progressBar = document.querySelector("#browser-progress-bar");
const progressLabel = document.querySelector("#browser-progress-label");

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
  hideProgress();
  console.error(error);
}

function hideProgress() {
  if (progress) progress.hidden = true;
}

const MIB = 1024 * 1024;
const megabytes = (bytes) => (bytes / MIB).toFixed(1);

/**
 * Render transfer progress.
 *
 * Deliberately independent of the status line: `instantiateStreaming` compiles
 * from the same body it is still downloading, so bytes keep arriving after the
 * phase has moved on to compiling. The status line owns the phase name and the
 * bar owns the byte count; neither overwrites the other.
 *
 * `total` is null when a response withheld its `Content-Length` — show bytes
 * received and an indeterminate bar rather than inventing a percentage.
 */
function showProgress(received, total) {
  if (!progress || !progressBar) return;
  progress.hidden = false;
  if (total) {
    const ratio = Math.min(1, Math.max(0, received / total));
    const percent = Math.floor(ratio * 100);
    progress.dataset.state = "determinate";
    progress.setAttribute("aria-valuenow", String(percent));
    progressBar.style.inlineSize = `${ratio * 100}%`;
    if (progressLabel) {
      progressLabel.textContent =
        `${percent}% · ${megabytes(received)} / ${megabytes(total)} MiB`;
    }
  } else {
    progress.dataset.state = "indeterminate";
    progress.removeAttribute("aria-valuenow");
    progressBar.style.inlineSize = "100%";
    if (progressLabel) progressLabel.textContent = `${megabytes(received)} MiB`;
  }
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

function inputSettled(sequence) {
  const expected = inputQueue[0]?.sequence;
  if (!inputInFlight || expected !== sequence) {
    throw new Error(
      `editor Worker settled input ${String(sequence)}; expected ${String(expected)}`,
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
  receipt.free();
  if (pendingPresentation !== null) {
    enqueueInput([{ type: "presentation-discarded", ...pendingPresentation }]);
  }
  pendingPresentation = { presentation, target };
}

let pendingPresentation = null;
function didPresentFrame(presentation, target) {
  if (pendingPresentation?.presentation === presentation) pendingPresentation = null;
  const events = [{ type: "presentation-activated", presentation, target }];
  if (activePresentation !== null) {
    events.push({ type: "presentation-retired", presentation: activePresentation });
  }
  activePresentation = presentation;
  targetFrame = target;
  enqueueInput(events);
}

function sendViewport() {
  enqueueInput([{
    type: "viewport-changed",
    ...observeBrowserViewport(globalThis),
    target: targetFrame,
  }]);
}

async function start() {
  if (typeof Worker !== "function") {
    throw new Error("this browser does not expose module Workers");
  }

  const jspi = typeof WebAssembly.Suspending === "function"
    && typeof WebAssembly.promising === "function";
  if (!jspi && !globalThis.crossOriginIsolated) {
    throw new Error("this browser needs JSPI or cross-origin isolation for Atomics input waits");
  }

  // The frontend module is ~10 MiB and is fetched and compiled on this thread
  // before the Worker is even spawned, so without this the first seconds of a
  // cold load report nothing at all.
  status.textContent = "Loading editor frontend…";
  await initializeWasmFrontend(
    init,
    new URL("./neomacs_wasm_bg.wasm", import.meta.url),
  );
  status.textContent = "Starting editor Worker…";
  set_presentation_callback(didPresentFrame);
  void observeFirstEditorPresentation(
    wait_for_first_editor_presentation,
    (presentation) => {
      status.textContent = `Neomacs ready (${workerStrategy} Worker suspension, presentation ${presentation})`;
      status.dataset.state = "ready";
      hideProgress();
    },
    (error) => {
      showFailure(error);
      worker?.terminate();
      worker = null;
    },
  );
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
        observePointer: browser_pointer_input,
      });
      sendViewport();
      flushInput();
    } else if (message?.type === "input-accepted" || message?.type === "input-rejected") {
      try {
        if (message.type === "input-rejected") {
          console.error("Editor rejected browser input:", message.message);
        }
        inputSettled(message.sequence);
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
    } else if (message?.type === "progress") {
      showProgress(message.received, message.total);
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

  worker.postMessage({
    type: "start",
    wasmUrl: new URL("./neomacs_wasm_worker.wasm", import.meta.url).href,
    runtimeImageUrl: new URL("./assets/neomacs.portable", import.meta.url).href,
    runtimeImageIdUrl: new URL(
      "./assets/neomacs.portable.sha256",
      import.meta.url,
    ).href,
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
      ...observeBrowserEditorGeometry(globalThis),
      color_scheme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
    },
  });
}

start().catch(showFailure);

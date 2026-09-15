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
import { observeAssetDownload } from "./worker-assets.mjs";

const MAILBOX_CAPACITY = 1024 * 1024;
const MAILBOX_HEADER_BYTES = 16;
const encoder = new TextEncoder();
const status = document.querySelector("#browser-status");
const startupOverlay = document.querySelector("#browser-startup");
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
let reportedScale = null;
let activePresentation = null;

function updatePhase(phase, state) {
  // The checklist owner enforces its terminal-state policy.
  document.dispatchEvent(new CustomEvent("neomacs:startup-phase", { detail: {phase, state} }));
}

function phaseDetail(phase, message) {
  document.dispatchEvent(new CustomEvent("neomacs:startup-detail", { detail: {phase, message} }));
}

function showFailure(error) {
  settleStartup("failed", `Neomacs failed: ${error instanceof Error ? error.message : String(error)}`);
  console.error(error);
}

function startupIsSettled() {
  return ["ready", "failed", "stopped"].includes(status.dataset.state);
}

function settleStartup(state, message) {
  // A presentation promise may resolve after startup has already failed.
  if (state === "ready" && startupIsSettled()) return;
  updatePhase(state === "ready" ? "first-frame" : null, state === "ready" ? "done" : "failed");
  if (state === "ready") document.querySelector("#browser-startup-log").hidden = true;
  status.dataset.state = state;
  status.textContent = message;
  hideProgress();
  // Retain the status host for later runtime failures, but remove the entire
  // startup layout from rendering immediately on the first editor frame.
  startupOverlay.hidden = state === "ready";
}

function hideProgress() {
  if (progress) progress.hidden = true;
}

const MIB = 1024 * 1024;
const megabytes = (bytes) => (bytes / MIB).toFixed(1);

/**
 * Render transfer progress.
 *
 * `instantiateStreaming` compiles from the body it is still downloading.
 * Stream completion, not compiler status or an estimated percentage, decides
 * when to hide the bar and switch to the text-only initialization state.
 *
 * `total` is null when a response withheld its `Content-Length` — show bytes
 * received and an indeterminate bar rather than inventing a percentage.
 */
function showProgress(received, total, complete = false) {
  if (startupIsSettled()) return;
  if (complete) {
    hideProgress();
    return;
  }
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
  reconcileDeviceScale();
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
  const viewport = observeBrowserViewport(globalThis);
  reportedScale = viewport.scale_factor;
  enqueueInput([{ type: "viewport-changed", ...viewport, target: targetFrame }]);
}

/**
 * Re-send the viewport if the device scale drifted from what the evaluator was
 * last told.
 *
 * `installDeviceScaleViewportObserver` arms a `(resolution: Xdppx)` media query
 * for the fast path, but correctness cannot rest on that notification alone: a
 * scale change with an unchanged CSS viewport emits no `resize`, and Chrome
 * does not dispatch resolution-query changes at all when the scale moves via
 * CDP device-metrics emulation -- which is how the HiDPI smoke drives it.
 *
 * Checked here because a scale change always provokes a repaint, so a frame is
 * the one event guaranteed to follow it. Self-limiting: the re-send updates
 * `reportedScale`, so the next frame finds them equal.
 */
function reconcileDeviceScale() {
  const scale = globalThis.devicePixelRatio || 1;
  if (reportedScale !== null && scale !== reportedScale) sendViewport();
}

async function start() {
  updatePhase("frontend-modules", "done");
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
  updatePhase("frontend-download", "active");
  const frontend = observeAssetDownload(
    await fetch(new URL("./neomacs_wasm_bg.wasm", import.meta.url)),
    ({ received, total, complete }) => {
      showProgress(received, total, complete);
      if (complete) {
        phaseDetail("frontend-download", `Received ${received} bytes`);
        updatePhase("frontend-download", "done");
      }
    },
  );
  updatePhase("frontend-init", "active");
  await initializeWasmFrontend(
    init,
    frontend,
  );
  updatePhase("frontend-init", "done");
  updatePhase("worker-start", "active");
  set_presentation_callback(didPresentFrame);
  void observeFirstEditorPresentation(
    wait_for_first_editor_presentation,
    (presentation) => {
      settleStartup("ready", `Neomacs ready (${workerStrategy} Worker suspension, presentation ${presentation})`);
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
      showProgress(message.received, message.total, message.complete);
      if (message.complete) {
        const phase = message.phase === "packages" ? "packages" : "worker-download";
        phaseDetail(phase, `Received ${message.received} bytes`);
        updatePhase(phase, "done");
      }
    } else if (message?.type === "startup-phase") {
      updatePhase(message.phase, message.state);
      if (message.phase === "packages" && message.state === "done") hideProgress();
    } else if (message?.type === "startup-detail") {
      phaseDetail(message.phase, message.message);
    } else if (message?.type === "status") {
      // Informational runtime messages do not determine checklist completion.
    } else if (message?.type === "failed") {
      showFailure(new Error(message.message));
      worker.terminate();
    } else if (message?.type === "exited") {
      settleStartup(message.exitCode === 0 ? "stopped" : "failed",
        `Neomacs stopped (status ${message.exitCode})`);
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

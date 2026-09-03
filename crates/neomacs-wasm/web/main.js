import init from "./neomacs_wasm.js";

const status = document.querySelector("#browser-status");

function showFailure(error) {
  status.dataset.state = "failed";
  status.textContent = `Neomacs failed to start: ${error instanceof Error ? error.message : String(error)}`;
  console.error(error);
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

  const mailbox = jspi ? null : new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT);
  const worker = new Worker(new URL("./editor-worker.js", import.meta.url), { type: "module" });
  worker.onmessage = (event) => {
    const message = event.data;
    if (message?.type === "suspension-waiting") {
      if (mailbox) {
        const state = new Int32Array(mailbox);
        Atomics.store(state, 0, 1);
        Atomics.notify(state, 0, 1);
      }
      worker.postMessage({ type: "wake" });
      return;
    }
    if (message?.type === "ready") {
      status.textContent = `Neomacs browser frontend loaded (${message.strategy} Worker suspension)`;
      status.dataset.state = "ready";
      return;
    }
    if (message?.type === "failed") {
      showFailure(new Error(message.message));
    }
  };
  worker.onerror = (event) => showFailure(new Error(event.message));
  worker.postMessage({
    type: "start",
    wasmUrl: new URL("./neomacs_wasm_worker.wasm", import.meta.url).href,
    mailbox,
  });

  await init();
}

start().catch(showFailure);

const INPUT_WAKE = 1;
const TIMEOUT_WAKE = 2;
const RESUMED_INPUT = 0x4e450001;
const RESUMED_TIMEOUT = 0x4e450002;

let pendingJspiWake = null;

function post(type, payload = {}) {
  self.postMessage({ type, ...payload });
}

function supportsJspi() {
  return typeof WebAssembly.Suspending === "function"
    && typeof WebAssembly.promising === "function";
}

function createJspiWait() {
  return new WebAssembly.Suspending((timeoutMilliseconds) => new Promise((resolve) => {
    const timeout = setTimeout(() => {
      pendingJspiWake = null;
      resolve(TIMEOUT_WAKE);
    }, timeoutMilliseconds);
    pendingJspiWake = () => {
      clearTimeout(timeout);
      pendingJspiWake = null;
      resolve(INPUT_WAKE);
    };
    post("suspension-waiting");
  }));
}

function createAtomicsWait(mailbox) {
  if (!(mailbox instanceof SharedArrayBuffer)) {
    throw new Error("Atomics suspension requires a SharedArrayBuffer mailbox");
  }
  const state = new Int32Array(mailbox, 0, 1);
  return (timeoutMilliseconds) => {
    post("suspension-waiting");
    const result = Atomics.wait(state, 0, 0, timeoutMilliseconds);
    return result === "timed-out" ? TIMEOUT_WAKE : INPUT_WAKE;
  };
}

async function instantiateWorker(wasmUrl, mailbox) {
  const jspi = supportsJspi();
  const waitForInput = jspi ? createJspiWait() : createAtomicsWait(mailbox);
  const imports = { neomacs_host: { wait_for_input: waitForInput } };
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(`failed to fetch editor Worker Wasm: ${response.status} ${response.statusText}`);
  }
  const { instance } = await WebAssembly.instantiateStreaming(response, imports);
  const probe = instance.exports.neomacs_wasm_worker_probe;
  if (typeof probe !== "function") {
    throw new Error("editor Worker artifact does not export its suspension probe");
  }
  return {
    strategy: jspi ? "jspi" : "atomics",
    probe: jspi ? WebAssembly.promising(probe) : probe,
  };
}

self.onmessage = async (event) => {
  const message = event.data;
  if (message?.type === "wake") {
    pendingJspiWake?.();
    return;
  }
  if (message?.type !== "start") return;

  try {
    const worker = await instantiateWorker(message.wasmUrl, message.mailbox);
    const result = await worker.probe(5000);
    if (result !== RESUMED_INPUT && result !== RESUMED_TIMEOUT) {
      throw new Error(`editor Worker suspension resumed with invalid proof 0x${result.toString(16)}`);
    }
    post("ready", { strategy: worker.strategy });
  } catch (error) {
    post("failed", { message: error instanceof Error ? error.message : String(error) });
  }
};

import { fetchEditorWorkerAssets } from "./worker-assets.mjs";

const INPUT_WAKE = 1;
const TIMEOUT_WAKE = 2;
const RESUMED_INPUT = 0x4e450001;
const RESUMED_TIMEOUT = 0x4e450002;
const MAILBOX_HEADER_BYTES = 16;

const encoder = new TextEncoder();
const decoder = new TextDecoder();
let memory = null;
let runtimeImage = null;
let runtimeImageId = null;
let runtimeResourceBundle = null;
let runtimeResourceId = null;
let startup = null;
let mailbox = null;
let queuedInput = null;
let queuedInputSequence = null;
let pendingJspiWake = null;
let probing = true;

function post(type, payload = {}, transfer = []) {
  self.postMessage({ type, ...payload }, transfer);
}

function supportsJspi() {
  return typeof WebAssembly.Suspending === "function"
    && typeof WebAssembly.promising === "function";
}

function mailboxState() {
  return mailbox ? new Int32Array(mailbox, 0, 4) : null;
}

function mailboxInput() {
  if (!mailbox) return null;
  const state = mailboxState();
  if (Atomics.load(state, 0) !== 1) return null;
  const length = Atomics.load(state, 1);
  const capacity = mailbox.byteLength - MAILBOX_HEADER_BYTES;
  if (length <= 0 || length > capacity) return null;
  return new Uint8Array(mailbox, MAILBOX_HEADER_BYTES, length);
}

function currentInput() {
  return queuedInput ?? mailboxInput();
}

function createJspiWait() {
  return new WebAssembly.Suspending(async (timeoutMilliseconds) => {
    if (currentInput()) return INPUT_WAKE;
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        pendingJspiWake = null;
        resolve(TIMEOUT_WAKE);
      }, timeoutMilliseconds);
      pendingJspiWake = () => {
        clearTimeout(timeout);
        pendingJspiWake = null;
        resolve(INPUT_WAKE);
      };
      if (probing) post("probe-waiting");
    });
  });
}

function createAtomicsWait() {
  if (!(mailbox instanceof SharedArrayBuffer)) {
    throw new Error("Atomics suspension requires a SharedArrayBuffer mailbox");
  }
  const state = mailboxState();
  return (timeoutMilliseconds) => {
    if (Atomics.load(state, 0) === 1) return INPUT_WAKE;
    if (probing) post("probe-waiting");
    const result = Atomics.wait(state, 0, 0, timeoutMilliseconds);
    return result === "timed-out" ? TIMEOUT_WAKE : INPUT_WAKE;
  };
}

function copyToMemory(source, destination, capacity) {
  if (!memory || !source || source.byteLength > capacity) return 0;
  new Uint8Array(memory.buffer, destination, source.byteLength).set(source);
  return source.byteLength;
}

function decodeMemoryString(source, length) {
  return decoder.decode(new Uint8Array(memory.buffer, source, length));
}

function currentInputSequence() {
  if (queuedInput !== null) return queuedInputSequence;
  const input = mailboxInput();
  if (input === null) return null;
  try {
    const sequence = JSON.parse(decoder.decode(input))?.sequence;
    return typeof sequence === "string" ? sequence : null;
  } catch {
    return null;
  }
}

function acknowledgeInput(source, length) {
  const acknowledged = decodeMemoryString(source, length);
  if (currentInputSequence() !== acknowledged) return 0;
  queuedInput = null;
  queuedInputSequence = null;
  const state = mailboxState();
  if (state) {
    Atomics.store(state, 1, 0);
    Atomics.store(state, 0, 0);
  }
  post("input-accepted", { sequence: acknowledged });
  return 1;
}

function hostImports(waitForInput) {
  return {
    neomacs_host: {
      wait_for_input: waitForInput,
      startup_len: () => startup?.byteLength ?? 0,
      copy_startup: (destination, capacity) => copyToMemory(startup, destination, capacity),
      runtime_image_len: () => runtimeImage?.byteLength ?? 0,
      copy_runtime_image: (destination, capacity) => copyToMemory(runtimeImage, destination, capacity),
      runtime_image_id_len: () => runtimeImageId?.byteLength ?? 0,
      copy_runtime_image_id: (destination, capacity) =>
        copyToMemory(runtimeImageId, destination, capacity),
      runtime_resource_bundle_len: () => runtimeResourceBundle?.byteLength ?? 0,
      copy_runtime_resource_bundle: (destination, capacity) =>
        copyToMemory(runtimeResourceBundle, destination, capacity),
      runtime_resource_id_len: () => runtimeResourceId?.byteLength ?? 0,
      copy_runtime_resource_id: (destination, capacity) =>
        copyToMemory(runtimeResourceId, destination, capacity),
      input_len: () => currentInput()?.byteLength ?? 0,
      copy_input: (destination, capacity) => copyToMemory(currentInput(), destination, capacity),
      acknowledge_input: acknowledgeInput,
      publish_frame: (source, length) => {
        try {
          const payload = new Uint8Array(memory.buffer, source, length).slice().buffer;
          post("frame", { payload }, [payload]);
          return 1;
        } catch (error) {
          post("failed", { message: `failed to transfer editor frame: ${error}` });
          return 0;
        }
      },
      post_status: (source, length) => post("status", {
        message: decodeMemoryString(source, length),
      }),
      post_failure: (source, length) => post("failed", {
        message: decodeMemoryString(source, length),
      }),
    },
  };
}

async function instantiate(response, imports) {
  if (typeof WebAssembly.instantiateStreaming === "function") {
    try {
      return await WebAssembly.instantiateStreaming(response.clone(), imports);
    } catch (error) {
      if (response.headers.get("Content-Type") === "application/wasm") throw error;
    }
  }
  return WebAssembly.instantiate(await response.arrayBuffer(), imports);
}

async function start(message) {
  const jspi = supportsJspi();
  mailbox = message.mailbox;
  startup = encoder.encode(JSON.stringify(message.startup));
  const assets = await fetchEditorWorkerAssets(message);
  runtimeImage = assets.runtimeImage;
  runtimeImageId = assets.runtimeImageId;
  runtimeResourceBundle = assets.runtimeResourceBundle;
  runtimeResourceId = assets.runtimeResourceId;

  const waitForInput = jspi ? createJspiWait() : createAtomicsWait();
  const { instance } = await instantiate(assets.wasmResponse, hostImports(waitForInput));
  memory = instance.exports.memory;
  const probe = instance.exports.neomacs_wasm_worker_probe;
  const run = instance.exports.neomacs_wasm_worker_run;
  if (typeof probe !== "function" || typeof run !== "function") {
    throw new Error("editor Worker artifact is missing its controlled entry points");
  }

  const promisedProbe = jspi ? WebAssembly.promising(probe) : probe;
  const proof = await promisedProbe(5000);
  if (proof !== RESUMED_INPUT && proof !== RESUMED_TIMEOUT) {
    throw new Error(`editor Worker suspension resumed with invalid proof 0x${proof.toString(16)}`);
  }
  probing = false;
  const state = mailboxState();
  if (state) {
    Atomics.store(state, 1, 0);
    Atomics.store(state, 0, 0);
  }
  post("ready", { strategy: jspi ? "jspi" : "atomics" });

  const runEditor = jspi ? WebAssembly.promising(run) : run;
  const exitCode = await runEditor();
  post("exited", { exitCode });
}

self.onmessage = (event) => {
  const message = event.data;
  if (message?.type === "wake-probe") {
    pendingJspiWake?.();
    return;
  }
  if (message?.type === "input") {
    queuedInput = encoder.encode(JSON.stringify(message.batch));
    queuedInputSequence = typeof message.batch?.sequence === "string"
      ? message.batch.sequence
      : null;
    pendingJspiWake?.();
    return;
  }
  if (message?.type === "start") {
    start(message).catch((error) => {
      post("failed", { message: error instanceof Error ? error.message : String(error) });
    });
  }
};

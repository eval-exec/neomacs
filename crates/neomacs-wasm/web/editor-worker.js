import { fetchEditorWorkerAssets } from "./worker-assets.mjs";
import { fetchPackageAssets } from "./packages.mjs";
import { createHttpHostImports } from "./network/host.mjs";
import { createNavigationHostImports } from "./navigation.mjs";
import { WorkerWait, HostWake } from "./worker-wait.mjs";
import { WorkerInput } from "./worker-input.mjs";
import { openBlockingFileSystem } from "./storage/blocking.mjs";
import {
  OriginPrivateFileSystem,
  createOpfsHostImports,
} from "./opfs-storage.mjs";

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
let packageAssets = null;
let startup = null;
let mailbox = null;
const queuedInput = new WorkerInput(receipt => self.postMessage(receipt));
const workerWait = new WorkerWait(() => Boolean(currentInput()));
let probing = true;

function post(type, payload = {}, transfer = []) {
  self.postMessage({ type, ...payload }, transfer);
}

function phase(id, state) {
  post("startup-phase", { phase: id, state });
}

let runtimePhase = null;
function advanceRuntimePhase(id) {
  if (runtimePhase) phase(runtimePhase, "done");
  runtimePhase = id;
  phase(id, "active");
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
  return queuedInput.bytes() ?? mailboxInput();
}

function createJspiWait() {
  return new WebAssembly.Suspending(async (timeoutMilliseconds) => {
    const pending = workerWait.wait(timeoutMilliseconds);
    if (probing) post("probe-waiting");
    return pending;
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
  const input = mailboxInput();
  if (input === null) return null;
  try {
    // TextDecoder does not accept SharedArrayBuffer-backed views in Firefox.
    // The producer cannot reuse the mailbox until acknowledgement; decode a
    // local snapshot while that handshake guarantees its contents are stable.
    const sequence = JSON.parse(decoder.decode(input.slice()))?.sequence;
    return typeof sequence === "string" ? sequence : null;
  } catch {
    return null;
  }
}

function acknowledgeInput(source, length) {
  const acknowledged = decodeMemoryString(source, length);
  if (queuedInput.bytes() !== null) return Number(queuedInput.accept(acknowledged));
  if (currentInputSequence() !== acknowledged) return 0;
  const state = mailboxState();
  if (state) {
    Atomics.store(state, 1, 0);
    Atomics.store(state, 0, 0);
  }
  post("input-accepted", { sequence: acknowledged });
  return 1;
}

function rejectInput(source, length) {
  const message = decodeMemoryString(source, length);
  if (queuedInput.reject(message)) return;
  if (mailboxInput() === null) return;
  const sequence = currentInputSequence();
  const state = mailboxState();
  Atomics.store(state, 1, 0);
  Atomics.store(state, 0, 0);
  post("input-rejected", { sequence, message });
}

function hostImports(waitForInput, filesystemImports) {
  return {
    neomacs_host: {
      wait_for_input: waitForInput,
      monotonic_time_milliseconds: () => performance.now(),
      wall_time_milliseconds: () => Date.now(),
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
      package_bundle_len: () => packageAssets?.archive.byteLength ?? 0,
      copy_package_bundle: (destination, capacity) => copyToMemory(packageAssets?.archive, destination, capacity),
      package_id_len: () => packageAssets?.id.byteLength ?? 0,
      copy_package_id: (destination, capacity) => copyToMemory(packageAssets?.id, destination, capacity),
      input_len: () => currentInput()?.byteLength ?? 0,
      copy_input: (destination, capacity) => copyToMemory(currentInput(), destination, capacity),
      acknowledge_input: acknowledgeInput,
      reject_input: rejectInput,
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
      post_status: (source, length) => post("startup-detail", {
        phase: runtimePhase,
        message: decodeMemoryString(source, length),
      }),
      post_startup_phase: (source, length) => advanceRuntimePhase(decodeMemoryString(source, length)),
      post_failure: (source, length) => post("failed", {
        message: decodeMemoryString(source, length),
      }),
      ...filesystemImports,
      ...createHttpHostImports(() => memory, () => workerWait.notify()),
      ...createNavigationHostImports(() => memory, url => post("open-external-url", { url })),
    },
  };
}

function suspendingFilesystemImports(imports) {
  const suspending = new Set([
    "fs_stat",
    "fs_read",
    "fs_read_directory",
    "fs_write",
    "fs_create_directory",
    "fs_remove_file",
    "fs_remove_directory",
    "fs_rename",
    "fs_canonicalize",
  ]);
  return Object.fromEntries(Object.entries(imports).map(([name, implementation]) => [
    name,
    suspending.has(name) ? new WebAssembly.Suspending(implementation) : implementation,
  ]));
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
  phase("worker-start", "done");
  const jspi = supportsJspi();
  if (!jspi && !globalThis.crossOriginIsolated) {
    throw new Error(
      "neomacs-wasm requires JSPI or cross-origin isolation for browser storage",
    );
  }
  mailbox = message.mailbox;
  startup = encoder.encode(JSON.stringify(message.startup));
  // Downloading is the longest startup phase on a real link (about 88 MB on a
  // first visit) and the compiler streams from the same responses, so report
  // bytes as they arrive instead of leaving the user watching a still screen.
  phase("worker-download", "active");
  const assets = await fetchEditorWorkerAssets(message, undefined, (progress) => {
    post("progress", { phase: "download", ...progress });
  });
  runtimeImage = assets.runtimeImage;
  runtimeImageId = assets.runtimeImageId;
  runtimeResourceBundle = assets.runtimeResourceBundle;
  runtimeResourceId = assets.runtimeResourceId;
  phase("storage", "active");
  const filesystem = jspi
    ? await OriginPrivateFileSystem.open()
    : await openBlockingFileSystem();
  phase("storage", "done");
  const imports = createOpfsHostImports(filesystem, () => memory);
  const filesystemImports = jspi ? suspendingFilesystemImports(imports) : imports;

  const waitForInput = jspi ? createJspiWait() : createAtomicsWait();
  // `instantiateStreaming` compiles while the body is still arriving, so the
  // download progress above continues to advance during this phase.
  phase("worker-compile", "active");
  const { instance } = await instantiate(
    assets.wasmResponse,
    hostImports(waitForInput, filesystemImports),
  );
  memory = instance.exports.memory;
  phase("worker-compile", "done");
  const probe = instance.exports.neomacs_wasm_worker_probe;
  const run = instance.exports.neomacs_wasm_worker_run;
  if (typeof probe !== "function" || typeof run !== "function") {
    throw new Error("editor Worker artifact is missing its controlled entry points");
  }

  const promisedProbe = jspi ? WebAssembly.promising(probe) : probe;
  phase("worker-probe", "active");
  const proof = await promisedProbe(5000);
  if (proof !== RESUMED_INPUT && proof !== RESUMED_TIMEOUT) {
    throw new Error(`editor Worker suspension resumed with invalid proof 0x${proof.toString(16)}`);
  }
  probing = false;
  phase("worker-probe", "done");
  phase("packages", "active");
  try {
    packageAssets = await fetchPackageAssets(new URL("./packages.json", import.meta.url), {
      onProgress: progress => post("progress", {phase: "packages", ...progress}),
      onDetail: message => post("startup-detail", {phase: "packages", message}),
    });
  } catch (error) {
    post("startup-detail", {phase: "packages", message:
      `Optional packages unavailable: ${error.message}. Starting basic landing page; reload to retry.`});
  }
  phase("packages", "done");
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
    // The isolated startup probe intentionally simulates an input wake.
    workerWait.notify(HostWake.Input);
    return;
  }
  if (message?.type === "input") {
    try {
      queuedInput.enqueue(message.batch);
    } catch (error) {
      post("failed", { message: String(error) });
      return;
    }
    workerWait.notify();
    return;
  }
  if (message?.type === "start") {
    start(message).catch((error) => {
      post("failed", {
        message: error instanceof Error ? (error.stack ?? error.message) : String(error),
      });
    });
  }
};

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

  await init();
  status.textContent = "Neomacs browser frontend loaded";
  status.dataset.state = "ready";
}

start().catch(showFailure);

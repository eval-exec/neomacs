export async function initializeWasmFrontend(initialize, moduleUrl) {
  await initialize({ module_or_path: moduleUrl });
}

export function observeFirstEditorPresentation(waitForPresentation, onReady, onFailure) {
  return waitForPresentation().then(onReady, onFailure);
}

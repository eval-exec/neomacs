export async function initializeWasmFrontend(initialize, moduleUrl) {
  await initialize({ module_or_path: moduleUrl });
}

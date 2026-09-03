const assetSpecs = [
  ["wasmResponse", "wasmUrl", "editor Worker Wasm"],
  ["runtimeImage", "runtimeImageUrl", "portable runtime image"],
  ["runtimeImageId", "runtimeImageIdUrl", "portable runtime image ID"],
  ["runtimeResourceBundle", "runtimeResourceBundleUrl", "runtime resource bundle"],
  ["runtimeResourceId", "runtimeResourceIdUrl", "runtime resource bundle ID"],
];

/** Fetch the complete immutable input set for one editor Worker instance. */
export async function fetchEditorWorkerAssets(message, fetchAsset = globalThis.fetch) {
  const responses = await Promise.all(
    assetSpecs.map(([, urlField]) => fetchAsset(message[urlField])),
  );
  for (let index = 0; index < assetSpecs.length; index += 1) {
    const [, , description] = assetSpecs[index];
    const response = responses[index];
    if (!response?.ok) {
      throw new Error(`failed to fetch ${description}: ${response?.status ?? "no response"}`);
    }
  }

  const assets = { wasmResponse: responses[0] };
  await Promise.all(
    assetSpecs.slice(1).map(async ([resultField], index) => {
      assets[resultField] = new Uint8Array(await responses[index + 1].arrayBuffer());
    }),
  );
  return assets;
}

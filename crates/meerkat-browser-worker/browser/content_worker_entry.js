export async function bootContentWorker(loadWasmModule) {
  const exports = await loadWasmModule();
  if (typeof exports?.install_content_worker !== "function") {
    throw new Error(
      "loadWasmModule() must resolve to wasm exports with install_content_worker()",
    );
  }
  return exports.install_content_worker();
}

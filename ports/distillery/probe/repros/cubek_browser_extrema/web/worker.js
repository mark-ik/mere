import init, { run_extrema_repro } from "./pkg/cubek_browser_extrema_repro.js";

const gpuErrors = [];

if (navigator.gpu?.requestAdapter) {
  const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
  navigator.gpu.requestAdapter = async (...adapterArgs) => {
    const adapter = await requestAdapter(...adapterArgs);
    if (!adapter) return adapter;
    const requestDevice = adapter.requestDevice.bind(adapter);
    adapter.requestDevice = async (...deviceArgs) => {
      const device = await requestDevice(...deviceArgs);
      const popErrorScope = device.popErrorScope.bind(device);
      device.popErrorScope = () => popErrorScope().then((error) => {
        if (error) {
          gpuErrors.push({
            error_type: error.constructor?.name ?? "GPUError",
            message: error.message ?? String(error),
          });
        }
        return error;
      });
      return device;
    };
    return adapter;
  };
}

self.addEventListener("message", async (event) => {
  if (event.data?.command !== "run") return;
  try {
    await init();
    const result = JSON.parse(await run_extrema_repro());
    self.postMessage({ kind: "result", result, gpu_errors: gpuErrors });
  } catch (error) {
    self.postMessage({
      kind: "error",
      error: String(error),
      stack: error?.stack ?? null,
      gpu_errors: gpuErrors,
    });
  }
});

// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import init, { run_embedding_repro } from "./pkg/burn_browser_embedding_repro.js";

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
    const result = JSON.parse(await run_embedding_repro());
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

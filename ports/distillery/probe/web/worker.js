// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import init, { request_cancel, run_probe } from "./pkg/distillery_model_probe.js";

let initialized;
const trackedDevices = new Set();

// wgpu 30's WebGPU backend currently panics while classifying an internal
// browser GPU error. Preserve the browser's original error type and message in
// the probe stream before handing it back to wgpu unchanged.
if (navigator.gpu?.requestAdapter) {
  const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
  navigator.gpu.requestAdapter = async (...adapterArgs) => {
    const adapter = await requestAdapter(...adapterArgs);
    if (!adapter) return adapter;
    const requestDevice = adapter.requestDevice.bind(adapter);
    adapter.requestDevice = async (...deviceArgs) => {
      const device = await requestDevice(...deviceArgs);
      trackedDevices.add(device);
      const popErrorScope = device.popErrorScope.bind(device);
      device.popErrorScope = () => popErrorScope().then((error) => {
        if (error) {
          self.postMessage({
            kind: "gpu_error",
            error_type: error.constructor?.name ?? "GPUError",
            error: error.message ?? String(error),
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
  if (event.data?.command === "cancel") {
    request_cancel();
    self.postMessage({ kind: "cancel_ack" });
    return;
  }
  if (event.data?.command === "destroy_devices") {
    const errors = [];
    const count = trackedDevices.size;
    for (const device of trackedDevices) {
      try {
        device.destroy();
      } catch (error) {
        errors.push(String(error));
      }
    }
    trackedDevices.clear();
    self.postMessage({ kind: "device_teardown", count, errors });
    return;
  }
  if (event.data?.command !== "run") return;
  try {
    initialized ??= init();
    await initialized;
    const report = JSON.parse(await run_probe(JSON.stringify(event.data.config)));
    self.postMessage({ kind: "result", report });
  } catch (error) {
    self.postMessage({
      kind: "error",
      error: String(error),
      stack: error?.stack ?? null,
    });
  }
});

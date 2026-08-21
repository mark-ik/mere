const controls = document.getElementById("probe-controls");
const status = document.getElementById("status");
const stateLog = document.getElementById("state-log");
const receiptView = document.getElementById("receipt");
const downloadButton = document.getElementById("download-receipt");

let activeWorker = null;
let activeRun = null;
let receipt = null;
let framePhase = null;
let lastFrame = null;
const frameSamples = new Map();

function setState(state, text) {
  document.body.dataset.probeState = state;
  status.textContent = text;
}

function logState(run, state, detail) {
  const item = document.createElement("li");
  item.textContent = `${run}: ${state} · ${detail}`;
  stateLog.append(item);
  stateLog.scrollTop = stateLog.scrollHeight;
}

function beginFrames(phase) {
  framePhase = phase;
  lastFrame = null;
  frameSamples.set(phase, []);
}

function endFrames(phase, bound) {
  if (framePhase === phase) framePhase = null;
  return summarizeFrames(frameSamples.get(phase) ?? [], bound);
}

function frameLoop(now) {
  if (framePhase) {
    if (lastFrame !== null) frameSamples.get(framePhase).push(now - lastFrame);
    lastFrame = now;
  }
  requestAnimationFrame(frameLoop);
}
requestAnimationFrame(frameLoop);

function percentile(sorted, fraction) {
  if (sorted.length === 0) return null;
  const index = Math.min(sorted.length - 1, Math.floor(sorted.length * fraction));
  return sorted[index];
}

function summarizeFrames(samples, bound) {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    count: sorted.length,
    p50_ms: percentile(sorted, 0.5),
    p95_ms: percentile(sorted, 0.95),
    max_ms: sorted.at(-1) ?? null,
    configured_bound_ms: bound,
    over_bound: sorted.filter((sample) => sample > bound).length,
  };
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function storageSnapshot() {
  if (!navigator.storage) return { state: "unknown", reason: "StorageManager unavailable" };
  const estimate = await navigator.storage.estimate();
  const persisted = await navigator.storage.persisted();
  return {
    state: persisted ? "persistent" : "best_effort",
    quota_bytes: estimate.quota ?? null,
    usage_bytes: estimate.usage ?? null,
  };
}

async function memorySnapshot() {
  if (typeof performance.measureUserAgentSpecificMemory !== "function") {
    return { state: "unknown", reason: "measureUserAgentSpecificMemory unavailable" };
  }
  try {
    const measurement = await performance.measureUserAgentSpecificMemory();
    return { state: "reported", bytes: measurement.bytes };
  } catch (error) {
    return { state: "unknown", reason: String(error) };
  }
}

async function browserAdapter() {
  if (!navigator.gpu) return { state: "unsupported" };
  try {
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) return { state: "unavailable" };
    const info = adapter.info ?? {};
    return {
      state: "available",
      vendor: info.vendor || null,
      architecture: info.architecture || null,
      device: info.device || null,
      description: info.description || null,
      note: "Browser adapter sample; Burn's worker-internal adapter identity is not exposed.",
    };
  } catch (error) {
    return { state: "failed", error: String(error) };
  }
}

async function wasmBundle() {
  try {
    const response = await fetch("./pkg/distillery_model_probe_bg.wasm", { method: "HEAD" });
    return {
      url: response.url,
      bytes: Number(response.headers.get("content-length")) || null,
    };
  } catch (error) {
    return { url: null, bytes: null, error: String(error) };
  }
}

function configuredInput() {
  return {
    model_base_url: document.getElementById("model-base-url").value.trim(),
    model_id: document.getElementById("model-id").value.trim(),
    architecture: "bert",
    license: "Apache-2.0",
    input: document.getElementById("probe-input").value,
    run_count: Number(document.getElementById("run-count").value),
  };
}

function parseWorkerMessage(data) {
  if (typeof data === "string") {
    try { return JSON.parse(data); } catch { return { kind: "unknown", data }; }
  }
  return data;
}

function runWorker(label, config, { cancelAtState = null, frameBound }) {
  if (activeWorker) throw new Error("another probe worker is active");
  return new Promise((resolve, reject) => {
    const worker = new Worker("./worker.js", { type: "module", name: `distillery-${label}` });
    activeWorker = worker;
    activeRun = label;
    beginFrames(label);
    let settled = false;
    let terminationRequestedAt = null;
    let lateMessages = 0;
    const gpuErrors = [];

    const finish = (value, error = null) => {
      if (settled) return;
      settled = true;
      activeWorker = null;
      activeRun = null;
      const frames = endFrames(label, frameBound);
      if (error) reject(Object.assign(error, { frames, gpuErrors }));
      else resolve({ ...value, frames, gpu_errors: gpuErrors });
    };

    worker.addEventListener("message", async (event) => {
      const message = parseWorkerMessage(event.data);
      if (settled) {
        lateMessages += 1;
        return;
      }
      if (message.kind === "state") {
        logState(label, message.state, message.detail);
        setState(message.state, `${label}: ${message.detail}`);
        if (cancelAtState === message.state) {
          terminationRequestedAt = performance.now();
          worker.terminate();
          const terminationCallMs = performance.now() - terminationRequestedAt;
          activeWorker = null;
          activeRun = null;
          await delay(300);
          finish({
            canceled: true,
            canceled_at_state: message.state,
            termination_call_ms: terminationCallMs,
            quiet_window_ms: 300,
            late_messages: lateMessages,
            cooperative_model_cancel: false,
            note: "This receipt proves worker termination and message cutoff, not ESP decoder cancellation or GPU-memory release.",
          });
        }
        return;
      }
      if (message.kind === "gpu_error") {
        gpuErrors.push({ type: message.error_type, message: message.error });
        logState(label, message.error_type, message.error);
        return;
      }
      if (message.kind === "result") {
        worker.terminate();
        finish({ report: message.report });
        return;
      }
      if (message.kind === "error") {
        worker.terminate();
        const details = [message.error, message.stack].filter(Boolean).join("\n");
        finish(null, new Error(details));
      }
    });

    worker.addEventListener("error", (event) => {
      worker.terminate();
      const location = event.filename
        ? `${event.filename}:${event.lineno ?? "?"}:${event.colno ?? "?"}`
        : null;
      const details = [
        event.message || "worker error",
        location,
        event.error?.stack ?? null,
      ].filter(Boolean).join("\n");
      finish(null, new Error(details));
    });

    worker.postMessage({ command: "run", config });
  });
}

function terminateActive(reason = "owner requested termination") {
  if (!activeWorker) return false;
  activeWorker.terminate();
  logState(activeRun ?? "worker", "terminated", reason);
  activeWorker = null;
  activeRun = null;
  framePhase = null;
  setState("terminated", reason);
  return true;
}

function showReceipt() {
  receiptView.textContent = JSON.stringify(receipt, null, 2);
  downloadButton.disabled = !receipt;
}

async function runSuite() {
  stateLog.replaceChildren();
  receipt = null;
  showReceipt();
  const input = configuredInput();
  const idleMs = Number(document.getElementById("idle-ms").value);
  const frameBound = Number(document.getElementById("frame-bound").value);
  if (!input.model_base_url || !input.model_id || !input.input || input.run_count < 1) {
    throw new Error("model URL, identity, input, and a positive execution count are required");
  }

  setState("sampling_idle", "Measuring the idle animation baseline.");
  beginFrames("idle");
  await delay(idleMs);
  const idleFrames = endFrames("idle", frameBound);

  const storageBefore = await storageSnapshot();
  let persistenceRequest = { requested: false };
  if (document.getElementById("request-persistence").checked && navigator.storage?.persist) {
    persistenceRequest = {
      requested: true,
      granted: await navigator.storage.persist(),
    };
  }

  const environment = {
    user_agent: navigator.userAgent,
    platform: navigator.userAgentData?.platform ?? navigator.platform ?? "unknown",
    hardware_concurrency: navigator.hardwareConcurrency ?? null,
    device_memory_gib: navigator.deviceMemory ?? null,
    cross_origin_isolated: crossOriginIsolated,
    adapter: await browserAdapter(),
    wasm_bundle: await wasmBundle(),
    memory_before: await memorySnapshot(),
  };

  const cold = await runWorker("cold", {
    ...input,
    mode: "cold",
    manifest_id: null,
    expected_hashes: null,
    expected_output_hash: null,
  }, { frameBound });

  const expected = cold.report;
  const cancellation = await runWorker("cancel", {
    ...input,
    mode: "warm",
    manifest_id: expected.manifest_id,
    expected_hashes: expected.component_hashes,
    expected_output_hash: expected.execution.output_hash,
  }, { cancelAtState: "executing", frameBound });

  const warm = await runWorker("warm", {
    ...input,
    mode: "warm",
    manifest_id: expected.manifest_id,
    expected_hashes: expected.component_hashes,
    expected_output_hash: expected.execution.output_hash,
  }, { frameBound });

  const storageAfter = await storageSnapshot();
  environment.memory_after = await memorySnapshot();
  const outputNumericallyValid = cold.report.execution.all_finite
    && warm.report.execution.all_finite
    && Math.abs(cold.report.execution.l2_norm - 1) < 0.001
    && Math.abs(warm.report.execution.l2_norm - 1) < 0.001;
  const referenceFixtureMatch = cold.report.execution.reference_within_tolerance === true
    && warm.report.execution.reference_within_tolerance === true;
  const gpuValidationErrorsObserved = cold.gpu_errors.length
    + cancellation.gpu_errors.length
    + warm.gpu_errors.length > 0;
  receipt = {
    schema: "distillery.browser-model-probe/v1",
    generated_at: new Date().toISOString(),
    configuration: {
      ...input,
      idle_sample_ms: idleMs,
      frame_bound_ms: frameBound,
      persistence_request: persistenceRequest,
    },
    environment,
    storage: { before: storageBefore, after: storageAfter },
    frames: { idle: idleFrames, cold: cold.frames, cancellation: cancellation.frames, warm: warm.frames },
    gpu_errors: {
      cold: cold.gpu_errors,
      cancellation: cancellation.gpu_errors,
      warm: warm.gpu_errors,
    },
    cold: cold.report,
    cancellation: { ...cancellation, frames: undefined },
    warm: warm.report,
    conclusions: {
      artifact_reopened: warm.report.manifest_id === cold.report.manifest_id,
      integrity_reopened: warm.report.integrity_matches,
      output_repeated_across_workers: warm.report.execution.matches_prior_worker,
      output_numerically_valid: outputNumericallyValid,
      reference_fixture_match: referenceFixtureMatch,
      gpu_validation_errors_observed: gpuValidationErrorsObserved,
      worker_termination_quiet: cancellation.late_messages === 0,
      execution_kind: "sentence_embedding",
      decoder_streaming_cancel_measured: false,
      limiting_layer: outputNumericallyValid && referenceFixtureMatch
        ? "unmeasured above this one-model embedding row"
        : "Burn/CubeCL BrowserWebGpu execution at the one-model embedding row",
    },
  };
  showReceipt();
  if (outputNumericallyValid && referenceFixtureMatch) {
    setState("complete", "Cold store, termination, warm reopen, and WGPU embedding completed.");
  } else {
    setState(
      "limited",
      "Artifact and worker lifecycle passed; WGPU output failed numerical validation.",
    );
  }
  return receipt;
}

controls.addEventListener("submit", async (event) => {
  event.preventDefault();
  document.getElementById("run-suite").disabled = true;
  try {
    await runSuite();
  } catch (error) {
    receipt = {
      schema: "distillery.browser-model-probe/v1",
      generated_at: new Date().toISOString(),
      failed: true,
      error: String(error),
    };
    showReceipt();
    setState("failed", String(error));
  } finally {
    document.getElementById("run-suite").disabled = false;
  }
});

document.getElementById("cancel-active").addEventListener("click", () => {
  terminateActive();
});

downloadButton.addEventListener("click", () => {
  if (!receipt) return;
  const blob = new Blob([JSON.stringify(receipt, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = "browser-model-probe.json";
  link.click();
  URL.revokeObjectURL(link.href);
});

window.distilleryModelProbe = {
  runSuite,
  terminateActive,
  receipt: () => structuredClone(receipt),
};

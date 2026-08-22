const controls = document.getElementById("probe-controls");
const status = document.getElementById("status");
const stateLog = document.getElementById("state-log");
const receiptView = document.getElementById("receipt");
const downloadButton = document.getElementById("download-receipt");
const matrixButton = document.getElementById("run-matrix");
const runButton = document.getElementById("run-suite");
const modelSelection = document.getElementById("model-selection");
const modelMetadata = document.getElementById("model-metadata");

let activeWorker = null;
let activeRun = null;
let receipt = null;
let matrixConfiguration = null;
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

function formatBytes(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

async function loadMatrix() {
  const response = await fetch("../model-matrix.json", { cache: "no-store" });
  if (!response.ok) throw new Error(`model matrix: HTTP ${response.status}`);
  const parsed = await response.json();
  if (parsed.schema !== "distillery.browser-model-matrix/v1" || !parsed.models?.length) {
    throw new Error("model matrix had the wrong schema or no rows");
  }
  matrixConfiguration = parsed;
  modelSelection.replaceChildren();
  for (const model of parsed.models) {
    const option = document.createElement("option");
    option.value = model.model_id;
    option.textContent = `${model.model_id} · ${formatBytes(model.artifacts.weights.bytes)} ${model.artifacts.weights.dtype}`;
    modelSelection.append(option);
  }
  const preferred = parsed.models.find((model) => model.model_id.includes("all-MiniLM-L6-v2"))
    ?? parsed.models[0];
  modelSelection.value = preferred.model_id;
  applyModel(preferred);
  modelSelection.disabled = false;
  matrixButton.disabled = false;
  return parsed;
}

function selectedModel() {
  return matrixConfiguration?.models.find((model) => model.model_id === modelSelection.value) ?? null;
}

function applyModel(model) {
  document.getElementById("model-base-url").value = model.model_base_url;
  document.getElementById("model-id").value = model.model_id;
  document.getElementById("probe-input").value = matrixConfiguration.input;
  document.getElementById("run-count").value = matrixConfiguration.run_count;
  document.getElementById("idle-ms").value = matrixConfiguration.idle_sample_ms;
  document.getElementById("frame-bound").value = matrixConfiguration.frame_bound_ms;
  modelMetadata.textContent = [
    model.revision.slice(0, 12),
    `${model.expected_dimensions} dimensions`,
    `${formatBytes(model.artifacts.weights.bytes)} ${model.artifacts.weights.dtype}`,
    model.license,
  ].join(" · ");
}

async function resolveModel(selection) {
  await matrixReady;
  if (selection && typeof selection === "object") return selection;
  if (typeof selection === "string") {
    const match = matrixConfiguration.models.find((model) => model.model_id === selection);
    if (!match) throw new Error(`unknown configured model: ${selection}`);
    return match;
  }
  return selectedModel();
}

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

function configuredInput(model) {
  const modelBaseUrl = document.getElementById("model-base-url").value.trim();
  const modelId = document.getElementById("model-id").value.trim();
  const matchesConfiguredRow = model
    && model.model_base_url === modelBaseUrl
    && model.model_id === modelId;
  return {
    model_base_url: modelBaseUrl,
    model_id: modelId,
    architecture: matchesConfiguredRow ? model.architecture : "bert",
    license: matchesConfiguredRow ? model.license : "unknown",
    input: document.getElementById("probe-input").value,
    run_count: Number(document.getElementById("run-count").value),
    expected_dimensions: matchesConfiguredRow ? model.expected_dimensions : null,
    reference_first_8: matchesConfiguredRow ? model.reference?.first_8 ?? null : null,
    reference_tolerance: matchesConfiguredRow ? model.reference?.tolerance ?? null : null,
    diagnostic_trace: matchesConfiguredRow ? matrixConfiguration.diagnostic_trace === true : false,
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
    let lateMessages = 0;
    let lastState = "worker_created";
    const stateHistory = [];
    const gpuErrors = [];

    const finish = (value, error = null) => {
      if (settled) return;
      settled = true;
      activeWorker = null;
      activeRun = null;
      const frames = endFrames(label, frameBound);
      if (error) {
        error.probe = {
          label,
          last_state: lastState,
          state_history: stateHistory,
          frames,
          gpu_errors: gpuErrors,
        };
        reject(error);
      } else {
        resolve({ ...value, frames, gpu_errors: gpuErrors, state_history: stateHistory });
      }
    };

    worker.addEventListener("message", async (event) => {
      const message = parseWorkerMessage(event.data);
      if (settled) {
        lateMessages += 1;
        return;
      }
      if (message.kind === "state") {
        lastState = message.state;
        stateHistory.push({ state: message.state, detail: message.detail });
        logState(label, message.state, message.detail);
        setState(message.state, `${label}: ${message.detail}`);
        if (cancelAtState === message.state) {
          const terminationRequestedAt = performance.now();
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
  receiptView.textContent = receipt ? JSON.stringify(receipt, null, 2) : "No run yet.";
  downloadButton.disabled = !receipt;
}

function classifyFailure(state) {
  if (["acquiring", "storing", "reopening", "verifying"].includes(state)) {
    return "artifact_storage_or_integrity";
  }
  if (["loading", "ready"].includes(state)) return "esp_loader_or_wgpu_upload";
  if (["executing", "tracing"].includes(state)) return "burn_browserwebgpu_execution";
  return "browser_worker_or_probe";
}

async function environmentSnapshot() {
  return {
    user_agent: navigator.userAgent,
    platform: navigator.userAgentData?.platform ?? navigator.platform ?? "unknown",
    hardware_concurrency: navigator.hardwareConcurrency ?? null,
    device_memory_gib: navigator.deviceMemory ?? null,
    cross_origin_isolated: crossOriginIsolated,
    adapter: await browserAdapter(),
    wasm_bundle: await wasmBundle(),
    memory_before: await memorySnapshot(),
  };
}

async function runConfiguredRow(model, rowIndex = null) {
  const input = configuredInput(model);
  const idleMs = Number(document.getElementById("idle-ms").value);
  const frameBound = Number(document.getElementById("frame-bound").value);
  if (!input.model_base_url || !input.model_id || !input.input || input.run_count < 1) {
    throw new Error("model URL, identity, input, and a positive execution count are required");
  }

  const prefix = rowIndex === null ? model.model_id : `row-${rowIndex + 1}:${model.model_id}`;
  setState("sampling_idle", `${prefix}: measuring the idle animation baseline.`);
  beginFrames(`${prefix}:idle`);
  await delay(idleMs);
  const idleFrames = endFrames(`${prefix}:idle`, frameBound);

  const storageBefore = await storageSnapshot();
  let persistenceRequest = { requested: false };
  if (document.getElementById("request-persistence").checked && navigator.storage?.persist) {
    persistenceRequest = {
      requested: true,
      granted: await navigator.storage.persist(),
    };
  }
  const environment = await environmentSnapshot();
  const configuration = {
    ...input,
    model_revision: model.revision,
    pooling: model.pooling,
    expected_artifacts: model.artifacts,
    reference_source: model.reference?.source ?? null,
    idle_sample_ms: idleMs,
    frame_bound_ms: frameBound,
    persistence_request: persistenceRequest,
  };

  try {
    const cold = await runWorker(`${prefix}:cold`, {
      ...input,
      mode: "cold",
      manifest_id: null,
      expected_hashes: null,
      expected_output_hash: null,
    }, { frameBound });
    const expected = cold.report;
    const cancellation = await runWorker(`${prefix}:cancel`, {
      ...input,
      mode: "warm",
      manifest_id: expected.manifest_id,
      expected_hashes: expected.component_hashes,
      expected_output_hash: expected.execution.output_hash,
    }, { cancelAtState: "executing", frameBound });
    const warm = await runWorker(`${prefix}:warm`, {
      ...input,
      mode: "warm",
      manifest_id: expected.manifest_id,
      expected_hashes: expected.component_hashes,
      expected_output_hash: expected.execution.output_hash,
    }, { frameBound });

    const storageAfter = await storageSnapshot();
    environment.memory_after = await memorySnapshot();
    const dimensionsMatch = cold.report.execution.dimensions_match !== false
      && warm.report.execution.dimensions_match !== false;
    const outputNumericallyValid = dimensionsMatch
      && cold.report.execution.all_finite
      && warm.report.execution.all_finite
      && Math.abs(cold.report.execution.l2_norm - 1) < 0.001
      && Math.abs(warm.report.execution.l2_norm - 1) < 0.001;
    const referenceFixtureMatch = cold.report.execution.reference_within_tolerance === true
      && warm.report.execution.reference_within_tolerance === true;
    const gpuValidationErrorsObserved = cold.gpu_errors.length
      + cancellation.gpu_errors.length
      + warm.gpu_errors.length > 0;
    const rowPassed = outputNumericallyValid
      && referenceFixtureMatch
      && !gpuValidationErrorsObserved
      && cancellation.late_messages === 0
      && warm.report.integrity_matches;
    return {
      schema: "distillery.browser-model-probe/v2",
      generated_at: new Date().toISOString(),
      configuration,
      environment,
      storage: { before: storageBefore, after: storageAfter },
      frames: {
        idle: idleFrames,
        cold: cold.frames,
        cancellation: cancellation.frames,
        warm: warm.frames,
      },
      gpu_errors: {
        cold: cold.gpu_errors,
        cancellation: cancellation.gpu_errors,
        warm: warm.gpu_errors,
      },
      cold: cold.report,
      cancellation: { ...cancellation, frames: undefined, gpu_errors: undefined },
      warm: warm.report,
      conclusions: {
        row_passed: rowPassed,
        artifact_reopened: warm.report.manifest_id === cold.report.manifest_id,
        integrity_reopened: warm.report.integrity_matches,
        output_repeated_across_workers: warm.report.execution.matches_prior_worker,
        output_dimensions_match: dimensionsMatch,
        output_numerically_valid: outputNumericallyValid,
        reference_fixture_match: referenceFixtureMatch,
        gpu_validation_errors_observed: gpuValidationErrorsObserved,
        worker_termination_quiet: cancellation.late_messages === 0,
        execution_kind: "sentence_embedding",
        decoder_streaming_cancel_measured: false,
        limiting_layer: rowPassed
          ? "unmeasured above this configured embedding row"
          : "configured BrowserWebGpu embedding row",
      },
    };
  } catch (error) {
    const probe = error.probe ?? {};
    const storageAfter = await storageSnapshot();
    environment.memory_after = await memorySnapshot();
    const phase = probe.label?.split(":").at(-1) ?? "unknown";
    return {
      schema: "distillery.browser-model-probe/v2",
      generated_at: new Date().toISOString(),
      configuration,
      environment,
      storage: { before: storageBefore, after: storageAfter },
      failed: true,
      failure: {
        run: probe.label ?? prefix,
        phase,
        last_state: probe.last_state ?? "unknown",
        state_history: probe.state_history ?? [],
        error: String(error),
      },
      frames: {
        idle: idleFrames,
        [phase]: probe.frames ?? null,
      },
      gpu_errors: {
        [phase]: probe.gpu_errors ?? [],
      },
      conclusions: {
        row_passed: false,
        limiting_layer: classifyFailure(probe.last_state),
      },
    };
  }
}

function rowPassed(row) {
  return row?.conclusions?.row_passed === true;
}

async function runSuite(selection = null) {
  const model = await resolveModel(selection);
  stateLog.replaceChildren();
  receipt = null;
  showReceipt();
  receipt = await runConfiguredRow(model);
  showReceipt();
  if (rowPassed(receipt)) {
    setState("complete", `${model.model_id}: cold store, termination, warm reopen, and numerical WGPU validation passed.`);
  } else {
    setState("limited", `${model.model_id}: stopped at ${receipt.conclusions.limiting_layer}.`);
  }
  return receipt;
}

async function runMatrix() {
  await matrixReady;
  stateLog.replaceChildren();
  receipt = null;
  showReceipt();
  const rows = [];
  for (const [index, model] of matrixConfiguration.models.entries()) {
    modelSelection.value = model.model_id;
    applyModel(model);
    const row = await runConfiguredRow(model, index);
    rows.push(row);
    receipt = {
      schema: "distillery.browser-model-matrix-receipt/v1",
      generated_at: new Date().toISOString(),
      configured_matrix_schema: matrixConfiguration.schema,
      rows,
      partial: true,
    };
    showReceipt();
    if (!rowPassed(row)) break;
  }
  const successes = rows.filter(rowPassed);
  const firstLimitIndex = rows.findIndex((row) => !rowPassed(row));
  const largestSuccess = successes.reduce((largest, row) => {
    if (!largest) return row;
    return row.configuration.expected_artifacts.weights.bytes
      > largest.configuration.expected_artifacts.weights.bytes ? row : largest;
  }, null);
  receipt = {
    schema: "distillery.browser-model-matrix-receipt/v1",
    generated_at: new Date().toISOString(),
    configured_matrix_schema: matrixConfiguration.schema,
    configured_input: matrixConfiguration.input,
    configured_row_count: matrixConfiguration.models.length,
    attempted_row_count: rows.length,
    rows,
    conclusions: {
      successful_rows: successes.length,
      all_configured_rows_passed: rows.length === matrixConfiguration.models.length
        && successes.length === rows.length,
      first_limit_row: firstLimitIndex >= 0
        ? rows[firstLimitIndex].configuration.model_id
        : null,
      first_limit_layer: firstLimitIndex >= 0
        ? rows[firstLimitIndex].conclusions.limiting_layer
        : null,
      largest_successful_model: largestSuccess?.configuration.model_id ?? null,
      largest_successful_weight_bytes: largestSuccess?.configuration.expected_artifacts.weights.bytes ?? null,
      upper_boundary: firstLimitIndex >= 0
        ? "bounded by the first configured limit"
        : "unmeasured above the configured matrix",
    },
  };
  showReceipt();
  if (receipt.conclusions.all_configured_rows_passed) {
    setState("complete", "Every configured D2c model row passed; the upper boundary remains above this matrix.");
  } else {
    setState("limited", `D2c stopped at ${receipt.conclusions.first_limit_row}: ${receipt.conclusions.first_limit_layer}.`);
  }
  return receipt;
}

function setRunControlsDisabled(disabled) {
  runButton.disabled = disabled;
  matrixButton.disabled = disabled;
  modelSelection.disabled = disabled;
}

controls.addEventListener("submit", async (event) => {
  event.preventDefault();
  setRunControlsDisabled(true);
  try {
    await runSuite();
  } catch (error) {
    receipt = {
      schema: "distillery.browser-model-probe/v2",
      generated_at: new Date().toISOString(),
      failed: true,
      error: String(error),
    };
    showReceipt();
    setState("failed", String(error));
  } finally {
    setRunControlsDisabled(false);
  }
});

matrixButton.addEventListener("click", async () => {
  setRunControlsDisabled(true);
  try {
    await runMatrix();
  } catch (error) {
    receipt = {
      schema: "distillery.browser-model-matrix-receipt/v1",
      generated_at: new Date().toISOString(),
      failed: true,
      error: String(error),
    };
    showReceipt();
    setState("failed", String(error));
  } finally {
    setRunControlsDisabled(false);
  }
});

modelSelection.addEventListener("change", () => {
  const model = selectedModel();
  if (model) applyModel(model);
});

document.getElementById("cancel-active").addEventListener("click", () => {
  terminateActive();
});

downloadButton.addEventListener("click", () => {
  if (!receipt) return;
  const blob = new Blob([JSON.stringify(receipt, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = receipt.schema.includes("matrix")
    ? "browser-model-matrix.json"
    : "browser-model-probe.json";
  link.click();
  URL.revokeObjectURL(link.href);
});

const matrixReady = loadMatrix().catch((error) => {
  setState("failed", String(error));
  throw error;
});

window.distilleryModelProbe = {
  ready: matrixReady,
  runSuite,
  runMatrix,
  terminateActive,
  matrix: async () => structuredClone(await matrixReady),
  receipt: () => structuredClone(receipt),
};

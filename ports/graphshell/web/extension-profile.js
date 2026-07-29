const extensionApi = globalThis.browser ?? globalThis.chrome;
const model = globalThis.GraphshellCaptureModel;
const panel = document.querySelector("#capture-panel");
const statusNode = document.querySelector("#capture-status");
const rangeNode = document.querySelector("#capture-range");
const stripQueryNode = document.querySelector("#capture-strip-query");
const exclusionsNode = document.querySelector("#capture-exclusions");
const enableButton = document.querySelector("#capture-enable");
const saveButton = document.querySelector("#capture-save");
const disableButton = document.querySelector("#capture-disable");
const historyRangeNode = document.querySelector("#history-filter-range");
const historyPersonaNode = document.querySelector("#history-filter-persona");
const historyDeviceNode = document.querySelector("#history-filter-device");
const historyFilterButton = document.querySelector("#history-filter-apply");
const historyResultStatusNode = document.querySelector("#history-result-status");
const historyForgetAddressNode = document.querySelector("#history-forget-address");
const historyForgetObjectNode = document.querySelector("#history-forget-object");
const historyForgetButton = document.querySelector("#history-forget");
const HISTORY_FILTER_STATE_KEY = "graphshell.history.filter.controls.v1";
const HISTORY_FORGET_KEY = "graphshell.history.forget.pending.v1";
let pulledIds = [];
let currentPolicy = model.defaultPolicy();

function setStatus(message, state = "ready") {
  statusNode.textContent = message;
  statusNode.dataset.state = state;
}

async function send(message) {
  const response = await extensionApi.runtime.sendMessage(message);
  if (response?.error) {
    throw new Error(response.error);
  }
  return response;
}

function policyFromControls(current, enabled = current.enabled) {
  return model.normalizePolicy({
    ...current,
    enabled,
    strip_query: stripQueryNode.checked,
    excluded_origins: exclusionsNode.value
      .split(/[\n,]/)
      .map((origin) => origin.trim())
      .filter(Boolean),
  });
}

function showPolicy(policy) {
  stripQueryNode.checked = policy.strip_query;
  exclusionsNode.value = policy.excluded_origins.join("\n");
}

function sessionJson(key, fallback) {
  try {
    const value = sessionStorage.getItem(key);
    return value ? JSON.parse(value) : fallback;
  } catch {
    return fallback;
  }
}

function historyFilterState() {
  const stored = sessionJson(HISTORY_FILTER_STATE_KEY, {});
  return {
    days: Number.isInteger(stored.days) ? stored.days : 7,
    persona: typeof stored.persona === "string" ? stored.persona : "",
    device: typeof stored.device === "string" ? stored.device : "",
  };
}

function showHistoryFilter(state) {
  historyRangeNode.value = String(state.days);
  historyPersonaNode.value = state.persona;
  historyDeviceNode.value = state.device;
}

function applyHistoryFilter() {
  const state = {
    days: Number.parseInt(historyRangeNode.value, 10),
    persona: historyPersonaNode.value.trim(),
    device: historyDeviceNode.value.trim(),
  };
  sessionStorage.setItem(HISTORY_FILTER_STATE_KEY, JSON.stringify(state));
  historyResultStatusNode.textContent = "Applying authority filter…";
  location.reload();
}

function forgetHistory() {
  const request = model.forgetRequest(
    historyForgetAddressNode.value,
    historyForgetObjectNode.checked,
    currentPolicy,
  );
  if (!request) {
    historyResultStatusNode.textContent =
      "Enter a stored HTTP or HTTPS address to forget.";
    historyResultStatusNode.dataset.state = "failed";
    return;
  }
  const objectClause = request.remove_object
    ? " and remove its capture-created graph object"
    : "";
  if (
    !globalThis.confirm(
      `Forget all stored browser history for ${request.url}${objectClause}?`,
    )
  ) {
    return;
  }
  sessionStorage.setItem(HISTORY_FORGET_KEY, JSON.stringify(request));
  historyResultStatusNode.textContent = "Forgetting stored history…";
  location.reload();
}

function publishHistoryInputs() {
  const state = historyFilterState();
  showHistoryFilter(state);
  globalThis.graphshellHistoryFilterJson = JSON.stringify(
    model.historyFilterFromControls(
      state.days,
      state.persona,
      state.device,
    ),
  );
  const pendingForget = sessionStorage.getItem(HISTORY_FORGET_KEY);
  if (pendingForget) {
    globalThis.graphshellHistoryForgetJson = pendingForget;
  }
}

function finishHistoryControls() {
  sessionStorage.removeItem(HISTORY_FORGET_KEY);
  const error = document.body.dataset.historyError;
  const count = Number(document.body.dataset.historyResultCount || 0);
  const attempted = document.body.dataset.historyForgetAttempted === "true";
  const forgotten = Number(document.body.dataset.historyForgotten || 0);
  if (error) {
    historyResultStatusNode.textContent = `History action failed: ${error}`;
    historyResultStatusNode.dataset.state = "failed";
  } else if (forgotten > 0) {
    historyResultStatusNode.textContent =
      `Forgot ${forgotten} browsing trace(s) · ${count} matching visit(s) remain`;
  } else if (attempted) {
    historyResultStatusNode.textContent =
      `No stored browsing traces matched · ${count} matching visit(s) remain`;
  } else {
    historyResultStatusNode.textContent = `${count} matching stored visit(s)`;
  }
}

async function refreshStatus() {
  const status = await send({ type: "capture.status" });
  currentPolicy = status.policy;
  showPolicy(status.policy);
  enableButton.disabled = status.permission && status.policy.enabled;
  disableButton.disabled = !status.permission && !status.policy.enabled;
  setStatus(
    status.permission && status.policy.enabled
      ? `Capture enabled · ${status.queued} visit(s) waiting for Graphshell`
      : "Capture is off. Graphshell has not requested browser history access.",
  );
  return status;
}

async function enableCapture() {
  setStatus("Waiting for browser permission…", "working");
  const granted = await extensionApi.permissions.request({
    permissions: ["history"],
  });
  if (!granted) {
    setStatus("Browser history access was not granted.", "failed");
    return;
  }
  const current = (await send({ type: "capture.status" })).policy;
  const policy = policyFromControls(current, true);
  await send({ type: "capture.configure", policy });
  const days = Number.parseInt(rangeNode.value, 10);
  const imported =
    days > 0
      ? await send({ type: "capture.import", days })
      : { queued: 0 };
  setStatus(`Capture enabled · ${imported.queued} imported visit(s)`, "working");
  location.reload();
}

async function savePolicy() {
  const status = await send({ type: "capture.status" });
  const policy = policyFromControls(status.policy);
  await send({ type: "capture.configure", policy });
  setStatus("Privacy settings saved. They apply before visits enter the queue.");
}

async function disableCapture() {
  const removed = await extensionApi.permissions.remove({
    permissions: ["history"],
  });
  const status = await send({ type: "capture.status" });
  await send({
    type: "capture.configure",
    policy: { ...status.policy, enabled: false },
  });
  setStatus(
    removed
      ? "Capture stopped and browser history permission removed."
      : "Capture stopped.",
  );
  await refreshStatus();
}

async function prepareCapture() {
  panel.hidden = false;
  historyFilterButton.addEventListener("click", applyHistoryFilter);
  historyForgetButton.addEventListener("click", forgetHistory);
  enableButton.addEventListener("click", () => {
    enableCapture().catch((error) => setStatus(String(error), "failed"));
  });
  saveButton.addEventListener("click", () => {
    savePolicy().catch((error) => setStatus(String(error), "failed"));
  });
  disableButton.addEventListener("click", () => {
    disableCapture().catch((error) => setStatus(String(error), "failed"));
  });

  const status = await refreshStatus();
  const pulled =
    status.permission && status.policy.enabled
      ? await send({ type: "capture.pull" })
      : { policy: status.policy, visits: [], ids: [] };
  pulledIds = pulled.ids;
  globalThis.graphshellCapturePolicyJson = JSON.stringify(pulled.policy);
  globalThis.graphshellInitialVisitsJson = JSON.stringify(pulled.visits);
  currentPolicy = pulled.policy;
  publishHistoryInputs();

  document.addEventListener(
    "graphshell-capture-complete",
    async () => {
      if (pulledIds.length > 0) {
        await send({ type: "capture.ack", ids: pulledIds });
        pulledIds = [];
      }
      const accepted = Number(document.body.dataset.captureAccepted || 0);
      const dropped = Number(document.body.dataset.captureDropped || 0);
      setStatus(`Graph updated · ${accepted} accepted · ${dropped} filtered`);
    },
    { once: true },
  );
  document.addEventListener(
    "graphshell-history-controls-complete",
    finishHistoryControls,
    { once: true },
  );
}

export { prepareCapture };

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
let pulledIds = [];

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

async function refreshStatus() {
  const status = await send({ type: "capture.status" });
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
}

export { prepareCapture };

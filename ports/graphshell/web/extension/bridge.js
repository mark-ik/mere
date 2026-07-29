const extensionApi = globalThis.browser ?? globalThis.chrome;
const statusNode = document.querySelector("#status");
const cardsNode = document.querySelector("#cards");
const cardTemplate = document.querySelector("#card-template");
const disconnectButton = document.querySelector("#disconnect");
const port = extensionApi.runtime.connectNative("org.mere.graphshell");
const encoder = new TextEncoder();
const decoder = new TextDecoder();

let nextId = 1;
let projection = null;
let snapshot = null;
let cleanDisconnect = false;
let nativeIdentityInFlight = false;
const pending = new Map();

function setStatus(message, state = "live") {
  statusNode.textContent = message;
  statusNode.dataset.state = state;
}

function nonce() {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

function request(body, kind) {
  const id = nextId++;
  pending.set(id, kind);
  port.postMessage({
    type: "request",
    request: { id, body },
  });
}

function requestNativeIdentity(action, kind) {
  if (nativeIdentityInFlight) {
    return false;
  }
  const id = nextId++;
  pending.set(id, kind);
  nativeIdentityInFlight = true;
  setNativeIdentityControlsDisabled(true);
  try {
    port.postMessage({
      type: "native_identity",
      request: {
        id,
        session: snapshot.session,
        action,
      },
    });
  } catch (error) {
    pending.delete(id);
    nativeIdentityInFlight = false;
    setNativeIdentityControlsDisabled(false);
    throw error;
  }
  return true;
}

function openSession() {
  request({
    Open: {
      version: { major: 1, minor: 3 },
      capabilities: { capabilities: ["PortableCard"] },
    },
  }, "open");
}

function responseBody(response) {
  if (response.body.Err) {
    throw new Error(response.body.Err.message);
  }
  return response.body.Ok;
}

function handleResponse(response) {
  const kind = pending.get(response.id);
  pending.delete(response.id);
  const body = responseBody(response);
  if (kind === "open") {
    projection = body.Opened.descriptor.projections[0]?.request;
    if (!projection) {
      throw new Error("The native authority disclosed no identity projection.");
    }
    request({ Snapshot: projection }, "snapshot");
    return;
  }
  if (kind === "snapshot") {
    snapshot = body.Snapshot;
    cardsNode.replaceChildren();
    const resources = [];
    for (const binding of snapshot.presentation.bindings) {
      const offers = snapshot.presentation.offers[binding.key] ?? [];
      const offer = offers.find((candidate) => candidate.codec === "PortableCardV1");
      if (offer) {
        resources.push({ binding, offer });
      }
    }
    for (const item of resources) {
      request({
        Resource: {
          session: snapshot.session,
          resource: item.offer.resource,
        },
      }, { type: "resource", item });
    }
    setStatus(`Admitted · loading ${resources.length} public cards`);
    return;
  }
  if (kind?.type === "resource") {
    const resource = body.Resource;
    const card = JSON.parse(decoder.decode(new Uint8Array(resource.bytes)));
    renderCard(card, kind.item);
    setStatus(`Admitted · ${cardsNode.childElementCount} public cards`);
    return;
  }
  if (kind?.type === "intent") {
    if (body.Intent === "Accepted") {
      setStatus(`${kind.label} accepted · refreshing`);
      request({ Snapshot: projection }, "snapshot");
    } else {
      setStatus(`${kind.label} was not accepted`, "failed");
    }
    return;
  }
  if (kind === "close" && body === "Closed") {
    cleanDisconnect = true;
    disconnectButton.disabled = true;
    setStatus("Device session closed", "closed");
    port.disconnect();
  }
}

const nativeFailureLabels = {
  wrong_session: "The native session changed. Refresh and try again.",
  ui_unavailable: "This device has no graphical native-dialog provider.",
  selected_file_unreadable: "The selected file could not be read.",
  selected_file_too_large: "The selected file is too large to be an SSH private key.",
  invalid_private_key: "The selected file is not a supported OpenSSH private key.",
  incorrect_passphrase: "That passphrase did not unlock the selected SSH private key.",
  import_rejected: "Personae rejected the selected key.",
};

function setNativeIdentityControlsDisabled(disabled) {
  for (const control of document.querySelectorAll(
    ".native-import button, .native-import select, .native-import input",
  )) {
    control.disabled = disabled;
  }
}

function handleNativeIdentityResult(message) {
  const kind = pending.get(message.id);
  pending.delete(message.id);
  nativeIdentityInFlight = false;
  setNativeIdentityControlsDisabled(false);
  const result = message.result;
  if (result.status === "imported_ssh_private") {
    const replacement = result.replaced_existing ? "replaced" : "imported";
    setStatus(`SSH key ${replacement} · ${result.fingerprint} · refreshing`);
    request({ Snapshot: projection }, "snapshot");
    return;
  }
  if (result.status === "cancelled") {
    setStatus(`${kind?.label ?? "Native interaction"} cancelled`);
    return;
  }
  if (result.status === "rejected") {
    setStatus(
      nativeFailureLabels[result.reason] ?? "The native identity interaction was rejected.",
      "failed",
    );
    return;
  }
  setStatus("The native host returned an unknown identity result.", "failed");
}

function importUnlockPolicy(select, idleInput) {
  if (select.value === "short_ttl") {
    const idleSeconds = Number.parseInt(idleInput.value, 10);
    if (!Number.isInteger(idleSeconds) || idleSeconds < 1 || idleSeconds > 86400) {
      throw new Error("Idle approval must be between 1 and 86400 seconds.");
    }
    return { kind: "short_ttl", idle_seconds: idleSeconds };
  }
  return { kind: select.value };
}

function invokeIntent(action, item, payload) {
  request({
    Intent: {
      session: snapshot.session,
      target: item.binding.instance,
      observed_epoch: snapshot.scene.epoch,
      observed_revision: snapshot.scene.revision,
      intent: action.intent,
      payload: Array.from(encoder.encode(JSON.stringify(payload))),
    },
  }, { type: "intent", label: action.label });
}

function renderNativeImportAction(action, actionsNode) {
  const controls = document.createElement("div");
  controls.className = "native-import";

  const selectLabel = document.createElement("label");
  selectLabel.textContent = "Signing approval";
  const select = document.createElement("select");
  select.setAttribute("aria-label", "Signing approval policy for imported key");
  for (const [value, label] of [
    ["per_use", "Every use"],
    ["short_ttl", "Short idle window"],
    ["session", "Unlocked session"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.append(option);
  }
  selectLabel.append(select);

  const idleLabel = document.createElement("label");
  idleLabel.textContent = "Idle seconds";
  idleLabel.hidden = true;
  const idleInput = document.createElement("input");
  idleInput.type = "number";
  idleInput.min = "1";
  idleInput.max = "86400";
  idleInput.value = "300";
  idleInput.setAttribute("aria-label", "Short idle approval in seconds");
  idleLabel.append(idleInput);
  select.addEventListener("change", () => {
    idleLabel.hidden = select.value !== "short_ttl";
  });

  const button = document.createElement("button");
  button.textContent = action.label;
  button.dataset.intent = action.intent;
  button.addEventListener("click", () => {
    try {
      const unlockPolicy = importUnlockPolicy(select, idleInput);
      const requested = requestNativeIdentity(
        {
          type: "import_ssh_private",
          unlock_policy: unlockPolicy,
        },
        { type: "native_identity", label: action.label },
      );
      if (requested) {
        setStatus("Opening the native key picker on this device…");
      }
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "failed");
    }
  });

  controls.append(selectLabel, idleLabel, button);
  actionsNode.append(controls);
}

function renderConfirmedIdentityAction(action, card, item, actionsNode) {
  const specs = {
    "graphshell.identity.ssh.remove": {
      field: "Fingerprint",
      key: "fingerprint",
      prompt: "Remove this SSH key from the Personae vault?",
    },
    "graphshell.identity.device.revoke": {
      field: "Device",
      key: "device_id",
      prompt: "Revoke this delegated device and rotate its future access?",
    },
  };
  const spec = specs[action.intent];
  if (!spec) {
    return false;
  }
  const value = card.values.find((candidate) => candidate.label === spec.field)?.value;
  if (!value) {
    return true;
  }
  const button = document.createElement("button");
  button.textContent = action.label;
  button.dataset.intent = action.intent;
  button.addEventListener("click", () => {
    if (!globalThis.confirm(spec.prompt)) {
      setStatus(`${action.label} cancelled`);
      return;
    }
    invokeIntent(action, item, { [spec.key]: value, confirmed: true });
  });
  actionsNode.append(button);
  return true;
}

function renderCard(card, item) {
  const node = cardTemplate.content.firstElementChild.cloneNode(true);
  node.dataset.instance = String(item.binding.instance);
  node.querySelector("h2").textContent = card.title;
  const list = node.querySelector("ul");
  for (const value of card.values) {
    const row = document.createElement("li");
    const label = document.createElement("strong");
    label.textContent = `${value.label}: `;
    row.append(label, document.createTextNode(value.value));
    row.dataset.label = value.label;
    list.append(row);
  }
  node.querySelector(".badges").textContent = card.badges.join(" · ");
  const actionsNode = node.querySelector(".actions");
  for (const action of item.offer.semantics.actions) {
    if (action.intent === "graphshell.identity.ssh.import-native") {
      renderNativeImportAction(action, actionsNode);
      continue;
    }
    if (renderConfirmedIdentityAction(action, card, item, actionsNode)) {
      continue;
    }
    if (!action.intent.startsWith("graphshell.identity.signing.")) {
      continue;
    }
    const button = document.createElement("button");
    button.textContent = action.label;
    button.dataset.intent = action.intent;
    button.addEventListener("click", () => {
      const requestId = card.values.find((value) => value.label === "Request")?.value;
      if (!requestId) {
        setStatus("The pending request card has no public request id.", "failed");
        return;
      }
      invokeIntent(action, item, { request_id: requestId });
    });
    actionsNode.append(button);
  }
  cardsNode.append(node);
}

port.onMessage.addListener((message) => {
  try {
    if (message.type === "challenge") {
      port.postMessage({
        type: "connect",
        schema: "mere.graphshell/browser-connect/v1",
        host_nonce: message.challenge.host_nonce,
        client_nonce: nonce(),
      });
      setStatus("Native host found · admitting this extension");
      return;
    }
    if (message.type === "connected") {
      document.body.dataset.session = message.session;
      document.body.dataset.subject = message.subject;
      disconnectButton.disabled = false;
      setStatus(`Admitted through ${message.launcher.family}`);
      openSession();
      return;
    }
    if (message.type === "response") {
      handleResponse(message.response);
      return;
    }
    if (message.type === "native_identity_result") {
      handleNativeIdentityResult(message);
      return;
    }
    if (message.type === "failure") {
      throw new Error(message.message);
    }
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error), "failed");
  }
});

port.onDisconnect.addListener(() => {
  if (cleanDisconnect) {
    return;
  }
  const error = extensionApi.runtime.lastError;
  setStatus(error?.message ?? "Native host disconnected.", "failed");
});

disconnectButton.addEventListener("click", () => {
  disconnectButton.disabled = true;
  request("Close", "close");
});

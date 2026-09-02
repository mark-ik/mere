// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

const originalError = console.error.bind(console);

console.error = (...args) => {
  if (!document.title.startsWith("GRAPHSHELL H3 FAIL")) {
    document.title = `GRAPHSHELL H3 FAIL: ${args.map(String).join(" ").slice(0, 240)}`;
  }
  originalError(...args);
};

// Every page error, kept: the host rewrites the title each frame, so a title
// alone can hide one. A scenario receipt carries this list.
window.graphshellErrors = [];
window.addEventListener("error", (event) => {
  window.graphshellErrors.push(String(event.message));
  document.title = `GRAPHSHELL H3 FAIL: ${event.message}`;
});
window.addEventListener("unhandledrejection", (event) => {
  window.graphshellErrors.push(`unhandled rejection: ${event.reason}`);
});

function semanticNode(element) {
  const role =
    element.getAttribute("role") ||
    ({ BUTTON: "button", NAV: "navigation", MAIN: "main", SECTION: "region" }[
      element.tagName
    ] ?? null);
  const label =
    element.getAttribute("aria-label") ||
    (element.matches("button, h1, h2, dd") ? element.textContent.trim() : null);
  const children = [...element.children]
    .filter((child) => child.getAttribute("aria-hidden") !== "true")
    .map(semanticNode)
    .filter((child) => child.role || child.label || child.children.length);
  return {
    ...(role ? { role } : {}),
    ...(label ? { label } : {}),
    ...(element.id ? { id: element.id } : {}),
    ...(element.hasAttribute("aria-pressed")
      ? { pressed: element.getAttribute("aria-pressed") === "true" }
      : {}),
    children,
  };
}

window.graphshellSemanticTree = () =>
  semanticNode(document.getElementById("semantic-host"));

window.graphshellScenario = () => ({
  state: document.body.dataset.scenario ?? null,
  errors: [...window.graphshellErrors],
  result: JSON.parse(document.getElementById("scenario-result")?.textContent || "null"),
  captures: [...document.querySelectorAll("#scenario-captures img")].map((img) => ({
    name: img.dataset.capture,
    width: Number(img.width),
    height: Number(img.height),
    bytes: img.src.length,
  })),
});

window.graphshellReceipt = () => ({
  title: document.title,
  ready: document.body.dataset.ready === "true",
  session: document.body.dataset.session,
  detailOpen: document.body.dataset.detailOpen === "true",
  actionCount: Number(document.body.dataset.actionCount || 0),
  storage: document.body.dataset.storage,
  capture: {
    accepted: Number(document.body.dataset.captureAccepted || 0),
    dropped: Number(document.body.dataset.captureDropped || 0),
  },
  remote: {
    link: document.body.dataset.remoteLink,
    state: document.body.dataset.remoteState,
    revision: document.body.dataset.remoteRevision,
    cards: document.body.dataset.remoteCards,
    resume: document.body.dataset.remoteResume,
    subject: document.body.dataset.remoteSubject,
    session: document.body.dataset.remoteSession,
    actions: [...document.querySelectorAll("#remote-actions button")].map((b) => ({
      intent: b.dataset.intent,
      label: b.textContent,
    })),
  },
  camera: document.getElementById("graphshell-canvas").dataset.camera,
  focusedNode: document.getElementById("graphshell-canvas").dataset.focusedNode,
  product: {
    status: document.body.dataset.productStatus,
    nodeCount: Number(document.body.dataset.nodeCount || 0),
    filterCount: Number(document.body.dataset.filterCount || 0),
    layout: document.body.dataset.layout,
    physicsPaused: document.body.dataset.physicsPaused === "true",
    selectedCount: Number(document.body.dataset.selectedCount || 0),
    exportBytes: Number(document.body.dataset.exportBytes || 0),
    importedNodes: Number(document.body.dataset.importedNodes || 0),
    relationFamily: document.body.dataset.relationFamily,
    face: document.body.dataset.face,
  },
  viewport: {
    width: window.innerWidth,
    height: window.innerHeight,
  },
  projectionEditor: {
    open: document.body.dataset.projectionEditorOpen === "true",
    panel: document.body.dataset.projectionEditorPanel,
    content: document.body.dataset.projectionEditorContent,
    validation: document.body.dataset.projectionEditorValidation,
    errors: Number(document.body.dataset.projectionEditorErrors || 0),
    saveCount: Number(document.body.dataset.projectionEditorSaveCount || 0),
    status: document.getElementById("projection-editor-status")?.textContent,
    preview: document.getElementById("projection-editor-preview")?.textContent,
    source: {
      authority: document.getElementById("projection-source-authority")?.value,
      domain: document.getElementById("projection-source-domain")?.value,
      resource: document.getElementById("projection-source-resource")?.value,
    },
    reading: {
      key: document.getElementById("projection-reading-key")?.value,
    },
    encoding: {
      x: document.getElementById("projection-encoding-x")?.value,
      y: document.getElementById("projection-encoding-y")?.value,
    },
    arrangement: {
      kind: document.getElementById("projection-arrangement-kind")?.value,
      direction: document.getElementById("projection-arrangement-direction")?.value,
      spacing: document.getElementById("projection-arrangement-spacing")?.value,
    },
    appearance: {
      realization: document.getElementById("projection-appearance-realization")?.value,
      title: document.getElementById("projection-appearance-title")?.value,
    },
    provenance: {
      author: document.getElementById("projection-provenance-author")?.value,
      sourceRevision: document.getElementById("projection-provenance-revision")?.value,
      note: document.getElementById("projection-provenance-note")?.value,
    },
  },
  semantics: window.graphshellSemanticTree(),
});

try {
  if ((globalThis.browser ?? globalThis.chrome)?.runtime?.id) {
    await import("./capture-model.js");
    const extensionProfile = await import("./extension-profile.js");
    await extensionProfile.prepareCapture();
  }
  const module = await import("./pkg/graphshell_web.js");
  await module.default();
  // The remote link: `?signal=<url>` joins a host over WebRTC through its
  // signaling server (`GET /invite` unless `?invite=` is given, `POST
  // /offer`). Without it the in-process fixture stays mounted.
  const signal = new URLSearchParams(location.search).get("signal");
  if (signal) {
    await new Promise((resolve) => {
      const poll = () =>
        document.body.dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
      poll();
    });
    module.connect_remote(signal, new URLSearchParams(location.search).get("invite"));
  }
  // The scenario lane: `?scenario=<path>` names a script the page runs on
  // itself once the host reports ready. Results land in the DOM (see
  // src/web_scenario.rs); nothing here interprets them.
  const scenarioPath = new URLSearchParams(location.search).get("scenario");
  if (scenarioPath) {
    const response = await fetch(scenarioPath);
    if (!response.ok) {
      throw new Error(`scenario ${scenarioPath}: HTTP ${response.status}`);
    }
    const text = await response.text();
    await new Promise((resolve) => {
      const poll = () =>
        document.body.dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
      poll();
    });
    // `?sink=<url>` names a receipt sink: when the run completes, the result,
    // the host receipt and every capture (as data URLs) are POSTed there as
    // one JSON body, so a driver outside the page collects files rather
    // than reading a DOM it may not be able to reach.
    const sink = new URLSearchParams(location.search).get("sink");
    if (sink) {
      document.addEventListener(
        "graphshell-scenario-complete",
        async () => {
          const scenario = window.graphshellScenario();
          const captures = [...document.querySelectorAll("#scenario-captures img")].map(
            (img) => ({ name: img.dataset.capture, dataUrl: img.src }),
          );
          try {
            await fetch(sink, {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({
                scenario,
                receipt: window.graphshellReceipt(),
                captures,
              }),
            });
            document.body.dataset.scenarioSink = "delivered";
          } catch (error) {
            document.body.dataset.scenarioSink = `failed: ${error}`;
          }
        },
        { once: true },
      );
    }
    module.run_scenario(text);
  }
} catch (error) {
  document.title = `GRAPHSHELL H3 FAIL: ${error}`;
  originalError(error);
}

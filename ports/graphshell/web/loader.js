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

// The component's root and its parts. Everything the receipt reads comes
// from under the root; the page's own ids are not consulted.
const graphshellRoot = () => document.querySelector("graphshell-view");
const part = (name) => graphshellRoot()?.querySelector(`#gs-${name}`);

window.graphshellSemanticTree = () => semanticNode(part("semantic-host"));

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
  ready: graphshellRoot().dataset.ready === "true",
  session: graphshellRoot().dataset.session,
  detailOpen: graphshellRoot().dataset.detailOpen === "true",
  actionCount: Number(graphshellRoot().dataset.actionCount || 0),
  storage: graphshellRoot().dataset.storage,
  capture: {
    accepted: Number(graphshellRoot().dataset.captureAccepted || 0),
    dropped: Number(graphshellRoot().dataset.captureDropped || 0),
  },
  remote: {
    link: graphshellRoot().dataset.remoteLink,
    state: graphshellRoot().dataset.remoteState,
    revision: graphshellRoot().dataset.remoteRevision,
    cards: graphshellRoot().dataset.remoteCards,
    resume: graphshellRoot().dataset.remoteResume,
    subject: graphshellRoot().dataset.remoteSubject,
    session: graphshellRoot().dataset.remoteSession,
    actions: [...graphshellRoot().querySelectorAll("#gs-remote-actions button")].map((b) => ({
      intent: b.dataset.intent,
      label: b.textContent,
    })),
  },
  camera: part("graphshell-canvas").dataset.camera,
  focusedNode: part("graphshell-canvas").dataset.focusedNode,
  product: {
    status: graphshellRoot().dataset.productStatus,
    nodeCount: Number(graphshellRoot().dataset.nodeCount || 0),
    filterCount: Number(graphshellRoot().dataset.filterCount || 0),
    layout: graphshellRoot().dataset.layout,
    physicsPaused: graphshellRoot().dataset.physicsPaused === "true",
    selectedCount: Number(graphshellRoot().dataset.selectedCount || 0),
    exportBytes: Number(graphshellRoot().dataset.exportBytes || 0),
    importedNodes: Number(graphshellRoot().dataset.importedNodes || 0),
    relationFamily: graphshellRoot().dataset.relationFamily,
    face: graphshellRoot().dataset.face,
  },
  viewport: {
    width: window.innerWidth,
    height: window.innerHeight,
  },
  projectionEditor: {
    open: graphshellRoot().dataset.projectionEditorOpen === "true",
    panel: graphshellRoot().dataset.projectionEditorPanel,
    content: graphshellRoot().dataset.projectionEditorContent,
    validation: graphshellRoot().dataset.projectionEditorValidation,
    errors: Number(graphshellRoot().dataset.projectionEditorErrors || 0),
    saveCount: Number(graphshellRoot().dataset.projectionEditorSaveCount || 0),
    status: part("projection-editor-status")?.textContent,
    preview: part("projection-editor-preview")?.textContent,
    source: {
      authority: part("projection-source-authority")?.value,
      domain: part("projection-source-domain")?.value,
      resource: part("projection-source-resource")?.value,
    },
    reading: {
      key: part("projection-reading-key")?.value,
    },
    encoding: {
      x: part("projection-encoding-x")?.value,
      y: part("projection-encoding-y")?.value,
    },
    arrangement: {
      kind: part("projection-arrangement-kind")?.value,
      direction: part("projection-arrangement-direction")?.value,
      spacing: part("projection-arrangement-spacing")?.value,
    },
    appearance: {
      realization: part("projection-appearance-realization")?.value,
      title: part("projection-appearance-title")?.value,
    },
    provenance: {
      author: part("projection-provenance-author")?.value,
      sourceRevision: part("projection-provenance-revision")?.value,
      note: part("projection-provenance-note")?.value,
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
  // Two ways in, one component. `mountGraphshell(element)` is the plain
  // entry; `<graphshell-view>` is the same entry as a custom element, mounted
  // when it connects. Elements already in the document upgrade on define.
  window.mountGraphshell = (element) => module.mount(element);
  if (!customElements.get("graphshell-view")) {
    customElements.define(
      "graphshell-view",
      class extends HTMLElement {
        connectedCallback() {
          if (!this.dataset.mounted) {
            this.dataset.mounted = "true";
            module.mount(this);
          }
        }
      },
    );
  }
  // The remote link: `?signal=<url>` joins a host over WebRTC through its
  // signaling server (`GET /invite` unless `?invite=` is given, `POST
  // /offer`). Without it the in-process fixture stays mounted.
  const signal = new URLSearchParams(location.search).get("signal");
  if (signal) {
    await new Promise((resolve) => {
      const poll = () =>
        graphshellRoot().dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
      poll();
    });
    module.connect_remote(signal, new URLSearchParams(location.search).get("invite"));
  }
  // The scenario lane: `?scenario=<path>` names a script the page runs on
  // itself once the host reports ready. Results land in the DOM (see
  // src/web_scenario.rs); nothing here interprets them.
  const scenarioPath = new URLSearchParams(location.search).get("scenario");
  if (scenarioPath) {
    // Never from the HTTP cache: a receipt profile persists across runs, and
    // a scenario edited between them must be the one that runs.
    const response = await fetch(scenarioPath, { cache: "no-store" });
    if (!response.ok) {
      throw new Error(`scenario ${scenarioPath}: HTTP ${response.status}`);
    }
    const text = await response.text();
    await new Promise((resolve) => {
      const poll = () =>
        graphshellRoot().dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
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

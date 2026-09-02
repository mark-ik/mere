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

window.addEventListener("error", (event) => {
  document.title = `GRAPHSHELL H3 FAIL: ${event.message}`;
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
} catch (error) {
  document.title = `GRAPHSHELL H3 FAIL: ${error}`;
  originalError(error);
}

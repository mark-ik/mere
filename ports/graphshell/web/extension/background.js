// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

if (!globalThis.GraphshellCaptureModel && typeof importScripts === "function") {
  importScripts("capture-model.js");
}

const extensionApi = globalThis.browser ?? globalThis.chrome;
const model = globalThis.GraphshellCaptureModel;
const POLICY_KEY = "graphshell.capture.policy.v1";
const QUEUE_KEY = "graphshell.capture.queue.v1";
const VISIT_MAP_KEY = "graphshell.capture.visit-map.v1";
const SOURCE = extensionApi.runtime.getURL("").startsWith("moz-extension:")
  ? "firefox"
  : "chromium";
let queueMutation = Promise.resolve();

extensionApi.action.onClicked.addListener(() => {
  extensionApi.tabs.create({
    url: extensionApi.runtime.getURL("graph.html"),
  });
});

async function storedCaptureState() {
  const stored = await extensionApi.storage.local.get([
    POLICY_KEY,
    QUEUE_KEY,
    VISIT_MAP_KEY,
  ]);
  return {
    policy: model.normalizePolicy(stored[POLICY_KEY]),
    queue: Array.isArray(stored[QUEUE_KEY]) ? stored[QUEUE_KEY] : [],
    visitMap:
      stored[VISIT_MAP_KEY] && typeof stored[VISIT_MAP_KEY] === "object"
        ? stored[VISIT_MAP_KEY]
        : {},
  };
}

function serializedQueueMutation(operation) {
  const next = queueMutation.then(operation, operation);
  queueMutation = next.catch(() => {});
  return next;
}

async function queueVisits(rawVisits) {
  return serializedQueueMutation(async () => {
    const state = await storedCaptureState();
    if (!state.policy.enabled) {
      return 0;
    }
    const visits = rawVisits
      .map((visit) => model.sanitizeVisit(visit, state.policy))
      .filter(Boolean);
    const queue = model.mergeQueue(state.queue, visits);
    const visitMap = { ...state.visitMap };
    for (const visit of visits) {
      if (visit.visit_id) {
        visitMap[visit.visit_id] = { url: visit.url, at_ms: visit.at_ms };
      }
    }
    const retainedVisitMap = Object.fromEntries(
      Object.entries(visitMap)
        .sort((left, right) => left[1].at_ms - right[1].at_ms)
        .slice(-512),
    );
    await extensionApi.storage.local.set({
      [QUEUE_KEY]: queue,
      [VISIT_MAP_KEY]: retainedVisitMap,
    });
    return visits.length;
  });
}

async function liveVisit(historyItem) {
  const state = await storedCaptureState();
  if (!state.policy.enabled || !historyItem.url) {
    return;
  }
  const visits = await extensionApi.history.getVisits({ url: historyItem.url });
  const latest = visits
    .filter((visit) => Number.isFinite(visit.visitTime))
    .sort((left, right) => right.visitTime - left.visitTime)[0];
  await queueVisits([
    {
      source: SOURCE,
      visit_id: latest?.visitId ?? historyItem.id ?? null,
      url: historyItem.url,
      title: historyItem.title ?? null,
      favicon_url: null,
      referrer_url: latest?.referringVisitId
        ? state.visitMap[latest.referringVisitId]?.url ?? null
        : null,
      transition: latest?.transition ?? "unknown",
      at_ms: latest?.visitTime ?? historyItem.lastVisitTime ?? Date.now(),
      private: false,
    },
  ]);
}

async function importHistory(days) {
  const state = await storedCaptureState();
  if (!state.policy.enabled) {
    return 0;
  }
  const startTime = Date.now() - Math.max(1, Math.min(365, days)) * 86_400_000;
  const pages = await extensionApi.history.search({
    text: "",
    startTime,
    maxResults: 512,
  });
  const pageVisits = await Promise.all(
    pages
      .filter((page) => page.url)
      .map(async (page) => ({
        page,
        visits: await extensionApi.history.getVisits({ url: page.url }),
      })),
  );
  const flattened = pageVisits
    .flatMap(({ page, visits }) =>
      visits
        .filter((visit) => (visit.visitTime ?? 0) >= startTime)
        .map((visit) => ({ page, visit })),
    )
    .sort((left, right) => (left.visit.visitTime ?? 0) - (right.visit.visitTime ?? 0))
    .slice(-model.DEFAULT_QUEUE_LIMIT);
  const visitUrls = Object.fromEntries(
    flattened.map(({ page, visit }) => [visit.visitId, page.url]),
  );
  return queueVisits(
    flattened.map(({ page, visit }) => ({
      source: SOURCE,
      visit_id: visit.visitId,
      url: page.url,
      title: page.title ?? null,
      favicon_url: null,
      referrer_url: visit.referringVisitId
        ? visitUrls[visit.referringVisitId] ?? null
        : null,
      transition: visit.transition ?? "unknown",
      at_ms: visit.visitTime ?? page.lastVisitTime ?? 0,
      private: false,
    })),
  );
}

async function handleMessage(message) {
  if (message?.type === "capture.status") {
    const state = await storedCaptureState();
    return {
      policy: state.policy,
      queued: state.queue.length,
      permission: await extensionApi.permissions.contains({
        permissions: ["history"],
      }),
    };
  }
  if (message?.type === "capture.configure") {
    const policy = model.normalizePolicy(message.policy);
    const state = await storedCaptureState();
    const queue = state.queue
      .map((visit) =>
        model.sanitizeVisit(visit, {
          ...policy,
          enabled: true,
        }),
      )
      .filter(Boolean);
    await extensionApi.storage.local.set({
      [POLICY_KEY]: policy,
      [QUEUE_KEY]: model.mergeQueue([], queue),
    });
    return { policy };
  }
  if (message?.type === "capture.import") {
    return { queued: await importHistory(Number(message.days) || 1) };
  }
  if (message?.type === "capture.pull") {
    const state = await storedCaptureState();
    return {
      policy: state.policy,
      visits: state.queue,
      ids: state.queue.map(model.queueKey),
    };
  }
  if (message?.type === "capture.ack") {
    const acknowledged = new Set(Array.isArray(message.ids) ? message.ids : []);
    await serializedQueueMutation(async () => {
      const state = await storedCaptureState();
      await extensionApi.storage.local.set({
        [QUEUE_KEY]: state.queue.filter(
          (visit) => !acknowledged.has(model.queueKey(visit)),
        ),
      });
    });
    return { acknowledged: acknowledged.size };
  }
  throw new Error(`Unknown Graphshell extension message: ${message?.type}`);
}

extensionApi.runtime.onMessage.addListener((message, _sender, respond) => {
  handleMessage(message).then(
    (result) => respond(result),
    (error) => respond({ error: String(error) }),
  );
  return true;
});

let historyListenerInstalled = false;

function installHistoryListener() {
  if (historyListenerInstalled || !extensionApi.history?.onVisited) {
    return;
  }
  extensionApi.history.onVisited.addListener((historyItem) => {
    liveVisit(historyItem).catch((error) => console.error("Graphshell capture", error));
  });
  historyListenerInstalled = true;
}

installHistoryListener();

extensionApi.permissions.onAdded.addListener((added) => {
  if (added.permissions?.includes("history")) {
    installHistoryListener();
  }
});

extensionApi.permissions.onRemoved.addListener((removed) => {
  if (!removed.permissions?.includes("history")) {
    return;
  }
  storedCaptureState()
    .then((state) =>
      extensionApi.storage.local.set({
        [POLICY_KEY]: { ...state.policy, enabled: false },
      }),
    )
    .catch((error) => console.error("Graphshell permission removal", error));
});

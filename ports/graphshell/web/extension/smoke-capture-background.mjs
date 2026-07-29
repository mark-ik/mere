import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

function event() {
  return {
    listeners: [],
    addListener(listener) {
      this.listeners.push(listener);
    },
  };
}

const actionClicked = event();
const runtimeMessage = event();
const historyVisited = event();
const permissionAdded = event();
const permissionRemoved = event();
const stored = {};
let historyGranted = false;
const fromTime = Date.now() - 2000;
const toTime = Date.now() - 1000;

const chrome = {
  action: { onClicked: actionClicked },
  tabs: { create: async () => {} },
  runtime: {
    getURL: (path) => `chrome-extension://graphshell/${path}`,
    onMessage: runtimeMessage,
  },
  storage: {
    local: {
      async get(keys) {
        return Object.fromEntries(
          keys.filter((key) => key in stored).map((key) => [key, stored[key]]),
        );
      },
      async set(values) {
        Object.assign(stored, values);
      },
    },
  },
  permissions: {
    contains: async () => historyGranted,
    onAdded: permissionAdded,
    onRemoved: permissionRemoved,
  },
  history: {
    onVisited: historyVisited,
    async search() {
      return [
        {
          id: "page-1",
          url: "https://example.net/to?token=secret#part",
          title: "Destination",
          lastVisitTime: toTime,
        },
        {
          id: "page-2",
          url: "https://example.net/from?token=secret",
          title: "Source",
          lastVisitTime: fromTime,
        },
      ];
    },
    async getVisits({ url }) {
      if (url.includes("/from")) {
        return [
          {
            visitId: "from",
            referringVisitId: "",
            transition: "typed",
            visitTime: fromTime,
          },
        ];
      }
      return [
        {
          visitId: "to",
          referringVisitId: "from",
          transition: "link",
          visitTime: toTime,
        },
      ];
    },
  },
};

const context = vm.createContext({
  chrome,
  console,
  URL,
  globalThis: null,
  importScripts() {},
});
context.globalThis = context;
vm.runInContext(
  fs.readFileSync(new URL("./capture-model.js", import.meta.url), "utf8"),
  context,
);
vm.runInContext(
  fs.readFileSync(new URL("./background.js", import.meta.url), "utf8"),
  context,
);

function send(message) {
  return new Promise((resolve) => {
    const keptOpen = runtimeMessage.listeners[0](message, {}, resolve);
    assert.equal(keptOpen, true);
  }).then((response) => {
    if (response?.error) {
      throw new Error(response.error);
    }
    return response;
  });
}

historyGranted = true;
const status = await send({ type: "capture.status" });
assert.equal(status.permission, true);
const configured = await send({
  type: "capture.configure",
  policy: {
    ...status.policy,
    enabled: true,
    strip_query: true,
  },
});
assert.equal(configured.policy.enabled, true);
assert.equal((await send({ type: "capture.import", days: 7 })).queued, 2);
const pulled = await send({ type: "capture.pull" });
assert.equal(pulled.visits.length, 2);
assert.equal(pulled.visits[1].url, "https://example.net/to");
assert.equal(pulled.visits[1].referrer_url, "https://example.net/from");
assert.equal(pulled.visits[1].transition, "link");
await send({ type: "capture.ack", ids: pulled.ids });
assert.equal((await send({ type: "capture.pull" })).visits.length, 0);
console.log("Graphshell capture background smoke passed");

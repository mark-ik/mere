import assert from "node:assert/strict";
import "../extension/capture-model.js";

const model = globalThis.GraphshellCaptureModel;
const policy = model.normalizePolicy({
  ...model.defaultPolicy(),
  enabled: true,
  strip_query: true,
  excluded_origins: ["https://private.example/path"],
});
const accepted = model.sanitizeVisit(
  {
    source: "Firefox",
    visit_id: "visit-1",
    url: "https://example.net/research?token=secret#part",
    title: " Research ",
    favicon_url: null,
    referrer_url: "https://example.net/from?private=1#part",
    transition: "LINK",
    at_ms: 42,
    private: false,
  },
  policy,
);
assert.equal(accepted.url, "https://example.net/research");
assert.equal(accepted.referrer_url, "https://example.net/from");
assert.equal(accepted.title, "Research");
assert.equal(accepted.transition, "link");
assert.equal(
  model.sanitizeVisit(
    {
      ...accepted,
      url: "https://private.example/a",
    },
    policy,
  ),
  null,
);
assert.equal(model.sanitizeVisit({ ...accepted, private: true }, policy), null);
assert.deepEqual(
  model.mergeQueue([accepted], [{ ...accepted, title: "Updated" }]),
  [{ ...accepted, title: "Updated" }],
);
console.log("Graphshell capture policy smoke passed");

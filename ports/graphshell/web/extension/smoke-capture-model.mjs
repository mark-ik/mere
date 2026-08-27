// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
assert.deepEqual(
  model.historyFilterFromControls(7, " persona:a ", " device:b ", 700_000_000),
  {
    start_ms: 95_200_000,
    end_ms: null,
    persona: "persona:a",
    device: "device:b",
  },
);
assert.deepEqual(
  model.forgetRequest(
    "https://private.example/a?token=secret#part",
    true,
    policy,
  ),
  {
    url: "https://private.example/a",
    remove_object: true,
  },
  "an excluded origin stays forgettable after capture",
);
console.log("Graphshell capture policy smoke passed");

// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import "./action-form.js";

const action = {
  payload_schema: "cleromancy.intent.create-concurrence/v1",
  input_form: {
    schema: "cleromancy.intent.create-concurrence/v1",
    fields: [
      {
        name: "astrology_facts_digest",
        label: "Astrology facts",
        required: true,
        choices: [{ value: "facts-a", label: "August chart" }],
      },
      {
        name: "reading_session_id",
        label: "Reading session",
        required: true,
        choices: [{ value: "session-a", label: "Three-card reading" }],
      },
    ],
  },
};

assert.deepEqual(
  globalThis.GraphshellActionForm.composePayload(action, {
    astrology_facts_digest: "facts-a",
    reading_session_id: "session-a",
  }),
  {
    schema: "cleromancy.intent.create-concurrence/v1",
    astrology_facts_digest: "facts-a",
    reading_session_id: "session-a",
  },
);
assert.throws(
  () => globalThis.GraphshellActionForm.composePayload(action, { reading_session_id: "session-a" }),
  /Choose Astrology facts/,
);
assert.throws(
  () => globalThis.GraphshellActionForm.composePayload(action, {
    astrology_facts_digest: "invented",
    reading_session_id: "session-a",
  }),
  /not advertised/,
);
assert.throws(
  () => globalThis.GraphshellActionForm.composePayload({
    ...action,
    input_form: { ...action.input_form, schema: "wrong" },
  }, {
    astrology_facts_digest: "facts-a",
    reading_session_id: "session-a",
  }),
  /does not match/,
);
assert.throws(
  () => globalThis.GraphshellActionForm.composePayload({
    ...action,
    input_form: { ...action.input_form, fields: [null] },
  }, {}),
  /invalid field name/,
);

console.log("Graphshell bounded action form smoke passed");

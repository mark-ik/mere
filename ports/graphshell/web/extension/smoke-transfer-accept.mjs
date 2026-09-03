// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The accept action reaches the browser as an ordinary bounded form, so it
// needs no dedicated render arm. What this pins is the part that would fail
// silently: a form whose single advertised choice is the waiting transfer, so
// the submitted payload names which transfer was accepted.
//
// The shape below is what `PersonalSyncHost::supplemental_cards` composes.
// Its Rust counterpart asserts the host produces it; this asserts the bridge
// turns it into the payload the host expects back.
import assert from "node:assert/strict";
import "./action-form.js";

const TRANSFER = "3f6b1e28-0000-4000-8000-00000000ffee";
const SCHEMA = "graphshell.TransferAcceptIntent/v1";

const acceptAction = {
  intent: "graphshell.transfer.accept/v1",
  label: "Accept transfer",
  payload_schema: SCHEMA,
  input_form: {
    schema: SCHEMA,
    fields: [
      {
        name: "transfer_id",
        label: "Transfer",
        description: "Confirm which waiting transfer to bring onto this device.",
        required: true,
        choices: [{ value: TRANSFER, label: "2 object(s), 1 blob(s)" }],
      },
    ],
  },
};

const forms = globalThis.GraphshellActionForm;

assert.deepEqual(
  forms.composePayload(acceptAction, { transfer_id: TRANSFER }),
  { schema: SCHEMA, transfer_id: TRANSFER },
  "accepting composes a payload naming the transfer",
);

// A decision that names no transfer is ambiguous the moment two are waiting,
// so an empty selection must not compose.
assert.throws(
  () => forms.composePayload(acceptAction, {}),
  /Choose Transfer/,
  "an unselected transfer is refused rather than sent as an empty accept",
);

// The browser must not be able to accept a transfer the host did not offer on
// this card. Server-side the drain re-checks against the offers addressed to
// this device, but the client should not compose it in the first place.
assert.throws(
  () => forms.composePayload(acceptAction, { transfer_id: "some-other-transfer" }),
  /not advertised/,
  "an unadvertised transfer id is refused",
);

assert.throws(
  () => forms.composePayload(acceptAction, { transfer_id: TRANSFER, apply_now: "yes" }),
  /does not have a apply_now field/,
  "an unadvertised field is refused",
);

// A mismatch here would mean the host advertised a form for a different
// action, which the bridge must not paper over.
assert.throws(
  () =>
    forms.composePayload(
      { ...acceptAction, payload_schema: "graphshell.SomethingElse/v1" },
      { transfer_id: TRANSFER },
    ),
  /does not match the advertised payload/,
  "a form whose schema disagrees with its action is refused",
);

console.log("smoke-transfer-accept: ok");

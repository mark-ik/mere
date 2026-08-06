# G11 browser bounded action forms

**Date:** 2026-08-06
**Result:** the admitted browser bridge can render and submit an
endpoint-authored bounded `ActionFormV1`.

## Cut

The browser carrier already transports advertised endpoint actions, but its
bridge only understood Graphshell's private identity controls. It now accepts
an `input_form` on any other advertised action and renders one select for each
endpoint-authored field.

The page sends a payload only when:

- `input_form.schema` exactly equals `payload_schema`;
- every required field has a selected value;
- every selected value occurs in that field's advertised choices; and
- no unadvertised field is supplied.

The exact schema and values become the intent payload. The bridge supplies no
preselection, generated reading value, interpretation, or product-specific
field semantics. Existing native identity-import and signing controls stay on
their dedicated paths.

`action-form.js` owns the serialization rule so its behavior can be checked
without a browser DOM. `prepare-extension.ps1` copies it into both browser
packages.

## Evidence

```text
node --check ports/graphshell/web/extension/action-form.js
node --check ports/graphshell/web/extension/bridge.js
node ports/graphshell/web/extension/smoke-action-form.mjs
powershell -ExecutionPolicy Bypass -File \
  ports/graphshell/web/extension/prepare-extension.ps1 \
  -Browser chromium -Destination C:\\t\\graphshell-extension-action-form
```

The smoke test proves a Cleromancy-shaped two-field form produces the exact
schema-bearing payload, rejects a missing required choice, rejects an invented
choice, rejects schema disagreement, and fails safely on malformed fields.
The package check produced a Chromium extension directory containing
`action-form.js`.

## Boundary

This is a browser presentation and payload-composition capability, not a
Cleromancy browser carrier. `graphshell_device_host` remains the resident
owner of Personae authority. A Cleromancy endpoint still needs a resident
adapter that receives an admitted browser session, maps it to Cleromancy's
local endpoint session, and binds the admitted subject to Servitor scope.
Starting a second native host that opens the same vault, or adding a Mere to
Cleromancy dependency, would violate that ownership boundary.

## Stop rule

Do not add product-specific form defaults or interpretation here. The next
integration gate is an explicit resident endpoint-adapter contract with an
admitted-session and subject handoff.

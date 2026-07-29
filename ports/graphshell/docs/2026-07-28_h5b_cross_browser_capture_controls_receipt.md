# Graphshell H5b cross-browser capture and controls receipt

Date: 2026-07-28

Status: H5 complete.

## Product result

The packaged Graphshell portal now owns the full consented browser-history
loop:

- optional history permission requested only from the user's Enable action;
- bounded import and live `onVisited` delivery in Chromium and Firefox;
- query and fragment redaction before the extension queue;
- one atomic graph, AccessRecord, and browsing-trace commit before queue
  acknowledgement;
- IndexedDB reopen and source-event dedupe across browser restart;
- authority filtering by time, persona, and device;
- scoped forgetting, with an explicit option to remove a graph object created
  solely by capture;
- visible reference-host attribution for every capture.

Adding an origin to the intake exclusion list does not make old records for
that origin impossible to erase. Intake exclusions and erasure authority are
separate.

## Headed receipts

Chromium used the stable unpacked-extension id in a fresh profile. The user
granted `history`. A bounded import stripped its query and fragment, grew the
graph from eleven to twelve objects, drained the queue, and survived a cold
restart with the permission and IndexedDB document intact.

A second fresh Chromium profile selected live-only intake. A successful HTTP
navigation entered through the real `history.onVisited` listener. The extension
queue reached one, Graphshell accepted one redacted visit, the queue returned
to zero, and a cold restart reopened the same twelve-object graph and one
authority result.

The headed control surface filtered one stored visit to zero under a mismatched
persona, restored it under the matching persona and device, then forgot its
trace and capture-created object. The graph returned from twelve objects to
eleven.

Firefox installed the exact rebuilt package as the temporary extension
`graphshell@mere.systems`. The user granted Firefox's own optional history
prompt. A one-day import produced one redacted authority result. A later live
navigation produced a second result through `onVisited`, with IndexedDB
reported as reopened.

All isolated Chromium profiles, the Firefox XPI and Selenium receipt script,
and the loopback receipt server were removed after the checks.

## Verification

- `cargo test -p graphshell --all-features --offline`: 76 passed.
- `cargo test -p graphshell --no-default-features --features web --offline`:
  15 passed.
- `cargo test -p mere-eidetic --offline`: 88 passed.
- `cargo check -p graphshell-web --target wasm32-unknown-unknown --offline`
  passed.
- The capture-model and background-worker smokes passed, including queue
  dedupe, redaction, permission state, transition/referrer recovery, and
  acknowledgement.
- The port-boundary checker, manifest parsing, JavaScript syntax checks,
  package-script parsing, Graphshell package formatting, and scoped
  `git diff --check` passed.

## Personae boundary

`MereHost` receives a `SelectedPersonaRef` from its composing application.
That is the correct capture-attribution seam. The native Personae surface
selects a vault profile and exposes public carry personas and devices, but it
does not define which persona and device the user is currently browsing as.
Inferring that choice from a profile or roster entry would invent authority.

This reference host therefore displays and records its stable injected
reference persona and device. A real Graphshell consumer must inject its live
application selection through the same seam. That integration is a second-host
proof, not an H5 browser-capture requirement.

## Deferred richer capture

Favicon capture remains represented in the event and facet schemas but is not
requested. It needs a separate visible opt-in for the optional `tabs`
permission and its retention cost. The release package also needs an optimized
Wasm build; the headed receipts intentionally used the current debug artifact.

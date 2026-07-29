# Graphshell H5a browser storage and capture-core receipt

Date: 2026-07-28

Status: bounded H5 slice complete. The later
[H5b receipt](2026-07-28_h5b_cross_browser_capture_controls_receipt.md)
closes H5.

## What landed

Graphshell now owns a browser-local `muniment::Backend` over IndexedDB. The
Wasm host seeds its Mere document once and reopens the same document on later
loads. The adapter implements the complete byte-store contract, including one
IndexedDB transaction for `apply`.

The capture core accepts browser visits only through an explicit,
disabled-by-default policy. Before persistence it:

- rejects private, internal, unsupported, and excluded addresses;
- strips fragments by default and optionally strips query strings;
- deduplicates stable browser visit ids across process restart;
- records title, optional favicon, transition, referrer, source, persona,
  device, and observation time;
- appends a LocalOnly typed `graphshell.AccessRecord/v1` Eidetic record;
- updates the derived access and browser-history facets and traversal graph;
- flushes browsing-memory segments before acknowledging the batch;
- applies a configurable trace quota.

Access records can be filtered from the Eidetic authority by half-open time
range, persona, and device. Forgetting an address deletes every browsing trace
and AccessRecord manifest that mentions it, clears the derived facet, and can
remove a graph object created solely by capture. Content blobs remain eligible
for a later reachability GC; deleting a manifest does not pretend to erase a
shared blob immediately.

The extension package now includes the Wasm graph portal beside Identity. Its
required permissions remain `nativeMessaging` and `storage`. `history`,
`tabs`, and `webNavigation` are optional. The current capture surface requests
only `history`, directly from an Enable button. It offers live-only or bounded
one-time import, query-string stripping, origin exclusions, explicit stop, and
permission removal.

The service worker sanitizes addresses before placing them in its bounded
`storage.local` delivery queue. Graphshell acknowledges queue entries only
after the Wasm host has projected and persisted them into IndexedDB and
Eidetic. A suspended worker or failed graph load therefore leaves the batch
available for retry.

## Evidence

- `cargo test -p graphshell --all-features --offline` passes 76 tests. These
  include the capture policy, graph projection, LocalOnly authority, time /
  persona / device filter, atomic-failure retry, restart dedupe, forget, and
  post-forget recapture scenarios.
- `cargo test -p mere-eidetic --offline` passes 88 unit tests and preserves
  the native pack-signing and JSON Schema defaults.
- `cargo check -p graphshell-web --target wasm32-unknown-unknown --offline`
  passes with Eidetic's native-only signing and full JSON Schema cones omitted.
- `smoke-capture-model.mjs` proves pre-queue redaction, origin exclusion,
  private-event refusal, and stable queue dedupe.
- `smoke-capture-background.mjs` proves optional-permission state, bounded
  import, transition and referrer recovery, sanitized queue delivery, and
  acknowledgement.
- A real HTTP-origin headed browser load reported `IndexedDB seeded`, then
  `IndexedDB reopened` after reload with the same eleven-node graph.
- The rebuilt Chromium package loaded in a fresh isolated profile, started its
  MV3 service worker under the stable extension id, opened the Wasm graph
  portal, reported capture off, and produced the real browser-history
  permission prompt after the explicit Enable action.
- The user accepted that prompt. Chromium persisted the optional `history`
  grant, imported the isolated profile's synthetic local history, stripped its
  query and fragment before persistence, and acknowledged the delivery only
  after the queue reached zero. The graph grew from eleven to twelve nodes.
- The persisted authority contained a LocalOnly
  `graphshell.AccessRecord/v1` and browsing trace for
  `http://127.0.0.1:8765/`, with the original query absent.
- A cold Chromium restart with the same profile retained the permission,
  reopened IndexedDB at twelve nodes, and kept the delivery queue empty. The
  isolated test profile was then removed.

## Walls left open by H5a

This was not the H5 completion receipt. The later H5b receipt closes the first
three walls below.

- The Chromium permission, bounded import, redaction, acknowledgement, graph
  persistence, and cold-restart clauses are closed. Live `onVisited` intake
  still needs a headed receipt.
- The same packaged-extension receipt remains open on Firefox.
- Filter and forget are proved authority operations but do not yet have their
  final graph-portal controls.
- Favicon intake remains optional in the event shape but the extension does
  not request `tabs` or store favicons yet. This is a richer capture feature,
  not an H5 done condition.
- The portal labels captures with the persona and device injected by its
  composing host. A second host still needs to prove live selection injection;
  Graphshell does not infer that authority from the Personae vault profile.

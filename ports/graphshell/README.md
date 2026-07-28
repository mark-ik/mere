# Graphshell

Graphshell is Mere's local-first reference host and projection portal.
Applications retain authority over mounted domains; Graphshell also grows its
own local Mere graph through the reference-host plan.

## Current boundary

The reusable session stack lives in [`crates/graphshell`](../../crates/graphshell):

- `graphshell-protocol` carries versioned score, epoch-preserving scene,
  presentation, resume, status, and intent messages over an unspecified
  carrier.
- `graphshell-client` keeps endpoint-scoped snapshots, applies transactional
  diffs and resume replies, and persists only when session policy permits it.
- `graphshell-endpoint` defines injected projection and intent traits for
  applications to implement beside their own truth.
- `graphshell-stdio` provides the first local carrier: a newline-delimited JSON
  process boundary for discovery, snapshots, resources, resume, and intents.
This port is the `graphshell` presentation host. Its native receipt view can
place resolved presentations at disclosed Scenograph origins, draw disclosed
relations, and collapse to a semantic card stack on narrow screens.

## Build profiles

The package has two explicit capability profiles:

- `native` is the default. It contains the existing admitted sessions,
  Personae composition, native transports, stdio carrier, and receipt binaries.
- `web` selects Mere's portable graph + canvas facade and leaves native
  admission, transport, Tokio, Personae vault, and receipt binaries outside the
  WASM dependency cone.

H0 proves the web cone. H1 adds the local graph-host adapter inside that cone:

```powershell
$env:CARGO_TARGET_DIR = 'target-plan-graphshell'
cargo check -p graphshell-protocol -p graphshell-client --target wasm32-unknown-unknown
cargo check -p mere-canvas --target wasm32-unknown-unknown
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web
python scripts/check_port_boundaries.py
```

The `web` profile now exposes:

- a local Mere graph plus its unknown-forward facet store;
- Muniment persistence through an injected backend;
- an in-process Graphshell endpoint with portable cards and typed open intents;
- user-configurable handler offers;
- local and remote projections mounted through one `ClientState`;
- public Personae references while vault authority remains native.

The H1 fixture exercises web, custom-protocol, and file addresses; saved scene
and remote-mount facets; several relation families; two-device access history;
synthetic public identity projections; and a foreign facet namespace. Its open
intent mutates access history, persists, reopens, and re-saves the unchanged
graph/facet boundary byte-equivalently:

```powershell
$env:CARGO_TARGET_DIR = 'target-plan-graphshell'
cargo test -p graphshell --no-default-features --features web
```

H2 adds the real browser presenter as a separate `graphshell-web` workspace
package. It composes the H1 host with Mere Canvas, Cambium over Genet's neutral
DOM/layout seam, and NetRender over an asynchronous WebGPU canvas. The
browser-only stack does not enter the portable `graphshell` crate.

```powershell
$env:CARGO_TARGET_DIR = 'target-plan-graphshell-web'
cargo build -p graphshell-web --offline --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ports/graphshell/web/pkg `
  target-plan-graphshell-web/wasm32-unknown-unknown/debug/graphshell_web.wasm
python -m http.server 8765 --bind 127.0.0.1 --directory ports/graphshell/web
```

Headed Chromium and Firefox receipts cover pan, zoom, selection, persistent
node drag, detail, local/remote session switching, and advertised action
invocation at wide and narrow sizes. See the
[H2 receipt](docs/2026-07-27_h2_browser_presenter_receipt.md) and its
[screenshots plus semantic tree](docs/receipts/h2_browser_receipts.json).
Canvas2D was not needed.

H3 turns that presenter into the first standalone graph product cut. The
browser can create addressed and content-addressed file objects, edit graph
metadata and relations, filter and arrange the graph, save and reopen scenes,
choose representations and open handlers, and round-trip an explicitly scoped
graph engram. Device-local file metadata stays out of transfer unless the user
opts in. See the
[H3 local graph product receipt](docs/2026-07-28_h3_local_graph_product_receipt.md)
and its [headed browser record](docs/receipts/h3_browser_receipts.json).

The standalone native canvas presenter is similarly explicit:

```powershell
cargo run -p mere-canvas --features native-present --bin canvas
```

The portable crates may depend on Scenograph contracts, serialization, and
content-addressing primitives. They must not depend on Mere, Turnstone, Isometry,
Genet, Cambium, NetRender, a network runtime, or an application model. Product
adapters depend on `graphshell-endpoint` in the other direction.

## G1, G2, and the first product endpoint

G1 keeps presentation outside `sceno::Scene`. A snapshot carries a Graphshell
sidecar manifest that binds scene instances to ordered, versioned resource
offers. Resource bytes are fetched separately, verified by content hash, and
cached within the disclosing session.

The deterministic fixture proves two capability profiles over one scene:

- rich: portable card plus content-addressed image;
- compact: native glyph plus a labeled image placeholder;
- both: the same advertised actions in the accessibility projection.

Run the proof wall:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
cargo run -p graphshell --bin g1_receipt -- ports/graphshell/docs/receipts/g1_loopback.html
```

The committed [G1 receipt](docs/receipts/g1_loopback.html) is compared
byte-for-byte with fresh output by the test suite.

G2 adds stable scene epochs and revisions through Scenotime. The client applies
scene, presentation-resource, and status changes together; retains stale or
disconnected scenes; acknowledges revisions; and resumes from replay or a full
epoch-preserving snapshot. Persisted caches use an injected store and require
the protection promised by the session's cache policy.

The deterministic resume fixture disconnects after revision 2, replays
revision 3, and reaches the same scene as the endpoint's complete snapshot.
Its removed item remains a tombstone at slot 0 while later items stay at slots
1 and 2. See the [G2 receipt note](docs/2026-07-22_g2_diff_resume_receipt.md).

G3 lives in Turnstone, in the required dependency direction. Its endpoint reads
live Mere graph truth through Mere cartography, returns the resulting score,
scene, routed relations, and content-addressed card offers, and maps advertised
intents back through Turnstone's Servitor gate. Graphshell gains only the generic
spatial receipt view. The portable Graphshell crates still have no Mere or
Turnstone dependency; this application port selects Mere explicitly in its
`web` profile.

The portable stack was published on 2026-07-22 as the active Graphshell tree.
It joined Mere on 2026-07-23, and the reference application moved under
`ports/` on 2026-07-24. The retired browser donor remains intact in Mere's Git
history rather than appearing as current source or documentation.

## G4 local sessions

Graphshell can now discover and mount projections from arbitrary local endpoint
processes. The `g4_sessions` host has no product dependency: it asks each
endpoint for its catalog, mounts every advertised projection through the same
client state machine, resolves resources, invokes advertised actions, and puts
the resulting sessions behind keyboard-reachable tabs.

The committed [G4 receipt](docs/receipts/g4_session_switch.html) was generated
from the Turnstone browsing endpoint and Isometry's player-overmap and tile-board
endpoints. It proves three independently owned projections through one
Graphshell binary. The [receipt note](docs/2026-07-22_g4_cross_product_receipt.md)
records the commands and acceptance boundary.

This carrier is deliberately local and unauthenticated. Identity, negotiated
grants, revocation, reconnect, and cross-device transport belong to G5.

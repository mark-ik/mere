# Graphshell

Graphshell is a remote, local-first projection host. Applications retain their
own truth and authority; Graphshell stores only curation state, disclosed
scenes, and presentation preferences.

## Current boundary

The workspace is intentionally portable:

- `graphshell-protocol` carries versioned score, epoch-preserving scene,
  presentation, resume, status, and intent messages over an unspecified
  carrier.
- `graphshell-client` keeps endpoint-scoped snapshots, applies transactional
  diffs and resume replies, and persists only when session policy permits it.
- `graphshell-endpoint` defines injected projection and intent traits for
  applications to implement beside their own truth.
- `graphshell-stdio` provides the first local carrier: a newline-delimited JSON
  process boundary for discovery, snapshots, resources, resume, and intents.
- `graphshell` is the presentation host. Its native receipt view can place
  resolved presentations at disclosed Scenograph origins, draw disclosed
  relations, and collapse to a semantic card stack on narrow screens.

The portable crates may depend on Scenograph contracts, serialization, and
content-addressing primitives. They must not depend on Mere, Merecat, Isometry,
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
cargo run -p graphshell --bin g1_receipt -- docs/receipts/g1_loopback.html
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

G3 lives in Merecat, in the required dependency direction. Its endpoint reads
live Mere graph truth through Mere cartography, returns the resulting score,
scene, routed relations, and content-addressed card offers, and maps advertised
intents back through Merecat's Servitor gate. Graphshell gains only the generic
spatial receipt view; this repository still has no Mere or Merecat dependency.

The portable workspace was published on 2026-07-22 as the active Graphshell
tree. The retired browser donor remains intact in this repository's Git
history rather than appearing as current source or documentation.

## G4 local sessions

Graphshell can now discover and mount projections from arbitrary local endpoint
processes. The `g4_sessions` host has no product dependency: it asks each
endpoint for its catalog, mounts every advertised projection through the same
client state machine, resolves resources, invokes advertised actions, and puts
the resulting sessions behind keyboard-reachable tabs.

The committed [G4 receipt](docs/receipts/g4_session_switch.html) was generated
from the Merecat browsing endpoint and Isometry's player-overmap and tile-board
endpoints. It proves three independently owned projections through one
Graphshell binary. The [receipt note](docs/2026-07-22_g4_cross_product_receipt.md)
records the commands and acceptance boundary.

This carrier is deliberately local and unauthenticated. Identity, negotiated
grants, revocation, reconnect, and cross-device transport belong to G5.

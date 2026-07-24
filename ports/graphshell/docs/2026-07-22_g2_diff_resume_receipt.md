# G2 diff and resume receipt

**Status:** complete locally on 2026-07-22. Publication still waits for the
archived donor rename required by G0.

## What this proves

- Scenotime supplies typed epochs, revisions, stable slot tables, serialized
  tombstones, and transactional idempotent scene diffs.
- Graphshell carries presentation-resource and session-status changes beside
  the scene diff and commits the combined change only after validation.
- A missing base leaves the last acknowledged display intact and yields an
  explicit resynchronization request.
- The loopback endpoint disconnects after revision 2 and replays revision 3.
  The client converges with the endpoint's complete snapshot with two active
  items at slots 1 and 2 and the removed item retained as a tombstone at slot
  0. Duplicate replay and a current acknowledgement are idempotent.
- An unavailable history falls back to a full snapshot that preserves the
  epoch's stable slots.
- A permitted scene and its advertised content-addressed resource survive an
  injected encrypted-at-rest store and restore as stale. Memory-only policy
  refuses persistence. Graphshell relies on the injected store's protection
  contract; it does not implement storage encryption itself.
- Scenotime's deterministic 96-revision randomized oracle reaches the same
  item and order tables as an independently maintained final-state model.

## Acceptance

- Scenograph: `cargo test --workspace`
- Scenograph: `cargo check --workspace --target wasm32-unknown-unknown`
- Scenograph: `cargo clippy --workspace --all-targets -- -D warnings`
- Graphshell: `cargo test --workspace`
- Graphshell: `cargo check --workspace --target wasm32-unknown-unknown`
- Graphshell: `cargo clippy --workspace --all-targets -- -D warnings`
- Graphshell's existing G1 HTML receipt remains byte-identical under the new
  epoch/revision snapshot contract.
- The normal-dependency tree contains Graphshell, Sceno/Scenotime, serde,
  BLAKE3, and the G1 base64 view helper. It contains no product, renderer,
  carrier, or application-model dependency.

## Deliberate limits

This is still an in-memory loopback source. There is no authenticated carrier,
Merecat adapter, queued offline intent replay, live-pane codec, or durable host
store implementation. G3 is the first real product endpoint and headed
application proof.

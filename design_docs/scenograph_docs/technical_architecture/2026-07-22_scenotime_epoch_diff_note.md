# Scenotime Epoch and Diff Note

**Date:** 2026-07-22  
**Status:** landed and verified locally as the Scenograph half of Graphshell G2.

## Contract

`SceneSnapshot` wraps a dense `sceno::Scene` in an explicit `SceneEpoch` and
`Revision`. Its source, space, item, relation, and region tables use stable
slots. Removing a value leaves a serialized tombstone; additions append and
cannot reuse an index during the same epoch. Item order is a separate field,
so visual order can change without changing identity.

`SceneDiff` is one transactional revision transition. It can append, update,
or tombstone stable slots and change item layer/order, bounds, or generation.
The whole result is validated before it replaces the prior snapshot. Repeated
or older revisions are idempotent successful no-ops. A wrong epoch or missing
base remains explicit so a host can request replay or a full snapshot.

Presentation resources, session status, carriers, persistence, and source
authority remain outside Scenotime. Graphshell composes those beside the scene
diff rather than teaching Scenograph about remote applications.

## Receipts

- Seven Scenotime tests cover tombstones, idempotence, missing bases, epoch
  changes, transaction rollback, explicit item order/layer, serde round trips,
  and a deterministic 96-revision randomized oracle.
- `cargo test --workspace` passes across 13 Sceno, 4 Scenomise, and 7
  Scenotime tests.
- `cargo check --workspace --target wasm32-unknown-unknown` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.

## Remaining runtime work

Scenotime does not yet schedule incremental signal evaluation, maintain
derived caches, or resolve hits into authorized intents. Those belong to
later consumer proofs. G2 establishes only the portable identity and history
contract needed for safe remote replay.

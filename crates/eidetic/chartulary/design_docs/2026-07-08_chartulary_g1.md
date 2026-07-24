# chartulary G1: the edit spine

**Date:** 2026-07-08
**Status:** G1 landed. Follows G0 (`2026-07-08_chartulary_g0.md`). Canonical program
plan in mere: `design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`.

## What G1 is

The graph becomes event-sourced: mutations are logged edits, and the graph is the
replay. Done-condition met (13 tests): `replay(log)` equals the live graph, and a
checkpoint plus the tail of the log equals a full replay.

- **`GraphEdit<N, E>`** (`edit.rs`): `InsertNode(N)`, `RemoveNode(N::Id)`,
  `Connect { id, from, to, edge }`, `Disconnect(EdgeId)`. Edits reference nodes by
  **stable identity**, never by an ephemeral graph key, so replay into a fresh
  graph reconstructs the same result.
- **`EdgeId`**: a stable edge identity assigned at connect time and carried in the
  edit, so a specific edge can be retracted across replay. Nodes already had stable
  identity (`Identified`); edges did not, and the spine needs it for `Disconnect`.
- **`GraphLog<N, E>`** (`spine.rs`): the write side. Every mutation builds an edit,
  applies it, and appends it to a `Codicil`. Live editing and replay share one
  `apply_edit`, so they cannot diverge. It maintains the materialized graph plus an
  `EdgeId`-to-key index (both directions, so a node removal reaps the edge ids of
  its incident edges).
- **Snapshot + tail load**: `snapshot` writes the current state to a muniment slot
  as a compacted materialization (nodes, edges with their ids, `next_edge`, and the
  log length it reflects). `load_checkpointed` reads it and replays only the tail;
  `load_full` replays everything. Both yield equal graphs.

## Decisions made in G1

- **Edges get a stable id, minted by the log.** The alternative (identify an edge
  structurally by endpoints) is ambiguous under the multigraph semantics chosen at
  G0. `EdgeId` is a `u64` the `GraphLog` assigns monotonically; because it is
  carried in the `Connect` edit, replay reuses it and `next_edge` restores to
  `max(seen) + 1`.
- **The snapshot is a compacted materialization, not a serialized petgraph.** It
  stores nodes and `(EdgeId, from_id, to_id, edge)` tuples and rebuilds the graph
  by re-inserting and re-connecting on load. This deliberately does not depend on
  petgraph preserving internal edge indices across serde: identities are stable,
  keys are not, and the spine only ever trusts identities. A snapshot is therefore
  a minimal insert+connect prefix, which is exactly a compacted log.
- **One `apply_edit` for both paths.** The property "replay equals live" holds by
  construction because there is a single mutation function; the tests assert it
  rather than the design merely implying it.
- **Node removal reaps edge bookkeeping.** Removing a node drops its incident edges
  in petgraph; the spine removes their `EdgeId` entries in the same step (via
  `Graph::incident_edges`), so a freed key cannot later alias a reused one.
- **Persistence is async**, riding muniment's `?Send` seam; the in-memory edit and
  replay paths are sync.

## Not yet (later phases)

Incremental persistence (append only the tail past the last saved `Seq`) and
compaction policy are codicil-roadmap items, noted but not built. G2 is stemma
(node lineage as a projection over this spine); the fork primitive and provenance
header land there. G4 is scholia (the RDF projection over the semantic ring). G5
is mere's re-base.

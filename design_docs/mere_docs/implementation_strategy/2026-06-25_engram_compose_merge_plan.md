# Engram compose / merge

**Date**: 2026-06-25
**Status**: **P1+P2 done 2026-06-27** (snapshot-level `merge_snapshots` + `compose_graph_engrams`, 6 tests
green); **P3 host gesture deferred** (chrome-hot). Spun out of the
[Alembic tail handoff](2026-06-25_alembic_tail_and_audit_polish_handoff.md) B7; owns the "compose
sub-feature" that [decision #1](2026-06-24_alembic_implementation_plan.md) defers to post-A/D.
Architecture: [alembic memory + engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md).

## Goal

Union two graph engrams into a new engram, **retaining per-member provenance**. Merge-by-identity (URL
identity), layer the context. An Athanor per-compose option (R0: Athanor proposes, the host applies).
Post-A/D; **does not gate save/open** (decision #1, agreed with Mark).

## Locked decision (#1)

> Merge semantics (compose): merge-by-identity, layer the context (`import_provenance` is a `Vec`, so
> source records coexist); exposed as an Athanor per-compose option. Lands with the compose sub-feature
> (post-A/D); does not gate save/open.

## Prior art (read against the code)

- **The spine exists** — `session-runtime/graph_engram.rs`: `GraphEngram(GraphSnapshot)`, freeze
  (`save_graph_snapshot_engram`), thaw (`open_engram_as_session`), list. `ProvenanceRecord.upstream` is a
  `Vec` already, **currently empty** (`graph_engram_provenance` sets `upstream: Vec::new()` with the
  comment "composition / merge fills `upstream` later"). That is exactly the seam B7 fills.
- **`cross_graph.rs::copy_component_from` is the wrong shape, kept as reference.** It mints *fresh* nodes
  (a `CopiedFrom` derivation) and **deliberately drops** the donor `import_provenance` (the tear-out fork
  G4). B7 is the inverse: same-URL nodes **layer** (no fresh mint), and provenance is **retained +
  appended**, not dropped. Reuse its edge-repointing/component mechanics, not its node-copy semantics.
- **Per-node provenance is a `Vec`, but record-derived (audit).** `NodeImportProvenance { source_id,
  source_label }` (`types.rs:116`) on `node.import_provenance` — so it *can* name a source engram. But it
  is **synced from** the snapshot's `import_records` (`sync_node_import_provenance_from_records` rebuilds
  the node field, `import_records.rs:166`), so setting `node.import_provenance` directly is transient.
  Compose must union the snapshot's `import_records: Vec<ImportRecord>` (the durable source); those then
  sync to the node field, and both sources coexist.

## Plan

**P1 — the merge primitive (snapshot-level, in `session-runtime/graph_engram.rs`). Audit verdict: no
kernel edit needed.** `PersistedEdge` keys its endpoints by `from_node_id` / `to_node_id` (stable
`String`s, `persistence_edge.rs:400`), not petgraph indices, so `merge_snapshots(a, b) -> GraphSnapshot`
manipulates the plain `GraphSnapshot` Vecs directly — no `Graph::add_edge` / crate-internal `inner`, so it
sidesteps the hot kernel entirely. By URL identity:

- **Node-id remap.** Same-url nodes in A and B carry *different* `node_id`s; pick A's as canonical and
  build `b_node_id -> a_node_id` for the overlap. Same-url node → layer: union `tags`, merge `properties`
  (conflict policy below). Url in B only → add the node, keep its id.
- **Edges.** Rewrite every B edge's `from_node_id`/`to_node_id` through the remap, then union into A,
  deduped by `(from_node_id, to_node_id, families + sub-kind)`. Note `PersistedEdge.families` is a
  `Vec<PersistedEdgeFamily>` with per-family `Option` data — dedup merges families onto a shared endpoint
  pair, it does not treat each family as a separate edge.
- **The rest of the snapshot (audit gap — first draft missed these).** `GraphSnapshot` also carries
  `import_records`, `fields`, `couplings`, and `navigation`. Compose must **union `import_records`** (the
  durable provenance, remapped to canonical ids — this is what carries decision #1's per-member provenance,
  *not* the derived node field) and pick a documented policy for `fields`/`couplings`/`navigation`: first
  cut carry A's and **log that B's were dropped** (no silent loss); union is a later refinement once
  field-overlap collisions have a rule.
- Returns a merge report (added / layered / dropped counts) for the Athanor proposal.
- **Conflict policy** for a scalar property present in both with different values: default keep source A,
  record the divergence in provenance (non-destructive; matches the layer-not-overwrite spirit).

**P2 — the engram compose op (session-runtime/graph_engram).** `compose_graph_engrams(store, ids, redaction,
created_at) -> ManifestId`: thaw each id → `merge_from` into an accumulator → save the merged snapshot via
a variant of `save_graph_snapshot_engram` whose `ProvenanceRecord.upstream = ids` (so the new engram's
lineage records its sources — the empty `upstream` finally populated). Reuses the existing redaction + typed
save. Round-trip test: compose two known engrams, thaw the result, assert the union + that `upstream`
carries both source ids + that a shared-URL node carries both provenance records.

**P3 — Athanor option + host gesture.** Expose compose as an Athanor per-compose option (the propose/apply
actor; R0 proposes, host applies — same shape as the forgetting pass). Thinnest UI first: a
`>compose_engrams("<id-a>", "<id-b>")` omnibar verb routing to a `ShellCommand` (mirroring
`OpenEngramBeside`), then a two-select gesture in the Alembic **Engrams** section (the engram rows are
already clickable; add a "compose selected" affordance). UI is the least load-bearing part and can lag the
primitive.

## Phasing / done conditions

- P1 done: `merge_snapshots` unions two snapshots by URL identity, layers same-URL nodes, unions
  `import_records` (provenance), dedups multi-family edges, carries A's `fields`/`couplings`/`navigation`
  with a drop-log; unit-tested in `session-runtime` (no kernel touch).
- P2 done: `compose_graph_engrams` produces a new engram whose `upstream` is `[id_a, id_b]` and whose
  shared-URL nodes carry both sources' provenance; round-trip-tested.
- P3 done: compose reachable from the host (verb first), proposed via Athanor, applied by the host.

## Gotchas

- **Not kernel-hot after all (audit).** P1 lives in `session-runtime/graph_engram.rs` on the public
  `GraphSnapshot` structs, so it does *not* touch `graph-kernel` and does *not* collide with the
  `mere-orrery → glossary` rename. A kernel `Graph::merge_from` would be cleaner (reuses the URL index +
  edge API) but is collision-prone now and unnecessary — the snapshot-level merge is the unblocked path;
  revisit promoting it into the kernel later, when the kernel is quiet.
- The merge is at the **snapshot** level, not the event-log level; it composes *states*, not histories.
  (History composition is slice E / C1's territory, not this.)

## Progress

- 2026-06-25: Drafted from the handoff B7 + decision #1, verified against `graph_engram.rs`,
  `cross_graph.rs`, and the `import_provenance` types.
- 2026-06-25: **Audited against the code.** Feasibility **green** — `PersistedEdge` is `node_id`-keyed
  (`persistence_edge.rs:400`), so the merge is a pure `GraphSnapshot` operation in `session-runtime`,
  non-colliding (the kernel-hot gotcha is removed). Three under-specifications found and folded in:
  (1) the snapshot also carries `import_records`/`fields`/`couplings`/`navigation` the merge must handle;
  (2) provenance is record-derived (`sync_node_import_provenance_from_records`), so union `import_records`,
  not just the node field; (3) edge dedup must respect the multi-family `PersistedEdge` container. Ready to
  implement.
- 2026-06-27: **P1+P2 implemented** in `session-runtime` (the non-colliding snapshot-level path).
  `snapshot_merge::merge_snapshots` (+ `MergeReport`) and `graph_engram::compose_graph_engrams` (`Derived`
  origin, `upstream` = source ids). All three audit gaps handled. 6 tests green (94 in the crate). Landed
  via a concurrent bare commit (`831bdcf`). **P3 (host `>compose_engrams` verb + Alembic two-select
  gesture) deferred** — meerkat chrome, picked up when that lane is quiet.

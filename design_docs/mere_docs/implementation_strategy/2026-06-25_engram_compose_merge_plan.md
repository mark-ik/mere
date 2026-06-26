# Engram compose / merge

**Date**: 2026-06-25
**Status**: Planned. Spun out of the [Alembic tail handoff](2026-06-25_alembic_tail_and_audit_polish_handoff.md)
B7; owns the "compose sub-feature" that [decision #1](2026-06-24_alembic_implementation_plan.md) defers
to post-A/D. Architecture: [alembic memory + engrams](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md).

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
- **Per-node provenance is a `Vec`** — `node.import_provenance: Vec<NodeImportProvenance>`
  (`graph-kernel/.../node.rs`), set via `set_node_import_provenance` / synced from `ImportRecord`s
  (`import_records.rs`). Layering = appending the source records so they coexist.

## Plan

**P1 — the merge primitive (graph-kernel).** A `Graph::merge_from(&other, source_tag)` (or
`merge_graph_snapshots(a, b)` at the snapshot level — decide where; the Graph level reuses the URL index).
By URL identity:
- **Same URL in both** → layer: union `tags`, merge `properties` (conflict policy below), and **append**
  the other node's `import_provenance` records so both sources coexist (decision #1).
- **URL in one only** → add it, carrying its provenance.
- **Edges** → union, deduped by `(from, to, family/sub-kind)` so a relation asserted in both is not
  doubled; clone the payload like `copy_component_from` does.
- Returns a merge report (added / layered counts) for the Athanor proposal.
- **Open: conflict policy** for a scalar property present in both with different values — keep-both
  (provenance-tagged), last-writer (newer `created_at`), or source-A-wins. Default: keep source A, record
  the divergence in provenance (non-destructive, matches the layer-not-overwrite spirit).

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

- P1 done: `merge_from` unions two graphs by URL identity, layers same-URL nodes, retains + appends
  provenance, dedups edges; unit-tested at the kernel level.
- P2 done: `compose_graph_engrams` produces a new engram whose `upstream` is `[id_a, id_b]` and whose
  shared-URL nodes carry both sources' provenance; round-trip-tested.
- P3 done: compose reachable from the host (verb first), proposed via Athanor, applied by the host.

## Gotchas

- **Kernel-hot.** P1 lands in `graph-kernel` (`cross_graph.rs` neighborhood) — coordinate with concurrent
  kernel work (the `mere-orrery → glossary` rename was staged 2026-06-25).
- The merge is at the **snapshot/graph** level, not the event-log level; it composes *states*, not
  histories. (History composition is slice E / C1's territory, not this.)

## Progress

- 2026-06-25: Drafted from the handoff B7 + decision #1, verified against `graph_engram.rs`,
  `cross_graph.rs`, and the `import_provenance` types. Not started.

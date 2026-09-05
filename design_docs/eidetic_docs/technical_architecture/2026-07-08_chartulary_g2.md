# chartulary G2: fork and lineage

**Date:** 2026-07-08
**Status:** G2 landed. Follows G1 (`2026-07-08_chartulary_g1.md`). The lineage crate
`stemma` was founded separately (now `crates/eidetic/chartulary/src/stemma`, G2a). This doc records the
fork-and-derivation half (G2b) that lives in chartulary and codicil. Canonical
program plan in mere:
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`.

## What G2 is

The graph gains lineage: a graph can be forked into a self-contained copy that
knows where it came from, and individual nodes can record that they were derived
from another graph's node. Done-condition met (15 tests): a forked graph carries
lineage across the fork, and derivation records survive a round-trip.

- **codicil fork (now `crates/eidetic/eidetic-core/src/codicil.rs`).** A `Codicil` can carry a `LogId` and, if it is
  a fork, a `Provenance { source, at }`. `fork(new_id)` copies the current entries,
  stamps provenance (source log id + the seq at the fork point), and diverges. This
  is git-style branching over the append-only log.
- **`GraphLog::fork`.** Forks the underlying log and replays it, producing a
  self-contained forked graph that carries the provenance header (`provenance()`).
  Because it replays, the fork rebuilds its own graph, edge index, and derivation
  map; diverging edits do not touch the source.
- **Per-node derivation.** `GraphEdit::Derive { node, from }` records a
  `DerivationRecord { source_log, source_node, kind }` (kind: `CopiedFrom`,
  `ClippedFrom`, `GeneratedFrom`, `TranslatedFrom`). Because it is an edit, it lives
  in the log and replays, so `derivations(node)` survives full and checkpointed
  loads. This is the node-level provenance that tracks a duplicate across graphs
  rather than deduplicating it.

## Two provenance concepts, deliberately distinct

- **Whole-graph fork provenance** lives on the *log* (codicil's `Provenance`
  header): this entire graph branched from source at seq N.
- **Per-node derivation** lives in the *graph* (chartulary's `DerivationRecord`):
  this one node is a copy of a node in another graph, for cross-graph copy / tear-out.

They compose: a fork copies everything and records one header; a tear-out copies one
node and records one derivation.

## What is deferred, and why

The plan's G2 also names "wire stemma as a projection fed by the spine." Whether the
spine auto-emits a visit-shaped event per edit, or wiring is consumer-side, is the
plan's open question 6, explicitly to be decided "at G2 with real use." There is no
real consumer yet (that is G3), so the wiring decision waits for one. stemma stands
ready as the lineage machinery; the fork-and-derivation lineage that the G2
done-condition requires lives here in codicil and chartulary, and does not depend on
that wiring. Node-level visit lineage joins at G3 when an app supplies the
engagement semantics.

## Not yet (later phases)

G3 is a first real consumer (which settles the stemma wiring). G4 is scholia (the
RDF projection over the semantic ring). G5 is mere's re-base.

# node-lineage

Owner-scoped navigation-lineage model for [Mere](https://crates.io/crates/mere). Deduplicated entries shared by typed-cursor owners (panes, tabs, graph views, sessions); per-owner branching topology over those entries; derived edge projections.

This crate's role used to be called *graph-memory*. The name was retired because the "memory" layer in Mere is [`eidetic`](https://crates.io/crates/eidetic) (content-addressed engrams, durable). This crate is *lineage* — the per-owner record of where the user has been navigating and how those visits branch.

## Layered lineage

The lineage concept layers at two granularities:

- **url → url** (within-tile, branchable internal lineage): navigating inside a tile extends a visit thread; navigating back and then forward to a different link spawns a branch in the same tile's visit tree. Visible in the lineage facet as both branches.
- **node → node** / **tile → tile** (external lineage on the graph itself): when a within-tile branch is promoted into its own anchor — taking on an identity external to the original node or tile — it surfaces as a directed edge in the canonical graph.

Both granularities use the same Entry / Visit / Owner machinery. Promotion to an anchor is the affirmative gesture that crosses the boundary.

## Concepts

- `Entry` — deduplicated content identity. Many visits can resolve to the same entry.
- `Visit` — a concrete, persisted navigation occurrence. Has a parent and zero or more children; ownership semantics let multiple owners share visits.
- `Owner` — a cursor-bearing actor (pane / tab / graph view / session). Tracks `origin` and `current` visits and the set of visits it owns.
- `EdgeView` — derived graph projection over visit parentage. Edges aren't stored separately; they're computed from the visit tree.
- `AggregatedEntryEdgeView` — entry-level aggregate over edge views (useful for graph rendering at the entry granularity).

## Credit

The Entry / Visit / Owner data model is adapted from [Atlas Engineer](https://atlas.engineer/)'s [`history-tree`](https://github.com/atlas-engineer/history-tree) (Common Lisp, BSD-3-Clause), originally written for the [Nyxt browser](https://nyxt.atlas.engineer/). Mere's implementation is an independent Rust reimplementation against the same abstract data model — no `history-tree` source or binaries are included, no Lisp expression was translated. The conceptual debt is real and acknowledged.

## License

[MPL-2.0](../../../LICENSE-MPL).

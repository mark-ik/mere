# glossary

Graph digest module for the [mere](https://crates.io/crates/mere) browser.

Pure `Graph -> human-facing view`: turns [`kernel::graph::Graph`] (the user's
graph of nodes + edges) into consumer-facing **digests** — a textual outline and
summary metrics — that the gloss Navigator, apparatus, and export surfaces render.
The textual / statistical sibling of `cartography` (spatial projection) and
`linked-data` (RDF / JSON-LD interchange). Host-free, DOM-free, `&Graph`-immutable.

## Planned surface

See the [gloss-outline-lens plan](../../../design_docs/mere_docs/implementation_strategy/2026-06-23_gloss_outline_lens_plan.md).

- `outline_djot(&Graph) -> String` — a hierarchical [djot](https://djot.net) outline
  of the graph, nested by **parsed URL structure** (host -> path from each node's
  address; explicit containment overlaid where present), each node a `[title](url)`
  entry. The outline doubles as an editable knot (the first notetaking feature).
- `graph_metrics(&Graph) -> GraphMetrics` — counts (node / edge / relation-family),
  degree, components, traversal aggregates. Cheap kernel-sourced stats; the expensive
  signals (centrality / community) are consumed from `intel/signals`, not reproduced.

## History

Renamed from `mere-orrery` on 2026-06-23 and relocated from `crates/orrery/` to
`crates/graph/` (beside `linked-data`). Its prior role — the a11y `project_graph`
projection of the graph into an AccessKit / `uxtree` subtree — moved host-side into
meerkat's `orrery_a11y_tree` (unified-document-host slice 4, which sources the orrery
a11y from the laid-out DOM cards) and was retired here.

## Status

Pre-1.0. Scaffolded for the digest surface above; functions land in the gloss-outline
plan's P0.

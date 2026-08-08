# glossary

Graph digest projections for [mere](https://crates.io/crates/mere): pure
`&Graph -> human-facing view`. Turns `kernel::graph::Graph` into a textual
outline and summary metrics. The textual / statistical sibling of `cartography`
(spatial projection) and `linked-data` (RDF / JSON-LD interchange).

Package `mere-glossary`, lib name `glossary`.

## Public surface

| Item | What it is |
| --- | --- |
| `outline_rows(&Graph) -> Vec<OutlineRow>` | The URL-structure outline as a flat, depth-tagged row list. |
| `outline_djot(&Graph) -> String` | The same outline formatted as nested [djot](https://djot.net) bullets, a node as `[label](url)`. |
| `OutlineRow` | `depth: usize`, `label: String`, `url: Option<String>`. `url` is `Some` for a graph node, `None` for a structural path segment. |
| `graph_metrics(&Graph) -> GraphMetrics` | Summary statistics read off the kernel's own queries. |
| `GraphMetrics` | `node_count`, `edge_count`, `relation_count`, `relations_by_family: BTreeMap<EdgeFamily, usize>`, `orphan_count`, `component_count`, `largest_component`. |
| `VERSION` / `STAGE` | Crate version string, lifecycle marker (`"pre-alpha"`). |

## Outline shape

Nesting comes from parsed URL structure rather than containment edges: the host
is depth 0, each path segment adds a level, and a node lands at its full-path
leaf. A segment with no node at it emits a structural row (`url: None`).
Addresses without `scheme://host` are listed flat at the end. A node's label is
its title, or the last path segment when the title is empty or still seeded to
the URL. Output is deterministic: children are `BTreeMap`-ordered and the loose
list is sorted.

`graph_metrics` reads `node_count` / `edge_count` / `relations` /
`orphan_node_keys` / `weakly_connected_components` off the kernel. Centrality
and community signals come from `intel/signals`.

## Dependencies

- `kernel` (`mere-kernel`), the only runtime dependency: `Graph`, `EdgeFamily`.
- dev: `kernel` with the `fixtures` feature, `euclid`.

Host-free and DOM-free. Takes `&Graph` and never mutates it.

## Consumers

- `crates/domain/gloss`: `build_outline_snapshot` wraps `outline_rows` with
  per-node state/selection and carries `GraphMetrics` in its snapshot.
- `crates/mere`: re-exports `glossary` under the `graph` feature.

## Related

- [gloss outline lens plan](../../../design_docs/mere_docs/implementation_strategy/2026-06-23_gloss_outline_lens_plan.md)

Renamed from `mere-orrery` on 2026-06-23 and moved from `crates/orrery/` to
`crates/graph/`. Its prior a11y `project_graph` projection moved host-side and
was retired here.

# mere-orrery

Orrery domain module for the [mere](https://crates.io/crates/mere) browser.

Projects [`kernel::graph::Graph`] (the user's graph of nodes + edges)
into a subtree of [AccessKit](https://accesskit.dev) nodes for `uxtree`.
Each graph node becomes an addressable accessibility / automation node
identified by its UUID, regardless of whether it's currently in the
viewport.

In the printing-press / tier-framework metaphor, the *orrery* is the
moving model of the local universe — your graph as the personal-tier
artifact (orrery is the t1 name in the orrery → moot → moothold →
coalition tier ladder).

## What it produces

- **Orrery root** (`Role::Group`, label = `"Graph"`)
  - **Graph node** (`Role::Link`, label = node title, value = address URL)
    one per `(NodeKey, &Node)` from `Graph::nodes()`.

Stable IDs:

- Root → `orrery`
- Node → `orrery/node/{uuid}` (uses node's `Uuid` for stability across
  sessions and arrangements).

## What it does NOT do (v0)

- **Edges** — skipped. Edges are relationships between nodes, not
  addressable accesskit objects. A future version will model them via
  `aria_owns` / `aria_controls` properties or as a separate edges
  subtree, depending on what screen readers + automation tools find
  most useful.
- **Viewport / camera state** — projection covers the full graph, not
  what's currently visible. Bounds (when nodes are on-screen) are
  filled in by the host after layout.
- **Node provenance / classification metadata** — extra Node fields
  (tags, classifications, lifecycle) project later as additional
  description / value content if a consumer asks for it.

## Status

Pre-1.0. Initial projection covers all graph nodes as `Role::Link`.

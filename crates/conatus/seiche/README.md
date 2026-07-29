# seiche

Kernel-free, rapier-backed force-directed graph layout. A host feeds node keys
and positions; seiche holds the rapier world that settles them under themable
forces.

- **Bodies bound to node keys.** `sync_nodes` reconciles the body set to a
  `(NodeKey, position)` list, `sync_edges` sets the spring topology. `NodeKey`
  is petgraph's `NodeIndex`, so seiche is graph-agnostic — a consumer supplies
  keys from any graph, or mints them directly.
- **Built-in forces** (node exclusion, edge springs, a centering boundary) plus
  an optional Barnes–Hut approximation for big-graph repulsion.
- **Field couplings.** A [`quint`](https://github.com/merely-made/mere) field
  (evaluated in closed form with finite-difference gradients) compiles to a
  `CouplingForce` the same tick integrates, so scriptable fields drive layout
  beside the native forces.

seiche knows nothing about any host graph type: reading the settled layout back
(`positions`) and writing it wherever the host keeps its data is the host's job.

## License

MIT OR Apache-2.0.

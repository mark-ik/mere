# chartulary

The generic content-addressed container graph (aka **chart**). A `Graph<N, E>`
where nodes are content-addressed containers and edges are typed relations, over
one shared, app-agnostic model. The substrate that binds
[muniment](https://github.com/merely-made/mere)'s blobs and
[codicil](https://github.com/merely-made/mere)'s log into a graph.

Fully generic: a node needs one capability, `Identified`, to live in the graph.
Everything else is an opt-in trait that unlocks a feature.

```rust
use chartulary::{Container, Graph, Recognized, Relation, RelationClass};

let mut g: Graph<Container, Relation> = Graph::new();
let a = g.insert(Container::new("a").with_title("Article A").with_tag("research"));
let b = g.insert(Container::new("b").with_title("Article B"));
g.connect(a, b, Relation::new(RelationClass::recognized(Recognized::Cites)));

// query by identity, by neighbor, by relation class, by tag
assert_eq!(g.get(&"a".to_string()).unwrap().title, Some("Article A".into()));
let cites = RelationClass::recognized(Recognized::Cites);
assert_eq!(g.out_edges_of_class(a, &cites).count(), 1);
assert_eq!(g.nodes_tagged("research").count(), 1);
```

Relations come in two rings: a shared **semantic** ring (recognized core plus open
predicate IRIs) that projects to RDF, and **app-private** families that do not.
The provided `Container` and `Relation` payloads implement every capability trait;
an app that needs more implements the traits on its own types.

The name: a chartulary is the register a house kept its charters and muniments in.
`chart = { package = "chartulary" }` in a consumer workspace for the short name.

This is the **G0** cut (the generic core, capability traits, default payloads,
two-ring taxonomy). The edit spine, lineage (stemma), and RDF projection (scholia)
are later phases. Canonical plan: mere's
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`.
See [`design_docs/`](design_docs/).

License: dual MIT OR Apache-2.0, at your option.

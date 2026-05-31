# The cartography-gyre layout seam

**Date**: 2026-05-29
**Status**: Architecture decision + landed substrate primitives. Resolves how the
projection layer (cartography + graph-layout) relates to the physics substrate
(gyre), now that gyre has a working force-directed layout. Companion to the
[between-tiles layout seam](2026-05-26_between_tiles_layout_seam.md) (the same
seam-doc shape, one layer over).
**Related**: [cartography layer brief](../research/2026-05-10_cartography_layer_brief.md),
[composition spine](2026-05-21_mere_composition_spine.md),
[serval-as-host eval](2026-05-29_serval_as_host_evaluation.md),
[adoption roadmap](../implementation_strategy/2026-05-27_adoption_roadmap.md) R1.

---

## The question

R1 says "wire cartography + graph-layout for real layout (replaces the seeded
ring)." With gyre now doing force-directed physics, that raises a sharp
question: gyre *is* a force-directed layout, and so is graph-layout. Are they
redundant, and is gyre a cartography strategy?

## The three layers (verified against the crates)

- **cartography** owns the *contract*: two strategy traits in `strategy.rs`.
  `LayoutStrategy` is analytic and one-shot (`project(request) -> Projection`);
  `StreamingLayoutStrategy` is iterative with a **host-owned, serializable**
  `State: Default + Clone + Serialize + Deserialize`, the strategy staying
  `&self`-stateless. Both emit one `Projection` (`projection.rs`: a
  `Vec<PositionedNode { node, position, radius }>` plus edges, overlays,
  minimap, bounds).
- **graph-layout** owns the *strategies*, as adapters onto cartography's traits:
  analytic (`Radial`, `Penrose`, `Grid`, `Kanban`, `LSystem`, `Phyllotaxis`,
  `SemanticEmbedding`) and streaming (`ForceDirected`, `BarnesHut`,
  `SemanticEdgeWeight`). Its streaming state (`ForceDirectedState`) is a pure,
  `Serialize`-able value. graph-layout depends on cartography.
- **gyre** owns the *physics*: a stateful rapier world (bodies, colliders,
  the `QueryPipeline`, drag-pinning), with the `NodeExclusion` / `EdgeSpring` /
  `Boundary` fields.

## Finding 1: gyre is not a cartography strategy

`StreamingLayoutStrategy::State` must be `Default + Clone + Serialize +
Deserialize`, and the strategy must be `&self`-stateless (the host threads the
state across frames, snapshots it for undo, persists it). A rapier world is none
of those. So gyre **cannot** implement `StreamingLayoutStrategy`, and that is
not a defect to paper over: gyre is a different *kind* of thing. cartography
strategies *compute* a layout (pure function of input + serializable state);
gyre *simulates* one (a stateful actor with collision and interaction).

## Finding 2: the two force-directed implementations are not redundant

There are two force-directed layouts, and they serve different regimes:

- **graph-layout `ForceDirected` / `BarnesHut`** — pure, deterministic,
  serializable state, O(n log n) via Barnes-Hut, no hard collision. The right
  tool for headless / batch / deterministic layout: switcher thumbnails,
  minimaps, snapshot-and-freeze, large graphs, any non-interactive view. It
  produces a `Projection`.
- **gyre** — stateful rapier rigid bodies with *hard collision* (nodes cannot
  overlap), kinematic drag-pinning (grab one, the rest react), continuous
  real-time settling, and the `QueryPipeline` for hit-test/cull. The right tool
  for the *live interactive orrery*.

The canvas picks by regime: interactive orrery uses gyre; everything headless
uses graph-layout. The force *model* (repulsion + spring + centering) is shared
conceptually; the *integration* differs (pure n-body vs rapier-with-collision).

## The bridge: the `Projection` type, with a clean dependency direction

gyre and cartography meet through the `Projection`, not through the strategy
trait:

- **Seed**: a cartography strategy computes a `Projection` (radial, astroid, or a
  converged force-directed pass); its `PositionedNode` positions seed gyre,
  which then runs live physics on top. This is what *replaces the seeded ring* —
  the orrery's starting layout becomes a real strategy's output, refined by
  collision and interaction.
- **Read back**: gyre's live `(node, position)` stream can be rebuilt into a
  `Projection` for downstream consumers, so gyre and the analytic strategies
  present the same shape to the canvas.

**Dependency direction is the constraint.** In the spine, cartography is a
projection layer *above* the kernel-tier substrates (gyre's own doc places it
at the same tier as kernel and petgraph). So gyre must **not** depend on
cartography; a substrate depending on the projection layer above it is a layering
inversion. The bridge therefore lives at the *caller* (the host), not in gyre.
gyre speaks only kernel types.

### Landed (host-agnostic, in gyre)

`gyre::Simulation` gained the two primitives the bridge needs, in kernel types
only, tested:

- `seed_positions(impl IntoIterator<Item = (NodeKey, Point2D)>)` — override body
  positions (e.g. from `Projection.nodes`), resetting velocity and refreshing the
  query index. A test confirms a seeded overlap is then separated by physics, the
  exact refinement a pure strategy cannot do.
- `positions() -> impl Iterator<Item = (NodeKey, Point2D)>` — read the live
  layout back, for the caller to rebuild a `Projection`.

### The host glue (thin, gated on the host flip)

The `Projection` ↔ positions mapping is a few lines the host owns, sketched here
(illustrative, not compile-ready):

```rust
// seed: cartography Projection -> gyre
sim.seed_positions(
    projection.nodes.iter().map(|n| (n.node, Point2D::new(n.position.x, n.position.y))),
);

// read back: gyre -> a Projection the canvas consumes
let projection = Projection {
    nodes: sim.positions().map(|(node, p)| PositionedNode {
        node, position: PortablePoint::new(p.x, p.y), radius: 0.0,
    }).collect(),
    metadata: ProjectionMetadata { strategy_id: Some("force_directed.physics".into()),
                                    settled: sim.is_at_rest(0.01) },
    ..Projection::empty()
};
```

This glue, the strategy selection, and the actual seeded-ring replacement are
host concerns, so they stay thin and wait on the host flip (the
[serval-as-host eval](2026-05-29_serval_as_host_evaluation.md)'s standing rule),
where the orrery becomes a serval custom element fed by this same seed/read pair.

## What this settles

- gyre is the interactive-physics layout; graph-layout is the pure/headless
  layout; cartography is the contract both feed into. No redundancy, no second
  spatial index (the R1b spike already settled that), and no upward dependency.
- R1's "wire cartography + graph-layout (replaces the seeded ring)" is now a
  well-defined, small host task (seed gyre from a strategy `Projection`), with
  the substrate side already in place.

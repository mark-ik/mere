# cartography

`cartography` is the **non-destructive projection layer** for the
[mere](https://crates.io/crates/mere) browser. It sits between graph
truth + intelligence signals on the input side, and canvas swatches on
the output side, and owns the *contracts* — the strategy trait, the
projection / overlay / minimap vocabulary, and the narrow
`IntelligenceSignals` shape that firewalls cartography from
`embed`' internals.

The graph stays canonical. Cartography is *representation*, not
truth — it never mutates the graph; it produces alternative views of
the same territory at the user's current scale and intent.

## The framing

Inputs:

- **Graph truth** — `kernel::graph::Graph` (immutable reference).
- **Intelligence signals** — clusters, affinity, hot regions, bridge
  nodes, importance hints. Consumed through `IntelligenceSignals`, not
  a direct dependency on the producer crate.
- **View intent** — what the user is trying to see right now: scale,
  dimension, focus, filter, **form factor** (orrery root, workbench
  swatch, volvelle radial, astroid hub-collapse, minimap thumbnail).

Outputs:

- **`Projection`** — positioned nodes + edges + overlays, ready for a
  canvas swatch to render.
- **`Overlay` variants** — semantic emphases (`ClusterHalo`,
  `ActivityHeat`, `BridgeEmphasis`, `ImportanceScale`, `EdgeWeight`)
  the canvas applies on top of geometry.
- **`MinimapDescriptor`** — thumbnail-scale projection metadata for
  any swatch.

## What's in the crate (v0)

Contracts only:

- **`LayoutStrategy`** trait — `projection_id()` + `project(&request)
  -> Projection`. Implementations live in sibling crates
  (`graph-layout`, future `document-layout`).
- **`ProjectionRequest<'a>`** — borrow-lifetimed input bundle (graph,
  signals, intent).
- **`ViewIntent`** + **`FormFactor`** + **`ProjectionDimension`** +
  **`NodeFilter`** + **`TargetSize`** — the declarative shape of a
  "render this view" request.
- **`IntelligenceSignals`** + `ClusterSet` / `AffinityScores` /
  `BridgeNodes` / `ImportanceWeights` — additive sparse signal types.
- **`Projection`** + `PositionedNode` / `PositionedEdge` /
  `ProjectionMetadata` — output bundle.
- **`Overlay`** variants — additive overlay vocabulary; canvases that
  don't recognize a variant ignore it.
- **`MinimapDescriptor`** + `MinimapOverlayKind` — minimap
  descriptors for the same-strategy-at-different-form-factor pattern.

## What's NOT in the crate

- **No strategy implementations** — force-directed, radial,
  cluster-collapsed, phyllotaxis, etc. live in `graph-layout`.
- **No embed dependency** — `IntelligenceSignals` is
  a narrow contract type. Producers fill in fields they have; cartography
  consumers read what's there.
- **No rendering** — `graph-canvas` (and the future `document-canvas`
  minimap consumer) renders `Projection`s.
- **No host-platform coupling** — `wasm32-unknown-unknown` clean.
  Geometry types come from `kernel` (which uses `euclid`).

## Position parity contract

`(x, y)` is identical across all `ProjectionDimension` variants. The
`z` axis (when applicable) is metadata-driven via the
[graph-canvas field algebra plan](https://github.com/merely-made/mere/blob/main/design_docs/graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md)'s
`FieldProjection` / `ZSource` — not strategy-driven. Switching
dimensions never invalidates a strategy's output.

## How it relates to other workspace crates

```text
                  kernel::graph::Graph
                            │ &Graph
                            │
embed ────►│ IntelligenceSignals
                            │
                            ▼
                     ┌──────────────┐
                     │ cartography  │  ◄────── ViewIntent
                     │  (contracts) │           from host / platen
                     └──────┬───────┘
                            │
                  picks strategy
                            │
                            ▼
                     ┌──────────────┐
                     │ graph-layout │  (sibling — owns the strategies)
                     │  (strategies)│
                     └──────┬───────┘
                            │ Projection
                            ▼
                     ┌──────────────┐
                     │ graph-canvas │  ◄── renderer; consumes Projection
                     │  (renderer)  │       and draws it
                     └──────────────┘
```

See [`design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md`](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
for the full design + strategy catalogue + scoping decisions.

## Status

Pre-1.0. v0 ships contract types only. Strategy implementations land
in `graph-layout` (sibling crate, follows). Wiring through `platen`
and the host's surface-placement plan lands as their consumers
materialize.

## License

MIT OR Apache-2.0.

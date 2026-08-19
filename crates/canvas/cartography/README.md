# cartography

The projection layer for [mere](https://crates.io/crates/mere). It owns the
contracts between graph truth plus intelligence signals on the input side and
canvas swatches on the output side: the `LayoutStrategy` trait, the
`Projection` / `Overlay` / `MinimapDescriptor` vocabulary, and the
`IntelligenceSignals` shape strategies read instead of depending on `embed`.
It reads `&Graph` and never mutates it.

Package `mere-cartography`, lib name `cartography`. Most contract types derive
`serde::Serialize` / `Deserialize`.

## Modules

| Module | Public items |
| --- | --- |
| `strategy` | `LayoutStrategy`, with `projection_id() -> &'static str` and `project(&ProjectionRequest<'_>) -> Projection`. |
| `request` | `ProjectionRequest<'a>` (borrowed graph + signals, owned intent), `ViewIntent`, `FormFactor`, `ProjectionDimension`, `NodeFilter`, `TargetSize`, `AxisValue`. |
| `projection` | `Projection`, `PositionedNode`, `PositionedEdge`, `ProjectionMetadata`. |
| `overlay` | `Overlay`, whose variants are `ClusterHalo`, `ActivityHeat`, `BridgeEmphasis`, `ImportanceScale`, `EdgeWeight`. |
| `signals` | `IntelligenceSignals`, `ClusterSet`, `Cluster`, `AffinityScores`, `BridgeNodes`, `ImportanceWeights`, `NodeEmbeddings`. |
| `minimap` | `MinimapDescriptor`, `MinimapOverlayKind`. |
| `representation` | Class profiles, primitive and behavior bindings, and ordered representation ladders built from measure/operation/threshold/hysteresis conditions. |
| `scene_out` | `scene_from_projection`, `MERE_GRAPH_ADAPTER`, `HEAT_CHANNEL`, `BRIDGE_CHANNEL`. |
| `spiral_score` | `project_spiral_score`, `project_spiral_score_for_view`, `MereSpiralProjection`. |
| `adapters` | Doc-only stub. The `graph_canvas` adapter family moved to `arrangements` on 2026-05-18. |

`VERSION` and `STAGE` (`"pre-alpha"`) are crate-root constants.

## Request vocabulary

- `FormFactor`: `Canvas` (default), `Orrery`, `Volvelle`, `Astroid`, `Minimap`.
- `ProjectionDimension`: `TwoD` (default), `TwoPointFive`, `Isometric`, `ThreeD`.
  `(x, y)` is identical across variants; `z` is metadata-driven.
- `NodeFilter`: `Neighborhood { center, hops }`, `Tagged(Vec<String>)`,
  `Explicit(Vec<NodeKey>)`.
- `TargetSize`: `Default` (480x360 logical), `Pixels { width, height }`,
  `Logical { width, height }`. `TargetSize::logical_size()` normalizes to a
  `PortableSize`.
- `ViewIntent` also carries `focus`, `axis_values` (per-node `AxisValue` for
  Timeline / Kanban) and `extents` (per-node measured `(w, h)`). Builders:
  `ViewIntent::canvas_pixels`, `ViewIntent::minimap`, `with_focus`,
  `with_filter`, `with_axis_values`.

## Scene output

`scene_from_projection(&Projection, id_of, extent_of) -> sceno::Scene` lowers a
projection into the portable scenograph contract. Nodes become
`sceno::ProjectedItem`s (measured extent becomes `Footprint::Rect`, otherwise
the projection radius becomes `Footprint::Circle`, otherwise `Footprint::Point`),
edges become `RoutedRelation`s carrying `weight`, and `Overlay::ClusterHalo`
becomes a `sceno::Region`. `ImportanceScale` folds into the instance transform;
`ActivityHeat` and `BridgeEmphasis` ride the item channel map as `"heat"` and
`"bridge"`. `id_of` supplies the stable node Uuid string, not the session-local
`NodeKey`.

`project_spiral_score(&Graph, extents, focus, recent_first)` is mere's graph
adapter to the spiral arrangement: it builds a `sceno::Score`, solves it with
`scenomise::solve`, and returns both the score and the `Projection` it realizes
as `MereSpiralProjection`. `project_spiral_score_for_view` also accepts the
declared zoom and prior score. It evaluates each node's class profile against
screen extent, normalized recency, focus, and zoom, using the prior selected
rung for hysteresis. The conditions remain host-side registry data; the score
continues to carry only the selected `Glyph`, `Card`, or `LivePane` slot.

## Dependencies

- `kernel` (`mere-kernel`): `Graph`, `NodeKey`, `EdgeKey`, and the geometry
  types (`PortablePoint`, `PortableRect`, `PortableSize`).
- `sceno`, `scenomise`: the scenograph scene and score contracts and the
  arrangement solver.
- `serde`.
- dev: `uuid`.

No cargo features; `default = []`.

## Where the pieces live

- Strategy implementations: `crates/canvas/arrangements` (penrose, l-system,
  phyllotaxis, grid, radial, axial kanban/timeline, semantic embedding), with
  `LayoutStrategy` adapters in `arrangements::adapters`. Live force physics is
  `seiche`.
- Renderer: `crates/canvas/canvas` (`mere-canvas`) consumes `Projection`.
- Signal producers: `crates/intel/signals` and `crates/intel/embed` fill
  `IntelligenceSignals`.

## Related

- [cartography layer brief](../../../design_docs/mere_docs/research/2026-05-10_cartography_layer_brief.md)
- [cartography / aether layout seam](../../../design_docs/mere_docs/technical_architecture/2026-05-29_cartography_aether_layout_seam.md)

## Status

Pre-1.0.

## License

MIT OR Apache-2.0.

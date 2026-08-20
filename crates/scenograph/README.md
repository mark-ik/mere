# scenograph

A projection engine family for interactive surfaces: heterogeneous sources
(graphs, maps, timelines, instruments, meshes) projected into scenes through one
grammar.

```text
source data + relationships + signals + score
    → select and derive
    → map data to visual channels
    → solve placement
    → produce an interactive scene
```

Sources keep their native truth behind adapters; what is shared is the scene
contract, not a data model. The representation measures content; the projection
places it.

Four crates on the unpublished 0.0.4 development line, sharing the `sceno-` stem with one function morpheme
each.

| Crate | Contents |
| --- | --- |
| [sceno](sceno/) | Core contracts. `SourceRef` / `SourceIx`, `Space` / `SpaceId`, `InstanceId`, `Backdrop`, `Footprint`, `Representation`, `ProjectedItem`, `RoutedRelation`, `Region`, `Scene`, plus the persisted `Score` / `ScoreItem` / `Arrangement` / `Placement` / `SCORE_VERSION` vocabulary and the geometry types `Vec2`, `Size2`, `Rect`, `Transform2`. |
| [scenomise](scenomise/) | Choreography. `solve(&Score) -> Scene` realizes the arrangements; `relax(&mut Scene, &Relaxation)` is a dependency-free repulsion / spring / arrangement-pull pass for surfaces without their own physics sim, including static collision against collidable backdrops. |
| [scenotime](scenotime/) | Runtime. `SceneSnapshot` / `SceneTables` with tombstoned slots, `SceneEpoch` / `Revision` / `BackdropId` / `RelationId` / `RegionId`, `SceneDiff` / `SceneOp` / `apply_diff` returning `ApplyOutcome`, `TransitionSpec` / `TransitionSchedule` with pure host-time sampling, and `pick(world) -> Option<InstanceId>`. |
| [scenograph](scenograph/) | Thin facade re-exporting the three. |

## Vocabulary

- `Arrangement`: `Spiral` (with `SpiralCurve`), `Grid`, `Geographic`, `Hulls`.
- `Placement`: `Ordinal`, `Cell { column, row }`, `Coordinate(Vec2)`.
- `Footprint`: `Point`, `Circle`, `Rect`, `Polygon`, `Path`.
- `Representation`: `Glyph`, `Card`, `Sprite`, `Snapshot`, `LivePane`,
  `Open { kind }`.
- `SceneOp`: add / update / tombstone for sources, spaces, backdrops, items,
  relations and regions, plus `SetItemLayer`, `SetItemOrder`, `SetBounds`,
  `SetGeneration`.
- `TransitionSpec`: duration, easing, and enter / update / exit stage windows;
  `TransitionSchedule` derives stable item windows from one validated diff and
  samples them at host-supplied elapsed time.

A `ProjectedItem` carries `source`, `space`, `transform`, `footprint`,
`representation`, `layer`, `visible`, an optional `hit` shape, and an open
`channels` map of `(name, value)` emphasis pairs.

A `Backdrop` carries source provenance, space, transform, footprint, an open
appearance kind, visibility, and collision participation. Backdrops paint in
table order behind graph content and remain pointer-transparent. Interactive
features stay ordinary items over the environment.

`SceneSnapshot::from_dense` starts an epoch from a one-shot `Scene`;
`SceneTables::pick` resolves a world point to the topmost live instance
(highest layer, then latest explicit order, then highest stable slot), using an
item's `hit` shape when present and its footprint otherwise.

## Dependencies

`sceno` depends on `serde` alone. `scenomise` depends on `sceno`. `scenotime`
depends on `sceno` and `serde`. `scenograph` depends on all three. No product,
engine, or GPU dependencies.

## Status

Action intents are not modelled here. `sceno` owns instance identity,
`scenotime` owns epoch and revision identity plus deterministic transition
evaluation. The consuming protocol owns the intent triple, and the host owns
the clock. Incremental signal evaluation and renderer realization are later
work.

See [the scene contract note](design_docs/2026-07-22_scene_contract_note.md) and
[the epoch/diff note](design_docs/2026-07-22_scenotime_epoch_diff_note.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

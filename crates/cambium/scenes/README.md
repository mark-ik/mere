# Cambium scenes

The scene lane of the Cambium umbrella: a projection engine family for interactive surfaces: heterogeneous sources
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

Three crates on the unpublished 0.0.4 development line, sharing the `sceno-`
stem with one function morpheme each. They landed under `crates/cambium/scenes/`
on 2026-09-03 with the Cambium family, per the platform boundary plan's P2:
the widget lane and the scene lane share lifecycle, input, styling and host
integration, and keep their state models apart. A widget tree is retained
interaction structure; a scene is a projection of content.

| Crate | Contents |
| --- | --- |
| [sceno](sceno/) | Core contracts. `SourceRef` / `SourceIx`, `Space` / `SpaceId`, `InstanceId`, `Backdrop`, `Footprint`, `Representation`, `ProjectedItem`, `RoutedRelation`, `Region`, `Scene`, plus the persisted `Score` / `ScoreItem` / `Arrangement` / `Placement` / `SCORE_VERSION` vocabulary and the geometry types `Vec2`, `Size2`, `Rect`, `Transform2`. |
| [scenomise](scenomise/) | Choreography. `solve(&Score) -> Scene` realizes the arrangements; `relax(&mut Scene, &Relaxation)` is a dependency-free repulsion / spring / arrangement-pull pass for surfaces without their own physics sim, including static collision against collidable backdrops. |
| [scenotime](scenotime/) | Runtime. `SceneSnapshot` / `SceneTables` with tombstoned slots, `SceneEpoch` / `Revision` / `BackdropId` / `RelationId` / `RegionId`, `SceneDiff` / `SceneOp` / `apply_diff` returning `ApplyOutcome`, `TransitionSpec` / `TransitionSchedule` with pure host-time sampling, and `pick(world) -> Option<InstanceId>`. |

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

`sceno` depends on `serde` alone. `scenomise` depends on `sceno`, `serde` and
`serde_json` — the last two for the solver registry it absorbed. `scenotime`
depends on `sceno` and `serde`. No product, engine, or GPU dependencies: the
scene lane does not reach up into Cambium's widgets or down into Genet.

The generic `scenograph` facade crate is gone (platform boundary plan §1).
Every consumer already took the members directly, and its one substantive
file, the solver registry behind `sceno::Arrangement::Custom`, moved into
`scenomise` as `scenomise::registry` — a registry needs the score contract and
the solver contract at once, and `scenomise` owns both. Its types are
re-exported at `scenomise`'s root as the facade exported them; its `solve` is
named `scenomise::solve_via`, because the plain name is already
`scenomise`'s closed-form solve over the named families. The published
`scenograph 0.0.4` name is held for the scene editor product.

## Status

Action intents are not modelled here. `sceno` owns instance identity,
`scenotime` owns epoch and revision identity plus deterministic transition
evaluation. The consuming protocol owns the intent triple, and the host owns
the clock. Incremental signal evaluation and renderer realization are later
work.

See [the scene contract note](../../../design_docs/scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md)
and [the epoch/diff note](../../../design_docs/scenograph_docs/technical_architecture/2026-07-22_scenotime_epoch_diff_note.md).

## License

Licensed under the Mozilla Public License, Version 2.0
([LICENSE](../../../LICENSE)), as the rest of this workspace is. The
dual Apache/MIT notice this file carried until 2026-09-03 was the standalone
`scenograph` repository's and was already stale: mere has no `LICENSE-APACHE`
or `LICENSE-MIT`, and every source file in these three crates carries an
`SPDX-License-Identifier: MPL-2.0` header.

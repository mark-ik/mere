# Scene Contract Note — the first sceno slice

**Date**: 2026-07-22 (reconciled 2026-08-19)
**Status**: **Historical 0.0.3 release baseline.** Landed and consumed. Written as rationale for
the P2 type sketch, before mere wired onto it; mere, isometry, and graphshell
now all consume the contract, so the review question is no longer "is this the
right shape to build" but "which parts did the consumers actually force".
That release carried 45 family tests (sceno 19, scenomise 9, scenotime 17),
verified 2026-07-24. Current `main` is the unpublished 0.0.4 development line.

The four questions this note left open are answered below under
[Rulings](#rulings-2026-07-24-release), executed per the
[Scenograph 0.0.3 release plan](../../../design_docs/mere_docs/implementation_strategy/2026-07-24_scenograph_0_0_3_release_plan.md).

The family moved on 2026-07-23: the standalone `scenograph` repo was absorbed
into mere at `crates/scenograph`, and on 2026-09-03 the three crates moved
again to `crates/cambium/scenes/` under the Cambium umbrella, with the generic
`scenograph` facade dissolved (platform boundary plan, P2). The proof sequence
and findings live
in this repo beside it,
[projection_proofs_plan](../../../design_docs/mere_docs/implementation_strategy/2026-07-21_projection_proofs_plan.md);
the direction record is the
[prior-art brief](../../../design_docs/mere_docs/research/2026-07-21_projection_engine_prior_art_brief.md).

## What landed

- `geometry`: `Vec2` / `Size2` / `Rect` / `Transform2` (similarity: uniform
  scale → rotate → translate). Dependency-free wire shapes; hosts convert at
  the boundary.
- `footprint`: `Point | Circle | Rect | Polygon | Path` in item-local space,
  with `bounds()`. The proof-1 fix: solvers can now see what they must clear.
- `scene`: `SourceRef` (adapter + opaque id) interned into `sources`;
  `Space` chain with `to_world()` (cycle-safe); `ProjectedItem` (source ix,
  space, transform, footprint, representation slot, layer, visibility, hit
  override); `RoutedRelation` (full polyline + open kind + weight);
  `Region` (members + optional contour + confidence).
- ~~`measure`: `Measurement` / `Measurements`~~ — removed before 0.0.3. Hosts
  measure representations and stamp the per-instance `ScoreItem.footprint`.
- `score` (landed 2026-07-22, after this note's first draft): `Score`,
  `Arrangement` (`Spiral` | `Grid` | `Geographic` | `Hulls`), `ScoreItem`, `Placement`,
  `SCORE_VERSION`. The deferral below expected these at the proof-4 era; the
  P3/P4/P5 boundary proof pulled them forward, because a product-free
  *persisted* vocabulary is what makes two adapters comparable.

Beside it in the family: `scenomise::solve` realizes a score footprint-aware
(the spiral grows its spacing to clear the largest measured item),
`scenomise::relax` gives any scene dependency-free repulsion/spring/anchor
relaxation, and `scenotime` wraps a dense scene in epoch-scoped stable slots
with transactional diffs (see the
[scenotime note](2026-07-22_scenotime_epoch_diff_note.md)).

## Decisions, with reasons

1. **String `SourceRef`, interned, not a generic `Scene<Id>`.** Heterogeneous
   scenes (a mere node beside a hocket phrase) need refs that carry their
   source lane, and serialized scenes need concrete types. The allocation
   cost is bounded by interning (`sources` is deduped; items carry a `u32`),
   and scenes rebuild on change, not per frame (scenotime's job). This also
   makes the source-vs-instance separation *structural*: one interned source,
   many items pointing at it.
2. **Identity is an index** (the stack's data-oriented doctrine): dense
   vectors, `u32` id newtypes indexing them, world space at index 0 by
   construction.
3. **Uniform-scale similarity transforms.** Non-uniform scale distorts
   footprint shapes and is a representation concern; placement gets
   translate/rotate/uniform-scale. Revisit only if a real solver needs
   shear/stretch.
4. **Footprints are item-local, centered on the anchor.** The transform
   carries them into a space; `Point` is the degenerate pre-P2 case so
   position-only strategies stay expressible.
5. **Spaces are the frame primitive.** Hull groups, geographic ground
   planes, fretboards, nested maps: all `Space` entries; dragging a group is
   one transform edit. This is the frames ruling from the brief landing as
   data.
6. **Representation is a slot with a recognized LOD core**
   (Glyph/Card/Sprite/Snapshot/LivePane) **plus an open tail** — the same
   recognized-core-plus-open-tail shape as the coupling vocabulary. Scenes
   never carry rendered content.
7. **Overlay migration**: cartography's `EdgeWeight` → `RoutedRelation.weight`;
   `ClusterHalo` → `Region`; `ImportanceScale` folds into assigned
   footprints/transforms; `ActivityHeat`/`BridgeEmphasis` await the channel
   question below.

## Deliberately deferred

- ~~**Score types**~~ **landed 2026-07-22** in `sceno::score`, earlier than
  this note projected. Proof 4 arrived the same day and needed a persisted
  score to compare two adapters against, so the schema question was answered
  by exercise rather than by deliberation: version, arrangement, ordered
  measured items, generation.
- **Action intents**: still absent from `sceno`, but no longer unexercised.
  Graphshell shipped its own reverse path at the protocol layer instead:
  `IntentReference`, `AdvertisedAction`, `IntentInvocation`, `IntentResult`,
  with the payload deliberately opaque at G1. So the question has changed
  shape. It is no longer "when do intent types land in the contract" but
  "does the contract need them at all, or is a hit shape plus a
  product-owned action id the whole engine-side story, with authorization and
  dispatch staying in the protocol and the participant gate?" Answer this
  before adding a second intent vocabulary that has to agree with the first.
- ~~**Scene diffs**~~ **landed 2026-07-22** in `scenotime`: epoch-scoped stable
  slots, tombstones that survive serialization, transactional revision
  transitions, idempotent replay.
- **Per-item emphasis channels** (heat, bridge rings): may be a
  `channels: Vec<(String, f32)>` open map per item, or host-side signal
  reads; decide when a consumer forces it rather than guessing.
- **Volumes / 3D footprints**: per the brief's task-and-display gating; the
  types are shaped so a z-bearing variant extends rather than migrates.

## Open questions, and what two consumers ruled

Verified against the code 2026-07-24, after mere, isometry, and graphshell had
all wired on.

- **`Region`: does it want a members-free, `SpaceId`-only form?** Unforced, so
  hold the single shape. Only `cartography::scene_out` emits regions at all
  (`ClusterHalo` → `Region`); isometry emits none. The pure-frame-group case
  that would have wanted the alternative is already served by `Space`, which
  is what makes the members list non-redundant. Revisit when hull authoring
  produces a group before it has members.
- **Should `RoutedRelation` endpoints attach to regions?** Hold
  instance-to-instance. Both emitters (`cartography::scene_out`,
  `isometry-graphshell`) emit instance pairs, and `scenomise::relax` now reads
  relations as springs between *placed items*, so a region endpoint would owe
  a relaxation rule as well as a stroke. Two lanes to define, still zero
  consumers asking.
- **`Measurements` lookup shape** — the question is moot, and its answer is
  worth more than the question. Nothing consumes `sceno::measure`: not mere,
  not isometry, not turnstone, not graphshell. The extent lane shipped in two
  other places instead. Hosts measure and stamp `ScoreItem.footprint`, and
  mere carries per-node extents through `ViewIntent.extents` →
  `CartographySceneOptions.extents` → the adapter. "The representation
  measures, the projection places" held up as a *principle*; the module that
  encoded it did not, because a score that already carries a measured
  footprint per item leaves `Measurements` with nothing to say. Decide before
  0.0.3 whether `measure` earns its place, folds into the score, or ships as
  documented-but-unused surface. Publishing dead contract surface is the one
  outcome to avoid.

## Where it went

P2's next slice, and everything the original version of this section
anticipated, is done. Mere consumes the contract through
`cartography::scene_out` + `spiral_score`, and the proof-1 scenario re-ran
with every card clear. Isometry (proof 4) deleted `Overmap::layout` and its
force solver, adapting authored pins to a geographic score and unpinned sites
to the portable spiral. P5's coastal fixture exercises the geographic path
from serialized facts. On 2026-07-23 `scenomise::relax` extended the
capability to swatch-scale surfaces without a physics dependency, and
graphshell G2 took `scenotime`'s snapshot/diff pair as its remote replay
contract.

## Rulings (2026-07-24 release)

The four remaining items are decided. Each was answered by reading the
consumers rather than by deliberation, which is the standard the rest of this
note set; three of the four turned out to be already-answered by code nobody
had looked at together.

1. **Action intents stay out of the contract, permanently.** Not "not yet".
   `chirograph` already carries the whole vocabulary and binds an
   invocation to `sceno`'s `InstanceId` plus `scenotime`'s `SceneEpoch` and
   `Revision`. Its `IntentResult::Stale` is expressible *only* because the
   protocol sees epoch and revision together, so a sceno-side intent type
   could at best restate identities the contract already supplies, and would
   then owe agreement with the vocabulary that exists. The seam, recorded in
   `sceno`'s crate docs: sceno owns instance identity, scenotime owns
   epoch/revision, the consuming protocol owns advertise/invoke/result, the
   product's gate owns authority.

2. **`measure` is deleted.** Two findings beyond "no consumer". First,
   `ScoreItem.footprint` already carries the measured extent, and every host
   stamps it there. Second and decisive: `Measurements` was keyed by
   `SourceIx`, which contradicts this note's own decision 1. One source may
   be placed as many instances at different representation rungs, and a
   source-keyed map cannot say so. The module was shaped against the
   commitment it existed to serve. A legibility floor, if one is ever needed,
   returns as an optional field on `ScoreItem` beside the footprint that is
   already there, per instance.

3. **Per-item emphasis channels landed** as `ProjectedItem.channels:
   Vec<(String, f32)>`, an open map with recognized names documented rather
   than enumerated. The forcing argument was not a local one: graphshell
   ships scenes to clients with no access to the source, so emphasis kept as
   a host-side signal read is invisible to a remote viewer. Cartography now
   maps `ActivityHeat` to `"heat"` and `BridgeEmphasis` to `"bridge"`, which
   completes the overlay migration this note started.

   **Relations deliberately did not get channels.** The pressure everyone
   expected from woodshed (one chord pair carrying diatonic, shared-tone,
   voice-leading, and practiced-after at once, without a winning reason)
   turns out to need no contract change at all: mere already ruled multi-edge
   is truth and the collapse is an experience setting, and `canvas::edge_cells`
   implements it. A pair related for four reasons is four `RoutedRelation`s.

4. **Picking belongs to `scenotime`, and hosts may ignore it.**
   `SceneTables::pick` resolves a world point to the topmost live instance
   (layer, then explicit order, then slot), honoring `hit` over `footprint`,
   skipping invisible items and tombstones, and carrying the point back
   through the space chain. Sceno gained the two pieces it should have owned
   all along, `Transform2::inverse` and `Footprint::contains`, so scenotime
   holds only traversal.

   This finally gives `hit` a reader. It was `None` at every construction
   site in the workspace, while the consumer that most needs it, a remote
   client filling `IntentInvocation.target`, has no physics world to ask.
   Mere keeps resolving against its seiche colliders and is unaffected; the
   default is a floor for viewers that have nothing better, and is linear in
   live items by design.

Also settled in passing: `Footprint::Point` contains nothing, so a
zero-extent item is unpickable unless it supplies a `hit` shape. That is the
case `hit` exists for, and is now documented rather than implied.

The third consumer is **woodshed**. Its data half is now founded through
`StageGraphSnapshot`; headed rendering remains. It brings two inputs the first
two consumers could not: a non-graph frame (a fretboard is a `Space`
mapping (string, fret) to screen, with notes as point footprints and
fingerings as paths), and a source whose relations are dense,
multi-family, and deterministic on day one, with no authoring step.

## Current delta (2026-08-19)

The protocol has continued to develop. Score v2 added authored holds. Score v3
renames the regular-cell `Board` arrangement to `Grid`. `Scene` and
`SceneTables` carry both honored and unmet pins, Chirograph carries Selection
separately from scene truth, and its retained transport artifact is a
`ProjectionCaptureV1`. Graphshell's `FrozenScene` is a deliberately static,
noninteractive realization which can be replaced by an interactive realization
from the same scene; the name does not describe the protocol or transport.
